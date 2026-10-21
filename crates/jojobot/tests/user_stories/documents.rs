//! "I keep a manual as one markdown file. I want to put it in whole, read it back
//! whole, and change one part without touching the rest."
//!
//! **A document is held by section.** `put_document` splits a file at its
//! headings and stores each heading with what sits under it as one claim, so a
//! part can be edited, cited and read the history of on its own. `read_document`
//! returns the file byte for byte.
//!
//! **The negatives are the half that makes the positive mean something.** An
//! edit to one part leaves every other part's bytes where they were, and a second
//! put with no place to go is refused rather than writing the text twice.

use serde_json::json;

use super::dsl::Story;

const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil and wipe the lid.\n";

/// **The file put is the file read, and one edited part moves alone.**
#[tokio::test]
async fn a_manual_put_whole_is_read_back_whole_and_an_edit_moves_one_part() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;

    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await
        .json();
    assert_eq!(put["sections"], 3, "{put}");

    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(read["markdown"], MANUAL, "{read}");
    assert_eq!(read["name"], "Fryer manual");

    // The second section is the opening. Its address is what a claim cites and
    // what an edit goes through.
    let recalled = session.recall("document:homer").await;
    let address = recalled.json()["objects"][0]["facts"][1]["address"]
        .as_str()
        .expect("a section has an address")
        .to_string();
    session
        .call(
            "update_fact",
            json!({
                "address": address, "field": "details",
                "find": "at six", "replace": "at five",
            }),
        )
        .await;

    let edited = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        edited["markdown"],
        MANUAL.replace("at six", "at five"),
        "only the opening changed: {edited}"
    );
}

/// **A second put with no place to go is refused, and one that names a place
/// lands there.**
#[tokio::test]
async fn a_second_put_must_say_where_it_goes() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;

    let refused = session
        .refused(
            "put_document",
            json!({"document": "document:homer", "markdown": "## Cleaning\n\nOnce a week.\n"}),
        )
        .await;
    refused.says("after").says("update_fact");

    let opening =
        session.recall("document:homer").await.json()["objects"][0]["facts"][1]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string();
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": opening,
                "markdown": "## Cleaning\n\nOnce a week.\n\n",
            }),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Cleaning\n\nOnce a week.\n\n## Closing\n\nDrain the oil and wipe the lid.\n",
    );
}

/// **A section put after the last one reads as its own section.** The last
/// section of a file that ends without a blank line holds no separator after
/// its words, so a section put behind it would have its heading glued to that
/// line. The put gives the old last section the separator a whole put would
/// have, and the `edges` record shows it before and after.
///
/// **Paired**: a file that ends with one line feed gets the blank line it
/// lacked, a section put in the middle leaves the separators where they were,
/// and a file put whole reads back byte for byte.
#[tokio::test]
async fn a_section_put_after_the_last_one_reads_as_its_own_section() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    let last_edges = |facts: &serde_json::Value, n: usize| {
        facts["objects"][0]["facts"][n]["fields"]["edges"]
            .as_str()
            .expect("a section keeps its edges")
            .to_string()
    };
    let address_of = |facts: &serde_json::Value, n: usize| {
        facts["objects"][0]["facts"][n]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string()
    };

    // A file that ends on its last word: two sections, no trailing separator.
    let whole = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.";
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": whole}),
        )
        .await;
    let before = session.recall("document:homer").await.json();
    assert_eq!(
        session
            .call("read_document", json!({"document": "document:homer"}))
            .await
            .json()["markdown"],
        whole,
        "a file put whole reads back byte for byte"
    );
    assert!(
        last_edges(&before, 1).ends_with(";"),
        "the last section ends on its words: {before}"
    );

    // A middle put keeps the separators as they were.
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": address_of(&before, 0),
                "markdown": "## Hours\n\nSix to ten.\n\n",
            }),
        )
        .await;
    let middle = session.recall("document:homer").await.json();
    assert_eq!(
        last_edges(&middle, 1),
        last_edges(&before, 1),
        "the last section was not touched"
    );

    // The sections are listed in the order they were written; the opening is the
    // second, and it is still the last one in the document.
    // The put behind the last section: its heading is on its own line, after a
    // blank line, and the old last section is not written: the new section's own
    // lead carries the line feeds.
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": address_of(&middle, 1),
                "markdown": "## Closing\n\nDrain the oil.",
            }),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Hours\n\nSix to ten.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil.",
    );
    let after = session.recall("document:homer").await.json();
    assert_eq!(
        last_edges(&after, 1),
        last_edges(&middle, 1),
        "the old last section was written by the put behind it: {after}"
    );
    assert!(
        last_edges(&after, 3).starts_with("\\n\\n"),
        "the new section's lead carries the blank line: {after}"
    );

    // A file that ends with one line feed gets the blank line it lacked.
    session.add("document:marge", "Fryer notes").await;
    session
        .call(
            "put_document",
            json!({"document": "document:marge", "markdown": "# Notes\n\nFirst.\n"}),
        )
        .await;
    let notes = session.recall("document:marge").await.json();
    session
        .call(
            "put_document",
            json!({
                "document": "document:marge", "after": address_of(&notes, 0),
                "markdown": "## More\n\nSecond.\n",
            }),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:marge"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Notes\n\nFirst.\n\n## More\n\nSecond.\n"
    );
}

