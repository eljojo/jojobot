//! [`FoldedSessions`] — every run, held in RAM above the store (decision log 292).
//!
//! jojobot is one server instance and the only writer, so it holds the runs in
//! memory and answers a read from there. [`FoldedSessions`] wraps a
//! [`Sessions`] and, once [`rebuild`](FoldedSessions::rebuild) has run, serves
//! the reads of the port from an image filled by the store's own reads: every
//! run whole, newest start first, and the ledger of charter changes.
//!
//! **A write goes to the store first, and the image takes the answer the store
//! gave.** The entry a write appended, the run a write changed: each comes back
//! from the store as it stored it, and the image holds that, so a held copy is
//! the store's own answer rather than a second computation of it. The writes
//! that answer `()` change one column by a known amount, and the image changes
//! that column by the same amount.
//!
//! Before the first `rebuild`, and after a write the image could not follow, no
//! image is held and every read goes to the store underneath: slower and still
//! correct. The next write fills it again.
//!
//! ⚠️ **This is only correct with exactly one writer in the process**, for the
//! reason [`super::Folded`] gives.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::Date;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::history::{
    Change, SessionChange, SessionHistories, Wording, WriteStamp, next_change, next_wording,
};
use jojobot_domain::session::{
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId,
    SessionState, SessionSummary, Sessions, WrapWindow, validate_session_id,
};

/// The runs and the charter ledger, as the store holds them.
struct Image {
    /// Every run, in the store's order: newest start first, the id breaking a
    /// tie.
    runs: Vec<Session>,
    /// Where each run sits in `runs`.
    at: BTreeMap<SessionId, usize>,
    /// The newest charter change another identity made to each bot.
    charter_changes: BTreeMap<EntityId, CharterChange>,
    /// Every change of every run that has any, oldest first.
    changes: BTreeMap<SessionId, Vec<SessionChange>>,
    /// Every wording of every entry that has any, oldest first.
    wordings: BTreeMap<(SessionId, EntryId), Vec<Wording>>,
}

impl Image {
    fn of(
        mut runs: Vec<Session>,
        charter_changes: BTreeMap<EntityId, CharterChange>,
        histories: SessionHistories,
    ) -> Self {
        runs.sort_by(order);
        let mut image = Image {
            runs,
            at: BTreeMap::new(),
            charter_changes,
            changes: histories.changes.into_iter().collect(),
            wordings: histories
                .wordings
                .into_iter()
                .map(|(run, entry, kept)| ((run, entry), kept))
                .collect(),
        };
        image.reindex();
        image
    }

    fn reindex(&mut self) {
        self.at = self
            .runs
            .iter()
            .enumerate()
            .map(|(place, run)| (run.id.clone(), place))
            .collect();
    }

    fn run(&self, id: &SessionId) -> Option<&Session> {
        self.at.get(id).map(|place| &self.runs[*place])
    }

    fn run_mut(&mut self, id: &SessionId) -> Option<&mut Session> {
        let place = *self.at.get(id)?;
        Some(&mut self.runs[place])
    }

    /// **Append the record of a change**, made by [`next_change`] as the store
    /// makes it, at the moment this write holds.
    fn record(&mut self, id: &SessionId, change: Change<'_>, at: Timestamp) {
        let held = self.changes.entry(id.clone()).or_default();
        // The records are held oldest first, so the newest is the highest.
        let made = next_change(held.last().map(|c| c.ordinal), change, at);
        held.push(made);
    }

    /// **Append the wording an entry was just written with**, made by
    /// [`next_wording`] as the store makes it.
    fn keep_wording(&mut self, id: &SessionId, written: &JournalEntry, at: Timestamp) {
        let held = self
            .wordings
            .entry((id.clone(), written.id.clone()))
            .or_default();
        let made = next_wording(
            held.iter().map(|w| w.revision),
            &written.text,
            written.touched,
            at,
        );
        held.push(made);
    }

