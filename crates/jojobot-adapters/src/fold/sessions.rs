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

use crate::writer::owned;
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
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, RunTotals, Session, SessionError,
    SessionId, SessionState, SessionSummary, Sessions, WrapWindow, validate_session_id,
};
use jojobot_domain::usage::{CallCount, CallKey, CallOutcome, RunCall, ServedDay};

/// The runs, their histories and the charter ledger when the store lists one, as
/// the store holds them.
struct Image {
    /// Every run, in the store's order: newest start first, the id breaking a
    /// tie.
    runs: Vec<Session>,
    /// Where each run sits in `runs`.
    at: BTreeMap<SessionId, usize>,
    /// The newest charter change another identity made to each bot. `None` when
    /// the store keeps no list to hand over: a bot's change is then asked of the
    /// store, and everything else stays in memory.
    charter_changes: Option<BTreeMap<EntityId, CharterChange>>,
    /// Every change of every run that has any, oldest first.
    changes: BTreeMap<SessionId, Vec<SessionChange>>,
    /// Every wording of every entry that has any, oldest first.
    wordings: BTreeMap<(SessionId, EntryId), Vec<Wording>>,
    /// How many calls each key has been counted under.
    calls: BTreeMap<CallKey, u64>,
    /// How many calls each run made to each verb, and how they ended.
    run_calls: BTreeMap<(String, String, CallOutcome), u64>,
    /// What each run was served on each day.
    served: BTreeMap<(SessionId, jiff::civil::Date), u64>,
}

impl Image {
    fn of(
        mut runs: Vec<Session>,
        charter_changes: Option<BTreeMap<EntityId, CharterChange>>,
        histories: SessionHistories,
        calls: Vec<CallCount>,
        run_calls: Vec<RunCall>,
        served: Vec<ServedDay>,
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
            calls: calls
                .into_iter()
                .map(|held| (held.key, held.count))
                .collect(),
            run_calls: run_calls
                .into_iter()
                .map(|held| ((held.run, held.verb, held.outcome), held.count))
                .collect(),
            served: served
                .into_iter()
                .map(|held| ((held.run, held.day), held.chars))
                .collect(),
        };
        image.reindex();
        image
    }

    /// A run's summary, with the compactions its kept changes hold.
    fn summary(&self, run: &Session) -> SessionSummary {
        SessionSummary::of(run, self.changes.get(&run.id).map_or(&[], Vec::as_slice))
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
            // The wordings are held oldest first, so the newest is the highest.
            held.last().map(|w| w.revision),
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
    jojobot_domain::session::history::newest_first((a.started_at, &a.id), (b.started_at, &b.id))
}

