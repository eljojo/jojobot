//! "I review a topic a record at a time, and I need five things of each record.
//! Most of what came back was keys I never read, and reading every topic took
//! nine calls."
//!
//! **A read names the keys of each record it wants** (`record_keys`), as `keys`
//! names the keys of each object: every record comes back with its address and
//! the keys named, and the answer says once what it left out and how to read it.
//! **A record that holds no references and stands for nothing carries neither
//! key**, so no record has an empty list on it. **A page holds at most the number
//! a call names** (`limit`), counting what `offset` counts, and the ceiling can
//! still make a page shorter.

use serde_json::json;

use super::dsl::{Session, Story};

const ALPHA: &str = "topic:alpha";
const CHILDREN: [(&str, &str); 3] = [
    ("topic:beta", "Beta"),
    ("topic:gamma", "Gamma"),
    ("topic:epsilon", "Epsilon"),
];

/// A topic with three child topics, each holding two records: one with details
/// and a reference, one with neither.
async fn a_topic_under_review(s: &Session) {
    s.add(ALPHA, "Alpha").await;
    s.add("person:milhouse", "Milhouse").await;
    for (handle, name) in CHILDREN {
        s.add_under(ALPHA, handle, name).await;
        s.call(
            "capture",
            json!({
                "subject": handle, "content": format!("{name} opened"),
                "details": "the reasons under the claim", "provenance": "testimony",
                "refs": ["person:milhouse"],
            }),
        )
        .await;
        s.call(
            "capture",
            json!({"subject": handle, "content": format!("{name} closed"),
                   "provenance": "testimony"}),
        )
        .await;
    }
}

const LEAN: [&str; 4] = ["content", "provenance", "standing", "recorded_at"];

#[tokio::test]
async fn a_review_names_the_keys_of_each_record_and_is_told_once_what_it_left_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_topic_under_review(&s).await;

    let lean = s
        .call("recall", json!({"parent": ALPHA, "record_keys": LEAN}))
        .await;
    let objects = lean.json()["objects"]
        .as_array()
        .expect("the children")
        .clone();
    assert_eq!(objects.len(), 3, "{}", lean.raw());
    for object in &objects {
        // Naming record keys asks for the records: no `facts: true` was sent.
        let facts = object["facts"].as_array().expect("the records").clone();
        assert_eq!(facts.len(), 2, "{object}");
        for fact in &facts {
            let mut keys: Vec<&str> = fact
                .as_object()
                .expect("a record")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                [
                    "address",
                    "content",
                    "provenance",
                    "recorded_at",
                    "standing"
                ],
                "{fact}"
            );
        }
    }
    // ── said once, for the answer: the keys left out and how to read them ───
    let narrowed = &lean.json()["records_narrowed"];
    let left_out: Vec<&str> = narrowed["left_out_keys"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer names what it left out: {}", lean.raw()))
        .iter()
        .filter_map(|key| key.as_str())
        .collect();
    for key in ["subject", "status", "fields", "refs", "details"] {
        assert!(left_out.contains(&key), "{key} is named: {narrowed}");
    }
    for key in LEAN {
        assert!(!left_out.contains(&key), "{key} came back: {narrowed}");
    }
    assert!(
        narrowed["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("record_keys")),
        "{narrowed}"
    );
    // The details are part of what the narrowing left out, not a second line.
    lean.never_says("details_left_out");

    // ── naming `details` brings them back, with no `details: true` ───────────
    let with_details = s
        .call(
            "recall",
            json!({"subject": "topic:beta", "record_keys": ["content", "details"]}),
        )
        .await;
    with_details.says("the reasons under the claim");
    let facts = with_details.json()["objects"][0]["facts"]
        .as_array()
        .expect("the records")
        .clone();
    assert!(
        facts.iter().all(|fact| fact.get("provenance").is_none()),
        "{}",
        with_details.raw()
    );

    // ── a read without it is the read it always was ─────────────────────────
    let full = s
        .call("recall", json!({"subject": "topic:beta", "facts": true}))
        .await;
    full.never_says("records_narrowed");
    let fact = &full.json()["objects"][0]["facts"][0];
    for key in ["address", "subject", "content", "provenance", "fields"] {
        assert!(fact.get(key).is_some(), "{key} is there: {fact}");
    }
    story.finish().await;
}

#[tokio::test]
async fn a_record_with_no_references_carries_no_empty_list() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_topic_under_review(&s).await;

    let read = s
        .call("recall", json!({"subject": "topic:beta", "facts": true}))
        .await;
    let facts = read.json()["objects"][0]["facts"]
        .as_array()
        .expect("the records")
        .clone();
    let opened = facts
        .iter()
        .find(|fact| fact["content"] == "Beta opened")
        .expect("the record with a reference");
    let closed = facts
        .iter()
        .find(|fact| fact["content"] == "Beta closed")
        .expect("the record with none");
    // The positive the absence depends on: a reference is still served.
    assert_eq!(opened["refs"], json!(["person:milhouse"]), "{opened}");
    assert!(closed.get("refs").is_none(), "{closed}");
    assert!(closed.get("stands_for").is_none(), "{closed}");
    for fact in &facts {
        for (key, value) in fact.as_object().expect("a record") {
            assert!(
                value.as_array().is_none_or(|list| !list.is_empty()),
                "{key} is an empty list: {fact}"
            );
        }
    }
    // A write's receipt is the same record, and holds no empty list either.
    let receipt = s
        .call(
            "capture",
            json!({"subject": "topic:gamma", "content": "Gamma reopened",
                   "provenance": "testimony"}),
        )
        .await;
    let record = receipt.json();
    assert!(
        record["address"]
            .as_str()
            .is_some_and(|address| address.starts_with("topic:gamma#")),
        "{record}"
    );
    assert!(record.get("refs").is_none(), "{record}");
    assert!(record.get("stands_for").is_none(), "{record}");
    story.finish().await;
}

