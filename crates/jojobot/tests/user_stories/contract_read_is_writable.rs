//! **Contract: whatever a read serves, the write that sets it accepts as served.**
//!
//! One world uses every write option once. The cases read it back through every
//! read that serves a claim or a thing, and hold three things. A claim copied
//! whole into a capture lands. A thing copied into an update lands. Every key a
//! read serves is a published argument of the write that sets it, or a named
//! rename, or named read-only: a key that is none of these is a new mismatch.
use super::dsl::{Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};
use serde_json::{Value, json};

/// A world that uses every write option once, so the reads serve every form.
async fn a_world_that_uses_every_write_option(s: &Session) {
    s.add("thing:sigma", "Sigma").await;
    s.add("thing:omega", "Omega").await;
    s.add("thing:theta", "Theta").await;
    let first = s.fact("thing:sigma", "plain claim").await;
    s.call(
        "capture",
        json!({"subject": "thing:sigma", "content": "with settings",
               "provenance": "testimony", "sets": {"size": "big"},
               "fields": {"note": "own"}}),
    )
    .await;
    s.add("place:delta", "Delta").await;
    s.add("org:epsilon", "Epsilon").await;
    s.add("event:zeta", "Zeta").await;
    for (shape, object) in [
        ("location", "place:delta"),
        ("membership", "org:epsilon"),
        ("attendance", "event:zeta"),
        ("about", "thing:omega"),
        ("connection", "thing:omega"),
    ] {
        s.fact_about("thing:sigma", &format!("edge {shape}"), shape, object)
            .await;
    }
    s.call(
        "capture",
        json!({"subject": "thing:sigma", "content": "derived", "provenance": "inference",
               "derived_from": first}),
    )
    .await;
    s.fact_that_happened_on("thing:sigma", "happened", "2026-01-02")
        .await;
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
    add_entry(s, "list:packing", "stove", None).await;
    s.add("thing:kappa", "Kappa").await;
    s.fact("thing:kappa", "will merge").await;
    s.merge_entities("thing:kappa", "thing:theta", "same thing", None)
        .await;
}

/// Every claim and thing the reads serve, from every read that serves one.
async fn what_the_reads_serve(s: &Session) -> (Vec<Value>, Vec<Value>) {
    let (mut claims, mut things) = (Vec::new(), Vec::new());
    for subject in [
        "thing:sigma",
        "thing:theta",
        "thing:omega",
        "place:delta",
        "list:packing",
    ] {
        let read = s
            .call(
                "recall",
                json!({"subject": subject, "facts": true, "details": true}),
            )
            .await
            .json();
        for object in read["objects"].as_array().expect("objects") {
            claims.extend(object["facts"].as_array().cloned().unwrap_or_default());
            things.push(object.clone());
        }
    }
    (claims, things)
}

/// **A served claim, copied whole into a capture on another thing.**
fn the_claim_as_served(claim: &Value, onto: &str) -> Value {
    let mut args = json!({"subject": onto, "content": claim["content"],
                          "provenance": claim["provenance"], "standing": claim["standing"]});
    for key in [
        "details",
        "happened_at",
        "happened_through",
        "derived_from",
        "fields",
        "sets",
    ] {
        if !claim[key].is_null() {
            args[key] = claim[key].clone();
        }
    }
    if let Some(edge) = claim.get("edge") {
        args["shape"] = edge["type"].clone();
        args["object"] = edge["object"].clone();
    }
    args
}

