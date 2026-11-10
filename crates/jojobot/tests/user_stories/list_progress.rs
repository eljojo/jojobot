//! "The shed holds five pieces of work. How far along is it?"
//!
//! A piece of work that bundles several holds a list whose entries link them.
//! **Reading the list says how far along the bundle is: "3 of 5 done", then
//! "done".** Done is the linked work's own state, the verdict the ready view gives
//! (finished), so ticking an entry changes nothing, even when the tick lets it
//! drop out of view. A dropped entry is no longer part of the bundle. Nothing is
//! stored: the count is worked out at each read, and the bundle's own status stays
//! what it was.
//!
//! The roster names are the cast: a shed, a bundle of building work, a
//! checklist.

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

/// The bundle `work:the-utilities` under the shed, holding the `leave` list
/// `list:checklist`, with an entry linking each of the five parts (their addresses
/// in order) and a plain entry behind them.
async fn a_shed_with_five_parts(s: &Session) -> Vec<String> {
    s.add("project:the-shed", "The shed").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "the-utilities", "name": "The utilities",
               "source": "user-named", "parent": "project:the-shed",
               "sets": {"status": "now"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "checklist", "name": "Checklist",
               "source": "user-named", "parent": "work:the-utilities",
               "sets": {"tick": "leave"}}),
    )
    .await;
    let mut at = Vec::new();
    for part in PARTS {
        work(s, part, "next").await;
        at.push(entry_about(s, "list:checklist", part, part).await);
    }
    plain_entry(s, "list:checklist", "buy nails").await;
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
    let at = a_shed_with_five_parts(&s).await;

    // ── nothing is done yet ─────────────────────────────────────────────────
    let none = progress(&s, "list:checklist").await;
    assert_eq!(none, json!({"done": 0, "of": 5, "reads": "0 of 5 done"}));

    // ── three of the five are done ──────────────────────────────────────────
    for part in &PARTS[..3] {
        finish(&s, part).await;
    }
    let three = progress(&s, "list:checklist").await;
    assert_eq!(three, json!({"done": 3, "of": 5, "reads": "3 of 5 done"}));

    // ── ticking an entry changes nothing, even when it drops from view ──────
    tick(&s, "list:checklist", &at[3], "done").await;
    let list = the_list(&s, "list:checklist").await;
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
    let all = progress(&s, "list:checklist").await;
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
    let reopened = progress(&s, "list:checklist").await;
    assert_eq!(reopened["reads"], "4 of 5 done", "{reopened}");

    // ── dropping an entry takes it out of the five ──────────────────────────
    tick(&s, "list:checklist", &at[0], "dropped").await;
    let dropped = progress(&s, "list:checklist").await;
    assert_eq!(dropped["reads"], "3 of 4 done", "{dropped}");

    // ── the bundle's own status is what it was, and work that waits on it
    //    still waits ────────────────────────────────────────────────────────
    let utilities = s
        .call("recall", json!({"subject": "work:the-utilities"}))
        .await
        .json();
    assert_eq!(
        utilities["objects"][0]["fields"]["status"], "now",
        "progress is read, never written: {utilities}"
    );
    work(&s, "work:sand-the-floor", "next").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "wire-the-outlets", "name": "wire-the-outlets",
               "source": "user-named",
               "sets": {"status": "next", "depends_on": "work:the-utilities"}}),
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
    plain_entry(&s, "list:errands", "buy nails").await;
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
        json!({"kind": "list", "handle": "checklist", "name": "Checklist",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "leave"}}),
    )
    .await;
    work(&s, "work:pour-the-slab", "finished").await;
    work(&s, "work:order-the-lumber", "next").await;
    entry_about(&s, "list:checklist", "the slab", "work:pour-the-slab").await;
    entry_about(&s, "list:checklist", "the lumber", "work:order-the-lumber").await;
    let read = progress(&s, "list:checklist").await;
    assert_eq!(read["reads"], "1 of 2 done", "{read}");
    story.finish().await;
}

/// **A piece of work archived before it finished leaves the bundle**, the way a
/// dropped entry does: it is in neither `done` nor `of`, so a bundle whose other
/// work is done reads "done". A finished piece stays counted as done, archived or
/// not, and restoring the archived one puts it back in `of`.
#[tokio::test]
async fn an_archived_piece_that_never_finished_leaves_the_bundle_and_a_finished_one_stays() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:the-move", "The move").await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Packing",
               "source": "user-named", "parent": "project:the-move",
               "sets": {"tick": "leave"}}),
    )
    .await;
    for part in &PARTS[..3] {
        work(&s, part, "next").await;
        entry_about(&s, "list:packing", part, part).await;
    }
    finish(&s, PARTS[0]).await;
    finish(&s, PARTS[1]).await;
    let archive = |handle: &'static str, restore: bool| {
        let s = &s;
        async move {
            s.call(
                "archive_entity",
                json!({"handle": handle, "reason": "cancelled", "restore": restore}),
            )
            .await;
        }
    };
    let whole = progress(&s, "list:packing").await;
    assert_eq!(whole["reads"], "2 of 3 done", "{whole}");

    // ── the third, never finished, is archived: it leaves the bundle ────────
    archive(PARTS[2], false).await;
    let left = progress(&s, "list:packing").await;
    assert_eq!(left["reads"], "done", "{left}");
    assert_eq!(
        (left["done"].clone(), left["of"].clone()),
        (json!(2), json!(2))
    );

    // ── a finished piece archived afterwards still counts as done ───────────
    archive(PARTS[1], false).await;
    let finished_and_archived = progress(&s, "list:packing").await;
    assert_eq!(
        finished_and_archived["reads"], "done",
        "{finished_and_archived}"
    );
    assert_eq!(finished_and_archived["of"], 2, "{finished_and_archived}");

    // ── the third comes back: it is part of the bundle again ────────────────
    archive(PARTS[2], true).await;
    let restored = progress(&s, "list:packing").await;
    assert_eq!(restored["reads"], "2 of 3 done", "{restored}");
    story.finish().await;
}

