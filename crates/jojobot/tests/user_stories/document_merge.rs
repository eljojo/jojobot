//! "I kept the same manual under two names. Put them together."
//!
//! **Merging two documents leaves one document holding both**, and that is the
//! operator's word: the survivor's own sections first, then the folded
//! document's in the order they had, every long section whole. A long section is
//! stored as a heading and chunks that each name the heading they carry on, and
//! the merge gives every record a new id under the survivor, so the story reads
//! the merged document back whole and checks that no chunk names a heading that
//! is not on it.
//!
//! **The negatives are the half that makes the positive mean something.** A
//! document and a thing do not merge, in either direction, and the refusal names
//! the route; two things do.

use serde_json::json;

use super::dsl::{Session, Story};

const AUGUST: &str = "2026-08-03";
const SEPTEMBER: &str = "2026-09-15";
const OCTOBER: &str = "2026-10-08";

const ALPHA: &str = "# Alpha one\n\nthe first body\n\n# Alpha two\n\nthe second body\n\n\
                     # Alpha three\n\nthe third body\n";

/// The folded document: a section long enough to be cut into a heading and
/// chunks, then a short one.
fn beta() -> String {
    let paragraph = "word ".repeat(200);
    let mut text = String::from("# Beta long\n\n");
    for _ in 0..40 {
        text.push_str(paragraph.trim_end());
        text.push_str("\n\n");
    }
    text.push_str("# Beta short\n\nshort body\n");
    text
}

