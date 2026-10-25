//! "The view of what the twelfth rule governs, saved by its number."
//!
//! **A saved view's `where` may name a decision by its number**, the way a live
//! read of the same line does. The value is bound to the decision it names, so a
//! rename of the decision changes nothing the view selects, and a number nobody
//! holds is refused where the view is saved.

use serde_json::json;

use super::dsl::{Session, Story};

const RULES: &str = "topic:rules-springfield";

/// Springfield's numbered rules with its twelfth rule, and a ledger that records
/// which rule governs it.
async fn the_ledger_under_the_twelfth_rule(s: &Session) {
    s.add(RULES, "Springfield rules").await;
    s.call(
        "capture",
        json!({"subject": RULES, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "springfield-12", "name": "Leads",
               "source": "user-named", "parent": RULES, "sets": {"number": "12"}}),
    )
    .await;
    s.add("thing:omega", "Omega").await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "governed by the twelfth rule",
               "provenance": "testimony",
               "sets": {"ruled_by": "decision:springfield-12"}}),
    )
    .await;
}

fn ids(read: &serde_json::Value) -> Vec<String> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer has objects: {read}"))
        .iter()
        .map(|object| object["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// **The live read, the saved view and a rename agree.**
#[tokio::test]
async fn a_saved_view_names_a_decision_by_its_number_and_follows_its_rename() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    let line = format!("ruled_by = {RULES}/12");

    // The control: the decision named by its own handle selects the ledger.
    let direct = s
        .call(
            "recall",
            json!({"kind": "thing", "where": "ruled_by = decision:springfield-12"}),
        )
        .await;
    assert_eq!(ids(&direct.json()), ["thing:omega"], "{}", direct.raw());
    let live = s
        .call("recall", json!({"kind": "thing", "where": line}))
        .await;
    assert_eq!(ids(&live.json()), ["thing:omega"], "{}", live.raw());

    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "twelfth-rule-ledger", "name": "Twelfth rule ledger",
               "source": "user-named", "sets": {"selects": "thing", "where": line}}),
    )
    .await;
    let saved = s
        .call("recall", json!({"view": "twelfth-rule-ledger"}))
        .await;
    assert_eq!(ids(&saved.json()), ["thing:omega"], "{}", saved.raw());

    s.call(
        "rename_entity",
        json!({"handle": "decision:springfield-12", "to": "decision:springfield-13"}),
    )
    .await;
    let after = s
        .call("recall", json!({"view": "twelfth-rule-ledger"}))
        .await;
    assert_eq!(ids(&after.json()), ["thing:omega"], "{}", after.raw());
    story.finish().await;
}

/// **A number nobody holds is refused where the view is saved**, naming what it
/// found instead, and nothing is saved.
#[tokio::test]
async fn a_saved_view_naming_a_number_nobody_holds_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.refused(
        "add_entity",
        json!({"kind": "view", "handle": "ninety-ninth-rule-ledger", "name": "Ninety-ninth ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "where": format!("ruled_by = {RULES}/99")}}),
    )
    .await
    .says("\"wrote\":false");
    story.finish().await;
}

/// **A view that takes a decision as an input takes it by its number**, and the
/// kind it must be is the kind of the decision, not of the collection it is
/// addressed through. A number nobody holds is refused when the view is asked.
#[tokio::test]
async fn a_view_input_of_kind_decision_takes_a_decision_by_its_number() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "rulings-ledger", "name": "Rulings ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "inputs": "ruling: kind decision",
                        "where": "ruled_by = $ruling"}}),
    )
    .await;

    let by_handle = s
        .call(
            "recall",
            json!({"view": "rulings-ledger",
                   "inputs": {"ruling": "decision:springfield-12"}}),
        )
        .await;
    assert_eq!(
        ids(&by_handle.json()),
        ["thing:omega"],
        "{}",
        by_handle.raw()
    );
    let by_number = s
        .call(
            "recall",
            json!({"view": "rulings-ledger",
                   "inputs": {"ruling": format!("{RULES}/12")}}),
        )
        .await;
    assert_eq!(
        ids(&by_number.json()),
        ["thing:omega"],
        "{}",
        by_number.raw()
    );
    s.refused(
        "recall",
        json!({"view": "rulings-ledger", "inputs": {"ruling": format!("{RULES}/99")}}),
    )
    .await
    .says("names nothing");
    story.finish().await;
}
