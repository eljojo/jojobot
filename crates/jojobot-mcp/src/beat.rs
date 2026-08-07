//! **jojobot's own account of what a session did** — one beat per verb class,
//! its count and examples corrected in place as the class repeats.
//!
//! Marked apart from what the session said about itself, because what you said
//! you were doing and what jojobot noticed you doing are different kinds of
//! evidence. The tally's FORMAT — how a beat renders and reads back — is the
//! session record's own and lives in `jojobot_domain::session`; what is here is
//! the orchestration: resolve the caller, take the gate, read the tally off the
//! chronology, amend or append.
//!
//! Called from every context that writes, which is why it is not in one.

use super::*;

impl Jojobot {
    /// Record one coarse beat for a verb class — **at most one per class per
    /// session**, its count and examples corrected in place as the class repeats.
    ///
    /// One case leaves two lines of a class, and does so deliberately: a beat
    /// somebody rewrote by hand no longer parses as a tally, so [`beats_of`]
    /// does not find it and the class opens a fresh one beside it. Their words
    /// stay theirs — overwriting what a person wrote on the card to keep a count
    /// tidy is the worse trade.
    ///
    /// Silent by design in three cases, all of them "there is nobody to record
    /// this for": a caller carrying no handle (jojobot will not guess which
    /// identity made a call), a session store that refuses, and a beat that
    /// fails to
    /// write. **A beat never fails the verb it is about.** A capture that landed
    /// did land; reporting it as failed because its footnote could not be
    /// written would make the record wrong in the more damaging direction.
    ///
    /// The `sid` rides every verb, so a caller that keeps passing it is
    /// beaten about wherever it writes, whatever its client does with
    /// connections. What is left in the first case is a caller carrying no
    /// `sid` at all, which has not asked to be recorded anywhere.
    ///
    /// A handle that is DEAD is not one of the silent cases: that refusal is
    /// made before the write, by [`Jojobot::attributable`]. What is left here
    /// is the sliver where a handle died between that check and this call,
    /// and silence is right for it — the write has already landed.
    pub(crate) async fn beat(&self, class: &'static str, example: &str, sid: Option<&str>) {
        // **No caller, no beat.** jojobot does not guess which identity made a
        // call, and an anonymous one is legitimate — a reader, a poster who
        // never booted. What it is not is somebody to record work against.
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let Some((_, phrase)) = BEAT_CLASSES.iter().find(|(known, _)| *known == class) else {
            // A class with no phrase would render a beat nothing can read back,
            // so it writes none at all rather than one that breaks the tally on
            // the next reconnect.
            tracing::warn!(
                class,
                "no beat phrase for this verb class — no beat written"
            );
            return;
        };
        let gate = self.registry.gate(&self.gate_key(sid));
        let _serialized = gate.lock().await;
        // Re-read the caller inside the gate: a racing write may have
        // materialized the card since, and beginning a second one here is the
        // fork this lock exists to prevent.
        let Ok(Some(caller)) = self.caller(Some(caller.sid.as_str())) else {
            return;
        };
        let Ok(session) = self
            .session_for(&_serialized, &caller, None, Some(phrase))
            .await
        else {
            return;
        };

        // The tally is read back off the session, never cached: caching it
        // on the connection would let a reconnect append a second beat for a
        // class that already has one.
        let held = match self.sessions.read_session(&session).await {
            Ok(read) => beats_of(&read).get(class).cloned(),
            Err(e) => {
                tracing::warn!(error = %e, class, "a session could not be read for its tally");
                return;
            }
        };
        let outcome = match held {
            Some(mut beat) => {
                beat.count += 1;
                if beat.examples.len() < BEAT_EXAMPLES
                    && !beat.examples.iter().any(|e| e == example)
                {
                    beat.examples.push(example.to_string());
                }
                let text = beat_text(phrase, &beat);
                self.sessions
                    .amend_beat(&session, &beat.entry, &text, jiff::Timestamp::now())
                    .await
                    .map(|_| ())
            }
            None => {
                let beat = Beat {
                    entry: EntryId(String::new()),
                    count: 1,
                    examples: vec![example.to_string()],
                };
                let text = beat_text(phrase, &beat);
                self.sessions
                    .append(
                        &session,
                        NewEntry::beat(class, text, jiff::Timestamp::now()),
                    )
                    .await
                    .map(|_| ())
            }
        };
        if let Err(e) = outcome {
            tracing::warn!(
                error = %e, class, session = %session,
                "a session beat could not be written — the verb it is about still succeeded"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;
    use rmcp::handler::server::wrapper::Parameters;

    /// **A session's own entries come back e1, e3, e5 — and the gaps are
    /// jojobot's beats, not lost writes.**
    ///
    /// Reported from a live run as "e1 then e3 with nothing written in between,
    /// cause unknown", with a guess that a focus write was consuming an id. It
    /// is not that, and nothing is missing. Entry ids are minted over the WHOLE
    /// chronology, and jojobot's own beats are entries in it — so a caller
    /// reading only the lines it wrote sees its own subsequence with the beats'
    /// ids cut out of it. `next_entry_id` scans every id on the page precisely
    /// so none is ever reused; a gap is the guarantee working, not a symptom.
    ///
    /// Written down as a test rather than as an answer in a report, because the
    /// next person to notice it will notice it the same way and reason their way
    /// to the same wrong guess.
    #[tokio::test]
    async fn a_gap_in_a_sessions_own_entry_ids_is_a_beat_and_not_a_lost_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        journal_entry(&jojobot, &sid, "read the hand-off").await;
        // A write of a class jojobot beats about — the thing that lands between
        // the caller's two entries without the caller writing it.
        ensure_as(&jojobot, &sid, "alpha").await;
        journal_entry(&jojobot, &sid, "scoped the slice").await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let entries = &live[0].entries;

        let mine: Vec<&str> = entries
            .iter()
            .filter(|e| e.beat.is_none())
            .map(|e| e.id.as_str())
            .collect();
        let beats: Vec<&str> = entries
            .iter()
            .filter(|e| e.beat.is_some())
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(mine.len(), 2, "the caller wrote two: {entries:?}");
        assert_eq!(beats.len(), 1, "jojobot wrote one: {entries:?}");

        // The whole point: the caller's own ids are NOT consecutive, and the
        // id missing from its run is the beat's.
        assert_ne!(
            mine[1], beats[0],
            "a beat is a different entry, not a relabelled one"
        );
        let all: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            all.len(),
            all.iter().collect::<std::collections::BTreeSet<_>>().len(),
            "and no id is ever reused, which is what the gap is protecting: {all:?}"
        );
        let at = |id: &str| all.iter().position(|c| *c == id).expect("an entry");
        assert!(
            at(mine[0]) < at(beats[0]) && at(beats[0]) < at(mine[1]),
            "the beat sits between them, which is why the caller sees a gap: {all:?}"
        );
    }

    /// **One beat per verb class, its count kept current.** jojobot's own
    /// footnotes are a tally, not a log: the second capture corrects the first
    /// beat rather than adding one, and they stay marked apart from what the
    /// session said about itself.
    #[tokio::test]
    async fn jojobot_writes_one_beat_per_verb_class_and_keeps_its_count() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        ensure_as(&jojobot, &sid, "alpha").await;
        ensure_as(&jojobot, &sid, "milhouse").await;
        capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        capture_as(&jojobot, &sid, capture_args("milhouse", "plays chess")).await;
        journal_entry(&jojobot, &sid, "captured a couple of things").await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let entries = &live[0].entries;
        let beats: Vec<(&str, &str)> = entries
            .iter()
            .filter_map(|e| e.beat.as_deref().map(|b| (b, e.text.as_str())))
            .collect();
        assert_eq!(
            beats
                .iter()
                .filter(|(class, _)| *class == "capture")
                .count(),
            1,
            "one beat for the class, however many captures: {entries:?}"
        );
        let (_, tally) = beats
            .iter()
            .find(|(class, _)| *class == "capture")
            .expect("a capture beat");
        assert!(
            tally.contains("(2)"),
            "…with its count kept current: {tally}"
        );
        assert!(
            tally.contains("person:alpha"),
            "…and what it touched: {tally}"
        );
        assert!(tally.contains("person:milhouse"), "…both of them: {tally}");

        // The classes stay apart, and so do jojobot's words and the session's.
        assert!(
            beats.iter().any(|(class, _)| *class == "add_entity"),
            "a different verb class is a different beat: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| !e.is_auto() && e.text == "captured a couple of things"),
            "the session's own entry is not a beat: {entries:?}"
        );
    }

