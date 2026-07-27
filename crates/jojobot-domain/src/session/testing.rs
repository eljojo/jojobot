//! The Sessions contract, and the in-memory fake that must satisfy it.
//!
//! One behavioural spec, three tiers — the fake here, the real Vikunja adapter
//! over an in-memory API double, and the real adapter against real Vikunja —
//! and **the spec is the same code in all three**, which is what stops the fake
//! from drifting into a store that agrees with the tests and disagrees with
//! reality.
//!
//! Behind the `testing` feature, so it compiles for tests here and in downstream
//! crates but never ships in a production binary.

use std::sync::Mutex;

use jiff::Timestamp;

use super::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions, normalize_entry, validate_entry, validate_focus, validate_session_id,
};
use crate::memory::EntityId;

/// The in-memory [`Sessions`] fake — a real store that holds a write, with no
/// network. Deterministic: ids are a monotonic counter, never a clock.
#[derive(Default)]
pub struct InMemorySessions {
    sessions: Mutex<Vec<Session>>,
    next_id: Mutex<u64>,
}

impl InMemorySessions {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn mint(&self) -> String {
        let mut next = self.next_id.lock().expect("id lock");
        *next += 1;
        next.to_string()
    }

    /// The refusal every verb that writes to a session owes: a closed session
    /// takes nothing more. One helper, so the three write verbs cannot come to
    /// disagree about what closed means.
    fn writable(sessions: &mut [Session], id: &SessionId) -> Result<usize, SessionError> {
        let at = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        if sessions[at].state.is_terminal() {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: sessions[at].state,
            });
        }
        Ok(at)
    }
}

#[async_trait::async_trait]
impl Sessions for InMemorySessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let mut found: Vec<Session> = self
            .sessions
            .lock()
            .expect("session lock")
            .iter()
            .filter(|s| &s.bot == bot)
            .cloned()
            .collect();
        // Newest start first, the id breaking a tie so the order is total and
        // two reads agree.
        found.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(found)
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        self.sessions
            .lock()
            .expect("session lock")
            .iter()
            .find(|s| &s.id == id)
            .cloned()
            .ok_or_else(|| SessionError::UnknownSession {
                attempted: id.to_string(),
            })
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        validate_focus(&new.focus)?;
        let session = Session {
            id: SessionId(self.mint()),
            bot: new.bot,
            focus: new.focus.trim().to_string(),
            started_at: new.started_at,
            state: SessionState::Active,
            entries: Vec::new(),
        };
        self.sessions
            .lock()
            .expect("session lock")
            .push(session.clone());
        Ok(session)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(&entry.text)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        let recorded = JournalEntry {
            id: EntryId(self.mint()),
            at: entry.at,
            text: normalize_entry(&entry.text),
            touched: None,
            beat: entry.beat,
        };
        sessions[at].entries.push(recorded.clone());
        Ok(recorded)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(text)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        let last = sessions[at]
            .entries
            .last_mut()
            .ok_or_else(|| SessionError::NoEntries {
                attempted: id.to_string(),
            })?;
        last.text = normalize_entry(text);
        Ok(last.clone())
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(text)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let index = Self::writable(&mut sessions, id)?;
        let held = sessions[index]
            .entries
            .iter_mut()
            .find(|e| &e.id == entry)
            .ok_or_else(|| SessionError::NoEntries {
                attempted: id.to_string(),
            })?;
        if !held.is_auto() {
            return Err(SessionError::NotABeat {
                attempted: entry.to_string(),
                session: id.to_string(),
            });
        }
        held.text = normalize_entry(text);
        // The beat keeps its place in the chronology and records that it moved.
        held.touched = Some(at);
        Ok(held.clone())
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        validate_focus(focus)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        sessions[at].focus = focus.trim().to_string();
        Ok(sessions[at].clone())
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        sessions[at].state = to;
        Ok(sessions[at].clone())
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        if sessions[at].state.is_final() {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: sessions[at].state,
            });
        }
        sessions[at].state = SessionState::Active;
        Ok(sessions[at].clone())
    }
}

