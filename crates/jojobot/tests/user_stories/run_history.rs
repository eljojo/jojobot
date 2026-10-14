//! "What did this run change, and what did it say before it said that?"
//!
//! **A run used to keep only where it stands now.** A focus overwrote the one
//! before, an amended entry lost its first wording, and the key a run works
//! through changed without a trace of when. Each change is kept now, in order,
//! and read back through `recall` of the run with `history`, as a thing's key is.
//!
//! ⛔️ **The key is never read back.** A run's key opens it, so the history says
//! that the key changed and when and carries nothing of it.
//!
//! The ordinary read of the run answers as it always did, with or without the
//! history beside it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_run_can_see_what_its_focus_was_and_what_an_entry_said_before_it_was_corrected() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    run.call(
        "journal",
        json!({"entry": "read the card", "focus": "reading the card"}),
    )
    .await;
    run.call(
        "journal",
        json!({"entry": "found the cause", "focus": "writing the fix"}),
    )
    .await;
    run.call(
        "amend_journal",
        json!({"entry": "found the cause, said better"}),
    )
    .await;
    run.call(
        "amend_journal",
        json!({"entry": "found the cause, said best"}),
    )
    .await;
    let handle = format!("session:{}", run.run());

    // ── The focus, oldest first ───────────────────────────────────────────────
    let focus = run
        .call("recall", json!({"subject": &handle, "history": "focus"}))
        .await;
    let history = focus.json()["objects"][0]["history"].clone();
    assert_eq!(history["key"], "focus", "{}", focus.raw());
    let values: Vec<&str> = history["writes"]
        .as_array()
        .expect("the changes come back as a list")
        .iter()
        .map(|write| write["value"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        values.last().copied(),
        Some("writing the fix"),
        "the newest focus is last: {}",
        focus.raw()
    );
    assert!(
        values.contains(&"reading the card"),
        "the focus before it is kept: {}",
        focus.raw()
    );
    assert!(
        history["writes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|write| write["written_at"].is_string()),
        "every change says when: {}",
        focus.raw()
    );

    // ── The wordings of the entry that was corrected twice ────────────────────
    let journal = run
        .call("recall", json!({"subject": &handle, "history": "journal"}))
        .await;
    let entries = journal.json()["objects"][0]["history"]["entries"].clone();
    let texts: Vec<&str> = entries[0]["wordings"]
        .as_array()
        .expect("the wordings come back as a list")
        .iter()
        .map(|wording| wording["text"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        vec![
            "found the cause",
            "found the cause, said better",
            "found the cause, said best"
        ],
        "{}",
        journal.raw()
    );

    // ── The ordinary read is what it was ──────────────────────────────────────
    let plain = run.call("recall", json!({"subject": &handle})).await;
    assert!(
        plain.json()["objects"][0].get("history").is_none(),
        "a history nobody asked for is not in the answer: {}",
        plain.raw()
    );
    assert_eq!(
        plain.json()["objects"][0]["name"],
        "writing the fix",
        "{}",
        plain.raw()
    );
    run.wrap("traced the focus and the wordings").await;
    story.finish().await;
}

#[tokio::test]
async fn the_history_of_a_runs_key_says_it_changed_and_never_what_it_changed_to() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    run.journal("a beat before the run is wrapped").await;
    let handle = format!("session:{}", run.run());
    let key = run.sid().to_string();
    run.wrap("done, and the key goes with it").await;

    // A second run of the same bot reads the wrapped run's history, because the
    // wrapped run's own key is gone.
    let reader = story.session().await;
    let keyed = reader
        .call("recall", json!({"subject": &handle, "history": "key"}))
        .await;
    let writes = keyed.json()["objects"][0]["history"]["writes"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the key's changes come back: {}", keyed.raw()));
    assert!(
        !writes.is_empty(),
        "wrapping took the key away and the history says so: {}",
        keyed.raw()
    );
    for write in &writes {
        assert_eq!(write["changed"], true, "{}", keyed.raw());
        assert!(
            write.get("value").is_none(),
            "a value is in the key's history: {}",
            keyed.raw()
        );
        assert!(write["written_at"].is_string(), "{}", keyed.raw());
    }
    assert!(
        !keyed.raw().contains(&key),
        "the key is in its own history: {}",
        keyed.raw()
    );
    reader.wrap("read it back").await;
    story.finish().await;
}

/// "Can a walk hand me a sitting with the history of its journal beside it?"
///
/// **Not today: nothing links to a run, and a run links to nothing.** A write
/// that names a run in its words is refused, and a walk over runs comes back
/// with nothing connected in either direction, so a run is only ever the
/// object a call selected.
///
/// 🚨 **This pins that.** `fit_run_histories` cuts the history of the objects a
/// call selected and not of the objects it reached, and it can leave the nested
/// case alone only while no walk reaches a run. The day a run becomes linkable
/// this case goes red, and the cut must be extended to the objects reached by
/// the walk (rule 498) in the same change.
#[tokio::test]
async fn a_run_cannot_be_linked_to_so_no_walk_reaches_one_with_its_history() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    run.add("person:milhouse", "Milhouse").await;
    run.call(
        "journal",
        json!({"entry": "met @person:milhouse", "focus": "meeting"}),
    )
    .await;
    run.call(
        "amend_journal",
        json!({"entry": "met @person:milhouse, said better"}),
    )
    .await;
    let other = story.session_in("America/Toronto", Some("new")).await;
    other
        .call(
            "journal",
            json!({"entry": "also met @person:milhouse", "focus": "meeting again"}),
        )
        .await;
    let handle = format!("session:{}", run.run());

    // ── A claim that names a run is refused ───────────────────────────────────
    let refusal = run
        .refused(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": format!("sat in on @{handle}"),
                "provenance": "testimony",
            }),
        )
        .await;
    assert_eq!(refusal.json()["wrote"], false, "{}", refusal.raw());
    // Paired: the same claim naming a thing that exists lands.
    run.add("event:birthday-party", "Birthday Party").await;
    run.fact("person:milhouse", "sat in on @event:birthday-party")
        .await;

    // ── A walk over runs reaches nothing from them, either way ────────────────
    for direction in ["out", "in"] {
        let walked = run
            .call(
                "recall",
                json!({
                    "kind": "session",
                    "follow": {"direction": direction, "depth": 3},
                    "history": "journal",
                }),
            )
            .await
            .json();
        let objects = walked["objects"].as_array().expect("the runs come back");
        assert_eq!(objects.len(), 2, "both runs are selected: {walked}");
        for object in objects {
            assert_eq!(
                object["connected"],
                json!([]),
                "a run reached something going {direction}: {walked}"
            );
        }
        let cut = objects
            .iter()
            .find(|object| object["id"] == handle.as_str())
            .unwrap_or_else(|| panic!("the run is selected: {walked}"));
        assert!(
            cut["history"].to_string().contains("said better"),
            "the selected run carries its history: {walked}"
        );
    }
    run.wrap("pinned that no walk reaches a run").await;
    other.wrap("read it back").await;
    story.finish().await;
}
