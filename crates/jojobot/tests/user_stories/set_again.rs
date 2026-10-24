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

/// Another bot's write can land while the image still holds the bot's own
/// record. Setting again must append after that write, preserving both values.
#[tokio::test]
async fn a_set_selected_before_another_bots_edit_appends_instead_of_rewriting_it() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let (gamma, marge) = gamma_and_marge(&story).await;
    set_agent(&gamma, "role:gamma", "a1").await;
    let before = set_records(&gamma, "role:gamma").await;
    let address = before[0]["address"].as_str().expect("an address");

    let gate = store.holds_the_next_document_read();
    let theirs = marge.call(
        "update_fact",
        json!({"address": address, "sets": {"agent": "a2"}}),
    );
    let ours = async {
        gate.reached().await;
        let setting = set_agent(&gamma, "role:gamma", "a3");
        tokio::pin!(setting);
        let early = tokio::time::timeout(std::time::Duration::from_millis(500), &mut setting).await;
        gate.release();
        assert!(early.is_err(), "the edit must wait for the in-flight write");
        setting.await
    };
    tokio::join!(theirs, ours);

    let records = set_records(&gamma, "role:gamma").await;
    assert_eq!(records.len(), 2, "the stale selection appends: {records:?}");
    let original = records
        .iter()
        .find(|r| r["address"] == address)
        .expect("the original");
    assert_eq!(original["sets"]["agent"], "a2", "{records:?}");
    let appended = records
        .iter()
        .find(|r| r["address"] != address)
        .expect("the new claim");
    assert_eq!(appended["sets"]["agent"], "a3", "{records:?}");
    let read = gamma.recall("role:gamma").await.json();
    assert_eq!(read["objects"][0]["fields"]["agent"], "a3", "{read}");
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

#[tokio::test]
async fn a_counter_key_set_twice_adds_to_the_sum() {
    let story = Story::begin("bot:otto").await;
    let (gamma, _) = gamma_and_marge(&story).await;
    gamma
        .call(
            "declare_type",
            json!({"name": "snacking", "fields": [{"key": "donuts", "folds": "sum"}]}),
        )
        .await;

    for _ in 0..2 {
        gamma
            .call(
                "update_entity",
                json!({"handle": "role:gamma", "sets": {"donuts": "1"}}),
            )
            .await;
    }

    let read = gamma.recall("role:gamma").await.json();
    assert_eq!(
        read["objects"][0]["fields"]["donuts"], "2",
        "the thing holds the sum: {read}"
    );
    let records = set_records(&gamma, "role:gamma").await;
    assert_eq!(records.len(), 2, "each set is its own record: {records:?}");
    story.finish().await;
}

#[tokio::test]
async fn a_record_another_bot_edited_since_is_not_rewritten() {
    let story = Story::begin("bot:otto").await;
    let (gamma, marge) = gamma_and_marge(&story).await;

    set_agent(&gamma, "role:gamma", "a1").await;
    let records = set_records(&gamma, "role:gamma").await;
    let address = records[0]["address"].as_str().expect("an address");
    marge
        .call(
            "update_fact",
            json!({"address": address, "sets": {"agent": "a2"}}),
        )
        .await;

    set_agent(&gamma, "role:gamma", "a3").await;
    let after = set_records(&gamma, "role:gamma").await;
    assert_eq!(after.len(), 2, "marge's edit stays readable: {after:?}");
    story.finish().await;
}

/// The stored due moment of a thing, when it holds one.
async fn due_on(s: &Session, handle: &str) -> Option<String> {
    let read = s
        .call("recall", json!({"subject": handle, "keys": ["due_on"]}))
        .await
        .json();
    read["objects"][0]["fields"]["due_on"]
        .as_str()
        .map(str::to_string)
}

/// **A rewrite that finishes a thing, cut before the stored due moment came off,
/// is finished by the same call again.** Gamma's own "Set status" record is
/// rewritten to `done`, which ends what the work owes, and the step that takes the
/// due moment off fails once. The call says the status landed and asks for a
/// resend; the resend, with the same `request_id`, takes the due moment off and
/// writes the status no second time.
#[tokio::test]
async fn a_cut_rewrite_that_ends_what_is_owed_is_finished_by_the_same_call_again() {
    let story = Story::begin("bot:otto").await;
    let (gamma, _) = gamma_and_marge(&story).await;
    gamma.add("work:handcart", "Handcart").await;
    gamma
        .call(
            "capture",
            json!({"subject": "work:handcart", "content": "the handcart is due",
                   "provenance": "testimony", "sets": {"runs_out": "2099-12-01"}}),
        )
        .await;
    assert_eq!(
        due_on(&gamma, "work:handcart").await.as_deref(),
        Some("2099-12-01")
    );
    gamma
        .call(
            "update_entity",
            json!({"handle": "work:handcart", "sets": {"status": "next"}}),
        )
        .await;
    let before = set_records(&gamma, "work:handcart").await;
    let own = before
        .iter()
        .find(|r| r["sets"].get("status").is_some())
        .unwrap_or_else(|| panic!("gamma's status record: {before:?}"));
    let own_address = own["address"].as_str().expect("an address").to_string();

    let ending = json!({"handle": "work:handcart", "sets": {"status": "done"},
                        "request_id": "s1"});
    story.fails_the_next_commit_of("clear_due");
    let stopped = gamma.refused("update_entity", ending.clone()).await.json();
    assert_eq!(stopped["fix_by"], "retry", "{stopped}");
    assert_eq!(
        due_on(&gamma, "work:handcart").await.as_deref(),
        Some("2099-12-01"),
        "the step that failed has not run"
    );

    let finished = gamma.call("update_entity", ending).await.json();
    assert_ne!(finished["status"], "blocked", "{finished}");
    assert_eq!(
        due_on(&gamma, "work:handcart").await,
        None,
        "the resend took the stored due moment off"
    );
    let after = set_records(&gamma, "work:handcart").await;
    assert_eq!(
        after.len(),
        1,
        "the status record was rewritten once: {after:?}"
    );
    assert_eq!(after[0]["address"], json!(own_address), "{after:?}");
    assert_eq!(after[0]["sets"]["status"], "done", "{after:?}");
    story.finish().await;
}
