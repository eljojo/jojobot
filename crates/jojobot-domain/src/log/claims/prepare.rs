//! **Capture's admission, decided from the state alone.** [`prepare`] reads a
//! request and the [`State`], and answers with the events the capture writes or
//! the one refusal it meets. Pure: no store, no clock, nothing emitted, and the
//! state is not changed.
//!
//! **The order of refusal is part of the rule.** A request with two faults is
//! told the first, so a caller that fixes it meets the second. The order is:
//!
//! 1. the write alone: a setting with no value, a describing key set on a thing;
//! 2. the subject, then the edge's object, then the refs, then the objects a
//!    reference field names, then the claim the write is derived from;
//! 3. the number a container's member asks for, when another member holds it.
//!
//! The write is judged first because it needs no object: it is a fault of the
//! request, and a store that named a missing subject first would leave the
//! caller to find it after.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;

use super::{ClaimBody, ClaimEvent, FieldValue, KeyId, ObjectId, RecordId, State};
use crate::app::AppStamp;
use crate::memory::{
    self, FactStatus, MemoryError, Provenance, Standing, WriteClass, types::DeclaredType,
};

/// **What the caller decided about the claim itself**, as opposed to where it
/// goes. [`prepare`] copies these into the claim and decides the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub content: String,
    pub details: Option<String>,
    pub provenance: Provenance,
    pub standing: Standing,
    pub status: FactStatus,
    pub recorded_at: Date,
    pub happened_at: Option<Date>,
    pub happened_through: Option<Date>,
    pub stale_after: Option<Date>,
    pub receipt: Option<String>,
    pub app: Option<AppStamp>,
}

/// **One capture, as the caller asks it.** Every name is a spelling the caller
/// typed: [`prepare`] resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRequest {
    /// The identity the claim takes. The caller mints it: the state holds no
    /// counter for it.
    pub record: RecordId,
    pub subject: String,
    pub draft: Draft,
    /// The edge's shape and the spelling of the object it points at.
    pub edge: Option<(String, String)>,
    pub refs: Vec<String>,
    pub fields: BTreeMap<String, String>,
    /// The keys of `fields` that are settings of the thing.
    pub sets: BTreeSet<String>,
    /// The keys of `fields` that jojobot computed.
    pub derived: BTreeSet<String>,
    /// The claim this one is derived from: a spelling, a `#`, a label.
    pub derived_from: Option<String>,
}

/// **The one answer a refused request gets.** Each variant names the first
/// fault the order above meets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The write is malformed on its own. Carries the words the caller reads.
    Malformed(String),
    UnknownSubject(String),
    UnknownEdgeObject(String),
    UnknownRef(String),
    /// A reference field names an object nobody holds.
    UnknownFieldTarget(String),
    /// The home a `derived_from` names is unknown.
    UnknownSourceHome(String),
    /// The home is known and holds no claim under the label.
    UnknownSourceClaim(String),
    /// A container numbers its members, and another member holds the number the
    /// write asks for. An archived member holds its number too.
    NumberHeld {
        container: ObjectId,
        key: String,
        number: u64,
        holder: ObjectId,
    },
}

/// **The events a capture writes**, in the order they apply: the claim, then one
/// write per key.
pub type Admitted = Vec<ClaimEvent>;

/// Decide one capture over the state. `declared` is what the types declare: the
/// state does not hold it.
pub fn prepare(
    state: &State,
    declared: &[DeclaredType],
    request: &CaptureRequest,
) -> Result<Admitted, Refusal> {
    // The write alone, before any object is looked up.
    let classes = memory::classes_for_write(
        &BTreeMap::new(),
        &request.fields,
        &request.sets,
        &request.derived,
        &[],
        declared,
        false,
    )
    .map_err(|refused| match refused {
        MemoryError::InvalidFact(words) => Refusal::Malformed(words),
        other => Refusal::Malformed(other.to_string()),
    })?;

    let home = state
        .resolve(&request.subject)
        .ok_or_else(|| Refusal::UnknownSubject(request.subject.clone()))?
        .clone();
    let edge = match &request.edge {
        Some((shape, object)) => {
            let object = state
                .resolve(object)
                .ok_or_else(|| Refusal::UnknownEdgeObject(object.clone()))?;
            Some((shape.clone(), object.clone()))
        }
        None => None,
    };
    let mut refs = Vec::new();
    for name in &request.refs {
        let object = state
            .resolve(name)
            .ok_or_else(|| Refusal::UnknownRef(name.clone()))?;
        refs.push(object.clone());
    }
    let mut targets: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    for (key, items) in memory::reference_field_values(&request.fields, declared) {
        for item in items {
            let object = state
                .resolve(&item)
                .ok_or_else(|| Refusal::UnknownFieldTarget(item.clone()))?;
            targets.entry(key.clone()).or_default().push(object.clone());
        }
    }
    let derived_from = match &request.derived_from {
        Some(address) => Some(
            state
                .claim_at(address)
                .ok_or_else(|| match address.rsplit_once('#') {
                    Some((home, _)) if state.resolve(home).is_some() => {
                        Refusal::UnknownSourceClaim(address.clone())
                    }
                    _ => Refusal::UnknownSourceHome(address.clone()),
                })?
                .record,
        ),
        None => None,
    };

    // The last fault in the order: it needs the subject and nothing else.
    for (container, numbers_by) in state.numbering_scopes(&home) {
        let Some(number) = request
            .fields
            .iter()
            .find(|(key, _)| key.trim() == numbers_by)
            .and_then(|(_, value)| memory::numbered::whole_number(value))
        else {
            continue;
        };
        if let Some(holder) = state.numbered_member(&container, &numbers_by, number)
            && holder != &home
        {
            return Err(Refusal::NumberHeld {
                container,
                key: numbers_by,
                number,
                holder: holder.clone(),
            });
        }
    }

    let draft = &request.draft;
    let claim = ClaimBody {
        label: format!("f{}", state.highest_claim_number(&home) + 1),
        home: home.clone(),
        content: draft.content.clone(),
        details: draft.details.clone(),
        provenance: draft.provenance,
        standing: draft.standing,
        status: draft.status,
        recorded_at: draft.recorded_at,
        happened_at: draft.happened_at,
        happened_through: draft.happened_through,
        stale_after: draft.stale_after,
        edge,
        derived_from,
        refs,
        stands_for: Vec::new(),
        receipt: draft.receipt.clone(),
        app: draft.app.clone(),
        inserted_at: None,
    };
    let mut events = vec![ClaimEvent::RecordCreated {
        record: request.record.clone(),
        claim,
    }];
    for (key, value) in &request.fields {
        let key = KeyId(key.trim().to_string());
        events.push(ClaimEvent::FieldWritten {
            record: request.record.clone(),
            subject: home.clone(),
            ordinal: state.next_ordinal(&home, &key),
            class: classes
                .get(key.0.as_str())
                .copied()
                .unwrap_or(WriteClass::Own),
            write: FieldValue::Set(value.clone()),
            targets: targets.get(&key.0).cloned().unwrap_or_default(),
            key,
        });
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