/// A document read whole, in as many parts as the answer ceiling needs.
async fn read_whole(session: &Session, document: &str, as_of: Option<&str>) -> String {
    let mut text = String::new();
    let mut offset = 0;
    loop {
        let mut ask = json!({"document": document, "offset": offset});
        if let Some(day) = as_of {
            ask["as_of"] = json!(day);
        }
        let part = session.call("read_document", ask).await.json();
        text.push_str(part["markdown"].as_str().expect("a part carries markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = next as usize,
            None => return text,
        }
    }
}

/// Every record of a document, in as many parts as `recall` needs.
async fn records_of(session: &Session, document: &str) -> Vec<serde_json::Value> {
    let mut held = Vec::new();
    let mut offset = 0;
    loop {
        let body = session
            .call(
                "recall",
                json!({"subject": document, "facts": true, "offset": offset}),
            )
            .await
            .json();
        held.extend(
            body["objects"][0]["facts"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next as usize,
            None => return held,
        }
    }
}

/// **Beta merged into alpha reads as alpha's three sections and then beta's
/// two, the long one whole, and no chunk on alpha continues an id outside it.**
#[tokio::test]
async fn two_documents_merged_read_as_one_with_the_folded_sections_last() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("document:beta", "Beta notes").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;
    let put = s
        .call(
            "put_document",
            json!({"document": "document:beta", "markdown": beta()}),
        )
        .await
        .json();
    assert!(
        put["cut_for_size"].as_u64().is_some_and(|n| n >= 1),
        "beta's long section was cut into chunks: {put}"
    );
    // The positive the merge is read against: each reads as its own text.
    assert_eq!(read_whole(&s, "document:alpha", None).await, ALPHA);
    assert_eq!(read_whole(&s, "document:beta", None).await, beta());

    clock.stating(OCTOBER.parse().expect("a day"));
    let merged = s
        .merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;
    assert_eq!(merged["claims_moved"].as_u64(), Some(3), "{merged}");

    assert_eq!(
        read_whole(&s, "document:alpha", None).await,
        format!("{ALPHA}{}", beta()),
        "alpha's sections, then beta's in their own order, the long one whole"
    );

    // No chunk on alpha continues an id outside it.
    let held = records_of(&s, "document:alpha").await;
    let ids: std::collections::BTreeSet<String> = held
        .iter()
        .filter(|fact| fact["fields"].get("continues").is_none())
        .map(|fact| {
            fact["address"]
                .as_str()
                .and_then(|address| address.rsplit('#').next())
                .expect("an address")
                .to_string()
        })
        .collect();
    let chunks: Vec<&serde_json::Value> = held
        .iter()
        .filter(|fact| fact["fields"].get("continues").is_some())
        .collect();
    assert!(!chunks.is_empty(), "the long section's chunks are on alpha");
    for chunk in chunks {
        let head = chunk["fields"]["continues"].as_str().expect("a heading id");
        assert!(ids.contains(head), "{head} is no heading on alpha: {ids:?}");
    }
    // **And it is its own heading**: an id a chunk kept from beta can name another
    // heading that wears it on alpha, so each chunk continues the heading in front
    // of it in reading order.
    let mut in_order = held.clone();
    in_order.sort_by_key(|fact| {
        fact["fields"]["order"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    });
    let mut heading = String::new();
    for fact in &in_order {
        let id = fact["address"]
            .as_str()
            .and_then(|address| address.rsplit('#').next())
            .expect("an address");
        match fact["fields"]["continues"].as_str() {
            None => heading = id.to_string(),
            Some(head) => assert_eq!(head, heading, "{id} continues a heading not in front of it"),
        }
    }
    story.finish().await;
}

/// **A document merged into a thing, or a thing into a document, is refused with
/// the route.** Paired: two things merge.
#[tokio::test]
async fn a_document_and_a_thing_do_not_merge_and_the_refusal_names_the_route() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("thing:handcart", "The Handcart").await;
    s.add("thing:the-couch", "The Couch").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;

    for (duplicate, survivor) in [
        ("document:alpha", "thing:handcart"),
        ("thing:handcart", "document:alpha"),
    ] {
        let refused = s
            .refused(
                "merge_entities",
                json!({"duplicate": duplicate, "survivor": survivor}),
            )
            .await;
        refused.says("read_document").says("archive_entity");
    }
    // Nothing moved.
    assert_eq!(read_whole(&s, "document:alpha", None).await, ALPHA);

    // The positive: two things merge.
    s.merge_entities("thing:the-couch", "thing:handcart", "one thing twice", None)
        .await;
    story.finish().await;
}

/// **A read as of a day before the merge returns the records of both
/// documents, each whole.** A merge moves claims and not dates, so a claim the
/// folded document wrote in August reads on the survivor from August: the
/// September read holds beta's sections as well, placed by the order keys they
/// were written with, which came from the same start as alpha's. What it must
/// not do is lose a record's edges or hand one record another's, which is what a
/// folded record's writes did while they wore an id the survivor also held.
#[tokio::test]
async fn a_read_as_of_before_the_merge_returns_every_section_whole() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("document:beta", "Beta notes").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;
    s.call(
        "put_document",
        json!({"document": "document:beta", "markdown": "# Beta one\n\nb1\n\n# Beta two\n\nb2\n"}),
    )
    .await;
    clock.stating(OCTOBER.parse().expect("a day"));
    s.merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;

    let then = read_whole(&s, "document:alpha", Some(SEPTEMBER)).await;
    for kept in [
        "# Alpha one\n\nthe first body\n\n",
        "# Alpha two\n\nthe second body\n\n",
        "# Alpha three\n\nthe third body\n",
        "# Beta one\n\nb1\n\n",
        "# Beta two\n\nb2\n",
    ] {
        assert!(then.contains(kept), "{kept:?} is whole in {then:?}");
    }
    let place = |heading: &str| {
        then.find(heading)
            .unwrap_or_else(|| panic!("{heading} in {then:?}"))
    };
    assert!(
        place("# Alpha one") < place("# Alpha two")
            && place("# Alpha two") < place("# Alpha three"),
        "alpha keeps its own order: {then:?}"
    );
    assert!(place("# Beta one") < place("# Beta two"), "{then:?}");
    story.finish().await;
}
