//! "I marked the job finished, and a step on its checklist was still open."
//!
//! **A work item or project is not set done while a list hung on it has an open
//! entry.** The refusal names each open entry, and the same write lands once
//! every entry is ticked done or dropped. Every verb that can set a status is
//! held to it: a capture, an entity update and a claim update.
//!
//! The roster names are the cast: a slab to pour, a checklist, a rotation.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::add_entry;

/// A piece of work in progress with a list of the given setting hung on it.
async fn work_with_a_list(s: &Session, list: &str, setting: &str) {
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "pour-the-slab", "name": "Pour the slab",
               "source": "user-named", "sets": {"status": "now"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": list, "name": "Slab steps",
               "source": "user-named", "parent": "work:pour-the-slab",
               "sets": {"tick": setting}}),
    )
    .await;
}

fn ticking(list: &str, entry: &str, act: &str) -> Value {
    json!({"subject": list, "content": format!("{act} it"), "provenance": "testimony",
           "tick": {"entry": entry, "act": act}})
}

async fn status_of(s: &Session) -> String {
    let answer = s
        .call("recall", json!({"subject": "work:pour-the-slab"}))
        .await;
    answer.json()["objects"][0]["fields"]["status"]
        .as_str()
        .unwrap_or_else(|| panic!("the work item holds a status: {}", answer.raw()))
        .to_string()
}

/// **Each verb refuses done while two entries are open and names them; ticking
/// one narrows the refusal to the other; dropping that one lets the write
/// land.**
#[tokio::test]
async fn done_is_refused_through_every_verb_while_an_entry_is_open_and_lands_after() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    work_with_a_list(&s, "checklist", "sink").await;
    let claim = s
        .call(
            "capture",
            json!({"subject": "work:pour-the-slab", "content": "under way",
                   "provenance": "testimony", "sets": {"status": "now"}}),
        )
        .await;
    let claim = claim.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    let formwork = add_entry(&s, "list:checklist", "set the formwork", None).await;
    let rebar = add_entry(&s, "list:checklist", "lay the rebar", None).await;

    let by_capture = s
        .refused(
            "capture",
            json!({"subject": "work:pour-the-slab", "content": "finished",
                   "provenance": "testimony", "sets": {"status": "done"}}),
        )
        .await;
    by_capture.says(&formwork).says(&rebar);
    let by_update = s
        .refused(
            "update_entity",
            json!({"handle": "work:pour-the-slab", "sets": {"status": "done"}}),
        )
        .await;
    by_update.says(&formwork).says(&rebar);
    let by_claim = s
        .refused(
            "update_fact",
            json!({"address": claim, "sets": {"status": "done"}}),
        )
        .await;
    by_claim.says(&formwork).says(&rebar);
    assert_eq!(
        status_of(&s).await,
        "now",
        "a refused write changes nothing"
    );

    s.call("capture", ticking("list:checklist", &formwork, "done"))
        .await;
    let narrowed = s
        .refused(
            "update_entity",
            json!({"handle": "work:pour-the-slab", "sets": {"status": "done"}}),
        )
        .await;
    narrowed.says(&rebar);
    assert!(
        !narrowed.raw().contains(&formwork),
        "a ticked entry is not named: {}",
        narrowed.raw()
    );

    s.call("capture", ticking("list:checklist", &rebar, "dropped"))
        .await;
    s.call(
        "update_entity",
        json!({"handle": "work:pour-the-slab", "sets": {"status": "done"}}),
    )
    .await;
    assert_eq!(status_of(&s).await, "done");
    story.finish().await;
}

/// **A rotation never completes, so its open entries do not hold a work item
/// back.**
#[tokio::test]
async fn a_rotation_does_not_hold_a_work_item_from_done() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    work_with_a_list(&s, "packing", "back").await;
    add_entry(&s, "list:packing", "check the forms", None).await;
    s.call(
        "update_entity",
        json!({"handle": "work:pour-the-slab", "sets": {"status": "done"}}),
    )
    .await;
    assert_eq!(status_of(&s).await, "done");
    story.finish().await;
}