/// **A part moves to where it belongs, and the rest of the manual does not
/// move.** One call names the section it should follow; the page reads in the
/// new order and the sections that stayed are the ones put.
#[tokio::test]
async fn a_part_is_moved_behind_another_with_one_call() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let facts = session.recall("document:homer").await.json();
    let address_of = |n: usize| {
        facts["objects"][0]["facts"][n]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string()
    };
    let (opening, closing) = (address_of(1), address_of(2));

    session
        .call(
            "update_fact",
            json!({"address": closing, "after": address_of(0)}),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    // **No blank line stands between "wipe the lid." and "## Opening"**: the read
    // joins the sections byte for byte, and the section that closed the manual
    // never ended in a blank line, so moving it into the middle carries none. The
    // opening now ends the document with the blank line it had in the middle.
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Closing\n\nDrain the oil and wipe the lid.\n## Opening\n\nSwitch the fryer on at six.\n\n",
    );

    // An ordinary claim has no place to move, and the refusal says what
    // `after` is for.
    let remark = session
        .fact("document:homer", "reviewed by a colleague")
        .await;
    session
        .refused("update_fact", json!({"address": remark, "after": opening}))
        .await
        .says("after");
}

/// **A search for a word in a manual lands on the part that holds it, and says
/// which manual that is.** Two parts of one manual match: they arrive together,
/// each naming the document.
#[tokio::test]
async fn a_search_lands_on_a_part_and_names_the_manual() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    // An ordinary claim about the manual, which is not part of its text: it keeps
    // the manual's name, so asking by that name still reaches it.
    session
        .fact("document:homer", "reviewed by a colleague")
        .await;
    let found = session
        .call("search", json!({"query": "fryer"}))
        .await
        .json();
    let results = found["results"].as_array().expect("results");
    assert!(
        results
            .iter()
            .any(|hit| hit["content"] == "reviewed by a colleague"),
        "a claim about the manual is found by the manual's name: {found}"
    );
    let parts: Vec<&serde_json::Value> = results
        .iter()
        .filter(|hit| hit["document"]["handle"] == "document:homer")
        .collect();
    // **Both parts that hold the word are asserted**: the title part and the
    // opening. Grouping is a claim about more than one hit, so a search that found
    // one part and named its manual would pass a check that the manual was named.
    let contents: Vec<&str> = parts
        .iter()
        .filter_map(|hit| hit["content"].as_str())
        .collect();
    for held in ["# Fryer manual", "## Opening"] {
        assert!(
            contents.contains(&held),
            "{held} mentions the fryer and was found: {found}"
        );
    }
    // **A part that holds nothing asked for is not a hit**, in the same read as
    // the parts that do and as the manual found by its name. The closing part
    // says nothing of the fryer: it sits on a document called "Fryer manual",
    // which is the manual's name and not the part's words.
    assert_eq!(
        contents.len(),
        2,
        "only the parts that hold the word come back: {found}"
    );
    assert!(
        !contents.contains(&"## Closing"),
        "the closing part holds nothing asked for: {found}"
    );
    assert!(
        results
            .iter()
            .any(|hit| hit["hit"] == "entity" && hit["id"] == "document:homer"),
        "the manual itself is found by its name: {found}"
    );
    assert!(
        parts
            .iter()
            .all(|hit| hit["document"]["name"] == "Fryer manual"),
        "{found}"
    );
    // They arrive together: no other hit stands between the manual's parts.
    let places: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, hit)| hit["document"]["handle"] == "document:homer")
        .map(|(place, _)| place)
        .collect();
    assert!(places.len() >= 2, "more than one part is grouped: {found}");
    assert!(
        places.windows(2).all(|pair| pair[1] - pair[0] == 1),
        "the parts of the manual are listed together: {found}"
    );
}

