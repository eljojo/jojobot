//! **Capture's admission, decided from the state alone.** [`prepare`] reads a
//! request and the [`State`], and answers with the events the capture writes or
//! the one refusal it meets. Pure: no store, no clock, nothing emitted, and the
//! state is not changed.
//!
//! **The order of refusal is part of the rule.** A request with two faults is
//! told the first, so a caller that fixes it meets the second. The order is:
//!
//! 1. the guards only the verb applies: a role's lease field, then a key
//!    jojobot writes itself (the stored due moment, the review stamp);
//! 2. the write alone: a setting with no value, a describing key set on a thing;
//! 3. the subject, then who may write the keys it holds (an app's owned keys,
//!    then a bot's guarded keys), then the edge's object, then the refs, then the objects a
//!    reference field names, then the claim the write is derived from;
//! 4. the number a container's member asks for, when another member holds it.
//!
//! The write is judged first because it needs no object: it is a fault of the
//! request, and a store that named a missing subject first would leave the
//! caller to find it after.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;

mod chart;
mod numbering;

use super::{ClaimBody, ClaimEvent, FieldValue, KeyId, ObjectId, RecordId, State};
use crate::actor::Actor;
use crate::app::AppStamp;
use crate::attention;
use crate::memory::{
    self, FactStatus, MemoryError, Provenance, Standing, WriteClass, types::DeclaredType,
};

/// **Which door the request came through.** The verb is the caller-facing
/// surface; the engine writes its own records, and some of them carry keys the
/// verb refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Verb,
    Engine,
}

/// **What the door resolved before the request reached the state**: who is
/// writing, which keys of the subject's kind its declaration marks as owned by
/// the object's binding, and whether the owner's grant is revoked. The state
/// holds none of it: revocation comes from the grants view, the marks from the
/// declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door {
    pub actor: Actor,
    pub owned_keys: BTreeSet<String>,
    pub owner_revoked: bool,
}

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
    pub route: Route,
    pub door: Door,
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
    /// The subject is bound to an app, and the writer is not that app.
    OwnedKeyWritten {
        subject: String,
        key: String,
    },
    /// The owning app's grant is revoked: its keys stay locked, to the app too.
    OwnerRevoked {
        subject: String,
        key: String,
    },
    /// A role's lease field named through the verb. The boot door owns it.
    RoleFieldGuarded {
        role: String,
        key: String,
    },
    /// A bot writes a guarded key its declaration does not license it to
    /// write, or names a manager that would invert the chart.
    Unlicensed(MemoryError),
    /// The verb names a key jojobot writes itself: the stored due moment, the
    /// review stamp and the other bookkeeping keys.
    ServerWritten {
        key: String,
    },
    /// The numbering decision refuses the write for a reason the variants above
    /// do not name: a value that is no number, a number a member would free, a
    /// collection founded over members that break its promise.
    Numbering(MemoryError),
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
    /// Nobody holds the number now, and a claim that held it was taken back.
    /// A number issued once is never issued again.
    NumberRetired {
        container: ObjectId,
        key: String,
        number: u64,
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
    // The guards only the verb applies come first: they read the request's own
    // words and need no object. The engine's routes write a role's lease fields.
    if request.route == Route::Verb
        && let Some(MemoryError::RoleFieldGuarded { role, key }) = memory::refuses_role_fields(
            &memory::EntityId(request.subject.clone()),
            request.fields.keys(),
        )
    {
        return Err(Refusal::RoleFieldGuarded { role, key });
    }
    if request.route == Route::Verb
        && let Some(key) = request.fields.keys().map(|key| key.trim()).find(|key| {
            *key == attention::DUE_ON || memory::review::SERVER_WRITTEN_KEYS.contains(key)
        })
    {
        return Err(Refusal::ServerWritten {
            key: key.to_string(),
        });
    }
    // The write alone, before any object is looked up.
    let classes = memory::classes_for_write(
        &BTreeMap::new(),
        &request.fields,
        &request.sets,
        &request.derived,
        &[],
        declared,
        // A new own field stays on its claim, as the store's default decides.
        true,
    )
    .map_err(|refused| match refused {
        MemoryError::InvalidFact(words) => Refusal::Malformed(words),
        other => Refusal::Malformed(other.to_string()),
    })?;

    let home = state
        .resolve(&request.subject)
        .ok_or_else(|| Refusal::UnknownSubject(request.subject.clone()))?
        .clone();
    // Who may write a key: only the app the subject is bound to, and only while
    // its grant stands. A key the kind does not mark, or a subject bound to no
    // app, is open.
    if let Some(owner) = state.object(&home).and_then(|body| body.bound_to.as_ref()) {
        let touched = request
            .fields
            .keys()
            .map(|key| key.trim())
            .find(|key| request.door.owned_keys.contains(*key));
        if let Some(key) = touched {
            let key = key.to_string();
            let subject = request.subject.clone();
            if request.door.owner_revoked {
                return Err(Refusal::OwnerRevoked { subject, key });
            }
            if request.door.actor != Actor::App(owner.clone()) {
                return Err(Refusal::OwnedKeyWritten { subject, key });
            }
        }
    }
    // A bot writes a guarded key only where its declaration licenses it, judged
    // over the chart the state holds. An app was judged by its grant at the door.
    if let Actor::Bot(caller) = &request.door.actor
        && memory::names_a_guarded_key(&request.fields)
    {
        let subject = state
            .handle_of(&home)
            .unwrap_or_else(|| memory::EntityId(request.subject.clone()));
        let chart = chart::Chart::new(state, declared);
        let held = chart.fold(&subject);
        // A resend of the value a key holds is not a change.
        let changing: BTreeMap<String, String> = request
            .fields
            .iter()
            .filter(|(key, value)| {
                let guarded = memory::GUARDED_KEYS
                    .iter()
                    .any(|rule| rule.key == key.trim());
                !(guarded
                    && held
                        .get(key.trim())
                        .is_some_and(|now| now.trim() == value.trim()))
            })
            .map(|(key, value)| (key.trim().to_string(), value.clone()))
            .collect();
        let lineage =
            memory::needs_the_chart(&changing).map(|named| chart.lineage(&subject, &home, named));
        if let Some(refused) =
            memory::refuses_unlicensed_write(&subject, caller, &changing, lineage.as_ref())
        {
            return Err(Refusal::Unlicensed(refused));
        }
    }
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
    // The link decision (rule 646): a lone name under a reference key must be the
    // badge of a thing, and any other value names a thing only through the
    // handles it holds, whatever its key. Prose names nothing.
    for (name, refusal) in memory::lone_reference_names(&request.fields, declared) {
        if state.resolve(name).is_none() {
            return Err(Refusal::Malformed(match refusal {
                MemoryError::InvalidFact(words) => words,
                other => other.to_string(),
            }));
        }
    }
    let reference_keys: BTreeSet<String> =
        memory::reference_field_values(&request.fields, declared)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
    let mut targets: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    for (key, value) in &request.fields {
        let mut named: Vec<String> = memory::handles_in_value(value)
            .into_iter()
            .map(|handle| handle.to_string())
            .collect();
        // A reference key also takes the badge of a thing it holds.
        if reference_keys.contains(key) && named.is_empty() {
            named = value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| state.resolve(item).is_some())
                .collect();
        }
        for item in named {
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
    if let Some(refused) = numbering::refuses(state, declared, &home, request) {
        return Err(refused);
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
