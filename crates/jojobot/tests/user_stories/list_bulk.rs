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
    add_entry(&s, "list:packing", "stove", None).await;
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
    // A dropped entry leaves the open entries without being done: the list
    // holds the other three, and the tick for it is the one `dropped`.
    assert_eq!(
        states,
        ["stove:open", "tent:done", "lantern:done"],
        "the dropped kettle is not among the entries"
    );
    let ticks = ticks_of(&s, "list:packing").await;
    assert_eq!(ticks.len(), 3, "{ticks:?}");
    let acts: Vec<&str> = ticks.iter().filter_map(|t| t["act"].as_str()).collect();
    assert_eq!(acts.iter().filter(|a| **a == "done").count(), 2, "{acts:?}");
    assert_eq!(acts.iter().filter(|a| **a == "dropped").count(), 1);
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
    refused
        .holds_at("/fix_by", "change")
        .says("\"wrote\":false");
    assert_eq!(read_states(&s, "list:packing").await, ["tent:done"]);
    // Paired: a batch of exactly the cap lands, so 51 is refused for its count.
    let at_the_cap: Vec<String> = (1..=50).map(|n| format!("thing number {n}")).collect();
    s.call(
        "capture",
        json!({"subject": "list:packing", "provenance": "testimony",
               "entries": at_the_cap}),
    )
    .await;
    assert_eq!(read_states(&s, "list:packing").await.len(), 51);
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
    stopped.holds_at("/fix_by", "retry");
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

/// Homer's chore list with two chores, and their addresses.
async fn homer_chores(s: &Session) -> (String, String) {
    a_list_on_the_trip(s, "homer-chores", json!({"tick": "sink"})).await;
    let a = add_entry(s, "list:homer-chores", "mow the lawn", None).await;
    let b = add_entry(s, "list:homer-chores", "wash the car", None).await;
    (a, b)
}

/// **A batch that drops one chore and ticks another is finished by the same call
/// when the second write fails once.** The retry finds the drop recorded and does
/// not judge it again against a list it has already changed: the lawn is dropped
/// once, the car is done once, and nothing is refused.
#[tokio::test]
async fn a_cut_batch_with_a_drop_is_finished_by_the_same_call_again() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (lawn, car) = homer_chores(&s).await;
    let call = json!({"subject": "list:homer-chores", "provenance": "testimony",
                      "request_id": "chores-1",
                      "ticks": [{"entry": lawn, "act": "dropped"},
                                {"entry": car, "act": "done"}]});

    story.fails_the_next_commit_of("item-1");
    s.refused("capture", call.clone()).await;
    assert_eq!(
        ticks_of(&s, "list:homer-chores").await.len(),
        1,
        "the drop before the failed item landed"
    );

    let finished = s.call("capture", call).await;
    assert_eq!(
        finished.json()["finished_a_cut_batch"]["already_written"],
        1,
        "{}",
        finished.raw()
    );
    assert_eq!(ticks_of(&s, "list:homer-chores").await.len(), 2);
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["wash the car:done"],
        "the lawn is dropped once and the car is done once"
    );
    story.finish().await;
}

/// **A batch that ticks one chore twice in a way that cannot stand is refused
/// before anything is written**, each tick judged against the list as the
/// ticks before it leave it.
#[tokio::test]
async fn a_batch_that_cannot_stand_is_refused_whole_naming_the_second_item() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (lawn, _car) = homer_chores(&s).await;
    let before = read_states(&s, "list:homer-chores").await;

    for second in ["dropped", "done"] {
        let refused = s
            .refused(
                "capture",
                json!({"subject": "list:homer-chores", "provenance": "testimony",
                       "ticks": [{"entry": lawn, "act": "dropped"},
                                 {"entry": lawn, "act": second}]}),
            )
            .await;
        refused.says("item 2").says("\"wrote\":false");
        assert!(
            ticks_of(&s, "list:homer-chores").await.is_empty(),
            "dropped then {second} wrote nothing"
        );
        assert_eq!(read_states(&s, "list:homer-chores").await, before);
    }

    // A tick that names an entry of another list is no entry of this one.
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "marge-errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "sink"}}),
    )
    .await;
    let elsewhere = add_entry(&s, "list:marge-errands", "post the letter", None).await;
    let refused = s
        .refused(
            "capture",
            json!({"subject": "list:homer-chores", "provenance": "testimony",
                   "ticks": [{"entry": elsewhere, "act": "done"}]}),
        )
        .await;
    refused.says("item 1").says("\"wrote\":false");
    assert!(ticks_of(&s, "list:homer-chores").await.is_empty());
    story.finish().await;
}

