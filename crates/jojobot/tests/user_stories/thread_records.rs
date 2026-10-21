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
    let first = note(&s, "the first quote from the movers").await;
    note(&s, "the second quote from the movers").await;
    note(&s, "the third quote from the movers").await;

    // ── the fourth is refused, with the count, the ceiling and the routes ────
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": THREAD, "content": "the fourth quote from the movers",
                "provenance": "testimony",
            }),
        )
        .await;
    refused
        .says("\"wrote\":false")
        .number("/live", 3)
        .number("/ceiling", 3);
    let body = refused.json();
    // A thread exactly full and a write that adds one record: the refusal also
    // names the reserve and who raises the ceiling.
    let way = body["how_to_proceed"].as_str().expect("a way forward");
    assert!(way.contains("borrow: true"), "{body}");
    assert!(way.contains("record_capacity"), "{body}");
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
            "the first quote from the movers",
            "the second quote from the movers",
            "the third quote from the movers"
        ],
        "{body}"
    );
    // Nothing was written: the fourth is nowhere on the thread.
    s.call(
        "recall",
        json!({"subject": THREAD, "facts": true, "status": "active"}),
    )
    .await
    .never_says("the fourth quote from the movers")
    .says("the third quote from the movers");

    // ── a rewrite lands at full, because it adds no record ───────────────────
    s.call(
        "update_fact",
        json!({"address": first, "content": "the first quote from the movers, reworded"}),
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
        .says("the first quote from the movers, reworded")
        .never_says("\"content\":\"the first quote from the movers\"");
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
    note(&s, "the fourth quote from the movers").await;
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
    let first = note(&s, "the first quote from the movers").await;
    note(&s, "the second quote from the movers").await;
    note(&s, "the third quote from the movers").await;

    // ── an explicit borrow admits one over the ceiling ───────────────────────
    s.call(
        "capture",
        json!({
            "subject": THREAD, "content": "the borrowed quote from the movers",
            "provenance": "testimony", "borrow": true,
        }),
    )
    .await;

    // ── and the second is refused, borrow or not ─────────────────────────────
    let again = s
        .refused(
            "capture",
            json!({
                "subject": THREAD, "content": "a second borrowed note",
                "provenance": "testimony", "borrow": true,
            }),
        )
        .await;
    again.number("/live", 4).number("/ceiling", 3);
    // The reserve is spent, so the way forward does not offer it again.
    assert!(
        !again.json()["how_to_proceed"]
            .as_str()
            .expect("a way forward")
            .contains("borrow: true"),
        "{}",
        again.raw()
    );

    // ── a thread over its number still reads, edits and archives ─────────────
    s.call(
        "recall",
        json!({"subject": THREAD, "facts": true, "status": "active"}),
    )
    .await
    .says("the borrowed quote from the movers");
    s.call(
        "update_fact",
        json!({"address": first, "content": "the first quote from the movers, edited over the ceiling"}),
    )
    .await;
    // The edit stands on a read, and the thread still holds four.
    let over = s
        .call(
            "recall",
            json!({"subject": THREAD, "facts": true, "status": "active"}),
        )
        .await;
    over.says("the first quote from the movers, edited over the ceiling");
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
    // The record that sets the number is not a note filling the thread, so the
    // bot that sets it is not bound by it.
    s.call(
        "capture",
        json!({
            "subject": THREAD, "content": "how many quotes the move holds",
            "provenance": "testimony", "sets": {"record_capacity": "5"},
        }),
    )
    .await;
    for n in 2..=5 {
        note(&s, &format!("quote number {n} from the movers")).await;
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
                "subject": THREAD, "content": "how many quotes the move holds",
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
                "subject": "bot:ralph", "content": "the move is still on",
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
    // The record that sets the number is not a note filling the thread, so the
    // bot that wrote it may set the number again.
    let first = s
        .call(
            "capture",
            json!({
                "subject": THREAD, "content": "the first quote from the movers",
                "provenance": "testimony", "sets": {"record_capacity": "4"},
            }),
        )
        .await
        .field("address");

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
        // The refusal quotes the value it judged, so each of the four bad
        // spellings is told apart from the others by its own refusal.
        for refusal in &refusals {
            refusal
                .says(&format!("'{bad}' is not one"))
                .holds_at("/fix_by", "change");
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
        note(&s, &format!("quote number {n} from the movers")).await;
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

/// Notes on a thread, the first of which sets the thread's own number.
async fn a_thread_with_a_number(
    s: &Session,
    writer: &Session,
    handle: &str,
    number: u32,
    notes: u32,
) {
    s.add(handle, handle).await;
    s.call(
        "capture",
        json!({
            "subject": handle, "content": format!("how many notes {handle} holds"),
            "sets": {"record_capacity": number.to_string()},
        }),
    )
    .await;
    for n in 2..=notes {
        writer
            .call(
                "capture",
                json!({"subject": handle, "content": format!("note {n} on {handle}")}),
            )
            .await;
    }
}

/// **A merge is judged against the ceiling the merged thing will hold.** The
/// merged thing folds to one of the two numbers, whichever was set last, so a
/// merge that fits one and not the other would leave a thread over its number
/// with no way to write to it.
#[tokio::test]
async fn a_merge_is_held_to_the_lower_of_the_two_numbers() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    // otto sets the numbers and merges; marge writes the notes, and reports to
    // otto, so he is above her and is not one of the bots that fill the threads.
    s.add("bot:marge", "Marge").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:marge", "content": "marge reports up",
            "provenance": "testimony", "sets": {"reports_to": "bot:otto"},
        }),
    )
    .await;
    let marge = story.as_bot("bot:marge").await;
    // The duplicate's lower number is set FIRST and the survivor's higher one
    // LAST, so a merge that took the number set last would take the survivor's.
    a_thread_with_a_number(&s, &marge, THREAD, 3, 3).await;
    a_thread_with_a_number(&s, &marge, "thread:simpson-kitchen", 20, 10).await;

    // ── 13 records fit the survivor's 20 and not the duplicate's 3 ───────────
    let merge = json!({
        "duplicate": THREAD, "survivor": "thread:simpson-kitchen",
        "reason": "one line under two names",
    });
    let (_, survivor_held, _) = held(&s, "thread:simpson-kitchen").await;
    let (_, duplicate_held, _) = held(&s, THREAD).await;
    s.refused("merge_entities", merge.clone())
        .await
        .number("/live", 10)
        .number("/adding", 3)
        .number("/ceiling", 3);
    // Nothing moved: each thing answers at its own handle with its own number
    // and the records it held. A thing folded into another would answer as it.
    let (id, count, fields) = held(&s, THREAD).await;
    assert_eq!((id.as_str(), count), (THREAD, duplicate_held), "{fields}");
    assert_eq!(fields["record_capacity"], "3", "{fields}");
    let (id, count, fields) = held(&s, "thread:simpson-kitchen").await;
    assert_eq!(
        (id.as_str(), count),
        ("thread:simpson-kitchen", survivor_held),
        "{fields}"
    );
    assert_eq!(fields["record_capacity"], "20", "{fields}");

    // ── the other way round: the duplicate's higher number set LAST does not ─
    // raise the survivor's lower one. Taking the duplicate's number whenever it
    // differs, or the one set last, lets these ten records in; the lower holds.
    a_thread_with_a_number(&s, &marge, "thread:alpha", 3, 3).await;
    a_thread_with_a_number(&s, &marge, "thread:krusty-burger", 20, 10).await;
    s.refused(
        "merge_entities",
        json!({
            "duplicate": "thread:krusty-burger", "survivor": "thread:alpha",
            "reason": "one line under two names",
        }),
    )
    .await
    .number("/live", 3)
    .number("/adding", 10)
    .number("/ceiling", 3);
    let (id, _, fields) = held(&s, "thread:krusty-burger").await;
    assert_eq!(id, "thread:krusty-burger", "{fields}");
    assert_eq!(fields["record_capacity"], "20", "{fields}");

    // ── the positive: when the lower number holds them all, the merge lands ──
    a_thread_with_a_number(&s, &marge, "thread:krusty-telethon", 6, 2).await;
    a_thread_with_a_number(&s, &marge, "thread:forwarded-line", 9, 2).await;
    s.call(
        "merge_entities",
        json!({
            "duplicate": "thread:forwarded-line", "survivor": "thread:krusty-telethon",
            "reason": "one line under two names",
        }),
    )
    .await;
    story.finish().await;
}

