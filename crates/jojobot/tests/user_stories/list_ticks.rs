//! "I ticked it off, and the list did what it is for."
//!
//! **A tick is its own record on the list, and it never edits the entry.** What a
//! tick does depends on the list's `tick` setting: a checklist keeps what is done
//! in view at the bottom, a queue lets it go, a rotation moves it to the end. An
//! entry that no longer applies is dropped, which is not done.
//!
//! The roster names are the cast: a camping trip, a packing list, three people.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// What a list serves: its entries as `text:state`, in order.
pub(super) async fn read_states(s: &Session, list: &str) -> Vec<String> {
    let answer = s.call("recall", json!({"subject": list})).await;
    answer.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", answer.raw()))
        .iter()
        .map(|entry| {
            format!(
                "{}:{}",
                entry["text"].as_str().unwrap_or_default(),
                entry["state"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

/// One tick, as a caller sends it.
fn ticking(list: &str, entry: &str, act: &str) -> Value {
    json!({"subject": list, "content": format!("{act} it"), "provenance": "testimony",
           "tick": {"entry": entry, "act": act}})
}

/// Three entries on a fresh list with this setting; their addresses in order.
async fn a_list_of_three(s: &Session, setting: &str) -> Vec<String> {
    a_list_on_the_trip(s, "packing", json!({"tick": setting})).await;
    vec![
        add_entry(s, "list:packing", "tent", None).await,
        add_entry(s, "list:packing", "stove", None).await,
        add_entry(s, "list:packing", "lantern", None).await,
    ]
}

/// **A checklist keeps what is done in view, at the bottom, in the order things
/// were done, and an entry ticked undone is where its place put it.**
#[tokio::test]
async fn a_sink_list_keeps_done_entries_at_the_bottom_in_the_order_done() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "sink").await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open", "lantern:open"]
    );

    s.call("capture", ticking("list:packing", &at[1], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "lantern:open", "stove:done"]
    );
    s.call("capture", ticking("list:packing", &at[0], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["lantern:open", "stove:done", "tent:done"]
    );

    s.call("capture", ticking("list:packing", &at[1], "undone"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open", "tent:done"]
    );
    story.finish().await;
}

/// **A queue lets a done entry go from the open view, says it left, and the tick
/// history still has it.** The default read carries no ticks.
#[tokio::test]
async fn a_leave_list_lets_a_done_entry_go_and_says_so() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "leave").await;
    s.call("capture", ticking("list:packing", &at[0], "done"))
        .await;

    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open"]
    );
    let read = s.call("recall", json!({"subject": "list:packing"})).await;
    let object = read.json()["objects"][0].clone();
    assert_eq!(object["entries_left_out"]["done"], 1, "{object}");
    assert!(
        object.get("ticks").is_none(),
        "the default read carries no ticks: {object}"
    );

    let history = s
        .call("recall", json!({"subject": "list:packing", "ticks": true}))
        .await;
    let ticks = history.json()["objects"][0]["ticks"].clone();
    assert_eq!(ticks[0]["act"], "done", "{ticks}");
    assert_eq!(ticks[0]["entry"], at[0].as_str(), "{ticks}");
    story.finish().await;
}

/// **A rotation takes its turn and goes to the end, still open; two full circuits
/// end in the original order.**
#[tokio::test]
async fn a_back_list_rotates_and_two_circuits_end_in_the_original_order() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "back").await;

    s.call("capture", ticking("list:packing", &at[0], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open", "tent:open"]
    );
    for _ in 0..2 {
        for entry in &at {
            s.call("capture", ticking("list:packing", entry, "done"))
                .await;
        }
    }
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open", "lantern:open"]
    );
    story.finish().await;
}

/// **Dropping is not doing**: a dropped entry is out of the open view, counted as
/// dropped, named dropped in the history and never done. It is brought back with
/// `undone`, and a tick on it before that is refused.
#[tokio::test]
async fn a_dropped_entry_reads_as_dropped_and_never_as_done() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "sink").await;
    s.call("capture", ticking("list:packing", &at[1], "dropped"))
        .await;

    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "lantern:open"]
    );
    let read = s
        .call("recall", json!({"subject": "list:packing", "ticks": true}))
        .await;
    let object = read.json()["objects"][0].clone();
    assert_eq!(object["entries_left_out"]["dropped"], 1, "{object}");
    assert_eq!(object["entries_left_out"]["done"], 0, "{object}");
    assert_eq!(object["ticks"][0]["act"], "dropped", "{object}");

    s.refused("capture", ticking("list:packing", &at[1], "done"))
        .await
        .says("undone")
        .says("\"wrote\":false");
    s.call("capture", ticking("list:packing", &at[1], "undone"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open", "lantern:open"]
    );
    story.finish().await;
}

