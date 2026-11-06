//! **What a capture hands the writer's job.** The store decides a capture over the
//! claims State it holds, and four things it cannot work out itself come from
//! here: the door (who writes and with which grant), the size of the claim, the
//! weight of a bot's boot, and which bot each run belongs to. Each runs inside
//! the writer's job, against the State the batch is staged on, never before it.
//!
//! **One builder makes every job** ([`capture_job`]): it is a function of the
//! actor and the stack's parts, so the verb, the engine's own writes and the app
//! path get the same door rules from one place. The door and the size are pure
//! functions of the State. The boot render and the run-to-bot map read through the
//! port today and write nothing: TEMPORARY(9.3.7), they retire when the boot and
//! the sessions render from the State.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::clock::Clock;
use jojobot_domain::log::RunId;
use jojobot_domain::log::claims::job::CaptureJob;
use jojobot_domain::log::claims::{BootFloor, Door, Measures};
use jojobot_domain::memory::NewFact;
use jojobot_domain::session::Sessions;

use super::reads_left::ReadsLeft;
use super::*;

/// The weight of a bot's boot with a claim and without it, or `None` for a boot
/// that cannot be read or a write that cannot grow it. TEMPORARY(9.3.7).
pub(crate) type BootRender =
    dyn Fn(&NewFact) -> Pin<Box<dyn Future<Output = Option<BootFloor>> + Send>> + Send + Sync;

/// **The job of the captures one call makes**, written by `actor`.
///
/// **The door names no owned key and no revoked owner**, because no stored row
/// binds an object to an app yet; the build that adds the rows (9.3.3 to 9.3.5)
/// fills both here. The ageing cutoff the verb worked out is carried on the claim.
/// A stack with no `sessions` names no bot for any run, and one with no `boot`
/// renders no boot: a write that stars a rule or sets a seat count is the verb's,
/// and the engine's own writes are neither.
pub(crate) fn capture_job(
    actor: &Actor,
    clock: &Clock,
    sessions: Option<Arc<dyn Sessions>>,
    boot: Option<Arc<BootRender>>,
) -> CaptureJob {
    let door_actor = actor.clone();
    let clock = *clock;
    CaptureJob {
        door: Arc::new(move |_state, _declared, fact| {
            Ok(Door {
                actor: door_actor.clone(),
                owned_keys: Default::default(),
                owner_revoked: false,
                measures: Measures::default(),
                runs: BTreeMap::new(),
                run: fact.session.clone().map(RunId),
                aged_before: fact.aged_before,
            })
        }),
        measure: Arc::new(move |state, new, _request| Measures {
            claim: Jojobot::measure_new_claim(state, new, clock.now()),
            boot_floor: None,
        }),
        boot_floor: Arc::new(move |new, _request| match &boot {
            Some(boot) => boot(new),
            None => Box::pin(async { None }),
        }),
        run_bots: Arc::new(move |runs| {
            let sessions = sessions.clone();
            Box::pin(async move {
                let mut bots = BTreeMap::new();
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

/// **The job of jojobot's own captures**: the ledger of release notes it keeps for
/// a bot, and any other record the engine writes for itself. Built once where the
/// stack is assembled and scoped around the engine's writes. The actor is jojobot
/// itself (rule 731).
pub fn engine_capture_job(clock: &Clock) -> CaptureJob {
    capture_job(&Actor::System("jojobot".into()), clock, None, None)
}

impl Jojobot {
    /// The job of the captures one call makes, written by `actor`.
    pub(crate) fn capture_job(&self, actor: &Actor, reads: Arc<ReadsLeft>) -> CaptureJob {
        let this = self.clone();
        capture_job(
            actor,
            self.clock(),
            Some(Arc::clone(&self.sessions)),
            Some(Arc::new(move |new| {
                let (this, new, reads) = (this.clone(), new.clone(), Arc::clone(&reads));
                Box::pin(async move { this.boot_floor_measure_for_capture(&new, &reads).await })
            })),
        )
    }
}
