//! "Who is the operator, and what is waiting on them?"
//!
//! **A piece of work that waits on the operator has to wait on somebody.** Until
//! the operator is a thing in the graph, `waiting_on` has nothing to point at
//! and the answer to "what waits on whom" is prose. The instance's own record,
//! `topic:instance`, holds the operator under one key, `operator`, beside the
//! zone it already holds. The boot names them in one line, so a session reads
//! who to write into `waiting_on` before it writes anything.
//!
//! **An instance with no operator entity says so, and says how to make one.**
//! The record absent, and the record present with no such key, are the same
//! answer: a session is never left to infer an operator from silence.

use serde_json::{Value, json};

use super::dsl::Story;

/// What a boot's one operator line says, as the string it is.
fn operator_line(boot: &Value) -> &str {
    boot["operator"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot carries one operator line: {boot}"))
}

#[tokio::test]
async fn the_boot_names_the_operator_and_what_waits_on_them_is_one_read_away() {
    let story = Story::begin("bot:otto").await;

    // No record at all: the boot says there is no entity yet, and names the two
    // handles a session needs to make one.
    let (boot, first) = story.boot_sending_no_zone(Some("new")).await;
    let said = operator_line(&boot);
    assert!(
        said.contains("add_entity") && said.contains("topic:instance"),
        "with no record, the line says how to make one: {boot}",
    );
    assert!(!said.starts_with("person:"), "no operator is named: {boot}");

    // The record is there, and holds only a zone: still no operator, and the
    // line says so rather than going quiet.
    first.add("topic:instance", "This instance").await;
    first
        .event_with(
            "topic:instance",
            "where the operator lives",
            json!({ "timezone": "Etc/GMT+12" }),
            &[],
        )
        .await;
    let (boot, _) = story.boot_sending_no_zone(Some("new")).await;
    assert_eq!(boot["timezone"]["from"], "instance", "{boot}");
    let said = operator_line(&boot);
    assert!(
        said.contains("add_entity") && !said.starts_with("person:"),
        "a record with no operator key names nobody: {boot}",
    );

    // A piece of work waits on a person, and the operator is that person.
    first.add("project:atlas", "Atlas").await;
    first.add("person:lisa", "Lisa").await;
    let made = first
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "phi", "name": "Phi", "source": "user-named",
                "parent": "project:atlas",
                "fields": {"status": "waiting", "waiting_on": "person:lisa"},
            }),
        )
        .await
        .json();
    assert_eq!(made["id"], "work:phi", "{made}");
    first
        .event_with(
            "topic:instance",
            "who the operator is",
            json!({ "operator": "person:lisa" }),
            &[],
        )
        .await;

    // The boot names them, by the handle a `waiting_on` holds.
    let (boot, later) = story.boot_sending_no_zone(Some("new")).await;
    assert_eq!(operator_line(&boot), "person:lisa", "{boot}");

    // What waits on whom, for the operator the boot named: one read.
    let waiting = later
        .call(
            "recall",
            json!({
                "kind": "work",
                "fields": [{"key": "waiting_on", "value": operator_line(&boot)}],
            }),
        )
        .await;
    waiting.says("work:phi");
    story.finish().await;
}