/// The shared behavioural spec — every adapter must satisfy all of it.
///
/// Names here come from a fixed, openly fictional roster; nothing in this file
/// names anything from the operator's life.
pub mod contract {
    use super::*;

    /// A fixed instant, so the spec never reads a clock.
    pub fn epoch() -> Timestamp {
        Timestamp::from_second(1_780_000_000).expect("a valid fixed instant")
    }

    fn at(offset: i64) -> Timestamp {
        epoch() + jiff::SignedDuration::from_secs(offset)
    }

    fn bot(slug: &str) -> EntityId {
        EntityId(format!("bot:{slug}"))
    }

    /// Begin a session, asserting the store took it.
    pub async fn begin(store: &dyn Sessions, slug: &str, focus: &str, at_offset: i64) -> Session {
        store
            .begin(NewSession {
                bot: bot(slug),
                focus: focus.to_string(),
                started_at: at(at_offset),
            })
            .await
            .expect("begin should succeed")
    }

    /// Append a manual entry, asserting the store took it.
    pub async fn journal(
        store: &dyn Sessions,
        id: &SessionId,
        text: &str,
        at_offset: i64,
    ) -> JournalEntry {
        store
            .append(id, NewEntry::manual(text, at(at_offset)))
            .await
            .expect("append should succeed")
    }

    /// A begun session is active, carries what it was begun with, and has no
    /// chronology yet — a session that has done nothing says nothing.
    pub async fn a_begun_session_is_active_and_empty(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        assert_eq!(session.bot, bot("gamma"));
        assert_eq!(session.focus, "reading the hand-off");
        assert_eq!(session.started_at, at(0));
        assert_eq!(session.state, SessionState::Active);
        assert!(
            session.entries.is_empty(),
            "a fresh session has no chronology"
        );
        assert!(!session.id.as_str().is_empty(), "the store mints an id");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(
            read, session,
            "…and it reads back exactly as it was written"
        );
    }