/// **A container's number is raised by somebody above the writers filling it,
/// never by one of them.** The bot that wrote a container's live records is
/// among its writers whether the container is a thread or any other thing, and
/// whether or not it also pointed a thought at it. Whoever set the number is not
/// a writer for having set it.
#[tokio::test]
async fn the_bot_that_fills_a_container_does_not_raise_its_number() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let marge = story.as_bot("bot:marge").await;
    // marge reports to otto, so otto is above marge.
    s.call(
        "capture",
        json!({
            "subject": "bot:marge", "content": "marge reports up",
            "provenance": "testimony", "sets": {"reports_to": "bot:otto"},
        }),
    )
    .await;
    for (handle, name) in [
        ("topic:marge-baking", "Marge baking"),
        (THREAD, "Milhouse moves"),
    ] {
        s.add(handle, name).await;
        // ── otto sets the number; marge writes the other two records ─────────
        let number = s
            .call(
                "capture",
                json!({
                    "subject": handle, "content": format!("how many notes {name} holds"),
                    "sets": {"record_capacity": "3"},
                }),
            )
            .await
            .field("address");
        for n in 2..=3 {
            marge
                .call(
                    "capture",
                    json!({"subject": handle, "content": format!("note {n} on {name}")}),
                )
                .await;
        }

        // ── marge filled it, so marge raises it by neither verb ──────────────
        marge
            .refused(
                "update_fact",
                json!({"address": number, "sets": {"record_capacity": "50"}}),
            )
            .await
            .says("bot:otto")
            .says("\"wrote\":false");
        marge
            .refused(
                "update_entity",
                json!({"handle": handle, "sets": {"record_capacity": "50"}}),
            )
            .await
            .says("bot:otto");
        s.call("recall", json!({"subject": handle}))
            .await
            .says("\"record_capacity\":\"3\"");

        // ── the positive: otto, above marge and not a filler, does ─────────────
        s.call(
            "update_fact",
            json!({"address": number, "sets": {"record_capacity": "6"}}),
        )
        .await;
        s.call("recall", json!({"subject": handle}))
            .await
            .says("\"record_capacity\":\"6\"");
    }
    story.finish().await;
}

