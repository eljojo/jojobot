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
    // **A delivery renders it too**, and a number nobody holds reads as unresolved
    // where its collection numbers, so nothing follows it to the collection.
    s.post(
        "otto",
        "a note to self",
        &format!("the tickets are set by @{COLLECTION}/999 and nobody has read it"),
    )
    .await;
    s.call("read_mailbox", json!({}))
        .await
        .says(&format!("{COLLECTION}/999 (no such handle)"));
    story.finish().await;
}

#[tokio::test]
async fn a_journal_beat_naming_a_members_number_links_the_member() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;

    s.journal(&format!("read @{COLLECTION}/2 and the fines are fair"))
        .await;
    s.journal(&format!("looked for @{COLLECTION}/999 and found nothing"))
        .await;
    s.call("wrap_session", json!({"story": "read the fines"}))
        .await
        .says(&format!("{COLLECTION}/999 (no such handle)"))
        .says(PARKING)
        .never_says(&format!("{COLLECTION}/2"))
        .never_says(&format!("{PARKING} (no such handle)"));
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

/// **A reason cannot carry an unknown stored mark**: a mark whose badge nobody
/// wears remains an unchecked link, as in every other text a caller writes.
#[tokio::test]
async fn an_archive_reason_carrying_the_stored_mark_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.refused(
        "archive_entity",
        json!({"handle": "thing:kettle", "reason": "see @#k7h2mn"}),
    )
    .await;
    // Paired with the reason that carries none, which archives.
    s.call(
        "archive_entity",
        json!({"handle": "thing:kettle", "reason": "replaced"}),
    )
    .await;
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

/// **A slash and a number after a thing that numbers nothing is ordinary prose.**
/// "met @thing:kettle/2 today" links the kettle and keeps `/2` as words, in every
/// text a write takes; only a collection that numbers reads `/N` as an address.
/// Paired with the numbering collection above, where `/999` is refused.
#[tokio::test]
async fn a_slash_number_after_a_thing_that_numbers_nothing_is_words_in_every_text() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.add("document:homer", "Fryer manual").await;

    s.call(
        "capture",
        json!({"subject": "thing:kettle", "provenance": "testimony",
               "content": "met @thing:kettle/2 today"}),
    )
    .await;
    s.recall("thing:kettle")
        .await
        .says("@thing:kettle/2 today")
        .never_says("no such handle");
    // **A slash and digits that are no whole number are words as well**: a
    // leading zero, a zero, a number too long to count.
    for shape in ["007", "0", "99999999999999999999999"] {
        s.call(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("met @thing:kettle/{shape} today")}),
        )
        .await;
    }
    s.recall("thing:kettle")
        .await
        .says("@thing:kettle/007 today")
        .says("@thing:kettle/0 today")
        .says("@thing:kettle/99999999999999999999999 today")
        .never_says("no such handle");

    s.post(
        "assistant",
        "a note",
        "left a lid on @thing:kettle/2 for tea",
    )
    .await;
    s.call("search", json!({"query": "tea", "include_mail": true}))
        .await
        .says("@thing:kettle/2")
        .never_says("no such handle");

    s.journal("looked at @thing:kettle/2 again").await;
    s.call(
        "put_document",
        json!({"document": "document:homer",
               "markdown": "# Manual\n\nSee @thing:kettle/2 for tea.\n"}),
    )
    .await;
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let markdown = read["markdown"].as_str().unwrap_or_default();
    assert!(
        markdown.contains("@thing:kettle/2 for tea") && !markdown.contains("no such handle"),
        "{read}"
    );
    // **Linked, not kept as typed**: a rename of the thing reaches the words.
    s.call(
        "rename_entity",
        json!({"handle": "thing:kettle", "to": "thing:teapot"}),
    )
    .await;
    s.recall("thing:teapot")
        .await
        .says("@thing:teapot/2 today")
        .never_says("thing:kettle/2");
    s.call("search", json!({"query": "tea", "include_mail": true}))
        .await
        .says("@thing:teapot/2 ")
        .never_says("thing:kettle/2");
    s.call("wrap_session", json!({"story": "looked at the kettle"}))
        .await
        .says("@thing:teapot/2 again")
        .never_says("no such handle");
}

/// **A reason follows the thing it names through a rename**, for a numbered
/// address as for any handle: the reason an entity was archived, and the reason a
/// thought was dropped. Paired with the member still reading under the old
/// handle before the rename.
#[tokio::test]
async fn a_reason_naming_a_members_number_follows_it_through_a_rename() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;
    s.add("thing:kettle", "Kettle").await;
    s.call(
        "archive_entity",
        json!({"handle": "thing:kettle", "reason": format!("replaced under @{COLLECTION}/2")}),
    )
    .await;
    s.recall("thing:kettle").await.says(PARKING);

    s.add("bot:gamma", "Gamma").await;
    s.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "capacity is one",
               "sets": {"thought_capacity": "1"}}),
    )
    .await;
    let gamma = story.as_bot("bot:gamma").await;
    s.add("thing:jukebox", "The Jukebox").await;
    s.add("thing:battery", "The Battery").await;
    let first = gamma
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "the jukebox needs a needle",
                   "shape": "connection", "object": "thing:jukebox"}),
        )
        .await
        .field("address");
    gamma
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "the battery needs replacing",
                   "shape": "connection", "object": "thing:battery",
                   "drop": &first,
                   "drop_because": format!("see @{COLLECTION}/2 instead")}),
        )
        .await;
    s.recall_with_details("bot:gamma").await.says(PARKING);

    s.call(
        "rename_entity",
        json!({"handle": PARKING, "to": "decision:contract-ordinance-story-curfew"}),
    )
    .await;
    let renamed = "decision:contract-ordinance-story-curfew";
    s.recall("thing:kettle")
        .await
        .says(renamed)
        .never_says(PARKING);
    s.recall_with_details("bot:gamma")
        .await
        .says(renamed)
        .never_says(PARKING);
    story.finish().await;
}