/// **Ticks that only stand in order land in order.** A chore dropped, brought
/// back and done is three ticks that the list as it was refuses in part and the
/// list as the batch leaves it accepts; and done then undone on a checklist is
/// judged in order too, so it lands and leaves the chore open.
#[tokio::test]
async fn ticks_are_judged_in_order_so_a_drop_undone_and_a_done_undone_both_land() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (lawn, car) = homer_chores(&s).await;
    s.call(
        "capture",
        json!({"subject": "list:homer-chores", "content": "not the lawn",
               "provenance": "testimony", "tick": {"entry": lawn, "act": "dropped"}}),
    )
    .await;

    s.call(
        "capture",
        json!({"subject": "list:homer-chores", "provenance": "testimony",
               "ticks": [{"entry": lawn, "act": "undone"},
                         {"entry": lawn, "act": "done"}]}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "list:homer-chores", "provenance": "testimony",
               "ticks": [{"entry": car, "act": "done"},
                         {"entry": car, "act": "undone"}]}),
    )
    .await;
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["wash the car:open", "mow the lawn:done"]
    );
    assert_eq!(ticks_of(&s, "list:homer-chores").await.len(), 5);
    story.finish().await;
}

/// **A batch with no `request_id` that stops partway says what landed and how to
/// finish without writing it twice.** The third entry's edge points at a thing that
/// does not exist yet, which the store judges at the write, so the first two land.
/// The answer names both, does not say nothing was written, and does not advise
/// sending the same call again, which would write them a second time. Sending the
/// chore that failed alone lands it, and the list holds each chore once.
#[tokio::test]
async fn a_batch_that_half_landed_without_an_id_says_what_landed_and_how_to_finish() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "homer-chores", json!({"tick": "sink"})).await;
    let stopped = s
        .refused(
            "capture",
            json!({"subject": "list:homer-chores", "provenance": "testimony",
                   "entries": ["mow the lawn", "wash the car",
                               {"content": "fix the kettle", "shape": "about",
                                "object": "thing:kettle"}]}),
        )
        .await;
    stopped
        .says("item 3")
        .says("mow the lawn")
        .says("wash the car");
    let body = stopped.json();
    let how = body["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        !how.contains("Nothing was written"),
        "two chores landed: {how}"
    );
    assert!(
        how.contains("again would write") && how.contains("only"),
        "it says a resend writes them twice and names what to send instead: {how}"
    );
    assert_eq!(body["wrote"], true, "{body}");
    assert_eq!(
        body["landed"].as_array().map(Vec::len),
        Some(2),
        "the two addresses that landed: {body}"
    );
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        ["mow the lawn:open", "wash the car:open"]
    );

    s.add("thing:kettle", "The Kettle").await;
    s.call(
        "capture",
        json!({"subject": "list:homer-chores", "provenance": "testimony",
               "entries": [{"content": "fix the kettle", "shape": "about",
                            "object": "thing:kettle"}]}),
    )
    .await;
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        [
            "mow the lawn:open",
            "wash the car:open",
            "fix the kettle:open"
        ],
        "each chore once"
    );
    story.finish().await;
}