#[tokio::test]
async fn a_claim_a_read_serves_is_accepted_by_the_write_that_sets_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_world_that_uses_every_write_option(&s).await;
    s.add("thing:lambda", "Lambda").await;
    let (claims, _) = what_the_reads_serve(&s).await;
    let mut refused = Vec::new();
    for claim in &claims {
        let args = the_claim_as_served(claim, "thing:lambda");
        let body = s.attempt("capture", args.clone()).await;
        if body["status"] == "blocked" {
            refused.push(format!("{} sent {args} -> {body}", claim["address"]));
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}

#[tokio::test]
async fn a_thing_a_read_serves_is_accepted_by_the_write_that_sets_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_world_that_uses_every_write_option(&s).await;
    let (_, things) = what_the_reads_serve(&s).await;
    let mut refused = Vec::new();
    for thing in &things {
        let mut args = json!({"handle": thing["id"], "name": thing["name"],
                              "source": thing["source"], "aliases": thing["alternateName"]});
        if !thing["fields"].is_null() {
            args["sets"] = thing["fields"].clone();
        }
        let body = s.attempt("update_entity", args.clone()).await;
        if body["status"] == "blocked" {
            refused.push(format!("{} sent {args} -> {body}", thing["id"]));
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}

/// Served keys no write sets: (writing verb, served key, why).
const READ_ONLY: &[(&str, &str, &str)] = &[
    (
        "capture",
        "address",
        "names the claim; update_fact takes it as address",
    ),
    ("capture", "inserted_at", "the store's own stamp"),
    (
        "capture",
        "written_at",
        "the store's own stamp on a version",
    ),
    ("capture", "nth", "a version's place in the claim's history"),
    ("capture", "run", "the session that wrote a version"),
    ("capture", "revised", "counts the claim's rewrites"),
    ("capture", "revision_count", "counts the claim's rewrites"),
    (
        "capture",
        "status",
        "set by update_fact's archive, never by a capture",
    ),
    (
        "capture",
        "stands_for",
        "what a list entry stands for, read from the entry's own links",
    ),
    ("capture", "hit", "a search hit's own envelope"),
    ("capture", "parent", "a search hit's thing, an envelope"),
    (
        "update_entity",
        "type",
        "the schema name of the kind, which add_entity takes as kind",
    ),
    ("update_entity", "boot", "set when the thing is created"),
    ("update_entity", "parent", "set when the thing is created"),
    (
        "update_entity",
        "as_of",
        "the day the answer was worked out for",
    ),
    ("update_entity", "connected", "computed from the graph"),
    (
        "update_entity",
        "entries",
        "a list's entries, written by capture",
    ),
    (
        "update_entity",
        "facts",
        "the thing's claims, written by capture",
    ),
    (
        "update_entity",
        "fields_backing_note",
        "explains the answer",
    ),
    ("update_entity", "foldedNames", "names a merge folded in"),
    ("update_entity", "held", "computed from the graph"),
    (
        "update_entity",
        "record_history",
        "the claim histories, written by update_fact",
    ),
    (
        "update_entity",
        "records",
        "the thing's claims, written by capture",
    ),
    (
        "update_entity",
        "ticked",
        "a list's tick count, written by capture's tick",
    ),
];
/// Served keys the write names differently: (writing verb, served, written).
/// Each is replayed under its written name by the two cases above.
const RENAMED: &[(&str, &str, &str)] = &[
    ("capture", "edge", "shape"),
    ("update_entity", "id", "handle"),
    ("update_entity", "alternateName", "aliases"),
    ("update_entity", "fields", "sets"),
];

/// Every claim-shaped and thing-shaped object in every read, with the verb that
/// sets what it holds.
fn records_in(read: &Value, found: &mut Vec<(&'static str, Value)>) {
    match read {
        Value::Array(items) => items.iter().for_each(|item| records_in(item, found)),
        Value::Object(map) => {
            let claim = map.contains_key("provenance")
                && (map.contains_key("content") || map.contains_key("text"));
            let thing = map.contains_key("id") && map.contains_key("source");
            if claim {
                found.push(("capture", read.clone()));
            } else if thing {
                found.push(("update_entity", read.clone()));
            }
            map.values().for_each(|value| records_in(value, found));
        }
        _ => {}
    }
}

#[tokio::test]
async fn every_key_a_read_serves_is_a_write_argument_or_accounted_for() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_world_that_uses_every_write_option(&s).await;
    let mut reads = Vec::new();
    for subject in ["thing:sigma", "thing:theta", "thing:omega", "list:packing"] {
        reads.push(
            s.call(
                "recall",
                json!({"subject": subject, "facts": true, "details": true}),
            )
            .await
            .json(),
        );
    }
    reads.push(
        s.call(
            "recall",
            json!({"subject": "thing:sigma", "history_record": "thing:sigma#f15"}),
        )
        .await
        .json(),
    );
    reads.push(s.call("search", json!({"query": "edge"})).await.json());
    reads.push(
        s.call("list_entities", json!({"kind": "thing"}))
            .await
            .json(),
    );
    let mut records = Vec::new();
    reads.iter().for_each(|read| records_in(read, &mut records));
    let mut unaccounted = std::collections::BTreeSet::new();
    for (verb, record) in &records {
        for key in record.as_object().expect("a record is an object").keys() {
            let known = READ_ONLY
                .iter()
                .any(|(by, read_key, _)| by == verb && read_key == key)
                || RENAMED
                    .iter()
                    .any(|(by, read_key, _)| by == verb && read_key == key)
                || s.verbs_publishing(key).await.iter().any(|v| v == verb);
            if !known {
                unaccounted.insert(format!("{verb}: {key}"));
            }
        }
    }
    for (verb, _, written) in RENAMED {
        assert!(
            s.verbs_publishing(written).await.iter().any(|v| v == verb),
            "{verb} no longer takes {written}"
        );
    }
    assert!(
        records.len() > 20,
        "the reads served {} records",
        records.len()
    );
    assert!(unaccounted.is_empty(), "{unaccounted:#?}");
}
