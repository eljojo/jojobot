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
        "the release 10 relation model (rule 718)",
    ),
    (
        "capture",
        "text",
        "content",
        "none named: a list entry serves its words as text",
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

/// Served keys no write sets: (verb, served keys, why).
const READ_ONLY: &[(&str, &[&str], &str)] = &[
    (
        "capture",
        &[
            "address",
            "inserted_at",
            "written_at",
            "nth",
            "run",
            "revised",
            "revision_count",
        ],
        "the store's own identity and stamps",
    ),
    (
        "capture",
        &["status", "stands_for", "hit", "parent", "state"],
        "set by another verb, or a search envelope",
    ),
    (
        "update_entity",
        &["type", "boot", "parent"],
        "fixed when add_entity creates the thing",
    ),
    (
        "update_entity",
        &[
            "as_of",
            "connected",
            "held",
            "fields_backing_note",
            "foldedNames",
            "record_history",
            "entries",
            "facts",
            "records",
            "ticked",
        ],
        "an answer worked out from the claims, which capture and update_fact write",
    ),
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
        RENAMED.iter().all(|row| !row.3.is_empty())
            && READ_ONLY.iter().all(|row| !row.2.is_empty())
    );

    // ── every served key is taken by its write, renamed, or read-only ────────
    let (mut unaccounted, mut landed) = (Vec::new(), Vec::new());
    for (verb, record) in &records {
        let mut args = serde_json::Map::new();
        for (key, value) in record.as_object().expect("a record is an object") {
            let rename = RENAMED.iter().find(|row| row.0 == *verb && row.1 == key);
            let written = rename.map_or(key.as_str(), |row| row.2);
            let read_only = READ_ONLY
                .iter()
                .any(|row| row.0 == *verb && row.1.contains(&key.as_str()));
            if key == "edge" {
                args.insert("shape".into(), value["type"].clone());
                args.insert("object".into(), value["object"].clone());
            } else if s.takes(verb, written).await {
                args.insert(written.into(), value.clone());
            } else if !read_only {
                unaccounted.push(format!("{verb}: {key}"));
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
        let receipt = s.attempt(verb, Value::Object(args.clone())).await;
        landed.push((*verb, args, receipt));
    }
    unaccounted.sort();
    unaccounted.dedup();
    assert!(unaccounted.is_empty(), "{unaccounted:#?}");

    // ── and what lands reads back as sent, unless the receipt named it dropped ─
    let mut changed = Vec::new();
    for (verb, args, receipt) in &landed {
        let held = if receipt["status"] == "blocked" {
            changed.push(format!("{verb} refused {args:?}: {receipt}"));
            continue;
        } else if *verb == "capture" {
            // The lambda claims are numbered in landing order: read the one page.
            let n = receipt["address"]
                .as_str()
                .and_then(|a| a.rsplit('f').next());
            let at = n.and_then(|n| n.parse::<u32>().ok()).unwrap_or(1) - 1;
            let page = json!({"subject": "thing:lambda", "facts": true, "details": true,
                              "offset": at, "limit": 1});
            s.call("recall", page).await.json()["objects"][0]["facts"][0].clone()
        } else {
            let thing = s
                .call("recall", json!({"subject": args["handle"]}))
                .await
                .json();
            thing["objects"][0].clone()
        };
        let dropped = receipt["dropped_keys"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (key, sent) in args {
            let renamed = RENAMED.iter().find(|row| row.0 == *verb && row.2 == key);
            let served = renamed.map_or(key.as_str(), |row| row.1);
            let served = if held.get(key).is_some() { key } else { served };
            let kept = match key.as_str() {
                "shape" => &held["edge"]["type"],
                "object" => &held["edge"]["object"],
                _ => &held[served],
            };
            let lost: Vec<String> = match sent
                .as_object()
                .filter(|_| key == "fields" || key == "sets")
            {
                Some(bag) => bag
                    .iter()
                    .filter(|(k, v)| kept.get(k) != Some(v) && !dropped.contains(&json!(k)))
                    .map(|(k, _)| k.clone())
                    .collect(),
                None if kept == sent || sent.as_array().is_some_and(Vec::is_empty) => vec![],
                None => vec![key.clone()],
            };
            if !lost.is_empty() {
                changed.push(format!(
                    "{verb} sent {key} {sent} and read back {kept} in {held}"
                ));
            }
        }
    }
    assert!(changed.is_empty(), "{}", changed.join("\n"));
}
