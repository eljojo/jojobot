//! "The moving job holds five pieces of work. How far along is it?"
//!
//! A piece of work that bundles several holds a list whose entries link them.
//! **Reading the list says how far along the bundle is: "3 of 5 done", then
//! "done".** Done is the linked work's own state, the verdict the ready view gives
//! (finished), so ticking an entry changes nothing, even when the tick lets it
//! drop out of view. A dropped entry is no longer part of the bundle. Nothing is
//! stored: the count is worked out at each read, and the bundle's own status stays
//! what it was.
//!
//! The roster names are the cast: a move, a bundle of building work, a packing
//! list.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The five pieces of work the bundle holds, each linked by an entry.
const PARTS: [&str; 5] = [
    "work:pour-the-slab",
    "work:order-the-lumber",
    "work:frame-the-walls",
    "work:build-the-roof",
    "work:paint-the-door",
];

/// A piece of work with its status.
pub(super) async fn work(s: &Session, handle: &str, status: &str) {
    let (_, slug) = handle.split_once(':').expect("a handle is kind:slug");
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": {"status": status}}),
    )
    .await;
}

/// An entry on a list that points at a handle. Its address.
pub(super) async fn entry_about(s: &Session, list: &str, text: &str, object: &str) -> String {
    s.call(
        "capture",
        json!({"subject": list, "content": text, "provenance": "testimony",
               "shape": "about", "object": object}),
    )
    .await
    .json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string()
}

/// An entry on a list that points at nothing. Its address.
pub(super) async fn plain_entry(s: &Session, list: &str, text: &str) -> String {
    s.call(
        "capture",
        json!({"subject": list, "content": text, "provenance": "testimony"}),
    )
    .await
    .json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string()
}

/// One tick, as a caller sends it.
pub(super) async fn tick(s: &Session, list: &str, entry: &str, act: &str) {
    s.call(
        "capture",
        json!({"subject": list, "content": format!("{act} it"), "provenance": "testimony",
               "tick": {"entry": entry, "act": act}}),
    )
    .await;
}

/// The list as a read serves it.
pub(super) async fn the_list(s: &Session, list: &str) -> Value {
    s.call("recall", json!({"subject": list})).await.json()["objects"][0].clone()
}

/// What the list says about how far along it is, or `Null` when it says nothing.
async fn progress(s: &Session, list: &str) -> Value {
    the_list(s, list).await["progress"].clone()
}

/// The bundle `work:the-movers` under the move, holding the `leave` list
/// `list:packing`, with an entry linking each of the five parts (their addresses
/// in order) and a plain entry behind them.
async fn a_move_with_five_parts(s: &Session) -> Vec<String> {
    s.add("project:the-move", "The move").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "the-movers", "name": "The movers",
               "source": "user-named", "parent": "project:the-move",
               "sets": {"status": "now"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Packing",
               "source": "user-named", "parent": "work:the-movers",
               "sets": {"tick": "leave"}}),
    )
    .await;
    let mut at = Vec::new();
    for part in PARTS {
        work(s, part, "next").await;
        at.push(entry_about(s, "list:packing", part, part).await);
    }
    plain_entry(s, "list:packing", "buy fuel").await;
    at
}

pub(super) async fn finish(s: &Session, handle: &str) {
    s.call(
        "update_entity",
        json!({"handle": handle, "sets": {"status": "done"}}),
    )
    .await;
}

