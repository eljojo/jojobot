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

    // The record is there from the first boot and names nobody: the boot says
    // there is no entity yet, and names the two handles a session needs to make
    // one.
    let (boot, first) = story.boot_sending_no_zone(Some("new")).await;
    let said = operator_line(&boot);
    assert!(
        said.contains("add_entity") && said.contains("topic:instance"),
        "with no record, the line says how to make one: {boot}",
    );
    assert!(
        !said.contains("add_entity topic:instance"),
        "and does not send a session to make the record, which exists: {boot}",
    );
    assert!(!said.starts_with("person:"), "no operator is named: {boot}");

    // The record is there, and holds only a zone: still no operator, and the
    // line says so rather than going quiet.
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
    first.add("person:milhouse", "Milhouse").await;
    let made = first
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "phi", "name": "Phi", "source": "user-named",
                "parent": "project:atlas",
                "sets": {"status": "waiting", "waiting_on": "person:lisa"},
            }),
        )
        .await
        .json();
    assert_eq!(made["id"], "work:phi", "{made}");
    // **A second piece of work waits on somebody else.** Without it the read
    // below would find the only work item whatever it filtered by.
    first
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "sigma", "name": "Sigma", "source": "user-named",
                "parent": "project:atlas",
                "sets": {"status": "waiting", "waiting_on": "person:milhouse"},
            }),
        )
        .await;
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
    waiting.says("work:phi").never_says("work:sigma");
    // …and asked of the other person, the answer is the other piece of work.
    later
        .call(
            "recall",
            json!({
                "kind": "work",
                "fields": [{"key": "waiting_on", "value": "person:milhouse"}],
            }),
        )
        .await
        .says("work:sigma")
        .never_says("work:phi");
    story.finish().await;
}

/// **A fresh instance holds its own record before anybody writes to it.** The
/// first session reads `topic:instance` and writes a key onto it with no
/// `add_entity` first, so the operator, the zone and any setting the record
/// holds need no step that only exists to make the record.
#[tokio::test]
async fn a_fresh_instance_holds_its_own_record_and_takes_a_key_with_no_add_entity_first() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.recall("topic:instance")
        .await
        .says("topic:instance")
        .never_says("columns");

    s.event_with(
        "topic:instance",
        "how this instance names its columns",
        json!({ "columns": "someday, next, now, waiting, done, review" }),
        &[],
    )
    .await;
    s.recall("topic:instance")
        .await
        .says("topic:instance")
        .says("someday, next, now, waiting, done, review");

    // The boot's operator line no longer sends a caller to make the record.
    let (boot, _) = story.boot_sending_no_zone(Some("new")).await;
    assert!(
        !operator_line(&boot).contains("add_entity topic:instance"),
        "the record exists from the first boot, so the line does not say to make it: {boot}",
    );
    story.finish().await;
}
