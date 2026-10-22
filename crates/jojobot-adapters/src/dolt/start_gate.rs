//! **How many stores may be starting at once in one process.**
//!
//! A store's server spends its first seconds on the machine's cpu and disk: it
//! creates its files and its root account before it listens. A test run starts
//! one store per case, a case at a time per core, so several servers are in that
//! stretch together. Under a job that shares the machine the stretch outlasts the
//! 30 seconds `Dolt::start` waits for the first answer, and the case dies with
//! `NeverReady` although nothing is wrong with the store.
//!
//! The gate bounds the stretch, not the life of a store: a permit is held from
//! the start of a start until it returns, and the wait for a permit is outside
//! the wait for the server. A case that needs two stores at once can therefore
//! never deadlock on the gate, because the first store's permit is back before
//! the second asks. A process that starts one store passes through it without
//! waiting.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Semaphore, SemaphorePermit};

/// **Stores a process may be starting at once.**
pub(super) const CONCURRENT_STARTS: usize = 4;

/// A bound on starts in flight, and the most that were ever in flight at once.
pub(super) struct Gate {
    room: Semaphore,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
}

/// A start's place in the gate; it frees the place when it is dropped.
pub(super) struct Place<'gate> {
    gate: &'gate Gate,
    _permit: SemaphorePermit<'gate>,
}

impl Gate {
    pub(super) const fn new(bound: usize) -> Self {
        Gate {
            room: Semaphore::const_new(bound),
            in_flight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// Waits for room, then takes a place.
    pub(super) async fn enter(&self) -> Place<'_> {
        let permit = self.room.acquire().await.expect("the gate is never closed");
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        Place {
            gate: self,
            _permit: permit,
        }
    }

    /// The most starts that were in flight together since the gate was made.
    #[cfg(test)]
    pub(super) fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        self.gate.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The gate every start in this process goes through.
pub(super) fn process_gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| Gate::new(CONCURRENT_STARTS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::Dolt;
    use crate::testing::Scratch;
    use std::time::Duration;

    /// **A gate lets its bound through together and no more, and everyone gets
    /// through.** The positive is that the bound is reached: a gate that let one
    /// start in at a time would pass the upper limit and fail this.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_gate_admits_its_bound_at_once_and_everyone_in_the_end() {
        static GATE: Gate = Gate::new(3);
        let mut starts = Vec::new();
        for _ in 0..9 {
            starts.push(tokio::spawn(async {
                let _place = GATE.enter().await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }));
        }
        for start in starts {
            start.await.expect("a start that waited got through");
        }
        assert_eq!(GATE.peak(), 3, "three at once, never more");
    }

    /// **Real stores started together by more cases than the bound all come up,
    /// and never more than the bound were starting at once.** This travels
    /// `Dolt::start`, the path every case takes, so a start that skipped the
    /// gate would show in the peak.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn more_real_stores_than_the_bound_start_together_and_all_come_up() {
        let mut starts = Vec::new();
        for n in 0..CONCURRENT_STARTS * 2 {
            starts.push(tokio::spawn(async move {
                let scratch = Scratch::new(&format!("start-gate-{n}"));
                let mut store = Dolt::start(&scratch.0, crate::testing::free_port())
                    .await
                    .expect("the store comes up");
                store.stop().await;
            }));
        }
        for start in starts {
            start.await.expect("every store came up");
        }
        let peak = process_gate().peak();
        assert!(
            (1..=CONCURRENT_STARTS).contains(&peak),
            "starts in flight together: {peak}"
        );
    }
}
