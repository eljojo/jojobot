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

/// **A read as of a day before the merge returns the survivor's own sections
/// of that day, and a read as of the merge day or later returns both.** Alpha and
/// beta are written in August and beta is merged into alpha in October. As of
/// September alpha held its three sections and beta's two were on beta, so the
/// read holds alpha's alone. As of the merge day the folded sections have arrived,
/// and they read after the survivor's last. The two reads differ only by the
/// merge, so the argument is doing something.
#[tokio::test]
async fn a_read_as_of_before_the_merge_returns_only_the_survivors_own_sections() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("document:beta", "Beta notes").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;
    let betas = "# Beta one\n\nb1\n\n# Beta two\n\nb2\n";
    s.call(
        "put_document",
        json!({"document": "document:beta", "markdown": betas}),
    )
    .await;
    clock.stating(OCTOBER.parse().expect("a day"));
    s.merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;

    assert_eq!(
        read_whole(&s, "document:alpha", Some(SEPTEMBER)).await,
        ALPHA,
        "before the merge alpha held its own sections and nothing of beta's"
    );
    for day in [OCTOBER, "2026-10-09"] {
        assert_eq!(
            read_whole(&s, "document:alpha", Some(day)).await,
            format!("{ALPHA}{betas}"),
            "as of {day} alpha's sections, then beta's, whole"
        );
    }
    story.finish().await;
}

/// **A read as of a day, sent under a name the document no longer wears, reads
/// what the same read under its current name reads.** Alpha and beta are written
/// in August, beta is merged into alpha in October, and alpha is renamed in
/// November. As of September the document held alpha's own sections whichever
/// name asks; the folded sections arrived in October and read under both names
/// from then on. **Paired**: the read as of October holds both documents' sections
/// under the old name, so the September read is short of them because of the date
/// and not because of the name.
#[tokio::test]
async fn a_read_as_of_under_a_former_name_matches_the_current_names() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("document:beta", "Beta notes").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;
    let betas = "# Beta one\n\nb1\n\n# Beta two\n\nb2\n";
    s.call(
        "put_document",
        json!({"document": "document:beta", "markdown": betas}),
    )
    .await;
    clock.stating(OCTOBER.parse().expect("a day"));
    s.merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;
    clock.stating("2026-11-02".parse().expect("a day"));
    s.call(
        "rename_entity",
        json!({"handle": "document:alpha", "to": "document:marge"}),
    )
    .await;

    for name in ["document:marge", "document:alpha"] {
        assert_eq!(
            read_whole(&s, name, Some(OCTOBER)).await,
            format!("{ALPHA}{betas}"),
            "as of the merge day {name} holds both documents' sections"
        );
        assert_eq!(
            read_whole(&s, name, Some(SEPTEMBER)).await,
            ALPHA,
            "as of September {name} holds the survivor's own sections alone"
        );
    }
    story.finish().await;
}