/// **A section that opens with a code fence keeps the fence, and what is inside
/// it stays literal.** The text is read back byte for byte, a handle in the fence
/// that names nothing does not refuse the put, and a handle in the fence that
/// names a thing is not linked to it: a rename leaves the fenced text as it was
/// written.
///
/// **Paired**: the same handle in a section after the fence is a link, and the
/// rename moves what it reads as, so the fenced handle stayed because it is
/// code and not because nothing was ever linked.
#[tokio::test]
async fn a_section_that_opens_with_a_code_fence_keeps_what_is_inside_it_literal() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session.add("person:milhouse", "Milhouse").await;
    let markdown = "```text\nrun @person:milhouse and @person:nelson\n```\n\n## Notes\n\nask @person:milhouse about it\n";

    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": markdown}),
        )
        .await
        .json();
    assert_eq!(put["sections"], 2, "{put}");
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(read["markdown"], markdown, "{read}");

    session
        .call(
            "rename_entity",
            json!({"handle": "person:milhouse", "to": "person:bart"}),
        )
        .await;
    let renamed = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        renamed["markdown"],
        markdown.replace("ask @person:milhouse", "ask @person:bart"),
        "the fenced handle stays as written and the one in the notes follows the rename: {renamed}"
    );

    // A walk in from the thing reaches the document that mentions it in a
    // sentence and not one that names it only inside a fence.
    session.add("document:marge", "Griddle manual").await;
    session.add("person:ned-flanders", "Ned").await;
    session
        .call(
            "put_document",
            json!({
                "document": "document:marge",
                "markdown": "```\n@person:bart\n```\n\nplain words for @person:ned-flanders\n",
            }),
        )
        .await;
    session
        .call(
            "recall",
            json!({"subject": "person:bart", "follow": {"direction": "in"}}),
        )
        .await
        .says("document:homer")
        .never_says("document:marge");
    // …and the walk out from a document reaches what its sentences name and not
    // what its fence holds.
    session
        .call(
            "recall",
            json!({"subject": "document:homer", "follow": {"direction": "out"}}),
        )
        .await
        .says("person:bart");
    session
        .call(
            "recall",
            json!({"subject": "document:marge", "follow": {"direction": "out"}}),
        )
        .await
        .says("person:ned-flanders")
        .never_says("person:bart");

    // An edit to the details of the section that opens the fence is read beside
    // its content, so the fence still holds what is inside it.
    let recalled = session.recall("document:homer").await;
    let address = recalled.json()["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("a section has an address")
        .to_string();
    session
        .call(
            "update_fact",
            json!({"address": address, "details": "see @person:nelson again\n```"}),
        )
        .await;
    let edited = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        edited["markdown"],
        "```text\nsee @person:nelson again\n```\n\n## Notes\n\nask @person:bart about it\n",
        "{edited}"
    );
}

/// A paragraph of about a thousand characters, ending in a blank line.
fn paragraph() -> String {
    format!("{}\n\n", "word ".repeat(200).trim_end())
}

