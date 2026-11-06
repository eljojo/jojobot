//! **What the verb layer hands the writer's capture job.** Two pure functions,
//! called inside the writer's job against the State it holds: one fills the
//! door (the writer's grants, runs and ageing, and the owned keys of the
//! subject's kind), the other renders the sizes of the request. The port's
//! `capture` still takes only the claim; these travel beside it on the call's
//! context, as the request does, so the claim's path through every wrapper of
//! the port is unchanged.
//!
//! A capture that reaches the store with no job fails: the store never falls
//! back to a door of its own.

use std::future::Future;
use std::sync::Arc;

use super::{BootFloor, CaptureRequest, Door, Measures, State};
use crate::memory::{MemoryError, NewFact, types::DeclaredType};

/// Fills the door for `fact` from the held State and the declarations in force.
pub type FillDoor =
    dyn Fn(&State, &[DeclaredType], &NewFact) -> Result<Door, MemoryError> + Send + Sync;

/// Renders the sizes of the request as an answer would carry them.
pub type Measure = dyn Fn(&State, &CaptureRequest) -> Measures + Send + Sync;

/// **What a bot's boot weighs with and without the claim**, rendered by today's
/// boot render, which reads the box through the port. Called only for a write that
/// stars a rule or sets a seat count ([`CaptureRequest::could_grow_the_boot_floor`]).
/// It reaches no write. `None` is a boot that could not be read, and passes.
///
/// TEMPORARY(9.3.7): it retires when the boot renders from the State.
pub type BootFloorMeasure = dyn Fn(&CaptureRequest) -> std::pin::Pin<Box<dyn Future<Output = Option<BootFloor>> + Send>>
    + Send
    + Sync;

/// **Which bot each run belongs to**, from the sessions the process holds. Called
/// with the runs that first wrote the claims on the subject and the run the
/// request is written in, only for a write that could be a thought
/// ([`CaptureRequest::could_be_a_thought`]). It reaches no write.
///
/// TEMPORARY(9.3.7): sessions reach the log then, and the door reads the image.
pub type RunBotsMeasure = dyn Fn(
        Vec<crate::log::RunId>,
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                    Output = std::collections::BTreeMap<crate::log::RunId, crate::memory::EntityId>,
                > + Send,
        >,
    > + Send
    + Sync;

/// The functions of one capture call.
#[derive(Clone)]
pub struct CaptureJob {
    pub door: Arc<FillDoor>,
    pub measure: Arc<Measure>,
    pub boot_floor: Arc<BootFloorMeasure>,
    pub run_bots: Arc<RunBotsMeasure>,
}

tokio::task_local! {
    static JOB: CaptureJob;
}

/// Run `work` with `job` visible to the capture it makes.
pub async fn within<F: Future>(job: CaptureJob, work: F) -> F::Output {
    JOB.scope(job, work).await
}

/// **A fresh identity for the claim a capture creates.** Drawn from the system's
/// entropy: the State holds no counter for it, and an imported record's identity
/// is made from the thing and its label, so the two cannot meet.
pub fn fresh_record() -> crate::log::RecordId {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system draws entropy");
    crate::log::RecordId(format!(
        "new-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

/// The job of the call in flight, when it carries one.
pub fn current() -> Option<CaptureJob> {
    JOB.try_with(Clone::clone).ok()
}

/// Run `work` under a job taken from [`current`] on another task.
pub(crate) async fn carried<F: Future>(job: Option<CaptureJob>, work: F) -> F::Output {
    match job {
        Some(job) => JOB.scope(job, work).await,
        None => work.await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::context::Context;

    fn job() -> CaptureJob {
        CaptureJob {
            door: Arc::new(|_, _, _| Err(MemoryError::Store("no door in this case".into()))),
            measure: Arc::new(|_, _| Measures::default()),
            boot_floor: Arc::new(|_| Box::pin(async { None })),
            run_bots: Arc::new(|_| Box::pin(async { Default::default() })),
        }
    }

    /// **The job travels with a write onto its owned task**, as the request
    /// does: a write handed to the writer's task still finds it.
    #[tokio::test]
    async fn the_job_reaches_the_task_that_owns_the_write() {
        assert!(
            current().is_none(),
            "no job outside a call that carries one"
        );
        let context = within(job(), async { Context::capture() }).await;
        let found = tokio::spawn(context.run(async { current().is_some() }))
            .await
            .expect("the owned task ends");
        assert!(found, "the owned task sees the job of the call");

        let bare = Context::capture();
        let found = tokio::spawn(bare.run(async { current().is_some() }))
            .await
            .expect("the owned task ends");
        assert!(!found, "a call with no job leaves none on the owned task");
    }
}
