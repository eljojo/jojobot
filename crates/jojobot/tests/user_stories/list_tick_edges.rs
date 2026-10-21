//! "I ticked it twice, or un-ticked what was never ticked, and the list said so."
//!
//! **A tick does what it says or is refused saying why.** Undoing an open entry
//! and finishing a finished one change nothing, so each is refused naming the
//! entry's state, and the list and its tick history stay as they were. A list's
//! `tick` setting is one of the three words and nothing else.
//!
//! The roster names are the cast: Homer's chores.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// One tick, as a caller sends it.
fn ticking(list: &str, entry: &str, act: &str) -> Value {
    json!({"subject": list, "content": format!("{act} it"), "provenance": "testimony",
           "tick": {"entry": entry, "act": act}})
}

/// What a list serves: its entries as `text:state`, in order.
async fn read_states(s: &Session, list: &str) -> Vec<String> {
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

/// How many ticks the list holds, read the way a caller reads the history.
async fn ticks_held(s: &Session, list: &str) -> usize {
    let answer = s
        .call("recall", json!({"subject": list, "ticks": true}))
        .await;
    answer.json()["objects"][0]["ticks"]
        .as_array()
        .map_or(0, Vec::len)
}

/// Three chores on a list with this setting; their addresses in order.
async fn homers_chores(s: &Session, setting: &str) -> Vec<String> {
    a_list_on_the_trip(s, "homer-chores", json!({"tick": setting})).await;
    vec![
        add_entry(s, "list:homer-chores", "mow", None).await,
        add_entry(s, "list:homer-chores", "rake", None).await,
        add_entry(s, "list:homer-chores", "sweep", None).await,
    ]
}

/// **Undoing an entry that is open is refused, and says the entry is open.**
/// Nothing is written: the list and its tick history are as they were. The paired
/// positive is `undone` on an entry that is done, which lands and opens it.
#[tokio::test]
async fn undone_on_an_open_entry_is_refused_and_writes_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = homers_chores(&s, "sink").await;

    let refused = s
        .refused("capture", ticking("list:homer-chores", &at[0], "undone"))
        .await;
    refused.says("open").says("\"wrote\":false");
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["mow:open", "rake:open", "sweep:open"]
    );
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 0);

    s.call("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    s.call("capture", ticking("list:homer-chores", &at[0], "undone"))
        .await;
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["mow:open", "rake:open", "sweep:open"]
    );
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 2);
    story.finish().await;
}

/// **Finishing an entry that is already done is refused on a sink list, and the
/// order things were done in does not move.** The paired positive is the first
/// `done` on each, which lands.
#[tokio::test]
async fn done_on_a_done_entry_of_a_sink_list_is_refused_and_the_done_order_holds() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = homers_chores(&s, "sink").await;

    s.call("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    s.call("capture", ticking("list:homer-chores", &at[1], "done"))
        .await;
    let before = read_states(&s, "list:homer-chores").await;
    assert_eq!(before, ["sweep:open", "mow:done", "rake:done"]);

    let refused = s
        .refused("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    refused.says("done").says("\"wrote\":false");
    assert_eq!(read_states(&s, "list:homer-chores").await, before);
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 2);
    story.finish().await;
}

/// **The same holds on a leave list**, where a done entry has left the open view
/// but still stands done: its second `done` is refused and the history keeps one
/// tick.
#[tokio::test]
async fn done_on_a_done_entry_of_a_leave_list_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = homers_chores(&s, "leave").await;

    s.call("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 1);

    let refused = s
        .refused("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    refused.says("done").says("\"wrote\":false");
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 1);
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["rake:open", "sweep:open"]
    );
    story.finish().await;
}