/// "I sent the packing list a capture with nothing to put on it. What do I send?"
///
/// **A capture with no words on a list says the four ways to write to one**: `content`
/// for one entry, `entries` for several, `tick` to tick one and `ticks` to tick several.
/// The refusal writes nothing, and the entry it names lands when the call is made as it
/// says. The pair: the same empty capture on a thing that is not a list gets the plain
/// refusal and none of those arguments.
#[tokio::test]
async fn a_capture_with_no_words_on_a_list_names_the_ways_to_write_to_one() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.add("thing:kettle", "The Kettle").await;

    let on_a_list = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony"}),
        )
        .await;
    for way in ["`content`", "`entries`", "`tick`", "`ticks`"] {
        on_a_list.says(way);
    }
    assert!(
        read_states(&s, "list:packing").await.is_empty(),
        "the refusal wrote nothing"
    );

    // The pair: not a list, so the refusal does not talk of entries and ticks.
    let elsewhere = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony"}),
        )
        .await;
    elsewhere.says("empty");
    elsewhere.never_says("`entries`");

    // The way named works: one entry by `content`.
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "tent", "provenance": "testimony"}),
    )
    .await;
    assert_eq!(read_states(&s, "list:packing").await, ["tent:open"]);
    story.finish().await;
}

/// **A batch with a `request_id` that half-landed says it wrote, and the answer
/// agrees with the one a batch with no id gets, field for field.** The third
/// chore's write fails once. The answer says `wrote` true, lists the first two
/// with their words and addresses, and tells the caller to send the same call
/// again; sending it writes the third alone. The same shape of batch with no id
/// carries the same `wrote` and the same keys on each landed item.
#[tokio::test]
async fn a_half_landed_batch_with_an_id_says_it_wrote_and_agrees_with_the_one_without() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "homer-chores", json!({"tick": "sink"})).await;
    let call = json!({"subject": "list:homer-chores", "provenance": "testimony",
                      "request_id": "r1",
                      "entries": ["mow the lawn", "wash the car", "fix the kettle"]});

    story.fails_the_next_commit_of("item-2");
    let stopped = s.refused("capture", call.clone()).await;
    stopped
        .says("item 3")
        .says("mow the lawn")
        .says("wash the car");
    let with_id = stopped.json();
    assert_eq!(with_id["wrote"], true, "two chores landed: {with_id}");
    assert_eq!(with_id["fix_by"], "retry", "{with_id}");
    let how = with_id["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains("same call again") && how.contains("request_id"),
        "it tells the caller to send the same call again with its id: {how}"
    );
    assert_eq!(with_id["landed"].as_array().map(Vec::len), Some(2));

    let finished = s.call("capture", call).await;
    assert_eq!(
        finished.json()["finished_a_cut_batch"]["already_written"],
        2,
        "{}",
        finished.raw()
    );
    assert_eq!(
        read_states(&s, "list:homer-chores").await,
        [
            "mow the lawn:open",
            "wash the car:open",
            "fix the kettle:open"
        ],
        "the third chore alone was written by the resend"
    );

    // The same batch with no id: the edge to a thing that does not exist fails
    // the third write.
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "marge-errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "sink"}}),
    )
    .await;
    let without_id = s
        .refused(
            "capture",
            json!({"subject": "list:marge-errands", "provenance": "testimony",
                   "entries": ["post the letter", "buy stamps",
                               {"content": "fix the kettle", "shape": "about",
                                "object": "thing:kettle"}]}),
        )
        .await
        .json();
    assert_eq!(without_id["wrote"], with_id["wrote"], "{without_id}");
    let shape = |body: &Value| -> Vec<(u64, Vec<String>)> {
        body["landed"]
            .as_array()
            .expect("a list of what landed")
            .iter()
            .map(|item| {
                (
                    item["position"].as_u64().expect("a position"),
                    item.as_object()
                        .expect("an object")
                        .keys()
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    };
    assert_eq!(
        shape(&without_id),
        shape(&with_id),
        "the landed list has the same positions and keys on both paths"
    );
    story.finish().await;
}
