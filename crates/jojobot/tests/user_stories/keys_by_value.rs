//! "Which chores are in the kitchen, which are due by Friday, and what is
//! first?"
//!
//! **A question over the values things hold is answered from the values the
//! server keeps by key, and the answer is the one reading every chore gives.**
//! The board is four chores. Each case asks one shape a caller sends — a list
//! that holds an item, a `!=`, a range of dates, a range of numbers, an order —
//! and names the chores it must come back with, written out by hand from what
//! the board holds. A value that is no value of its key's type (a size that is a
//! word, a day that is not a day) is in no range and last in every order, as it
//! has always been.
//!
//! Then the board is edited, one kind of write after another, and the same
//! questions are asked again: an answer that followed a write is proof that the
//! values the server keeps by key followed it.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The chores board and the declaration that makes `due` a date, `size` a number
/// and `touches` a list. The shelf holds a word where a number goes and a word
/// where a day goes; the porch holds neither.
async fn a_board(s: &Session) -> Vec<String> {
    s.call(
        "declare_type",
        json!({
            "name": "thing",
            "fields": [
                { "key": "due", "holds": "date" },
                { "key": "size", "holds": "number" },
                { "key": "touches", "holds": "list:text" },
            ],
        }),
    )
    .await
    .says("\"name\":\"thing\"");
    s.add("person:milhouse", "Milhouse").await;
    for (handle, name) in [
        ("thing:fix-the-gate", "Fix the gate"),
        ("thing:paint-the-fence", "Paint the fence"),
        ("thing:sweep-the-porch", "Sweep the porch"),
        ("thing:sort-the-shelf", "Sort the shelf"),
    ] {
        s.add(handle, name).await;
    }
    let mut records = Vec::new();
    for (handle, held) in [
        (
            "thing:fix-the-gate",
            json!({ "status": "now", "size": "10", "due": "2026-10-12",
                    "touches": "hammer, ladder", "owner": "person:milhouse" }),
        ),
        (
            "thing:paint-the-fence",
            json!({ "status": "next", "size": "2", "due": "2026-11-20",
                    "touches": "hammer", "owner": "person:milhouse" }),
        ),
        (
            "thing:sweep-the-porch",
            json!({ "status": "done", "due": "2026-10-01" }),
        ),
        (
            "thing:sort-the-shelf",
            json!({ "status": "now", "size": "big", "due": "someday" }),
        ),
    ] {
        records.push(s.event_with(handle, "where it stands", held, &[]).await);
    }
    records
}

