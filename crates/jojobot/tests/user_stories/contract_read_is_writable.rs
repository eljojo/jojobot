//! **Contract: whatever a read serves, the write that sets it accepts as served.**
//!
//! One world uses every write option once. Every claim and thing that any read
//! serves is copied into the write that sets it, and lands. Every key it serves is
//! a published argument of that write, a named rename, or named read-only: a key
//! that is none of these is a new mismatch, and the case names it.
use super::dsl::{Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};
use serde_json::{Value, json};

/// Served keys the write names differently: (verb, served, written, retired by).
const RENAMED: &[(&str, &str, &str, &str)] = &[
    (
        "capture",
        "edge",
        "shape",
        "the relation model, release 10 (rule 718)",
    ),
    (
        "update_entity",
        "id",
        "handle",
        "the release 10 vocabulary pass",
    ),
    (
        "update_entity",
        "alternateName",
        "aliases",
        "the release 10 vocabulary pass",
    ),
    (
        "update_entity",
        "fields",
        "sets",
        "the release 10 vocabulary pass",
    ),
];

/// Served keys no write sets: (verb, served, why).
const READ_ONLY: &[(&str, &str, &str)] = &[
    (
        "capture",
        "address",
        "names the claim; update_fact takes it",
    ),
    ("capture", "inserted_at", "the store's stamp"),
    ("capture", "written_at", "the store's stamp on a version"),
    ("capture", "nth", "a version's place in the history"),
    ("capture", "run", "the session that wrote a version"),
    ("capture", "revised", "counts rewrites"),
    ("capture", "revision_count", "counts rewrites"),
    (
        "capture",
        "status",
        "update_fact archives; capture never sets it",
    ),
    ("capture", "stands_for", "read from the entry's own links"),
    ("capture", "hit", "a search hit's envelope"),
    ("capture", "parent", "a search hit's thing"),
    (
        "update_entity",
        "type",
        "the kind's schema name; add_entity takes kind",
    ),
    ("update_entity", "boot", "set at creation"),
    ("update_entity", "parent", "set at creation"),
    ("update_entity", "as_of", "the day the answer is for"),
    ("update_entity", "connected", "computed from the graph"),
    ("update_entity", "entries", "written by capture"),
    ("update_entity", "facts", "written by capture"),
    ("update_entity", "records", "written by capture"),
    (
        "update_entity",
        "fields_backing_note",
        "explains the answer",
    ),
    ("update_entity", "foldedNames", "names a merge folded in"),
    ("update_entity", "held", "computed from the graph"),
    ("update_entity", "record_history", "written by update_fact"),
    ("update_entity", "ticked", "written by capture's tick"),
];

