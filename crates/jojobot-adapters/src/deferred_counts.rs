//! **A call's bookkeeping never makes its caller wait on the writer.** Counting a
//! call and adding what it served are writes, and every write goes through the one
//! writer, one job at a time. A read that awaited its own count was answered only
//! after every write queued before it.
//!
//! Both go on one ordered lane instead, and the call is answered at once. The
//! reads that show a count (and the verb that resets a total) wait for the lane to
//! run dry first, so a count is never read before it lands and an answer counted
//! before a reset never lands after it. A read a caller waits on for its answer
//! does not wait: that is the whole point.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, OnceLock};

use futures_util::FutureExt;
use jiff::civil::Date;
use jojobot_domain::session::{SessionId, Sessions};
use jojobot_domain::usage::{CallCount, CallKey, RunCall, ServedDay};
use tokio::sync::{mpsc, oneshot};

enum Work {
    Count(CallKey),
    Served(SessionId, u64, Date),
    Settle(oneshot::Sender<()>),
}

/// The sessions port, with the bookkeeping of a call taken off the path of the
/// answer.
pub struct DeferredCounts {
    inner: Arc<dyn Sessions>,
    /// Opened by the first count, so port assembly needs no executor.
    lane: OnceLock<mpsc::UnboundedSender<Work>>,
}

impl DeferredCounts {
    pub fn new(inner: Arc<dyn Sessions>) -> Self {
        Self {
            inner,
            lane: OnceLock::new(),
        }
    }

    fn queue(&self, work: Work) {
        let lane = self.lane.get_or_init(|| {
            let (sender, mut queued) = mpsc::unbounded_channel::<Work>();
            let inner = self.inner.clone();
            tokio::spawn(async move {
                while let Some(work) = queued.recv().await {
                    // One count that panics must not stop the lane for every
                    // later one.
                    let ran = AssertUnwindSafe(Self::apply(&inner, work))
                        .catch_unwind()
                        .await;
                    if ran.is_err() {
                        tracing::error!("a call's count panicked; the lane keeps serving");
                    }
                }
            });
            sender
        });
        // The worker holds the receiver until every sender is gone.
        let _ = lane.send(work);
    }

    async fn apply(inner: &Arc<dyn Sessions>, work: Work) {
        match work {
            Work::Count(key) => {
                if let Err(error) = inner.count_call(&key).await {
                    tracing::warn!(verb = %key.verb, %error, "a call was not counted: the store refused the count");
                }
            }
            Work::Served(id, chars, day) => {
                let _ = inner.add_served(&id, chars, day).await;
            }
            Work::Settle(done) => {
                let _ = done.send(());
            }
        }
    }

    /// Wait until every count queued so far has reached the store.
    pub async fn settle(&self) {
        let Some(lane) = self.lane.get() else {
            return;
        };
        let (done, waiting) = oneshot::channel();
        if lane.send(Work::Settle(done)).is_ok() {
            let _ = waiting.await;
        }
    }
}

#[jojobot_domain::forward(jojobot_domain::forward_sessions, via = inner)]
impl Sessions for DeferredCounts {
    async fn count_call(&self, key: &CallKey) -> Result<(), SessionError> {
        self.queue(Work::Count(key.clone()));
        Ok(())
    }

