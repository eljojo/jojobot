//! "I keep a packing list for the camping trip, and I want it in order."
//!
//! **A list is an ordered set of short entries hung on the thing it is about.**
//! One kind serves a checklist, a queue and a rotation, and its `tick` setting
//! says which. A list may say which day or ISO week it covers. Entries are
//! claims on the list, placed by the server at the end, after a named entry or
//! at the front, and read back in that order.
//!
//! The roster names are the cast: a camping trip, a tent, a packing list.

use serde_json::json;

use super::dsl::{Session, Story};

/// A list on the camping trip, created with the settings given.
pub(super) async fn a_list_on_the_trip(s: &Session, handle: &str, sets: serde_json::Value) {
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": handle, "name": "Packing",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": sets}),
    )
    .await;
}

/// **A list takes its tick setting from the closed three, and a word outside
/// them is refused naming all three.** The paired positive is a list that lands.
#[tokio::test]
async fn a_list_takes_sink_leave_or_back_and_refuses_any_other_tick() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;

    for tick in ["sink", "leave", "back"] {
        s.call(
            "add_entity",
            json!({"kind": "list", "handle": format!("trip-{tick}"), "name": format!("Trip {tick}"),
                   "source": "user-named", "parent": "thing:springfield-camping",
                   "sets": {"tick": tick}}),
        )
        .await
        .says("list:");
    }

    let refused = s
        .refused(
            "add_entity",
            json!({"kind": "list", "handle": "mystery", "name": "Mystery",
                   "source": "user-named", "parent": "thing:springfield-camping",
                   "sets": {"tick": "vanish"}}),
        )
        .await;
    refused
        .says("sink")
        .says("leave")
        .says("back")
        .says("\"wrote\":false");
    story.finish().await;
}

/// **`covers` is one calendar day or one ISO week, and a refusal names both
/// forms.** The paired positives are a day and a week that land.
#[tokio::test]
async fn a_list_covers_a_day_or_an_iso_week_and_refuses_any_other_span() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;

    for (handle, covers) in [("saturday", "2026-10-19"), ("fortnight", "2026-W43")] {
        s.call(
            "add_entity",
            json!({"kind": "list", "handle": handle, "name": handle,
                   "source": "user-named", "parent": "thing:springfield-camping",
                   "sets": {"tick": "leave", "covers": covers}}),
        )
        .await
        .says("list:");
    }

    for bad in ["next week", "2026-10", "2026-W54"] {
        let refused = s
            .refused(
                "add_entity",
                json!({"kind": "list", "handle": "nonsense", "name": "Nonsense",
                       "source": "user-named", "parent": "thing:springfield-camping",
                       "sets": {"covers": bad}}),
            )
            .await;
        refused
            .says("YYYY-MM-DD")
            .says("YYYY-Www")
            .says("\"wrote\":false");
    }
    story.finish().await;
}

/// The texts of a list's open entries, in the order `recall` serves them.
pub(super) async fn read_back(s: &Session, list: &str) -> Vec<String> {
    let answer = s.call("recall", json!({"subject": list})).await;
    answer.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", answer.raw()))
        .iter()
        .map(|entry| entry["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// One entry captured on a list, with where it goes.
pub(super) async fn add_entry(s: &Session, list: &str, text: &str, after: Option<&str>) -> String {
    let mut args = json!({"subject": list, "content": text, "provenance": "testimony"});
    if let Some(after) = after {
        args["after"] = json!(after);
    }
    let answer = s.call("capture", args).await;
    answer.json()["address"]
        .as_str()
        .unwrap_or_else(|| panic!("a capture answers with the address: {}", answer.raw()))
        .to_string()
}

/// **A checklist reads back in the order its entries were added**, and an entry
/// placed after the first sits second. The empty list reads back empty first, so
/// the order is the entries' and not the read's.
#[tokio::test]
async fn entries_read_back_in_the_order_added_and_one_placed_after_the_first_sits_second() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    assert_eq!(read_back(&s, "list:packing").await, Vec::<String>::new());

    let tent = add_entry(&s, "list:packing", "tent poles", None).await;
    add_entry(&s, "list:packing", "sleeping bags", None).await;
    add_entry(&s, "list:packing", "stove", None).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["tent poles", "sleeping bags", "stove"]
    );

    add_entry(&s, "list:packing", "lantern", Some(&tent)).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["tent poles", "lantern", "sleeping bags", "stove"]
    );
    story.finish().await;
}