/// **A merge into a thread that a bot of the thread's own refuses names the bots
/// that may make it.** The thread's number is raised by somebody above the bots
/// that write into it, so a merge that carries a number onto it is held to the
/// same rule. The refusal for it names those bots and the way on, and never says
/// that any different identity would do: marge is a different identity from the
/// thread, and is refused all the same.
#[tokio::test]
async fn a_thread_merge_refused_for_its_writers_names_who_may_make_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let marge = story.as_bot("bot:marge").await;
    // marge reports to otto, so otto is above marge.
    s.call(
        "capture",
        json!({
            "subject": "bot:marge", "content": "marge reports up",
            "provenance": "testimony", "sets": {"reports_to": "bot:otto"},
        }),
    )
    .await;
    // marge keeps a thought about the survivor, so she is one of its writers.
    s.add(THREAD, "Milhouse moves").await;
    marge
        .call(
            "capture",
            json!({
                "subject": "bot:marge", "content": "marge follows the move",
                "shape": "connection", "object": THREAD,
            }),
        )
        .await;
    // The duplicate holds a number, set by otto.
    s.add("thread:simpson-kitchen", "Simpson kitchen").await;
    s.call(
        "capture",
        json!({
            "subject": "thread:simpson-kitchen", "content": "how many notes it holds",
            "sets": {"record_capacity": "5"},
        }),
    )
    .await;
    let merge = json!({
        "duplicate": "thread:simpson-kitchen", "survivor": THREAD,
        "reason": "one line under two names",
    });
    let (_, survivor_held, _) = held(&s, THREAD).await;
    let (_, duplicate_held, _) = held(&s, "thread:simpson-kitchen").await;

    // ── marge is refused, and the refusal names otto, who is above her ───────
    let refused = marge.refused("merge_entities", merge.clone()).await;
    refused.says("bot:otto");
    let body = refused.json();
    assert_eq!(body["status"], "blocked", "{body}");
    assert_eq!(body["wrote"], false, "{body}");
    assert_eq!(body["attempted"], THREAD, "{body}");
    // The survivor gained no ceiling and no record from the refused merge, and
    // the duplicate answers at its own handle with its number and its records.
    let (id, count, fields) = held(&s, THREAD).await;
    assert_eq!((id.as_str(), count), (THREAD, survivor_held), "{fields}");
    assert!(
        fields.get("record_capacity").is_none_or(|n| n.is_null()),
        "the refused merge set a ceiling on the survivor: {fields}"
    );
    let (id, count, fields) = held(&s, "thread:simpson-kitchen").await;
    assert_eq!(
        (id.as_str(), count),
        ("thread:simpson-kitchen", duplicate_held),
        "{fields}"
    );
    assert_eq!(fields["record_capacity"], "5", "{fields}");

    // ── the positive: otto, above marge and not one of the writers, may ──────
    s.call("merge_entities", merge).await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"5\"");
    story.finish().await;
}