/// **A long section's code stays literal across the cut.** A section too big for
/// one record is cut into several, and the cut here would fall inside a fenced
/// block that holds a handle naming nothing. The cut moves to before the block,
/// the document is stored, and it reads back byte for byte.
///
/// **Paired**: the same handle in a paragraph of a section as long is still
/// refused, so the put was stored because the handle is code and not because
/// nothing was read.
#[tokio::test]
async fn a_long_sections_code_stays_literal_across_the_cut() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session.add("document:marge", "Griddle manual").await;

    // About 22 thousand characters of prose, then a fenced block of about 8
    // thousand that holds the handle at its end: the record limit falls inside it.
    let mut text = String::from("# Big\n\n");
    for _ in 0..22 {
        text.push_str(&paragraph());
    }
    text.push_str("```text\n");
    for _ in 0..7 {
        text.push_str(&format!("{}\n\n", "line ".repeat(200).trim_end()));
    }
    text.push_str("see @person:nelson here\n```\n\n");
    for _ in 0..30 {
        text.push_str(&paragraph());
    }

    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": text}),
        )
        .await
        .json();
    assert!(
        put["cut_for_size"].as_u64().is_some_and(|n| n >= 1),
        "the section is cut for size: {put}"
    );
    // A long document is read in parts, each naming where the next begins.
    let mut read = String::new();
    let mut offset = 0;
    loop {
        let part = session
            .call(
                "read_document",
                json!({"document": "document:homer", "offset": offset}),
            )
            .await
            .json();
        read.push_str(part["markdown"].as_str().expect("a part has markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(read, text, "stored whole and read back whole");

    // The same handle outside code, in a section as long, is refused.
    let mut prose = String::from("# Big\n\n");
    for _ in 0..22 {
        prose.push_str(&paragraph());
    }
    prose.push_str("see @person:nelson here\n\n");
    for _ in 0..30 {
        prose.push_str(&paragraph());
    }
    session
        .refused(
            "put_document",
            json!({"document": "document:marge", "markdown": prose}),
        )
        .await
        .says("person:nelson");
}

// --- a document as it stood on a day ------------------------------------------
//
// "What did the manual say before the last edit?" `read_document` takes `as_of`,
// a day or an instant, and answers with the sections as they stood then. Every
// story below writes on two different days, because a story that wrote on one
// day could not tell a read as of a day from a read of today.

const AUGUST: &str = "2026-08-03";
const SEPTEMBER: &str = "2026-09-15";
const OCTOBER: &str = "2026-10-08";

/// The addresses of the sections of [`MANUAL`], in the order they were put.
async fn manual_addresses(session: &super::dsl::Session) -> Vec<String> {
    let facts = session.recall("document:homer").await.json();
    facts["objects"][0]["facts"]
        .as_array()
        .expect("the document's records")
        .iter()
        .map(|fact| fact["address"].as_str().expect("an address").to_string())
        .collect()
}

/// **A section edited on a later day reads, as of the earlier day, as it was
/// then, and today's read is unchanged.** Three reads of one document: as of the
/// day it was put, as of a day between the two writes, and as of the day after the
/// edit. The pair is the point: the read as of the day before the edit and the
/// read of today differ, so the argument is doing something.
#[tokio::test]
async fn a_document_as_of_a_day_reads_that_days_wording_and_today_is_unchanged() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let opening = manual_addresses(&session).await[1].clone();

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({
                "address": opening, "field": "details",
                "find": "at six", "replace": "at five",
            }),
        )
        .await;
    let edited = MANUAL.replace("at six", "at five");

    let today = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(today["markdown"], edited, "today reads the edit: {today}");
    assert!(
        today.get("as_of").is_none(),
        "a read with no as_of names no moment: {today}"
    );

    for day in [AUGUST, SEPTEMBER] {
        let then = session
            .call(
                "read_document",
                json!({"document": "document:homer", "as_of": day}),
            )
            .await
            .json();
        assert_eq!(
            then["markdown"], MANUAL,
            "as of {day} the wording is the one put in August: {then}"
        );
        assert_eq!(
            then["as_of"]["asked"], day,
            "the answer echoes the ask: {then}"
        );
        assert!(
            then["as_of"]["moment"]
                .as_str()
                .is_some_and(|moment| moment.starts_with(&format!("{day}T23:59:59"))),
            "a day means the end of that day, and the answer names the instant: {then}"
        );
    }

    let after = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": OCTOBER}),
        )
        .await
        .json();
    assert_eq!(
        after["markdown"], edited,
        "as of the day of the edit the edit is there: {after}"
    );
}

/// **A section made later is absent as of the earlier day, and one archived later
/// is present.** Paired in one document: the section put in October is not in the
/// August read, and the section archived in October is.
#[tokio::test]
async fn a_section_made_later_is_absent_and_one_archived_later_is_present() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let addresses = manual_addresses(&session).await;
    let (closing, opening) = (addresses[2].clone(), addresses[1].clone());

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": closing,
                "markdown": "## Cleaning\n\nWipe the fryer down each night.\n",
            }),
        )
        .await;
    session
        .call(
            "update_fact",
            json!({"address": opening, "status": "archived"}),
        )
        .await;

    let today = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let today_text = today["markdown"].as_str().expect("text");
    assert!(
        today_text.contains("Cleaning") && !today_text.contains("Opening"),
        "today has the new section and not the archived one: {today}"
    );

    let then = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": SEPTEMBER}),
        )
        .await
        .json();
    assert_eq!(
        then["markdown"], MANUAL,
        "as of September the manual is as it was put: the archived section is there and the later one is not: {then}"
    );
    assert!(
        then["archived_sections_left_out"].is_null(),
        "nothing had been archived yet, so nothing is counted as left out: {then}"
    );
}

/// **A section moved later reads, as of the earlier day, in the place it had.**
#[tokio::test]
async fn a_section_moved_later_reads_in_its_old_place_as_of_the_earlier_day() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let addresses = manual_addresses(&session).await;

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({"address": addresses[2], "after": addresses[0]}),
        )
        .await;

    let today = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_ne!(
        today["markdown"], MANUAL,
        "the move changed today's read, or this story proves nothing: {today}"
    );
    let then = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": SEPTEMBER}),
        )
        .await
        .json();
    assert_eq!(
        then["markdown"], MANUAL,
        "as of September the closing is still last: {then}"
    );
    // The pair: as of a day after the move the section is in its new place.
    let after = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": OCTOBER}),
        )
        .await
        .json();
    assert_eq!(
        after["markdown"], today["markdown"],
        "as of the day of the move the closing has moved: {after}"
    );
}

