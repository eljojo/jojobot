//! "I have a long packing list to put down, and a dozen things to tick off."
//!
//! **A bot adds many entries to a list, or ticks many entries, in one call.**
//! Each entry is still its own claim with its own address, and each tick is
//! still its own record; the call only carries several. One bad item refuses the
//! whole call, names the item by its position and writes nothing, and a resend
//! under the same `request_id` writes nothing a second time.
//!
//! The roster names are the cast: a camping trip and a packing list.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// A list's entries as `text:state`, in order.
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

/// The ticks a list holds, newest first.
async fn ticks_of(s: &Session, list: &str) -> Vec<Value> {
    let answer = s
        .call("recall", json!({"subject": list, "ticks": true}))
        .await;
    answer.json()["objects"][0]["ticks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Twenty-three things to pack, in the order a person would say them.
fn the_packing() -> Vec<String> {
    [
        "tent",
        "stove",
        "sleeping bag",
        "lantern",
        "water jug",
        "cooler",
        "first aid kit",
        "rain tarp",
        "camp chairs",
        "hatchet",
        "matches",
        "rope",
        "headlamp",
        "map",
        "compass",
        "sunscreen",
        "bug spray",
        "frying pan",
        "kettle",
        "plates",
        "spoons",
        "trash bags",
        "firewood",
    ]
    .map(String::from)
    .to_vec()
}

/// **Twenty-three entries go down in one call, in the order sent, and the
/// receipt lists the address of every one.** Each entry is a claim of its own,
/// so a read of the list serves each under the address the receipt named.
#[tokio::test]
async fn twenty_three_entries_in_one_call_read_back_in_the_order_sent() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let packing = the_packing();

    let receipt = s
        .call(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "entries": packing}),
        )
        .await;
    let named = receipt.json()["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("the receipt lists the entries: {}", receipt.raw()))
        .clone();
    assert_eq!(named.len(), 23, "{}", receipt.raw());
    let addresses: Vec<&str> = named
        .iter()
        .map(|entry| entry["address"].as_str().expect("an address each"))
        .collect();
    let mut distinct = addresses.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 23, "each entry has its own address");

    let expected: Vec<String> = packing.iter().map(|t| format!("{t}:open")).collect();
    assert_eq!(read_states(&s, "list:packing").await, expected);

    // The addresses the receipt gave are the ones the list serves.
    let read = s.call("recall", json!({"subject": "list:packing"})).await;
    let served: Vec<String> = read.json()["objects"][0]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["address"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(served, addresses);
    story.finish().await;
}

/// **With `after`, the whole run goes behind that entry, in order.** The entries
/// that were after it come after the run.
#[tokio::test]
async fn entries_with_after_place_the_whole_run_behind_that_entry_in_order() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    add_entry(&s, "list:packing", "tent", None).await;
    let second = add_entry(&s, "list:packing", "stove", None).await;
    add_entry(&s, "list:packing", "lantern", None).await;

    s.call(
        "capture",
        json!({"subject": "list:packing", "provenance": "testimony",
               "after": second, "entries": ["kettle", "plates", "spoons"]}),
    )
    .await;
    assert_eq!(
        read_states(&s, "list:packing").await,
        [
            "tent:open",
            "stove:open",
            "kettle:open",
            "plates:open",
            "spoons:open",
            "lantern:open"
        ]
    );
    story.finish().await;
}

/// **Three ticks in one call are three tick records**, and the list's read shows
/// each state. An item may be `dropped`, which is not done.
#[tokio::test]
async fn three_ticks_in_one_call_are_three_tick_records() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;
    let lantern = add_entry(&s, "list:packing", "lantern", None).await;
    let kettle = add_entry(&s, "list:packing", "kettle", None).await;

    s.call(
        "capture",
        json!({"subject": "list:packing", "provenance": "testimony",
               "ticks": [{"entry": tent, "act": "done"},
                         {"entry": lantern, "act": "done"},
                         {"entry": kettle, "act": "dropped"}]}),
    )
    .await;
    let states = read_states(&s, "list:packing").await;
    assert!(states.contains(&"stove:open".to_string()), "{states:?}");
    assert!(states.contains(&"tent:done".to_string()), "{states:?}");
    assert!(states.contains(&"lantern:done".to_string()), "{states:?}");
    assert!(
        !states.iter().any(|state| state.starts_with("kettle:done")),
        "dropped is not done: {states:?}"
    );
    let ticks = ticks_of(&s, "list:packing").await;
    assert_eq!(ticks.len(), 3, "{ticks:?}");
    let acts: Vec<&str> = ticks.iter().filter_map(|t| t["act"].as_str()).collect();
    assert_eq!(acts.iter().filter(|a| **a == "done").count(), 2, "{acts:?}");
    assert_eq!(acts.iter().filter(|a| **a == "dropped").count(), 1);
    let _ = stove;
    story.finish().await;
}

