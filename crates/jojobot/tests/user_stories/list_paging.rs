//! "The packing list has four hundred things on it. Can I still read it?"
//!
//! **A list of any size reads within the answer ceiling and pages its entries.**
//! Reading a list that holds more entries than fit gives the first ones in
//! order, says how many it left out and names the `offset` that reads the next
//! part. Reading on never repeats an entry. How far along the list is, by its
//! ticks, counts every entry and not the page that was shown.
//!
//! The roster names are the cast: a camping trip and a packing list.

use jojobot_domain::text::ANSWER_CEILING;
use serde_json::json;

use super::dsl::{Answer, Session, Story};
use super::lists::{a_list_on_the_trip, add_entry};

/// The text of the nth entry: a number first, so order is readable, and enough
/// words after it that four hundred of them cannot fit one answer.
fn entry_text(n: usize) -> String {
    format!(
        "item {n:03}: the thing to pack for the trip, with a note on where it lives in the shed \
         and when it was last used"
    )
}

/// Four hundred entries, put on the list fifty to a call.
async fn put_down(s: &Session, list: &str, count: usize) {
    for first in (1..=count).step_by(50) {
        let texts: Vec<String> = (first..first + 50)
            .take_while(|n| *n <= count)
            .map(entry_text)
            .collect();
        s.call(
            "capture",
            json!({"subject": list, "provenance": "testimony", "entries": texts}),
        )
        .await;
    }
}

/// The entry texts an answer carries, in the order it carries them.
fn texts(read: &Answer) -> Vec<String> {
    read.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", read.raw()))
        .iter()
        .map(|entry| {
            entry["text"]
                .as_str()
                .expect("an entry has text")
                .to_string()
        })
        .collect()
}

/// The `offset` an answer names for reading on, or none when it left nothing out.
fn read_on_from(read: &Answer) -> Option<u64> {
    read.json()["not_shown"]["offset"].as_u64()
}

#[tokio::test]
async fn a_list_of_four_hundred_entries_reads_in_parts_that_fit_and_never_repeats_one() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    put_down(&s, "list:packing", 400).await;

    // ── the first part fits, starts at the front, and says what it left out ──
    let first = s.call("recall", json!({"subject": "list:packing"})).await;
    assert!(
        first.size() <= ANSWER_CEILING,
        "the answer is {} characters, over the ceiling of {ANSWER_CEILING}",
        first.size()
    );
    let shown = texts(&first);
    assert!(
        !shown.is_empty() && shown.len() < 400,
        "some entries lead, not all: {}",
        shown.len()
    );
    assert_eq!(shown[0], entry_text(1), "the first entry leads");
    first.number("/not_shown/count", (400 - shown.len()) as i64);
    assert_eq!(read_on_from(&first), Some(shown.len() as u64));

    // ── reading on returns the next entries, each entry once, in order ───────
    let mut all = shown;
    let mut next = read_on_from(&first);
    let mut parts = 1;
    while let Some(offset) = next {
        let part = s
            .call(
                "recall",
                json!({"subject": "list:packing", "offset": offset}),
            )
            .await;
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts} is over the ceiling"
        );
        let more = texts(&part);
        assert!(
            !more.is_empty(),
            "a part that names more to read holds some"
        );
        all.extend(more);
        next = read_on_from(&part);
        parts += 1;
        assert!(parts < 50, "paging ends");
    }
    let expected: Vec<String> = (1..=400).map(entry_text).collect();
    assert_eq!(all, expected, "every entry once and in order");
    story.finish().await;
}

#[tokio::test]
async fn how_far_along_a_long_list_is_counts_every_entry_and_not_the_page_shown() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    put_down(&s, "list:packing", 399).await;
    // The last entry is not on the first page.
    let address = add_entry(&s, "list:packing", &entry_text(400), None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "packed it", "provenance": "testimony",
               "tick": {"entry": address, "act": "done"}}),
    )
    .await;

    let first = s.call("recall", json!({"subject": "list:packing"})).await;
    let ticked = first.json()["objects"][0]["ticked"].clone();
    assert_eq!(ticked["of"], 400, "the count is over every entry: {ticked}");
    assert_eq!(
        ticked["done"], 1,
        "the tick on an unshown entry counts: {ticked}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_list_of_five_reads_whole_with_nothing_left_out() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    for n in 1..=5 {
        add_entry(&s, "list:packing", &entry_text(n), None).await;
    }
    let read = s.call("recall", json!({"subject": "list:packing"})).await;
    assert_eq!(texts(&read), (1..=5).map(entry_text).collect::<Vec<_>>());
    assert!(
        read.json().get("not_shown").is_none(),
        "nothing was left out: {}",
        read.raw()
    );
    story.finish().await;
}
