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

/// What the job admitted: the request as it was decided and the events that
/// were added to the transaction's batch.
pub(crate) struct Admission {
    /// The record the capture created.
    pub record: RecordId,
    pub request: CaptureRequest,
    pub events: Vec<Event<ClaimEvent>>,
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
    at: Timestamp,
) -> Result<Result<Admission, Declined>, MemoryError> {
    seam.settle().await.map_err(store)?;
    seam.settle_claims().await.map_err(store)?;
    let decided = seam.read_claims(|state| {
        let declared = state.declared_types();
        let mut request = build(state, &declared)?;
        request.door.measures = measure(state, &request);
        Ok::<_, MemoryError>(match prepare(state, &declared, &request) {
            Ok(admitted) => Ok((request, admitted)),
            // **Answered from the State that refused**, before it can move.
            Err(refusal) => Err(refusal.declined(state)),
        })
    })?;
    let (request, admitted) = match decided {
        Ok(decided) => decided,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let events: Vec<Event<ClaimEvent>> = admitted
        .into_iter()
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
    admit(
        seam,
        Box::new(move |state, declared| {
            let door = door(state, declared, fact)?;
            Ok(CaptureRequest::of_fact(fact, record, Route::Verb, door))
        }),
        Box::new(move |state, request| measure(state, request)),
        at,
    )
    .await
}

#[cfg(test)]
mod tests;