/// **A merge asks the same writers question an edit of the number asks.** The
/// bot that wrote plain notes into a thread is one of its writers, thought or no
/// thought, so it cannot raise the thread's number by merging in a duplicate that
/// carries a higher one, any more than it can by writing the number. Somebody
/// above it, who is not a writer, can.
#[tokio::test]
async fn a_bot_that_filled_a_thread_cannot_raise_its_number_by_merging() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let marge = story.as_bot("bot:marge").await;
    // marge reports to otto, so otto is above marge.
    s.call(
        "capture",
        json!({
            "subject": "bot:marge", "content": "marge reports up",
            "provenance": "testimony", "sets": {"reports_to": "bot:otto"},
        }),
    )
    .await;
    // marge writes two plain notes into the survivor and keeps no thought about it.
    s.add(THREAD, "Milhouse moves").await;
    for n in 1..=2 {
        marge
            .call(
                "capture",
                json!({"subject": THREAD, "content": format!("note {n} from marge")}),
            )
            .await;
    }
    // The duplicate holds a number, set by otto.
    s.add("thread:simpson-kitchen", "Simpson kitchen").await;
    s.call(
        "capture",
        json!({
            "subject": "thread:simpson-kitchen", "content": "how many notes it holds",
            "sets": {"record_capacity": "5"},
        }),
    )
    .await;
    let merge = json!({
        "duplicate": "thread:simpson-kitchen", "survivor": THREAD,
        "reason": "one line under two names",
    });

    // ── marge is refused, as she is when she writes the number ───────────────
    marge
        .refused("merge_entities", merge.clone())
        .await
        .says("record_capacity")
        .says("bot:otto")
        .says("\"wrote\":false");
    // Nothing moved: the duplicate still answers at its own handle and the
    // survivor holds no number.
    s.call(
        "recall",
        json!({"subject": "thread:simpson-kitchen", "facts": true}),
    )
    .await
    .says("how many notes it holds");
    s.call("recall", json!({"subject": THREAD}))
        .await
        .never_says("record_capacity");

    // ── the positive: otto, above marge and not one of the writers, may ──────
    s.call("merge_entities", merge).await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"5\"");
    story.finish().await;
}

