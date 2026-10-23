//! "See ordinance 2" in a message, a journal beat and a document.
//!
//! A numbered address (`@collection/N`) links the member that holds the number,
//! wherever the words are written. A claim already does; this story holds the
//! other text-taking writes to it: a posted message, a journal beat, a put
//! document and a restore. A number nobody holds is never linked.

use serde_json::json;

use super::dsl::{Session, Story};

const COLLECTION: &str = "topic:contract-ordinances-story";
const PARKING: &str = "decision:contract-ordinance-story-parking";
const AUGUST: &str = "2026-08-03";
const SEPTEMBER: &str = "2026-09-15";
const OCTOBER: &str = "2026-10-08";

/// A collection that numbers its members, holding one member numbered 2.
async fn ordinances(s: &Session) {
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    let (kind, slug) = PARKING.split_once(':').expect("a handle");
    s.call(
        "add_entity",
        json!({"kind": kind, "handle": slug, "name": "Parking", "source": "user-named",
               "parent": COLLECTION, "sets": {"number": "2"}}),
    )
    .await;
}

#[tokio::test]
async fn a_posted_message_naming_a_members_number_links_the_member() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;

    s.post(
        "assistant",
        "a note on parking",
        &format!("the fines are set by @{COLLECTION}/2 and nobody has read it"),
    )
    .await;
    s.call("search", json!({"query": "fines", "include_mail": true}))
        .await
        .says(PARKING)
        .never_says(&format!("{COLLECTION}/2"))
        .never_says("no such handle");

    // The paired miss: a number nobody holds is not linked to anything.
    s.post(
        "assistant",
        "a note on nothing",
        &format!("the permits are set by @{COLLECTION}/999 and nobody has read it"),
    )
    .await;
    s.call("search", json!({"query": "permits", "include_mail": true}))
        .await
        .never_says(PARKING);
    story.finish().await;
}

#[tokio::test]
async fn a_journal_beat_naming_a_members_number_links_the_member() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;

    s.journal(&format!("read @{COLLECTION}/2 and the fines are fair"))
        .await;
    s.call("wrap_session", json!({"story": "read the fines"}))
        .await
        .says(PARKING)
        .never_says(&format!("{COLLECTION}/2"))
        .never_says("no such handle");
}

#[tokio::test]
async fn a_document_section_naming_a_members_number_links_the_member_or_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;
    s.add("document:homer", "Fryer manual").await;

    s.call(
        "put_document",
        json!({"document": "document:homer",
               "markdown": format!("# Manual\n\nSee @{COLLECTION}/2 before parking.\n")}),
    )
    .await;
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let markdown = read["markdown"].as_str().unwrap_or_default().to_string();
    assert!(
        markdown.contains(PARKING) && !markdown.contains(&format!("{COLLECTION}/2")),
        "the section does not link the member: {read}"
    );

    let facts = s.recall("document:homer").await.json();
    let last = facts["objects"][0]["facts"]
        .as_array()
        .and_then(|all| all.last())
        .map(|fact| fact["address"].clone())
        .unwrap_or_else(|| panic!("the document's records: {facts}"));
    let refused = s
        .refused(
            "put_document",
            json!({"document": "document:homer",
                   "after": last,
                   "markdown": format!("## Permits\n\nSee @{COLLECTION}/999 first.\n")}),
        )
        .await;
    refused.says(COLLECTION).says("999");
    story.finish().await;
}

#[tokio::test]
async fn a_restore_of_a_section_naming_a_members_number_links_the_member() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:otto", AUGUST).await;
    let s = story.session().await;
    ordinances(&s).await;
    s.add("document:homer", "Fryer manual").await;
    s.call(
        "put_document",
        json!({"document": "document:homer",
               "markdown": format!("# Manual\n\n## Parking\n\nSee @{COLLECTION}/2 before parking.\n")}),
    )
    .await;
    let facts = s.recall_with_details("document:homer").await.json();
    let parking = facts["objects"][0]["facts"]
        .as_array()
        .expect("the document's records")
        .iter()
        .find(|fact| fact["details"].to_string().contains("before parking"))
        .unwrap_or_else(|| panic!("the section: {facts}"))["address"]
        .as_str()
        .expect("an address")
        .to_string();

    clock.stating(OCTOBER.parse().expect("a day"));
    s.call(
        "update_fact",
        json!({"address": parking, "status": "archived"}),
    )
    .await;

    let restored = s
        .call(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await
        .json();
    assert_ne!(restored["status"], "blocked", "{restored}");
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let markdown = read["markdown"].as_str().unwrap_or_default().to_string();
    assert!(
        markdown.contains(PARKING) && !markdown.contains(&format!("{COLLECTION}/2")),
        "the restored section does not link the member: {read}"
    );
}