    /// Hold `run` as the store answered it: replace the run of that id, or add
    /// it where the store's order puts it.
    fn put(&mut self, run: Session) {
        match self.at.get(&run.id) {
            Some(place) => self.runs[*place] = run,
            None => {
                let place = self.runs.partition_point(|held| order(held, &run).is_lt());
                self.runs.insert(place, run);
                self.reindex();
            }
        }
    }
}

/// The store's order: newest start first, the id breaking a tie.
fn order(a: &Session, b: &Session) -> std::cmp::Ordering {
    b.started_at
        .cmp(&a.started_at)
        .then_with(|| b.id.cmp(&a.id))
}

/// **Drops the image unless disarmed**: armed across a write whose outcome the
/// image cannot know if the write does not finish. A future dropped while the
/// store wrote never reaches `disarm`, and the image goes with it.
struct Uncertain<'a> {
    image: Option<&'a RwLock<Option<Image>>>,
}

impl<'a> Uncertain<'a> {
    fn armed(image: &'a RwLock<Option<Image>>) -> Self {
        Uncertain { image: Some(image) }
    }

    /// The outcome is known: keep the image.
    fn disarm(mut self) {
        self.image = None;
    }
}

impl Drop for Uncertain<'_> {
    fn drop(&mut self) {
        if let Some(image) = self.image {
            tracing::warn!(
                "a write's outcome is unknown, so reads go to the store until the next write \
                 fills the in-memory image of the runs again"
            );
            *image.write().expect("fold lock") = None;
        }
    }
}

/// A [`Sessions`] wrapped in a RAM image of everything its reads answer.
pub struct FoldedSessions {
    inner: Arc<dyn Sessions>,
    /// The image, once a [`rebuild`](FoldedSessions::rebuild) has filled it;
    /// `None` sends every read to the store.
    image: RwLock<Option<Image>>,
    /// **Held across a write and the image's following it.** Two writes then
    /// reach the image in the order they reached the store.
    writing: tokio::sync::Mutex<()>,
    /// Set by the first successful `rebuild`. A write the image could not follow
    /// drops the image, and this is what tells the next write to fill it again.
    wanted: AtomicBool,
    /// **The moment of the write in flight, shared with the store beneath.** The
    /// image takes the moment once per write and holds it here while the store
    /// writes, so the records the image appends are the records the store made.
    stamp: WriteStamp,
    /// The clock the moment is taken from.
    clock: Clock,
}

impl FoldedSessions {
    /// Wrap a store with **no image**. Every read answers correctly before a
    /// rebuild: it goes to the store.
    ///
    /// `stamp` is the one the store beneath reads its moment from, and `clock`
    /// is where the image takes a write's moment.
    pub fn new(inner: Arc<dyn Sessions>, stamp: WriteStamp, clock: Clock) -> Self {
        FoldedSessions {
            inner,
            image: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
            wanted: AtomicBool::new(false),
            stamp,
            clock,
        }
    }

    /// **The moment of this write**, taken once and held for the store beneath
    /// until the returned guard drops.
    fn moment(&self) -> (jojobot_domain::session::history::HeldStamp, Timestamp) {
        let at = self.clock.now();
        (self.stamp.hold(at), at)
    }

    /// Fill the image from a full read of the store — the boot path. Returns
    /// how many runs were held. A store that cannot list its charter changes,
    /// or a row it cannot read, leaves no image: the reads go to the store.
    pub async fn rebuild(&self) -> Result<usize, SessionError> {
        let _gate = self.writing.lock().await;
        self.fill().await
    }

    async fn fill(&self) -> Result<usize, SessionError> {
        let read = async {
            let runs = self.inner.all_sessions().await?;
            let changes = self.inner.charter_changes().await?;
            let histories = self.inner.scan_session_history().await?;
            Ok::<_, SessionError>((runs, changes, histories))
        }
        .await;
        match read {
            Ok((runs, Some(changes), histories)) => {
                let held = runs.len();
                *self.image.write().expect("fold lock") = Some(Image::of(runs, changes, histories));
                self.wanted.store(true, Ordering::SeqCst);
                Ok(held)
            }
            Ok((_, None, _)) => {
                *self.image.write().expect("fold lock") = None;
                Ok(0)
            }
            Err(e) => {
                *self.image.write().expect("fold lock") = None;
                Err(e)
            }
        }
    }

