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

/// **The parts one served object carries, nested things and all.** A part is a
/// key beyond [`BARE`] on a thing (prose, facts, details, history and the rest),
/// named with the thing that carries it; a history is named by the key it reads
/// too. The values are never compared: the version says whether a thing moved,
/// and the parts say what of it the session was handed.
fn shape_of(fields: &Map<String, Value>) -> Shape {
    fn token(owner: &str, key: &str, window: Option<&str>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (owner, key, window).hash(&mut hasher);
        hasher.finish()
    }
    fn collect(value: &Value, owner: &str, out: &mut Shape) {
        match value {
            Value::Object(fields) => {
                let named = thing_of(fields).map(|(_, _, name)| name);
                let owner = named.unwrap_or(owner);
                if named.is_some() {
                    for (key, inner) in fields {
                        if BARE.contains(&key.as_str()) {
                            continue;
                        }
                        // A history is named by the key it reads and the window it
                        // shows (how many records, and which one opens it).
                        let window = (key == "history").then(|| {
                            let records = inner.get("records").and_then(Value::as_array);
                            format!(
                                "{}:{}:{}",
                                inner.get("key").and_then(Value::as_str).unwrap_or_default(),
                                records.map_or(0, Vec::len),
                                records
                                    .and_then(|rows| rows.first())
                                    .and_then(|row| row.get("record"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                            )
                        });
                        out.insert(token(owner, key, window.as_deref()));
                        // A read narrowed by `keys` serves some of the fields, so
                        // each field key is a part of its own (names, never values).
                        if key == "fields"
                            && let Some(served) = inner.as_object()
                        {
                            for field in served.keys() {
                                out.insert(token(owner, &format!("fields.{field}"), None));
                            }
                        }
                    }
                }
                fields.values().for_each(|inner| collect(inner, owner, out));
            }
            Value::Array(items) => items.iter().for_each(|item| collect(item, owner, out)),
            _ => {}
        }
    }
    let mut out = Shape::new();
    collect(&Value::Object(fields.clone()), "", &mut out);
    out
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
            "history": {"key": "mood", "records": [{"record": "r1", "value": "calm"}]}}]});
        let colour = json!({"objects": [{"id": "person:homer", "name": "Homer",
            "history": {"key": "colour", "records": [{"record": "r2", "value": "yellow"}]}}]});
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
        let history = |records: Value| {
            json!({"objects": [{"id": "person:homer", "name": "Homer",
                "history": {"key": "mood", "records": records}}]})
        };
        let first = history(json!([{"record": "r1", "value": "calm"}]));
        let wider = history(json!([
            {"record": "r1", "value": "calm"},
            {"record": "r0", "value": "tired"}
        ]));
        let unchanged = vec![entity("person:homer")];

        let mut asked = wider.clone();
        assert_eq!(collapse(&mut asked, &unchanged, &received_of(&first)), 0);
        assert_eq!(asked, wider, "{asked}");

        let mut again = first.clone();
        assert_eq!(collapse(&mut again, &unchanged, &received_of(&first)), 1);
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