/// **A restore sent under a name the document no longer wears puts it back as
/// the current name would.** Alpha and beta are written in August, beta is merged
/// into alpha in October and alpha is renamed in November. A restore to September
/// under the old name archives the sections that arrived by the merge, because
/// alpha did not hold them then. **Paired**: before the restore the document holds
/// both documents' sections, so the archive is the restore's doing.
#[tokio::test]
async fn a_restore_under_a_former_name_puts_the_document_back_as_it_stood() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:gamma", AUGUST).await;
    let s = story.session().await;
    s.add("document:alpha", "Alpha notes").await;
    s.add("document:beta", "Beta notes").await;
    s.call(
        "put_document",
        json!({"document": "document:alpha", "markdown": ALPHA}),
    )
    .await;
    let betas = "# Beta one\n\nb1\n\n# Beta two\n\nb2\n";
    s.call(
        "put_document",
        json!({"document": "document:beta", "markdown": betas}),
    )
    .await;
    clock.stating(OCTOBER.parse().expect("a day"));
    s.merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;
    clock.stating("2026-11-02".parse().expect("a day"));
    s.call(
        "rename_entity",
        json!({"handle": "document:alpha", "to": "document:marge"}),
    )
    .await;
    assert_eq!(
        read_whole(&s, "document:marge", None).await,
        format!("{ALPHA}{betas}"),
        "the merged sections stand before the restore"
    );

    let restored = s
        .call(
            "put_document",
            json!({"document": "document:alpha", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");
    assert_eq!(
        read_whole(&s, "document:marge", None).await,
        ALPHA,
        "the September page: {restored}"
    );
    story.finish().await;
}

const BETA_SHORT: &str =
    "# Beta one\n\nfirst body\n\n# Beta two\n\nsecond body\n\n# Beta three\n\nthird body\n";

/// **A document restored to a day before it absorbed another reads as it did
/// that day, whatever order keys the folded sections kept.** Beta had a section
/// archived before the merge, and that section keeps the order key it had on
/// beta. The restore must not bring it back among alpha's own.
#[tokio::test]
async fn a_restore_to_a_day_before_a_merge_does_not_bring_back_the_folded_archived_section() {
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
        json!({"document": "document:beta", "markdown": BETA_SHORT}),
    )
    .await;
    clock.stating(SEPTEMBER.parse().expect("a day"));
    let beta_two = records_of(&s, "document:beta")
        .await
        .into_iter()
        .find(|fact| fact["content"] == "# Beta two")
        .and_then(|fact| fact["address"].as_str().map(str::to_string))
        .expect("beta's second section");
    s.call(
        "update_fact",
        json!({"address": beta_two, "status": "archived"}),
    )
    .await;
    clock.stating(OCTOBER.parse().expect("a day"));
    s.merge_entities("document:beta", "document:alpha", "one manual twice", None)
        .await;
    // The positive the restore is read against: the sections that stood on beta
    // follow alpha's, and the one archived before the merge does not.
    let merged = read_whole(&s, "document:alpha", None).await;
    assert_eq!(
        merged,
        format!("{ALPHA}# Beta one\n\nfirst body\n\n# Beta three\n\nthird body\n"),
        "the folded sections that stand follow alpha's own"
    );
    // The case reaches the branch it is about: the archived section kept the key
    // it had on beta, and an active section of alpha holds the same one.
    let held = records_of(&s, "document:alpha").await;
    let key_of = |content: &str| {
        held.iter()
            .find(|fact| fact["content"] == content)
            .and_then(|fact| fact["fields"]["order"].as_str().map(str::to_string))
            .unwrap_or_else(|| panic!("a record {content} with an order key: {held:?}"))
    };
    assert_eq!(
        key_of("# Beta two"),
        key_of("# Alpha two"),
        "the archived folded section kept a key an active section holds"
    );
    // The document before the merge is alpha's own text, so a restore to it has
    // nothing of beta to bring back.
    assert_eq!(read_whole(&s, "document:alpha", Some(AUGUST)).await, ALPHA);
    let restored = s
        .call(
            "put_document",
            json!({"document": "document:alpha", "restore_to": AUGUST}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");
    assert_eq!(
        restored["restored"]["recreated"].as_array().map(Vec::len),
        Some(0),
        "no section is brought back: {restored}"
    );
    assert_eq!(read_whole(&s, "document:alpha", None).await, ALPHA);
    let after = records_of(&s, "document:alpha").await;
    let active_keys: Vec<&str> = after
        .iter()
        .filter(|fact| fact["status"] == "active" && fact["fields"]["order"].is_string())
        .filter_map(|fact| fact["fields"]["order"].as_str())
        .collect();
    let distinct: std::collections::BTreeSet<&str> = active_keys.iter().copied().collect();
    assert_eq!(
        active_keys.len(),
        distinct.len(),
        "no two active sections share an order key: {active_keys:?}"
    );
    story.finish().await;
}