    async fn add_served(&self, id: &SessionId, chars: u64, day: Date) -> Result<(), SessionError> {
        self.queue(Work::Served(id.clone(), chars, day));
        Ok(())
    }

    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.settle().await;
        self.inner.reset_served(id).await
    }

    async fn run_totals(&self) -> Result<Vec<RunTotals>, SessionError> {
        self.settle().await;
        self.inner.run_totals().await
    }

    async fn call_counts(&self) -> Result<Vec<CallCount>, SessionError> {
        self.settle().await;
        self.inner.call_counts().await
    }

    async fn served_days(&self) -> Result<Vec<ServedDay>, SessionError> {
        self.settle().await;
        self.inner.served_days().await
    }

    async fn run_calls(&self, runs: Option<&[String]>) -> Result<Vec<RunCall>, SessionError> {
        self.settle().await;
        self.inner.run_calls(runs).await
    }

    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        self.settle().await;
        self.inner.summaries_of(bot).await
    }

    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.settle().await;
        self.inner.all_summaries().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::sessions::FoldedSessions;
    use crate::writer::Writer;
    use jojobot_domain::clock::Clock;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::usage::CallOutcome;
    use std::time::Duration;

    fn a_call() -> CallKey {
        CallKey {
            bot: None,
            day: jiff::civil::date(2026, 11, 1),
            verb: "recall".into(),
            outcome: CallOutcome::Landed,
            run: None,
        }
    }

    /// The counting port over a folded image on a writer of its own, and that
    /// writer, with a write held in it until `release` is notified.
    async fn behind_a_held_write() -> (
        DeferredCounts,
        tokio::task::JoinHandle<()>,
        Arc<tokio::sync::Notify>,
        jojobot_domain::session::Session,
    ) {
        let store = Arc::new(InMemorySessions::new());
        // Begun before the hold: beginning a run is itself a write on the writer.
        let run = jojobot_domain::session::testing::contract::begin(
            store.as_ref(),
            "gamma",
            "reading",
            0,
        )
        .await;
        let writer = Writer::default();
        let folded = Arc::new(
            FoldedSessions::new(store.clone(), store.write_stamp(), Clock::Real)
                .with_writer(writer.clone()),
        );
        folded.rebuild().await.expect("the image loads");
        let counting = DeferredCounts::new(folded);

        let release = Arc::new(tokio::sync::Notify::new());
        let before = writer.submitted_jobs();
        let held = tokio::spawn({
            let (writer, release) = (writer.clone(), release.clone());
            async move { writer.run(async move { release.notified().await }).await }
        });
        while writer.submitted_jobs() == before {
            tokio::task::yield_now().await;
        }
        (counting, held, release, run)
    }

    /// **A counted call is answered while a slow write holds the writer, and it is
    /// counted exactly once when the write ends.** A write is held in the writer;
    /// the count is asked for behind it and must return at once; the held write is
    /// released, the counts settle, and the count is one.
    #[tokio::test]
    async fn a_counted_call_does_not_wait_behind_a_slow_write_and_is_counted_exactly() {
        let (counting, held, release, _) = behind_a_held_write().await;

        tokio::time::timeout(Duration::from_secs(1), counting.count_call(&a_call()))
            .await
            .expect("counting a call waited for the writer")
            .expect("the count is accepted");

        release.notify_one();
        held.await.expect("the slow write ends");
        counting.settle().await;
        let counts = counting.call_counts().await.expect("the counts read");
        assert_eq!(counts.len(), 1, "{counts:?}");
        assert_eq!(counts[0].count, 1, "{counts:?}");
    }

    /// **A read of the counts waits for the counts queued before it**, so a count
    /// is never read before it lands. Paired with the case above, which proves the
    /// call itself does not wait: here the count is still behind the held write when
    /// the write ends, and the read, with no settle of its own, must see it.
    #[tokio::test]
    async fn a_read_of_the_counts_sees_every_count_queued_before_it() {
        let (counting, held, release, _) = behind_a_held_write().await;
        counting
            .count_call(&a_call())
            .await
            .expect("the count is accepted");

        release.notify_one();
        held.await.expect("the slow write ends");
        let counts = counting.call_counts().await.expect("the counts read");
        assert_eq!(counts.len(), 1, "{counts:?}");
        assert_eq!(counts[0].count, 1, "{counts:?}");
    }

    /// **The starts of a bot's runs are answered while a slow write holds the
    /// writer and a count waits behind it.** The room of a bot's thoughts and the
    /// age of a thought ask only when runs began, so they must not wait for the
    /// counts that `summaries_of` waits for. Paired with the case above: the same
    /// held write, the same queued count, and a read that is answered at once.
    #[tokio::test]
    async fn the_starts_of_runs_are_answered_without_waiting_for_a_queued_count() {
        let (counting, held, release, run) = behind_a_held_write().await;
        counting
            .count_call(&a_call())
            .await
            .expect("the count is accepted");

        let starts =
            tokio::time::timeout(Duration::from_secs(1), counting.run_starts(Some(&run.bot)))
                .await
                .expect("reading when runs began waited for the writer")
                .expect("the starts read");
        assert_eq!(starts.len(), 1, "{starts:?}");
        assert_eq!(starts[0].started_at, run.started_at, "{starts:?}");

        release.notify_one();
        held.await.expect("the slow write ends");
    }
}