/// How many records a thing holds, read through the surface, with the fields it
/// answers at its own handle. The handle in the answer is the thing's own, so a
/// thing folded into another reads as that other.
async fn held(s: &Session, handle: &str) -> (String, usize, serde_json::Value) {
    let read = s
        .call("recall", json!({"subject": handle, "facts": true}))
        .await
        .json();
    let object = &read["objects"][0];
    (
        object["id"].as_str().unwrap_or_default().to_string(),
        object["facts"].as_array().map_or(0, Vec::len),
        object["fields"].clone(),
    )
}

/// Three live notes on a thread, so it holds its number.
async fn a_full_thread(s: &Session, handle: &str, name: &str) {
    s.add(handle, name).await;
    for n in 1..=3 {
        s.call(
            "capture",
            json!({
                "subject": handle, "content": format!("note {n} on {name}"),
                "provenance": "testimony",
            }),
        )
        .await;
    }
}

/// **A write jojobot makes on its own behalf never strands the call that asked
/// for it.** Restoring a thread that held its three records brings it back as it
/// was, and the claim that records the restore is not a fourth note: it lands,
/// with or without a request id, and the thread still holds three.
#[tokio::test]
async fn a_thread_at_its_number_is_restored_with_the_record_of_the_restore() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name, request_id) in [
        (THREAD, "Milhouse moves", Some("restore-1")),
        ("thread:simpson-kitchen", "Simpson kitchen", None),
    ] {
        a_full_thread(&s, handle, name).await;
        s.call(
            "archive_entity",
            json!({"handle": handle, "reason": "the line has ended"}),
        )
        .await;
        let mut args = json!({"handle": handle, "reason": "the line is on again", "restore": true});
        if let Some(id) = request_id {
            args["request_id"] = json!(id);
        }

        // ── the restore finishes: the row is back and its record is written ──
        let restored = s.call("archive_entity", args).await.json();
        assert_eq!(restored["restored"], true, "{restored}");
        let recorded = restored["recorded_as"].as_str().expect("an address");
        assert!(recorded.starts_with(&format!("{handle}#")), "{restored}");
        s.call("recall", json!({"subject": handle, "facts": true}))
            .await
            .says(recorded)
            .says("the line has ended");

        // ── and the thread still holds three: the record took no slot ────────
        s.refused(
            "capture",
            json!({"subject": handle, "content": "a fourth note", "provenance": "testimony"}),
        )
        .await
        .number("/live", 3)
        .number("/ceiling", 3);
    }
    story.finish().await;
}

/// **The same for a rename that keeps the reason it was let through.** The reason
/// is a claim jojobot writes about the renamed thread; on a thread at its number
/// it lands, and a resend under the id finishes nothing twice.
#[tokio::test]
async fn a_thread_at_its_number_is_renamed_with_the_reason_it_was_let_through() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_full_thread(&s, THREAD, "Milhouse moves").await;
    s.add("thread:simpson-kitchen", "Simpson kitchen").await;
    let call = json!({"handle": THREAD, "to": "thread:kitchen"});
    let token = s
        .refused("rename_entity", call)
        .await
        .advised("override_token");
    let args = json!({
        "handle": THREAD, "to": "thread:kitchen", "override_token": token,
        "override_reason": "the move is in the kitchen, a line of its own",
        "request_id": "rename-1",
    });

    // ── the rename finishes, and the reason that let it through is kept ──────
    let renamed = s.call("rename_entity", args.clone()).await.json();
    assert_eq!(renamed["id"], "thread:kitchen", "{renamed}");
    assert!(renamed.get("reason_not_kept").is_none(), "{renamed}");
    s.recall("thread:kitchen")
        .await
        .says("the move is in the kitchen");

    // ── a resend under the same id answers the first receipt, writes nothing ─
    let (_, records_before, _) = held(&s, "thread:kitchen").await;
    let again = s.call("rename_entity", args).await.json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["id"], "thread:kitchen", "{again}");
    let (_, records_after, _) = held(&s, "thread:kitchen").await;
    assert_eq!(records_after, records_before, "the resend wrote a record");

    // ── and the thread still holds three: the reason took no slot ────────────
    s.refused(
        "capture",
        json!({"subject": "thread:kitchen", "content": "a fourth note", "provenance": "testimony"}),
    )
    .await
    .number("/live", 3)
    .number("/ceiling", 3);
    story.finish().await;
}

