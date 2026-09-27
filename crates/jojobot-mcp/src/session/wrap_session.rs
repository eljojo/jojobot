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
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as
    /// the handle that thing wears today, even after a rename.
    pub(crate) story: String,
    /// Your session id — the session to wrap.
    pub(crate) sid: String,
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
                       boot again for a fresh sid. THE CLOSING ENTRY COMES BACK AS A RECEIPT \
                       and the story is not read back to you — it is in the chronology below it, \
                       where you can see what jojobot folded into it. THE CHRONOLOGY THIS HANDS BACK IS THE NEWEST \
                       OF THE RECORD, not all of it: a long run's answer would be one no client \
                       can read. `entry_count` is the whole length, and `chronology_elided` with \
                       `entries_omitted` says how much is not here. Nothing was dropped from the \
                       record — what was written is stored whole."
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
                .append(
                    &session,
                    NewEntry::manual(&story, self.clock().now(), caller.day),
                )
                .await
            {
                Ok(entry) => entry,
                Err(e) => return session_declined(e, caller.sid.as_str()),
            },
        };

        let wrapped = match self.sessions.close(&session, SessionState::Wrapped).await {
            Ok(wrapped) => wrapped,
            Err(e) => return session_declined(e, caller.sid.as_str()),
        };
        // **A wrapped session releases every role it held.** Best-effort and
        // after the close: the wrap itself already landed, and releasing a
        // lease is not what a caller telling its story asked for.
        self.release_role_claims(&caller.bot, caller.sid.as_str())
            .await;
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
            "entry": entry_receipt_json(&entry),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::{MarkProcessedArgs, PostMessageArgs};
    use crate::memory::archive_entity::ArchiveEntityArgs;
    use crate::memory::merge_entities::MergeArgs;
    use crate::memory::testing::{add_args, capture_args, recall_args, update_args};
    use crate::memory::{
        DeclareTypeArgs, RecallArgs, RenameEntityArgs, RetractArgs, SetCharterArgs,
    };
    use crate::session::testing::*;
    use jojobot_domain::session::Sid;

    /// **Wrapping releases every role the wrapping session held, and a fresh
    /// claimant is granted the same role at once** — through the served
    /// surface. A role held by a DIFFERENT sid is left alone, which is the
    /// pair that tells "release" from "clear every claim on the bot".
    #[tokio::test]
    async fn wrapping_releases_the_sessions_own_role_claims_and_leaves_others_alone() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let claimed = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let holder_sid = sid_of(&claimed).expect("a handle");
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");

        // A second, unrelated role, held by a DIFFERENT session of the same
        // bot — the control that tells release from "clear everything".
        let other = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("reviewer-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let other_sid = sid_of(&other).expect("a handle");
        assert_eq!(other["session"]["claim"]["status"], "taken", "{other}");

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done with dev-dispatch for now".into(),
                sid: holder_sid.clone(),
            }))
            .await
            .expect("wrap ok");

        // A fresh session claims the released role at once and is granted it.
        // `resume: "new"` for the same reason `other`'s own claim above needs
        // it: `other`'s session is still live, so a bare `resume: None` here
        // would meet that unrelated choice rather than a claimant to decide.
        let fresh = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            fresh["session"]["claim"]["status"], "taken",
            "the wrap must have released dev-dispatch, or a fresh claimant meets a lease its \
             old holder never gave up: {fresh}"
        );

        // The other session's own, unrelated role is untouched.
        let rival_on_other = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("reviewer-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            rival_on_other["session"]["claim"]["status"], "refused",
            "wrapping one session must not release a DIFFERENT session's role: {rival_on_other}"
        );
        assert_eq!(
            rival_on_other["session"]["claim"]["holder"], other_sid,
            "{rival_on_other}"
        );
    }

    /// **Wrapping flushes the current unpublished beat INTO the story, as one
    /// entry.** The operator's ruling, and the "one entry" half is the part a
    /// reasonable implementation gets wrong: *"it should be both but it should
    /// be one entry."*
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

    /// **A wrap's answer is capped like a boot's, and this pins the decision
    /// rather than discovering it.**
    ///
    /// The cap lives in the one session renderer, so `wrap_session` inherited
    /// it: no card scoped that, and a behaviour nobody decided is one anybody
    /// may unpick by accident. It is the right answer here for the reason it is
    /// right at a boot — a long run's whole chronology is a response no client
    /// reads — and nothing is lost, because the store holds the record whole and
    /// the answer states what it left out.
    #[tokio::test]
    async fn a_long_runs_wrap_serves_the_newest_of_its_chronology_and_says_so() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        for nth in 0..20 {
            jojobot
                .journal(Parameters(JournalArgs {
                    entry: format!("beat {nth:02} {}", "w".repeat(1500)),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok");
        }

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "the run is over and the story is told".into(),
                    sid,
                }))
                .await
                .expect("wrap ok"),
        );
        let session = &wrapped["session"];
        let chronology = session["chronology"].as_array().expect("a chronology");

        // The positive first: the story a wrap exists to tell is in the answer.
        // Asserting only that entries were dropped would pass on an answer that
        // dropped the one entry this verb wrote.
        assert!(
            chronology.last().expect("a wrap carries its own entry")["text"]
                .as_str()
                .expect("an entry's text")
                .contains("the run is over and the story is told"),
            "{session}"
        );
        assert_eq!(session["chronology_elided"], true, "{session}");
        assert!(
            session["entries_omitted"]
                .as_u64()
                .expect("what was left out is counted")
                > 0,
            "{session}"
        );
        assert!(
            chronology.len() < session["entry_count"].as_u64().expect("a length") as usize,
            "entry_count is the whole record and the answer carries less: {session}"
        );
    }

    /// **The elision names the way back, and the way it names really reaches
    /// what it left out.** Rule 306 refused truncate-then-hunt-by-grep; the
    /// fix is an ordinary addressable read, not a search, not a raised cap,
    /// and not a sibling verb. `recall` already reads a session's own handle
    /// whole (`session::projected`, unbounded) — this proves the note names
    /// exactly that call, and that the call it names lands on the oldest
    /// entry the wrap answer itself dropped.
    #[tokio::test]
    async fn the_chronology_note_names_a_call_that_actually_reaches_what_it_left_out() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        for nth in 0..20 {
            jojobot
                .journal(Parameters(JournalArgs {
                    entry: format!("beat {nth:02} {}", "w".repeat(1500)),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok");
        }

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "the run is over and the story is told".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        let session = &wrapped["session"];
        assert_eq!(session["chronology_elided"], true, "{session}");
        let session_id = session["id"].as_str().expect("a session id").to_string();

        // The note names an address and a verb — structure, not prose: a
        // rewording would not break this, a renamed handle scheme or a
        // renamed verb would.
        let note = session["chronology_note"]
            .as_str()
            .expect("an elision says how to reach the rest");
        assert!(
            note.contains(&session_id) && note.contains("recall"),
            "the note does not name an addressable way back: {note}"
        );

        // The oldest beat is the control: it must be the one thing this
        // wrap's own answer did NOT carry, or the round trip below proves
        // nothing. The needle is the entry's own body, not the bare "beat
        // 00" prefix — the wrap's closing entry legitimately flushes a
        // stale, never-updated focus derived from that same prefix, and a
        // needle that matched it would make this control pass for the wrong
        // reason.
        let oldest_body = format!("beat 00 {}", "w".repeat(1500));
        let kept_chronology = session["chronology"].as_array().expect("a chronology");
        assert!(
            !kept_chronology.iter().any(|e| e["text"]
                .as_str()
                .unwrap_or_default()
                .contains(&oldest_body)),
            "the fixture is only a control if the oldest beat was really left out: {session}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    view: None,
                    subject: Some(format!("session:{session_id}")),
                    kind: None,
                    answers_type: None,
                    fields: None,
                    facts: None,
                    stood_for: None,
                    prose: Some(true),
                    charter: None,
                    follow: None,
                    overdue: None,
                    near: None,
                    sid: Some(sid),
                    history: None,
                    history_record: None,
                    history_most: None,
                    values: None,
                    values_most: None,
                    built_on: None,
                    backing: None,
                }))
                .await
                .expect("recall answers rather than failing the protocol"),
        );
        let prose = recalled["objects"][0]["prose"]
            .as_str()
            .expect("the session's own prose, asked for by name");
        assert!(
            prose.contains(&oldest_body),
            "the call the note names did not reach the entry the wrap answer left out: {prose}"
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
        // **Read off the chronology, which is the half the caller does not
        // hold.** The receipt says the entry landed; the record says what it
        // says, and with no focus to fold in it is the story alone.
        assert_eq!(
            wrapped["entry"]["text"],
            serde_json::Value::Null,
            "the story is not read back to the session that just told it: {wrapped}"
        );
        let chronology = wrapped["session"]["chronology"]
            .as_array()
            .expect("the record's own chronology");
        assert_eq!(
            chronology.last().expect("the closing entry")["text"],
            "booted, found nothing to do",
            "with nothing open to fold in, the closing entry is the story alone: {wrapped}"
        );
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

    /// **Wrapped is terminal for every write, not only the three session
    /// verbs.** `journal`, `amend_journal` and `wrap_session` already refuse
    /// through the store's own close/append check — this proves the rest of
    /// the surface, which carries a wrapped sid only for attribution, refuses
    /// too. Each write is proven to land BEFORE the wrap and refuse AFTER
    /// it, which is the pair that tells "this verb checks" from "this verb
    /// never worked". A read stays answered either side: it is attributed,
    /// never journalled.
    #[tokio::test]
    async fn a_wrapped_session_refuses_every_write_outside_the_session_surface() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let before = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(sid.clone()),
                    ..capture_args("bot:gamma", "before the wrap")
                }))
                .await
                .expect("capture call ok"),
        );
        assert_ne!(
            before["status"], "blocked",
            "a write before wrap must land: {before}"
        );

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
            assert_eq!(body["wrote"], false, "{verb}: {body}");
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("story has been told"),
                "{verb} has to say why: {how}"
            );
        };

        refused(
            json_of(
                &jojobot
                    .capture(Parameters(CaptureArgs {
                        sid: Some(sid.clone()),
                        ..capture_args("bot:gamma", "after the wrap")
                    }))
                    .await
                    .expect("call ok"),
            ),
            "capture",
        );
        refused(
            json_of(
                &jojobot
                    .update_fact(Parameters(UpdateFactArgs {
                        sid: Some(sid.clone()),
                        ..update_args("bot:gamma#f1")
                    }))
                    .await
                    .expect("call ok"),
            ),
            "update_fact",
        );
        refused(
            json_of(
                &jojobot
                    .add_entity(Parameters(AddEntityArgs {
                        sid: Some(sid.clone()),
                        ..add_args("topic", "widgets", "Widgets")
                    }))
                    .await
                    .expect("call ok"),
            ),
            "add_entity",
        );
        refused(
            json_of(
                &jojobot
                    .archive_entity(Parameters(ArchiveEntityArgs {
                        handle: "bot:gamma".into(),
                        reason: "test".into(),
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "archive_entity",
        );
        refused(
            json_of(
                &jojobot
                    .merge_entities(Parameters(MergeArgs {
                        duplicate: "bot:gamma".into(),
                        survivor: "bot:gamma".into(),
                        reason: None,
                        recorded_at: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "merge_entities",
        );
        refused(
            json_of(
                &jojobot
                    .rename_entity(Parameters(RenameEntityArgs {
                        handle: "bot:gamma".into(),
                        to: "gamma2".into(),
                        parent: None,
                        recorded_at: None,
                        override_token: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "rename_entity",
        );
        refused(
            json_of(
                &jojobot
                    .retract(Parameters(RetractArgs {
                        address: "bot:gamma#f1".into(),
                        reason: None,
                        recorded_at: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "retract",
        );
        refused(
            json_of(
                &jojobot
                    .update_entity(Parameters(UpdateEntityArgs {
                        handle: "bot:gamma".into(),
                        name: None,
                        aliases: None,
                        source: None,
                        crm: None,
                        override_token: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "update_entity",
        );
        refused(
            json_of(
                &jojobot
                    .set_charter(Parameters(SetCharterArgs {
                        bot: "bot:gamma".into(),
                        prose: "a new charter".into(),
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "set_charter",
        );
        refused(
            json_of(
                &jojobot
                    .post_message(Parameters(PostMessageArgs {
                        to: "gamma".into(),
                        body: "hi".into(),
                        sid: sid.clone(),
                        subject: None,
                        in_reply_to: None,
                    }))
                    .await
                    .expect("call ok"),
            ),
            "post_message",
        );
        refused(
            json_of(
                &jojobot
                    .mark_processed(Parameters(MarkProcessedArgs {
                        message_id: "whatever".into(),
                        notes: None,
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "mark_processed",
        );
        refused(
            json_of(
                &jojobot
                    .declare_type(Parameters(DeclareTypeArgs {
                        name: "whatever".into(),
                        fields: Vec::new(),
                        sid: Some(sid.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "declare_type",
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid.clone()),
                    ..recall_args("bot:gamma")
                }))
                .await
                .expect("recall answers rather than failing the protocol"),
        );
        assert_ne!(
            recalled["status"], "blocked",
            "a read after wrap must still answer: {recalled}"
        );
    }

    /// **Every write that reaches this list, not the list somebody wrote by
    /// hand.** `update_entity` carried a wrapped sid to a write for one whole
    /// slice because the test above named eleven verbs somebody chose, and a
    /// twelfth verb outside that choice was never asked. This reads the
    /// SERVED surface — the same list
    /// [`crate::surface::the_tool_surface_is_exactly_this_list`] pins — and
    /// requires every verb on it to be named in exactly one of the two lists
    /// below. A verb in neither fails the case: a new write verb that
    /// forgets the guard, or forgets to be named here, fails closed instead
    /// of passing silently.
    ///
    /// `WRITES` are proven refused above, in this test or its sibling
    /// (`journal`, `amend_journal` and `wrap_session` refuse through the
    /// store's own close/append check, proven by
    /// `a_wrapped_session_refuses_every_further_write`; the rest are proven
    /// by `a_wrapped_session_refuses_every_write_outside_the_session_surface`,
    /// directly above). `READS` stay answered on a wrapped sid because a
    /// read is attributed and never journalled — `start_here` is the boot
    /// door and out of scope for this rule, so it is named here rather than
    /// left to fall through unclassified.
    const WRITES: &[&str] = &[
        "add_entity",
        "amend_journal",
        "archive_entity",
        "capture",
        "declare_type",
        "journal",
        "mark_processed",
        "merge_entities",
        "post_message",
        "rename_entity",
        "retract",
        "set_charter",
        "update_entity",
        "update_fact",
        "wrap_session",
    ];
    const READS: &[&str] = &[
        "list_entities",
        "list_runs",
        "list_sent",
        "ping",
        "read_mailbox",
        "read_message",
        "recall",
        "search",
        "start_here",
    ];

    #[test]
    fn every_served_verb_is_named_as_a_write_or_a_read() {
        let tools = crate::Jojobot::tool_router().list_all();
        assert!(!tools.is_empty(), "the served surface named no verbs");
        for tool in &tools {
            let name = tool.name.as_ref();
            assert!(
                WRITES.contains(&name) || READS.contains(&name),
                "{name} is served but is named in neither WRITES nor READS — a new write \
                 verb has to gain the wrapped guard and a line here, or a new read verb \
                 has to be named in READS",
            );
        }
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
                timezone: None,
                bot: EntityId("bot:delta".into()),
                sid: Sid("d001".into()),
                focus: "their run".into(),
                started_at: jiff::Timestamp::now(),
                started_on: None,
            })
            .await
            .expect("begin ok");
        store
            .append(
                &theirs.id,
                NewEntry::manual("their beat", jiff::Timestamp::now(), None),
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
