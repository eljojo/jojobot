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

use super::{CaptureRequest, Door, Measures, State};
use crate::memory::{MemoryError, NewFact, types::DeclaredType};

/// Fills the door for `fact` from the held State and the declarations in force.
pub type FillDoor =
    dyn Fn(&State, &[DeclaredType], &NewFact) -> Result<Door, MemoryError> + Send + Sync;

/// Renders the sizes of the request as an answer would carry them.
pub type Measure = dyn Fn(&State, &CaptureRequest) -> Measures + Send + Sync;

/// The two functions of one capture call.
#[derive(Clone)]
pub struct CaptureJob {
    pub door: Arc<FillDoor>,
    pub measure: Arc<Measure>,
}

tokio::task_local! {
    static JOB: CaptureJob;
}

/// Run `work` with `job` visible to the capture it makes.
pub async fn within<F: Future>(job: CaptureJob, work: F) -> F::Output {
    JOB.scope(job, work).await
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