/// **An entry written as the operator's testimony stays testimony when another
/// session ticks it**, and its words are the words it was written with. The tick
/// is a record of its own.
#[tokio::test]
async fn a_tick_never_edits_the_entry_it_names() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;

    let other = story.session_in("UTC", Some("new")).await;
    other
        .call("capture", ticking("list:packing", &tent, "done"))
        .await;

    let read = s
        .call("recall", json!({"subject": "list:packing", "facts": true}))
        .await;
    let facts = read.json()["objects"][0]["facts"].clone();
    let entry = facts
        .as_array()
        .expect("facts")
        .iter()
        .find(|fact| fact["address"] == tent.as_str())
        .unwrap_or_else(|| panic!("the entry is still there: {facts}"));
    assert_eq!(entry["provenance"], "testimony", "{entry}");
    assert_eq!(entry["content"], "tent", "{entry}");
    assert_eq!(read_states(&s, "list:packing").await, ["tent:done"]);
    story.finish().await;
}

/// **A tick sent again under one request id lands once**, so a rotation never
/// turns twice. The paired positive is a second turn under a new id, which does.
#[tokio::test]
async fn a_retried_tick_under_one_request_id_rotates_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "back").await;

    let mut turn = ticking("list:packing", &at[0], "done");
    turn["request_id"] = json!("turn-one");
    s.call("capture", turn.clone()).await;
    s.call("capture", turn).await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open", "tent:open"]
    );
    let history = s
        .call("recall", json!({"subject": "list:packing", "ticks": true}))
        .await;
    assert_eq!(history.json()["objects"][0]["ticks_total"], 1);

    let mut next = ticking("list:packing", &at[1], "done");
    next["request_id"] = json!("turn-two");
    s.call("capture", next).await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["lantern:open", "tent:open", "stove:open"]
    );
    story.finish().await;
}

/// **The ticks are their own read, newest first, in parts.** Five ticks, two a
/// part: the first part says how many are left and the call that reads them.
#[tokio::test]
async fn the_tick_history_is_read_in_parts_newest_first() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "leave").await;
    for (entry, act) in [
        (0, "done"),
        (1, "dropped"),
        (2, "done"),
        (0, "undone"),
        (0, "done"),
    ] {
        s.call("capture", ticking("list:packing", &at[entry], act))
            .await;
    }

    let first = s
        .call(
            "recall",
            json!({"subject": "list:packing", "ticks": true, "history_most": 2}),
        )
        .await;
    let object = first.json()["objects"][0].clone();
    assert_eq!(
        object["ticks"].as_array().map(Vec::len),
        Some(2),
        "{object}"
    );
    assert_eq!(object["ticks"][0]["act"], "done", "{object}");
    assert_eq!(object["ticks"][1]["act"], "undone", "{object}");
    assert_eq!(object["ticks_total"], 5);
    assert_eq!(object["ticks_left_out"]["count"], 3, "{object}");

    let rest = s
        .call(
            "recall",
            json!({"subject": "list:packing", "ticks": true, "history_most": 2, "ticks_offset": 4}),
        )
        .await;
    let object = rest.json()["objects"][0].clone();
    assert_eq!(
        object["ticks"].as_array().map(Vec::len),
        Some(1),
        "{object}"
    );
    assert!(object.get("ticks_left_out").is_none(), "{object}");
    story.finish().await;
}

/// **A tick is refused with a way forward when it cannot be recorded**, and
/// nothing lands. Each refusal is paired with the call that works.
#[tokio::test]
async fn a_tick_that_cannot_be_recorded_is_refused_with_a_way_forward() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = a_list_of_three(&s, "sink").await;

    s.refused(
        "capture",
        ticking("list:packing", "list:packing#f99", "done"),
    )
    .await
    .says(&at[0])
    .says("\"wrote\":false");
    s.refused("capture", ticking("list:packing", &at[0], "finished"))
        .await
        .holds_at("/fix_by", "change")
        .says("\"wrote\":false");
    s.refused("capture", ticking("list:packing", &at[0], "rotated"))
        .await
        .holds_at("/fix_by", "change")
        .says("\"wrote\":false");
    s.refused(
        "capture",
        ticking("thing:springfield-camping", &at[0], "done"),
    )
    .await
    .holds_at("/fix_by", "change")
    .says("\"wrote\":false");
    let mut with_after = ticking("list:packing", &at[0], "done");
    with_after["after"] = json!("first");
    s.refused("capture", with_after)
        .await
        .holds_at("/fix_by", "change")
        .says("\"wrote\":false");
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open", "lantern:open"]
    );

    s.call("capture", ticking("list:packing", &at[0], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open", "tent:done"]
    );

    let back = story.session_in("UTC", Some("new")).await;
    back.call(
        "add_entity",
        json!({"kind": "list", "handle": "rotation", "name": "Bins",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "back"}}),
    )
    .await;
    let turn = add_entry(&back, "list:rotation", "bins", None).await;
    back.refused("capture", ticking("list:rotation", &turn, "undone"))
        .await
        .says("back")
        .says("\"wrote\":false");
    story.finish().await;
}

