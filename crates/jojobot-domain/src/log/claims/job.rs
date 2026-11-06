//! **What a capture hands the writer's job.** Functions called inside the writer's
//! job against the State it holds: one fills the door (the writer's grants, runs
//! and ageing, and the owned keys of the subject's kind), one renders the sizes of
//! the request, one weighs a bot's boot, one tells which bot a run belongs to. The
//! port's `capture` still takes only the claim; these travel beside it on the
//! call's context, as the request does, so the claim's path through every wrapper
//! of the port is unchanged.
//!
//! **One constructor builds every job** ([`CaptureJob::new`]), and it builds the
//! door: the door is pure (the actor, the run, the ageing), so it exists once.
//! The two parts only the answer's layer can compute, the size of a claim as a
//! read serves it and the weight of a boot, are passed in.
//!
//! A capture that reaches the store with no job fails: the store never falls
//! back to a door of its own.

use std::future::Future;
use std::sync::Arc;

use super::{BootFloor, CaptureRequest, ClaimSize, Door, Measures, State};
use crate::actor::Actor;
use crate::log::RunId;
use crate::memory::{MemoryError, NewFact, types::DeclaredType};
use crate::session::Sessions;

/// Fills the door for `fact` from the held State and the declarations in force.
pub type FillDoor =
    dyn Fn(&State, &[DeclaredType], &NewFact) -> Result<Door, MemoryError> + Send + Sync;

/// Renders the sizes of the request for `fact` as an answer would carry them.
pub type Measure = dyn Fn(&State, &NewFact, &CaptureRequest) -> Measures + Send + Sync;