    /// **The chronology is append-only and ordered.** Entries come back oldest
    /// first, whatever order the store holds them in — a journal read in an
    /// arbitrary order is not a journal.
    pub async fn the_chronology_accrues_oldest_first(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        journal(store, &session.id, "read the task", 60).await;
        journal(store, &session.id, "wrote the domain module", 120).await;
        journal(store, &session.id, "watched the contract fail", 180).await;

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "read the task",
                "wrote the domain module",
                "watched the contract fail"
            ],
            "oldest first"
        );
        assert!(
            read.entries.iter().all(|e| !e.is_auto()),
            "a session's own entries are not beats: {:?}",
            read.entries
        );
    }

    /// An automatic beat rides in the same chronology, **marked apart**. A
    /// reader has to be able to tell what the session said from what jojobot
    /// noticed, and a flag on the entry is the only place that can live.
    pub async fn a_beat_is_stored_beside_manual_entries_and_stays_distinguishable(
        store: &dyn Sessions,
    ) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        journal(store, &session.id, "read the task", 60).await;
        store
            .append(
                &session.id,
                NewEntry::beat("capture", "captured facts: person:milhouse", at(90)),
            )
            .await
            .expect("append should succeed");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(read.entries.len(), 2);
        assert!(!read.entries[0].is_auto(), "the session's own words");
        assert_eq!(
            read.entries[1].beat.as_deref(),
            Some("capture"),
            "…and jojobot's beat says which verb class it is about"
        );
    }

    /// **Only the most recent entry may be amended, and only in place.** An
    /// amend that appended instead would make a correction read as a new beat;
    /// one that reached further back would make the whole chronology rewritable.
    pub async fn an_amend_rewrites_the_last_entry_and_nothing_else(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        journal(store, &session.id, "read the task", 60).await;
        let second = journal(store, &session.id, "wrote the domian module", 120).await;

        let amended = store
            .amend_last(&session.id, "wrote the domain module")
            .await
            .expect("amend ok");
        assert_eq!(amended.id, second.id, "the same entry, rewritten in place");
        assert_eq!(amended.text, "wrote the domain module");

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["read the task", "wrote the domain module"],
            "two entries, not three — an amend is never an append"
        );
    }

    /// **A beat is a tally, so it is rewritten where it sits — and only a
    /// beat.** A second capture does not deserve a second entry; it deserves the
    /// first one to say two. Reaching for an entry the session wrote is refused,
    /// which is what keeps the append-only rule true while a machine's count
    /// stays accurate.
    pub async fn only_an_automatic_beat_is_amended_in_place(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        let beat = store
            .append(
                &session.id,
                NewEntry::beat("capture", "captured facts: person:milhouse", at(60)),
            )
            .await
            .expect("append ok");
        let mine = journal(store, &session.id, "read the task", 120).await;

        // The beat is rewritten where it sits — behind a later entry, which is
        // exactly what `amend_last` could never reach.
        let counted = store
            .amend_beat(
                &session.id,
                &beat.id,
                "captured facts: person:milhouse, person:otto (2)",
                at(180),
            )
            .await
            .expect("amend_beat ok");
        assert_eq!(counted.id, beat.id, "the same entry, rewritten in place");
        assert_eq!(counted.beat.as_deref(), Some("capture"), "still a beat");

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "captured facts: person:milhouse, person:otto (2)",
                "read the task"
            ],
            "two entries, in the same order — a growing tally is not a new beat"
        );

        // The session's own words are not a beat, and this verb will not touch
        // them wherever they sit.
        let err = store
            .amend_beat(&session.id, &mine.id, "read the task properly", at(240))
            .await
            .expect_err("a session's own entry is append-only");
        assert!(matches!(err, SessionError::NotABeat { .. }), "got {err:?}");
        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(
            read.entries[1].text, "read the task",
            "…and it is unchanged"
        );
    }

    /// **A session that is working is not idle.** A beat correction is a write,
    /// so it moves what the sweep measures — but it does NOT move the entry's
    /// place in the chronology, which is where it happened.
    ///
    /// Without this, a session that had used every verb class once looked
    /// motionless: every further call only amended an existing beat, no instant
    /// advanced, and a session working steadily became sweepable while it worked.
    pub async fn amending_a_beat_keeps_its_place_but_moves_the_clock(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        let beat = store
            .append(
                &session.id,
                NewEntry::beat("capture", "captured facts: person:milhouse (1)", at(60)),
            )
            .await
            .expect("append ok");
        journal(store, &session.id, "read the task", 120).await;

        let amended = store
            .amend_beat(
                &session.id,
                &beat.id,
                "captured facts: person:milhouse, person:otto (2)",
                at(600),
            )
            .await
            .expect("amend_beat ok");
        assert_eq!(amended.at, at(60), "the beat keeps when it happened");
        assert_eq!(
            amended.touched,
            Some(at(600)),
            "…and records when it was last corrected"
        );

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "captured facts: person:milhouse, person:otto (2)",
                "read the task"
            ],
            "the correction does not move the beat to the end of the record"
        );
        assert_eq!(
            read.last_beat(),
            at(600),
            "…but the sweep sees the work: a corrected beat is a session still going"
        );
    }

    /// Amending a session with nothing in it is refused, not silently turned
    /// into the first entry.
    pub async fn amending_with_no_entries_is_refused(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        let err = store
            .amend_last(&session.id, "there is nothing to correct")
            .await
            .expect_err("an amend with nothing to amend must not report success");
        assert!(matches!(err, SessionError::NoEntries { .. }), "got {err:?}");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert!(
            read.entries.is_empty(),
            "…and it wrote nothing: {:?}",
            read.entries
        );
    }

    /// **Focus is current truth, rewritten in place** — and rewriting it leaves
    /// the chronology alone. The two answer different questions, and a store
    /// that logged every focus change would answer the second with the first.
    pub async fn focus_is_rewritten_in_place_and_leaves_the_chronology_alone(store: &dyn Sessions) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        journal(store, &session.id, "read the task", 60).await;

        let moved = store
            .set_focus(&session.id, "building the session context")
            .await
            .expect("set_focus ok");
        assert_eq!(moved.focus, "building the session context");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(read.focus, "building the session context", "current truth");
        assert_eq!(
            read.entries.len(),
            1,
            "a focus change is not a chronology entry"
        );
    }

    /// **The two ends stop being the same at exactly one point: reopening.**
    ///
    /// `wrapped` is final. Its story was told, and a chronology that can grow
    /// after the telling makes the telling worthless — that is the whole of the
    /// terminal-both-ways rationale, and it is about this state.
    ///
    /// `abandoned` told no story. It means the run was never wrapped up — a
    /// disconnect, a closed laptop, an agent that moved on — so picking it back
    /// up is ordinary rather than recovery, and the record continues where it
    /// stopped rather than starting again beside it.
    ///
    /// **A reopened session is `active` in the full sense**: it takes entries
    /// again, its chronology is intact and unrewritten, and nothing marks it as
    /// having been away. It also stops being sweepable, which is the point —
    /// somebody is working in it.
    pub async fn an_abandoned_session_reopens_and_a_wrapped_one_never_does(store: &dyn Sessions) {
        let abandoned = begin(store, "gamma", "reading the hand-off", 0).await;
        journal(store, &abandoned.id, "got half way", 60).await;
        store
            .close(&abandoned.id, SessionState::Abandoned)
            .await
            .expect("close ok");

        let reopened = store
            .reopen(&abandoned.id)
            .await
            .expect("an abandoned run reopens");
        assert_eq!(reopened.state, SessionState::Active, "it is running again");
        assert_eq!(
            reopened.entries.len(),
            1,
            "…and the chronology is what it was: {:?}",
            reopened.entries
        );
        assert_eq!(reopened.entries[0].text, "got half way");
        assert_eq!(
            reopened.focus, "reading the hand-off",
            "and so is the focus"
        );

        // The proof that reopening MEANT something: the verb that was refused a
        // moment ago now lands, and lands on the same record.
        store
            .append(&reopened.id, NewEntry::manual("picked it back up", at(120)))
            .await
            .expect("a reopened session takes entries");
        let read = store.read_session(&abandoned.id).await.expect("read ok");
        assert_eq!(
            read.entries.len(),
            2,
            "one record, continued: {:?}",
            read.entries
        );
        assert_eq!(read.state, SessionState::Active);

        // Reopening what is already open is not an error — a caller resuming
        // the run they are already in has made no mistake.
        assert_eq!(
            store.reopen(&abandoned.id).await.expect("idempotent").state,
            SessionState::Active
        );

        // **And the half that does not bend.** A wrapped session told its
        // story; nothing reopens it, and the refusal names which end it reached
        // rather than pretending the id is unknown.
        let wrapped = begin(store, "delta", "chasing the flaky test", 200).await;
        journal(store, &wrapped.id, "found it", 240).await;
        store
            .close(&wrapped.id, SessionState::Wrapped)
            .await
            .expect("close ok");
        let refused = store
            .reopen(&wrapped.id)
            .await
            .expect_err("a wrapped session never reopens");
        assert!(
            matches!(&refused, SessionError::Closed { state, .. } if *state == SessionState::Wrapped),
            "the refusal says the story was already told: {refused:?}"
        );
        let read = store.read_session(&wrapped.id).await.expect("read ok");
        assert_eq!(read.state, SessionState::Wrapped, "and nothing moved");
        assert_eq!(read.entries.len(), 1, "…including its chronology");
    }

    /// **Both ends are terminal both ways.** A closed session takes no entry, no
    /// amend, no focus change and no second close — and it is still readable,
    /// because the record is the point.
    ///
    /// Reopening is the one verb where the two ends part company, and it has its
    /// own case above. Everything here holds for both, unchanged: while a
    /// session is closed it is closed, whichever end it reached.
    pub async fn a_closed_session_is_terminal_both_ways(store: &dyn Sessions) {
        for end in [SessionState::Wrapped, SessionState::Abandoned] {
            let session = begin(store, "gamma", "reading the hand-off", 0).await;
            let beat = store
                .append(
                    &session.id,
                    NewEntry::beat("capture", "captured facts: x (1)", at(30)),
                )
                .await
                .expect("append ok");
            journal(store, &session.id, "read the task", 60).await;
            let closed = store.close(&session.id, end).await.expect("close ok");
            assert_eq!(closed.state, end);

            let refused = |err: SessionError, verb: &str| {
                assert!(
                    matches!(&err, SessionError::Closed { state, .. } if *state == end),
                    "{verb} on a {end} session must be refused as closed: {err:?}"
                );
            };
            refused(
                store
                    .append(&session.id, NewEntry::manual("one more thing", at(120)))
                    .await
                    .expect_err("append must be refused"),
                "append",
            );
            refused(
                store
                    .amend_last(&session.id, "actually")
                    .await
                    .expect_err("amend must be refused"),
                "amend_last",
            );
            refused(
                store
                    .amend_beat(&session.id, &beat.id, "a corrected tally", at(180))
                    .await
                    .expect_err("amending a beat must be refused"),
                "amend_beat",
            );
            refused(
                store
                    .set_focus(&session.id, "something else")
                    .await
                    .expect_err("set_focus must be refused"),
                "set_focus",
            );
            refused(
                store
                    .close(&session.id, SessionState::Active)
                    .await
                    .expect_err("reopening must be refused"),
                "close(active)",
            );
            refused(
                store
                    .close(&session.id, SessionState::Wrapped)
                    .await
                    .expect_err("closing twice must be refused"),
                "close(wrapped)",
            );

            // …and the record stands, readable, with everything it had.
            let read = store.read_session(&session.id).await.expect("read ok");
            assert_eq!(read.state, end, "nothing moved it");
            assert_eq!(read.entries.len(), 2, "and nothing was appended to it");
        }
    }

    /// A bot's sessions come back newest first, and one bot never sees
    /// another's. Attaching reads this list, so a leak here is a session
    /// resuming as the wrong identity.
    pub async fn sessions_are_listed_per_bot_newest_first(store: &dyn Sessions) {
        let older = begin(store, "gamma", "the first run", 0).await;
        let newer = begin(store, "gamma", "the second run", 600).await;
        begin(store, "delta", "somebody else's run", 300).await;

        let mine = store.sessions_of(&bot("gamma")).await.expect("list ok");
        let ids: Vec<&str> = mine.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![newer.id.as_str(), older.id.as_str()],
            "newest first, and only this bot's"
        );

        let theirs = store.sessions_of(&bot("delta")).await.expect("list ok");
        assert_eq!(
            theirs.len(),
            1,
            "boxes of one bot never leak into another's"
        );
        assert_eq!(theirs[0].focus, "somebody else's run");
    }

    /// A closed session stays in the bot's list — closed is an archive, never a
    /// deletion, and the record of what happened is the whole capability.
    pub async fn a_closed_session_stays_on_the_record(store: &dyn Sessions) {
        let session = begin(store, "gamma", "the first run", 0).await;
        journal(store, &session.id, "read the task", 60).await;
        store
            .close(&session.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let listed = store.sessions_of(&bot("gamma")).await.expect("list ok");
        assert_eq!(listed.len(), 1, "nothing is deleted");
        assert_eq!(listed[0].state, SessionState::Wrapped);
        assert_eq!(
            listed[0].entries.len(),
            1,
            "…and its chronology is still readable: {:?}",
            listed[0].entries
        );
    }

    /// An id nothing answers to is a miss — never a create, never a silent
    /// success.
    pub async fn addressing_an_unknown_session_is_a_miss(store: &dyn Sessions) {
        begin(store, "gamma", "the first run", 0).await;
        let missing = SessionId("999999".into());
        for err in [
            store
                .read_session(&missing)
                .await
                .expect_err("read must miss"),
            store
                .append(&missing, NewEntry::manual("a beat", at(60)))
                .await
                .expect_err("append must miss"),
            store
                .amend_last(&missing, "a beat")
                .await
                .expect_err("amend must miss"),
            store
                .set_focus(&missing, "a focus")
                .await
                .expect_err("focus must miss"),
            store
                .close(&missing, SessionState::Wrapped)
                .await
                .expect_err("close must miss"),
        ] {
            assert!(
                matches!(err, SessionError::UnknownSession { .. }),
                "got {err:?}"
            );
        }
    }

    /// Malformed input is refused before anything is written.
    pub async fn malformed_input_is_refused(store: &dyn Sessions) {
        let session = begin(store, "gamma", "the first run", 0).await;
        assert!(
            store
                .append(&session.id, NewEntry::manual("   ", at(60)))
                .await
                .is_err(),
            "an empty entry is not a beat"
        );
        assert!(
            store.set_focus(&session.id, "two\nlines").await.is_err(),
            "a focus is one plain line"
        );
        assert!(
            store
                .begin(NewSession {
                    bot: bot("gamma"),
                    focus: "  ".into(),
                    started_at: at(0),
                })
                .await
                .is_err(),
            "a session with no focus says nothing about what it is doing"
        );

        let read = store.read_session(&session.id).await.expect("read ok");
        assert!(
            read.entries.is_empty(),
            "nothing malformed reached the store"
        );
        assert_eq!(read.focus, "the first run");
    }

    /// A multi-line entry survives the round trip, and CRLF normalizes — the
    /// same contract a message body carries, for the same reason.
    pub async fn an_entry_survives_the_round_trip(store: &dyn Sessions) {
        let session = begin(store, "gamma", "the first run", 0).await;
        let text = "found the cause\n\nthe rollback wrote a description it never wrote";
        journal(store, &session.id, text, 60).await;
        store
            .append(
                &session.id,
                NewEntry::manual("line one\r\nline two", at(120)),
            )
            .await
            .expect("append ok");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(read.entries[0].text, text, "paragraphs survive verbatim");
        assert_eq!(
            read.entries[1].text, "line one\nline two",
            "CRLF normalizes"
        );
    }

    /// The whole spec, against one store. Each case runs on a **fresh** store,
    /// so nothing here depends on the order the others ran in.
    pub async fn run_all<S: Sessions, F: Fn() -> S>(fresh: F) {
        a_begun_session_is_active_and_empty(&fresh()).await;
        the_chronology_accrues_oldest_first(&fresh()).await;
        a_beat_is_stored_beside_manual_entries_and_stays_distinguishable(&fresh()).await;
        an_amend_rewrites_the_last_entry_and_nothing_else(&fresh()).await;
        only_an_automatic_beat_is_amended_in_place(&fresh()).await;
        amending_a_beat_keeps_its_place_but_moves_the_clock(&fresh()).await;
        amending_with_no_entries_is_refused(&fresh()).await;
        focus_is_rewritten_in_place_and_leaves_the_chronology_alone(&fresh()).await;
        a_closed_session_is_terminal_both_ways(&fresh()).await;
        an_abandoned_session_reopens_and_a_wrapped_one_never_does(&fresh()).await;
        sessions_are_listed_per_bot_newest_first(&fresh()).await;
        a_closed_session_stays_on_the_record(&fresh()).await;
        addressing_an_unknown_session_is_a_miss(&fresh()).await;
        malformed_input_is_refused(&fresh()).await;
        an_entry_survives_the_round_trip(&fresh()).await;
    }
}