/// **A section that rests on a write the store kept before it stamped its
/// writes is named, never vouched for.** The pair is the read before the
/// stamps are lost, which names nothing.
#[tokio::test]
async fn a_section_resting_on_an_unstamped_write_is_named_as_one_the_read_could_not_place() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;

    let stamped = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": "2999-12-31"}),
        )
        .await
        .json();
    assert_eq!(stamped["markdown"], MANUAL, "{stamped}");
    assert!(
        stamped["as_of"].get("unplaced").is_none(),
        "a section with a moment of its own is placed: {stamped}"
    );

    store.forgets_when_its_writes_happened();
    story.the_store_changed_underneath().await;
    let unstamped = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": "2999-12-31"}),
        )
        .await
        .json();
    assert_eq!(
        unstamped["as_of"]["unplaced"].as_array().map(Vec::len),
        Some(3),
        "all three sections rest on a write with no moment: {unstamped}"
    );
}

// --- a document restored to a day --------------------------------------------
//
// "Put the manual back the way it was in September." `put_document` takes
// `restore_to`, a day or an instant, and writes that state back as new versions.
// History is never rewritten: a section that still stands is edited, a section
// archived since comes back as a new record that names the archived one in
// `derived_from`, and a section made since is archived.

/// The active record of a document whose words are `heading`, as recall serves it.
async fn active_section(session: &super::dsl::Session, heading: &str) -> serde_json::Value {
    let facts = session.recall("document:homer").await.json();
    facts["objects"][0]["facts"]
        .as_array()
        .expect("the document's records")
        .iter()
        .find(|fact| fact["content"] == heading && fact["status"] == "active")
        .unwrap_or_else(|| panic!("an active section {heading}: {facts}"))
        .clone()
}

/// Build the document the restore stories share: put on a day in August, then on a
/// day in October the opening is edited, a section is made and the closing is
/// archived. Answers the story, the clock, the session and the opening's and the
/// closing's addresses.
async fn manual_changed_in_october() -> (
    Story,
    jojobot_domain::memory::testing::ClockHandle,
    super::dsl::Session,
    String,
    String,
) {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let addresses = manual_addresses(&session).await;
    let (opening, closing) = (addresses[1].clone(), addresses[2].clone());

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({
                "address": opening, "field": "details",
                "find": "at six", "replace": "at five",
            }),
        )
        .await;
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": closing,
                "markdown": "## Cleaning\n\nWipe the fryer down each night.\n",
            }),
        )
        .await;
    session
        .call(
            "update_fact",
            json!({"address": closing, "status": "archived"}),
        )
        .await;
    (story, clock, session, opening, closing)
}

/// **A restore puts the document back as the read of that day showed it, byte for
/// byte, and every earlier version stays readable.** The opening was edited, the
/// closing archived and a section made since: after the restore the whole read is
/// the September read; the opening has its edit and its restoration in its own
/// history; the closing is a NEW record naming the archived one; the section made
/// since is archived. A second restore to the same day changes nothing.
#[tokio::test]
async fn a_restore_puts_the_document_back_as_a_day_read_it_and_keeps_every_version() {
    let (_story, _clock, session, opening, closing) = manual_changed_in_october().await;
    let then = session
        .call(
            "read_document",
            json!({"document": "document:homer", "as_of": SEPTEMBER}),
        )
        .await
        .json();
    assert_eq!(then["markdown"], MANUAL, "the September read: {then}");
    let today = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_ne!(
        today["markdown"], then["markdown"],
        "the document moved since, or this story proves nothing: {today}"
    );

    let restored = session
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");

    let now = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        now["markdown"], then["markdown"],
        "the whole read is the September read, byte for byte: {now}"
    );

    // The opening still stands, on its own address, with every version.
    let history = session
        .call(
            "recall",
            json!({"history_record": opening, "details": true}),
        )
        .await
        .json();
    let writes = history["objects"][0]["record_history"]["writes"]
        .as_array()
        .unwrap_or_else(|| panic!("the opening's writes: {history}"));
    let said = |needle: &str| writes.iter().any(|w| w.to_string().contains(needle));
    assert!(
        writes.len() >= 3 && said("at six") && said("at five"),
        "put, edit and restoration are all there: {history}"
    );

    // The closing was archived, so it comes back as a new record that names it.
    let back = active_section(&session, "## Closing").await;
    assert_ne!(
        back["address"], closing,
        "an archived claim has no un-archive, so the section is a new record: {back}"
    );
    assert_eq!(
        back["derived_from"], closing,
        "…and it names the one it restores: {back}"
    );

    // A second restore to the same day finds nothing to do.
    let again = session
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(again["status"], "blocked", "{again}");
    let after_again = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        after_again["markdown"], MANUAL,
        "a restore to the same day twice leaves the same page: {after_again}"
    );
    assert_eq!(
        again["restored"]["changed"], 0,
        "…and says it changed nothing: {again}"
    );
}

