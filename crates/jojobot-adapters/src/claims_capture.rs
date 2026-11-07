//! **The capture job's admission**, run inside a writer job over the claims
//! State the writer's seam holds. In order: the held State is caught up to the
//! log, the verb's builder fills the request from that State, the verb's measure
//! renders the sizes from that same State, and [`prepare`] decides. What it
//! admits is added to the open transaction's batch ([`sql::emit`]) and applied
//! to the State when the commit is acknowledged.
//!
//! **Nothing is measured or read before the job and taken again inside it.** The
//! one writer serialises every write, so the State read here is the State the
//! batch is staged against.
//!
//! Callers are the capture verb's port and, through it, the overlay on a
//! supplied record: that overlay is a capture and takes this path.

// The port that calls the job lands in the next commit of 9.3.3-d; until then only
// its cases do, and what the job calls (`sql::emit`, `Seam::read_claims`) is live
// through it.
#![allow(dead_code)]

use std::sync::Arc;

use jiff::Timestamp;
use jojobot_domain::log::Event;
use jojobot_domain::log::RecordId;
use jojobot_domain::log::claims::job::{self, CaptureJob};
use jojobot_domain::log::claims::{
    CaptureRequest, ClaimEvent, Declined, Measures, Route, State, prepare,
};
use jojobot_domain::memory::{MemoryError, NewFact, types::DeclaredType};

use crate::ledger_seam::Seam;
use crate::writer::sql;

/// Fills the request from the held State: the subject, the draft, and the door
/// (owned keys from the declarations, the app's revocation, the runs and the
/// ageing). Pure: it reads the State it is given and nothing else.
pub(crate) type Build<'a> =
    Box<dyn FnOnce(&State, &[DeclaredType]) -> Result<CaptureRequest, MemoryError> + Send + 'a>;

/// Renders the sizes of the request as an answer would carry them. Pure.
pub(crate) type Measure<'a> = Box<dyn Fn(&State, &CaptureRequest) -> Measures + Send + 'a>;

