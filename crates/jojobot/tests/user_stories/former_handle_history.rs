//! "I only ever knew it by its old name. What did it say, and what was built
//! on it?"
//!
//! **A rename moves the name and nothing else**, so every question that was
//! answerable by the old handle has to stay answerable by it. A reader holding
//! an old note, an old mention or an old address asks with the handle they
//! have; a trace that matched on the handle as sent would find the thing and
//! return no history, which reads as *nothing was ever written*.
//!
//! ⭐ **Each trace argument is asked twice, by the former handle and by the
//! current one, and the two answers are the same non-empty history, in a case
//! of its own so a break in one walk names that walk.** The answer names the
//! thing by its current handle either way.

use serde_json::{Value, json};

use super::dsl::Story;

const OLD: &str = "thing:shelf";
const NEW: &str = "thing:omega";

/// The part of an answer a trace fills, and the thing it names.
fn traced(answer: &super::dsl::Answer, part: &str) -> Value {
    let object = &answer.json()["objects"][0];
    json!({"id": object["id"], part: object[part]})
}

/// **A thing with a history under its first handle, renamed by a later run.**
/// Returns the reader's session and the address of a claim that was corrected
/// and the address of the claim that replaced it, both written under the
/// former handle.
async fn a_shelf_renamed_after_its_history_was_written() -> (super::dsl::Session, String, String) {
    let story = Story::begin("bot:otto").await;

    // ── Spring · the thing is made, a key is written twice, a claim is
    // corrected ────────────────────────────────────────────────────────────
    let spring = story.session_on("2026-04-14", None).await;
    spring.add(OLD, "The Shelf").await;
    for colour in ["red", "blue"] {
        spring
            .call(
                "capture",
                json!({"subject": OLD, "content": format!("the flag is {colour}"),
                       "provenance": "testimony", "sets": {"flag": colour}}),
            )
            .await;
    }
    let first = spring.fact(OLD, "holds the spare keys").await;
    let corrected = spring
        .replace(&first, "holds the spare keys and the map", json!({}))
        .await;
    spring.wrap("wrote the shelf down").await;

    // ── Autumn · another run renames it ──────────────────────────────────────
    let autumn = story.session_on("2026-09-15", Some("new")).await;
    autumn
        .call(
            "rename_entity",
            json!({"handle": OLD, "to": NEW, "recorded_at": "2026-09-15"}),
        )
        .await;
    autumn.wrap("renamed the shelf").await;

    // ── Winter · the reader ───────────────────────────────────────────────────
    (
        story.session_on("2026-12-20", Some("new")).await,
        first,
        corrected,
    )
}

/// The same address, written under the current handle.
fn under_new(address: &str) -> String {
    address.replacen(OLD, NEW, 1)
}

#[tokio::test]
async fn the_writes_of_a_key_are_the_same_by_a_former_handle() {
    let (winter, _, _) = a_shelf_renamed_after_its_history_was_written().await;

    let by_new = winter
        .shape(
            "what did the flag say?",
            json!({"subject": NEW, "history": "flag"}),
        )
        .await;
    let by_old = winter
        .shape(
            "what did the flag say, by the old name?",
            json!({"subject": OLD, "history": "flag"}),
        )
        .await;

    by_new.number("/objects/0/history/count", 2);
    assert_eq!(
        traced(&by_old, "history"),
        traced(&by_new, "history"),
        "the key's writes differ by the handle asked with: {}",
        by_old.raw()
    );
}

#[tokio::test]
async fn the_writes_of_a_claim_are_the_same_by_an_address_under_a_former_handle() {
    let (winter, first, _) = a_shelf_renamed_after_its_history_was_written().await;

    let by_new = winter
        .shape(
            "has the record always said that?",
            json!({"history_record": under_new(&first)}),
        )
        .await;
    let by_old = winter
        .shape(
            "has the record always said that, by the old address?",
            json!({"history_record": first}),
        )
        .await;

    by_new.number("/objects/0/record_history/count", 2);
    by_old.number("/objects/0/record_history/count", 2);
    assert_eq!(
        by_old.json()["objects"][0]["record_history"]["writes"],
        by_new.json()["objects"][0]["record_history"]["writes"],
        "the claim's writes differ by the address asked with: {}",
        by_old.raw()
    );
    assert_eq!(
        by_old.json()["objects"][0]["id"],
        NEW,
        "the answer names the thing by its current handle: {}",
        by_old.raw()
    );
}

#[tokio::test]
async fn the_versions_of_a_thing_are_the_same_by_a_former_handle() {
    let (winter, _, _) = a_shelf_renamed_after_its_history_was_written().await;

    let by_new = winter
        .shape("what was it called?", json!({"history_record": NEW}))
        .await;
    let by_old = winter
        .shape(
            "what was it called, by the old name?",
            json!({"history_record": OLD}),
        )
        .await;

    assert!(
        by_new.json()["objects"][0]["entity_history"]["count"]
            .as_i64()
            .unwrap_or(0)
            > 0,
        "the current handle finds no versions, so the pair proves nothing: {}",
        by_new.raw()
    );
    assert_eq!(
        by_old.json()["objects"][0]["entity_history"],
        by_new.json()["objects"][0]["entity_history"],
        "the former handle finds a different history of the thing: {}",
        by_old.raw()
    );
}

#[tokio::test]
async fn what_was_built_on_a_claim_is_the_same_by_an_address_under_a_former_handle() {
    let (winter, first, corrected) = a_shelf_renamed_after_its_history_was_written().await;

    let by_new = winter
        .shape("what replaced it?", json!({"built_on": under_new(&first)}))
        .await;
    let by_old = winter
        .shape(
            "what replaced it, by the old address?",
            json!({"built_on": first}),
        )
        .await;

    by_new.number("/built_on/count", 1);
    by_new.says(&under_new(&corrected));
    assert_eq!(
        by_old.json()["built_on"]["claims"],
        by_new.json()["built_on"]["claims"],
        "the claims built on it differ by the address asked with: {}",
        by_old.raw()
    );
}