    /// **The tally belongs to the session, not to the connection.** Resuming
    /// rebuilt an empty beat map, so the first verb of each class after every
    /// reconnect appended a SECOND beat for that class — and a reconnect is the
    /// headline case this milestone exists for, so the duplicate would have been
    /// the normal shape rather than the rare one.
    ///
    /// The chronology already says which class each beat is about, so the tally
    /// is re-derivable: attaching reads it back off the entries.
    #[tokio::test]
    async fn the_beat_tally_survives_a_reconnect() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let first = connection(memory.clone(), store.clone());
        make_bot(&first, "gamma").await;
        let sid = booted(&first, "gamma").await;
        ensure_as(&first, &sid, "alpha").await;
        capture_as(&first, &sid, capture_args("alpha", "plays go")).await;

        // A reconnect, then another capture. The handle is the address, so
        // resuming the run in flight is what the reconnect answers with.
        let second = connection(memory, store.clone());
        let again = resumed(&second, "gamma").await;
        ensure_as(&second, &again, "milhouse").await;
        capture_as(&second, &again, capture_args("milhouse", "plays chess")).await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let captures: Vec<&str> = live[0]
            .entries
            .iter()
            .filter(|e| e.beat.as_deref() == Some("capture"))
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            captures.len(),
            1,
            "one beat for the class across both connections: {:?}",
            live[0].entries
        );
        assert!(
            captures[0].contains("(2)"),
            "…and the count carried across the reconnect: {}",
            captures[0]
        );
        assert!(
            captures[0].contains("person:alpha") && captures[0].contains("person:milhouse"),
            "…along with what it touched on both sides: {}",
            captures[0]
        );
    }

    /// **A beat line a person rewrote stays theirs, and the class starts over
    /// beside it.** jojobot reads its tally back out of the line it rendered, so
    /// a hand-edited one no longer parses — and the deliberate answer is to
    /// leave their words alone and open a fresh tally rather than overwrite what
    /// somebody wrote on the card. The cost is the one case where a session
    /// carries two beat lines of one class, which is why the rule is "at most
    /// one per class that jojobot itself is still keeping".
    #[tokio::test]
    async fn a_hand_edited_beat_is_left_alone_and_the_class_starts_a_fresh_tally() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let first = connection(memory.clone(), store.clone());
        make_bot(&first, "gamma").await;
        let sid = booted(&first, "gamma").await;
        ensure_as(&first, &sid, "alpha").await;
        capture_as(&first, &sid, capture_args("alpha", "plays go")).await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let beat = live[0]
            .entries
            .iter()
            .find(|e| e.beat.as_deref() == Some("capture"))
            .expect("a capture beat")
            .clone();

        // Somebody edits that comment on the board, in their own words.
        let theirs = "I checked these myself — they are right";
        store
            .amend_beat(&live[0].id, &beat.id, theirs, jiff::Timestamp::now())
            .await
            .expect("amend ok");

        // A reconnect: the tally is re-read off the chronology, and this line no
        // longer says anything jojobot can count.
        let second = connection(memory, store.clone());
        let again = resumed(&second, "gamma").await;
        ensure_as(&second, &again, "milhouse").await;
        capture_as(&second, &again, capture_args("milhouse", "plays chess")).await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let captures: Vec<&str> = live[0]
            .entries
            .iter()
            .filter(|e| e.beat.as_deref() == Some("capture"))
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            captures,
            vec![theirs, "captured facts about: person:milhouse (1)"],
            "their line untouched, and a fresh tally beside it: {:?}",
            live[0].entries
        );
    }

    /// **A call carrying no identity auto-journals nothing** — not even when
    /// there is exactly one live session it could obviously have meant.
    ///
    /// jojobot does not guess which session made a call. The temptation is the
    /// single-candidate case: one bot, one run in flight, an unaddressed write
    /// arriving — and resolving it "helpfully" attributes somebody's work to a
    /// session they did not name.
    ///
    /// **The fixture is the assertion here.** This booted nothing and wrote
    /// nothing before, so the board was empty and there was nothing for a
    /// guessing implementation to guess FROM: restoring the deleted
    /// fall-back-to-the-one-live-run resolver left the suite green, which is a
    /// test that cannot fail. There is a card on the board now, warm and
    /// unambiguous, and the anonymous write must still leave it alone.
    #[tokio::test]
    async fn a_call_carrying_no_sid_writes_no_beats() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        ensure(&jojobot, "alpha").await;
        make_bot(&jojobot, "gamma").await;

        // One live run, with a card already materialized: the single obvious
        // candidate an unaddressed write would land in if anything resolved it.
        let sid = booted(&jojobot, "gamma").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("alpha", "plays go")
            },
        )
        .await;
        let chronology = || async {
            let runs = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            assert_eq!(runs.len(), 1, "one run in flight: {runs:?}");
            runs[0]
                .entries
                .iter()
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
        };
        let before = chronology().await;
        assert_eq!(
            before.len(),
            1,
            "…carrying the beat for the write that named it: {before:?}"
        );

        // **Two anonymous writes, of two classes, because they fail
        // differently.** A guessing resolver reached by a class the session
        // already has AMENDS that beat in place — the entry count never moves,
        // so counting entries alone is a test that still cannot fail. A class it
        // does not have opens a new one.
        capture_ok(&jojobot, capture_args("alpha", "plays go on tuesdays")).await;
        jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "person:alpha".into(),
                name: Some("Alpha, renamed by nobody".into()),
                aliases: None,
                source: None,
                crm: None,
                override_token: None,
                sid: None,
            }))
            .await
            .expect("update ok");

        let after = chronology().await;
        assert_eq!(
            after, before,
            "an anonymous write lands in nobody's chronology, however obvious the candidate — \
             neither as a new beat nor as a count moving on one that is already there"
        );
    }

    /// The same race, one class down: two concurrent captures must leave one
    /// beat, not two.
    ///
    /// **Counting the class on one card is not enough to see the failure.** A
    /// beat mints the card it writes to, so an ungated race forks — and each of
    /// the two cards then carries exactly one beat of the class, which reads as
    /// a pass on whichever card is looked at. The card count is what turns the
    /// fork into a failure, so it is asserted first.
    #[tokio::test]
    async fn concurrent_same_class_verbs_leave_exactly_one_beat() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let (a, b) = tokio::join!(
            jojobot.capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("alpha", "plays go")
            })),
            jojobot.capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("milhouse", "plays chess")
            })),
        );
        a.expect("capture ok");
        b.expect("capture ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, not one per racing capture: {live:?}"
        );
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.beat.as_deref() == Some("capture"))
                .count(),
            1,
            "one beat for the class, whatever raced: {:?}",
            live[0].entries
        );
    }
}
