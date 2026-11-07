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
//! hidden behind a handle it did. **A version says the thing has not moved, not
//! that the session received every part of it**: prose, details, facts, a history
//! window the session never asked for are parts it was not handed, so a thing
//! collapses only when every part its object now carries was served at this
//! version ([`Shape`], the set of those parts). The rendering is never compared:
//! a search hit carries the bare parts, so it collapses after any recall. Messages
//! are not collapsed: the mailbox already says by the store which messages were
//! delivered.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde_json::{Map, Value, json};

use super::{Held, Version, claim_address_of, served_in};
use crate::seen::Shape;

/// **The parts a read works out from its own question** (the record filter, the
/// day an overdue question names, the keys, the status or the view that narrows
/// it). No version sees the arguments of a call, so each is named by what the
/// question made of it and the same thing read under another question is
/// another part. The renderers in `memory/recall.rs` set these from an argument.
const FROM_THE_QUESTION: [&str; 10] = [
    "scoped_out",
    "overdue_by_days",
    "due_in_days",
    "fields_left_out",
    "facts_status",
    "archived_left_out",
    "active_left_out",
    "review_marks_left_out",
    "release_notes_record_left_out",
    "fields_backing_left_out",
];

/// The key the count rides under, at the top of an answer that collapsed.
pub(crate) const COLLAPSED: &str = "held_collapsed";

/// What a collapsed thing says in place of its body.
const UNCHANGED: &str = "unchanged since served";

/// **The parts no version of the thing can follow**: a thing that carries one
/// is served whole every time. A plan reads what is due, overdue and ready from
/// today's date, which no write moves. A walk (`connected`) reads every claim
/// anywhere that names the thing, and the version reads no such claim. **A part
/// that says nothing** (`null` is the placeholder of a plan not asked for, `[]` is
/// a walk that was not asked for or found nothing) hides no text, so it does not
/// count.
const NEVER_WITHHELD: [&str; 2] = ["plan", "connected"];

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

/// What the object `fields` is, when it is an entity or a claim: the thing, the
/// key it is named under and its name.
fn thing_of(fields: &Map<String, Value>) -> Option<(Held, &'static str, &str)> {
    if let Some(handle) = crate::seen::entity_handle_of(fields) {
        Some((Held::Entity(handle.to_string()), "id", handle))
    } else {
        claim_address_of(fields)
            .map(|address| (Held::Claim(address.to_string()), "address", address))
    }
}

/// **The keys every rendering of a thing carries**, a recall and a search hit
/// alike: who it is and how a hit frames it. Serving them is not serving a part.
const BARE: [&str; 13] = [
    "id",
    "name",
    "type",
    "source",
    "boot",
    "hit",
    "parent",
    "address",
    "content",
    "provenance",
    "recorded_at",
    "standing",
    "status",
];

/// A page is named by its rows and the record keys it serves, never their prose.
/// Histories also name what they read, how many writes they show of how many,
/// and each write's place. Journal wordings belong to their entry.
fn part_window(key: &str, value: &Value) -> Option<String> {
    fn rows(value: &Value) -> Value {
        value
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        json!([
                            row.get("address"),
                            row.get("record"),
                            row.get("nth"),
                            row.get("position"),
                            row.get("revision"),
                            row.as_object()
                                .map(|fields| fields.keys().collect::<Vec<_>>()),
                        ])
                    })
                    .collect::<Vec<_>>()
                    .into()
            })
            .unwrap_or(Value::Null)
    }

    let window = match key {
        "entries" | "facts" | "ticks" => rows(value),
        "history" | "record_history" | "entity_history" => json!([
            value.get("key"),
            value.get("record"),
            value.get("entity"),
            value.get("count"),
            value.get("shown"),
            rows(&value["writes"]),
            rows(&value["versions"]),
            value
                .get("entries")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|entry| {
                            json!([
                                entry.get("entry"),
                                entry.get("count"),
                                rows(&entry["wordings"])
                            ])
                        })
                        .collect::<Vec<_>>()
                }),
        ]),
        _ => return None,
    };
    Some(window.to_string())
}

