//! **When the serving process takes a snapshot of the log's State.** The design
//! (event log, section 7) names three moments: after a full replay or family
//! import, after [`CADENCE`] events, and on an orderly shutdown with writes not
//! yet covered. The snapshot itself is
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
}

/// The design's initial cadence.
pub const CADENCE: Cadence = Cadence { events: 10_000 };

/// Why a snapshot is taken now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Events,
}

struct Kept {
    covered: Position,
}

pub struct SnapshotKeeper {
    log: Arc<dyn LogStore>,
    cadence: Cadence,
    kept: Mutex<Kept>,
}

impl SnapshotKeeper {
    /// Start from the position the boot's snapshot covered, as
    /// [`crate::log_snapshot::credited`] reads it off the boot: a restored snapshot
    /// or one successfully published from the boot's full replay earns coverage.
    /// A full replay whose snapshot publication failed earns zero coverage.
    pub async fn open(
        log: Arc<dyn LogStore>,
        cadence: Cadence,
        covered: Position,
    ) -> Result<Self, LogError> {
        Ok(Self {
            log,
            cadence,
            kept: Mutex::new(Kept { covered }),
        })
    }

    /// The position the newest snapshot this keeper knows covers.
    pub fn covered(&self) -> Position {
        self.kept
            .lock()
            .expect("the keeper is not poisoned")
            .covered
    }

    /// Whether enough events have committed past the newest snapshot.
    pub fn due(&self, head: Position, _now: Instant) -> Option<Why> {
        let kept = self.kept.lock().expect("the keeper is not poisoned");
        if head <= kept.covered {
            return None;
        }
        if head.0 - kept.covered.0 >= self.cadence.events {
            Some(Why::Events)
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
    pub async fn open(
        log: Arc<crate::writer_log::WriterLog>,
        covered: Position,
    ) -> Result<Self, LogError> {
        let keeper = SnapshotKeeper::open(log.clone(), CADENCE, covered).await?;
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