/// The bundle as a read of it serves it, narrowed to the keys when some are named.
async fn the_bundle(s: &Session, keys: Option<Value>) -> Value {
    let mut args = json!({"subject": "work:the-utilities"});
    if let Some(keys) = keys {
        args["keys"] = keys;
    }
    s.call("recall", args).await.json()["objects"][0].clone()
}

#[tokio::test]
async fn the_read_of_a_bundle_says_how_far_along_each_list_under_it_is() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_five_parts(&s).await;

    let none = the_bundle(&s, None).await;
    assert_eq!(
        none["progress"],
        json!([{"list": "list:checklist", "done": 0, "of": 5, "reads": "0 of 5 done"}]),
        "{none}"
    );
    assert_eq!(
        none["fields"]["status"], "now",
        "the bundle's own status is untouched: {none}"
    );
    assert_eq!(
        none["progress"][0]["reads"],
        progress(&s, "list:checklist").await["reads"],
        "the bundle's row and the list's own read are one count"
    );
    story.finish().await;
}

/// **A bundle read again after its linked work moved is served the new progress.**
/// The bundle itself was not written, so what says the progress changed is the work
/// its lists link; a read that held the bundle back as unchanged would show the
/// session the old count.
#[tokio::test]
async fn a_bundle_read_again_after_its_linked_work_moved_serves_the_new_progress() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_five_parts(&s).await;

    let before = the_bundle(&s, None).await;
    assert_eq!(before["progress"][0]["reads"], "0 of 5 done", "{before}");
    for part in &PARTS[..2] {
        finish(&s, part).await;
    }
    let after = the_bundle(&s, None).await;
    assert_eq!(
        after["progress"][0]["reads"], "2 of 5 done",
        "the progress is worked out at each read: {after}"
    );
    story.finish().await;
}

/// **Only a one-subject read of a piece of work carries the rows.** A list keeps
/// its own block, a project or anything else that is not work gets none even with a
/// list of its own that links work, a browse and a walk get none, a read that names
/// other keys works none out, a list that links no work has no row, and an archived
/// list is no part of the bundle.
#[tokio::test]
async fn only_the_one_subject_read_of_a_piece_of_work_carries_the_rows() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_shed_with_five_parts(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "Errands",
               "source": "user-named", "parent": "work:the-utilities"}),
    )
    .await;
    plain_entry(&s, "list:errands", "buy nails").await;

    let bundle = the_bundle(&s, None).await;
    assert_eq!(
        bundle["progress"].as_array().map(Vec::len),
        Some(1),
        "a list that links no work has no row: {bundle}"
    );

    let narrowed = the_bundle(&s, Some(json!(["status"]))).await;
    assert!(narrowed.get("progress").is_none(), "{narrowed}");

    // A project with a list of its own that links work: not a piece of work.
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Packing",
               "source": "user-named", "parent": "project:the-shed"}),
    )
    .await;
    entry_about(&s, "list:packing", "the roof", PARTS[3]).await;
    let project = s
        .call("recall", json!({"subject": "project:the-shed"}))
        .await
        .json()["objects"][0]
        .clone();
    assert!(project.get("progress").is_none(), "{project}");

    // A walk from the bundle is not the one-subject read.
    let walked = s
        .call(
            "recall",
            json!({"subject": "work:the-utilities", "follow": {"direction": "out"}}),
        )
        .await
        .json();
    assert!(
        walked["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .all(|object| object.get("progress").is_none() || object["id"] != "work:the-utilities"),
        "{walked}"
    );

    // A browse of many carries none, and a list keeps its own single block.
    let browse = s
        .call("recall", json!({"parent": "project:the-shed"}))
        .await
        .json();
    assert!(
        browse["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .all(|object| object["id"] == "list:packing"
                || object["id"] == "list:checklist"
                || object.get("progress").is_none()),
        "{browse}"
    );
    assert_eq!(
        the_list(&s, "list:checklist").await["progress"]["of"],
        5,
        "a list's own read is one block, not rows"
    );

    // An archived list is no part of the bundle.
    s.call(
        "archive_entity",
        json!({"handle": "list:checklist", "reason": "cancelled"}),
    )
    .await;
    let after = the_bundle(&s, None).await;
    assert!(after.get("progress").is_none(), "{after}");
    story.finish().await;
}