/// **The parts one served object carries, nested things and all.** A part is a
/// key beyond [`BARE`] on a thing (prose, facts, details, history and the rest),
/// named with the thing that carries it; a history is named by the key it reads
/// too. Production never compares values: the version says whether a thing moved,
/// and the parts say what of it the session was handed.
fn parts_of(fields: &Map<String, Value>, part: &mut impl FnMut(u64, &str, &Value)) {
    fn token(owner: &str, key: &str, window: Option<&str>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (owner, key, window).hash(&mut hasher);
        hasher.finish()
    }
    fn collect(value: &Value, owner: &str, part: &mut impl FnMut(u64, &str, &Value)) {
        match value {
            Value::Object(fields) => {
                let named = thing_of(fields).map(|(_, _, name)| name);
                let owner = named.unwrap_or(owner);
                if named.is_some() {
                    for (key, inner) in fields {
                        if BARE.contains(&key.as_str()) {
                            continue;
                        }
                        // **The held block is named by the frame it was read under**: the
                        // day it stands on (`as_of`) and the note that says which way the
                        // call asked (a walk of its own, or a plain read). The same block
                        // read under another frame says other things.
                        let frame = match key.as_str() {
                            "held" => {
                                let text = |name: &str| {
                                    inner.get(name).and_then(Value::as_str).unwrap_or_default()
                                };
                                Some(format!("{}|{}", text("as_of"), text("note")))
                            }
                            // A walk that found nothing is a result, not the
                            // absence of one: it is another part than a walk that
                            // found a neighbor.
                            "connected" => Some(says_something(inner).to_string()),
                            question if FROM_THE_QUESTION.contains(&question) => {
                                Some(inner.to_string())
                            }
                            _ => None,
                        };
                        let window = frame.or_else(|| part_window(key, inner));
                        part(token(owner, key, window.as_deref()), key, inner);
                        // A read narrowed by `keys` serves some of the fields, so
                        // each field key is a part of its own (names, never values).
                        if key == "fields"
                            && let Some(served) = inner.as_object()
                        {
                            for (field, value) in served {
                                let key = format!("fields.{field}");
                                part(token(owner, &key, None), &key, value);
                            }
                        }
                    }
                }
                fields
                    .values()
                    .for_each(|inner| collect(inner, owner, part));
            }
            Value::Array(items) => items.iter().for_each(|item| collect(item, owner, part)),
            _ => {}
        }
    }
    collect(&Value::Object(fields.clone()), "", part);
}

fn shape_of(fields: &Map<String, Value>) -> Shape {
    let mut out = Shape::new();
    parts_of(fields, &mut |token, _, _| {
        out.insert(token);
    });
    out
}

#[cfg(any(test, feature = "testing"))]
fn part_values(fields: &Map<String, Value>) -> crate::seen::PartValues {
    let mut out = crate::seen::PartValues::new();
    parts_of(fields, &mut |token, key, value| {
        // Narrowed reads serve individual fields, not the whole fields map.
        if key != "fields" {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            value.to_string().hash(&mut hasher);
            out.entry(token)
                .or_insert_with(|| (key.to_string(), Default::default()))
                .1
                .insert(hasher.finish());
        }
    });
    out
}

