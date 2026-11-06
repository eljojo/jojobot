//! **A capture as the port is asked it, read as the request [`prepare`] decides.**
//! The port's `NewFact` holds what the caller wrote; the door holds what the
//! server resolved about the writer. This is the one place the two become a
//! request, so the store and the differential read a capture the same way.

use super::{CaptureRequest, Door, Draft, Route};
use crate::log::RecordId;
use crate::memory::{NewFact, Standing};

impl CaptureRequest {
    /// **Whether this write could make a bot's boot heavier than it can be**: it
    /// stars a rule or sets a seat count. Only such a write is asked for the
    /// boot floor ([`Measures::boot_floor`](super::Measures)).
    pub fn could_grow_the_boot_floor(&self) -> bool {
        self.fields.get("starred").is_some_and(|v| v == "true")
            || self.fields.contains_key(crate::memory::RULE_SEATS)
    }

    /// The request for `fact`, written under `record` through `route`, with the
    /// door the server resolved. Every name is the spelling the caller typed;
    /// [`prepare`](super::prepare) resolves it.
    pub fn of_fact(fact: &NewFact, record: RecordId, route: Route, door: Door) -> Self {
        Self {
            record,
            route,
            door,
            subject: fact.subject.0.clone(),
            draft: Draft {
                content: fact.content.clone(),
                details: fact.details.clone(),
                provenance: fact.provenance,
                standing: fact
                    .standing
                    .unwrap_or_else(|| Standing::default_for(fact.provenance)),
                status: fact.status,
                recorded_at: fact.recorded_at,
                happened_at: fact.happened_at,
                happened_through: fact.happened_through,
                stale_after: fact.stale_after,
                receipt: fact.receipt.clone(),
                app: fact.app_stamp.clone(),
            },
            edge: fact
                .edge
                .as_ref()
                .map(|edge| (edge.shape, edge.object.0.clone())),
            refs: fact.refs.iter().map(|object| object.0.clone()).collect(),
            fields: fact.fields.clone(),
            sets: fact.sets.clone(),
            derived: fact.derived.clone(),
            derived_from: fact.derived_from.as_ref().map(ToString::to_string),
            borrow: fact.borrow,
            drop: fact
                .drop
                .as_ref()
                .zip(fact.drop_because.as_ref())
                .map(|(victim, why)| (victim.to_string(), why.clone())),
        }
    }
}
