//! **The rows of a capture the claims State decided.** `prepare` admits a capture
//! as events: the claim, one write for each key, and whatever it took back. The
//! rows are still read until the reads come from the State, so the same
//! transaction that stages the events writes the rows they stand for.
//!
//! **This allocates nothing, refuses nothing and emits nothing.** The place of a
//! write, the label of a claim and every refusal were settled by the decision;
//! what is written here is what it admitted, in the order it admitted it, through
//! the inserts the old writers call.

use std::collections::BTreeMap;
use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::log::claims::{ClaimBody, ClaimEvent, FieldValue};
use jojobot_domain::log::{Event, RecordId};
use jojobot_domain::memory::{Edge, EdgeShape};

use super::*;
use crate::ledger_seam::Seam;

impl DoltMemory {
    /// **Write the rows of the events a capture was admitted as**, in `tx`.
    pub(super) async fn write_admitted(
        &self,
        tx: &mut Transaction<'_, MySql>,
        seam: &Arc<Seam>,
        events: &[Event<ClaimEvent>],
        places: &BTreeMap<RecordId, i64>,
    ) -> Result<(), MemoryError> {
        // The label each claim is stored under, for the claims this batch writes.
        let mut labels: BTreeMap<RecordId, String> = BTreeMap::new();
        for event in events {
            let session = event.run.as_ref().map(|run| run.0.as_str());
            let bot = match &event.actor {
                Some(Actor::Bot(bot)) => Some(self.stored_writer(tx, bot).await?),
                _ => None,
            };
            // **The moment is the decision's**: an event that carries none was not
            // admitted by it, and no clock is asked here.
            let moment = event
                .written_at
                .map(|at| at.to_string())
                .ok_or_else(|| MemoryError::Store("an admitted event carries no moment".into()))?;
            match &event.payload {
                ClaimEvent::RecordCreated { record, claim } => {
                    let fact = fact_of(seam, claim);
                    Self::write_fact_row(tx, &fact).await?;
                    let place = place_of(places, record)?;
                    Self::insert_fact_write(tx, &fact, place, &moment, session, bot.as_ref())
                        .await?;
                    labels.insert(record.clone(), claim.label.clone());
                }
                ClaimEvent::RecordRevised { record, claim } => {
                    let place = place_of(places, record)?;
                    let fact = fact_of(seam, claim);
                    Self::write_fact_row(tx, &fact).await?;
                    Self::insert_fact_write(tx, &fact, place, &moment, session, bot.as_ref())
                        .await?;
                    labels.insert(record.clone(), claim.label.clone());
                }
                ClaimEvent::FieldWritten {
                    record,
                    subject,
                    key,
                    ordinal,
                    class,
                    write,
                    ..
                } => {
                    let label = match labels.get(record) {
                        Some(label) => label.clone(),
                        None => seam
                            .read_claims(|state| {
                                state.record(record).map(|held| held.claim.label.clone())
                            })
                            .ok_or_else(|| {
                                MemoryError::Store(format!(
                                    "a key write names a claim nothing holds: {}",
                                    record.0
                                ))
                            })?,
                    };
                    let place = place_of(places, record)?;
                    let value = match write {
                        FieldValue::Set(value) => Some(value.clone()),
                        FieldValue::Clear | FieldValue::Removed => None,
                    };
                    let thing = EntityId(subject.0.clone());
                    Self::insert_field_write(
                        tx,
                        &thing,
                        &FactId(label),
                        (&key.0, *ordinal as i64),
                        &value,
                        &moment,
                        *class,
                        place,
                    )
                    .await?;
                    // A role's claim of its seat rides the heartbeat home, as the
                    // old capture noted it.
                    if key.0 == jojobot_domain::session::ROLE_CLAIMED_AT
                        && let Some(moment) = &value
                        && let Some(handle) = seam.read_claims(|state| state.handle(subject))
                        && self.role_state_of(tx, &handle, &thing).await?.is_some()
                    {
                        Self::note_lease_write(tx, &thing, None, moment, &self.clock).await?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// **The place the decision gave a claim's write.**
fn place_of(places: &BTreeMap<RecordId, i64>, record: &RecordId) -> Result<i64, MemoryError> {
    places.get(record).copied().ok_or_else(|| {
        MemoryError::Store(format!(
            "the decision gave no place to the claim {}",
            record.0
        ))
    })
}

/// **A claim as the rows store it**: its thing and its pointers by the key each
/// is filed under, which is what the State names them by.
fn fact_of(seam: &Arc<Seam>, claim: &ClaimBody) -> Fact {
    let address_of = |record: &RecordId| {
        seam.read_claims(|state| {
            state.record(record).map(|held| {
                FactAddress::new(
                    EntityId(held.claim.home.0.clone()),
                    FactId(held.claim.label.clone()),
                )
            })
        })
    };
    let home = EntityId(claim.home.0.clone());
    Fact {
        app_stamp: claim.app.clone(),
        written_by_another: false,
        sets: Default::default(),
        reaching: Default::default(),
        field_links: Default::default(),
        id: FactId(claim.label.clone()),
        home: home.clone(),
        subject: home,
        content: claim.content.clone(),
        details: claim.details.clone(),
        provenance: claim.provenance,
        standing: claim.standing,
        status: claim.status,
        recorded_at: claim.recorded_at,
        happened_at: claim.happened_at,
        happened_through: claim.happened_through,
        edge: claim.edge.as_ref().and_then(|(shape, object)| {
            Some(Edge {
                shape: EdgeShape::from_token(shape)?,
                object: EntityId(object.0.clone()),
            })
        }),
        fields: Default::default(),
        refs: claim
            .refs
            .iter()
            .map(|object| EntityId(object.0.clone()))
            .collect(),
        derived_from: claim.derived_from.as_ref().and_then(address_of),
        stands_for: claim.stands_for.iter().filter_map(address_of).collect(),
        inserted_at: claim.inserted_at,
        stale_after: claim.stale_after,
        receipt: claim.receipt.clone(),
    }
}
