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

/// "Was this run ever handed its charter, and when?"
///
/// **A run no call handed its charter to is held to have been told it before
/// every rewrite there can be**, and that moment is the start of the clock. It
/// is a marker and not a day anybody lived, so the history of the run says
/// *never told* for it and never a date in 1970. A run that read its charter
/// shows the moment it did.
#[tokio::test]
async fn a_run_never_handed_its_charter_says_never_told_and_one_that_read_it_says_when() {
    let story = Story::begin("bot:otto").await;
    // A boot that has no run to offer back ships the charter. One that answers
    // the offer with `new` ships none, so this run is handed it by no call.
    let earlier = story.session().await;
    earlier
        .journal("a run left open, so the next boot is offered it")
        .await;
    let run = story.session_in("America/New_York", Some("new")).await;
    run.journal("a first write, before any charter was read")
        .await;
    let handle = format!("session:{}", run.run());

    // ── before it reads its charter ───────────────────────────────────────────
    let before = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "charter_told"}),
        )
        .await;
    let writes = before.json()["objects"][0]["history"]["writes"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the charter's changes come back: {}", before.raw()));
    assert_eq!(
        writes.len(),
        1,
        "one change, the run's first write: {}",
        before.raw()
    );
    assert_eq!(writes[0]["value"], "never told", "{}", before.raw());
    before.never_says("1970");

    // ── after it reads its charter ────────────────────────────────────────────
    run.call("recall", json!({"subject": "bot:otto", "charter": true}))
        .await;
    let after = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "charter_told"}),
        )
        .await;
    let writes = after.json()["objects"][0]["history"]["writes"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the charter's changes come back: {}", after.raw()));
    let newest = writes
        .last()
        .and_then(|write| write["value"].as_str())
        .unwrap_or_else(|| panic!("the newest change has a value: {}", after.raw()));
    assert!(
        newest
            .parse::<jiff::Timestamp>()
            .is_ok_and(|at| at.as_second() > 0),
        "the run read its charter, so its history says the moment: {}",
        after.raw()
    );
    after.never_says("1970");
    run.wrap("read the charter and asked when").await;
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
    let other = story.session_in("America/New_York", Some("new")).await;
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

/// "Give me the history, none of it at a time." A page of zero changes reads
/// nothing, so it is refused and says what to send; a page of one reads the
/// newest change, and the pointer it leaves moves forward to the next.
#[tokio::test]
async fn a_page_of_no_changes_is_refused_and_a_page_of_one_moves_forward_when_followed() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    for focus in ["reading the card", "writing the fix", "checking the fix"] {
        run.call("journal", json!({"entry": focus, "focus": focus}))
            .await;
    }
    let handle = format!("session:{}", run.run());

    // The zero page: refused, naming the argument and what to send.
    run.refused(
        "recall",
        json!({"subject": &handle, "history": "focus", "history_most": 0}),
    )
    .await
    .says("history_most");

    // The pair: one change at a time is read, and following the pointer moves.
    let value_of = |answer: &super::dsl::Answer| {
        answer.json()["objects"][0]["history"]["writes"][0]["value"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let first = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "focus", "history_most": 1}),
        )
        .await;
    let pointer = first.json()["objects"][0]["history"]["not_shown"].clone();
    let offset = pointer["offset"]
        .as_u64()
        .unwrap_or_else(|| panic!("the first page points on: {}", first.raw()));
    let second = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "focus", "history_most": 1,
                   "offset": offset}),
        )
        .await;
    assert_eq!(value_of(&first), "checking the fix", "{}", first.raw());
    assert_ne!(
        value_of(&second),
        value_of(&first),
        "following the pointer moved to older changes: {}",
        second.raw()
    );
    run.wrap("read the focus history a change at a time").await;
    story.finish().await;
}

/// "Show me the newest wording first." The newest wording is the one written
/// last. A beat jojobot keeps for the run can be rewritten after a later entry
/// was, and its new wording is then the newest of all, so a page of one holds
/// it.
#[tokio::test]
async fn the_first_page_of_wordings_holds_the_one_written_last_even_from_an_early_entry() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    let handle = format!("session:{}", run.run());
    // An entry of the run's, then a write that makes jojobot's own beat, then
    // a second entry that is corrected, then another write that rewrites the
    // beat jojobot made before it.
    run.call("journal", json!({"entry": "a first beat"})).await;
    run.add("decision:contract-ordinance-story-curfew", "Curfew")
        .await;
    run.call("journal", json!({"entry": "a later beat"})).await;
    run.call(
        "amend_journal",
        json!({"entry": "a later beat, said better"}),
    )
    .await;
    run.add("decision:contract-ordinance-story-parking", "Parking")
        .await;

    let all = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "journal", "history_most": 20}),
        )
        .await;
    let wordings: Vec<(String, String)> = all.json()["objects"][0]["history"]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("the amended entries come back: {}", all.raw()))
        .iter()
        .flat_map(|entry| {
            entry["wordings"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|w| {
                    (
                        w["written_at"].as_str().unwrap_or_default().to_string(),
                        w["text"].as_str().unwrap_or_default().to_string(),
                    )
                })
        })
        .collect();
    let written_last = wordings
        .iter()
        .max_by(|a, b| a.0.cmp(&b.0))
        .expect("wordings")
        .1
        .clone();
    assert_ne!(
        written_last, "a later beat, said better",
        "the case needs a wording written after the later entry's: {wordings:?}"
    );

    let page = run
        .call(
            "recall",
            json!({"subject": &handle, "history": "journal", "history_most": 1}),
        )
        .await;
    let held: usize = page.json()["objects"][0]["history"]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a page of one: {}", page.raw()))
        .iter()
        .map(|entry| entry["wordings"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(held, 1, "{}", page.raw());
    assert!(
        page.raw().contains(&written_last),
        "the first page holds the wording written last, {written_last}: {}",
        page.raw()
    );
    run.wrap("read the journal's wordings newest first").await;
    story.finish().await;
}
