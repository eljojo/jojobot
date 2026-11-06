//! **When the serving process takes a snapshot of the log's State.** The design
//! (event log, section 7) names three moments: after a family import, after
//! [`CADENCE`] events or minutes of unsnapshotted writes, and on an orderly
//! shutdown with writes not yet covered. The snapshot itself is
//! [`take_snapshot`]: it rebuilds the State from the log's newest valid snapshot
//! plus its tail, so the view is coherent at one position. Periodic work does not
//! hold the writer's queue; the shutdown snapshot is its last job. This type only
//! decides when to call it. The numbers are engineering defaults to measure, not settings.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jojobot_domain::log::{LogError, LogStore, Position};

use crate::log_snapshot::take_snapshot;

/// How much unsnapshotted work makes a snapshot due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cadence {
    /// Committed events past the newest snapshot.
    pub events: u64,
    /// Time since the log was first seen holding an event past the newest snapshot.
    pub dirty_for: Duration,
}

/// The design's initial cadence.
pub const CADENCE: Cadence = Cadence {
    events: 10_000,
    dirty_for: Duration::from_secs(5 * 60),
};

/// Why a snapshot is taken now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Events,
    Dirty,
}

struct Kept {
    covered: Position,
    dirty_since: Option<Instant>,
}

pub struct SnapshotKeeper {
    log: Arc<dyn LogStore>,
    cadence: Cadence,
    kept: Mutex<Kept>,
}

impl SnapshotKeeper {
    /// Start from the newest snapshot the log holds, if any.
    pub async fn open(log: Arc<dyn LogStore>, cadence: Cadence) -> Result<Self, LogError> {
        // A snapshot of another build is returned when this build has none; it
        // covers nothing here, or this build would never publish its own.
        let covered = log
            .load_snapshot()
            .await?
            .filter(|snapshot| {
                snapshot.projection_version == crate::log_snapshot::PROJECTION_FINGERPRINT
            })
            .map_or(Position(0), |snapshot| snapshot.covered_position);
        Ok(Self {
            log,
            cadence,
            kept: Mutex::new(Kept {
                covered,
                dirty_since: None,
            }),
        })
    }

    /// The position the newest snapshot this keeper knows covers.
    pub fn covered(&self) -> Position {
        self.kept
            .lock()
            .expect("the keeper is not poisoned")
            .covered
    }

    /// Whether a snapshot is due with the log at `head`. The first call that sees
    /// the head past the snapshot starts the dirty clock.
    pub fn due(&self, head: Position, now: Instant) -> Option<Why> {
        let mut kept = self.kept.lock().expect("the keeper is not poisoned");
        if head <= kept.covered {
            kept.dirty_since = None;
            return None;
        }
        let since = *kept.dirty_since.get_or_insert(now);
        if head.0 - kept.covered.0 >= self.cadence.events {
            Some(Why::Events)
        } else if now.saturating_duration_since(since) >= self.cadence.dirty_for {
            Some(Why::Dirty)
        } else {
            None
        }
    }

    /// Take a snapshot when one is due.
    pub async fn keep(
        &self,
        head: Position,
        now: Instant,
        at: jiff::Timestamp,
    ) -> Result<Option<Position>, LogError> {
        match self.due(head, now) {
            Some(_) => self.take(at).await.map(Some),
            None => Ok(None),
        }
    }

    /// Take a snapshot when the log holds anything the newest one does not cover:
    /// after a family import and on an orderly shutdown.
    pub async fn flush(
        &self,
        head: Position,
        at: jiff::Timestamp,
    ) -> Result<Option<Position>, LogError> {
        if head <= self.covered() {
            return Ok(None);
        }
        self.take(at).await.map(Some)
    }

    /// **Keep the log's snapshot while the process serves.** A tick asks whether
    /// one is due; when `stop` ends, the writer owns the final flush after its
    /// other jobs have drained. A failed snapshot is logged and tried again at
    /// the next tick: a snapshot is disposable, so a miss costs boot time and nothing else.
    pub async fn run(
        &self,
        head: impl Fn() -> Option<Position>,
        every: Duration,
        stop: impl std::future::Future<Output = ()>,
    ) {
        tokio::pin!(stop);
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = &mut stop => break,
                _ = ticks.tick() => {
                    let Some(head) = head() else { continue };
                    match self.keep(head, Instant::now(), jiff::Timestamp::now()).await {
                        Ok(Some(covered)) => tracing::info!(covered = covered.0, "snapshot taken"),
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(%error, "snapshot not taken; the next tick tries again")
                        }
                    }
                }
            }
        }
    }

    async fn take(&self, at: jiff::Timestamp) -> Result<Position, LogError> {
        let covered = take_snapshot(self.log.as_ref(), at).await?;
        let mut kept = self.kept.lock().expect("the keeper is not poisoned");
        kept.covered = kept.covered.max(covered);
        kept.dirty_since = None;
        Ok(covered)
    }
}

/// How often the serving loop asks whether a snapshot is due.
pub const TICK: Duration = Duration::from_secs(30);

/// **The snapshots of the log the process serves from**: the keeper, and the
/// writer's log it reads the head of.
pub struct ServingSnapshots {
    keeper: SnapshotKeeper,
    log: Arc<crate::writer_log::WriterLog>,
}

impl ServingSnapshots {
    pub async fn open(log: Arc<crate::writer_log::WriterLog>) -> Result<Self, LogError> {
        let keeper = SnapshotKeeper::open(log.clone(), CADENCE).await?;
        Ok(Self { keeper, log })
    }

    /// The head the log's writer holds.
    pub fn head(&self) -> Option<Position> {
        self.log.head()
    }

    pub fn covered(&self) -> Position {
        self.keeper.covered()
    }

    /// A family import landed: cover it, so the next boot does not replay it.
    /// A failure is logged and left to the cadence.
    pub async fn after_import(&self) {
        let Some(head) = self.head() else { return };
        match self.keeper.flush(head, jiff::Timestamp::now()).await {
            Ok(Some(covered)) => tracing::info!(covered = covered.0, "snapshot taken after import"),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "no snapshot after import; the cadence tries later")
            }
        }
    }

    /// Serve until `stop` ends. The writer takes the shutdown snapshot separately.
    pub async fn run(&self, stop: impl std::future::Future<Output = ()>) {
        self.keeper.run(|| self.head(), TICK, stop).await;
    }

    /// Called as the writer's last job, after its other writes have drained.
    pub async fn shutdown(&self) {
        let Some(head) = self.head() else { return };
        match self.keeper.flush(head, jiff::Timestamp::now()).await {
            Ok(Some(covered)) => tracing::info!(covered = covered.0, "snapshot taken at shutdown"),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "no snapshot at shutdown; the next boot replays the tail")
            }
        }
    }
}

#[cfg(test)]
mod serving_tests;
#[cfg(test)]
mod tests;