/// **A restore that would change a section whose wording is testimony is refused
/// whole unless the call carries `confirmed_by_user`**, naming the sections and
/// the flag, and nothing is written. With the flag the testimony section comes
/// back as a new record that is still testimony and names the version restored.
#[tokio::test]
async fn a_restore_over_testimony_needs_the_operators_confirmation_or_changes_nothing() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "markdown": MANUAL,
                "provenance": "testimony",
            }),
        )
        .await;
    let opening = manual_addresses(&session).await[1].clone();
    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({
                "address": opening, "field": "details",
                "find": "at six", "replace": "at five",
            }),
        )
        .await;
    let edited = MANUAL.replace("at six", "at five");

    session
        .refused(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .says("confirmed_by_user")
        .says(&opening);
    let unchanged = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        unchanged["markdown"], edited,
        "a refused restore wrote nothing: {unchanged}"
    );

    let confirmed = session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "restore_to": SEPTEMBER,
                "confirmed_by_user": true,
            }),
        )
        .await
        .json();
    assert_ne!(confirmed["status"], "blocked", "{confirmed}");
    let restored = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(restored["markdown"], MANUAL, "{restored}");
    let back = active_section(&session, "## Opening").await;
    assert_eq!(
        back["provenance"], "testimony",
        "the restored wording keeps the provenance it had: {back}"
    );
    assert_eq!(
        back["derived_from"], opening,
        "…and names the version restored: {back}"
    );
}

/// **A restore names a moment or it is refused, and it is refused for a moment the
/// document did not exist.** Neither `markdown` nor `restore_to`, and both, are
/// refused; a restore to a day before the first write would archive every section,
/// which is not a restoration, and is refused with nothing written.
#[tokio::test]
async fn a_restore_without_a_moment_or_to_a_day_before_the_document_is_refused() {
    let (_story, _clock, session, _opening, _closing) = manual_changed_in_october().await;
    let before = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();

    session
        .refused("put_document", json!({"document": "document:homer"}))
        .await
        .says("restore_to");
    session
        .refused(
            "put_document",
            json!({
                "document": "document:homer", "markdown": MANUAL,
                "restore_to": SEPTEMBER,
            }),
        )
        .await
        .says("restore_to");
    session
        .refused(
            "put_document",
            json!({"document": "document:homer", "restore_to": "2026-07-01"}),
        )
        .await
        .says("restore_to");

    let after = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        after["markdown"], before["markdown"],
        "a refused restore wrote nothing: {after}"
    );
}

