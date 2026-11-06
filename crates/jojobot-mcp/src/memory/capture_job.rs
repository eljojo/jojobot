//! **What the verb layer hands a capture's job.** The domain builds every job
//! ([`CaptureJob::new`]) and owns the door, which is pure. Two parts only the
//! answer's layer can compute are supplied from here: the size of a claim as a
//! read serves it, and the weight of a bot's boot. This file shrinks to those two
//! for the verb, the app path and the engine.
//!
//! The boot render reads through the port and writes nothing: TEMPORARY(9.3.7), it
//! retires when the boot renders from the State.

use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::job::{BootRender, CaptureJob, SizeMeasure};

use super::reads_left::ReadsLeft;
use super::*;

/// The size of a claim as a read serves it, on the thing as the State holds it.
fn size_measure(clock: &Clock) -> Arc<SizeMeasure> {
    let clock = *clock;
    Arc::new(move |state, new| Jojobot::measure_new_claim(state, new, clock.now()))
}

/// **The job of jojobot's own captures**: the ledger of release notes it keeps for
/// a bot, and any other record the engine writes for itself. Built once where the
/// stack is assembled and scoped around the engine's writes. The actor is jojobot
/// itself (rule 731); the engine stars no rule and sets no seat count, so it
/// weighs no boot.
pub fn engine_capture_job(clock: &Clock) -> CaptureJob {
    CaptureJob::new(
        &Actor::System("jojobot".into()),
        None,
        size_measure(clock),
        None,
    )
}

impl Jojobot {
    /// The job of the captures one call makes, written by `actor`.
    pub(crate) fn capture_job(&self, actor: &Actor, reads: Arc<ReadsLeft>) -> CaptureJob {
        let this = self.clone();
        let boot: Arc<BootRender> = Arc::new(move |new| {
            let (this, new, reads) = (this.clone(), new.clone(), Arc::clone(&reads));
            Box::pin(async move { this.boot_floor_measure_for_capture(&new, &reads).await })
        });
        CaptureJob::new(
            actor,
            Some(Arc::clone(&self.sessions)),
            size_measure(self.clock()),
            Some(boot),
        )
    }
}