/// **A slash and a number after a thing that numbers nothing does not move the
/// graph.** The mention is the thing, so a walk in from the thing finds the claim
/// that names it and a walk out from the claim reaches the thing, and neither
/// reads a thing that does not exist.
#[tokio::test]
async fn a_slash_number_after_a_thing_that_numbers_nothing_walks_to_the_thing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.add("thing:teapot", "Teapot").await;
    s.call(
        "capture",
        json!({"subject": "thing:teapot", "provenance": "testimony",
               "content": "met @thing:kettle/2 today"}),
    )
    .await;
    s.shape(
        "what mentions the kettle",
        json!({"subject": "thing:kettle", "follow": {"direction": "in"}}),
    )
    .await
    .says("thing:teapot")
    .says("\"mention\":true")
    .never_says("thing:kettle/2");
    s.shape(
        "what the teapot's claim mentions",
        json!({"subject": "thing:teapot", "follow": {"direction": "out"}}),
    )
    .await
    .says("thing:kettle")
    .never_says("thing:kettle/2");
    story.finish().await;
}

/// **A blank `numbers_by` numbers nothing**, the way the domain reads it: a slash
/// and a number after such a thing is words, as it is after a thing that carries
/// no key at all. Paired with the key that is set, in the stories above.
#[tokio::test]
async fn a_blank_numbering_key_numbers_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.add("thing:teapot", "Teapot").await;
    for (blank, name) in [("", "empty"), ("  ", "spaces")] {
        s.call(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("{name} key"), "sets": {"numbers_by": blank}}),
        )
        .await;
        s.call(
            "capture",
            json!({"subject": "thing:teapot", "provenance": "testimony",
                   "content": format!("met @thing:kettle/2 with a {name} key")}),
        )
        .await;
    }
    s.recall("thing:teapot")
        .await
        .says("@thing:kettle/2 with a empty key")
        .says("@thing:kettle/2 with a spaces key")
        .never_says("no such handle");
    story.finish().await;
}

/// **A restore is screened before anything is written**, and the screen reads a
/// number as an address once its collection numbers. A section written while the
/// collection numbered nothing keeps `/7` as words; after the collection starts to
/// number and nobody holds 7, putting that history back is refused as naming
/// nothing, and the section that comes first in the restore is not touched either.
#[tokio::test]
async fn a_restore_of_history_naming_a_number_nobody_holds_is_refused_whole() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:otto", AUGUST).await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.add("document:homer", "Fryer manual").await;
    s.call(
        "put_document",
        json!({"document": "document:homer",
               "markdown": format!("# Manual\n\n## Opening\n\nSwitch it on at six.\n\n## Parking\n\nSee @{COLLECTION}/7 before parking.\n")}),
    )
    .await;
    let facts = s.recall_with_details("document:homer").await.json();
    let address_of = |needle: &str| {
        facts["objects"][0]["facts"]
            .as_array()
            .expect("the document's records")
            .iter()
            .find(|fact| fact["details"].to_string().contains(needle))
            .unwrap_or_else(|| panic!("the section {needle}: {facts}"))["address"]
            .as_str()
            .expect("an address")
            .to_string()
    };
    let (opening, parking) = (address_of("at six"), address_of("before parking"));

    clock.stating(OCTOBER.parse().expect("a day"));
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "update_fact",
        json!({"address": opening, "field": "details", "find": "at six", "replace": "at five"}),
    )
    .await;
    s.call(
        "update_fact",
        json!({"address": parking, "status": "archived"}),
    )
    .await;

    let refused = s
        .refused(
            "put_document",
            json!({"document": "document:homer", "restore_to": SEPTEMBER}),
        )
        .await;
    refused.says(COLLECTION).says("7");
    // Nothing landed: the opening still reads as it was edited.
    s.call("read_document", json!({"document": "document:homer"}))
        .await
        .says("at five")
        .never_says("at six");
    story.finish().await;
}

/// **A refused number offers real things only.** The roster carries a marker for
/// each collection that numbers, and the candidates of a miss come from the
/// things that exist: the collection is offered, the marker is not.
#[tokio::test]
async fn a_refused_number_offers_real_candidates_only() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    ordinances(&s).await;
    s.add("thing:kettle", "Kettle").await;
    let refused = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("parked under @{COLLECTION}/9")}),
        )
        .await;
    let candidates = refused.json()["candidates"].to_string();
    assert!(
        candidates.contains(COLLECTION) && !candidates.contains("/#"),
        "the candidates of a miss: {candidates}"
    );
    story.finish().await;
}

/// **The page links the handle before digits that are no number**, whole through
/// the web page of the claim's thing: the anchor is the thing's own page and the
/// digits stay words beside it. Paired with the claim before them, whose
/// whole-number spelling reads the same on the page.
#[tokio::test]
async fn a_page_links_the_handle_before_digits_that_are_no_number() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.add("thing:teapot", "Teapot").await;
    for digits in ["2", "007", "0"] {
        s.call(
            "capture",
            json!({"subject": "thing:teapot", "provenance": "testimony",
                   "content": format!("met @thing:kettle/{digits} today")}),
        )
        .await;
    }
    let page = story.page("thing:teapot").await;
    let page = page.raw();
    for digits in ["007", "0"] {
        assert!(
            page.contains(&format!(">@thing:kettle</a>/{digits} today")),
            "the handle before /{digits} does not link: {page}"
        );
    }
    assert!(page.contains(">@thing:kettle/2</a> today"), "{page}");
    story.finish().await;
}
