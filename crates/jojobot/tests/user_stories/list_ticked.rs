//! "The checklist has twenty-four things on it. How many have I ticked?"
//!
//! **Reading a list says how far along it is by its own ticks**: "1 of 24 done"
//! on a checklist, done and open on a queue, turns on a rotation. The count is
//! worked out at the read from the same entry states the read already shows, and
//! stored nowhere. A dropped entry is not part of the list, so it is not in the
//! total. A list that holds nothing says nothing, because a count of nothing is
//! not a count.
//!
//! **It is not `progress`.** `progress` is how far along the work the entries
//! link is; a tick says the list item is done and not that the work is.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::list_progress::{entry_about, finish, plain_entry, the_list, tick, work};

/// A list under a thing, with the tick setting given.
async fn a_list(s: &Session, handle: &str, setting: &str) {
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    let (_, slug) = handle.split_once(':').expect("a handle is kind:slug");
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": slug, "name": slug,
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": setting}}),
    )
    .await;
}

/// `count` plain entries, their addresses in order.
async fn entries(s: &Session, list: &str, count: usize) -> Vec<String> {
    let mut at = Vec::new();
    for n in 1..=count {
        at.push(plain_entry(s, list, &format!("item {n:02}")).await);
    }
    at
}

/// What the list says its ticks come to, or `Null` when it says nothing.
async fn ticked(s: &Session, list: &str) -> Value {
    the_list(s, list).await["ticked"].clone()
}

#[tokio::test]
async fn a_checklist_reads_how_many_of_its_entries_are_ticked_and_leaves_a_dropped_one_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list(&s, "list:checklist", "sink").await;
    let at = entries(&s, "list:checklist", 25).await;

    // ── nothing ticked yet: the count is there, at nought ───────────────────
    assert_eq!(
        ticked(&s, "list:checklist").await,
        json!({"done": 0, "of": 25, "reads": "0 of 25 done"})
    );

    // ── one done and one dropped: the dropped one is not part of the list ────
    tick(&s, "list:checklist", &at[0], "done").await;
    tick(&s, "list:checklist", &at[1], "dropped").await;
    assert_eq!(
        ticked(&s, "list:checklist").await,
        json!({"done": 1, "of": 24, "reads": "1 of 24 done"})
    );

    // ── ticking it undone takes it back ─────────────────────────────────────
    tick(&s, "list:checklist", &at[0], "undone").await;
    assert_eq!(
        ticked(&s, "list:checklist").await,
        json!({"done": 0, "of": 24, "reads": "0 of 24 done"})
    );
    story.finish().await;
}

#[tokio::test]
async fn a_queue_reads_how_many_are_done_and_how_many_are_still_open() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list(&s, "list:errands", "leave").await;
    let at = entries(&s, "list:errands", 10).await;
    for entry in &at[..3] {
        tick(&s, "list:errands", entry, "done").await;
    }
    // The three done entries have left the open view, and the count still has them.
    assert_eq!(
        ticked(&s, "list:errands").await,
        json!({"done": 3, "open": 7})
    );
    story.finish().await;
}

#[tokio::test]
async fn a_rotation_reads_how_many_turns_it_has_taken_and_has_no_done() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list(&s, "list:phi-checklist", "back").await;
    let at = entries(&s, "list:phi-checklist", 3).await;
    // Three turns by a done tick and two by a rotation.
    for (entry, act) in [
        (&at[0], "done"),
        (&at[1], "done"),
        (&at[2], "rotated"),
        (&at[0], "rotated"),
        (&at[1], "done"),
    ] {
        tick(&s, "list:phi-checklist", entry, act).await;
    }
    assert_eq!(ticked(&s, "list:phi-checklist").await, json!({"turns": 5}));
    // An entry that was dropped is no longer part of the rotation, and its two
    // turns go with it.
    tick(&s, "list:phi-checklist", &at[1], "dropped").await;
    assert_eq!(ticked(&s, "list:phi-checklist").await, json!({"turns": 3}));
    story.finish().await;
}

#[tokio::test]
async fn a_list_that_links_work_reads_its_ticks_and_the_works_progress_apart() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list(&s, "list:checklist", "sink").await;
    for part in ["work:pour-the-slab", "work:order-the-lumber"] {
        work(&s, part, "next").await;
    }
    let slab = entry_about(&s, "list:checklist", "slab", "work:pour-the-slab").await;
    entry_about(&s, "list:checklist", "lumber", "work:order-the-lumber").await;

    // The entry is ticked and its work is not finished: the two counts differ.
    tick(&s, "list:checklist", &slab, "done").await;
    let list = the_list(&s, "list:checklist").await;
    assert_eq!(
        list["ticked"],
        json!({"done": 1, "of": 2, "reads": "1 of 2 done"})
    );
    assert_eq!(
        list["progress"],
        json!({"done": 0, "of": 2, "reads": "0 of 2 done"})
    );

    // The work is finished and nothing was ticked for it: the counts differ the other way.
    finish(&s, "work:order-the-lumber").await;
    let list = the_list(&s, "list:checklist").await;
    assert_eq!(list["ticked"]["done"], 1, "{list}");
    assert_eq!(list["progress"]["done"], 1, "{list}");
    finish(&s, "work:pour-the-slab").await;
    tick(&s, "list:checklist", &slab, "undone").await;
    let list = the_list(&s, "list:checklist").await;
    assert_eq!(list["ticked"]["done"], 0, "{list}");
    assert_eq!(list["progress"]["done"], 2, "{list}");
    story.finish().await;
}

#[tokio::test]
async fn a_list_that_holds_nothing_says_nothing_about_ticks() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list(&s, "list:checklist", "sink").await;
    let list = the_list(&s, "list:checklist").await;
    assert!(
        list.get("ticked").is_none(),
        "an empty list has nothing to count: {list}"
    );

    // Paired in one case: the same list with one entry carries the count, and with
    // that entry dropped it holds nothing again.
    let only = plain_entry(&s, "list:checklist", "one thing").await;
    assert_eq!(
        ticked(&s, "list:checklist").await,
        json!({"done": 0, "of": 1, "reads": "0 of 1 done"})
    );
    tick(&s, "list:checklist", &only, "dropped").await;
    let list = the_list(&s, "list:checklist").await;
    assert!(
        list.get("ticked").is_none(),
        "a list whose only entry is dropped holds nothing: {list}"
    );
    story.finish().await;
}