/// **The label that takes a record out of the count is jojobot's to write.** A
/// caller that sent it could keep adding to a full thread.
#[tokio::test]
async fn a_caller_cannot_label_its_own_record_as_one_that_takes_no_slot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_full_thread(&s, THREAD, "Milhouse moves").await;

    // The positive: the same fourth note without the label is an ordinary record
    // and is refused for the number, so the ceiling is counting here.
    s.refused(
        "capture",
        json!({"subject": THREAD, "content": "a fourth note", "provenance": "testimony"}),
    )
    .await
    .number("/live", 3)
    .number("/ceiling", 3);

    // The negative, for each key jojobot alone writes: refused for the key, not
    // for the number, and nothing written.
    for key in ["engine_record", "review_mark", "defragged_at"] {
        let refused = s
            .refused(
                "capture",
                json!({
                    "subject": THREAD, "content": "a fourth note", "provenance": "testimony",
                    "fields": {key: "restore"},
                }),
            )
            .await;
        refused
            .says(key)
            .holds_at("/fix_by", "change")
            .says("\"wrote\":false");
        let body = refused.json();
        assert!(
            body.pointer("/live").is_none(),
            "{key} was refused for the number, not as a key: {body}"
        );
    }
    let (_, records, _) = held(&s, THREAD).await;
    assert_eq!(records, 3, "nothing was written");
    story.finish().await;
}

/// **The assistant raises the number of a container it fills.** The operator
/// exempted the assistant from the rule that holds the writers of a container to
/// the number it has. Any other writer is held to it, and a bot above the writers
/// still raises it.
#[tokio::test]
async fn the_assistant_raises_the_number_of_a_container_it_fills() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let assistant = story.as_bot("bot:assistant").await;
    let marge = story.as_bot("bot:marge").await;
    for (handle, name) in [
        ("topic:marge-baking", "Marge baking"),
        (THREAD, "Milhouse moves"),
    ] {
        s.add(handle, name).await;
        // ── otto sets the number; the assistant and marge write the rest ─────
        let number = s
            .call(
                "capture",
                json!({
                    "subject": handle, "content": format!("how many notes {name} holds"),
                    "sets": {"record_capacity": "3"},
                }),
            )
            .await
            .field("address");
        for (writer, n) in [(&assistant, 2), (&marge, 3)] {
            writer
                .call(
                    "capture",
                    json!({"subject": handle, "content": format!("note {n} on {name}")}),
                )
                .await;
        }

        // ── marge filled it and raises it neither by edit nor by creation ────
        marge
            .refused(
                "update_fact",
                json!({"address": number, "sets": {"record_capacity": "50"}}),
            )
            .await
            .says("record_capacity");

        // ── the assistant filled it too, and raises it by two verbs ──────────
        assistant
            .call(
                "update_fact",
                json!({"address": number, "sets": {"record_capacity": "6"}}),
            )
            .await;
        s.call("recall", json!({"subject": handle}))
            .await
            .says("\"record_capacity\":\"6\"");
        assistant
            .call(
                "update_entity",
                json!({"handle": handle, "sets": {"record_capacity": "7"}}),
            )
            .await;
        s.call("recall", json!({"subject": handle}))
            .await
            .says("\"record_capacity\":\"7\"");
    }
    story.finish().await;
}

