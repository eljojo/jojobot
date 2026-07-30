//! `wrap_session` — End the session and tell its story.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `wrap_session`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WrapSessionArgs {
    /// The story of this session, for somebody with none of your context: what
    /// it was for, what happened, what is left. It becomes the final entry in
    /// this session's own chronology, and goes nowhere else.
    pub story: String,
    /// Your session id — the session to wrap.
    pub sid: String,
}

/// End the session, telling its story into its own chronology.
#[tool_router(router = wrap_session_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "End your session and tell its story. Two things happen together: the \
                       story is recorded in your chronology as its final entry, and the session \
                       moves to `wrapped` — terminal both ways, so \
                       nothing appends to it or reopens it afterwards, and a later \
                       journal/amend_journal/wrap_session on that id comes back status: blocked. \
                       A wrap you have to retry finishes what the first attempt started rather \
                       than repeating it, so the story is told once in each place — which means \
                       it is your chronology's newest entry only when nothing was written \
                       between the attempts. Write the story for somebody with \
                       none of your context: what this run was for, what actually happened, what \
                       is left. A session that stops without wrapping is not lost — the next \
                       boot of the same identity sweeps it to `abandoned` after a day, its \
                       chronology stays readable, and the run itself can be picked up again — but \
                       its story was never told, and that is the difference between the two \
                       endings. Pass your `sid` on every call. When the work continues but this \
                       run has gotten long, wrapping is also how you ROTATE: wrap the story, then \
                       boot again for a fresh sid."
    )]
    pub(crate) async fn wrap_session(
        &self,
        Parameters(args): Parameters<WrapSessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // A run that never wrote anything can still tell its story: the card is
        // created here, exactly as a first journal entry would create it, so
        // "I booted, did the work elsewhere, and I am done" is not a dead end.
        let session = self
            .session_for(&_serialized, &caller, None, Some(&args.story))
            .await?;

        // **A retry must not tell the story twice.** The order below is the
        // right one — the story reaches the session's own record before
        // anything else, so a failure anywhere after it leaves the story safe
        // and the session open — but the step most likely to fail transiently is
        // the LAST one, the close. After that failure the story is already in
        // both places and the only move left is to wrap again, which without
        // this would append it to both a second time. So each write is guarded
        // by whether its own half is already done, and a retry finishes what the
        // first attempt started rather than repeating it.
        let story = jojobot_domain::session::normalize_entry(&args.story);
        // **The current unpublished beat is flushed INTO the story, as ONE
        // entry.** A session's focus is truth about the run, rewritten in place,
        // and becomes chronology only once something has happened (rule 81).
        // Wrapping is the last thing that happens, so the focus that never
        // became a beat becomes one here.
        //
        // One entry, not two beside each other: the focus and the story are the
        // same moment, and a chronology ending on two records of it leaves a
        // reader unable to tell which is the account. Chronological inside —
        // the focus is what the run was doing, the story is what became of it.
        //
        // Read before the guard below, so the retry looks for the composed text
        // rather than the story alone. A retry that searched for half of what it
        // wrote would tell it twice.
        let focus = match self.sessions.read_session(&session).await {
            Ok(read) => jojobot_domain::session::normalize_entry(&read.focus),
            // Unreadable is not "no focus", but the append below fails in that
            // verb's own words; guessing an empty one here only risks losing a
            // line, never duplicating the story.
            Err(_) => String::new(),
        };
        // **A focus DERIVED from this same story is not an unpublished beat.** A
        // wrap that is the session's first write creates the card with a focus
        // made out of the story itself (`display_line`), so folding it back in
        // would tell the story twice inside one entry — and it would not compare
        // equal, because the derived form is flattened to one display line.
        // Compared through the same derivation, which is the only form the two
        // can meet in.
        let story = if focus.is_empty() || display_line(&story) == focus {
            story
        } else {
            format!("{focus}\n\n{story}")
        };
        // **Anywhere in the chronology, not just at its tail.** The retry is the
        // move left after a failed close, and the natural thing to write between
        // the two is a beat saying the wrap failed — which made the story no
        // longer the newest entry, and the retry told it again.
        let already = match self.sessions.read_session(&session).await {
            Ok(read) => read
                .entries
                .iter()
                .rev()
                .find(|e| !e.is_auto() && e.text == story)
                .cloned(),
            // Not fatal: an unreadable session fails the append below, in that
            // verb's own words rather than this guard's.
            Err(_) => None,
        };
        let entry = match already {
            Some(told) => told,
            None => match self
                .sessions
                .append(&session, NewEntry::manual(&story, jiff::Timestamp::now()))
                .await
            {
                Ok(entry) => entry,
                Err(e) => return session_declined(e),
            },
        };

        let wrapped = match self.sessions.close(&session, SessionState::Wrapped).await {
            Ok(wrapped) => wrapped,
            Err(e) => return session_declined(e),
        };
        // **The handle outlives the run it named, and stops addressing it.** The
        // registry keeps the mapping — re-issuing a wrapped run's handle would
        // send somebody's next call into an archive — so nothing is removed
        // here. What changes is what the store will accept: `wrapped` is the
        // last word, and every later write on this handle comes back blocked in
        // those words.
        //
        // A bot that wraps and keeps working boots again for a fresh handle,
        // which is the rotation the description names.
        json_result(&serde_json::json!({
            "session": session_json(&wrapped),
            "entry": entry_json(&entry),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// **Wrapping flushes the current unpublished beat INTO the story, as one
    /// entry.** His ruling, and the "one entry" half is the part a reasonable
    /// implementation gets wrong: *"it should be both but it should be one
    /// entry."*
    ///
    /// A session's `focus` is current truth, rewritten in place, and it becomes
    /// chronology only once something has happened (rule 81). Wrapping IS
    /// something happening — it is the last thing that happens — so the focus
    /// that never became a beat becomes one. Writing it as a SECOND entry beside
    /// the story would leave the chronology ending on two records of one moment,
    /// and a reader unable to tell which was the account.
    ///
    /// Ordering is chronological: the focus was what the run was doing, the
    /// story is the account of it, so the focus comes first inside the entry.
    #[tokio::test]
    async fn wrapping_flushes_the_unpublished_focus_into_the_story_as_one_entry() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let started = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: Some("cutting the codec seam".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        let session = SessionId(
            started["session"]
                .as_str()
                .expect("a session id")
                .to_string(),
        );

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "the seam is cut and the suite is green".into(),
                sid,
            }))
            .await
            .expect("wrap ok");

        let read = store.read_session(&session).await.expect("read ok");
        let told: Vec<&str> = read
            .entries
            .iter()
            .filter(|e| !e.is_auto())
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            told.len(),
            2,
            "the beat, then ONE closing entry — never two for one moment: {told:?}"
        );
        let last = told[1];
        assert!(
            last.contains("cutting the codec seam"),
            "the unpublished focus was flushed: {last:?}"
        );
        assert!(
            last.contains("the seam is cut and the suite is green"),
            "…into the story, not beside it: {last:?}"
        );
        assert!(
            last.find("cutting the codec seam") < last.find("the seam is cut"),
            "…and in the order they happened: {last:?}"
        );
    }

    /// A run that set no focus wraps on the story alone — nothing empty is
    /// folded in, and no blank line is left where a flush would have been.
    #[tokio::test]
    async fn wrapping_with_no_unpublished_focus_tells_the_story_alone() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "booted, found nothing to do".into(),
                    sid,
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["entry"]["text"], "booted, found nothing to do");
    }

    /// **A wrapped `sid` stays closed, and the bot behind it boots its next
    /// run.** Those are different questions and the answers have to differ: a
    /// `sid` names one run, and closed is terminal both ways for that record —
    /// while the identity outlives any run of it, so booting again is ordinary
    /// rather than a way back in.
    #[tokio::test]
    async fn a_wrapped_sid_stays_closed_while_its_bot_boots_the_next_run() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
        let first = booted(&client.call(), "gamma").await;

        client
            .call()
            .journal(Parameters(JournalArgs {
                entry: "the first run".into(),
                focus: None,
                sid: first.clone(),
            }))
            .await
            .expect("journal ok");
        let wrapped = json_of(
            &client
                .call()
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "the first run is over".into(),
                    sid: first.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        let closed = wrapped["session"]["id"]
            .as_str()
            .expect("an id")
            .to_string();

        // Naming THAT session is blocked — you meant that record.
        let named = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "one more thing".into(),
                    focus: None,
                    sid: first.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(
            named["status"], "blocked",
            "a closed session takes no more entries: {named}"
        );

        // Booting the BOT again starts its next run — the identity outlives the
        // run, and the door is where the name is given now.
        let second = booted(&client.call(), "gamma").await;
        let next = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "the second run".into(),
                    focus: None,
                    sid: second,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            next["session"],
            closed.as_str(),
            "a new run, not the closed one: {next}"
        );

        let all = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 2, "two runs of one role: {all:?}");
        assert_eq!(
            all.iter().filter(|s| !s.state.is_terminal()).count(),
            1,
            "…and exactly one of them is open"
        );
    }

    /// **A wrap as a first write is the same bug, and it is always prose.** A
    /// story written for somebody with none of your context is never one short
    /// line, so this path was broken for every caller who wrapped without
    /// journalling first.
    #[tokio::test]
    async fn a_wrap_can_be_a_first_write_and_the_story_is_prose() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let story = "read the hand-off and found nothing to do.\n\nWrapping without a beat: the \
                     `dev` box was empty and there was no slice to build.";
        let body = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    sid,
                }))
                .await
                .expect("a wrap as a first write must not error"),
        );
        assert_eq!(body["session"]["state"], "wrapped");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1);
        assert_eq!(
            live[0].entries[0].text,
            jojobot_domain::session::normalize_entry(story),
            "the story is the record — it must not be cut to fit a display field"
        );
    }

    /// **Wrapped is terminal both ways, through the surface.** Every session
    /// verb on a closed id comes back blocked, in the guards' one shape.
    #[tokio::test]
    async fn a_wrapped_session_refuses_every_further_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;
        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done".into(),
                sid: sid.clone(),
            }))
            .await
            .expect("wrap ok");

        let refused = |body: serde_json::Value, verb: &str| {
            assert_eq!(body["status"], "blocked", "{verb} must be blocked: {body}");
            assert_eq!(body["wrote"], false);
            let how = body["how_to_proceed"].as_str().expect("advice");
            // This end is the last word because the run told its story —
            // that, not a published account, is what makes this refusal
            // different from the one an abandoned run gets.
            assert!(
                how.contains("story has been told"),
                "{verb} has to say why: {how}"
            );
            assert!(
                !how.contains("Journal"),
                "{verb} must not cite a Journal that is gone: {how}"
            );
        };
        refused(
            json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: "one more thing".into(),
                        focus: None,
                        sid: sid.clone(),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "journal",
        );
        refused(
            json_of(
                &jojobot
                    .amend_journal(Parameters(AmendJournalArgs {
                        entry: "actually".into(),
                        sid: sid.clone(),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "amend_journal",
        );
        refused(
            json_of(
                &jojobot
                    .wrap_session(Parameters(WrapSessionArgs {
                        story: "done again".into(),
                        sid: sid.clone(),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "wrap_session",
        );
    }

    /// Wrapping one session leaves every other one running: a wrap reaches
    /// exactly the run its handle addresses — the session it closes, the story
    /// it tells, and nothing else. Closing somebody else's run must leave this
    /// one's card, tally and chronology exactly where they were.
    #[tokio::test]
    async fn wrapping_another_session_leaves_this_one_running() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        // Somebody else's session, on the same board.
        let theirs = store
            .begin(NewSession {
                bot: EntityId("bot:delta".into()),
                sid: Sid("d001".into()),
                focus: "their run".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .append(
                &theirs.id,
                NewEntry::manual("their beat", jiff::Timestamp::now()),
            )
            .await
            .expect("append ok");

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_id = mine["session"].as_str().expect("a session id").to_string();

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "wrapping theirs".into(),
                sid: as_run(&jojobot, "delta", &theirs.id),
            }))
            .await
            .expect("wrap ok");

        // My next beat continues MY session rather than minting a second card.
        journal_entry(&jojobot, &sid, "my second beat").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one card for this run, not two: {live:?}");
        assert_eq!(live[0].id.as_str(), my_id);
        assert_eq!(
            live[0].entries.len(),
            2,
            "…and it kept accruing: {:?}",
            live[0].entries
        );
    }

    /// **A retried wrap finishes what the first one started.** The close is the
    /// step most likely to fail transiently, and by then the story is already in
    /// the chronology AND the operator's Journal — so the only move left, wrap
    /// again, told the story twice in both places.
    ///
    /// The ordering is deliberately unchanged: the story reaches the session's
    /// own record first, so a failure after it loses nothing. What changed is
    /// that each write asks whether its own half is already done.
    #[tokio::test]
    async fn a_wrap_retried_after_a_failed_close_tells_the_story_once() {
        let (jojobot, store, _memory, sid) = refusing_close().await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                sid: sid.clone(),
            }))
        };
        assert!(
            wrap().await.is_err(),
            "the close refused, so the wrap failed"
        );

        // The retry, with the close working this time.
        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store
            .inner
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        // **Counted as occurrences, not as whole entries.** A wrap folds the
        // session's unpublished focus into the story, so the closing entry is
        // the story plus that line — the guard is about telling the story once,
        // not about the entry equalling it.
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.text.contains(story))
                .count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
        );
    }

    /// **A retry finishes what the first attempt started, wherever the story now
    /// sits.** The chronology half of the guard looked only at the newest entry,
    /// so anything written between the failed close and the retry — a journal
    /// entry saying the wrap failed, which is the natural thing to write — pushed
    /// the story off the tail and the retry told it a second time.
    #[tokio::test]
    async fn a_wrap_retried_after_an_intervening_entry_tells_the_story_once() {
        let (jojobot, store, _memory, sid) = refusing_close().await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                sid: sid.clone(),
            }))
        };
        assert!(
            wrap().await.is_err(),
            "the close refused, so the wrap failed"
        );

        // The natural next beat: saying so. It is now the tail, not the story.
        journal_entry(&jojobot, &sid, "the wrap failed at the close — retrying").await;

        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store
            .inner
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        // **Counted as occurrences, not as whole entries.** A wrap folds the
        // session's unpublished focus into the story, so the closing entry is
        // the story plus that line — the guard is about telling the story once,
        // not about the entry equalling it.
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.text.contains(story))
                .count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
        );
    }
}
