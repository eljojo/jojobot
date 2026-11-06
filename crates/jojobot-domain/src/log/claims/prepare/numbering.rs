//! **The numbering decision of a capture, asked over the state.** The same
//! [`Reach`] the store builds inside its transaction, built here from the
//! folds, the parents and the numbers issued that the state holds, and judged
//! by the domain's one question, [`Reach::refuses`].
//!
//! The collections above the subject are judged as they stand, and a write that
//! makes the subject a collection (or numbers it by another key, or by none) is
//! judged as the collection it would become.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{ObjectId, State};
use super::{CaptureRequest, Refusal};
use crate::memory::numbered::{
    Change, Founding, KeyChange, Member, Numbered, Reach, numbering_change,
};
use crate::memory::{EntityId, MemoryError, types::DeclaredType};

fn handle(state: &State, object: &ObjectId) -> EntityId {
    state
        .handle_of(object)
        .unwrap_or_else(|| EntityId(object.0.clone()))
}

/// **A container read as a collection numbering by `key`**: every object under
/// it with what it holds under the key. The numbers issued and taken back ride
/// on one member that holds nothing, because no member owns them.
fn collection(
    state: &State,
    declared: &[DeclaredType],
    container: &ObjectId,
    key: &str,
) -> Numbered {
    let mut members: Vec<Member> = state
        .members_below(container)
        .iter()
        .map(|member| Member {
            handle: handle(state, member),
            value: state.fields(member, declared).get(key).cloned(),
            retired: BTreeSet::new(),
        })
        .collect();
    members.push(Member {
        handle: handle(state, container),
        value: None,
        retired: state.issued_unheld(container, key),
    });
    Numbered {
        collection: handle(state, container),
        key: key.to_string(),
        members,
    }
}

/// The thing as it stands once the write has landed: what it holds now, with
/// the keys the write sets or computed laid over it. A blank value takes the
/// key off.
fn after_the_write(
    held: &BTreeMap<String, String>,
    request: &CaptureRequest,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut after = held.clone();
    let mut written = BTreeMap::new();
    for (key, value) in &request.fields {
        let key = key.trim();
        let reaches = request
            .sets
            .iter()
            .chain(&request.derived)
            .any(|k| k.trim() == key);
        if !reaches {
            continue;
        }
        written.insert(key.to_string(), value.clone());
        if crate::memory::types::holds_a_value(value) {
            after.insert(key.to_string(), value.clone());
        } else {
            after.remove(key);
        }
    }
    (after, written)
}

/// **The refusal the numbering decision gives a capture, if it gives one.**
pub(super) fn refuses(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<Refusal> {
    let subject = handle(state, home);
    let held = state.fields(home, declared);
    let (after, written) = after_the_write(&held, request);
    let mut reach = Reach::default();
    for (container, key) in state.numbering_scopes(home) {
        reach
            .scopes
            .push(collection(state, declared, &container, &key));
    }
    let now = state.numbers_by(home);
    let old = now
        .as_ref()
        .map(|key| collection(state, declared, home, key));
    match numbering_change(Some(&held), &after) {
        Some(KeyChange::By(key)) => reach.founded.push(Founding {
            old,
            new: Some(collection(state, declared, home, &key)),
        }),
        Some(KeyChange::Off) => reach.founded.push(Founding { old, new: None }),
        None => {}
    }
    let change = Change::of_capture(&subject, &after, &written);
    let refused = reach.refuses(&change)?;
    let container_of = |name: &str| {
        state
            .resolve(name)
            .cloned()
            .unwrap_or_else(|| ObjectId(name.to_string()))
    };
    Some(match refused {
        MemoryError::NumberTaken {
            collection,
            key,
            number,
            holder,
            next,
        } => Refusal::NumberHeld {
            container: container_of(&collection),
            key,
            number,
            holder: container_of(&holder),
            next,
        },
        MemoryError::NumberRetired {
            collection,
            key,
            number,
            next,
        } => Refusal::NumberRetired {
            container: container_of(&collection),
            key,
            number,
            next,
        },
        other => Refusal::Numbering(other),
    })
}