/// A [`Sessions`] wrapped in a RAM image of everything its reads answer.
#[derive(Clone)]
pub struct FoldedSessions {
    writer: crate::writer::Writer,
    inner: Arc<dyn Sessions>,
    /// The image, once a [`rebuild`](FoldedSessions::rebuild) has filled it;
    /// `None` sends every read to the store.
    image: Arc<RwLock<Option<Image>>>,
    /// Set by the first `rebuild`, whether or not its fill succeeded. A fill that
    /// failed, or a write the image could not follow, leaves no image, and this is
    /// what tells the next write to fill it again.
    wanted: Arc<AtomicBool>,
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
            writer: crate::writer::Writer::default(),
            inner,
            image: Arc::new(RwLock::new(None)),
            wanted: Arc::new(AtomicBool::new(false)),
            stamp,
            clock,
        }
    }

    /// Share the serving process's writer with every other rail.
    pub fn with_writer(mut self, writer: crate::writer::Writer) -> Self {
        self.writer = writer;
        self
    }

    /// A fixture's fence after ending a caller's wait for an accepted write.
    #[cfg(any(test, feature = "testing"))]
    pub async fn wait_for_writes(&self) {
        self.writer.drain().await;
    }

    /// **The moment of this write**, taken once and held for the store beneath
    /// until the returned guard drops.
    fn moment(&self) -> (jojobot_domain::session::history::HeldStamp, Timestamp) {
        let at = self.clock.now();
        (self.stamp.hold(at), at)
    }

    /// Fill the image from a full read of the store — the boot path. Returns
    /// how many runs were held. A row the store cannot read leaves no image: the
    /// reads go to the store. A store that keeps no list of its charter changes
    /// still fills the image; only a bot's charter change is asked of the store.
    pub async fn rebuild(&self) -> Result<usize, SessionError> {
        let this = self.clone();
        self.writer
            .run(async move {
                if this.writer.rail_uncertain("runs") {
                    return Err(SessionError::Store(
                        "the rail is uncertain; rebuild requires its fence".into(),
                    ));
                }
                // **Wanted from the first attempt, whether or not it succeeds**: a fill that
                // failed leaves no image, and the next write fills it again.
                this.wanted.store(true, Ordering::SeqCst);
                this.fill().await
            })
            .await
    }

    async fn fill(&self) -> Result<usize, SessionError> {
        let read = crate::writer::sql::reading(async {
            let runs = self.inner.all_sessions().await?;
            let changes = self.inner.charter_changes().await?;
            let histories = self.inner.scan_session_history().await?;
            let calls = self.inner.call_counts().await?;
            let run_calls = self.inner.run_calls(None).await?;
            let served = self.inner.served_days().await?;
            Ok::<_, SessionError>((runs, changes, histories, calls, run_calls, served))
        })
        .await;
        match read {
            Ok((runs, changes, histories, calls, run_calls, served)) => {
                let held = runs.len();
                *self.image.write().expect("fold lock") = Some(Image::of(
                    runs, changes, histories, calls, run_calls, served,
                ));
                Ok(held)
            }
            Err(e) => {
                if !self.writer.rail_uncertain("runs") {
                    *self.image.write().expect("fold lock") = None;
                }
                Err(e)
            }
        }
    }

    /// Keep the last published image until an uncertain write's rail has been
    /// fenced and reloaded through its boot loader.
    async fn scoped<T>(
        &self,
        write: impl std::future::Future<Output = Result<T, SessionError>>,
    ) -> Result<T, SessionError> {
        crate::writer::recovery::guarded(&self.writer, "runs", write, || self.fill()).await
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
    fn readable(&self) -> Result<(), SessionError> {
        if self.writer.rail_uncertain("runs") && self.image.read().expect("fold lock").is_none() {
            self.unheld()?;
        }
        Ok(())
    }

    fn unheld(&self) -> Result<(), SessionError> {
        if self.writer.rail_uncertain("runs") {
            return Err(SessionError::Store(
                "the uncertain rail has no published image".into(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl Sessions for FoldedSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.readable()?;
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
        self.readable()?;
        match self.held(|image| {
            image
                .runs
                .iter()
                .filter(|run| &run.bot == bot)
                .map(|run| image.summary(run))
                .collect::<Vec<_>>()
        }) {
            Some(summaries) => Ok(summaries),
            None => self.inner.summaries_of(bot).await,
        }
    }

    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.readable()?;
        match self.held(|image| {
            image
                .runs
                .iter()
                .map(|run| image.summary(run))
                .collect::<Vec<_>>()
        }) {
            Some(summaries) => Ok(summaries),
            None => self.inner.all_summaries().await,
        }
    }

    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        self.readable()?;
        // An id that could never be a run's is no run, and is not an error.
        if validate_session_id(id).is_err() {
            return Ok(None);
        }
        match self.held(|image| image.run(id).map(|run| image.summary(run))) {
            Some(summary) => Ok(summary),
            None => self.inner.summary_of(id).await,
        }
    }

    async fn run_totals(&self) -> Result<Vec<RunTotals>, SessionError> {
        self.readable()?;
        match self.held(|image| {
            jojobot_domain::session::run_totals_of(&image.runs, |id| {
                image.changes.get(id).map(Vec::as_slice)
            })
        }) {
            Some(totals) => Ok(totals),
            None => self.inner.run_totals().await,
        }
    }

    async fn served_days(&self) -> Result<Vec<ServedDay>, SessionError> {
        self.readable()?;
        match self.held(|image| {
            image
                .served
                .iter()
                .filter_map(|((run, day), chars)| {
                    Some(ServedDay {
                        run: run.clone(),
                        bot: image.run(run)?.bot.clone(),
                        day: *day,
                        chars: *chars,
                    })
                })
                .collect::<Vec<_>>()
        }) {
            Some(days) => Ok(days),
            None => self.inner.served_days().await,
        }
    }

    async fn call_counts(&self) -> Result<Vec<CallCount>, SessionError> {
        self.readable()?;
        match self.held(|image| {
            image
                .calls
                .iter()
                .map(|(key, count)| CallCount {
                    key: key.clone(),
                    count: *count,
                })
                .collect::<Vec<_>>()
        }) {
            Some(counts) => Ok(counts),
            None => self.inner.call_counts().await,
        }
    }

    async fn run_calls(&self, runs: Option<&[String]>) -> Result<Vec<RunCall>, SessionError> {
        self.readable()?;
        match self.held(|image| {
            image
                .run_calls
                .iter()
                .filter(|((run, _, _), _)| runs.is_none_or(|named| named.contains(run)))
                .map(|((run, verb, outcome), count)| RunCall {
                    run: run.clone(),
                    verb: verb.clone(),
                    outcome: outcome.clone(),
                    count: *count,
                })
                .collect::<Vec<_>>()
        }) {
            Some(counts) => Ok(counts),
            None => self.inner.run_calls(runs).await,
        }
    }

    async fn count_call(&self, key: &CallKey) -> Result<(), SessionError> {
        owned!(self, this, [key = key.clone()], this.count_call(&key).await);
        self.scoped(async {
            self.inner.count_call(key).await?;
            self.follow("count_call", |image| {
                *image.calls.entry(key.without_run()).or_default() += 1;
                if let Some(run) = &key.run {
                    *image
                        .run_calls
                        .entry((run.clone(), key.verb.clone(), key.outcome.clone()))
                        .or_default() += 1;
                }
                true
            })
            .await;
            Ok(())
        })
        .await
    }

    async fn run_counts(&self) -> Result<BTreeMap<EntityId, usize>, SessionError> {
        self.readable()?;
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
        self.readable()?;
        match self.held(|image| image.runs.clone()) {
            Some(runs) => Ok(runs),
            None => self.inner.all_sessions().await,
        }
    }

    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        self.unheld()?;
        self.inner.write_summary().await
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.readable()?;
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
        owned!(self, this, [], this.begin(new).await);
        // An unacknowledged begin reloads the rail under the fence. A same-sid
        // retry then sees the store's original Begin rather than stamping another.
        self.scoped(async {
            let (_held, at) = self.moment();
            let run = self.inner.begin(new).await?;
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
        })
        .await
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        owned!(self, this, [id = id.clone()], this.append(&id, entry).await);
        self.scoped(async {
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
        })
        .await
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), text = text.to_string()],
            this.amend_last(&id, &text).await
        );
        self.scoped(async {
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
        })
        .await
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        owned!(
            self,
            this,
            [
                id = id.clone(),
                entry = entry.clone(),
                text = text.to_string()
            ],
            this.amend_beat(&id, &entry, &text, at).await
        );
        self.scoped(async {
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
        })
        .await
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), focus = focus.to_string()],
            this.set_focus(&id, &focus).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            let run = self.inner.set_focus(id, focus).await?;
            self.put("set_focus", run, |image, run| {
                image.record(id, Change::Focus(&run.focus), at);
            })
            .await
        })
        .await
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), timezone = timezone.map(str::to_string)],
            this.set_timezone(&id, timezone.as_deref()).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            let run = self.inner.set_timezone(id, timezone).await?;
            self.put("set_timezone", run, |image, run| {
                image.record(id, Change::Timezone(run.timezone.as_deref()), at);
            })
            .await
        })
        .await
    }

    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone()],
            this.set_stated_day(&id, day).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            let run = self.inner.set_stated_day(id, day).await?;
            self.put("set_stated_day", run, |image, run| {
                image.record(id, Change::StatedDay(run.stated_day), at);
            })
            .await
        })
        .await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone()],
            this.set_wrap_window(&id, window).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            let run = self.inner.set_wrap_window(id, window).await?;
            self.put("set_wrap_window", run, |image, run| {
                image.record(id, Change::WrapWindow(run.wrap_window.as_ref()), at);
            })
            .await
        })
        .await
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        owned!(self, this, [id = id.clone()], this.close(&id, to).await);
        self.scoped(async {
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
        })
        .await
    }

    async fn close_displaced(
        &self,
        id: &SessionId,
        by: &jojobot_domain::session::Sid,
    ) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), by = by.clone()],
            this.close_displaced(&id, &by).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            let had_window = self
                .held(|image| image.run(id).is_some_and(|run| run.wrap_window.is_some()))
                .unwrap_or(false);
            let run = self.inner.close_displaced(id, by).await?;
            self.put("close_displaced", run, |image, _| {
                image.record(id, Change::State(SessionState::Abandoned), at);
                if had_window {
                    image.record(id, Change::WrapWindow(None), at);
                }
            })
            .await
        })
        .await
    }

    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError> {
        owned!(
            self,
            this,
            [id = id.clone()],
            this.add_served(&id, chars, day).await
        );
        self.scoped(async {
            self.inner.add_served(id, chars, day).await?;
            self.follow("add_served", |image| match image.run_mut(id) {
                Some(run) => {
                    run.served_chars += chars;
                    if chars > 0 {
                        *image.served.entry((id.clone(), day)).or_default() += chars;
                    }
                    true
                }
                None => false,
            })
            .await;
            Ok(())
        })
        .await
    }

    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        owned!(self, this, [id = id.clone()], this.reset_served(&id).await);
        self.scoped(async {
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
        })
        .await
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        owned!(self, this, [id = id.clone()], this.reopen(&id).await);
        self.scoped(async {
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
        })
        .await
    }

    async fn set_charter_told(&self, id: &SessionId, told: Timestamp) -> Result<(), SessionError> {
        owned!(
            self,
            this,
            [id = id.clone()],
            this.set_charter_told(&id, told).await
        );
        self.scoped(async {
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
        })
        .await
    }

    async fn scan_session_history(&self) -> Result<SessionHistories, SessionError> {
        self.readable()?;
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
        self.readable()?;
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
        self.readable()?;
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

    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        owned!(
            self,
            this,
            [
                id = id.clone(),
                hash = hash.to_string(),
                trail = trail.clone()
            ],
            this.set_key_hash(&id, &hash, &trail).await
        );
        self.scoped(async {
            let (_held, at) = self.moment();
            self.inner.set_key_hash(id, hash, trail).await?;
            self.follow("set_key_hash", |image| match image.run_mut(id) {
                Some(run) => {
                    run.key_hash = Some(hash.to_string());
                    run.key_trail = trail.clone();
                    image.record(id, Change::Key, at);
                    true
                }
                None => false,
            })
            .await;
            Ok(())
        })
        .await
    }

    /// **Straight through**: the fold holds the cards, and a minted run has none.
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), SessionError> {
        owned!(
            self,
            this,
            [run = run.clone()],
            this.note_minted(&run).await
        );
        self.scoped(self.inner.note_minted(run)).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, SessionError> {
        self.unheld()?;
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(&self, before: jiff::Timestamp) -> Result<usize, SessionError> {
        owned!(self, this, [], this.prune_minted(before).await);
        self.scoped(self.inner.prune_minted(before)).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        owned!(
            self,
            this,
            [
                bot = bot.clone(),
                by_session = by_session.map(str::to_string)
            ],
            this.note_charter_change(&bot, change, by_session.as_deref())
                .await
        );
        self.scoped(async {
            self.inner
                .note_charter_change(bot, change.clone(), by_session)
                .await?;
            self.follow("note_charter_change", |image| {
                // The store keeps a bot's own rewrite apart from the change the
                // reads answer with, and the image follows the store.
                if change.by != *bot
                    && let Some(held) = image.charter_changes.as_mut()
                {
                    held.insert(bot.clone(), change);
                }
                true
            })
            .await;
            Ok(())
        })
        .await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.readable()?;
        // A store that lists no changes is asked bot by bot, image or not.
        match self.held(|image| {
            image
                .charter_changes
                .as_ref()
                .map(|held| held.get(bot).cloned())
        }) {
            Some(Some(change)) => Ok(change),
            _ => {
                self.unheld()?;
                self.inner.charter_change_of(bot).await
            }
        }
    }

    async fn charter_changes(
        &self,
    ) -> Result<Option<BTreeMap<EntityId, CharterChange>>, SessionError> {
        self.readable()?;
        if self.writer.rail_uncertain("runs") {
            if let Some(Some(held)) = self.held(|image| image.charter_changes.clone()) {
                return Ok(Some(held));
            }
            self.unheld()?;
        }
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