/// The whole read of `document:homer`, taken in as many parts as it comes in.
async fn read_the_whole_manual(session: &super::dsl::Session) -> String {
    let mut read = String::new();
    let mut offset = 0;
    loop {
        let part = session
            .call(
                "read_document",
                json!({"document": "document:homer", "offset": offset}),
            )
            .await
            .json();
        read.push_str(part["markdown"].as_str().expect("a part has markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => return read,
        }
    }
}

/// "I restored the fryer manual to September and the end of the long section was
/// missing. Does restoring again bring it back?" A section too long for one
/// record is a heading and chunks that carry it on. The section is archived and
/// restored, and then the chunk that holds its last line is archived by hand: the
/// state a restore that stopped before its last write leaves. The same restore
/// sent again must bring that last line back where it is read, not write it
/// under the archived heading it copied.
///
/// **Paired**: the first restore reads whole, so the loss is the hand-archive and
/// not a cut that never worked; and a third restore writes nothing, so the second
/// brought back a section that stays.
#[tokio::test]
async fn a_restore_sent_again_after_a_chunk_was_lost_brings_back_every_chunk() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    let mut manual = String::from("# Big\n\n");
    for _ in 0..40 {
        manual.push_str(&paragraph());
    }
    manual.push_str("The tail ends here.\n");
    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": manual}),
        )
        .await
        .json();
    assert!(
        put["cut_for_size"].as_u64().is_some_and(|n| n >= 1),
        "the section is cut into chunks: {put}"
    );
    assert_eq!(read_the_whole_manual(&session).await, manual, "put whole");

    // October: the heading is archived, which takes the section off the page.
    clock.stating(OCTOBER.parse().expect("a day"));
    let heading = manual_addresses(&session).await[0].clone();
    session
        .call(
            "update_fact",
            json!({"address": heading, "status": "archived"}),
        )
        .await;
    assert_ne!(
        read_the_whole_manual(&session).await,
        manual,
        "the section is gone, or this story proves nothing"
    );

    let restore = || {
        session.call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
    };
    let first = restore().await.json();
    assert_ne!(first["status"], "blocked", "{first}");
    assert_eq!(
        read_the_whole_manual(&session).await,
        manual,
        "the first restore reads whole"
    );

    // The chunk that holds the last line, as the restore wrote it, is lost.
    // Recall serves the large chunks a few at a time, so read every page. The
    // first section's own chunks still stand under its archived heading; the
    // restore wrote its copy of the last line after them, so the newest record
    // holding the line is the restored one.
    let mut holding: Vec<(String, String)> = Vec::new();
    let mut offset = 0;
    loop {
        let page = session
            .call(
                "recall",
                json!({"subject": "document:homer", "facts": true, "details": true, "offset": offset}),
            )
            .await
            .json();
        let facts = page["objects"][0]["facts"]
            .as_array()
            .unwrap_or_else(|| panic!("a page of records: {page}"));
        holding.extend(
            facts
                .iter()
                .filter(|fact| {
                    fact["status"] == "active"
                        && ["content", "details"].iter().any(|part| {
                            fact[*part]
                                .as_str()
                                .is_some_and(|text| text.contains("The tail ends here."))
                        })
                })
                .map(|fact| {
                    (
                        fact["inserted_at"].as_str().unwrap_or_default().to_string(),
                        fact["address"].as_str().expect("an address").to_string(),
                    )
                }),
        );
        match page["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    let lost = holding.into_iter().max().map(|(_, address)| address);
    let lost = lost.expect("the restored chunk that holds the last line");
    session
        .call(
            "update_fact",
            json!({"address": lost, "status": "archived"}),
        )
        .await;
    assert_ne!(
        read_the_whole_manual(&session).await,
        manual,
        "the last line is gone, or this story proves nothing"
    );

    let second = restore().await.json();
    assert_ne!(second["status"], "blocked", "{second}");
    assert_eq!(
        read_the_whole_manual(&session).await,
        manual,
        "the restore sent again brings the last line back: {second}"
    );

    let third = restore().await.json();
    assert_eq!(
        third["restored"]["changed"], 0,
        "…and a further repeat writes nothing: {third}"
    );
}

/// "I folded the closing into the opening and noted it came from the closing.
/// Does restoring September put the closing back without touching the opening?"
/// The closing is archived and the opening is edited to say it was worked out
/// from the closing. A restore to the day before must bring the closing back as
/// its own section and leave the opening as it was, not read the opening as the
/// closing's copy and put the closing's words over it.
///
/// **Paired**: the restore reads the September page byte for byte, so the
/// closing came back and the opening is the opening.
#[tokio::test]
async fn a_section_worked_out_from_an_archived_one_is_not_its_restored_copy() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let addresses = manual_addresses(&session).await;
    let (opening, closing) = (addresses[1].clone(), addresses[2].clone());

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({"address": closing, "status": "archived"}),
        )
        .await;
    session
        .call(
            "update_fact",
            json!({"address": opening, "derived_from": closing}),
        )
        .await;
    assert_ne!(
        read_the_whole_manual(&session).await,
        MANUAL,
        "the closing is gone, or this story proves nothing"
    );

    let restored = session
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");
    assert_eq!(
        read_the_whole_manual(&session).await,
        MANUAL,
        "the September page, byte for byte: {restored}"
    );
}

/// Every record of `document:homer` that stands, across the pages recall serves
/// them in.
async fn standing_records(session: &super::dsl::Session) -> Vec<serde_json::Value> {
    let mut found = Vec::new();
    let mut offset = 0;
    loop {
        let page = session
            .call(
                "recall",
                json!({"subject": "document:homer", "facts": true, "details": true, "offset": offset}),
            )
            .await
            .json();
        let facts = page["objects"][0]["facts"]
            .as_array()
            .unwrap_or_else(|| panic!("a page of records: {page}"));
        found.extend(facts.iter().filter(|f| f["status"] == "active").cloned());
        match page["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => return found,
        }
    }
}

/// "I replaced the closing with a new cleaning section, noting it came from the
/// closing. Restoring September: does the closing come back and say where it
/// came from, and does the cleaning section go?" The closing is archived and a
/// section made since is edited to say it was worked out from the closing. A
/// restore to the day before must bring the closing back as a section that says
/// which version it reproduces, and must archive the section made since as it
/// archives any section made since, not rewrite it into the closing.
///
/// **Paired in one read**: the page is the September page, the restored closing
/// says which version it reproduces, and the section made since no longer stands.
#[tokio::test]
async fn a_record_worked_out_from_a_section_is_not_taken_for_its_restored_copy() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let closing = manual_addresses(&session).await[2].clone();

    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({"address": closing, "status": "archived"}),
        )
        .await;
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": manual_addresses(&session).await[1],
                "markdown": "## Cleaning\n\nWipe the fryer down each night.\n",
            }),
        )
        .await;
    let cleaning = standing_records(&session)
        .await
        .into_iter()
        .find(|fact| fact["content"] == "## Cleaning")
        .expect("the section made since");
    session
        .call(
            "update_fact",
            json!({"address": cleaning["address"], "derived_from": closing}),
        )
        .await;

    let restored = session
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");
    assert_eq!(
        read_the_whole_manual(&session).await,
        MANUAL,
        "the September page: {restored}"
    );
    let standing = standing_records(&session).await;
    let back = standing
        .iter()
        .find(|fact| fact["content"] == "## Closing")
        .unwrap_or_else(|| panic!("the closing stands again: {standing:?}"));
    assert_eq!(
        back["fields"]["restored_from"], closing,
        "the restored closing says which version it reproduces: {back}"
    );
    assert!(
        standing.iter().all(|fact| fact["content"] != "## Cleaning"),
        "the section made since is archived, not rewritten into the closing: {standing:?}"
    );
}