/// **`first` puts an entry at the front, and `after` the last entry puts it
/// at the end.** Each paired with the default, which is the end.
#[tokio::test]
async fn first_places_at_the_front_and_after_the_last_entry_places_at_the_end() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    add_entry(&s, "list:packing", "stove", None).await;
    let last = add_entry(&s, "list:packing", "matches", None).await;

    add_entry(&s, "list:packing", "map", Some("first")).await;
    add_entry(&s, "list:packing", "kettle", Some(&last)).await;
    add_entry(&s, "list:packing", "mugs", None).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["map", "stove", "matches", "kettle", "mugs"]
    );
    story.finish().await;
}

/// **An entry may be a claim with an edge to a thing, and it reads back with the
/// link.** Its neighbour, a plain text entry, reads back with none.
#[tokio::test]
async fn an_entry_with_an_edge_reads_back_with_the_link() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.add("work:bart-tent", "Bart's tent").await;
    add_entry(&s, "list:packing", "stove", None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "pitch the tent",
               "provenance": "testimony", "shape": "about", "object": "work:bart-tent"}),
    )
    .await;

    let answer = s.call("recall", json!({"subject": "list:packing"})).await;
    let entries = answer.json()["objects"][0]["entries"].clone();
    assert_eq!(entries[0]["text"], "stove");
    assert!(entries[0].get("link").is_none(), "{entries}");
    assert_eq!(entries[1]["text"], "pitch the tent");
    assert_eq!(entries[1]["link"]["object"], "work:bart-tent", "{entries}");
    assert_eq!(entries[1]["state"], "open");
    story.finish().await;
}

/// **`after` naming something that is not an open entry of this list is
/// refused, naming the entries there are, and nothing lands.** The positive is
/// the same call naming a real entry.
#[tokio::test]
async fn after_naming_a_stranger_is_refused_with_the_entries_and_writes_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;
    add_entry(&s, "list:packing", "lantern", Some(&stove)).await;

    s.refused(
        "capture",
        json!({"subject": "list:packing", "content": "kettle",
               "provenance": "testimony", "after": "list:packing#f99"}),
    )
    .await
    .says(&stove)
    .says("`first`")
    .says("`last`")
    .says("\"wrote\":false");
    assert_eq!(read_back(&s, "list:packing").await, ["stove", "lantern"]);
    story.finish().await;
}

/// **`after` on a subject that is not a list is refused naming the list kind.**
/// The paired positive is the same words captured with no `after`.
#[tokio::test]
async fn after_on_a_subject_that_is_not_a_list_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;

    s.refused(
        "capture",
        json!({"subject": "thing:springfield-camping", "content": "the trip is on",
               "provenance": "testimony", "after": "first"}),
    )
    .await
    .holds_at("/fix_by", "change")
    .says("\"wrote\":false");
    s.call(
        "capture",
        json!({"subject": "thing:springfield-camping", "content": "the trip is on",
               "provenance": "testimony"}),
    )
    .await
    .says("thing:springfield-camping");
    story.finish().await;
}

/// **A copied entry order is dropped; the server mints it.** With no other edit
/// the call names no change. The paired positive is an edit of the entry's words by
/// the session that wrote it, which lands and leaves the entry where it was.
#[tokio::test]
async fn a_caller_sending_an_order_key_on_an_entry_is_refused_and_an_edit_of_its_words_is_not() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;
    add_entry(&s, "list:packing", "lantern", None).await;

    s.refused(
        "update_fact",
        json!({"address": stove, "fields": {"order": "zzz"}}),
    )
    .await
    .says("keep")
    .says("\"wrote\":false");
    assert_eq!(read_back(&s, "list:packing").await, ["stove", "lantern"]);

    s.call(
        "update_fact",
        json!({"address": stove, "content": "camp stove"}),
    )
    .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["camp stove", "lantern"]
    );
    story.finish().await;
}