/// A world that uses every write option once, so the reads serve every form.
async fn a_world_that_uses_every_write_option(s: &Session) {
    for (handle, name) in [
        ("thing:sigma", "Sigma"),
        ("thing:omega", "Omega"),
        ("thing:theta", "Theta"),
        ("thing:kappa", "Kappa"),
        ("thing:lambda", "Lambda"),
        ("place:delta", "Delta"),
        ("org:epsilon", "Epsilon"),
        ("event:zeta", "Zeta"),
    ] {
        s.add(handle, name).await;
    }
    let first = s.fact("thing:sigma", "plain claim").await;
    let with = |extra: Value| {
        let mut args = json!({"subject": "thing:sigma", "content": "a claim",
                              "provenance": "testimony"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        args
    };
    s.call(
        "capture",
        with(json!({"sets": {"size": "big"}, "fields": {"note": "own"}})),
    )
    .await;
    s.call(
        "capture",
        with(json!({"provenance": "inference", "derived_from": first})),
    )
    .await;
    s.call("capture", with(json!({"happened_at": "2026-01-02"})))
        .await;
    for (shape, object) in [
        ("location", "place:delta"),
        ("membership", "org:epsilon"),
        ("attendance", "event:zeta"),
        ("about", "thing:omega"),
        ("connection", "thing:omega"),
    ] {
        s.fact_about("thing:sigma", "an edge", shape, object).await;
    }
    s.hedged("thing:sigma", "unsure").await;
    let gone = s.fact("thing:sigma", "to archive").await;
    s.supersede(&gone).await;
    let wrong = s.fact("thing:sigma", "to retract").await;
    s.retract(&wrong, "never true").await;
    s.fact("thing:sigma", "mentions @thing:omega here").await;
    let revised = s.fact("thing:sigma", "first wording").await;
    s.correct(&revised, "second wording").await;
    a_list_on_the_trip(s, "packing", json!({"tick": "sink"})).await;
    add_entry(s, "list:packing", "tent poles", None).await;
    s.fact("thing:kappa", "will merge").await;
    s.merge_entities("thing:kappa", "thing:theta", "same thing", None)
        .await;
}

/// Every claim-shaped and thing-shaped object in a read, with the verb that sets it.
fn records_in(read: &Value, found: &mut Vec<(&'static str, Value)>) {
    match read {
        Value::Array(items) => items.iter().for_each(|item| records_in(item, found)),
        Value::Object(map) => {
            let claim = map.contains_key("provenance")
                && (map.contains_key("content") || map.contains_key("text"));
            if claim {
                found.push(("capture", read.clone()));
            } else if map.contains_key("id") && map.contains_key("source") {
                found.push(("update_entity", read.clone()));
            }
            map.values().for_each(|value| records_in(value, found));
        }
        _ => {}
    }
}

#[tokio::test]
async fn whatever_a_read_serves_the_write_that_sets_it_accepts_as_served() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_world_that_uses_every_write_option(&s).await;
    let mut reads = vec![
        s.call("search", json!({"query": "edge"})).await.json(),
        s.call("list_entities", json!({"kind": "thing"}))
            .await
            .json(),
        s.call(
            "recall",
            json!({"subject": "thing:sigma", "history_record": "thing:sigma#f15"}),
        )
        .await
        .json(),
    ];
    for subject in ["thing:sigma", "thing:theta", "list:packing"] {
        let facts = json!({"subject": subject, "facts": true, "details": true});
        reads.push(s.call("recall", facts).await.json());
    }
    let mut records = Vec::new();
    reads.iter().for_each(|read| records_in(read, &mut records));
    assert!(
        records.len() > 20,
        "the reads served {} records",
        records.len()
    );

    let (mut unaccounted, mut refused) = (Vec::new(), Vec::new());
    for (verb, record) in &records {
        let mut args = serde_json::Map::new();
        for (key, value) in record.as_object().expect("a record is an object") {
            let rename = RENAMED
                .iter()
                .find(|(by, served, ..)| by == verb && served == key);
            let written = rename.map_or(key.as_str(), |row| row.2);
            let published = s.verbs_publishing(written).await.iter().any(|v| v == verb);
            let read_only = READ_ONLY
                .iter()
                .any(|(by, served, _)| by == verb && served == key);
            if !published && !read_only {
                unaccounted.push(format!("{verb}: {key}"));
            } else if key == "edge" {
                args.insert("shape".into(), value["type"].clone());
                args.insert("object".into(), value["object"].clone());
            } else if published {
                args.insert(written.into(), value.clone());
            }
        }
        // A claim lands on another thing; a thing is written back onto itself.
        if *verb == "capture" && args.contains_key("content") {
            args.insert("subject".into(), json!("thing:lambda"));
        } else if *verb == "update_entity" {
            args.insert("handle".into(), record["id"].clone());
        } else {
            continue;
        }
        let body = s.attempt(verb, Value::Object(args.clone())).await;
        if body["status"] == "blocked" {
            refused.push(format!("{verb} sent {args:?} -> {body}"));
        }
    }
    for (verb, _, written, _) in RENAMED {
        assert!(s.verbs_publishing(written).await.iter().any(|v| v == verb));
    }
    unaccounted.sort();
    unaccounted.dedup();
    assert!(unaccounted.is_empty(), "{unaccounted:#?}");
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}
