//! "The next thing on the queue is waiting on another, and the queue says so."
//!
//! **Position is not readiness.** A list entry that points at a piece of work
//! reads back with that work's own state beside it: its status, its owner and
//! what it waits on. The list never skips a blocked entry or promotes a ready
//! one, and ticking an entry says the list item is done, not that the work is.
//! The work's own status stays the one record of whether the work is finished.
//!
//! The roster names are the cast: a camping trip, a queue, pieces of work.

use serde_json::{Value, json};

use super::dsl::{Answer, Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// A piece of work with its status, owner and what it depends on.
async fn work(s: &Session, handle: &str, status: &str, depends_on: Option<&str>) {
    let (_, slug) = handle.split_once(':').expect("a handle is kind:slug");
    let mut sets = json!({"status": status, "owner": "bot:gamma"});
    if let Some(depends_on) = depends_on {
        sets["depends_on"] = json!(depends_on);
    }
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": sets}),
    )
    .await;
}

/// An entry on the queue that points at a piece of work. Its address.
async fn entry_about(s: &Session, list: &str, text: &str, work: &str) -> String {
    s.call(
        "capture",
        json!({"subject": list, "content": text, "provenance": "testimony",
               "shape": "about", "object": work}),
    )
    .await
    .json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string()
}

/// The entries a list serves.
async fn entries(s: &Session, list: &str) -> Vec<Value> {
    let answer = s.call("recall", json!({"subject": list})).await;
    answer.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", answer.raw()))
        .clone()
}

/// A queue whose head points at work that waits on another piece of work, and a
/// plain entry behind it.
async fn a_queue_with_blocked_work_at_its_head(s: &Session) -> String {
    a_list_on_the_trip(s, "packing", json!({"tick": "leave"})).await;
    work(s, "work:phi", "next", None).await;
    work(s, "work:sigma", "now", Some("work:phi")).await;
    let head = entry_about(s, "list:packing", "ship sigma", "work:sigma").await;
    add_entry(s, "list:packing", "buy fuel", None).await;
    head
}

/// **A queue whose head points at work waiting on another reads that head first,
/// with the work's status, owner and blocker beside it.** The entry behind it,
/// which points at nothing, carries no such block.
#[tokio::test]
async fn a_linked_head_reads_first_with_the_works_status_owner_and_blocker() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_queue_with_blocked_work_at_its_head(&s).await;

    let read = entries(&s, "list:packing").await;
    assert_eq!(read[0]["text"], "ship sigma", "{read:?}");
    let linked = &read[0]["linked"];
    assert_eq!(linked["handle"], "work:sigma", "{linked}");
    assert_eq!(linked["status"], "now", "{linked}");
    assert_eq!(linked["owner"], "bot:gamma", "{linked}");
    assert_eq!(linked["ready"], false, "{linked}");
    assert_eq!(linked["blocked_by"][0]["handle"], "work:phi", "{linked}");
    assert_eq!(linked["blocked_by"][0]["why"], "unfinished", "{linked}");
    assert_eq!(read[1]["text"], "buy fuel");
    assert!(
        read[1].get("linked").is_none(),
        "an entry that points at nothing shows no work: {}",
        read[1]
    );
    story.finish().await;
}

/// **The linked state is read from the work each time and stored nowhere**: when
/// the blocker is finished the head reads ready, and when the work itself is
/// finished it reads done.
#[tokio::test]
async fn the_linked_state_follows_the_work_and_is_never_a_copy() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_queue_with_blocked_work_at_its_head(&s).await;

    s.call(
        "update_entity",
        json!({"handle": "work:phi", "sets": {"status": "done"}}),
    )
    .await;
    let read = entries(&s, "list:packing").await;
    assert_eq!(read[0]["linked"]["ready"], true, "{}", read[0]);
    assert!(read[0]["linked"].get("blocked_by").is_none(), "{}", read[0]);

    s.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "done"}}),
    )
    .await;
    let read = entries(&s, "list:packing").await;
    assert_eq!(read[0]["linked"]["status"], "done", "{}", read[0]);
    assert_eq!(read[0]["linked"]["ready"], false, "{}", read[0]);
    assert_eq!(
        read[0]["linked"]["blocked_by"][0]["blocker"], "finished",
        "{}",
        read[0]
    );
    story.finish().await;
}

/// **The linked verdict is the one the ready view gives** for the same work, for
/// every kind of blocker the store can hold: a dependency that is unfinished, and
/// what the work waits on. A second copy of the rule would part from the view
/// on one of them.
#[tokio::test]
async fn the_linked_verdict_is_the_one_the_ready_view_gives() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "leave"})).await;
    work(&s, "work:phi", "next", None).await;
    work(&s, "work:sigma", "now", Some("work:phi")).await;
    work(&s, "work:delta", "next", None).await;
    work(&s, "work:tau", "waiting", None).await;
    s.call(
        "update_entity",
        json!({"handle": "work:tau", "sets": {"waiting_on": "work:delta"}}),
    )
    .await;
    entry_about(&s, "list:packing", "ship sigma", "work:sigma").await;
    entry_about(&s, "list:packing", "ship tau", "work:tau").await;

    let read = entries(&s, "list:packing").await;
    for (at, work) in [(0, "work:sigma"), (1, "work:tau")] {
        let view = s
            .call("recall", json!({"view": "ready", "subject": work}))
            .await;
        let object = view.json()["objects"][0].clone();
        assert_eq!(
            read[at]["linked"]["ready"], object["ready"],
            "{work}: the entry {} and the view {object}",
            read[at]
        );
        assert_eq!(
            read[at]["linked"]["blocked_by"], object["blocked_by"],
            "{work}: the entry {} and the view {object}",
            read[at]
        );
        assert!(
            object["blocked_by"]
                .as_array()
                .is_some_and(|blockers| !blockers.is_empty()),
            "{work} is blocked, or this case compares nothing: {object}"
        );
    }
    story.finish().await;
}

/// **Ticking a linked entry says the list item is done, and the receipt says the
/// work is still open; the work's own status is untouched.** The pair is a work
/// that is finished, whose receipt says so.
#[tokio::test]
async fn ticking_a_linked_entry_says_whether_the_work_is_still_open_and_leaves_it_alone() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let head = a_queue_with_blocked_work_at_its_head(&s).await;

    let ticked: Answer = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "done it", "provenance": "testimony",
                   "tick": {"entry": head, "act": "done"}}),
        )
        .await;
    let receipt = ticked.json()["ticked"].clone();
    assert_eq!(receipt["act"], "done", "{}", ticked.raw());
    assert_eq!(receipt["linked"]["handle"], "work:sigma", "{receipt}");
    assert_eq!(receipt["linked"]["work_open"], true, "{receipt}");

    let work = s.recall("work:sigma").await;
    work.says("\"status\":\"now\"");

    s.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "done"}}),
    )
    .await;
    let undone = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "again", "provenance": "testimony",
                   "tick": {"entry": head, "act": "undone"}}),
        )
        .await;
    assert_eq!(
        undone.json()["ticked"]["linked"]["work_open"],
        false,
        "{}",
        undone.raw()
    );

    let plain = add_entry(&s, "list:packing", "pack the van", None).await;
    let plain_tick = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "done it", "provenance": "testimony",
                   "tick": {"entry": plain, "act": "done"}}),
        )
        .await;
    assert!(
        plain_tick.json()["ticked"].get("linked").is_none(),
        "an entry that points at no work names none: {}",
        plain_tick.raw()
    );
    story.finish().await;
}