/// **What a bot's boot weighs with and without the claim**, rendered by today's
/// boot render, which reads the box through the port. Called only for a write that
/// stars a rule or sets a seat count ([`CaptureRequest::could_grow_the_boot_floor`]).
/// It reaches no write. `None` is a boot that could not be read, and passes.
///
/// TEMPORARY(9.3.7): it retires when the boot renders from the State.
pub type BootFloorMeasure = dyn Fn(
        &NewFact,
        &CaptureRequest,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<BootFloor>> + Send>>
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

/// **The size of a claim as a read would serve it, and the room one answer has for
/// it**, or `None` for a thing the State does not hold. Pure. It is the answer's
/// arithmetic, so the layer that renders answers supplies it.
pub type SizeMeasure = dyn Fn(&State, &NewFact) -> Option<ClaimSize> + Send + Sync;

/// **What a bot's boot weighs with a claim and without it**, or `None` for a boot
/// that cannot be read or a write that cannot grow it. Reads the box through the
/// port and writes nothing.
///
/// TEMPORARY(9.3.7): it retires when the boot renders from the State.
pub type BootRender = dyn Fn(&NewFact) -> std::pin::Pin<Box<dyn Future<Output = Option<BootFloor>> + Send>>
    + Send
    + Sync;

/// The functions of one capture call.
#[derive(Clone)]
pub struct CaptureJob {
    pub door: Arc<FillDoor>,
    pub measure: Arc<Measure>,
    pub boot_floor: Arc<BootFloorMeasure>,
    pub run_bots: Arc<RunBotsMeasure>,
}

impl CaptureJob {
    /// **The job of the captures one call makes**, written by `actor`.
    ///
    /// **The door names no owned key and no revoked owner**, because no stored row
    /// binds an object to an app yet; the build that adds the rows (9.3.3 to
    /// 9.3.5) fills both here. The ageing cutoff the caller worked out is carried
    /// on the claim. A job with no `sessions` names no bot for any run, and one
    /// with no `boot` weighs no boot: a write that stars a rule or sets a seat
    /// count is the verb's, and the engine's own writes are neither.
    pub fn new(
        actor: &Actor,
        sessions: Option<Arc<dyn Sessions>>,
        size: Arc<SizeMeasure>,
        boot: Option<Arc<BootRender>>,
    ) -> Self {
        let door_actor = actor.clone();
        Self {
            door: Arc::new(move |_state, _declared, fact| {
                Ok(Door {
                    actor: door_actor.clone(),
                    owned_keys: Default::default(),
                    owner_revoked: false,
                    measures: Measures::default(),
                    runs: Default::default(),
                    run: fact.session.clone().map(RunId),
                    aged_before: fact.aged_before,
                })
            }),
            measure: Arc::new(move |state, new, _request| Measures {
                claim: size(state, new),
                boot_floor: None,
            }),
            boot_floor: Arc::new(move |new, _request| match &boot {
                Some(boot) => boot(new),
                None => Box::pin(async { None }),
            }),
            run_bots: Arc::new(move |runs| {
                let sessions = sessions.clone();
                Box::pin(async move {
                    let mut bots = std::collections::BTreeMap::new();
                    let Some(sessions) = sessions else {
                        return bots;
                    };
                    for run in runs {
                        // A run no card answers to names nobody, and a read that
                        // fails leaves the run unknown: "not known to be another
                        // bot's".
                        if let Ok(Some(summary)) = sessions.run_named(&run.0).await {
                            bots.insert(run, summary.bot);
                        }
                    }
                    bots
                })
            }),
        }
    }
}

/// **A job for the cases that run a store**: the same constructor, with a size
/// measure that reports every claim within its room and no boot render. The store
/// refuses a claim whose size nobody measured, so a job that measured nothing
/// would refuse every capture onto a held thing. A case about a refusal the size,
/// the seats or the boot makes opens its own scope with its own measure.
#[cfg(any(test, feature = "testing"))]
pub fn test_job(actor: &Actor) -> CaptureJob {
    CaptureJob::new(
        actor,
        None,
        Arc::new(|state, new| {
            state.thing_as_read(&new.subject.0, &[])?;
            Some(ClaimSize {
                size: 1,
                room: usize::MAX,
                row: None,
            })
        }),
        None,
    )
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
            measure: Arc::new(|_, _, _| Measures::default()),
            boot_floor: Arc::new(|_, _| Box::pin(async { None })),
            run_bots: Arc::new(|_| Box::pin(async { Default::default() })),
        }
    }

    fn fact() -> NewFact {
        let mut fact = NewFact::about(
            crate::memory::EntityId("person:homer".into()),
            "a claim",
            jiff::civil::date(2026, 4, 19),
        );
        fact.session = Some("run-a".into());
        fact.aged_before = Some(jiff::Timestamp::UNIX_EPOCH);
        fact
    }

    /// **The constructor builds the door from the call**: the actor it was given,
    /// the run and the ageing of the claim, and no owned key or revoked owner.
    #[test]
    fn a_job_from_the_constructor_fills_the_door_from_the_call() {
        let job = CaptureJob::new(&Actor::Operator, None, Arc::new(|_, _| None), None);
        let door = (job.door)(&State::default(), &[], &fact()).expect("a door");
        assert_eq!(door.actor, Actor::Operator);
        assert_eq!(door.run, Some(RunId("run-a".into())));
        assert_eq!(door.aged_before, Some(jiff::Timestamp::UNIX_EPOCH));
        assert!(door.owned_keys.is_empty() && !door.owner_revoked);
    }

    /// **The two parts passed in are the job's measure and boot**, and a job with
    /// no sessions and no boot names no bot and weighs nothing.
    #[tokio::test]
    async fn the_parts_passed_in_are_what_the_job_measures_and_weighs() {
        let size = ClaimSize {
            size: 7,
            room: 9,
            row: None,
        };
        let wanted = size.clone();
        let heavy = BootFloor {
            after: 3,
            before: 1,
            parts: Vec::new(),
            stamp_margin: 0,
        };
        let weight = heavy.clone();
        let job = CaptureJob::new(
            &Actor::Operator,
            None,
            Arc::new(move |_, _| Some(wanted.clone())),
            Some(Arc::new(move |_| {
                let weight = weight.clone();
                Box::pin(async move { Some(weight) })
            })),
        );
        let door = (job.door)(&State::default(), &[], &fact()).expect("a door");
        let request = CaptureRequest::of_fact(
            &fact(),
            crate::log::RecordId("r1".into()),
            super::super::Route::Verb,
            door,
        );
        let measures = (job.measure)(&State::default(), &fact(), &request);
        assert_eq!(measures.claim, Some(size));
        assert_eq!((job.boot_floor)(&fact(), &request).await, Some(heavy));
        assert!((job.run_bots)(vec![RunId("run-a".into())]).await.is_empty());

        let bare = CaptureJob::new(&Actor::Operator, None, Arc::new(|_, _| None), None);
        assert_eq!((bare.boot_floor)(&fact(), &request).await, None);
    }

    /// **The test job says every claim fits on a thing the State holds**, and
    /// measures nothing for a thing it does not.
    #[test]
    fn the_test_job_says_every_claim_fits() {
        crate::memory::kinds::load_shipped();
        let job = test_job(&Actor::Operator);
        let request = CaptureRequest::of_fact(
            &fact(),
            crate::log::RecordId("r1".into()),
            super::super::Route::Verb,
            (job.door)(&State::default(), &[], &fact()).expect("a door"),
        );
        assert_eq!(
            (job.measure)(&State::default(), &fact(), &request).claim,
            None,
            "a thing the State does not hold has no size"
        );
        use super::super::ClaimEvent;
        use super::super::tests::{body, object, replay};
        let held = replay(&[
            ClaimEvent::ObjectCreated {
                object: object("k1"),
                body: body("person", "person:homer"),
            },
            ClaimEvent::AddressBound {
                object: object("k1"),
                address: "person:homer".into(),
            },
        ]);
        let claim = (job.measure)(&held, &fact(), &request)
            .claim
            .expect("a thing the State holds has a size");
        assert!(claim.size <= claim.room, "every claim fits: {claim:?}");
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