/// A second list on the trip, with its own entries; their addresses in order.
async fn an_errands_list(s: &Session, setting: &str) -> Vec<String> {
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": setting}}),
    )
    .await;
    vec![
        add_entry(s, "list:errands", "letter", None).await,
        add_entry(s, "list:errands", "bread", None).await,
        add_entry(s, "list:errands", "bins", None).await,
    ]
}

/// **Two lists merged keep each list's ticks on the entries they were made for.**
/// The folded entries take new places on the survivor, and a tick names its entry
/// by the id the entry has there, so the survivor reads each folded entry in the
/// state its own list left it in.
#[tokio::test]
async fn ticks_follow_their_entries_when_two_lists_merge() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let packing = a_list_of_three(&s, "sink").await;
    let errands = an_errands_list(&s, "sink").await;
    s.call("capture", ticking("list:packing", &packing[1], "done"))
        .await;
    s.call("capture", ticking("list:errands", &errands[0], "done"))
        .await;
    s.call("capture", ticking("list:errands", &errands[2], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:errands").await,
        ["bread:open", "letter:done", "bins:done"]
    );

    s.merge_entities("list:errands", "list:packing", "one list, not two", None)
        .await;

    assert_eq!(
        read_states(&s, "list:packing").await,
        [
            "tent:open",
            "lantern:open",
            "bread:open",
            "stove:done",
            "letter:done",
            "bins:done"
        ]
    );
    story.finish().await;
}

/// **An address kept from before a merge still places `after` on the survivor.**
/// The entry it names was folded in and has a new id there; the address is
/// followed to it as every address of a folded claim is.
#[tokio::test]
async fn an_entry_address_from_before_a_merge_still_places_after_on_the_survivor() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_of_three(&s, "sink").await;
    let errands = an_errands_list(&s, "sink").await;

    s.merge_entities("list:errands", "list:packing", "one list, not two", None)
        .await;
    add_entry(&s, "list:packing", "kettle", Some(&errands[0])).await;

    assert_eq!(
        read_states(&s, "list:packing").await,
        [
            "tent:open",
            "stove:open",
            "lantern:open",
            "letter:open",
            "kettle:open",
            "bread:open",
            "bins:open"
        ]
    );
    story.finish().await;
}

/// **A dropped entry on a back list comes back with `undone`, as on every other
/// list.** The back-list rule is about turns, and an entry that was dropped never
/// took one: `done` on it is refused naming `undone`, and `undone` puts it back in
/// the rotation.
#[tokio::test]
async fn undone_revives_a_dropped_entry_on_a_back_list() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    let at = an_errands_list(&s, "back").await;

    s.call("capture", ticking("list:errands", &at[2], "dropped"))
        .await;
    assert_eq!(
        read_states(&s, "list:errands").await,
        ["letter:open", "bread:open"]
    );
    s.refused("capture", ticking("list:errands", &at[2], "done"))
        .await
        .says("undone")
        .says("\"wrote\":false");

    s.call("capture", ticking("list:errands", &at[2], "undone"))
        .await;
    assert_eq!(
        read_states(&s, "list:errands").await,
        ["letter:open", "bread:open", "bins:open"]
    );
    story.finish().await;
}

/// **A turn on a back list is still not undone.** An entry that took its turn went
/// to the end and is not put back; the refusal names `done`, the act that takes
/// the next turn, and the rotation stays as the turn left it. **Paired with the
/// case above**, so a rule loosened for every entry fails here while the revival
/// of a dropped one passes.
#[tokio::test]
async fn undone_still_refuses_to_undo_a_turn_on_a_back_list() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    let at = an_errands_list(&s, "back").await;

    s.call("capture", ticking("list:errands", &at[0], "done"))
        .await;
    assert_eq!(
        read_states(&s, "list:errands").await,
        ["bread:open", "bins:open", "letter:open"]
    );
    s.refused("capture", ticking("list:errands", &at[0], "undone"))
        .await
        .says("done")
        .says("\"wrote\":false");
    assert_eq!(
        read_states(&s, "list:errands").await,
        ["bread:open", "bins:open", "letter:open"],
        "the refused undo left the turn where it was"
    );
    story.finish().await;
}