/// **One bad item refuses the whole call, names the item by its position, and
/// writes nothing.** Three kinds of bad item, each after good ones, so a call
/// that wrote as it went would leave the good ones behind.
#[tokio::test]
async fn one_bad_item_refuses_the_whole_call_names_its_position_and_writes_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    let before = read_states(&s, "list:packing").await;

    // An empty entry, third of four.
    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "entries": ["stove", "lantern", "", "kettle"]}),
        )
        .await;
    refused.says("item 3").says("\"wrote\":false");
    assert_eq!(read_states(&s, "list:packing").await, before);

    // An address that is not on the list, second of three.
    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "ticks": [{"entry": tent, "act": "done"},
                             {"entry": "list:packing#f999", "act": "done"},
                             {"entry": tent, "act": "undone"}]}),
        )
        .await;
    refused.says("item 2").says("\"wrote\":false");
    assert_eq!(read_states(&s, "list:packing").await, before);
    assert!(ticks_of(&s, "list:packing").await.is_empty());

    // An act that is not one, last of two.
    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "ticks": [{"entry": tent, "act": "done"},
                             {"entry": tent, "act": "finished"}]}),
        )
        .await;
    refused.says("item 2").says("\"wrote\":false");
    assert!(ticks_of(&s, "list:packing").await.is_empty());
    assert_eq!(read_states(&s, "list:packing").await, before);
    story.finish().await;
}

/// **A resend under the same `request_id` writes nothing a second time.** A
/// rotation list would turn twice if it did.
#[tokio::test]
async fn a_resent_batch_writes_nothing_a_second_time() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "back"})).await;
    let entries = json!({"subject": "list:packing", "provenance": "testimony",
                         "entries": ["tent", "stove", "lantern"], "request_id": "pack-1"});
    s.call("capture", entries.clone()).await;
    s.call("capture", entries).await.says("repeat");
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open", "lantern:open"]
    );

    let read = s.call("recall", json!({"subject": "list:packing"})).await;
    let tent = read.json()["objects"][0]["entries"][0]["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let turn = json!({"subject": "list:packing", "provenance": "testimony",
                      "ticks": [{"entry": tent, "act": "rotated"}], "request_id": "turn-1"});
    s.call("capture", turn.clone()).await;
    s.call("capture", turn).await.says("repeat");
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["stove:open", "lantern:open", "tent:open"],
        "the rotation turned once"
    );
    assert_eq!(ticks_of(&s, "list:packing").await.len(), 1);
    story.finish().await;
}

/// **The single forms behave as they did**, and a batch past the cap is refused
/// naming the cap with nothing written.
#[tokio::test]
async fn the_single_forms_are_unchanged_and_a_batch_past_the_cap_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "tent is packed",
               "provenance": "testimony", "tick": {"entry": tent, "act": "done"}}),
    )
    .await;
    assert_eq!(read_states(&s, "list:packing").await, ["tent:done"]);

    let too_many: Vec<String> = (1..=51).map(|n| format!("thing number {n}")).collect();
    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "entries": too_many}),
        )
        .await;
    refused.says("50").says("\"wrote\":false");
    assert_eq!(read_states(&s, "list:packing").await, ["tent:done"]);
    story.finish().await;
}

/// **An item that passes the check and fails at the write stops there, names its
/// position, and the same `request_id` writes the rest and none twice.** The edge
/// of the third entry points at a thing that does not exist yet: the store judges
/// that at the write, so the first two land. The caller makes the thing and sends
/// the same call again, and the list holds each entry once.
#[tokio::test]
async fn an_item_that_fails_at_the_write_stops_there_and_the_same_id_finishes_the_rest() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let call = json!({"subject": "list:packing", "provenance": "testimony",
                      "request_id": "pack-2",
                      "entries": ["tent", "stove",
                                  {"content": "sharpen the axe", "shape": "about",
                                   "object": "thing:kettle"},
                                  "lantern"]});

    let stopped = s.refused("capture", call.clone()).await;
    stopped.says("item 3");
    stopped.says("2");
    assert_eq!(
        read_states(&s, "list:packing").await,
        ["tent:open", "stove:open"],
        "the two before the failed item landed and stay"
    );

    s.add("thing:kettle", "The Kettle").await;
    let finished = s.call("capture", call).await;
    assert_eq!(
        finished.json()["finished_a_cut_batch"]["already_written"],
        2,
        "{}",
        finished.raw()
    );
    assert_eq!(
        read_states(&s, "list:packing").await,
        [
            "tent:open",
            "stove:open",
            "sharpen the axe:open",
            "lantern:open"
        ],
        "each entry once, in the order sent"
    );
    story.finish().await;
}
