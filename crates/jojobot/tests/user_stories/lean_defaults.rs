//! "I read a thread and half of what came back was paragraphs I never asked for.
//! I wanted the list of what was said."
//!
//! **A listing leads with each claim.** A standard records read serves every
//! record's one-line claim with its address, who backs it and how sure anyone is.
//! The details under a claim are a paragraph of reasons, and they come only when
//! the call asks (`details: true`). The answer says once how many it left out and
//! how to ask, so a reader never has to infer withheld from absent. The operator's
//! page does the same in its own way: the claim first, the details behind a ▸.

use serde_json::json;

use super::dsl::{Session, Story};

const TOPIC: &str = "topic:milhouse-moves";

/// Three claims on one topic: two carry details, one carries none.
async fn a_topic_with_two_paragraphs(s: &Session) {
    s.add(TOPIC, "Milhouse moves").await;
    for (content, details) in [
        (
            "the first quote came in",
            Some("it was higher than the last one"),
        ),
        ("the second quote came in", None),
        (
            "the third quote came in",
            Some("the mover said the price would hold"),
        ),
    ] {
        let mut args = json!({
            "subject": TOPIC, "content": content, "provenance": "testimony",
        });
        if let Some(details) = details {
            args["details"] = details.into();
        }
        s.call("capture", args).await;
    }
}

#[tokio::test]
async fn a_records_read_leads_with_each_claim_and_says_how_many_details_it_left_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_topic_with_two_paragraphs(&s).await;

    // ── the standard read: claims, addresses, provenance and standing ────────
    let read = s
        .call("recall", json!({"subject": TOPIC, "facts": true}))
        .await;
    read.says("the first quote came in")
        .says("the third quote came in")
        .never_says("higher than the last one")
        .never_says("the price would hold");
    let facts = read.json()["objects"][0]["facts"]
        .as_array()
        .expect("the records")
        .clone();
    assert_eq!(facts.len(), 3, "{}", read.raw());
    for fact in &facts {
        assert!(fact["address"].is_string(), "{fact}");
        assert_eq!(fact["provenance"], "testimony", "{fact}");
        assert!(fact["standing"].is_string(), "{fact}");
        assert!(
            fact.get("details").is_none(),
            "a listing carries no details unasked: {fact}"
        );
    }
    // ── said once, with the count and how to ask ────────────────────────────
    let left_out = &read.json()["details_left_out"];
    assert_eq!(left_out["count"], 2, "{}", read.raw());
    assert!(
        left_out["how_to_ask"]
            .as_str()
            .is_some_and(|how| how.contains("details: true")),
        "{left_out}"
    );

    // ── asked for: the same records, with their details, and no line ────────
    let asked = s
        .call(
            "recall",
            json!({"subject": TOPIC, "facts": true, "details": true}),
        )
        .await;
    asked
        .says("it was higher than the last one")
        .says("the mover said the price would hold")
        .never_says("details_left_out");
    let with_details = asked.json()["objects"][0]["facts"]
        .as_array()
        .expect("the records")
        .clone();
    assert_eq!(with_details.len(), 3);
    for (lean, full) in facts.iter().zip(&with_details) {
        for key in ["address", "content", "provenance", "standing"] {
            assert_eq!(lean[key], full[key], "{key} is the same either way");
        }
    }

    // ── a read with nothing to leave out carries no line ────────────────────
    s.add("topic:marge-baking", "Marge baking").await;
    s.call(
        "capture",
        json!({"subject": "topic:marge-baking", "content": "rises overnight",
               "provenance": "testimony"}),
    )
    .await;
    s.call(
        "recall",
        json!({"subject": "topic:marge-baking", "facts": true}),
    )
    .await
    .never_says("details_left_out");
    story.finish().await;
}

#[tokio::test]
async fn reading_records_by_address_and_searching_follow_the_same_default() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_topic_with_two_paragraphs(&s).await;
    let first = format!("{TOPIC}#f1");
    let second = format!("{TOPIC}#f2");

    // ── records named by address ────────────────────────────────────────────
    let lean = s
        .call("recall", json!({"records": [&first, &second]}))
        .await;
    lean.says("the first quote came in")
        .never_says("higher than the last one");
    assert_eq!(
        lean.json()["details_left_out"]["count"],
        1,
        "{}",
        lean.raw()
    );
    s.call(
        "recall",
        json!({"records": [&first, &second], "details": true}),
    )
    .await
    .says("higher than the last one")
    .never_says("details_left_out");

    // ── a search hit on a claim ─────────────────────────────────────────────
    let found = s.call("search", json!({"query": "quote"})).await;
    found
        .says("the first quote came in")
        .never_says("higher than the last one");
    assert_eq!(
        found.json()["details_left_out"]["count"],
        2,
        "{}",
        found.raw()
    );
    s.call("search", json!({"query": "quote", "details": true}))
        .await
        .says("higher than the last one")
        .says("the mover said the price would hold")
        .never_says("details_left_out");
    story.finish().await;
}

#[tokio::test]
async fn a_page_shows_the_claim_and_puts_its_details_behind_a_marker() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(TOPIC, "Milhouse moves").await;
    s.call(
        "capture",
        json!({
            "subject": TOPIC, "content": "the first quote came in",
            "details": "it was <b>higher</b> than the last one", "provenance": "testimony",
        }),
    )
    .await;
    let page = story.page(TOPIC).await;
    let html = page.raw();
    let claim = html
        .find("the first quote came in")
        .expect("the page leads with the claim");
    // The ▸ is the stylesheet's: the fold's summary is bare and the same page
    // carries the rule that draws the marker on it.
    let marker = html
        .find("<details><summary></summary><small>")
        .expect("the details sit behind a marker");
    assert!(claim < marker, "the claim comes before its details: {html}");
    assert!(
        html.contains("details > summary::before { content: \"▸ \"; }"),
        "the page draws the marker: {html}"
    );
    // Escaped: what a person typed never becomes markup.
    assert!(!html.contains("higher</b>"), "{html}");
    assert!(html.contains("&lt;b&gt;higher&lt;/b&gt;"), "{html}");
    story.finish().await;
}

/// **The entity keys a read serves are untouched by the records default**, and
/// the question a creation is asked about a thing the session was shown still
/// fires after a default read of it.
#[tokio::test]
async fn a_default_read_still_shows_what_a_creation_is_asked_about() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("place:contract-wonder-wharf", "Wonder Wharf").await;
    s.call(
        "capture",
        json!({"subject": "place:contract-wonder-wharf", "content": "closes at dusk",
               "details": "a sign on the gate says so", "provenance": "testimony"}),
    )
    .await;
    // A default read of the place: no details, and the entity keys intact.
    let read = s
        .call(
            "recall",
            json!({"subject": "place:contract-wonder-wharf", "facts": true}),
        )
        .await;
    read.never_says("a sign on the gate says so");
    let object = &read.json()["objects"][0];
    assert_eq!(
        object["id"],
        "place:contract-wonder-wharf",
        "{}",
        read.raw()
    );
    assert_eq!(object["name"], "Wonder Wharf", "{}", read.raw());
    // The creation beside it is asked about it.
    s.call(
        "add_entity",
        json!({"kind": "place", "handle": "contract-wonder-road", "name": "Wonder Road",
               "source": "user-named"}),
    )
    .await
    .says("merge_entities");
    story.finish().await;
}
