//! **What a capture hands the writer's job.** The store decides a capture over the
//! claims State it holds, and four things it cannot work out itself come from
//! here: the door (who writes and with which grant), the size of the claim, the
//! weight of a bot's boot, and which bot each run belongs to. Each runs inside
//! the writer's job, against the State the batch is staged on, never before it.
//!
//! The door and the size are pure functions of the State. The boot render and
//! the run-to-bot map read through the port today and write nothing:
//! TEMPORARY(9.3.7), they retire when the boot and the sessions render from the
//! State.

use std::collections::BTreeMap;
use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::log::RunId;
use jojobot_domain::log::claims::job::CaptureJob;
use jojobot_domain::log::claims::{Door, Measures};

use super::reads_left::ReadsLeft;
use super::*;

impl Jojobot {
    /// The job of the captures one call makes, written by `actor`.
    ///
    /// **The door names no owned key and no revoked owner**, because no stored row
    /// binds an object to an app yet; the build that adds the rows (9.3.3 to
    /// 9.3.5) fills both here. The ageing cutoff the verb worked out is carried on
    /// the claim.
    pub(crate) fn capture_job(&self, actor: &Actor, reads: Arc<ReadsLeft>) -> CaptureJob {
        let door_actor = actor.clone();
        let now = self.clock().now();
        let this = self.clone();
        let sessions = Arc::clone(&self.sessions);
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
                claim: Self::measure_new_claim(state, new, now),
                boot_floor: None,
            }),
            boot_floor: Arc::new(move |new, _request| {
                let (this, new, reads) = (this.clone(), new.clone(), Arc::clone(&reads));
                Box::pin(async move { this.boot_floor_measure_for_capture(&new, &reads).await })
            }),
            run_bots: Arc::new(move |runs| {
                let sessions = Arc::clone(&sessions);
                Box::pin(async move {
                    let mut bots = BTreeMap::new();
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
