//! "Gamma's assistant says which agent holds the role every time it boots.
//! Why does the role end up with a record for each boot?"
//!
//! A bot that sets again the keys its own earlier "Set ... on" record holds,
//! and that nothing else holds, rewrites that record in place. Any other write
//! appends, as it always did.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The active records of `handle` that say which keys they set.
async fn set_records(s: &Session, handle: &str) -> Vec<Value> {
    let read = s.recall(handle).await.json();
    read["objects"][0]["facts"]
        .as_array()
        .expect("a recall with facts lists them")
        .iter()
        .filter(|fact| {
            fact["status"] == "active"
                && fact["content"]
                    .as_str()
                    .is_some_and(|words| words.starts_with("Set "))
        })
        .cloned()
        .collect()
}

/// A bot booted with its role claimed, and the colleague that writes beside it.
async fn gamma_and_marge(story: &Story) -> (Session, Session) {
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:marge", "Marge").await;
    let (_, gamma) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "gamma"}),
        )
        .await;
    let marge = story.as_bot("bot:marge").await;
    (gamma.expect("gamma boots"), marge)
}

async fn set_agent(s: &Session, handle: &str, agent: &str) -> Value {
    s.call(
        "update_entity",
        json!({"handle": handle, "sets": {"agent": agent}}),
    )
    .await
    .json()
}

#[tokio::test]
async fn a_bot_setting_the_same_key_again_rewrites_its_own_record() {
    let story = Story::begin("bot:otto").await;
    let (gamma, _) = gamma_and_marge(&story).await;

    set_agent(&gamma, "role:gamma", "a1").await;
    let second = set_agent(&gamma, "role:gamma", "a2").await;
    assert_ne!(second["status"], "blocked", "{second}");

    let records = set_records(&gamma, "role:gamma").await;
    assert_eq!(records.len(), 1, "one record holds the agent: {records:?}");
    assert_eq!(records[0]["sets"]["agent"], "a2", "{records:?}");
    let read = gamma.recall("role:gamma").await.json();
    assert_eq!(read["objects"][0]["fields"]["agent"], "a2", "{read}");

    let address = records[0]["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let history = gamma
        .call("recall", json!({"history_record": address}))
        .await
        .json();
    let writes = history["objects"][0]["record_history"]["writes"]
        .as_array()
        .expect("a record's history lists its writes");
    assert_eq!(writes.len(), 2, "both writes stay readable: {history}");
    let trail = gamma
        .call(
            "recall",
            json!({"subject": "role:gamma", "history": "agent"}),
        )
        .await
        .json();
    let seen = trail.to_string();
    assert!(seen.contains("a1") && seen.contains("a2"), "{trail}");
    story.finish().await;
}

#[tokio::test]
async fn a_record_that_holds_other_keys_is_not_rewritten() {
    let story = Story::begin("bot:otto").await;
    let (gamma, _) = gamma_and_marge(&story).await;

    gamma
        .call(
            "update_entity",
            json!({"handle": "role:gamma", "sets": {"agent": "a1", "seat": "front"}}),
        )
        .await;
    set_agent(&gamma, "role:gamma", "a2").await;

    let records = set_records(&gamma, "role:gamma").await;
    assert_eq!(records.len(), 2, "the second write appended: {records:?}");
    story.finish().await;
}

#[tokio::test]
async fn another_bots_record_is_not_rewritten() {
    let story = Story::begin("bot:otto").await;
    let (gamma, marge) = gamma_and_marge(&story).await;

    set_agent(&gamma, "role:gamma", "a1").await;
    let theirs = set_agent(&marge, "role:gamma", "a2").await;
    assert_ne!(
        theirs["status"], "blocked",
        "marge may set the agent: {theirs}"
    );

    let records = set_records(&gamma, "role:gamma").await;
    assert_eq!(records.len(), 2, "marge's write appended: {records:?}");
    let first = records
        .iter()
        .find(|r| r["sets"]["agent"] == "a1")
        .unwrap_or_else(|| panic!("gamma's record kept its value: {records:?}"));
    assert!(
        first.get("revision_count").is_none(),
        "never rewritten: {first}"
    );
    story.finish().await;
}