/// **On a sink list a tick that changes nothing is refused, and a batch holding
/// one is refused whole.** `done` then `done` is refused; `undone` on a done entry
/// lands; `undone` again is refused. A batch of two `done` ticks on one entry is
/// refused naming item 2 and writes nothing. Paired: `done`, `undone`, `done` in
/// one call lands as three ticks.
#[tokio::test]
async fn a_batch_with_a_tick_that_changes_nothing_is_refused_whole_and_one_in_order_lands() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = homers_chores(&s, "sink").await;

    s.call("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    s.refused("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;
    s.call("capture", ticking("list:homer-chores", &at[0], "undone"))
        .await;
    s.refused("capture", ticking("list:homer-chores", &at[0], "undone"))
        .await;
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 2);

    let twice = json!({"subject": "list:homer-chores", "provenance": "testimony",
                       "ticks": [{"entry": at[1], "act": "done"},
                                 {"entry": at[1], "act": "done"}]});
    let refused = s.refused("capture", twice).await;
    refused.says("item 2").says("\"wrote\":false");
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 2);
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["mow:open", "rake:open", "sweep:open"]
    );

    s.call(
        "capture",
        json!({"subject": "list:homer-chores", "provenance": "testimony",
               "ticks": [{"entry": at[1], "act": "done"},
                         {"entry": at[1], "act": "undone"},
                         {"entry": at[1], "act": "done"}]}),
    )
    .await;
    assert_eq!(ticks_held(&s, "list:homer-chores").await, 5);
    story.finish().await;
}

/// **A refused batch item states its reason once.** The item's own refusal ends
/// with the sentence every refusal ends with, and the batch's refusal carries that
/// sentence once, not twice, and joins the parts with a single full stop.
#[tokio::test]
async fn a_refused_batch_item_states_its_reason_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let at = homers_chores(&s, "sink").await;
    s.call("capture", ticking("list:homer-chores", &at[0], "done"))
        .await;

    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:homer-chores", "provenance": "testimony",
                   "ticks": [{"entry": at[1], "act": "done"},
                             {"entry": at[0], "act": "done"}]}),
        )
        .await;
    refused.says("item 2");
    let how = refused.json()["how_to_proceed"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        how.matches("Nothing is missing from the store").count(),
        1,
        "the tail is said once: {how}"
    );
    assert!(!how.contains(".."), "no doubled full stop: {how}");
    story.finish().await;
}

/// **A list's `tick` is `sink`, `leave` or `back` exactly, on every verb that
/// can set it, and a word outside them is refused naming all three.** A word that
/// differs only in case is outside them. The paired positives are the three words,
/// which land on the same verbs.
#[tokio::test]
async fn an_unknown_tick_setting_is_refused_on_every_write_that_sets_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "homer-chores", json!({"tick": "sink"})).await;

    let mut claim = String::new();
    for word in ["leave", "back", "sink"] {
        s.call(
            "update_entity",
            json!({"handle": "list:homer-chores", "sets": {"tick": word}}),
        )
        .await;
        let captured = s
            .call(
                "capture",
                json!({"subject": "list:homer-chores", "content": format!("tick is {word}"),
                       "provenance": "testimony", "sets": {"tick": word}}),
            )
            .await;
        claim = captured.json()["address"]
            .as_str()
            .expect("a capture answers with the address")
            .to_string();
        s.call(
            "update_fact",
            json!({"address": claim, "sets": {"tick": word}}),
        )
        .await;
    }

    for bad in ["Leave", "bounce"] {
        for refused in [
            s.refused(
                "update_entity",
                json!({"handle": "list:homer-chores", "sets": {"tick": bad}}),
            )
            .await,
            s.refused(
                "capture",
                json!({"subject": "list:homer-chores", "content": format!("tick is {bad}"),
                       "provenance": "testimony", "sets": {"tick": bad}}),
            )
            .await,
            s.refused(
                "update_fact",
                json!({"address": claim, "sets": {"tick": bad}}),
            )
            .await,
            s.refused(
                "add_entity",
                json!({"kind": "list", "handle": "marge-errands", "name": "Errands",
                       "source": "user-named", "parent": "thing:springfield-camping",
                       "sets": {"tick": bad}}),
            )
            .await,
        ] {
            refused
                .says("sink")
                .says("leave")
                .says("back")
                .says("\"wrote\":false");
        }
    }
    let read = s
        .call("recall", json!({"subject": "list:homer-chores"}))
        .await;
    assert_eq!(read.json()["objects"][0]["fields"]["tick"], "sink");
    story.finish().await;
}
