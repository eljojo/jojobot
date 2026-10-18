//! The Sessions contract, and the in-memory fake that must satisfy it.
//!
//! One behavioural spec, two tiers — the fake here, and the real adapter
//! against the real store it runs, a disposable database this process spawns
//! itself — and **the spec is the same code on both**, which is what stops the
//! fake from drifting into a store that agrees with the tests and disagrees
//! with reality.
//!
//! Behind the `testing` feature, so it compiles for tests here and in downstream
//! crates but never ships in a production binary.

use std::sync::Mutex;

use jiff::Timestamp;
use jiff::civil::Date;

use super::history::{
    Change, SessionChange, SessionHistories, Wording, WriteStamp, next_change, next_wording,
};
use super::{
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId,
    SessionState, Sessions, Sid, WRAP_CODE_PREFIX, WrapWindow, is_readable_sid, normalize_entry,
    validate_entry, validate_focus, validate_session_id,
};
use crate::memory::EntityId;

/// The in-memory [`Sessions`] fake — a real store that holds a write, with no
/// network. Deterministic: ids are a monotonic counter, never a clock.
#[derive(Default)]
pub struct InMemorySessions {
    sessions: Mutex<Vec<Session>>,
    next_id: Mutex<u64>,
    charter_changes: Mutex<std::collections::BTreeMap<EntityId, CharterChange>>,
    /// Every change of every run, in the order they were made.
    changes: Mutex<Vec<(SessionId, SessionChange)>>,
    /// Every wording of every entry, in the order they were made.
    wordings: Mutex<Vec<(SessionId, EntryId, Wording)>>,
    /// The seconds the last record was stamped at. **The fake stamps by counting,
    /// never by a clock**: a case reads the same on every run, and every record
    /// is later than the one before it.
    ticks: Mutex<i64>,
    /// The moment a layer above holds for the write in flight, which this
    /// stamps with instead of counting.
    held: WriteStamp,
    /// Set when this store keeps no list of its charter changes to hand over,
    /// as a port that leaves [`Sessions::charter_changes`] at its default.
    no_charter_list: bool,
    minted: Mutex<Vec<super::MintedRun>>,
    /// The ledger of request ids this store spends into, in the same step as
    /// the write. Set once by [`InMemorySessions::attach_requests`].
    requests: Mutex<Option<std::sync::Arc<crate::request::testing::InMemoryRequests>>>,
    /// How many calls each key has been counted under.
    calls: Mutex<std::collections::BTreeMap<crate::usage::CallKey, u64>>,
    /// What each run was served on each day.
    served_days: Mutex<std::collections::BTreeMap<(SessionId, jiff::civil::Date), u64>>,
    /// Set while the store is to refuse every count, as a store that cannot be
    /// reached does.
    refusing_counts: std::sync::atomic::AtomicBool,
}

impl InMemorySessions {
    /// **Refuse every count of a call from now on, or take them again.** A
    /// fixture, for the case that a count which cannot be kept must not fail the
    /// call it counts.
    pub fn refuse_counts(&self, refuse: bool) {
        self.refusing_counts
            .store(refuse, std::sync::atomic::Ordering::SeqCst);
    }

    /// **Spend request ids into `ledger` from now on.** See the field.
    pub fn attach_requests(
        &self,
        ledger: std::sync::Arc<crate::request::testing::InMemoryRequests>,
    ) {
        *self.requests.lock().expect("requests lock") = Some(ledger);
    }

    /// **Claim the id of the call in flight**, when it carries one and this write
    /// is a part of the verb's own: a write that finds its claim refused stores
    /// nothing.
    fn spend_request(&self, landed: impl FnOnce() -> String) -> Result<(), SessionError> {
        let Some(claim) = crate::request::take() else {
            return Ok(());
        };
        match self.requests.lock().expect("requests lock").as_ref() {
            Some(ledger) => {
                if ledger.spend(claim, landed()) {
                    Ok(())
                } else {
                    Err(SessionError::Conflict)
                }
            }
            None => {
                claim.failed();
                Ok(())
            }
        }
    }

    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// **A store that keeps no list of its charter changes**, as a port that does
    /// not override [`Sessions::charter_changes`]: the list answers `None`, and a
    /// bot's change is asked of [`Sessions::charter_change_of`] one bot at a time.
    pub fn without_a_charter_list(mut self) -> Self {
        self.no_charter_list = true;
        self
    }

    /// **Strip a session's stored handle** — the shape of a card written by an
    /// older jojobot, before handles were persisted.
    ///
    /// A fixture, not a verb: no port method does this, and none should. It
    /// exists so the migration path can be tested rather than assumed, because
    /// "old cards still work" is exactly the claim nobody notices breaking.
    pub fn forget_sid(&self, id: &SessionId) {
        if let Some(session) = self
            .sessions
            .lock()
            .expect("session lock")
            .iter_mut()
            .find(|s| &s.id == id)
        {
            session.sid = None;
        }
    }

    /// **The moment a layer above holds for the write in flight**, shared with
    /// it: hand this to the layer, and the records the fake makes carry the
    /// moment the layer took.
    pub fn write_stamp(&self) -> WriteStamp {
        self.held.clone()
    }

    /// The moment of the next record: the one a layer above holds for the write
    /// in flight, else one second after the last.
    fn stamp(&self) -> Timestamp {
        if let Some(held) = self.held.current() {
            return held;
        }
        let mut ticks = self.ticks.lock().expect("tick lock");
        *ticks += 1;
        Timestamp::from_second(1_790_000_000 + *ticks).expect("a valid instant")
    }

    /// Keep one change of a run, made by [`next_change`].
    fn record(&self, id: &SessionId, change: Change<'_>) {
        let at = self.stamp();
        let mut changes = self.changes.lock().expect("change lock");
        let made = next_change(
            changes
                .iter()
                .filter(|(held, _)| held == id)
                .map(|(_, c)| c.ordinal)
                .max(),
            change,
            at,
        );
        changes.push((id.clone(), made));
    }

    /// Keep one wording of an entry, made by [`next_wording`].
    fn keep_wording(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: Option<Timestamp>,
    ) {
        let at = self.stamp();
        let mut wordings = self.wordings.lock().expect("wording lock");
        let made = next_wording(
            // The wordings are pushed oldest first, so the newest is the highest.
            wordings
                .iter()
                .rfind(|(run, held, _)| run == id && held == entry)
                .map(|(_, _, w)| w.revision),
            text,
            touched,
            at,
        );
        wordings.push((id.clone(), entry.clone(), made));
    }

