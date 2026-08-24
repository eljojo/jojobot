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
    pub(crate) entry: String,
    /// What you are working on NOW, in one line. Optional, and it **replaces**
    /// the session's current focus rather than adding to it.
    #[serde(default)]
    pub(crate) focus: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. A session is
    /// bound to the bot that booted it; there is no way to write into another
    /// one.
    pub(crate) sid: String,
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
                       session. IT ANSWERS WITH A RECEIPT, NOT YOUR BEAT: the id the entry was \
                       given, when it was stamped, the run it landed in, the byte count of what \
                       was stored and the opening line. You wrote the entry; start_here returns \
                       the whole chronology when you resume."
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
                NewEntry::manual(args.entry, self.clock().now(), caller.day),
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
            Err(e) if !matches!(e, SessionError::Store(_)) => {
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
                        "entry": entry_receipt_json(&entry),
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
        let mut body = serde_json::json!({
            "session": session.as_str(),
            "entry": entry_receipt_json(&entry),
            "focus": moved.map(|s| s.focus),
        });
        if self.receipts.postcondition {
            crate::answer::note_postcondition(
                &mut body,
                self.what_a_beat_left_standing(&session).await,
            );
        }
        json_result(&body)
    }
}

impl Jojobot {
    /// **What a caller's own beat did to its chronology.**
    ///
    /// A chronology is append-only and only its newest entry can be amended,
    /// so a beat adds and never thins out. Saying so is the same work the
    /// memory writes do: a session that suspects a write replaces what it
    /// already recorded writes less than it knows, and a chronology is the one
    /// record whose whole worth is that nothing was left out of it.
    ///
    /// The length is read for this line and left out when the store cannot
    /// answer, since a number nothing backs is worse than the sentence alone.
    async fn what_a_beat_left_standing(&self, session: &SessionId) -> String {
        let length =
            self.sessions
                .read_session(session)
                .await
                .ok()
                .map_or_else(String::new, |run| {
                    format!(
                        " Its chronology is now {} long.",
                        crate::answer::counted(run.entries.len(), "entry", "entries")
                    )
                });
        format!(
            "Recorded as a new entry at the end of this session's chronology.{length} No earlier \
             entry was rewritten; only the newest one can be amended."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// **A beat says what now stands and what it left alone.**
    ///
    /// A chronology is append-only and only its newest entry can be amended,
    /// so a beat changes nothing that came before it. That is the same fear
    /// the memory writes answer — an agent that suspects a write overwrites
    /// what it already sent writes less than it knows — and a session's
    /// chronology is the one record whose whole value is that nothing thins it
    /// out.
    ///
    /// **The count is what makes the line worth reading**, so it is what this
    /// pins: a constant sentence cannot say the chronology got longer.
    #[tokio::test]
    async fn a_beat_says_the_chronology_only_grew() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);

        let first = journal_entry(&jojobot, &sid, "set out to find why the boot is slow").await;
        let opening = first["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {first}"))
            .to_string();
        assert!(
            opening.contains('1'),
            "the line has to say how long the chronology now is: {first}",
        );

        let second =
            journal_entry(&jojobot, &sid, "found it: the index rebuilds on every read").await;
        assert!(
            second["postcondition"]
                .as_str()
                .expect("a write states what now stands")
                .contains('2'),
            "a second beat and the line still says one, so it is not counting: {second}",
        );
    }

    /// **The count reads as English at one and at none.**
    ///
    /// A real model read *"1 entries long"* off this line in a paid run. The
    /// line exists to be read by something that reasons about what it says, so
    /// prose that announces itself as generated spends the trust the line was
    /// added to build.
    #[tokio::test]
    async fn the_chronology_count_reads_as_english_at_one() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let first = journal_entry(&jojobot, &sid, "set out to find why the boot is slow").await;
        let line = first["postcondition"].as_str().expect("a line").to_string();
        assert!(
            line.contains("1 entry") && !line.contains("1 entries"),
            "the singular case reads as generated text: {line}",
        );

        let second = journal_entry(&jojobot, &sid, "found it").await;
        let plural = second["postcondition"].as_str().expect("a line");
        assert!(
            plural.contains("2 entries"),
            "…and the plural still has to be plural: {plural}",
        );
    }

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
            Arc::new(crate::memory::testing::InMemoryMemory::booted()),
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
    /// **A beat is answered with a receipt, not with the beat.**
    ///
    /// The caller wrote the entry in the call it is reading the answer to, so
    /// the text is the one thing in that answer it already holds — and a
    /// journal entry is prose at the length prose reaches, which makes this the
    /// most expensive echo a session pays for, once per beat, all run long.
    ///
    /// **Both halves.** What must go is the text; what must stay is everything
    /// the caller could not know — the id the entry was given, the moment
    /// jojobot stamped it, the session it landed in — plus the byte count and
    /// the opening, which is how a caller tells two beats apart without the
    /// bodies. A test for the absence alone passes on an empty answer.
    #[tokio::test]
    async fn a_beat_is_receipted_without_shipping_the_entry_back() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let entry = "read the hand-off, scoped the slice, and found the guard already covered it";

        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: entry.into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );

        assert_eq!(
            body["entry"]["text"],
            serde_json::Value::Null,
            "the author of the entry is the one reader it teaches nothing: {body}"
        );
        assert_eq!(body["entry"]["text_elided"], true, "{body}");
        assert_eq!(
            body["entry"]["text_bytes"],
            entry.len(),
            "the count is of what was stored, so a caller learns a trim happened: {body}"
        );
        assert!(
            body["entry"]["text_head"]
                .as_str()
                .is_some_and(|head| entry.starts_with(&head[..20])),
            "…and enough of the opening to tell two beats apart: {body}"
        );
        assert!(
            body["entry"]["id"].is_string(),
            "the id the entry was given is what a caller cannot know: {body}"
        );
        assert!(
            body["entry"]["at"].is_string(),
            "…and when it landed: {body}"
        );
        assert!(
            body["session"].is_string(),
            "the run the beat landed in is named: {body}"
        );

        // The whole thing is still reachable, and the answer says by which call.
        let how = body["entry"]["how_to_read"].as_str().expect("a way to it");
        assert!(
            how.contains("start_here"),
            "eliding is never silent — it names the call that returns it: {how}"
        );
    }

    #[tokio::test]
    async fn a_journal_whose_focus_fails_says_the_entry_landed() {
        let store = Arc::new(RefusingFocus(InMemorySessions::new()));
        let jojobot = Jojobot::new(
            Arc::new(crate::memory::testing::InMemoryMemory::booted()),
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
        assert_eq!(
            body["entry"]["text"],
            serde_json::Value::Null,
            "the entry that landed is receipted rather than read back: {body}"
        );
        assert!(
            body["entry"]["id"].is_string() && body["entry"]["text_elided"] == true,
            "…and the receipt is what says it landed, which is the whole point of this \
             half-success: {body}"
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
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
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
            "…and never a shared Journal, which is not a thing here: {on_told}"
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