/// The handles an answer named, in answer order.
fn handles_of(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", answer.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

/// What a `where` over the chores names, in answer order.
async fn ask(s: &Session, line: &str) -> Vec<String> {
    let answer = s
        .call("recall", json!({ "kind": "thing", "where": line }))
        .await;
    handles_of(&answer)
}

/// What a `where` names, in a stable order, for the shapes that have none to
/// check.
async fn ask_sorted(s: &Session, line: &str) -> Vec<String> {
    let mut names = ask(s, line).await;
    names.sort();
    names
}

/// The chores in order of a key.
async fn in_order(s: &Session, key: &str, direction: &str) -> Vec<String> {
    let answer = s
        .call(
            "recall",
            json!({ "kind": "thing", "order": { "key": key, "direction": direction } }),
        )
        .await;
    handles_of(&answer)
}

/// The names, in the order a sorted answer lists them.
fn sorted<const N: usize>(names: [&str; N]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

const GATE: &str = "thing:fix-the-gate";
const FENCE: &str = "thing:paint-the-fence";
const PORCH: &str = "thing:sweep-the-porch";
const SHELF: &str = "thing:sort-the-shelf";

/// **Each shape answers with the chores it keeps and no other.** A list holds a
/// whole item and never a prefix of one; `!=` asks that the key is held; a range
/// leaves a word out; the order puts a word last.
#[tokio::test]
async fn a_question_over_values_names_the_chores_that_hold_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;

    assert_eq!(
        ask_sorted(&s, "touches contains hammer").await,
        sorted([FENCE, GATE])
    );
    assert_eq!(
        ask_sorted(&s, "touches contains ladder").await,
        sorted([GATE])
    );
    assert!(
        ask(&s, "touches contains ham").await.is_empty(),
        "a prefix of an item is not an item"
    );

    assert_eq!(
        ask_sorted(&s, "status != done").await,
        sorted([FENCE, GATE, SHELF])
    );
    assert_eq!(
        ask_sorted(&s, "status != now").await,
        sorted([FENCE, PORCH])
    );
    assert_eq!(
        ask_sorted(&s, "status = now && touches contains hammer").await,
        sorted([GATE]),
        "an equality and a list together"
    );

    assert_eq!(
        ask_sorted(&s, "due <= 2026-10-12").await,
        sorted([GATE, PORCH])
    );
    assert_eq!(
        ask(&s, "due > 2026-10-12").await,
        [FENCE],
        "the shelf's word is in no range of days"
    );
    assert_eq!(ask_sorted(&s, "size >= 2").await, sorted([FENCE, GATE]));
    assert_eq!(
        ask(&s, "size < 5").await,
        [FENCE],
        "a word is in no range of numbers, and the porch holds none"
    );

    assert_eq!(
        in_order(&s, "due", "ascending").await,
        [PORCH, GATE, FENCE, SHELF],
        "smallest first, the word last"
    );
    assert_eq!(
        in_order(&s, "due", "descending").await,
        [FENCE, GATE, PORCH, SHELF],
        "largest first, and the word is still last"
    );
    let by_size = in_order(&s, "size", "ascending").await;
    assert_eq!(by_size[..2], [FENCE, GATE]);
    let mut unplaced = by_size[2..].to_vec();
    unplaced.sort();
    assert_eq!(
        unplaced,
        sorted([PORCH, SHELF]),
        "a word and nothing come last"
    );

    let filtered = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "status != done",
                    "order": { "key": "due", "direction": "descending" } }),
        )
        .await;
    assert_eq!(
        handles_of(&filtered),
        [FENCE, GATE, SHELF],
        "a filter and an order together"
    );
    story.finish().await;
}

