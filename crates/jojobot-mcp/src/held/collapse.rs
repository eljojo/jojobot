//! **What a session already holds, unchanged, is not served again.**
//!
//! The held pass ([`Jojobot::add_held_changed`](super::Jojobot::add_held_changed))
//! decides once per answer which of the entities and claims it names the session
//! held before this call, at the version they stand at now. Each of those becomes
//! its handle or address alone, and the answer says once how many, and that the
//! same call with `reship: true` returns them whole.
//!
//! **A thing first served in this answer is never collapsed in it**: only what the
//! session held before the call is a candidate. **A changed thing stays whole**
//! beside its `held_changed` notice. **An entity collapses only when everything it
//! carries is held unchanged too**, so a claim the session never saw is never
//! hidden behind a handle it did. Messages are not collapsed: the mailbox already
//! says by the store which messages were delivered.

use serde_json::{Map, Value, json};

use super::{Held, Version, claim_address_of, served_in};

/// The key the count rides under, at the top of an answer that collapsed.
pub(crate) const COLLAPSED: &str = "held_collapsed";

/// What a collapsed thing says in place of its body.
const UNCHANGED: &str = "unchanged since served";

/// **The things this answer serves that the session held before it, at the same
/// version.** `before` is what the session held when the call began; `served` is
/// each thing the answer names, at the version it has now.
pub(super) fn unchanged(before: &[(Held, Version)], served: &[(Held, Version)]) -> Vec<Held> {
    served
        .iter()
        .filter(|(what, _)| !matches!(what, Held::Message(_)))
        .filter(|(what, now)| before.iter().any(|(held, was)| held == what && was == now))
        .map(|(what, _)| what.clone())
        .collect()
}

/// **Replace each unchanged thing in `body` with its handle or address**, and say
/// so once at the top. Returns how many were replaced.
pub(super) fn collapse(body: &mut Value, unchanged: &[Held]) -> usize {
    if unchanged.is_empty() {
        return 0;
    }
    let mut count = 0;
    visit(body, unchanged, &mut count);
    if count > 0
        && let Some(fields) = body.as_object_mut()
    {
        fields.insert(
            COLLAPSED.into(),
            json!({
                "count": count,
                "held": UNCHANGED,
                "reship": "send the same call with reship true to have them whole",
            }),
        );
    }
    count
}

fn visit(value: &mut Value, unchanged: &[Held], count: &mut usize) {
    match value {
        Value::Object(fields) => {
            if let Some(stub) = stub_of(fields, unchanged) {
                *fields = stub;
                *count += 1;
                return;
            }
            for held in fields.values_mut() {
                visit(held, unchanged, count);
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| visit(item, unchanged, count)),
        _ => {}
    }
}

/// The handle or address alone, when `fields` is a thing held unchanged and
/// everything it carries is held unchanged too.
fn stub_of(fields: &Map<String, Value>, unchanged: &[Held]) -> Option<Map<String, Value>> {
    let (what, key, name) = if let Some(handle) = crate::seen::entity_handle_of(fields) {
        (Held::Entity(handle.to_string()), "id", handle)
    } else if let Some(address) = claim_address_of(fields) {
        (Held::Claim(address.to_string()), "address", address)
    } else {
        return None;
    };
    if !unchanged.contains(&what) {
        return None;
    }
    let carried_whole = fields
        .values()
        .flat_map(served_in)
        .all(|inside| unchanged.contains(&inside));
    carried_whole.then(|| {
        let mut stub = Map::new();
        stub.insert(key.into(), Value::String(name.to_string()));
        stub.insert("held".into(), Value::String(UNCHANGED.into()));
        stub
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(handle: &str) -> Held {
        Held::Entity(handle.into())
    }

    fn at(day: u64) -> Version {
        Version::Entity(jiff::Timestamp::from_second(day as i64 * 86_400).ok())
    }

    /// **The decision, one row per case.** A thing held and unchanged is not
    /// served again; a changed one, one never held and one the session holds only
    /// as a message are served whole.
    #[test]
    fn the_decision_table() {
        let homer = entity("person:homer");
        let lisa = entity("person:lisa");
        let bart = entity("person:bart");
        let note = Held::Message("m1".into());
        let before = vec![
            (homer.clone(), at(1)),
            (lisa.clone(), at(1)),
            (note.clone(), Version::Message("new".into())),
        ];
        let served = vec![
            (homer.clone(), at(1)),
            (lisa.clone(), at(2)),
            (bart.clone(), at(1)),
            (note.clone(), Version::Message("new".into())),
        ];
        assert_eq!(
            unchanged(&before, &served),
            vec![homer],
            "held unchanged collapses; held changed, not held and a message do not"
        );
    }

    fn homer_whole() -> Value {
        json!({
            "id": "person:homer",
            "name": "Homer",
            "facts": [{
                "address": "person:homer#f1",
                "content": "likes donuts",
                "provenance": "testimony",
            }],
        })
    }

    /// **A held entity comes back as its handle, and the answer counts it.**
    #[test]
    fn a_held_entity_comes_back_as_its_handle_alone() {
        let mut body = json!({"objects": [homer_whole()]});
        let unchanged = vec![
            entity("person:homer"),
            Held::Claim("person:homer#f1".into()),
        ];
        assert_eq!(collapse(&mut body, &unchanged), 1);
        assert_eq!(
            body["objects"][0],
            json!({"id": "person:homer", "held": UNCHANGED}),
            "{body}"
        );
        assert_eq!(body[COLLAPSED]["count"], 1, "{body}");
    }

    /// **An entity holding a claim the session never saw stays whole**, and the
    /// claim it did see collapses inside it.
    #[test]
    fn an_entity_carrying_an_unseen_claim_stays_whole() {
        let mut body = json!({"objects": [homer_whole()]});
        let unchanged = vec![entity("person:homer")];
        assert_eq!(collapse(&mut body, &unchanged), 0);
        assert_eq!(body["objects"][0], homer_whole(), "{body}");
        assert!(body.get(COLLAPSED).is_none(), "{body}");

        let both = vec![entity("person:bart"), Held::Claim("person:homer#f1".into())];
        let mut body = json!({"objects": [homer_whole()]});
        assert_eq!(collapse(&mut body, &both), 1);
        assert_eq!(
            body["objects"][0]["facts"][0],
            json!({"address": "person:homer#f1", "held": UNCHANGED}),
            "the claim the session saw is not served again: {body}"
        );
        assert_eq!(body["objects"][0]["name"], "Homer", "{body}");
    }

    /// An answer with nothing held unchanged is left as it was.
    #[test]
    fn nothing_held_leaves_the_answer_alone() {
        let mut body = json!({"objects": [homer_whole()]});
        assert_eq!(collapse(&mut body, &[]), 0);
        assert_eq!(body, json!({"objects": [homer_whole()]}));
    }
}