    /// Answer from the image when one is held; `None` sends the read to the
    /// store.
    fn held<T>(&self, read: impl FnOnce(&Image) -> T) -> Option<T> {
        self.image.read().expect("fold lock").as_ref().map(read)
    }

    /// Bring the image up to date with a write that landed. A run the image
    /// does not hold, where the write needed one, drops the image rather than
    /// guessing; the next write fills it again.
    async fn follow(&self, verb: &str, apply: impl FnOnce(&mut Image) -> bool) {
        if !self.wanted.load(Ordering::SeqCst) {
            return;
        }
        let followed = {
            let mut slot = self.image.write().expect("fold lock");
            match slot.as_mut() {
                Some(image) => apply(image),
                None => false,
            }
        };
        if followed {
            return;
        }
        tracing::warn!(
            verb,
            "the in-memory image of the runs could not follow a write, so reads go to the store \
             until the next write fills it again"
        );
        if let Err(error) = self.fill().await {
            tracing::warn!(verb, %error, "the in-memory image of the runs could not be filled again");
        }
    }
}

#[async_trait]
impl Sessions for FoldedSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        match self.held(|image| {
            image
                .runs
                .iter()
                .filter(|run| &run.bot == bot)
                .cloned()
                .collect::<Vec<_>>()
        }) {
            Some(runs) => Ok(runs),
            None => self.inner.sessions_of(bot).await,
        }
    }

    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        match self.held(|image| {
            image
                .runs
                .iter()
                .filter(|run| &run.bot == bot)
                .map(SessionSummary::of)
                .collect::<Vec<_>>()
        }) {
            Some(summaries) => Ok(summaries),
            None => self.inner.summaries_of(bot).await,
        }
    }

    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        match self.held(|image| {
            image
                .runs
                .iter()
                .map(SessionSummary::of)
                .collect::<Vec<_>>()
        }) {
            Some(summaries) => Ok(summaries),
            None => self.inner.all_summaries().await,
        }
    }

    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        // An id that could never be a run's is no run, and is not an error.
        if validate_session_id(id).is_err() {
            return Ok(None);
        }
        match self.held(|image| image.run(id).map(SessionSummary::of)) {
            Some(summary) => Ok(summary),
            None => self.inner.summary_of(id).await,
        }
    }

    async fn run_counts(&self) -> Result<BTreeMap<EntityId, usize>, SessionError> {
        match self.held(|image| {
            let mut counts = BTreeMap::new();
            for run in &image.runs {
                *counts.entry(run.bot.clone()).or_insert(0) += 1;
            }
            counts
        }) {
            Some(counts) => Ok(counts),
            None => self.inner.run_counts().await,
        }
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        match self.held(|image| image.runs.clone()) {
            Some(runs) => Ok(runs),
            None => self.inner.all_sessions().await,
        }
    }

    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        self.inner.write_summary().await
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        match self.held(|image| image.run(id).cloned()) {
            Some(Some(run)) => Ok(run),
            Some(None) => Err(SessionError::UnknownSession {
                attempted: id.to_string(),
            }),
            None => self.inner.read_session(id).await,
        }
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        // **A begin whose outcome is uncertain drops the image.** Cancelled
        // while the store wrote, or answered with a store failure, the begin
        // may have committed: the run is then in the store and out of the
        // image, and a retry under the same sid writes nothing. Without the
        // image the retry fills it again from the store, so the image holds the
        // Begin the store holds rather than a second one stamped at the retry.
        let uncertain = Uncertain::armed(&self.image);
        let run = match self.inner.begin(new).await {
            Ok(run) => run,
            Err(e @ SessionError::Store(_)) => return Err(e),
            Err(e) => {
                uncertain.disarm();
                return Err(e);
            }
        };
        uncertain.disarm();
        self.follow("begin", |image| {
            // A retry of a begin that landed hands the run back and writes
            // nothing, so a run the image already holds begins no second time.
            let begun = image.run(&run.id).is_none();
            image.put(run.clone());
            if begun {
                image.record(&run.id, Change::Begin(&run.focus), at);
            }
            true
        })
        .await;
        Ok(run)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let written = self.inner.append(id, entry).await?;
        self.follow("append", |image| match image.run_mut(id) {
            Some(run) => {
                run.entries.push(written.clone());
                image.record(id, Change::Entry(&written.id), at);
                image.keep_wording(id, &written, at);
                true
            }
            None => false,
        })
        .await;
        Ok(written)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let written = self.inner.amend_last(id, text).await?;
        self.follow("amend_last", |image| {
            let replaced = replace_entry(image, id, &written);
            image.record(id, Change::AmendLast(&written.id), at);
            image.keep_wording(id, &written, at);
            replaced
        })
        .await;
        Ok(written)
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, moment) = self.moment();
        let written = self.inner.amend_beat(id, entry, text, at).await?;
        self.follow("amend_beat", |image| {
            let replaced = replace_entry(image, id, &written);
            image.record(id, Change::AmendBeat(&written.id), moment);
            image.keep_wording(id, &written, moment);
            replaced
        })
        .await;
        Ok(written)
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let run = self.inner.set_focus(id, focus).await?;
        self.put("set_focus", run, |image, run| {
            image.record(id, Change::Focus(&run.focus), at);
        })
        .await
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let run = self.inner.set_timezone(id, timezone).await?;
        self.put("set_timezone", run, |image, run| {
            image.record(id, Change::Timezone(run.timezone.as_deref()), at);
        })
        .await
    }

    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let run = self.inner.set_stated_day(id, day).await?;
        self.put("set_stated_day", run, |image, run| {
            image.record(id, Change::StatedDay(run.stated_day), at);
        })
        .await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let run = self.inner.set_wrap_window(id, window).await?;
        self.put("set_wrap_window", run, |image, run| {
            image.record(id, Change::WrapWindow(run.wrap_window.as_ref()), at);
        })
        .await
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        let had_window = self
            .held(|image| image.run(id).is_some_and(|run| run.wrap_window.is_some()))
            .unwrap_or(false);
        let run = self.inner.close(id, to).await?;
        self.put("close", run, |image, _| {
            // A close that ends an open window records the window going too.
            image.record(id, Change::State(to), at);
            if had_window {
                image.record(id, Change::WrapWindow(None), at);
            }
        })
        .await
    }

    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        self.inner.add_served(id, chars).await?;
        self.follow("add_served", |image| match image.run_mut(id) {
            Some(run) => {
                run.served_chars += chars;
                true
            }
            None => false,
        })
        .await;
        Ok(())
    }

    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        self.inner.reset_served(id).await?;
        self.follow("reset_served", |image| match image.run_mut(id) {
            Some(run) => {
                let total = std::mem::take(&mut run.served_chars);
                image.record(id, Change::ServedReset(total), at);
                true
            }
            None => false,
        })
        .await;
        Ok(())
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        // A run already open is resumed, which changes nothing and records
        // nothing: only a walk back from abandoned is a change.
        let was_abandoned = self
            .held(|image| {
                image
                    .run(id)
                    .is_some_and(|run| run.state == SessionState::Abandoned)
            })
            .unwrap_or(false);
        let run = self.inner.reopen(id).await?;
        self.put("reopen", run, |image, _| {
            if was_abandoned {
                image.record(id, Change::State(SessionState::Active), at);
            }
        })
        .await
    }

    async fn set_charter_told(&self, id: &SessionId, told: Timestamp) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        self.inner.set_charter_told(id, told).await?;
        self.follow("set_charter_told", |image| match image.run_mut(id) {
            Some(run) => {
                run.charter_told_at = Some(told);
                image.record(id, Change::CharterTold(told), at);
                true
            }
            None => false,
        })
        .await;
        Ok(())
    }

    async fn scan_session_history(&self) -> Result<SessionHistories, SessionError> {
        match self.held(|image| SessionHistories {
            changes: image
                .changes
                .iter()
                .map(|(id, kept)| (id.clone(), kept.clone()))
                .collect(),
            wordings: image
                .wordings
                .iter()
                .map(|((id, entry), kept)| (id.clone(), entry.clone(), kept.clone()))
                .collect(),
        }) {
            Some(all) => Ok(all),
            None => self.inner.scan_session_history().await,
        }
    }

    async fn session_history(&self, id: &SessionId) -> Result<Vec<SessionChange>, SessionError> {
        validate_session_id(id)?;
        match self.held(|image| {
            image
                .run(id)
                .map(|_| image.changes.get(id).cloned().unwrap_or_default())
        }) {
            Some(Some(changes)) => Ok(changes),
            Some(None) => Err(SessionError::UnknownSession {
                attempted: id.to_string(),
            }),
            None => self.inner.session_history(id).await,
        }
    }

    async fn journal_history(
        &self,
        id: &SessionId,
        entry: &EntryId,
    ) -> Result<Vec<Wording>, SessionError> {
        validate_session_id(id)?;
        match self.held(|image| {
            image.run(id).map(|run| {
                run.entries.iter().any(|e| &e.id == entry).then(|| {
                    image
                        .wordings
                        .get(&(id.clone(), entry.clone()))
                        .cloned()
                        .unwrap_or_default()
                })
            })
        }) {
            Some(Some(Some(wordings))) => Ok(wordings),
            Some(Some(None)) => Err(SessionError::NoEntries {
                attempted: id.to_string(),
            }),
            Some(None) => Err(SessionError::UnknownSession {
                attempted: id.to_string(),
            }),
            None => self.inner.journal_history(id, entry).await,
        }
    }

    async fn set_key_hash(&self, id: &SessionId, hash: &str) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        let (_held, at) = self.moment();
        self.inner.set_key_hash(id, hash).await?;
        self.follow("set_key_hash", |image| match image.run_mut(id) {
            Some(run) => {
                run.key_hash = Some(hash.to_string());
                image.record(id, Change::Key, at);
                true
            }
            None => false,
        })
        .await;
        Ok(())
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        self.inner
            .note_charter_change(bot, change.clone(), by_session)
            .await?;
        self.follow("note_charter_change", |image| {
            // The store keeps a bot's own rewrite apart from the change the
            // reads answer with, and the image follows the store.
            if change.by != *bot {
                image.charter_changes.insert(bot.clone(), change);
            }
            true
        })
        .await;
        Ok(())
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        match self.held(|image| image.charter_changes.get(bot).cloned()) {
            Some(change) => Ok(change),
            None => self.inner.charter_change_of(bot).await,
        }
    }

    async fn charter_changes(
        &self,
    ) -> Result<Option<BTreeMap<EntityId, CharterChange>>, SessionError> {
        self.inner.charter_changes().await
    }
}

impl FoldedSessions {
    /// Hold the run a write answered with, record what the write changed, and
    /// hand the run back.
    async fn put(
        &self,
        verb: &str,
        run: Session,
        record: impl FnOnce(&mut Image, &Session),
    ) -> Result<Session, SessionError> {
        self.follow(verb, |image| {
            image.put(run.clone());
            record(image, &run);
            true
        })
        .await;
        Ok(run)
    }
}

/// Replace the entry of that id in that run with the one the store answered.
fn replace_entry(image: &mut Image, id: &SessionId, written: &JournalEntry) -> bool {
    let Some(run) = image.run_mut(id) else {
        return false;
    };
    match run.entries.iter_mut().find(|e| e.id == written.id) {
        Some(held) => {
            *held = written.clone();
            true
        }
        None => false,
    }
}