/// **A question asked after a write is answered for the chores as they are
/// now.** Each kind of write moves what the server keeps by key, and the same
/// questions are asked again.
#[tokio::test]
async fn an_answer_follows_every_kind_of_write_to_a_chore() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    let records = a_board(&s).await;
    let shelf_record = &records[3];

    // A list gains an item, and a second, and loses the first.
    assert!(ask(&s, "touches contains kitchen").await.is_empty());
    s.call(
        "capture",
        json!({"subject": PORCH, "content": "the porch is a kitchen chore too",
               "provenance": "testimony", "sets_add": {"touches": "kitchen"}}),
    )
    .await;
    assert_eq!(ask(&s, "touches contains kitchen").await, [PORCH]);
    s.call(
        "capture",
        json!({"subject": PORCH, "content": "and the garden",
               "provenance": "testimony", "sets_add": {"touches": "garden"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": PORCH, "content": "no longer the kitchen",
               "provenance": "testimony", "sets_remove": {"touches": "kitchen"}}),
    )
    .await;
    assert!(
        ask(&s, "touches contains kitchen").await.is_empty(),
        "an item taken off is no longer found"
    );
    assert_eq!(
        ask(&s, "touches contains garden").await,
        [PORCH],
        "and the item that stayed is"
    );

    // A day is moved by an edit: it leaves one range and enters another.
    s.call(
        "update_fact",
        json!({"address": shelf_record, "sets": {"due": "2026-10-20", "size": "7"}}),
    )
    .await;
    assert_eq!(
        ask_sorted(&s, "due > 2026-10-12").await,
        sorted([FENCE, SHELF])
    );
    assert_eq!(ask_sorted(&s, "size >= 7").await, sorted([GATE, SHELF]));
    assert_eq!(
        in_order(&s, "due", "ascending").await,
        [PORCH, GATE, SHELF, FENCE],
        "the shelf has a place in the order now"
    );

    // A chore set aside is in no answer, and back when it is restored.
    s.call(
        "archive_entity",
        json!({"handle": GATE, "reason": "done in another way"}),
    )
    .await;
    assert_eq!(ask(&s, "touches contains hammer").await, [FENCE]);
    assert_eq!(
        ask_sorted(&s, "status != done").await,
        sorted([FENCE, SHELF])
    );
    assert_eq!(
        in_order(&s, "due", "ascending").await,
        [PORCH, SHELF, FENCE]
    );
    // The answer says what it left out: the gate matches both questions and is
    // set aside, so each says one archived chore was left out and how to read it.
    // The third question has a term the keys cannot answer (a negation), so it is
    // answered by reading the candidates rather than by the keys alone.
    for line in [
        "touches contains hammer",
        "status != done",
        "status != done && !(size = 2)",
    ] {
        let answer = s
            .call("recall", json!({ "kind": "thing", "where": line }))
            .await;
        assert_eq!(
            answer.json()["archived_excluded"],
            1,
            "{line}: {}",
            answer.raw()
        );
        assert!(
            answer.json()["excluded_note"].is_string(),
            "{line}: the answer names how to read what it left out: {}",
            answer.raw()
        );
        assert_eq!(
            answer.json()["merged_excluded"],
            0,
            "{line}: {}",
            answer.raw()
        );
    }
    s.call(
        "archive_entity",
        json!({"handle": GATE, "restore": true, "reason": "not done after all"}),
    )
    .await;
    let restored = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "touches contains hammer" }),
        )
        .await;
    assert_eq!(
        restored.json()["archived_excluded"],
        0,
        "{}",
        restored.raw()
    );
    assert!(
        restored.json().get("excluded_note").is_none(),
        "{}",
        restored.raw()
    );
    assert_eq!(
        ask_sorted(&s, "touches contains hammer").await,
        sorted([FENCE, GATE])
    );

    // A record taken back gives the older value back.
    s.call(
        "capture",
        json!({"subject": GATE, "content": "pushed to the end of the month",
               "provenance": "testimony", "sets": {"due": "2026-10-30"}}),
    )
    .await;
    assert_eq!(
        ask_sorted(&s, "due > 2026-10-25").await,
        sorted([FENCE, GATE])
    );
    let pushed = s
        .call("recall", json!({"subject": GATE, "facts": true}))
        .await;
    let address = pushed.json()["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"] == "pushed to the end of the month")
        })
        .and_then(|fact| fact["address"].as_str().map(str::to_string))
        .unwrap_or_else(|| panic!("the push is a record: {}", pushed.raw()));
    s.call("retract", json!({"address": address, "reason": "not so"}))
        .await;
    assert_eq!(
        ask(&s, "due > 2026-10-25").await,
        [FENCE],
        "with the push taken back the gate is due on the day it was"
    );
    assert_eq!(ask_sorted(&s, "due = 2026-10-12").await, sorted([GATE]));
    story.finish().await;
}

/// **A leaf that names a handle is answered for the handle a reader sees.** The
/// owner is stored as the thing's permanent id and read back as its handle, so
/// asking who is not Milhouse's must compare the handle.
#[tokio::test]
async fn a_leaf_naming_a_handle_is_answered_for_the_handle_a_reader_sees() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    assert_eq!(
        ask_sorted(&s, "owner != person:milhouse").await,
        Vec::<String>::new(),
        "both chores that have an owner have Milhouse"
    );
    s.add("person:apu", "Apu").await;
    s.call(
        "capture",
        json!({"subject": SHELF, "content": "Apu has the shelf",
               "provenance": "testimony", "sets": {"owner": "person:apu"}}),
    )
    .await;
    assert_eq!(
        ask_sorted(&s, "owner != person:milhouse").await,
        sorted([SHELF])
    );
    assert_eq!(
        ask_sorted(&s, "owner != person:apu").await,
        sorted([FENCE, GATE])
    );
    story.finish().await;
}