/// **The exemption is the whole rule's.** A thread binds the bots that wrote a
/// thought into it as well as the bots that wrote its records; the assistant is
/// exempt from both, and a bot that is not the assistant is bound by the thought.
#[tokio::test]
async fn the_assistant_that_wrote_a_thought_into_a_thread_raises_its_number() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let assistant = story.as_bot("bot:assistant").await;
    let marge = story.as_bot("bot:marge").await;
    s.add(THREAD, "Milhouse moves").await;
    let number = s
        .call(
            "capture",
            json!({
                "subject": THREAD, "content": "how many notes alpha holds",
                "sets": {"record_capacity": "3"},
            }),
        )
        .await
        .field("address");
    // Both bots write a thought into the thread, so the number now binds both.
    for bot in [&assistant, &marge] {
        let me = if std::ptr::eq(bot, &assistant) {
            "bot:assistant"
        } else {
            "bot:marge"
        };
        bot.call(
            "capture",
            json!({
                "subject": me, "content": "alpha is still on",
                "shape": "connection", "object": THREAD,
            }),
        )
        .await;
    }

    // ── marge is bound by the thought written into the thread
    marge
        .refused(
            "update_fact",
            json!({"address": number, "sets": {"record_capacity": "50"}}),
        )
        .await
        .says("record_capacity");

    // ── the assistant is not ─────────────────────────────────────────────────
    assistant
        .call(
            "update_fact",
            json!({"address": number, "sets": {"record_capacity": "6"}}),
        )
        .await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"6\"");
    story.finish().await;
}

/// **A bot is a writer of a container for the records it chose to write.** The
/// record of a restore says what jojobot did, so a bot that only restored a
/// thread did not fill it and may raise its number.
#[tokio::test]
async fn a_bot_that_only_restored_a_thread_did_not_fill_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    let marge = story.as_bot("bot:marge").await;
    s.add(THREAD, "Milhouse moves").await;
    let number = s
        .call(
            "capture",
            json!({
                "subject": THREAD, "content": "how many notes alpha holds",
                "sets": {"record_capacity": "3"},
            }),
        )
        .await
        .field("address");
    marge
        .call(
            "archive_entity",
            json!({"handle": THREAD, "reason": "the line has ended"}),
        )
        .await;
    marge
        .call(
            "archive_entity",
            json!({"handle": THREAD, "reason": "the line is on again", "restore": true}),
        )
        .await;
    marge
        .call(
            "update_fact",
            json!({"address": number, "sets": {"record_capacity": "6"}}),
        )
        .await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"record_capacity\":\"6\"");
    story.finish().await;
}

/// **A record ceiling binds whatever thing carries it, not only a thread.** A
/// topic given `record_capacity` refuses the write that would pass it, and the
/// refusal says the number came from the setting on the thing. Paired: a topic
/// with no setting has no ceiling and takes the same write.
#[tokio::test]
async fn a_topic_carrying_record_capacity_refuses_the_write_past_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call(
        "add_entity",
        json!({"kind": "topic", "handle": "all-rules", "name": "All rules",
               "source": "user-named", "sets": {"record_capacity": "2"}}),
    )
    .await;
    s.add("topic:the-five-words", "The five words").await;
    let write = |subject: &'static str, content: &'static str| json!({"subject": subject, "content": content, "provenance": "testimony"});

    // Creating the topic with its setting wrote its first record, so one more
    // fills the ceiling of two.
    s.call("capture", write("topic:all-rules", "the first rule"))
        .await;
    let refused = s
        .refused("capture", write("topic:all-rules", "the second rule"))
        .await;
    refused
        .number("/live", 2)
        .number("/ceiling", 2)
        .holds_at("/ceiling_from", "setting");

    for content in ["one", "two", "three", "four"] {
        s.call("capture", write("topic:the-five-words", content))
            .await;
    }
    story.finish().await;
}
