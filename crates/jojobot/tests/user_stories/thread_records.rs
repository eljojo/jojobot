//! "The moving thread keeps getting another note, and nobody ever retires an
//! old one. Can it just hold three?"
//!
//! A thread is bounded on purpose: it holds three live records unless it sets
//! its own number, and a write that would add one more is refused with nothing
//! written, so the writer has to choose what gives. **A thread over its number
//! still reads, edits and archives**, and the bots that write into it never raise
//! its number themselves.

use serde_json::json;

use super::dsl::{Session, Story};

const THREAD: &str = "thread:milhouse-moves";

async fn note(s: &Session, content: &str) -> String {
    s.call(
        "capture",
        json!({"subject": THREAD, "content": content, "provenance": "testimony"}),
    )
    .await
    .field("address")
}

#[tokio::test]
async fn a_fourth_live_record_on_a_thread_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(THREAD, "Milhouse moves").await;

    // ── three land ───────────────────────────────────────────────────────────
    let first = note(&s, "the first note on alpha").await;
    note(&s, "the second note on alpha").await;
    note(&s, "the third note on alpha").await;

    // ── the fourth is refused, with the count, the ceiling and the routes ────
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": THREAD, "content": "the fourth note on alpha",
                "provenance": "testimony",
            }),
        )
        .await;
    refused
        .says("\"wrote\":false")
        .number("/live", 3)
        .number("/ceiling", 3);
    let body = refused.json();
    assert_eq!(
        body["routes"],
        json!(["rewrite", "archive", "split", "document"]),
        "{body}"
    );
    // The refusal lists the live records, so choosing costs no second read.
    let listed: Vec<&str> = body["records"]
        .as_array()
        .expect("the refusal lists the live records")
        .iter()
        .filter_map(|record| record["content"].as_str())
        .collect();
    assert_eq!(
        listed,
        [
            "the first note on alpha",
            "the second note on alpha",
            "the third note on alpha"
        ],
        "{body}"
    );
    // Nothing was written: the fourth is nowhere on the thread.
    s.call(
        "recall",
        json!({"subject": THREAD, "facts": true, "status": "active"}),
    )
    .await
    .never_says("the fourth note on alpha")
    .says("the third note on alpha");

    // ── a rewrite lands at full, because it adds no record ───────────────────
    s.call(
        "update_fact",
        json!({"address": first, "content": "the first note on alpha, reworded"}),
    )
    .await;
    // The rewrite landed: the thread reads it, and still holds three.
    let reworded = s
        .call(
            "recall",
            json!({"subject": THREAD, "facts": true, "status": "active"}),
        )
        .await;
    reworded
        .says("the first note on alpha, reworded")
        .never_says("\"content\":\"the first note on alpha\"");
    assert_eq!(
        reworded.json()["objects"][0]["facts"]
            .as_array()
            .map(Vec::len),
        Some(3),
        "{}",
        reworded.raw()
    );

    // ── archiving one makes room, and the capture lands ──────────────────────
    s.call(
        "update_fact",
        json!({"address": first, "status": "archived", "details": "no longer earns its slot"}),
    )
    .await;
    note(&s, "the fourth note on alpha").await;
    s.refused(
        "capture",
        json!({"subject": THREAD, "content": "a fifth note", "provenance": "testimony"}),
    )
    .await
    .number("/live", 3);
    story.finish().await;
}

#[tokio::test]
async fn a_full_thread_borrows_one_record_once_and_archiving_repays_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(THREAD, "Milhouse moves").await;
    let first = note(&s, "the first note on alpha").await;
    note(&s, "the second note on alpha").await;
    note(&s, "the third note on alpha").await;

    // ── an explicit borrow admits one over the ceiling ───────────────────────
    s.call(
        "capture",
        json!({
            "subject": THREAD, "content": "the borrowed note on alpha",
            "provenance": "testimony", "borrow": true,
        }),
    )
    .await
    .says("emergency reserve");

    // ── and the second is refused, borrow or not ─────────────────────────────
    s.refused(
        "capture",
        json!({
            "subject": THREAD, "content": "a second borrowed note",
            "provenance": "testimony", "borrow": true,
        }),
    )
    .await
    .number("/live", 4)
    .number("/ceiling", 3);

    // ── a thread over its number still reads, edits and archives ─────────────
    s.call(
        "recall",
        json!({"subject": THREAD, "facts": true, "status": "active"}),
    )
    .await
    .says("the borrowed note on alpha");
    s.call(
        "update_fact",
        json!({"address": first, "content": "the first note on alpha, edited over the ceiling"}),
    )
    .await;
    // The edit stands on a read, and the thread still holds four.
    let over = s
        .call(
            "recall",
            json!({"subject": THREAD, "facts": true, "status": "active"}),
        )
        .await;
    over.says("the first note on alpha, edited over the ceiling");
    assert_eq!(
        over.json()["objects"][0]["facts"].as_array().map(Vec::len),
        Some(4),
        "{}",
        over.raw()
    );

    // ── archiving back to the ceiling repays it: the reserve is there again ──
    s.call(
        "update_fact",
        json!({"address": first, "status": "archived", "details": "repaying the borrow"}),
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": THREAD, "content": "borrowing again, once repaid",
            "provenance": "testimony", "borrow": true,
        }),
    )
    .await;

    // ── the whole thread, over its number, archives ──────────────────────────
    s.call("recall", json!({"kind": "thread"}))
        .await
        .says(THREAD);
    s.call(
        "archive_entity",
        json!({"handle": THREAD, "reason": "the line has ended"}),
    )
    .await;
    s.call("recall", json!({"kind": "thread"}))
        .await
        .never_says(THREAD);
    story.finish().await;
}