#[cfg(any(test, feature = "testing"))]
pub(super) fn values_in(body: &Value) -> Vec<(Held, crate::seen::PartValues)> {
    fn walk(value: &Value, out: &mut Vec<(Held, crate::seen::PartValues)>) {
        match value {
            Value::Object(fields) => {
                if let Some((what, ..)) = thing_of(fields) {
                    out.push((what, part_values(fields)));
                }
                fields.values().for_each(|inner| walk(inner, out));
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(body, &mut out);
    out
}

/// Test the very objects the production predicate would hide, before hiding them.
#[cfg(any(test, feature = "testing"))]
pub(super) fn assert_received(
    body: &Value,
    unchanged: &[Held],
    shapes: &HashMap<Held, Shape>,
    values: &HashMap<Held, crate::seen::PartValues>,
    verb: &str,
) {
    match body {
        Value::Object(fields) => {
            if stub_of(fields, unchanged, shapes).is_some() {
                let (what, _, name) = thing_of(fields).expect("a collapse names its thing");
                for (token, (part, now)) in part_values(fields) {
                    assert!(
                        values
                            .get(&what)
                            .and_then(|had| had.get(&token))
                            .is_some_and(|(_, was)| now.is_subset(was)),
                        "collapse withheld unseen text: thing={name}, part={part}, verb={verb}"
                    );
                }
                return;
            }
            for inner in fields.values() {
                assert_received(inner, unchanged, shapes, values, verb);
            }
        }
        Value::Array(items) => items.iter().for_each(|inner| {
            assert_received(inner, unchanged, shapes, values, verb);
        }),
        _ => {}
    }
}

#[cfg(any(test, feature = "testing"))]
pub(super) fn received_values(
    unchanged: &[Held],
    held: &HashMap<Held, crate::seen::PartValues>,
    served: Vec<(Held, crate::seen::PartValues)>,
) -> Vec<(Held, crate::seen::PartValues)> {
    let mut out: HashMap<Held, crate::seen::PartValues> = HashMap::new();
    for (what, values) in served {
        let entry = out.entry(what.clone()).or_insert_with(|| {
            if unchanged.contains(&what) {
                held.get(&what).cloned().unwrap_or_default()
            } else {
                crate::seen::PartValues::new()
            }
        });
        for (token, (part, values)) in values {
            entry
                .entry(token)
                .or_insert_with(|| (part, Default::default()))
                .1
                .extend(values);
        }
    }
    out.into_iter().collect()
}

/// **The fingerprint of every entity and claim an answer names**, once for each
/// time it names one.
pub(super) fn shapes_in(body: &Value) -> Vec<(Held, Shape)> {
    fn walk(value: &Value, out: &mut Vec<(Held, Shape)>) {
        match value {
            Value::Object(fields) => {
                if let Some((what, ..)) = thing_of(fields) {
                    out.push((what, shape_of(fields)));
                }
                fields.values().for_each(|inner| walk(inner, out));
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(body, &mut out);
    out
}

/// **What the session has received of each thing after this answer**: the texts
/// it had before, when the thing has not moved, and the texts of this answer.
pub(super) fn received(
    before: &[(Held, Version)],
    held_shapes: &HashMap<Held, Shape>,
    served: &[(Held, Version)],
    shapes: Vec<(Held, Shape)>,
) -> Vec<(Held, Shape)> {
    let mut out: HashMap<Held, Shape> = HashMap::new();
    for (what, shape) in shapes {
        let entry = out.entry(what.clone()).or_insert_with(|| {
            let not_moved = served.iter().any(|(held, now)| {
                *held == what
                    && before
                        .iter()
                        .any(|(was, version)| *was == what && version == now)
            });
            if not_moved {
                held_shapes.get(&what).cloned().unwrap_or_default()
            } else {
                Shape::new()
            }
        });
        entry.extend(shape);
    }
    out.into_iter().collect()
}

/// **Replace each unchanged thing in `body` with its handle or address**, and say
/// so once at the top. Returns how many were replaced.
pub(super) fn collapse(
    body: &mut Value,
    unchanged: &[Held],
    received: &HashMap<Held, Shape>,
) -> usize {
    if unchanged.is_empty() {
        return 0;
    }
    let mut count = 0;
    visit(body, unchanged, received, &mut count);
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

fn visit(
    value: &mut Value,
    unchanged: &[Held],
    received: &HashMap<Held, Shape>,
    count: &mut usize,
) {
    match value {
        Value::Object(fields) => {
            if let Some(stub) = stub_of(fields, unchanged, received) {
                *fields = stub;
                *count += 1;
                return;
            }
            // A thing carrying such a part is served whole, claims and all.
            if thing_of(fields).is_some() && carries_a_part_no_version_follows(fields) {
                return;
            }
            for held in fields.values_mut() {
                visit(held, unchanged, received, count);
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| visit(item, unchanged, received, count)),
        _ => {}
    }
}

/// Whether `fields` carries a part in [`NEVER_WITHHELD`], read.
fn carries_a_part_no_version_follows(fields: &Map<String, Value>) -> bool {
    NEVER_WITHHELD
        .iter()
        .any(|part| fields.get(*part).is_some_and(says_something))
}

/// Whether a part holds anything a reader could be handed.
fn says_something(read: &Value) -> bool {
    match read {
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
        _ => true,
    }
}

/// The handle or address alone, when `fields` is a thing held unchanged and
/// everything it carries is held unchanged too.
fn stub_of(
    fields: &Map<String, Value>,
    unchanged: &[Held],
    received: &HashMap<Held, Shape>,
) -> Option<Map<String, Value>> {
    let (what, key, name) = thing_of(fields)?;
    if !unchanged.contains(&what) {
        return None;
    }
    if carries_a_part_no_version_follows(fields) {
        return None;
    }
    // A text the session never received is delivered, whatever the version says.
    if !received
        .get(&what)
        .is_some_and(|had| shape_of(fields).is_subset(had))
    {
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
        Version::Entity {
            own: jiff::Timestamp::from_second(day as i64 * 86_400).ok(),
            around: None,
            children: (0, 0),
        }
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

    /// What a session has received of each thing named in `body`.
    fn received_of(body: &Value) -> HashMap<Held, Shape> {
        let mut out: HashMap<Held, Shape> = HashMap::new();
        for (what, shape) in shapes_in(body) {
            out.entry(what).or_default().extend(shape);
        }
        out
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
        let received = received_of(&body);
        let unchanged = vec![
            entity("person:homer"),
            Held::Claim("person:homer#f1".into()),
        ];
        assert_eq!(collapse(&mut body, &unchanged, &received), 1);
        assert_eq!(
            body["objects"][0],
            json!({"id": "person:homer", "held": UNCHANGED}),
            "{body}"
        );
        assert_eq!(body[COLLAPSED]["count"], 1, "{body}");
    }

    /// **A thing that carries a part no version can follow stays whole even when
    /// nothing was written**: a plan reads the day, a walk reads every claim that
    /// names the thing. The placeholder of a plan not asked for does not count.
    #[test]
    fn a_thing_carrying_a_part_no_version_follows_is_never_withheld() {
        let rows = [
            (
                json!({"plan": {"due": [], "overdue": [], "ready": []}}),
                false,
            ),
            (json!({"plan": null}), true),
            (json!({"connected": []}), true),
            (
                json!({"connected": [{"id": "list:packing", "name": "Packing"}]}),
                false,
            ),
            (json!({}), true),
        ];
        for (part, collapses) in rows {
            let mut list = json!({"id": "list:errands", "name": "Errands"});
            list.as_object_mut()
                .expect("an object")
                .extend(part.as_object().expect("an object").clone());
            let mut body = json!({"objects": [list]});
            let received = received_of(&body);
            let count = collapse(&mut body, &[entity("list:errands")], &received);
            assert_eq!(count, usize::from(collapses), "{body}");
        }
    }

    /// **A walk that found nothing, after one that found a neighbor, is served
    /// again**: an empty answer is a result the session was not handed, and an
    /// empty walk is the same part as another empty one. The guard counts it.
    #[test]
    fn a_walk_that_found_nothing_after_one_that_did_is_served_again() {
        let walked = |connected: Value| json!({"objects": [{"id": "person:homer", "name": "Homer", "connected": connected}]});
        let first = walked(json!([{"id": "person:marge", "name": "Marge"}]));
        let received = received_of(&first);
        let mut empty = walked(json!([]));
        assert_eq!(
            collapse(&mut empty, &[entity("person:homer")], &received),
            0
        );
        let mut again = walked(json!([]));
        let received = received_of(&empty);
        assert_eq!(
            collapse(&mut again, &[entity("person:homer")], &received),
            1
        );
        let object = empty["objects"][0].as_object().expect("an object");
        assert!(
            !part_values(object).is_empty(),
            "the guard counts an empty walk as a value"
        );
    }

    /// **A part the read worked out from its own question is named by what that
    /// question made of it**: the same thing served again under another record
    /// filter, another day or another narrowing ships the part, and under the same
    /// question it collapses. One row per part the renderers set from an argument.
    #[test]
    fn a_part_worked_out_from_the_question_is_named_by_it() {
        let served = |part: &str, value: Value| {
            let mut object = json!({"id": "rhythm:pay-rent-late", "name": "Pay rent late"});
            object[part] = value;
            json!({"objects": [object]})
        };
        let rows = [
            (
                "scoped_out",
                json!("2 more records do not answer the filter"),
                json!("5 more records do not answer the filter"),
            ),
            ("overdue_by_days", json!(3), json!(4)),
            ("due_in_days", json!(3), json!(4)),
            (
                "fields_left_out",
                json!("2 keys are left out"),
                json!("3 keys are left out"),
            ),
            ("facts_status", json!("active"), json!("archived")),
            (
                "archived_left_out",
                json!({"count": 1}),
                json!({"count": 2}),
            ),
            ("active_left_out", json!({"count": 1}), json!({"count": 2})),
            (
                "review_marks_left_out",
                json!({"count": 1}),
                json!({"count": 2}),
            ),
            (
                "release_notes_record_left_out",
                json!({"count": 1}),
                json!({"count": 2}),
            ),
            (
                "fields_backing_left_out",
                json!("1 key is not backed"),
                json!("2 keys are not backed"),
            ),
        ];
        for (part, asked, other) in rows {
            let received = received_of(&served(part, asked.clone()));
            let held = [entity("rhythm:pay-rent-late")];
            let mut same = served(part, asked);
            assert_eq!(
                collapse(&mut same, &held, &received),
                1,
                "{part}: the same question"
            );
            let mut changed = served(part, other);
            assert_eq!(
                collapse(&mut changed, &held, &received),
                0,
                "{part}: another question"
            );
        }
    }

    /// **A held block read under another frame is another part**: the same
    /// thing read on another day, or by a call that walks, has not been handed
    /// what this answer says.
    #[test]
    fn a_held_block_read_under_another_frame_is_another_part() {
        let held = |as_of: &str, note: &str| {
            json!({"id": "work:phi", "name": "Phi",
                   "held": {"as_of": as_of, "count": 0, "items": [], "note": note}})
        };
        let plain = held("2026-06-01", "nothing points at this");
        let shapes = |what: &Value| {
            let mut shape = Shape::new();
            for (_, parts) in shapes_in(&json!({"objects": [what]})) {
                shape.extend(parts);
            }
            shape
        };
        let had = shapes(&plain);
        assert!(shapes(&plain).is_subset(&had), "the same frame is held");
        assert!(
            !shapes(&held("2026-06-02", "nothing points at this")).is_subset(&had),
            "another day is another part"
        );
        assert!(
            !shapes(&held("2026-06-01", "this call follows links of its own")).is_subset(&had),
            "a walk is another part"
        );
    }

    /// **A thing served whole for such a part keeps its claims whole**, held
    /// unchanged or not: nothing inside it is withheld behind an address.
    #[test]
    fn a_thing_carrying_a_walk_keeps_its_claims_whole() {
        let mut body = json!({"objects": [{
            "id": "list:errands",
            "name": "Errands",
            "connected": [{"id": "list:packing", "name": "Packing"}],
            "facts": [{
                "address": "list:errands#f1",
                "content": "buy milk",
                "provenance": "testimony",
            }],
        }]});
        let received = received_of(&body);
        let unchanged = vec![
            entity("list:errands"),
            Held::Claim("list:errands#f1".into()),
        ];
        assert_eq!(collapse(&mut body, &unchanged, &received), 0);
        assert_eq!(
            body["objects"][0]["facts"][0]["content"], "buy milk",
            "{body}"
        );
    }

    /// **An entity holding a claim the session never saw stays whole**, and the
    /// claim it did see collapses inside it.
    #[test]
    fn an_entity_carrying_an_unseen_claim_stays_whole() {
        let mut body = json!({"objects": [homer_whole()]});
        let received = received_of(&body);
        let unchanged = vec![entity("person:homer")];
        assert_eq!(collapse(&mut body, &unchanged, &received), 0);
        assert_eq!(body["objects"][0], homer_whole(), "{body}");
        assert!(body.get(COLLAPSED).is_none(), "{body}");

        let both = vec![entity("person:bart"), Held::Claim("person:homer#f1".into())];
        let mut body = json!({"objects": [homer_whole()]});
        let received = received_of(&body);
        assert_eq!(collapse(&mut body, &both, &received), 1);
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
        let received = received_of(&body);
        assert_eq!(collapse(&mut body, &[], &received), 0);
        assert_eq!(body, json!({"objects": [homer_whole()]}));
    }

    /// **Text the session never received is delivered, though the version has not
    /// moved.** Homer was served bare; asked again with his prose, the page has
    /// never been read, so he comes back whole. The same text twice is a handle.
    #[test]
    fn text_never_received_is_delivered_and_text_received_is_not_again() {
        let bare = json!({"objects": [{"id": "person:homer", "name": "Homer"}]});
        let with_prose = json!({"objects": [
            {"id": "person:homer", "name": "Homer", "prose": "Lives next door."}
        ]});
        let unchanged = vec![entity("person:homer")];

        let mut asked = with_prose.clone();
        assert_eq!(collapse(&mut asked, &unchanged, &received_of(&bare)), 0);
        assert_eq!(asked, with_prose, "the page was never read: {asked}");

        let mut again = with_prose.clone();
        assert_eq!(
            collapse(&mut again, &unchanged, &received_of(&with_prose)),
            1,
            "the same text is not served twice: {again}"
        );
    }

    /// **A different value in the same layout is different text.** Two histories
    /// of one unchanged thing have the same keys and the same number of rows; the
    /// second is the first time the session reads its values. A claim's details,
    /// asked for after the claim was served without them, are the same case one
    /// level down: the parent carries the claim's text whole.
    #[test]
    fn a_value_never_received_in_a_familiar_layout_is_delivered() {
        let mood = json!({"objects": [{"id": "person:homer", "name": "Homer",
            "history": {"key": "mood", "count": 1, "shown": 1, "writes": [{"record": "person:homer#f2", "value": "calm"}]}}]});
        let colour = json!({"objects": [{"id": "person:homer", "name": "Homer",
            "history": {"key": "colour", "count": 1, "shown": 1, "writes": [{"record": "person:homer#f3", "value": "yellow"}]}}]});
        let unchanged = vec![entity("person:homer")];
        let mut asked = colour.clone();
        assert_eq!(collapse(&mut asked, &unchanged, &received_of(&mood)), 0);
        assert_eq!(asked, colour, "{asked}");

        let claim = |details: Option<&str>| {
            let mut fact = json!({
                "address": "person:homer#f1",
                "content": "likes donuts",
                "provenance": "testimony",
            });
            if let Some(details) = details {
                fact["details"] = json!(details);
            }
            json!({"objects": [{"id": "person:homer", "name": "Homer", "facts": [fact]}]})
        };
        let held = vec![
            entity("person:homer"),
            Held::Claim("person:homer#f1".into()),
        ];
        let mut with_details = claim(Some("since the wedding"));
        assert_eq!(
            collapse(&mut with_details, &held, &received_of(&claim(None))),
            0,
            "the requested details were never served: {with_details}"
        );
        assert_eq!(with_details, claim(Some("since the wedding")));
    }

    /// **The collapse decision, one row per case**: what the session held before
    /// the call, the parts it was handed, and what the answer now serves. The
    /// version decides moved or not; the parts decide received or not; the
    /// rendering is never compared. (`reship: true` is decided before this
    /// function, in the held pass, and is pinned by held_changed's story.)
    #[test]
    fn the_collapse_decision_table() {
        let homer = entity("person:homer");
        let bare = json!({"id": "person:homer", "name": "Homer"});
        let with_prose = json!({"id": "person:homer", "name": "Homer", "prose": "x"});
        let with_facts = json!({"id": "person:homer", "name": "Homer", "facts": []});
        //   (the row, what was held before as (version, the answer it was served
        //    in), the version now, the answer now, whether it collapses)
        let table = [
            (
                "same version, bare again",
                Some((1, &with_facts)),
                1,
                &bare,
                true,
            ),
            (
                "same version, same parts",
                Some((1, &with_prose)),
                1,
                &with_prose,
                true,
            ),
            (
                "same version, a part never served",
                Some((1, &bare)),
                1,
                &with_prose,
                false,
            ),
            (
                "held, then changed",
                Some((1, &with_facts)),
                2,
                &with_facts,
                false,
            ),
            ("not held before this call", None, 1, &with_facts, false),
        ];
        for (row, before, now, answer, collapses) in table {
            let held_before: Vec<(Held, Version)> = before
                .iter()
                .map(|(version, _)| (homer.clone(), at(*version)))
                .collect();
            let received: HashMap<Held, Shape> = before
                .iter()
                .flat_map(|(_, served)| shapes_in(served))
                .collect();
            let served = vec![(homer.clone(), at(now))];
            let mut body = json!({"objects": [answer]});
            let unchanged = unchanged(&held_before, &served);
            let count = collapse(&mut body, &unchanged, &received);
            assert_eq!(count == 1, collapses, "{row}: {body}");
        }
    }

    /// **A narrower read of a thing's fields is not a read of all of them.** A
    /// recall that asked for one key served that key; the same thing, unmoved,
    /// asked again for every key carries fields the session never received, so
    /// it comes back whole. The same keys again are a handle.
    #[test]
    fn a_narrowed_fields_read_is_not_the_whole_fields() {
        let narrow = json!({"objects": [{"id": "person:homer", "name": "Homer",
            "fields": {"status": "calm"}}]});
        let wide = json!({"objects": [{"id": "person:homer", "name": "Homer",
            "fields": {"status": "calm", "owner": "marge"}}]});
        let unchanged = vec![entity("person:homer")];

        let mut asked = wide.clone();
        assert_eq!(collapse(&mut asked, &unchanged, &received_of(&narrow)), 0);
        assert_eq!(asked, wide, "the other fields were never served: {asked}");

        let mut again = narrow.clone();
        assert_eq!(collapse(&mut again, &unchanged, &received_of(&narrow)), 1);
    }

    /// **A wider window of the same history is rows the session never received.**
    /// The key is the same and the thing has not moved; the second window
    /// carries a record the first did not, so it comes back whole. The same
    /// window again is a handle.
    #[test]
    fn a_wider_history_window_is_delivered() {
        // The shape the server serves (recall's history_json and record_history_json).
        let history = |shown: usize| {
            let writes: Vec<Value> = ["person:homer#f4", "person:homer#f3", "person:homer#f2"]
                .iter()
                .take(shown)
                .map(|record| json!({"record": record, "value": "calm"}))
                .collect();
            json!({"objects": [{"id": "person:homer", "name": "Homer",
                "history": {"key": "mood", "count": 3, "shown": shown, "writes": writes}}]})
        };
        let record_history = |shown: usize| {
            let writes: Vec<Value> = (1..=shown).rev().map(|nth| json!({"nth": nth})).collect();
            json!({"objects": [{"id": "person:homer", "name": "Homer",
                "record_history": {"record": "person:homer#f1", "count": 3, "shown": shown,
                    "writes": writes}}]})
        };
        let unchanged = vec![entity("person:homer")];

        for (narrow, wide) in [
            (history(1), history(2)),
            (record_history(1), record_history(2)),
        ] {
            let mut asked = wide.clone();
            assert_eq!(collapse(&mut asked, &unchanged, &received_of(&narrow)), 0);
            assert_eq!(asked, wide, "{asked}");

            let mut again = narrow.clone();
            assert_eq!(collapse(&mut again, &unchanged, &received_of(&narrow)), 1);
        }
    }

    /// Paging changes the received piece even when the owner has not moved.
    /// These are the blocks emitted by recall's list and history renderers and
    /// wire::narrow_record; their prose is not part of a window's identity.
    #[test]
    fn each_piece_has_its_own_part_token() {
        fn token_of(key: &str, value: &Value) -> u64 {
            let mut object = json!({"id": "person:homer", "name": "Homer"});
            object[key] = value.clone();
            let mut tokens = Vec::new();
            parts_of(object.as_object().unwrap(), &mut |token, part, _| {
                if part == key {
                    tokens.push(token);
                }
            });
            assert_eq!(tokens.len(), 1, "one named part: {object}");
            tokens[0]
        }

        let table = [
            (
                "entries offset",
                "entries",
                json!([{"address": "list:packing#f2", "text": "pack", "state": "open"}]),
                json!([{"address": "list:packing#f3", "text": "pack", "state": "open"}]),
                false,
            ),
            (
                "entries limit",
                "entries",
                json!([{"address": "list:packing#f2", "text": "pack", "state": "open"}]),
                json!([{"address": "list:packing#f2", "text": "pack", "state": "open"},
                       {"address": "list:packing#f3", "text": "pack", "state": "open"}]),
                false,
            ),
            (
                "entries past the end",
                "entries",
                json!([{"address": "list:packing#f2", "text": "pack", "state": "open"}]),
                json!([]),
                false,
            ),
            (
                "ticks offset",
                "ticks",
                json!([{"address": "list:packing#f4", "entry": "list:packing#f2", "act": "done", "note": "packed"}]),
                json!([{"address": "list:packing#f5", "entry": "list:packing#f2", "act": "undo", "note": "unpacked"}]),
                false,
            ),
            (
                "record keys",
                "facts",
                json!([{"address": "topic:beta#f1", "content": "opened", "provenance": "testimony"}]),
                json!([{"address": "topic:beta#f1", "content": "opened", "details": "reasons"}]),
                false,
            ),
            (
                "facts offset",
                "facts",
                json!([{"address": "topic:beta#f1", "content": "opened", "provenance": "testimony"}]),
                json!([{"address": "topic:beta#f2", "content": "closed", "provenance": "testimony"}]),
                false,
            ),
            (
                "history key",
                "history",
                json!({"key": "mood", "count": 1, "shown": 1, "writes": [{"record": "person:homer#f1", "value": "calm"}]}),
                json!({"key": "colour", "count": 1, "shown": 1, "writes": [{"record": "person:homer#f1", "value": "yellow"}]}),
                false,
            ),
            (
                "history most",
                "history",
                json!({"key": "mood", "count": 2, "shown": 1, "writes": [{"record": "person:homer#f1", "value": "calm"}]}),
                json!({"key": "mood", "count": 2, "shown": 2, "writes": [{"record": "person:homer#f1", "value": "calm"}, {"record": "person:homer#f2", "value": "calm"}]}),
                false,
            ),
            (
                "record history window",
                "record_history",
                json!({"record": "person:homer#f1", "count": 2, "shown": 1, "writes": [{"nth": 1, "content": "calm"}]}),
                json!({"record": "person:homer#f1", "count": 2, "shown": 1, "writes": [{"nth": 2, "content": "calm"}]}),
                false,
            ),
            (
                "record history details",
                "record_history",
                json!({"record": "person:homer#f1", "count": 1, "shown": 1, "writes": [{"nth": 1, "content": "calm"}]}),
                json!({"record": "person:homer#f1", "count": 1, "shown": 1, "writes": [{"nth": 1, "content": "calm", "details": "reasons"}]}),
                false,
            ),
            (
                "entity history window",
                "entity_history",
                json!({"entity": "person:homer", "count": 2, "shown": 1, "versions": [{"nth": 1, "name": "Homer"}]}),
                json!({"entity": "person:homer", "count": 2, "shown": 1, "versions": [{"nth": 2, "name": "Homer"}]}),
                false,
            ),
            (
                "session field offset",
                "history",
                json!({"key": "focus", "count": 2, "shown": 1, "writes": [{"position": 1, "value": "reading"}]}),
                json!({"key": "focus", "count": 2, "shown": 1, "writes": [{"position": 2, "value": "reading"}]}),
                false,
            ),
            (
                "journal wording offset",
                "history",
                json!({"key": "journal", "count": 2, "shown": 1, "entries": [{"entry": "m1", "count": 2, "wordings": [{"revision": 1, "text": "read"}]}]}),
                json!({"key": "journal", "count": 2, "shown": 1, "entries": [{"entry": "m1", "count": 2, "wordings": [{"revision": 2, "text": "read"}]}]}),
                false,
            ),
            (
                "same identifiers and keys, different prose",
                "entries",
                json!([{"address": "list:packing#f2", "text": "pack", "state": "open"}]),
                json!([{"address": "list:packing#f2", "text": "unpack", "state": "done"}]),
                true,
            ),
            (
                "unpaged prose",
                "prose",
                json!("read"),
                json!("write"),
                true,
            ),
        ];
        let mut wrong = Vec::new();
        for (row, key, first, next, same) in table {
            let token = token_of(key, &first);
            assert_eq!(token, token_of(key, &first), "same piece: {row}");
            if (token == token_of(key, &next)) != same {
                wrong.push(row);
            }
        }
        assert!(wrong.is_empty(), "wrong piece tokens: {wrong:?}");
    }

    /// **Received parts are kept while a thing has not moved and start again when
    /// it has.**
    #[test]
    fn received_parts_are_kept_until_the_thing_moves() {
        let homer = entity("person:homer");
        let bare = json!({"id": "person:homer", "name": "Homer"});
        let rich = json!({"id": "person:homer", "name": "Homer", "prose": "x"});
        let held_shapes = received_of(&rich);
        let before = vec![(homer.clone(), at(1))];

        let still = received(
            &before,
            &held_shapes,
            &[(homer.clone(), at(1))],
            shapes_in(&bare),
        );
        assert!(
            still[0].1.is_superset(&held_shapes[&homer]),
            "a bare answer adds nothing to what was received"
        );

        let moved = received(
            &before,
            &held_shapes,
            &[(homer.clone(), at(2))],
            shapes_in(&bare),
        );
        assert_eq!(
            moved[0].1,
            shapes_in(&bare)[0].1,
            "a new version starts again"
        );
    }
}