/// "I restored the closing, then fixed a word in it. If I restore September
/// again, do I get a second closing?" The closing is archived and restored, and
/// the restored closing is then edited. A restore sent again finds that copy by
/// the version it says it reproduces and puts its wording back on the SAME
/// record; it does not archive the copy and write a third closing.
///
/// **Paired in one read**: the page is the September page and the closing stands
/// on the address the first restore gave it, still saying which version it
/// reproduces.
#[tokio::test]
async fn an_edited_restored_copy_is_found_by_its_mark_and_put_back_in_place() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let closing = manual_addresses(&session).await[2].clone();
    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({"address": closing, "status": "archived"}),
        )
        .await;
    let restore = || {
        session.call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
    };
    restore().await;
    let copy = standing_records(&session)
        .await
        .into_iter()
        .find(|fact| fact["content"] == "## Closing")
        .expect("the restored closing");
    assert_eq!(copy["fields"]["restored_from"], closing, "{copy}");
    session
        .call(
            "update_fact",
            json!({
                "address": copy["address"], "field": "details",
                "find": "wipe the lid", "replace": "wipe the hood",
            }),
        )
        .await;
    assert_ne!(
        read_the_whole_manual(&session).await,
        MANUAL,
        "the copy was edited, or this story proves nothing"
    );

    let again = restore().await.json();
    assert_ne!(again["status"], "blocked", "{again}");
    assert_eq!(
        read_the_whole_manual(&session).await,
        MANUAL,
        "the September page: {again}"
    );
    let standing = standing_records(&session).await;
    let closings: Vec<_> = standing
        .iter()
        .filter(|fact| fact["content"] == "## Closing")
        .collect();
    assert_eq!(closings.len(), 1, "one closing stands: {standing:?}");
    assert_eq!(
        closings[0]["address"], copy["address"],
        "the copy was put back in place, not replaced: {again}"
    );
    assert_eq!(closings[0]["fields"]["restored_from"], closing);
}

/// **A restore sent again after the document was renamed finds the copy the
/// first one made.** The closing is archived and restored under the name the
/// document wore then; the document is renamed and the same restore is sent under
/// the new name. The copy names the version it reproduces by an address that
/// carries the old name, and the second restore has to read that address as the
/// version it stands for now. **Paired**: the first restore wrote, so the second
/// writing nothing is the rename being read and not a restore with nothing to do.
#[tokio::test]
async fn a_restore_sent_again_after_a_rename_finds_the_copy_it_made() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let closing = manual_addresses(&session).await[2].clone();
    clock.stating(OCTOBER.parse().expect("a day"));
    session
        .call(
            "update_fact",
            json!({"address": closing, "status": "archived"}),
        )
        .await;
    let first = session
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_eq!(first["restored"]["changed"], 1, "{first}");

    clock.stating("2026-11-02".parse().expect("a day"));
    session
        .call(
            "rename_entity",
            json!({"handle": "document:homer", "to": "document:marge"}),
        )
        .await;
    let again = session
        .call(
            "put_document",
            json!({"document": "document:marge", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(again["status"], "blocked", "{again}");
    assert_eq!(
        again["restored"]["changed"], 0,
        "the copy was found, so nothing is written: {again}"
    );
    let read = session
        .call("read_document", json!({"document": "document:marge"}))
        .await
        .json();
    assert_eq!(read["markdown"], MANUAL, "{read}");
}

/// **A section carries its own words only, in search.** The document's name is
/// on the document, which is found by it; a section that holds nothing asked for
/// is not a hit because it sits on a page with that name. The paired positive is
/// a search for the section's own words, which finds it.
#[tokio::test]
async fn a_section_is_found_by_its_own_words_and_not_by_the_name_of_its_document() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer",
                   "markdown": "# Warm up\n\nPreheat the oil at six.\n"}),
        )
        .await
        .json();
    assert_eq!(put["sections"], 1, "{put}");
    let address =
        session.recall("document:homer").await.json()["objects"][0]["facts"][0]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string();

    session
        .call("search", json!({"query": "Preheat"}))
        .await
        .says(&address);

    let by_name = session.call("search", json!({"query": "Fryer"})).await;
    by_name.says("document:homer");
    by_name.never_says(&address);
    story.finish().await;
}