#[tokio::test]
async fn a_thread_that_sets_its_own_number_is_bounded_by_that_instead() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(THREAD, "Milhouse moves").await;
    let first = note(&s, "the first note on alpha").await;
    s.call(
        "update_fact",
        json!({"address": first, "sets": {"record_capacity": "5"}}),
    )
    .await;
    for n in 2..=5 {
        note(&s, &format!("note number {n} on alpha")).await;
    }
    s.refused(
        "capture",
        json!({"subject": THREAD, "content": "a sixth note", "provenance": "testimony"}),
    )
    .await
    .number("/live", 5)
    .number("/ceiling", 5);
    story.finish().await;
}

#[tokio::test]
async fn the_bots_that_write_into_a_thread_never_raise_its_number() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(THREAD, "Milhouse moves").await;
    for bot in ["marge", "patty", "ralph"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let marge = story.as_bot("bot:marge").await;
    let patty = story.as_bot("bot:patty").await;
    let ralph = story.as_bot("bot:ralph").await;
    marge
        .call(
            "capture",
            json!({
                "subject": "bot:patty", "content": "patty reports up",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;
    patty
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "ralph reports up",
                "provenance": "testimony", "sets": {"reports_to": "bot:patty"},
            }),
        )
        .await;
    let first = s
        .call(
            "capture",
            json!({
                "subject": THREAD, "content": "how many notes alpha holds",
                "sets": {"record_capacity": "4"},
            }),
        )
        .await
        .field("address");
    // ralph writes a thought into the thread, so the number now binds ralph.
    ralph
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "alpha is still on",
                "shape": "connection", "object": THREAD,
            }),
        )
        .await;

    // ── ralph raises it neither by an edit nor by clearing it ────────────────
    ralph
        .refused(
            "update_fact",
            json!({"address": first, "sets": {"record_capacity": "50"}}),
        )
        .await
        .says("record_capacity")
        .says("bot:patty")
        .says("\"wrote\":false");
    ralph
        .refused(
            "update_fact",
            json!({"address": first, "clear_fields": ["record_capacity"]}),
        )
        .await
        .says("bot:patty");
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"4\"");

    // ── a bot above a writer does ────────────────────────────────────────────
    patty
        .call(
            "update_fact",
            json!({"address": first, "sets": {"record_capacity": "6"}}),
        )
        .await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"6\"");
    story.finish().await;
}

/// **A ceiling somebody set is the ceiling they meant, or the write says why
/// not.** A number that is not a whole number of one or more used to be stored
/// and then read as no setting at all, so the thread stayed at its default with
/// nothing to say so.
#[tokio::test]
async fn a_ceiling_that_is_not_a_whole_number_of_one_or_more_is_refused_on_every_verb() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(THREAD, "Milhouse moves").await;
    let first = note(&s, "the first note on alpha").await;

    for bad in ["three", "0", "-2", "2.5"] {
        let sets = json!({"record_capacity": bad});
        let refusals = [
            s.refused(
                "capture",
                json!({"subject": THREAD, "content": "x", "provenance": "testimony", "sets": sets}),
            )
            .await,
            s.refused("update_fact", json!({"address": first, "sets": sets}))
                .await,
            s.refused("update_entity", json!({"handle": THREAD, "sets": sets}))
                .await,
        ];
        for refusal in &refusals {
            refusal
                .says("record_capacity")
                .says(&format!("'{bad}'"))
                .says("whole number of one or more");
        }
    }
    // Nothing was written by any refusal: the thread still holds its one note.
    let held = s
        .call("recall", json!({"subject": THREAD, "facts": true}))
        .await
        .json();
    let records = held["objects"][0]["facts"].as_array().expect("facts");
    assert_eq!(records.len(), 1, "{held}");

    // ── the positive: a whole number lands and bounds the thread ────────────
    s.call(
        "update_entity",
        json!({"handle": THREAD, "sets": {"record_capacity": "5"}}),
    )
    .await;
    for n in 3..=5 {
        note(&s, &format!("note number {n} on alpha")).await;
    }
    s.refused(
        "capture",
        json!({"subject": THREAD, "content": "a sixth note", "provenance": "testimony"}),
    )
    .await
    .number("/live", 5)
    .number("/ceiling", 5);
    story.finish().await;
}