#[tokio::test]
async fn a_page_holds_at_most_the_number_a_call_names() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_topic_under_review(&s).await;

    // ── several objects: `limit` counts objects, and `offset` reads on ──────
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    loop {
        let page = s
            .call(
                "recall",
                json!({"parent": ALPHA, "limit": 2, "offset": offset}),
            )
            .await;
        let objects = page.json()["objects"].as_array().expect("a page").clone();
        assert!(!objects.is_empty() && objects.len() <= 2, "{}", page.raw());
        seen.extend(
            objects
                .iter()
                .map(|o| o["id"].as_str().expect("a handle").to_string()),
        );
        match page.json()["not_shown"]["offset"].as_u64() {
            Some(next) => {
                assert_eq!(next, offset + objects.len() as u64, "{}", page.raw());
                offset = next;
            }
            None => break,
        }
    }
    seen.sort();
    assert_eq!(seen, ["topic:beta", "topic:epsilon", "topic:gamma"]);

    // ── one subject with its records: `limit` counts records ────────────────
    let first = s
        .call(
            "recall",
            json!({"subject": "topic:beta", "facts": true, "limit": 1}),
        )
        .await;
    assert_eq!(
        first.json()["objects"][0]["facts"]
            .as_array()
            .expect("the records")
            .len(),
        1,
        "{}",
        first.raw()
    );
    assert_eq!(first.json()["not_shown"]["count"], 1, "{}", first.raw());
    assert_eq!(first.json()["not_shown"]["offset"], 1, "{}", first.raw());
    let rest = s
        .call(
            "recall",
            json!({"subject": "topic:beta", "facts": true, "limit": 1, "offset": 1}),
        )
        .await;
    assert_eq!(
        rest.json()["objects"][0]["facts"]
            .as_array()
            .expect("the records")
            .len(),
        1,
        "{}",
        rest.raw()
    );
    rest.never_says("not_shown");

    // ── a page of nothing is not a page ─────────────────────────────────────
    s.refused("recall", json!({"parent": ALPHA, "limit": 0}))
        .await
        .says("limit");
    story.finish().await;
}