/// **An entry carries its own words only, in search.** The list's name is on the
/// list, which is found by it; an entry that holds nothing asked for is not a hit
/// because it sits on a list with that name. The paired positive is a search for
/// the entry's own words, which finds it.
#[tokio::test]
async fn an_entry_is_found_by_its_own_words_and_not_by_the_name_of_its_list() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Campsite kit",
               "source": "user-named", "parent": "thing:springfield-camping"}),
    )
    .await;
    let stove = add_entry(&s, "list:packing", "kerosene stove", None).await;

    let by_words = s.call("search", json!({"query": "kerosene"})).await;
    by_words.says(&stove);

    let by_list_name = s.call("search", json!({"query": "Campsite"})).await;
    by_list_name.says("list:packing");
    by_list_name.never_says(&stove);
    story.finish().await;
}

/// One entry added at the front of the packing list by this session.
async fn add_at_the_front(who: &Session, n: usize) {
    who.call(
        "capture",
        json!({"subject": "list:packing", "content": format!("item {n}"),
               "provenance": "testimony", "after": "first"}),
    )
    .await;
}

/// **Entries added at the same moment each get a place of their own.** The
/// server mints the keys under one lock per list, from one read of the
/// neighbours, so no two callers can mint the same key. Six sessions add to one
/// list at once; the list holds seven entries, and their keys are seven
/// different keys.
#[tokio::test]
async fn entries_added_at_the_same_moment_get_keys_of_their_own() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    add_entry(&s, "list:packing", "stove", None).await;

    let (a, b, c, d, e, f) = (
        story.session_in("UTC", Some("new")).await,
        story.session_in("UTC", Some("new")).await,
        story.session_in("UTC", Some("new")).await,
        story.session_in("UTC", Some("new")).await,
        story.session_in("UTC", Some("new")).await,
        story.session_in("UTC", Some("new")).await,
    );
    tokio::join!(
        add_at_the_front(&a, 0),
        add_at_the_front(&b, 1),
        add_at_the_front(&c, 2),
        add_at_the_front(&d, 3),
        add_at_the_front(&e, 4),
        add_at_the_front(&f, 5),
    );

    let read = s
        .call("recall", json!({"subject": "list:packing", "facts": true}))
        .await;
    let body = read.json();
    assert_eq!(
        body["objects"][0]["entries"].as_array().map(Vec::len),
        Some(7)
    );
    let keys: std::collections::BTreeSet<String> = body["objects"][0]["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .filter_map(|fact| fact["fields"]["order"].as_str().map(str::to_string))
        .collect();
    assert_eq!(keys.len(), 7, "every entry has a key of its own: {keys:?}");
    story.finish().await;
}

/// **Merging two lists keeps each list's order.** The folded list's entries
/// follow the survivor's in their own order, every entry is still the testimony
/// it was written as, and an entry added after a merged one lands after it.
#[tokio::test]
async fn two_lists_merged_keep_each_lists_order_and_every_entry_its_provenance() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "sink"}}),
    )
    .await;
    for text in ["tent", "stove", "lantern"] {
        add_entry(&s, "list:packing", text, None).await;
    }
    for text in ["letter", "bread", "bins"] {
        add_entry(&s, "list:errands", text, None).await;
    }
    assert_eq!(
        read_back(&s, "list:errands").await,
        ["letter", "bread", "bins"]
    );

    s.merge_entities("list:errands", "list:packing", "one list, not two", None)
        .await;

    assert_eq!(
        read_back(&s, "list:packing").await,
        ["tent", "stove", "lantern", "letter", "bread", "bins"]
    );
    let read = s
        .call("recall", json!({"subject": "list:packing", "facts": true}))
        .await;
    let facts = read.json()["objects"][0]["facts"].clone();
    for text in ["tent", "letter", "bins"] {
        let entry = facts
            .as_array()
            .expect("facts")
            .iter()
            .find(|fact| fact["content"] == text)
            .unwrap_or_else(|| panic!("{text} is still there: {facts}"));
        assert_eq!(entry["provenance"], "testimony", "{entry}");
    }

    let letter = facts
        .as_array()
        .expect("facts")
        .iter()
        .find(|fact| fact["content"] == "letter")
        .and_then(|fact| fact["address"].as_str())
        .expect("the letter has an address")
        .to_string();
    add_entry(&s, "list:packing", "kettle", Some(&letter)).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        [
            "tent", "stove", "lantern", "letter", "kettle", "bread", "bins"
        ]
    );
    story.finish().await;
}
