//! `journal` — Record one beat in this session's chronology, and move its focus.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct JournalArgs {
    /// One high-level beat: what you set out to do, what you found, what you
    /// decided, what went wrong. Prose — paragraphs are fine.
    pub entry: String,
    /// What you are working on NOW, in one line. Optional, and it **replaces**
    /// the session's current focus rather than adding to it.
    #[serde(default)]
    pub focus: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. A session is
    /// bound to the bot that booted it; there is no way to write into another
    /// one.
    pub sid: String,
}

/// Record one beat in this session's chronology, and optionally move what
/// it says it is working on.
#[tool_router(router = journal_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Record ONE beat in your session's chronology — a literal journal, not a \
                       log. High-level: what you set out to do, what you found, what you \
                       decided, what went wrong. Not every tool call, not every file: a reader \
                       months from now wants the story, and a firehose buries it. `focus` \
                       rewrites what your session says it is working on RIGHT NOW, in place — \
                       the chronology is history, the focus is the present, and they answer \
                       different questions. The first journal entry (or the first write of any \
                       kind) is what brings your session's record into being, so a boot that does \
                       nothing leaves nothing behind. PASS `sid` — the session id the boot door \
                       gave you — ON EVERY CALL; it is the only address, and it is what tells \
                       jojobot which bot is writing. A `sid` whose session is closed comes back \
                       status: blocked: a closed session takes no more entries, whichever end it \
                       reached. The two ends part company on what comes NEXT — a run that stopped \
                       without being wrapped up is offered back at your next boot, and resuming \
                       it continues this same record, while a wrapped one is the last word — its \
                       story is told and nothing appends to it, so carrying on means a fresh \
                       session."
    )]
    pub(crate) async fn journal(
        &self,
        Parameters(args): Parameters<JournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let focus = args.focus.as_deref();
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        // Resolved inside the gate: a racing write may have materialized this
        // session's card since, and beginning a second one is the fork the lock
        // exists to prevent.
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **Screened here so the refusal is an ANSWER, and before anything is
        // written.** A focus the record cannot carry is a caller mistake, and
        // it reaches the store by two paths: the call that OPENS a session, and
        // the one that moves the focus of a session already open. The first
        // came back as a protocol error, which is a failure where the caller
        // needs a next move (rule 68); the second came back as a partial answer
        // saying jojobot's storage failed, which is not true of a bad argument
        // (rule 130). One screen closes both, and it runs before the entry is
        // appended, so nothing is written and the whole call can simply be sent
        // again.
        if let Some(theirs) = focus.map(str::trim).filter(|f| !f.is_empty()) {
            if let Err(e) = jojobot_domain::session::validate_focus(theirs) {
                return session_declined(e);
            }
        }
        let session = self
            .session_for(&_serialized, &caller, focus, Some(&args.entry))
            .await?;
        let entry = match self
            .sessions
            .append(
                &session,
                NewEntry::manual(args.entry, jiff::Timestamp::now()),
            )
            .await
        {
            Ok(entry) => entry,
            // **An append can fail with its write already on the page** — a
            // reread that failed after the entry landed, or a rollback that
            // failed too. A flat error reads as "nothing was written", and the
            // natural next move is the retry that appends a second entry
            // beside the first. That is the same hazard the focus case below
            // fixes, one call earlier.
            //
            // Uncertain rather than partial: unlike the focus, nothing here
            // knows whether the entry landed. So the answer says that plainly
            // and sends the caller to look, which is the conservative reading
            // and the only honest one.
            // **Only a STORE failure can have written before it failed.**
            // Every other variant is a clean refusal decided before anything
            // was touched — a closed run, an unknown session, a malformed
            // entry — and dressing those as uncertain would send a caller to
            // go and look for a write that provably never happened.
            Err(e) if !matches!(e, SessionError::Store(_) | SessionError::Stranded { .. }) => {
                return session_declined(e);
            }
            Err(e) => {
                tracing::error!(error = %e, "store failure left an append uncertain");
                return json_result(&serde_json::json!({
                    "status": "uncertain",
                    "wrote": "unknown",
                    "session": session.as_str(),
                    "why": "jojobot's own storage failed",
                    "how_to_proceed": "The entry may or may not have been recorded — this \
                                       failure cannot tell you which. Do NOT send it again \
                                       blind: read the session back first, and re-send only if \
                                       your entry is not the newest one in its chronology.",
                }));
            }
        };
        // The focus moves only once the beat is recorded: a session whose focus
        // says it is doing something its chronology never mentions is a record
        // that disagrees with itself.
        let moved = match focus {
            None => None,
            Some(focus) => match self.sessions.set_focus(&session, focus).await {
                Ok(session) => Some(session),
                // **The entry is already recorded, so this is not a failed
                // call — it is a call that half succeeded.**
                //
                // A flat error here reads as "nothing was written", which is
                // the safe assumption for every other failure on this surface
                // and the dangerous one here: the caller's natural next move is
                // to repeat the whole call, and repeating it appends the entry
                // a second time beside the one already recorded. That is not
                // hypothetical — it is what a session did after exactly this
                // failure, and it only avoided the duplicate because the error
                // happened to carry enough of the record to see the entry in.
                //
                // So the answer says what LANDED first and what did not, and
                // the way forward names the smaller call rather than the one
                // just made.
                // **The entry is already recorded, so this is not a failed
                // call — it is a call that half succeeded.**
                //
                // A flat error here reads as "nothing was written", which is
                // the safe assumption for every other failure on this surface
                // and the dangerous one here: the caller's natural next move is
                // to repeat the whole call, and repeating it appends the entry
                // a second time beside the one already recorded. That is not
                // hypothetical — it is what a session met after exactly this
                // failure, and the duplicate was avoided only because the error
                // happened to carry enough of the record to see the entry in.
                //
                // So the answer says what LANDED first, and the way forward
                // names the smaller call rather than the one just made.
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "store failure left the focus unmoved after the entry landed"
                    );
                    return json_result(&serde_json::json!({
                        "status": "partial",
                        "wrote": true,
                        "recorded": "entry",
                        "not_recorded": "focus",
                        "session": session.as_str(),
                        "entry": entry_json(&entry),
                        "why": "jojobot's own storage failed",
                        "how_to_proceed": "The entry IS recorded — do not send this call again, \
                                           or the entry lands twice. Only the focus did not move. \
                                           Set it on its own with a journal call carrying a focus \
                                           and no entry, or leave it: the chronology is what \
                                           outlives the session, and the focus is overwritten by \
                                           the next beat anyway.",
                    }));
                }
            },
        };
        json_result(&serde_json::json!({
            "session": session.as_str(),
            "entry": entry_json(&entry),
            "focus": moved.map(|s| s.focus),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// **A failure that cannot say whether the write landed says THAT.**
    ///
    /// An append can fail with its entry already on the page — a reread that
    /// failed after it landed, or a rollback that failed too. A flat error
    /// reads as nothing-happened, and the retry it invites appends a second
    /// entry. Nothing here knows which state it is in, so the answer says so
    /// and sends the caller to look rather than guessing for them.
    #[tokio::test]
    async fn a_journal_whose_entry_fails_says_it_cannot_tell() {
        let store = Arc::new(RefusingAppend(InMemorySessions::new()));
        let jojobot = Jojobot::new(
            Arc::new(crate::memory::testing::InMemoryMemory::new()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            crate::harness::seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "set out to read the box".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("an uncertain outcome is an answer, not a protocol failure"),
        );

        assert_eq!(body["status"], "uncertain", "{body}");
        assert_eq!(
            body["wrote"], "unknown",
            "neither true nor false: nothing here knows: {body}"
        );
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("Do NOT send it again blind"),
            "the blind retry is the danger: {how}"
        );
        assert!(
            how.contains("read the session back"),
            "…and the way out is to look, not to guess: {how}"
        );

        assert!(
            !body.to_string().contains("the entry row on the page"),
            "the adapter's own words crossed: {body}"
        );
    }

    /// **A call that half succeeded says so, and says which half.**
    ///
    /// One journal call carries an entry and a focus. In production the entry
    /// committed, the focus rolled back, and the answer was a flat error —
    /// indistinguishable from nothing-happened, which is the safe reading
    /// everywhere else on this surface and the dangerous one here. The natural
    /// retry appends the entry a second time beside the one already recorded.
    ///
    /// Both halves are asserted, because the obvious one passes on its own:
    /// the caller learns the entry landed AND is told not to repeat the call.
    #[tokio::test]
    async fn a_journal_whose_focus_fails_says_the_entry_landed() {
        let store = Arc::new(RefusingFocus(InMemorySessions::new()));
        let jojobot = Jojobot::new(
            Arc::new(crate::memory::testing::InMemoryMemory::new()),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            crate::harness::seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "set out to read the box".into(),
                    focus: Some("reading the box".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("a half-success is an answer, not a protocol failure"),
        );

        assert_eq!(body["status"], "partial", "{body}");
        assert_eq!(
            body["wrote"], true,
            "the default reading must be that something LANDED: {body}"
        );
        assert_eq!(body["recorded"], "entry");
        assert_eq!(body["not_recorded"], "focus");
        assert!(
            body["entry"]["text"] == "set out to read the box",
            "the entry that landed comes back, so the caller can see it: {body}"
        );

        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("do not send this call again"),
            "the retry is the danger, so the advice has to forbid it: {how}"
        );

        assert!(
            !body.to_string().contains("the focus cell on the page"),
            "the adapter's own words crossed: {body}"
        );

        // The entry really is on the record, which is what makes a repeat a
        // duplicate rather than a retry.
        let session = store
            .0
            .read_session(&SessionId(
                body["session"]
                    .as_str()
                    .expect("the answer names it")
                    .into(),
            ))
            .await
            .expect("the session reads");
        assert_eq!(session.entries.len(), 1, "{session:?}");
    }

    /// Writing to a closed run must say something different depending on
    /// which end it reached, because the way forward is different: an
    /// abandoned run reopens, and telling its owner to start a new one
    /// instead sends them to fork the work they were trying to continue.
    #[tokio::test]
    async fn writing_to_a_closed_run_says_which_end_it_reached() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let stopped = abandoned_run(&store, "gamma", "reading the hand-off", 30).await;
        let told = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .close(&told.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let advice = |session: &SessionId| {
            let jojobot = &jojobot;
            let sid = as_run(jojobot, "gamma", session);
            async move {
                let body = blocked(
                    &jojobot
                        .journal(Parameters(JournalArgs {
                            entry: "one more thing".into(),
                            focus: None,
                            sid,
                        }))
                        .await
                        .expect("a closed session is an answer, not a protocol failure"),
                );
                body["how_to_proceed"].as_str().expect("advice").to_string()
            }
        };

        let on_stopped = advice(&stopped.id).await;
        assert!(
            on_stopped.contains("resume") && on_stopped.contains("start_here"),
            "a run that stopped is picked back up, not replaced: {on_stopped}"
        );
        assert!(
            !on_stopped.contains("belongs to a new session"),
            "…and it must not send the caller off to fork the work: {on_stopped}"
        );

        let on_told = advice(&told.id).await;
        assert!(
            on_told.contains("story has been told"),
            "a told story names the reason this end is the last word: {on_told}"
        );
        assert!(
            !on_told.contains("Journal"),
            "…and never a shared Journal, which no longer exists: {on_told}"
        );
        assert!(
            on_told.contains("new session"),
            "…and there the next run really is the way forward: {on_told}"
        );
    }

    /// **A boot that does nothing leaves nothing behind.** The card materializes
    /// on the first write and never before, which is what keeps "creation is an
    /// intentional act" true for the one verb whose job is to start something.
    #[tokio::test]
    async fn booting_writes_no_session_card_until_the_first_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["available"], true);
        assert_eq!(booted["session"]["resumed"], false, "nothing was in flight");
        assert!(
            booted["session"]["session"].is_null(),
            "…and no card was written"
        );
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a boot that never works must leave no card at all"
        );

        // The first beat is what brings it into being.
        let sid = sid_of(&booted).expect("a handle");
        let journalled = journal_entry(&jojobot, &sid, "read the hand-off").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "the first entry materializes the card");
        assert_eq!(journalled["session"], live[0].id.as_str());
        assert_eq!(live[0].entries.len(), 1);
        assert_eq!(live[0].entries[0].text, "read the hand-off");
        assert_eq!(
            live[0].focus, "read the hand-off",
            "with nothing else to go on, what it first recorded is what it is doing"
        );
    }

    /// **THE BLOCKER: a first write is prose, and prose is not a focus.** The
    /// card materializes with a focus derived from the entry, so the focus's
    /// rules — one line, 200 characters, no backtick — were being applied to
    /// text nobody offered as a focus. A multi-line entry, a long story, or a
    /// one-liner naming code in backticks failed with `invalid entry` naming a
    /// `focus` parameter the caller never passed; the entry was dropped and no
    /// card appeared at all.
    ///
    /// The entry reaches the chronology **whole**. The focus is a glance, so it
    /// is derived: flattened, cut, and stripped of what a one-line display field
    /// cannot carry.
    #[tokio::test]
    async fn a_first_entry_is_prose_and_still_lands_whole() {
        let backticked = "started on `working_session`, which was the wrong shape";
        let long = "x".repeat(400);
        let cut = format!("{}…", "x".repeat(199));
        // The derived focus in full, not just its shape — a flatten that joined
        // with nothing would glue the words either side of a paragraph break
        // into one, and every rule-shaped assertion (no newline, no backtick,
        // within the cap) still holds of the glued line.
        let cases: [(&str, &str, &str); 3] = [
            (
                "multi-line",
                "read the hand-off\n\nthen scoped the slice",
                "read the hand-off then scoped the slice",
            ),
            (
                "backticked",
                backticked,
                "started on working_session, which was the wrong shape",
            ),
            ("over-long", &long, &cut),
        ];
        for (shape, entry, focus) in cases {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = with_sessions(store.clone());
            make_bot(&jojobot, "gamma").await;
            let sid = booted(&jojobot, "gamma").await;

            let body = json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: entry.into(),
                        focus: None,
                        sid,
                    }))
                    .await
                    .unwrap_or_else(|e| panic!("a {shape} first entry must not error: {e:?}")),
            );
            assert_ne!(body["status"], "blocked", "{shape}: {body}");

            let live = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            assert_eq!(live.len(), 1, "{shape}: the card must materialize");
            assert_eq!(
                live[0].entries[0].text,
                jojobot_domain::session::normalize_entry(entry),
                "{shape}: the entry reaches the chronology whole"
            );
            assert_eq!(
                live[0].focus, focus,
                "{shape}: the derived focus is display text, word for word"
            );
            assert!(
                live[0].focus.chars().count() <= 200,
                "{shape}: …and it is cut to fit: {:?}",
                live[0].focus
            );
        }
    }

    /// A focus the caller passed IS validated as a focus — the rules were never
    /// wrong, only misapplied. Its refusal names the parameter they actually
    /// sent, and it is a blocked ANSWER rather than a protocol error: a caller
    /// mistake does not leave through the error channel (rule 68).
    ///
    /// **The whole call is refused, so nothing is written.** The screen runs
    /// before the entry is appended, which is what lets the caller fix the
    /// argument and send the same call again — the alternative left the entry
    /// recorded and the focus unmoved, and reported that as jojobot's own
    /// storage failing.
    #[tokio::test]
    async fn an_explicit_focus_is_still_held_to_the_focus_rules() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        // **On a session that is already open**, which is the case that bites:
        // the entry appends before the focus moves, so a screen placed after
        // the append refuses the call with the entry already on the record.
        jojobot
            .journal(Parameters(JournalArgs {
                entry: "read the hand-off".into(),
                focus: None,
                sid: sid.clone(),
            }))
            .await
            .expect("the first beat lands");

        let body = blocked(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "scoped the slice".into(),
                    focus: Some("two\nlines".into()),
                    sid,
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        let said = jojobot_domain::session::validate_focus("two\nlines")
            .expect_err("a focus over two lines is refused")
            .to_string();
        assert!(
            advice.contains(&said),
            "the refusal names the fault.\n  wanted: {said}\n  got: {advice}"
        );

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let texts: Vec<&str> = live[0].entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            ["read the hand-off"],
            "a refused call writes nothing at all — not even the entry it carried"
        );
    }

    /// **The whole arc through the surface:** boot, journal with a focus, amend
    /// the beat, wrap. The focus is current truth and the chronology is history,
    /// and the wrap writes the story to both the session and the Journal.
    #[tokio::test]
    async fn the_session_arc_through_the_handler() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: Some("building the session context".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(first["focus"], "building the session context");
        assert!(
            first["entry"]["beat"].is_null(),
            "a session's own entry is not a beat"
        );

        let amended = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off and scoped the slice properly".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("amend ok"),
        );
        assert_eq!(amended["entry"]["id"], first["entry"]["id"], "in place");

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the session context; the sweep is lazy until M8".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["session"]["state"], "wrapped");
        // **Wrapping publishes NOWHERE.** It told the story into a shared
        // Journal document, and the operator's ruling deletes that: the journal
        // goes dark until events land, and a wrap is the session's own record
        // closing.
        assert!(
            wrapped.get("journal").is_none(),
            "a wrap publishes nowhere, so it reports no publication: {wrapped}"
        );
        assert!(
            !jojobot
                .memory
                .scan()
                .await
                .expect("scan ok")
                .iter()
                .any(|doc| doc.title.trim() == "Journal"),
            "…and no shared Journal document was brought into being"
        );

        let read = store
            .read_session(&SessionId(
                first["session"].as_str().expect("a session id").to_string(),
            ))
            .await
            .expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        // The closing entry carries the unpublished focus folded into the
        // story — one entry for one moment, which is the operator's ruling.
        assert_eq!(
            texts,
            vec![
                "read the hand-off and scoped the slice properly",
                "building the session context\n\nbuilt the session context; the sweep is lazy until M8",
            ],
            "two entries: the amended one, and the story with the flushed focus"
        );
    }

    /// A session verb on a connection that never booted is blocked with the way
    /// forward — jojobot will not guess which identity made the call.
    #[tokio::test]
    async fn a_session_verb_without_a_boot_is_blocked_with_the_way_forward() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "who am i".into(),
                    focus: None,
                    sid: String::new(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        // The remedy must be one that works on the caller's next call: it
        // must name `bot`, the address that survives a fresh connection, or
        // the very next call lands back here.
        assert!(
            how.contains("`sid`"),
            "the way out names the address: {how}"
        );
    }
}