    /// **An id of the shape the real store draws**: six characters of the
    /// drawn alphabet, here counted rather than drawn so a case reads the same
    /// on every run. A card's id is also a run's public name when the card was
    /// born without one, so it has to be a name that passes the shape check.
    fn mint(&self) -> String {
        let mut next = self.next_id.lock().expect("id lock");
        *next += 1;
        let alphabet = crate::handle::ALPHABET;
        let mut left = *next as usize;
        let mut out = [alphabet[0]; super::RUN_LEN];
        for slot in out.iter_mut().rev() {
            *slot = alphabet[left % alphabet.len()];
            left /= alphabet.len();
        }
        String::from_utf8(out.to_vec()).expect("the alphabet is ascii")
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
        if !sessions[at].takes_writes() {
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

    async fn count_call(&self, key: &crate::usage::CallKey) -> Result<(), SessionError> {
        if self
            .refusing_counts
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(SessionError::Store("the counts cannot be kept".to_string()));
        }
        *self
            .calls
            .lock()
            .expect("call lock")
            .entry(key.clone())
            .or_default() += 1;
        Ok(())
    }

    async fn call_counts(&self) -> Result<Vec<crate::usage::CallCount>, SessionError> {
        Ok(self
            .calls
            .lock()
            .expect("call lock")
            .iter()
            .map(|(key, count)| crate::usage::CallCount {
                key: key.clone(),
                count: *count,
            })
            .collect())
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        let mut all = self.sessions.lock().expect("session lock").clone();
        // The real store's order: newest start first, the id breaking a tie.
        all.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(all)
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
        // **A sid is minted, never written by hand.** The same refusal shape
        // a bad focus already gets: nothing is written, and the caller is
        // told why before anything reaches the store. Mirrors the Dolt
        // adapter's check, so the contract holds identically on both stores.
        if !is_readable_sid(new.sid.as_str()) {
            return Err(SessionError::InvalidEntry(format!(
                "sid '{}' is not a handle this build ever mints — a sid is {} characters drawn \
                 from its own alphabet, never one a caller writes",
                new.sid.as_str(),
                super::SID_LEN,
            )));
        }
        // **One handle, one run.** A caller retrying a `begin` whose write
        // committed before its read-back failed offers the same handle again;
        // appending unconditionally would fork the run. See the contract case.
        if let Some(held) = self
            .sessions
            .lock()
            .expect("session lock")
            .iter()
            .find(|s| s.sid.as_ref() == Some(&new.sid) && !s.state.is_terminal())
        {
            return Ok(held.clone());
        }
        // **A name the boot minted is the card's id**, and a card that already
        // wears that id is not a retry of this run: the id is refused rather
        // than shared, as the real store refuses a duplicate key.
        let id = match new.id {
            Some(id) => {
                if self
                    .sessions
                    .lock()
                    .expect("session lock")
                    .iter()
                    .any(|s| s.id == id)
                {
                    return Err(SessionError::InvalidEntry(format!(
                        "session '{id}' already exists, so it cannot begin again"
                    )));
                }
                id
            }
            None => SessionId(self.mint()),
        };
        let session = Session {
            id,
            sid: Some(new.sid),
            key_hash: new.key_hash,
            bot: new.bot,
            focus: new.focus.trim().to_string(),
            started_at: new.started_at,
            state: SessionState::Active,
            entries: Vec::new(),
            timezone: new.timezone,
            started_on: new.started_on,
            served_chars: 0,
            stated_day: None,
            wrap_window: None,
            charter_told_at: None,
            key_trail: Default::default(),
        };
        self.sessions
            .lock()
            .expect("session lock")
            .push(session.clone());
        self.record(&session.id, Change::Begin(&session.focus));
        Ok(session)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(&entry.text)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        let minted = EntryId(self.mint());
        self.spend_request(|| {
            format!(
                "{{\"entry\":{}}}",
                crate::request::json_text(minted.as_str())
            )
        })?;
        let recorded = JournalEntry {
            id: minted,
            at: entry.at,
            on: entry.on,
            text: normalize_entry(&entry.text),
            touched: None,
            beat: entry.beat,
            closing_focus: entry.closing_focus,
            closing: entry.closing,
        };
        sessions[at].entries.push(recorded.clone());
        self.record(id, Change::Entry(&recorded.id));
        self.keep_wording(id, &recorded.id, &recorded.text, None);
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
        let kept = last.clone();
        self.record(id, Change::AmendLast(&kept.id));
        self.keep_wording(id, &kept.id, &kept.text, kept.touched);
        Ok(kept)
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
        let kept = held.clone();
        self.record(id, Change::AmendBeat(&kept.id));
        self.keep_wording(id, &kept.id, &kept.text, kept.touched);
        Ok(kept)
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        validate_focus(focus)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        self.spend_request(|| {
            format!("{{\"focus\":{}}}", crate::request::json_text(focus.trim()))
        })?;
        sessions[at].focus = focus.trim().to_string();
        self.record(id, Change::Focus(&sessions[at].focus));
        Ok(sessions[at].clone())
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        sessions[at].timezone = timezone
            .map(str::trim)
            .filter(|z| !z.is_empty())
            .map(str::to_string);
        self.record(id, Change::Timezone(sessions[at].timezone.as_deref()));
        Ok(sessions[at].clone())
    }

    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        sessions[at].stated_day = day;
        self.record(id, Change::StatedDay(day));
        Ok(sessions[at].clone())
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        // **Not `writable`**: this is the write that is about a closed run.
        let at = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        if sessions[at].state != SessionState::Wrapped {
            return Err(SessionError::NotWrapped {
                attempted: id.to_string(),
                state: sessions[at].state,
            });
        }
        sessions[at].wrap_window = window;
        self.record(id, Change::WrapWindow(sessions[at].wrap_window.as_ref()));
        Ok(sessions[at].clone())
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = Self::writable(&mut sessions, id)?;
        // **A run that is already wrapped, with its window open, can only be
        // wrapped again** — that close is what ends the window. It is never
        // taken back to any other state.
        if sessions[at].state.is_terminal() && to != SessionState::Wrapped {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: sessions[at].state,
            });
        }
        let had_window = sessions[at].wrap_window.is_some();
        sessions[at].state = to;
        sessions[at].wrap_window = None;
        self.record(id, Change::State(to));
        if had_window {
            self.record(id, Change::WrapWindow(None));
        }
        Ok(sessions[at].clone())
    }

    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        // **Existence only, deliberately not `writable`**: this never refuses
        // on a closed session — see the trait's own doc.
        let at = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        sessions[at].served_chars += chars;
        *self
            .served_days
            .lock()
            .expect("served lock")
            .entry((id.clone(), day))
            .or_default() += chars;
        Ok(())
    }

    async fn served_days(&self) -> Result<Vec<crate::usage::ServedDay>, SessionError> {
        let sessions = self.sessions.lock().expect("session lock");
        Ok(self
            .served_days
            .lock()
            .expect("served lock")
            .iter()
            .filter_map(|((run, day), chars)| {
                let bot = sessions.iter().find(|s| &s.id == run)?.bot.clone();
                Some(crate::usage::ServedDay {
                    run: run.clone(),
                    bot,
                    day: *day,
                    chars: *chars,
                })
            })
            .collect())
    }

    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        self.spend_request(|| format!("{{\"reset\":{}}}", sessions[at].served_chars))?;
        let total = std::mem::take(&mut sessions[at].served_chars);
        self.record(id, Change::ServedReset(total));
        Ok(())
    }

    async fn scan_session_history(&self) -> Result<SessionHistories, SessionError> {
        let changes = self.changes.lock().expect("change lock");
        let mut runs: Vec<&SessionId> = changes.iter().map(|(id, _)| id).collect();
        runs.sort();
        runs.dedup();
        let wordings = self.wordings.lock().expect("wording lock");
        let mut entries: Vec<(&SessionId, &EntryId)> = wordings
            .iter()
            .map(|(run, entry, _)| (run, entry))
            .collect();
        entries.sort();
        entries.dedup();
        Ok(SessionHistories {
            changes: runs
                .into_iter()
                .map(|run| {
                    let mut kept: Vec<SessionChange> = changes
                        .iter()
                        .filter(|(held, _)| held == run)
                        .map(|(_, change)| change.clone())
                        .collect();
                    kept.sort_by_key(|change| change.ordinal);
                    (run.clone(), kept)
                })
                .collect(),
            wordings: entries
                .into_iter()
                .map(|(run, entry)| {
                    let mut kept: Vec<Wording> = wordings
                        .iter()
                        .filter(|(held, e, _)| held == run && e == entry)
                        .map(|(_, _, wording)| wording.clone())
                        .collect();
                    kept.sort_by_key(|wording| wording.revision);
                    (run.clone(), entry.clone(), kept)
                })
                .collect(),
        })
    }

    async fn session_history(&self, id: &SessionId) -> Result<Vec<SessionChange>, SessionError> {
        validate_session_id(id)?;
        self.read_session(id).await?;
        let mut kept: Vec<SessionChange> = self
            .changes
            .lock()
            .expect("change lock")
            .iter()
            .filter(|(held, _)| held == id)
            .map(|(_, change)| change.clone())
            .collect();
        kept.sort_by_key(|change| change.ordinal);
        Ok(kept)
    }

    async fn journal_history(
        &self,
        id: &SessionId,
        entry: &EntryId,
    ) -> Result<Vec<Wording>, SessionError> {
        validate_session_id(id)?;
        let session = self.read_session(id).await?;
        if !session.entries.iter().any(|e| &e.id == entry) {
            return Err(SessionError::NoEntries {
                attempted: id.to_string(),
            });
        }
        let mut kept: Vec<Wording> = self
            .wordings
            .lock()
            .expect("wording lock")
            .iter()
            .filter(|(run, held, _)| run == id && held == entry)
            .map(|(_, _, wording)| wording.clone())
            .collect();
        kept.sort_by_key(|wording| wording.revision);
        Ok(kept)
    }

    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &super::KeyTrail,
    ) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        let at_index = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        sessions[at_index].key_hash = Some(hash.to_string());
        self.record(id, Change::Key);
        sessions[at_index].key_trail = trail.clone();
        Ok(())
    }

    async fn note_minted(&self, run: &super::MintedRun) -> Result<(), SessionError> {
        let mut minted = self.minted.lock().expect("minted lock");
        // **Written once**: a second note of one name changes nothing.
        if !minted.iter().any(|held| held.name == run.name) {
            minted.push(run.clone());
        }
        Ok(())
    }

    async fn minted_without_a_card(&self) -> Result<Vec<super::MintedRun>, SessionError> {
        let sessions = self.sessions.lock().expect("session lock");
        let mut found: Vec<super::MintedRun> = self
            .minted
            .lock()
            .expect("minted lock")
            .iter()
            .filter(|run| {
                !sessions
                    .iter()
                    .any(|card| card.id.as_str() == run.name.as_str())
            })
            .cloned()
            .collect();
        found.sort_by_key(|run| run.minted_at);
        Ok(found)
    }

    async fn prune_minted(&self, before: Timestamp) -> Result<usize, SessionError> {
        let sessions = self.sessions.lock().expect("session lock");
        let mut minted = self.minted.lock().expect("minted lock");
        let held = minted.len();
        minted.retain(|run| {
            let has_card = sessions
                .iter()
                .any(|card| card.id.as_str() == run.name.as_str());
            !has_card && run.minted_at >= before
        });
        Ok(held - minted.len())
    }

    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut sessions = self.sessions.lock().expect("session lock");
        // Existence only, never refused on a closed session: see the trait.
        let at_index = sessions.iter().position(|s| &s.id == id).ok_or_else(|| {
            SessionError::UnknownSession {
                attempted: id.to_string(),
            }
        })?;
        sessions[at_index].charter_told_at = Some(at);
        self.record(id, Change::CharterTold(at));
        Ok(())
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        _by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.spend_request(|| format!("{{\"bot\":\"{bot}\"}}"))?;
        if change.by != *bot {
            self.charter_changes
                .lock()
                .expect("charter change lock")
                .insert(bot.clone(), change);
        }
        Ok(())
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        Ok(self
            .charter_changes
            .lock()
            .expect("charter change lock")
            .get(bot)
            .cloned())
    }

    async fn charter_changes(
        &self,
    ) -> Result<Option<std::collections::BTreeMap<EntityId, CharterChange>>, SessionError> {
        if self.no_charter_list {
            return Ok(None);
        }
        Ok(Some(
            self.charter_changes
                .lock()
                .expect("charter change lock")
                .clone(),
        ))
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
        // A run already open is resumed, which changes nothing and records
        // nothing.
        let was = sessions[at].state;
        sessions[at].state = SessionState::Active;
        if was != SessionState::Active {
            self.record(id, Change::State(SessionState::Active));
        }
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

    /// A deterministic handle for the contract's own sessions — the spec has to
    /// be reproducible, so it never draws entropy. Real handles are drawn; these
    /// only have to be distinct and shaped like one.
    fn sid(nth: i64) -> Sid {
        let handle = format!("s{:03}", nth.rem_euclid(1000));
        assert!(
            is_readable_sid(&handle),
            "the fixture handle {handle} is not readable"
        );
        Sid(handle)
    }

    /// Begin a session, asserting the store took it.
    pub async fn begin(store: &dyn Sessions, slug: &str, focus: &str, at_offset: i64) -> Session {
        store
            .begin(NewSession {
                bot: bot(slug),
                sid: sid(at_offset),
                focus: focus.to_string(),
                started_at: at(at_offset),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
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
            .append(id, NewEntry::manual(text, at(at_offset), None))
            .await
            .expect("append should succeed")
    }

    /// **A handle appears on at most one run, and `begin` is what has to hold
    /// that.**
    ///
    /// A store commits its write and then reads it back. If the write LANDS and
    /// the read-back fails — a dropped response, a transient fault on the
    /// second call — `begin` returns `Err` with the row already committed, and
    /// the caller still holds the handle it was going to attach. It retries.
    /// A `begin` that appends unconditionally then puts a second run under one
    /// handle, and the identity that the whole trace hangs from stops naming
    /// one thing.
    ///
    /// So beginning again under a handle a run already carries hands that run
    /// back rather than making another. The retry finishes what the first
    /// attempt started — the same shape `wrap_session` uses for its own retry.
    pub async fn beginning_twice_under_one_handle_yields_one_run(store: &dyn Sessions) {
        let first = begin(store, "gamma", "reading the hand-off", 0).await;

        // The same handle, offered again — as a caller retrying a `begin` whose
        // read-back failed after the write committed would offer it.
        let again = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(0),
                focus: "reading the hand-off".to_string(),
                started_at: at(0),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("beginning again under a live handle is not an error");

        assert_eq!(
            again.id, first.id,
            "the run already carrying this handle comes back; a second one is a fork"
        );
        let mine: Vec<Session> = store
            .sessions_of(&bot("gamma"))
            .await
            .expect("read ok")
            .into_iter()
            .filter(|s| s.sid == Some(sid(0)))
            .collect();
        assert_eq!(
            mine.len(),
            1,
            "one handle, one run — found {}: {mine:?}",
            mine.len()
        );
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
                NewEntry::beat("capture", "captured facts: person:milhouse", at(90), None),
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
                NewEntry::beat("capture", "captured facts: person:milhouse", at(60), None),
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

    /// **The kind check is position-blind, proved rather than inferred.**
    ///
    /// The case above protects a manual entry that happens to be the newest
    /// one — so it cannot tell "refused because it is not a beat" from
    /// "refused because it is not the newest". Here neither the beat nor the
    /// manual entry being tested is last: a third entry sits after both. The
    /// beat succeeds and the manual entry is refused, at comparable
    /// positions, so what decides it is the kind, not the age.
    pub async fn an_older_manual_entry_is_refused_by_address_and_a_beat_at_the_same_age_is_not(
        store: &dyn Sessions,
    ) {
        let session = begin(store, "gamma", "reading the hand-off", 0).await;
        let beat = store
            .append(
                &session.id,
                NewEntry::beat("capture", "captured facts: person:milhouse", at(60), None),
            )
            .await
            .expect("append ok");
        let older = journal(store, &session.id, "read the task", 120).await;
        // Neither entry above is newest once this lands.
        journal(store, &session.id, "wrote the domain module", 180).await;

        // The beat, no more recent than the manual entry beside it, is still
        // reached and rewritten in place.
        let counted = store
            .amend_beat(
                &session.id,
                &beat.id,
                "captured facts: person:milhouse, person:otto (2)",
                at(240),
            )
            .await
            .expect("amend_beat ok on a beat that is not the newest entry");
        assert_eq!(counted.id, beat.id, "the same entry, rewritten in place");

        // The older manual entry, at a comparable age, is refused all the same.
        let err = store
            .amend_beat(&session.id, &older.id, "read the task properly", at(300))
            .await
            .expect_err("an older manual entry is append-only, exactly as the newest one is");
        assert!(matches!(err, SessionError::NotABeat { .. }), "got {err:?}");

        let read = store.read_session(&session.id).await.expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "captured facts: person:milhouse, person:otto (2)",
                "read the task",
                "wrote the domain module",
            ],
            "the beat's count moved, the older manual entry did not, and nothing was appended \
             or reordered"
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
                NewEntry::beat(
                    "capture",
                    "captured facts: person:milhouse (1)",
                    at(60),
                    None,
                ),
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

    /// **A run's zone survives storage, and the door can move it.**
    ///
    /// A contract case because only a store can say whether the name came back
    /// as it went in. The handle registry is rebuilt from these rows at
    /// startup, so a zone the store dropped would be a run that answered in the
    /// caller's frame until the process restarted and then silently in the
    /// fallback.
    ///
    /// Three beats, and the third is the one the other two rest on: a run born
    /// with no zone carries none, so this cannot pass on a store that hands
    /// back a zone whatever it was given.
    /// **A session handed several answers totals them, and an untouched
    /// session reads zero — in the same read** (rule 264).
    ///
    /// Both halves matter and neither alone would catch a broken build:
    /// a counter always reporting zero would still pass the untouched case,
    /// and a counter that ignored `add_served` entirely would still pass a
    /// case that never checked a positive total.
    pub async fn served_characters_accrue_and_an_untouched_session_reads_zero(
        store: &dyn Sessions,
    ) {
        let handed = begin(store, "milhouse", "answering things", 90).await;
        let untouched = begin(store, "bart", "not yet asked anything", 91).await;

        store
            .add_served(&handed.id, 120, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        store
            .add_served(&handed.id, 340, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");

        let read_handed = store.read_session(&handed.id).await.expect("read ok");
        let read_untouched = store.read_session(&untouched.id).await.expect("read ok");

        assert_eq!(
            read_handed.served_chars, 460,
            "several answers total: {read_handed:?}"
        );
        assert_eq!(
            read_untouched.served_chars, 0,
            "an untouched session reads zero: {read_untouched:?}"
        );
    }

    /// **A run's totals carry what it has been served and each compaction it
    /// reported**, dated by the store, whosever the run is. A run that never
    /// compacted has none, and a compaction sets the running total back without
    /// losing the total it set back.
    pub async fn run_totals_carry_the_served_total_and_every_compaction(store: &dyn Sessions) {
        let compacted = begin(store, "homer", "answering things", 95).await;
        let untouched = begin(store, "marge", "not yet asked anything", 96).await;
        store
            .add_served(&compacted.id, 120, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        store
            .reset_served(&compacted.id)
            .await
            .expect("reset_served ok");
        store
            .add_served(&compacted.id, 30, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        store
            .add_served(&compacted.id, 5, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        store
            .reset_served(&compacted.id)
            .await
            .expect("reset_served ok");
        store
            .add_served(&compacted.id, 7, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");

        let totals = store.run_totals().await.expect("run_totals ok");
        let of = |slug: &str| -> &crate::session::RunTotals {
            totals
                .iter()
                .find(|run| run.bot == bot(slug))
                .unwrap_or_else(|| panic!("no run of {slug}: {totals:?}"))
        };
        let homer = of("homer");
        assert_eq!(homer.served_chars, 7, "{homer:?}");
        let set_back: Vec<u64> = homer.resets.iter().map(|reset| reset.total).collect();
        assert_eq!(set_back, vec![120, 35], "each compaction keeps its total");
        assert!(
            homer.resets[0].at < homer.resets[1].at,
            "oldest first, each dated: {homer:?}"
        );
        assert!(homer.dated_from.is_some(), "{homer:?}");
        assert_eq!(homer.started_at, at(95), "{homer:?}");
        let marge = of("marge");
        assert_eq!(
            (marge.served_chars, marge.resets.len()),
            (0, 0),
            "{marge:?}"
        );
        assert_eq!(
            untouched.id,
            store.read_session(&untouched.id).await.expect("read ok").id
        );
    }

    /// **Served characters are counted on the day they were served, in the same
    /// write as the run's total**: the days of a run add up to what it has been
    /// handed over its life, a compaction sets the running total back and takes
    /// no day away, a day is kept apart from the next and one run apart from
    /// another, and a run that does not exist counts none.
    pub async fn served_characters_are_counted_on_the_day_they_were_served(store: &dyn Sessions) {
        let day = |text: &str| -> jiff::civil::Date { text.parse().expect("a day") };
        let homer = begin(store, "homer", "answering things", 97).await;
        let marge = begin(store, "marge", "answering fewer things", 98).await;
        for (run, chars, on) in [
            (&homer, 120, "2026-10-11"),
            (&homer, 30, "2026-10-11"),
            (&homer, 7, "2026-10-12"),
            (&marge, 5, "2026-10-12"),
        ] {
            store
                .add_served(&run.id, chars, day(on))
                .await
                .expect("add_served ok");
        }
        store
            .reset_served(&homer.id)
            .await
            .expect("reset_served ok");
        store
            .add_served(&homer.id, 2, day("2026-10-12"))
            .await
            .expect("add_served ok");
        let held = store.served_days().await.expect("served_days ok");
        let of = |who: &crate::session::Session, on: &str| -> u64 {
            held.iter()
                .filter(|entry| entry.run == who.id && entry.day == day(on))
                .map(|entry| entry.chars)
                .sum()
        };
        assert_eq!(of(&homer, "2026-10-11"), 150, "{held:?}");
        assert_eq!(
            of(&homer, "2026-10-12"),
            9,
            "a compaction takes no day away: {held:?}"
        );
        assert_eq!(of(&marge, "2026-10-12"), 5, "{held:?}");
        assert_eq!(of(&marge, "2026-10-11"), 0, "{held:?}");
        assert!(
            held.iter()
                .filter(|entry| entry.run == homer.id)
                .all(|entry| entry.bot == bot("homer")),
            "each count names the bot of its run: {held:?}"
        );
        assert_eq!(held.len(), 3, "one entry per run and day: {held:?}");
        let missing = store
            .add_served(&SessionId("zzzzzz".into()), 5, day("2026-10-12"))
            .await;
        assert!(missing.is_err(), "a run that does not exist counts none");
        assert_eq!(
            store.served_days().await.expect("served_days ok").len(),
            3,
            "the refused add left no day behind"
        );
    }

    /// **Calls are counted per key and every count is kept**: two calls under
    /// one key are one entry of two, a different verb, day, outcome or word is
    /// its own entry, and a call that carried no session is kept apart from the
    /// calls of every bot.
    pub async fn call_counts_add_up_per_key_and_keep_a_call_with_no_bot_apart(
        store: &dyn Sessions,
    ) {
        use crate::usage::{CallCount, CallKey, CallOutcome};
        let day = |text: &str| -> jiff::civil::Date { text.parse().expect("a day") };
        let key = |who: Option<&str>, on: &str, verb: &str, outcome: CallOutcome| CallKey {
            bot: who.map(bot),
            day: day(on),
            verb: verb.to_string(),
            outcome,
        };
        let change = CallOutcome::Refused {
            fix_by: Some("change".to_string()),
        };
        let counted = [
            key(Some("homer"), "2026-10-14", "recall", CallOutcome::Landed),
            key(Some("homer"), "2026-10-14", "recall", CallOutcome::Landed),
            key(Some("homer"), "2026-10-14", "recall", change.clone()),
            key(Some("homer"), "2026-10-15", "recall", CallOutcome::Landed),
            key(Some("homer"), "2026-10-14", "capture", CallOutcome::Landed),
            key(Some("marge"), "2026-10-14", "recall", CallOutcome::Landed),
            key(None, "2026-10-14", "start_here", CallOutcome::Landed),
            key(None, "2026-10-14", "start_here", CallOutcome::Landed),
            key(
                Some("homer"),
                "2026-10-14",
                "capture",
                CallOutcome::Refused { fix_by: None },
            ),
        ];
        for each in &counted {
            store.count_call(each).await.expect("count_call ok");
        }
        let held = store.call_counts().await.expect("call_counts ok");
        let of = |who: Option<&str>, on: &str, verb: &str, outcome: CallOutcome| -> u64 {
            let wanted = key(who, on, verb, outcome);
            held.iter()
                .filter(|entry| entry.key == wanted)
                .map(|entry| entry.count)
                .sum()
        };
        assert_eq!(
            of(Some("homer"), "2026-10-14", "recall", CallOutcome::Landed),
            2
        );
        assert_eq!(of(Some("homer"), "2026-10-14", "recall", change), 1);
        assert_eq!(
            of(Some("homer"), "2026-10-15", "recall", CallOutcome::Landed),
            1
        );
        assert_eq!(
            of(Some("homer"), "2026-10-14", "capture", CallOutcome::Landed),
            1
        );
        assert_eq!(
            of(
                Some("homer"),
                "2026-10-14",
                "capture",
                CallOutcome::Refused { fix_by: None }
            ),
            1,
            "a refusal with no word is kept as one"
        );
        assert_eq!(
            of(Some("marge"), "2026-10-14", "recall", CallOutcome::Landed),
            1
        );
        assert_eq!(of(None, "2026-10-14", "start_here", CallOutcome::Landed), 2);
        let entries: Vec<&CallCount> = held.iter().collect();
        assert_eq!(entries.len(), 7, "one entry per key: {held:?}");
        assert_eq!(
            held.iter().map(|entry| entry.count).sum::<u64>(),
            counted.len() as u64,
            "no call is lost: {held:?}"
        );
    }

    pub async fn a_runs_zone_is_stored_and_can_be_moved(store: &dyn Sessions) {
        let born = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(70),
                focus: "reading the hand-off".to_string(),
                started_at: at(70),
                timezone: Some("Europe/Madrid".to_string()),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run may be born in a zone");
        assert_eq!(
            store
                .read_session(&born.id)
                .await
                .expect("it reads back")
                .timezone
                .as_deref(),
            Some("Europe/Madrid"),
            "the name comes back as it went in",
        );

        // **The device hop.** The same run, picked up somewhere else.
        let moved = store
            .set_timezone(&born.id, Some("America/New_York"))
            .await
            .expect("a run may be moved into another zone");
        assert_eq!(moved.timezone.as_deref(), Some("America/New_York"));
        assert_eq!(
            store
                .read_session(&born.id)
                .await
                .expect("it reads back")
                .timezone
                .as_deref(),
            Some("America/New_York"),
            "…and the card says so, not just the answer to the write",
        );

        // A run that named none carries none. Without this the beats above pass
        // on a store that invents a zone for every row.
        let bare = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(71),
                focus: "no zone named".to_string(),
                started_at: at(71),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run may name no zone");
        assert_eq!(
            store
                .read_session(&bare.id)
                .await
                .expect("it reads back")
                .timezone,
            None,
            "a run that named no zone is not handed one",
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
            .append(
                &reopened.id,
                NewEntry::manual("picked it back up", at(120), None),
            )
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
                    NewEntry::beat("capture", "captured facts: x (1)", at(30), None),
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
                    .append(
                        &session.id,
                        NewEntry::manual("one more thing", at(120), None),
                    )
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

    /// **The whole board, across bots and across states** — and it carries the
    /// handles.
    ///
    /// This is the read a restart rebuilds its handle registry from: it runs
    /// once at startup, and a card missing from it is a run whose address can
    /// never be recovered, which is the exact cliff persisting the handle was
    /// meant to close. It sat outside this contract entirely, so no store — fake
    /// or real — was ever asked whether it does what the restart path needs.
    ///
    /// Pinned by the two things that distinguish it from `sessions_of`: it does
    /// not narrow to one bot, and it does not drop what is closed. Found rather
    /// than counted, because a real board may carry cards this case never wrote.
    pub async fn all_sessions_carries_every_card_across_bots_with_its_handle(store: &dyn Sessions) {
        let mine = begin(store, "gamma", "the run that is still going", 0).await;
        let theirs = begin(store, "delta", "somebody else's run", 300).await;
        let ended = begin(store, "gamma", "the run that ended", 600).await;
        store
            .close(&ended.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let board = store.all_sessions().await.expect("read ok");
        for expected in [&mine, &theirs, &ended] {
            let found = board
                .iter()
                .find(|s| s.id == expected.id)
                .unwrap_or_else(|| panic!("{} is missing from the board: {board:?}", expected.id));
            assert_eq!(
                found.sid, expected.sid,
                "a card without its handle rebuilds into nothing: {found:?}"
            );
        }
        let closed = board
            .iter()
            .find(|s| s.id == ended.id)
            .expect("found above");
        assert_eq!(
            closed.state,
            SessionState::Wrapped,
            "a closed card is still on the board — this is not sessions_of's live view"
        );
    }

    /// **One run is summarised without its chronology, and the runs of each bot
    /// are counted without reading any of them.** The summary agrees with the one
    /// the bot's own listing gives for the same run, so the cheap read is the same
    /// answer; an id nothing carries, and an id that could never be a run's, are
    /// no run and not an error; a closed run is still a run; and a board with
    /// nothing on it counts nothing, so a count that always said something would
    /// fail on the empty store.
    pub async fn one_run_is_summarised_and_the_runs_of_each_bot_are_counted(store: &dyn Sessions) {
        assert!(
            store.run_counts().await.expect("count ok").is_empty(),
            "a board with no run counts none"
        );
        assert!(
            store
                .summary_of(&SessionId("nothing1".into()))
                .await
                .expect("summary ok")
                .is_none(),
            "a board with no run summarises none"
        );

        let busy = begin(store, "gamma", "the busy run", 0).await;
        journal(store, &busy.id, "first beat", 60).await;
        journal(store, &busy.id, "second beat", 120).await;
        let other = begin(store, "gamma", "the second run", 300).await;
        let theirs = begin(store, "delta", "somebody else's run", 600).await;
        store
            .close(&other.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let summary = store
            .summary_of(&busy.id)
            .await
            .expect("summary ok")
            .expect("the run exists");
        let listed = store
            .summaries_of(&bot("gamma"))
            .await
            .expect("list ok")
            .into_iter()
            .find(|run| run.id == busy.id)
            .expect("the bot lists it");
        assert_eq!(summary, listed, "the one-run read is the listing's answer");
        assert_eq!(summary.entry_count, 2, "{summary:?}");
        assert_eq!(summary.bot, bot("gamma"));

        let closed = store
            .summary_of(&other.id)
            .await
            .expect("summary ok")
            .expect("a closed run is still a run");
        assert_eq!(closed.state, SessionState::Wrapped);
        let elsewhere = store
            .summary_of(&theirs.id)
            .await
            .expect("summary ok")
            .expect("another bot's run is a run");
        assert_eq!(elsewhere.bot, bot("delta"));

        for id in ["nothing1", "Not A Run!"] {
            assert!(
                store
                    .summary_of(&SessionId(id.into()))
                    .await
                    .expect("a miss is not an error")
                    .is_none(),
                "{id} is no run"
            );
        }

        let counts = store.run_counts().await.expect("count ok");
        assert_eq!(counts.get(&bot("gamma")), Some(&2), "{counts:?}");
        assert_eq!(counts.get(&bot("delta")), Some(&1), "{counts:?}");
        assert_eq!(
            counts.len(),
            2,
            "no bot without a run is counted: {counts:?}"
        );
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
                .append(&missing, NewEntry::manual("a beat", at(60), None))
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
                .append(&session.id, NewEntry::manual("   ", at(60), None))
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
                    sid: sid(900),
                    focus: "  ".into(),
                    started_at: at(0),
                    timezone: None,
                    started_on: None,
                    id: None,
                    key_hash: None,
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

    /// A sid is minted, never written by hand. `begin` refuses one that does
    /// not have the drawn shape — every store, not only the one this build
    /// happens to run against.
    pub async fn a_handle_that_is_not_drawn_is_refused(store: &dyn Sessions) {
        let err = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: Sid("BAD1".into()),
                focus: "working".into(),
                started_at: at(0),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect_err("a handwritten sid is not a handle this build mints");
        assert!(matches!(err, SessionError::InvalidEntry(_)), "got {err:?}");
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
                NewEntry::manual("line one\r\nline two", at(120), None),
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

    /// **`wrap_session`'s own field survives a write and a read, and an
    /// ordinary entry carries none.** Paired, in one round trip: a store that
    /// answered every entry with the same value — the run's current focus,
    /// say — would satisfy the first half and fail silently on the second.
    pub async fn a_closing_focus_survives_the_round_trip_and_an_ordinary_entry_carries_none(
        store: &dyn Sessions,
    ) {
        let session = begin(store, "gamma", "the first run", 0).await;
        journal(store, &session.id, "read the hand-off", 60).await;
        store
            .append(
                &session.id,
                NewEntry::manual("the seam is cut", at(120), None)
                    .closing(Some("cutting the codec seam".to_string())),
            )
            .await
            .expect("append ok");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert_eq!(
            read.entries[0].closing_focus, None,
            "an ordinary entry carries no closing focus"
        );
        assert_eq!(
            read.entries[1].closing_focus.as_deref(),
            Some("cutting the codec seam"),
            "the closing entry's own field survives the round trip"
        );
    }

    /// **The closing mark survives a write and a read, whether or not the
    /// closing entry carries a focus, and an ordinary entry carries none.**
    /// Paired in one round trip: a store that marked every entry, or only the
    /// entries that carry a focus, would pass one half and fail the other. The
    /// closing entry with NO focus is the case the mark exists for.
    pub async fn a_closing_mark_survives_the_round_trip_and_an_ordinary_entry_carries_none(
        store: &dyn Sessions,
    ) {
        let session = begin(store, "gamma", "the first run", 0).await;
        journal(store, &session.id, "read the hand-off", 60).await;
        store
            .append(
                &session.id,
                NewEntry::manual("the seam is cut", at(120), None).closing(None),
            )
            .await
            .expect("append ok");
        store
            .append(
                &session.id,
                NewEntry::manual("and the focus rides with it", at(180), None)
                    .closing(Some("cutting the codec seam".to_string())),
            )
            .await
            .expect("append ok");

        let read = store.read_session(&session.id).await.expect("read ok");
        assert!(
            !read.entries[0].closing,
            "an ordinary entry is not a closing one"
        );
        assert!(
            read.entries[1].closing && read.entries[1].closing_focus.is_none(),
            "a closing entry with no focus is marked all the same"
        );
        assert!(
            read.entries[2].closing && read.entries[2].closing_focus.is_some(),
            "and so is one that carries a focus"
        );
        assert_eq!(
            read.closing_entry().map(|entry| entry.text.as_str()),
            Some("and the focus rides with it"),
            "the newest closing entry is the one the run ended on"
        );
    }

    /// The whole spec, against one store. Each case runs on a **fresh** store,
    /// so nothing here depends on the order the others ran in.
    /// 🚨 **A stated day survives a write and a read**, on the run and on the
    /// beat.
    ///
    /// The sweep decides in the caller's frame, and it reads that frame back
    /// out of the store on the next boot. **A store that takes the day and
    /// loses it answers every later boot on the clock while the caller believes
    /// it stated a frame** — the failure is silent and it looks exactly like
    /// the defect this replaced.
    ///
    /// ⚠️ **It belongs here rather than in a case of the fake's**, because the
    /// two stores fail this differently: a double holds a struct and cannot
    /// drop a column, and a real store drops one by naming the wrong list or
    /// binding nothing to it. Both of those happened while this was built.
    ///
    /// **The other half is the run that states nothing**, which must read back
    /// as nothing — a store that stamped a day of its own would satisfy the
    /// first half and assume the frame it is forbidden to assume.
    pub async fn a_stated_day_survives_a_write_and_a_read(store: &dyn Sessions) {
        let day = |text: &str| text.parse::<Date>().expect("a date");
        let stated = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(80),
                focus: "acting out a year".to_string(),
                started_at: at(80),
                timezone: None,
                started_on: Some(day("2026-03-15")),
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run may say which day it is in");
        store
            .append(
                &stated.id,
                NewEntry::manual("what I set out to do", at(81), Some(day("2026-03-16"))),
            )
            .await
            .expect("a beat may say which day it happened on");

        let read = store
            .read_session(&stated.id)
            .await
            .expect("the run reads back");
        assert_eq!(
            read.started_on,
            Some(day("2026-03-15")),
            "the day the run stated did not survive the round trip",
        );
        assert_eq!(
            read.entries.first().and_then(|e| e.on),
            Some(day("2026-03-16")),
            "the day the beat stated did not survive the round trip",
        );
        assert_eq!(
            read.last_beat_on(),
            Some(day("2026-03-16")),
            "the newest stated day is what the sweep measures, and it is not the one stored",
        );

        // **The run that states nothing.** A store that answered with a day of
        // its own would pass everything above and assume the frame.
        let quiet = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(82),
                focus: "happening now".to_string(),
                started_at: at(82),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run need not say which day it is in");
        store
            .append(&quiet.id, NewEntry::manual("a beat", at(83), None))
            .await
            .expect("a beat need not either");
        let read = store
            .read_session(&quiet.id)
            .await
            .expect("the run reads back");
        assert_eq!(read.started_on, None, "a day was invented for a run");
        assert_eq!(
            read.entries.first().and_then(|e| e.on),
            None,
            "a day was invented for a beat",
        );
        assert_eq!(
            read.last_beat_on(),
            None,
            "…so there is no frame to measure"
        );
    }

    /// **A resume moves the day exactly the way it moves the zone** —
    /// [`a_runs_zone_is_stored_and_can_be_moved`]'s own shape, on
    /// [`Sessions::set_stated_day`] rather than
    /// [`Sessions::set_timezone`]. `started_on` is untouched throughout:
    /// this is the OTHER day, the one a later resume moved to.
    pub async fn a_moved_day_is_stored_and_can_be_moved_again(store: &dyn Sessions) {
        let day = |text: &str| text.parse::<Date>().expect("a date");
        let born = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(90),
                focus: "acting out a year".to_string(),
                started_at: at(90),
                timezone: None,
                started_on: Some(day("2026-03-15")),
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run may be born stating a day");
        assert_eq!(
            store
                .read_session(&born.id)
                .await
                .expect("it reads back")
                .stated_day,
            None,
            "nothing has moved the day yet"
        );

        // **The device hop, or the day after.** The same run, picked up with a
        // different day stated.
        let moved = store
            .set_stated_day(&born.id, Some(day("2026-04-01")))
            .await
            .expect("a run may be moved into another day");
        assert_eq!(moved.stated_day, Some(day("2026-04-01")));
        assert_eq!(
            moved.started_on,
            Some(day("2026-03-15")),
            "the creation day is untouched by a move"
        );
        assert_eq!(
            store
                .read_session(&born.id)
                .await
                .expect("it reads back")
                .stated_day,
            Some(day("2026-04-01")),
            "…and the card says so, not just the answer to the write"
        );

        // A run nobody has moved carries no stated day at all — without this
        // a store that invents one for every row would pass everything above.
        let bare = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: sid(91),
                focus: "no day moved".to_string(),
                started_at: at(91),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("a run may have no moved day");
        assert_eq!(
            store
                .read_session(&bare.id)
                .await
                .expect("it reads back")
                .stated_day,
            None,
            "a run nobody moved is not handed a stated day"
        );
    }

    /// A fixed wrap code for the spec. It only has to be shaped like one.
    fn code(nth: i64) -> String {
        format!("{WRAP_CODE_PREFIX}fixture{nth}")
    }

    /// **A wrapped run's window is stored, read back and cleared, and no other
    /// run has one.** Paired with the refusal on a run that is not wrapped, and
    /// with a fresh run that carries none.
    pub async fn a_wrapped_runs_window_is_stored_read_back_and_cleared(store: &dyn Sessions) {
        let fresh = begin(store, "gamma", "never wrapped", 0).await;
        assert_eq!(fresh.wrap_window, None, "a fresh run carries no window");
        assert!(
            matches!(
                store
                    .set_wrap_window(&fresh.id, Some(WrapWindow::Offered(code(1))))
                    .await,
                Err(SessionError::NotWrapped { .. })
            ),
            "an active run has no window to set"
        );

        let run = begin(store, "gamma", "told its story", 10).await;
        store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        let offered = store
            .set_wrap_window(&run.id, Some(WrapWindow::Offered(code(2))))
            .await
            .expect("a wrapped run takes a window");
        assert_eq!(offered.wrap_window, Some(WrapWindow::Offered(code(2))));
        assert_eq!(
            offered.state,
            SessionState::Wrapped,
            "the state never moves"
        );
        assert_eq!(
            store
                .read_session(&run.id)
                .await
                .expect("read ok")
                .wrap_window,
            Some(WrapWindow::Offered(code(2))),
            "the card says so, not only the answer to the write"
        );
        let cleared = store
            .set_wrap_window(&run.id, None)
            .await
            .expect("a window can be cleared");
        assert_eq!(cleared.wrap_window, None);
        assert_eq!(
            store
                .read_session(&run.id)
                .await
                .expect("read ok")
                .wrap_window,
            None
        );
    }

    /// **An open window lets a wrapped run write once more, and the second
    /// close ends it.** An offered window does not: only the code's use opens
    /// it. The run reads `wrapped` the whole time.
    pub async fn an_open_window_lets_a_wrapped_run_write_and_the_second_close_ends_it(
        store: &dyn Sessions,
    ) {
        let run = begin(store, "gamma", "told its story", 0).await;
        journal(store, &run.id, "the first story", 30).await;
        store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        store
            .set_wrap_window(&run.id, Some(WrapWindow::Offered(code(3))))
            .await
            .expect("offered");

        let closed = |err: SessionError| {
            assert!(
                matches!(&err, SessionError::Closed { state, .. } if *state == SessionState::Wrapped),
                "a wrapped run with no open window is closed: {err:?}"
            );
        };
        closed(
            store
                .append(&run.id, NewEntry::manual("too early", at(60), None))
                .await
                .expect_err("an offered window does not take writes"),
        );

        store
            .set_wrap_window(&run.id, Some(WrapWindow::Open(code(3))))
            .await
            .expect("opened");
        store
            .append(&run.id, NewEntry::manual("a last change", at(90), None))
            .await
            .expect("an open window takes an append");
        store
            .amend_last(&run.id, "a last change, said better")
            .await
            .expect("…and an amend of the newest entry");
        store
            .set_focus(&run.id, "the last change")
            .await
            .expect("…and a focus");
        let open = store.read_session(&run.id).await.expect("read ok");
        assert_eq!(open.state, SessionState::Wrapped, "it stays wrapped");
        assert_eq!(
            open.entries.len(),
            2,
            "the first story stands, one entry added"
        );

        // **The second wrap**: a closing entry beside the first, then the close.
        store
            .append(&run.id, NewEntry::manual("the second story", at(120), None))
            .await
            .expect("the second story appends");
        let ended = store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("closing an open window is not refused");
        assert_eq!(ended.state, SessionState::Wrapped);
        assert_eq!(ended.wrap_window, None, "the second close ends the window");
        closed(
            store
                .append(&run.id, NewEntry::manual("too late", at(150), None))
                .await
                .expect_err("an ended window takes no more"),
        );
        let read = store.read_session(&run.id).await.expect("read ok");
        assert_eq!(read.entries.len(), 3, "both stories stand and nothing else");
        assert_eq!(read.wrap_window, None);
    }

    /// **A charter change is kept per bot, rewritten in place by the next one,
    /// and a bot with none reads as none.** Who and when survive the round trip;
    /// another bot's change is not this bot's.
    pub async fn a_charter_change_is_kept_per_bot_and_the_newest_replaces_it(store: &dyn Sessions) {
        let gamma = EntityId("bot:gamma".into());
        let sigma = EntityId("bot:sigma".into());
        let delta = EntityId("bot:delta".into());
        assert_eq!(
            store.charter_change_of(&gamma).await.expect("read ok"),
            None,
            "a bot nobody rewrote has no change"
        );

        store
            .note_charter_change(
                &gamma,
                CharterChange {
                    by: delta.clone(),
                    at: at(100),
                },
                None,
            )
            .await
            .expect("a change is kept");
        let first = store.charter_change_of(&gamma).await.expect("read ok");
        assert_eq!(
            first,
            Some(CharterChange {
                by: delta.clone(),
                at: at(100)
            }),
            "who and when survive the round trip"
        );
        assert_eq!(
            store.charter_change_of(&sigma).await.expect("read ok"),
            None,
            "another bot's charter is not this one's"
        );

        store
            .note_charter_change(
                &gamma,
                CharterChange {
                    by: sigma.clone(),
                    at: at(200),
                },
                None,
            )
            .await
            .expect("the next change replaces it");
        assert_eq!(
            store.charter_change_of(&gamma).await.expect("read ok"),
            Some(CharterChange {
                by: sigma,
                at: at(200)
            }),
            "only the newest change is kept"
        );
    }

    /// **A bot's own rewrite is not the change another identity made.** It
    /// leaves the newest change by another standing, and on a bot nobody else
    /// rewrote it leaves none to read.
    pub async fn a_bots_own_rewrite_leaves_the_newest_change_by_another_standing(
        store: &dyn Sessions,
    ) {
        let gamma = EntityId("bot:gamma".into());
        let sigma = EntityId("bot:sigma".into());
        let delta = EntityId("bot:delta".into());

        store
            .note_charter_change(
                &sigma,
                CharterChange {
                    by: sigma.clone(),
                    at: at(50),
                },
                None,
            )
            .await
            .expect("an own rewrite is kept");
        assert_eq!(
            store.charter_change_of(&sigma).await.expect("read ok"),
            None,
            "a bot nobody else rewrote has no change to be told of"
        );

        store
            .note_charter_change(
                &gamma,
                CharterChange {
                    by: delta.clone(),
                    at: at(100),
                },
                None,
            )
            .await
            .expect("another identity's change is kept");
        store
            .note_charter_change(
                &gamma,
                CharterChange {
                    by: gamma.clone(),
                    at: at(200),
                },
                None,
            )
            .await
            .expect("an own rewrite is kept");
        assert_eq!(
            store.charter_change_of(&gamma).await.expect("read ok"),
            Some(CharterChange {
                by: delta,
                at: at(100)
            }),
            "the change by another stands after the bot rewrote its own"
        );
    }

    /// **The moment a run was handed its charter is stored, can be moved, and a
    /// run nobody told reads as none.** It is set on a closed run too, because it
    /// changes nothing the run said.
    pub async fn the_moment_a_charter_was_handed_over_is_stored_and_can_be_moved(
        store: &dyn Sessions,
    ) {
        let told = begin(store, "gamma", "reading things", 5).await;
        let untold = begin(store, "sigma", "not asked yet", 6).await;
        assert_eq!(told.charter_told_at, None, "a new run holds none");

        store
            .set_charter_told(&told.id, at(100))
            .await
            .expect("set ok");
        assert_eq!(
            store
                .read_session(&told.id)
                .await
                .expect("read ok")
                .charter_told_at,
            Some(at(100)),
            "the moment survives the round trip"
        );
        store
            .set_charter_told(&told.id, at(200))
            .await
            .expect("moved ok");
        store
            .close(&told.id, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        store
            .set_charter_told(&told.id, at(300))
            .await
            .expect("a closed run still takes it");
        assert_eq!(
            store
                .read_session(&told.id)
                .await
                .expect("read ok")
                .charter_told_at,
            Some(at(300))
        );
        assert_eq!(
            store
                .read_session(&untold.id)
                .await
                .expect("read ok")
                .charter_told_at,
            None,
            "another run is not told by it"
        );
        let miss = store
            .set_charter_told(&SessionId("999999".into()), at(1))
            .await
            .expect_err("no such run");
        assert!(
            matches!(miss, SessionError::UnknownSession { .. }),
            "{miss:?}"
        );
    }

    /// **A run's key is stored as its hash, beside the card.** A run born with a
    /// hash reads it back, a run born without one reads `None` (the keyless run
    /// an older build left), the hash can be replaced, replaced by the void
    /// sentinel on a run that has wrapped, and a run that does not exist is a
    /// miss. The card is born with the id the boot minted for it when one is
    /// given.
    pub async fn a_runs_key_hash_is_stored_with_the_card_and_can_be_replaced(store: &dyn Sessions) {
        let keyless = begin(store, "gamma", "an older run", 0).await;
        assert_eq!(
            keyless.key_hash, None,
            "a run born without a key holds none"
        );

        let name = SessionId("k3m9qz".into());
        let keyed = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: Sid("k3m9qz".into()),
                id: Some(name.clone()),
                key_hash: Some("hash-one".into()),
                focus: "a keyed run".into(),
                started_at: at(1),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin with an id and a hash");
        assert_eq!(keyed.id, name, "the card is born with the minted id");
        assert_eq!(keyed.key_hash.as_deref(), Some("hash-one"));
        assert_eq!(
            store
                .read_session(&name)
                .await
                .expect("read")
                .key_hash
                .as_deref(),
            Some("hash-one"),
            "…and a read gives it back"
        );

        store
            .set_key_hash(&name, "hash-two", &Default::default())
            .await
            .expect("a key is replaced");
        assert_eq!(
            store
                .read_session(&name)
                .await
                .expect("read")
                .key_hash
                .as_deref(),
            Some("hash-two")
        );
        let listed = store.all_sessions().await.expect("all");
        let hashes: Vec<Option<&str>> = listed
            .iter()
            .filter(|run| run.id == name || run.id == keyless.id)
            .map(|run| run.key_hash.as_deref())
            .collect();
        assert!(
            hashes.contains(&Some("hash-two")) && hashes.contains(&None),
            "the board carries each card's hash: {hashes:?}"
        );

        // A wrap ends the key, and it does so on a run that is already closed.
        store
            .close(&name, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        store
            .set_key_hash(&name, super::super::KEY_VOID, &Default::default())
            .await
            .expect("a wrapped run's key is voided");
        assert_eq!(
            store
                .read_session(&name)
                .await
                .expect("read")
                .key_hash
                .as_deref(),
            Some(super::super::KEY_VOID)
        );

        let miss = store
            .set_key_hash(
                &SessionId("999999".into()),
                "hash-three",
                &Default::default(),
            )
            .await
            .expect_err("no such run");
        assert!(
            matches!(miss, SessionError::UnknownSession { .. }),
            "{miss:?}"
        );
    }

    /// **The changes of a run come back in order, each with its time and what was
    /// kept of it**, while every current read answers as it always did. Paired:
    /// the run's other changes are not in another run's account, and the focus,
    /// zone, day and window read back on the card as before.
    pub async fn a_runs_changes_are_kept_in_order_with_their_time_and_value(store: &dyn Sessions) {
        use super::super::history::ChangeKind as K;
        let run = begin(store, "gamma", "reading", 0).await;
        let other = begin(store, "delta", "elsewhere", 1).await;
        store
            .set_focus(&run.id, "writing")
            .await
            .expect("focus is set");
        store
            .set_timezone(&run.id, Some("Europe/Madrid"))
            .await
            .expect("zone is set");
        store
            .set_stated_day(&run.id, Some(Date::constant(2026, 3, 15)))
            .await
            .expect("day is stated");
        let entry = journal(store, &run.id, "read the task", 30).await;
        store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        for window in [
            Some(WrapWindow::Offered(code(2))),
            Some(WrapWindow::Open(code(2))),
            None,
        ] {
            store
                .set_wrap_window(&run.id, window)
                .await
                .expect("a wrapped run takes a window");
        }

        let history = store.session_history(&run.id).await.expect("history reads");

        assert_eq!(
            history.iter().map(|c| c.changed).collect::<Vec<_>>(),
            [
                K::Begin,
                K::Focus,
                K::Timezone,
                K::StatedDay,
                K::Entry,
                K::State,
                K::WrapWindow,
                K::WrapWindow,
                K::WrapWindow
            ]
            .map(Some)
        );
        assert_eq!(
            history
                .iter()
                .map(|c| c.value.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("reading"),
                Some("writing"),
                Some("Europe/Madrid"),
                Some("2026-03-15"),
                Some(entry.id.as_str()),
                Some("wrapped"),
                Some("offered"),
                Some("open"),
                None
            ]
        );
        assert_eq!(
            history.iter().map(|c| c.ordinal).collect::<Vec<_>>(),
            (1..=9).collect::<Vec<_>>()
        );
        let times: Vec<_> = history.iter().map(|c| c.written_at).collect();
        assert!(
            times.iter().all(Option::is_some) && times.windows(2).all(|w| w[0] <= w[1]),
            "every change carries its moment and they do not run backwards: {times:?}"
        );
        // The projection answers as before.
        let card = store.read_session(&run.id).await.expect("read ok");
        assert_eq!(card.focus, "writing");
        assert_eq!(card.timezone.as_deref(), Some("Europe/Madrid"));
        assert_eq!(card.stated_day, Some(Date::constant(2026, 3, 15)));
        assert_eq!(card.wrap_window, None);
        // …and the other run has an account of its own.
        let elsewhere = store
            .session_history(&other.id)
            .await
            .expect("history reads");
        assert_eq!(
            elsewhere
                .iter()
                .map(|c| (c.ordinal, c.changed))
                .collect::<Vec<_>>(),
            vec![(1, Some(K::Begin))]
        );
    }

    /// **A close that ends an open window records both**, and one that finds no
    /// window records the state alone. The wrap code is in no value anywhere.
    pub async fn closing_a_run_with_a_window_records_the_state_and_the_window_going(
        store: &dyn Sessions,
    ) {
        use super::super::history::ChangeKind as K;
        let run = begin(store, "gamma", "told its story", 0).await;
        store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("wrap ok");
        store
            .set_wrap_window(&run.id, Some(WrapWindow::Open(code(3))))
            .await
            .expect("window opens");
        store
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("the second close ends the window");

        let history = store.session_history(&run.id).await.expect("history reads");

        assert_eq!(
            history.iter().map(|c| c.changed).collect::<Vec<_>>(),
            [K::Begin, K::State, K::WrapWindow, K::State, K::WrapWindow].map(Some)
        );
        assert_eq!(history[4].value, None, "the window going has no value");
        assert!(
            !format!("{history:?}").contains(&code(3)),
            "the wrap code is in the account: {history:?}"
        );
    }

    /// **A reset of what is served, the key, the charter moment and an abandon
    /// and a walk-back are each recorded, and the key's hash is in no value.** An
    /// answer served adds to the total and records nothing: the reset is one
    /// record carrying the total it set back. A resume of a run that is already
    /// active changes nothing and records nothing.
    pub async fn served_key_charter_and_the_ends_of_a_run_are_recorded(store: &dyn Sessions) {
        use super::super::history::ChangeKind as K;
        let name = SessionId("k3m9qz".into());
        let run = store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: Sid("k3m9qz".into()),
                id: Some(name.clone()),
                key_hash: Some("hash-seed".into()),
                focus: "a keyed run".into(),
                started_at: at(1),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin with a key");
        store
            .add_served(&run.id, 40, jiff::civil::date(2026, 9, 2))
            .await
            .expect("served");
        store
            .add_served(&run.id, 2, jiff::civil::date(2026, 9, 2))
            .await
            .expect("served");
        store.reset_served(&run.id).await.expect("reset");
        store
            .set_key_hash(&run.id, "hash-one", &super::super::KeyTrail::default())
            .await
            .expect("rekey");
        store
            .set_key_hash(
                &run.id,
                super::super::KEY_VOID,
                &super::super::KeyTrail::default(),
            )
            .await
            .expect("void");
        store
            .set_charter_told(&run.id, at(7))
            .await
            .expect("charter told");
        store.reopen(&run.id).await.expect("an active run resumes");
        store
            .close(&run.id, SessionState::Abandoned)
            .await
            .expect("abandon");
        store.reopen(&run.id).await.expect("walk back");

        let history = store.session_history(&run.id).await.expect("history reads");

        assert_eq!(
            history.iter().map(|c| c.changed).collect::<Vec<_>>(),
            [
                K::Begin,
                K::ServedReset,
                K::Key,
                K::Key,
                K::CharterTold,
                K::State,
                K::State
            ]
            .map(Some)
        );
        assert_eq!(
            history
                .iter()
                .map(|c| c.value.as_deref())
                .collect::<Vec<_>>()[1..],
            [
                Some("42"),
                None,
                None,
                Some(at(7).to_string().as_str()),
                Some("abandoned"),
                Some("active")
            ]
        );
        let shown = format!("{history:?}");
        for secret in ["hash-seed", "hash-one"] {
            assert!(
                !shown.contains(secret),
                "{secret} is in the account: {shown}"
            );
        }
        assert!(history.iter().all(|c| c.written_at.is_some()));
    }

    /// **Every wording of an amended entry comes back in order**, the entry itself
    /// stays the newest, and an entry nobody amended has the one it was written
    /// with. An automatic beat keeps the moment each correction marked.
    pub async fn every_wording_of_an_amended_entry_is_kept(store: &dyn Sessions) {
        let run = begin(store, "gamma", "reading", 0).await;
        let first = journal(store, &run.id, "first wording", 10).await;
        store
            .amend_last(&run.id, "second wording")
            .await
            .expect("amend");
        store
            .amend_last(&run.id, "third wording")
            .await
            .expect("amend");
        let beat = store
            .append(
                &run.id,
                NewEntry::beat("capture", "captured facts: one (1)", at(60), None),
            )
            .await
            .expect("beat");
        store
            .amend_beat(&run.id, &beat.id, "captured facts: one, two (2)", at(600))
            .await
            .expect("amend_beat");
        store
            .amend_beat(
                &run.id,
                &beat.id,
                "captured facts: one, two, three (3)",
                at(700),
            )
            .await
            .expect("amend_beat");
        let plain = journal(store, &run.id, "never amended", 90).await;

        let wordings = store
            .journal_history(&run.id, &first.id)
            .await
            .expect("wordings read");
        assert_eq!(
            wordings
                .iter()
                .map(|w| (w.revision, w.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (1, "first wording"),
                (2, "second wording"),
                (3, "third wording")
            ]
        );
        assert!(
            wordings
                .windows(2)
                .all(|w| w[0].written_at <= w[1].written_at)
        );
        let moved = store
            .journal_history(&run.id, &beat.id)
            .await
            .expect("wordings read");
        assert_eq!(
            moved
                .iter()
                .map(|w| (w.text.as_str(), w.touched))
                .collect::<Vec<_>>(),
            vec![
                ("captured facts: one (1)", None),
                ("captured facts: one, two (2)", Some(at(600))),
                ("captured facts: one, two, three (3)", Some(at(700)))
            ]
        );
        let only = store
            .journal_history(&run.id, &plain.id)
            .await
            .expect("wordings read");
        assert_eq!(
            only.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            vec!["never amended"],
            "an entry nobody amended has the wording it was written with"
        );
        // The entry is the newest wording, and the account of the run names the
        // amendments in order beside the rest of its changes.
        let card = store.read_session(&run.id).await.expect("read ok");
        assert_eq!(card.entries[0].text, "third wording");
        let kinds: Vec<_> = store
            .session_history(&run.id)
            .await
            .expect("history reads")
            .iter()
            .map(|c| c.changed.map(|k| k.as_token()))
            .collect();
        assert_eq!(
            kinds,
            [
                "begin",
                "entry",
                "amend_last",
                "amend_last",
                "entry",
                "amend_beat",
                "amend_beat",
                "entry"
            ]
            .map(Some)
        );
    }

    /// **A history of a run or an entry that is not there is a miss.**
    pub async fn the_history_of_what_is_not_there_is_a_miss(store: &dyn Sessions) {
        let run = begin(store, "gamma", "reading", 0).await;
        assert!(matches!(
            store.session_history(&SessionId("zzzzzz".into())).await,
            Err(SessionError::UnknownSession { .. })
        ));
        assert!(matches!(
            store
                .journal_history(&SessionId("zzzzzz".into()), &EntryId("e1".into()))
                .await,
            Err(SessionError::UnknownSession { .. })
        ));
        assert!(matches!(
            store
                .journal_history(&run.id, &EntryId("nonesuch".into()))
                .await,
            Err(SessionError::NoEntries { .. })
        ));
    }

    /// **One scan lists every run's changes and every entry's wordings, each as
    /// the reads of one run and one entry give them.** An entry nobody wrote
    /// twice has the one wording it was written with, and an entry of another run
    /// has its own.
    pub async fn a_scan_of_every_record_gives_what_the_reads_of_each_give(store: &dyn Sessions) {
        let first = begin(store, "gamma", "reading", 0).await;
        let second = begin(store, "delta", "elsewhere", 1).await;
        let entry = journal(store, &first.id, "first wording", 10).await;
        store
            .amend_last(&first.id, "second wording")
            .await
            .expect("amend");
        store
            .set_focus(&second.id, "writing")
            .await
            .expect("focus is set");
        let other = journal(store, &second.id, "an entry of the other run", 11).await;

        let scanned = store.scan_session_history().await.expect("the scan reads");

        let mut runs: Vec<&SessionId> = scanned.changes.iter().map(|(id, _)| id).collect();
        runs.sort();
        let mut both = vec![&first.id, &second.id];
        both.sort();
        assert_eq!(runs, both, "both runs have changes: {scanned:?}");
        for (id, changes) in &scanned.changes {
            assert_eq!(
                changes,
                &store.session_history(id).await.expect("history reads"),
                "the scan and the history of {id:?} disagree"
            );
        }
        let mut entries: Vec<(&SessionId, &EntryId)> = scanned
            .wordings
            .iter()
            .map(|(run, e, _)| (run, e))
            .collect();
        entries.sort();
        let mut both = vec![(&first.id, &entry.id), (&second.id, &other.id)];
        both.sort();
        assert_eq!(entries, both, "both entries have wordings: {scanned:?}");
        for (run, entry, wordings) in &scanned.wordings {
            assert_eq!(
                wordings,
                &store
                    .journal_history(run, entry)
                    .await
                    .expect("wordings read"),
                "the scan and the wordings of {entry:?} disagree"
            );
        }
        let first_wordings = scanned
            .wordings
            .iter()
            .find(|(run, e, _)| run == &first.id && e == &entry.id)
            .expect("the amended entry is listed");
        assert_eq!(first_wordings.2.len(), 2);
    }

    /// **A key's history is stored in the same write as the key and replaced
    /// whole.** The retired key and the pick-up mark come back with the card,
    /// from a read of the one run and from a read of the board; a later write
    /// that carries none leaves none.
    pub async fn a_key_trail_is_stored_with_the_hash_and_replaced_whole(store: &dyn Sessions) {
        let name = SessionId("b4n8qz".into());
        store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: Sid("b4n8qz".into()),
                id: Some(name.clone()),
                key_hash: Some("hash-one".into()),
                focus: "a keyed run".into(),
                started_at: at(1),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin");
        assert_eq!(
            store.read_session(&name).await.expect("read").key_trail,
            super::super::KeyTrail::default(),
            "a run never resumed or wrapped holds no history"
        );

        let trail = super::super::KeyTrail {
            retired: Some(super::super::RetiredKey {
                hash: "hash-one".into(),
                at: at(5),
                by: super::super::RetiredBy::Resumed,
            }),
            taken_at: Some(at(6)),
        };
        store
            .set_key_hash(&name, "hash-two", &trail)
            .await
            .expect("a key is replaced with its history");
        assert_eq!(
            store.read_session(&name).await.expect("read").key_trail,
            trail
        );
        let board = store.all_sessions().await.expect("all");
        let card = board
            .iter()
            .find(|run| run.id == name)
            .expect("on the board");
        assert_eq!(card.key_trail, trail, "the board carries it too");
        assert_eq!(card.key_hash.as_deref(), Some("hash-two"));

        // A wrap's history names the other reason, and replaces the first whole.
        let wrapped = super::super::KeyTrail {
            retired: Some(super::super::RetiredKey {
                hash: "hash-two".into(),
                at: at(9),
                by: super::super::RetiredBy::Wrapped,
            }),
            taken_at: None,
        };
        store
            .set_key_hash(&name, super::super::KEY_VOID, &wrapped)
            .await
            .expect("a wrap ends the key");
        assert_eq!(
            store.read_session(&name).await.expect("read").key_trail,
            wrapped,
            "the pick-up mark of the earlier write is gone with it"
        );
        store
            .set_key_hash(&name, "hash-three", &Default::default())
            .await
            .expect("a key with no history");
        assert_eq!(
            store.read_session(&name).await.expect("read").key_trail,
            super::super::KeyTrail::default()
        );
    }

    /// **A boot's note of a run is kept until the run writes, and deleted when it
    /// is spent or abandoned.** Written once; a read shows only the notes with no
    /// card; the prune deletes the spent ones and the ones minted before the
    /// moment it is given, and keeps the rest.
    pub async fn a_minted_run_is_kept_until_it_writes_and_pruned_when_it_does_not(
        store: &dyn Sessions,
    ) {
        let minted = |name: &str, hash: &str, when: i64| super::super::MintedRun {
            name: Sid(name.into()),
            key_hash: hash.into(),
            bot: bot("gamma"),
            zone: Some("America/New_York".into()),
            day: Some(jiff::civil::date(2026, 10, 12)),
            minted_at: at(when),
        };
        let (spent, old, recent) = (
            minted("c5p7sz", "hash-spent", 10),
            minted("d6q2tz", "hash-old", 20),
            minted("f7r3vz", "hash-recent", 30),
        );
        for run in [&spent, &old, &recent] {
            store.note_minted(run).await.expect("a boot's note");
        }
        // Written once: the same name noted again changes nothing.
        store
            .note_minted(&super::super::MintedRun {
                key_hash: "hash-other".into(),
                ..old.clone()
            })
            .await
            .expect("noting a name twice is not an error");
        assert_eq!(
            store.minted_without_a_card().await.expect("read"),
            vec![spent.clone(), old.clone(), recent.clone()],
            "every note comes back whole, oldest first, with the first hash"
        );

        // The first write of one run makes its card, and its note is spent.
        store
            .begin(NewSession {
                bot: bot("gamma"),
                sid: spent.name.clone(),
                id: Some(SessionId(spent.name.0.clone())),
                key_hash: Some(spent.key_hash.clone()),
                focus: "wrote at last".into(),
                started_at: at(11),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin");
        assert_eq!(
            store.minted_without_a_card().await.expect("read"),
            vec![old.clone(), recent.clone()],
            "a run with a card is not read back from its note"
        );

        // The prune deletes the spent note and the note minted before the moment.
        assert_eq!(store.prune_minted(at(25)).await.expect("prune"), 2);
        assert_eq!(
            store.minted_without_a_card().await.expect("read"),
            vec![recent.clone()]
        );
        assert_eq!(
            store.prune_minted(at(25)).await.expect("prune again"),
            0,
            "nothing is left to delete"
        );
    }

    pub async fn run_all<S: Sessions, F: Fn() -> S>(fresh: F) {
        a_key_trail_is_stored_with_the_hash_and_replaced_whole(&fresh()).await;
        a_minted_run_is_kept_until_it_writes_and_pruned_when_it_does_not(&fresh()).await;
        a_runs_key_hash_is_stored_with_the_card_and_can_be_replaced(&fresh()).await;
        a_begun_session_is_active_and_empty(&fresh()).await;
        beginning_twice_under_one_handle_yields_one_run(&fresh()).await;
        the_chronology_accrues_oldest_first(&fresh()).await;
        a_beat_is_stored_beside_manual_entries_and_stays_distinguishable(&fresh()).await;
        an_amend_rewrites_the_last_entry_and_nothing_else(&fresh()).await;
        only_an_automatic_beat_is_amended_in_place(&fresh()).await;
        an_older_manual_entry_is_refused_by_address_and_a_beat_at_the_same_age_is_not(&fresh())
            .await;
        amending_a_beat_keeps_its_place_but_moves_the_clock(&fresh()).await;
        amending_with_no_entries_is_refused(&fresh()).await;
        focus_is_rewritten_in_place_and_leaves_the_chronology_alone(&fresh()).await;
        a_runs_zone_is_stored_and_can_be_moved(&fresh()).await;
        served_characters_accrue_and_an_untouched_session_reads_zero(&fresh()).await;
        run_totals_carry_the_served_total_and_every_compaction(&fresh()).await;
        call_counts_add_up_per_key_and_keep_a_call_with_no_bot_apart(&fresh()).await;
        served_characters_are_counted_on_the_day_they_were_served(&fresh()).await;
        a_closed_session_is_terminal_both_ways(&fresh()).await;
        an_abandoned_session_reopens_and_a_wrapped_one_never_does(&fresh()).await;
        sessions_are_listed_per_bot_newest_first(&fresh()).await;
        one_run_is_summarised_and_the_runs_of_each_bot_are_counted(&fresh()).await;
        all_sessions_carries_every_card_across_bots_with_its_handle(&fresh()).await;
        a_closed_session_stays_on_the_record(&fresh()).await;
        addressing_an_unknown_session_is_a_miss(&fresh()).await;
        malformed_input_is_refused(&fresh()).await;
        a_handle_that_is_not_drawn_is_refused(&fresh()).await;
        an_entry_survives_the_round_trip(&fresh()).await;
        a_closing_focus_survives_the_round_trip_and_an_ordinary_entry_carries_none(&fresh()).await;
        a_closing_mark_survives_the_round_trip_and_an_ordinary_entry_carries_none(&fresh()).await;
        a_stated_day_survives_a_write_and_a_read(&fresh()).await;
        a_moved_day_is_stored_and_can_be_moved_again(&fresh()).await;
        a_wrapped_runs_window_is_stored_read_back_and_cleared(&fresh()).await;
        an_open_window_lets_a_wrapped_run_write_and_the_second_close_ends_it(&fresh()).await;
        a_charter_change_is_kept_per_bot_and_the_newest_replaces_it(&fresh()).await;
        a_bots_own_rewrite_leaves_the_newest_change_by_another_standing(&fresh()).await;
        the_moment_a_charter_was_handed_over_is_stored_and_can_be_moved(&fresh()).await;
        a_runs_changes_are_kept_in_order_with_their_time_and_value(&fresh()).await;
        closing_a_run_with_a_window_records_the_state_and_the_window_going(&fresh()).await;
        served_key_charter_and_the_ends_of_a_run_are_recorded(&fresh()).await;
        every_wording_of_an_amended_entry_is_kept(&fresh()).await;
        the_history_of_what_is_not_there_is_a_miss(&fresh()).await;
        a_scan_of_every_record_gives_what_the_reads_of_each_give(&fresh()).await;
    }
}
