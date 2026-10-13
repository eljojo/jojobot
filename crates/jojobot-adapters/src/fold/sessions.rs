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
use jojobot_domain::memory::EntityId;
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
}

impl Image {
    fn of(mut runs: Vec<Session>, charter_changes: BTreeMap<EntityId, CharterChange>) -> Self {
        runs.sort_by(order);
        let mut image = Image {
            runs,
            at: BTreeMap::new(),
            charter_changes,
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
}

impl FoldedSessions {
    /// Wrap a store with **no image**. Every read answers correctly before a
    /// rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Sessions>) -> Self {
        FoldedSessions {
            inner,
            image: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
            wanted: AtomicBool::new(false),
        }
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
            Ok::<_, SessionError>((runs, changes))
        }
        .await;
        match read {
            Ok((runs, Some(changes))) => {
                let held = runs.len();
                *self.image.write().expect("fold lock") = Some(Image::of(runs, changes));
                self.wanted.store(true, Ordering::SeqCst);
                Ok(held)
            }
            Ok((_, None)) => {
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
        self.inner.all_summaries().await
    }

    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        self.inner.summary_of(id).await
    }

    async fn run_counts(&self) -> Result<BTreeMap<EntityId, usize>, SessionError> {
        self.inner.run_counts().await
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await
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
        let run = self.inner.begin(new).await?;
        self.follow("begin", |image| {
            image.put(run.clone());
            true
        })
        .await;
        Ok(run)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        let _gate = self.writing.lock().await;
        let written = self.inner.append(id, entry).await?;
        self.follow("append", |image| match image.run_mut(id) {
            Some(run) => {
                run.entries.push(written.clone());
                true
            }
            None => false,
        })
        .await;
        Ok(written)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        let _gate = self.writing.lock().await;
        let written = self.inner.amend_last(id, text).await?;
        self.follow("amend_last", |image| replace_entry(image, id, &written))
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
        let written = self.inner.amend_beat(id, entry, text, at).await?;
        self.follow("amend_beat", |image| replace_entry(image, id, &written))
            .await;
        Ok(written)
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.set_focus(id, focus).await?;
        self.put("set_focus", run).await
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.set_timezone(id, timezone).await?;
        self.put("set_timezone", run).await
    }

    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.set_stated_day(id, day).await?;
        self.put("set_stated_day", run).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.set_wrap_window(id, window).await?;
        self.put("set_wrap_window", run).await
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.close(id, to).await?;
        self.put("close", run).await
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

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        let _gate = self.writing.lock().await;
        let run = self.inner.reopen(id).await?;
        self.put("reopen", run).await
    }

    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        self.inner.set_charter_told(id, at).await?;
        self.follow("set_charter_told", |image| match image.run_mut(id) {
            Some(run) => {
                run.charter_told_at = Some(at);
                true
            }
            None => false,
        })
        .await;
        Ok(())
    }

    async fn set_key_hash(&self, id: &SessionId, hash: &str) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        self.inner.set_key_hash(id, hash).await?;
        self.follow("set_key_hash", |image| match image.run_mut(id) {
            Some(run) => {
                run.key_hash = Some(hash.to_string());
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
    ) -> Result<(), SessionError> {
        let _gate = self.writing.lock().await;
        self.inner.note_charter_change(bot, change.clone()).await?;
        self.follow("note_charter_change", |image| {
            image.charter_changes.insert(bot.clone(), change);
            true
        })
        .await;
        Ok(())
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.inner.charter_change_of(bot).await
    }

    async fn charter_changes(
        &self,
    ) -> Result<Option<BTreeMap<EntityId, CharterChange>>, SessionError> {
        self.inner.charter_changes().await
    }
}

impl FoldedSessions {
    /// Hold the run a write answered with, and hand it back.
    async fn put(&self, verb: &str, run: Session) -> Result<Session, SessionError> {
        self.follow(verb, |image| {
            image.put(run.clone());
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