/// The boot render of one claim, asked with the request it belongs to.
pub(crate) type RequestBoot = dyn Fn(
        &CaptureRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Option<jojobot_domain::log::claims::BootFloor>> + Send,
        >,
    > + Send
    + Sync;

/// What the job admitted: the request as it was decided and the events that
/// were added to the transaction's batch.
pub(crate) struct Admission {
    /// The record the capture created.
    pub record: RecordId,
    pub request: CaptureRequest,
    pub events: Vec<Event<ClaimEvent>>,
    /// **The place each claim's write holds in the log of its claim**, worked out
    /// from the State the decision was made on: the first write of a new claim,
    /// the next one of a claim the batch revises. The rows take their places from
    /// here and count nothing.
    pub places: std::collections::BTreeMap<RecordId, i64>,
}

fn store(why: impl std::fmt::Display) -> MemoryError {
    MemoryError::Store(why.to_string())
}

/// **Decide one capture over the held State and add what it admits to the open
/// transaction's batch.** A refusal adds nothing. `at` stamps the events.
pub(crate) async fn admit(
    seam: &Arc<Seam>,
    build: Build<'_>,
    measure: Measure<'_>,
    boot_floor: Option<Arc<RequestBoot>>,
    run_bots: Option<Arc<job::RunBotsMeasure>>,
    at: Timestamp,
) -> Result<Result<Admission, Declined>, MemoryError> {
    seam.settle().await.map_err(store)?;
    seam.settle_claims().await.map_err(store)?;
    let mut request = seam.read_claims(|state| {
        let declared = state.declared_types();
        let mut request = build(state, &declared)?;
        request.door.measures = measure(state, &request);
        Ok::<_, MemoryError>(request)
    })?;
    // **The boot is rendered only for a write that could make it heavier.** The
    // one writer lets no write land between this render and the decision below, so
    // the render sees what the State holds. A boot that cannot be read passes.
    // TEMPORARY(9.3.7): the boot renders from the State then, and this goes.
    if request.could_grow_the_boot_floor()
        && let Some(boot_floor) = &boot_floor
    {
        request.door.measures.boot_floor = boot_floor(&request).await;
    }
    // **Which bot each run belongs to is asked only of a write that could be a
    // thought**, for the runs that first wrote the claims on its subject and the
    // run it is written in. TEMPORARY(9.3.7): sessions reach the log then.
    if request.could_be_a_thought()
        && let Some(run_bots) = &run_bots
    {
        let mut runs = seam.read_claims(|state| state.first_runs_on(&request.subject));
        runs.extend(request.door.run.clone());
        if !runs.is_empty() {
            request.door.runs = run_bots(runs.into_iter().collect()).await;
        }
    }
    let decided = seam.read_claims(|state| {
        let declared = state.declared_types();
        match prepare(state, &declared, &request) {
            // **A capture that changes the spec of a bot request carries its number
            // when it is admitted**, so the rows written for it include the number
            // too. The seam leaves a batch alone that holds its own.
            Ok(mut admitted) => {
                let numbers = state.generation_events(
                    &admitted,
                    &declared,
                    at.to_zoned(jiff::tz::TimeZone::UTC).date(),
                    jojobot_domain::log::claims::job::fresh_record,
                );
                admitted.extend(numbers);
                let held = |record: &RecordId| {
                    state
                        .record(record)
                        .map_or(0, |held| held.history.len() as i64)
                };
                let mut places = std::collections::BTreeMap::new();
                for payload in &admitted {
                    match payload {
                        ClaimEvent::RecordCreated { record, .. } => {
                            places.insert(record.clone(), 1);
                        }
                        ClaimEvent::RecordRevised { record, .. } => {
                            places.insert(record.clone(), held(record) + 1);
                        }
                        ClaimEvent::FieldWritten { record, .. } => {
                            places.entry(record.clone()).or_insert_with(|| held(record));
                        }
                        _ => {}
                    }
                }
                Ok((admitted, places))
            }
            // **Answered from the State that refused**, before it can move.
            Err(refusal) => Err(refusal.declined(state)),
        }
    });
    let (admitted, places) = match decided {
        Ok(decided) => decided,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let events: Vec<Event<ClaimEvent>> = admitted
        .into_iter()
        .map(|mut payload| {
            // **The moment the claim was inserted is the moment of this write.**
            // The rule is pure and holds no clock, so the stamp is put on here.
            if let ClaimEvent::RecordCreated { claim, .. } = &mut payload {
                claim.inserted_at = Some(at);
            }
            payload
        })
        .map(|payload| Event {
            event_kind: String::new(),
            meaning_version: 1,
            actor: Some(request.door.actor.clone()),
            run: request.door.run.clone(),
            session_card: None,
            request: None,
            part: None,
            occurred_at: Some(at),
            written_at: Some(at),
            imported_source_time: None,
            payload,
        })
        .collect();
    sql::emit(events.clone()).map_err(store)?;
    Ok(Ok(Admission {
        record: request.record.clone(),
        request,
        events,
        places,
    }))
}

/// **Decide the capture of `fact` with the job its call carries**: the door is
/// filled from the held State, the sizes are rendered from it, and the claim is
/// written under a fresh record. A call with no job fails; the store has no door
/// of its own.
pub(crate) async fn admit_fact(
    seam: &Arc<Seam>,
    fact: &NewFact,
    job: &CaptureJob,
    at: Timestamp,
) -> Result<Result<Admission, Declined>, MemoryError> {
    let record = job::fresh_record();
    let (door, measure) = (Arc::clone(&job.door), Arc::clone(&job.measure));
    let boot_floor: Arc<RequestBoot> = {
        let (boot, fact) = (Arc::clone(&job.boot_floor), fact.clone());
        Arc::new(move |request| boot(&fact, request))
    };
    let boot_floor = Some(boot_floor);
    let run_bots = Some(Arc::clone(&job.run_bots));
    admit(
        seam,
        Box::new(move |state, declared| {
            // **The request is about the thing under the handle it wears now**, so
            // a caller that typed a former spelling is judged as one that typed the
            // current one.
            let subject = state
                .canonical_handle(&fact.subject.0)
                .unwrap_or_else(|| fact.subject.clone());
            let fact = &NewFact {
                subject,
                ..fact.clone()
            };
            let door = door(state, declared, fact)?;
            // The engine's own records carry keys a verb is refused.
            let route = match door.actor {
                jojobot_domain::actor::Actor::System(_) => Route::Engine,
                _ => Route::Verb,
            };
            Ok(CaptureRequest::of_fact(fact, record, route, door))
        }),
        Box::new(move |state, request| measure(state, fact, request)),
        boot_floor,
        run_bots,
        at,
    )
    .await
}

#[cfg(test)]
mod tests;