#[tokio::test]
async fn a_bundle_reads_how_far_along_it_is_from_the_work_its_entries_link() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_move_with_five_parts(&s).await;

    // ── nothing is done yet ─────────────────────────────────────────────────
    let none = progress(&s, "list:packing").await;
    assert_eq!(none, json!({"done": 0, "of": 5, "reads": "0 of 5 done"}));

    // ── three of the five are done ──────────────────────────────────────────
    for part in &PARTS[..3] {
        finish(&s, part).await;
    }
    let three = progress(&s, "list:packing").await;
    assert_eq!(three, json!({"done": 3, "of": 5, "reads": "3 of 5 done"}));

    // ── ticking an entry changes nothing, even when it drops from view ──────
    tick(&s, "list:packing", &at[3], "done").await;
    let list = the_list(&s, "list:packing").await;
    assert_eq!(list["progress"], three, "a tick is not the work being done");
    assert_eq!(
        list["entries_left_out"]["done"], 1,
        "the ticked entry has left the view and is counted all the same: {list}"
    );
    let shown = list["entries"].as_array().expect("entries");
    assert!(
        shown.iter().all(|entry| entry["text"] != PARTS[3]),
        "the ticked entry is out of view: {shown:?}"
    );

    // ── all five done: "done" ───────────────────────────────────────────────
    for part in &PARTS[3..] {
        finish(&s, part).await;
    }
    let all = progress(&s, "list:packing").await;
    assert_eq!(all["reads"], "done", "{all}");
    assert_eq!(
        (all["done"].clone(), all["of"].clone()),
        (json!(5), json!(5))
    );

    // ── one reopened: "4 of 5 done" ─────────────────────────────────────────
    s.call(
        "update_entity",
        json!({"handle": PARTS[4], "sets": {"status": "next"}}),
    )
    .await;
    let reopened = progress(&s, "list:packing").await;
    assert_eq!(reopened["reads"], "4 of 5 done", "{reopened}");

    // ── dropping an entry takes it out of the five ──────────────────────────
    tick(&s, "list:packing", &at[0], "dropped").await;
    let dropped = progress(&s, "list:packing").await;
    assert_eq!(dropped["reads"], "3 of 4 done", "{dropped}");

    // ── the bundle's own status is what it was, and work that waits on it
    //    still waits ────────────────────────────────────────────────────────
    let movers = s
        .call("recall", json!({"subject": "work:the-movers"}))
        .await
        .json();
    assert_eq!(
        movers["objects"][0]["fields"]["status"], "now",
        "progress is read, never written: {movers}"
    );
    work(&s, "work:sand-the-floor", "next").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "wire-the-outlets", "name": "wire-the-outlets",
               "source": "user-named",
               "sets": {"status": "next", "depends_on": "work:the-movers"}}),
    )
    .await;
    let ready = s.call("recall", json!({"view": "ready"})).await.json();
    let ready: Vec<&str> = ready["objects"]
        .as_array()
        .expect("the ready view's objects")
        .iter()
        .filter_map(|object| object["id"].as_str())
        .collect();
    assert!(ready.contains(&"work:sand-the-floor"), "{ready:?}");
    assert!(
        !ready.contains(&"work:wire-the-outlets"),
        "work that depends on the bundle stays blocked: {ready:?}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_plain_entry_or_a_link_to_a_person_is_not_counted_and_is_not_an_error() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "leave"}}),
    )
    .await;
    s.add("person:alpha", "Alpha").await;
    plain_entry(&s, "list:errands", "buy fuel").await;
    entry_about(&s, "list:errands", "call the person", "person:alpha").await;
    let list = the_list(&s, "list:errands").await;
    assert_eq!(list["entries"].as_array().map(Vec::len), Some(2), "{list}");
    assert!(
        list.get("progress").is_none(),
        "no linked work means no progress: {list}"
    );
    // The positive the negative depends on: one linked work and the list reads one.
    work(&s, "work:pour-the-slab", "done").await;
    entry_about(&s, "list:errands", "the slab", "work:pour-the-slab").await;
    let list = the_list(&s, "list:errands").await;
    assert_eq!(list["progress"]["reads"], "done", "{list}");
    assert_eq!(list["progress"]["of"], 1, "{list}");
    story.finish().await;
}

#[tokio::test]
async fn work_whose_status_word_means_done_on_this_instance_counts_as_done() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    // The instance maps its own word to done, as the ready view reads it.
    s.call(
        "capture",
        json!({"subject": "topic:instance", "content": "what finished means here",
               "provenance": "testimony", "sets": {"status_word/finished": "done"}}),
    )
    .await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Packing",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "leave"}}),
    )
    .await;
    work(&s, "work:pour-the-slab", "finished").await;
    work(&s, "work:order-the-lumber", "next").await;
    entry_about(&s, "list:packing", "the slab", "work:pour-the-slab").await;
    entry_about(&s, "list:packing", "the lumber", "work:order-the-lumber").await;
    let read = progress(&s, "list:packing").await;
    assert_eq!(read["reads"], "1 of 2 done", "{read}");
    story.finish().await;
}
