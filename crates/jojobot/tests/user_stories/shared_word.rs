//! "I searched for a place, found nothing, and made it. Was it already there?"
//!
//! **A search that finds nothing names the things that share a word with it.**
//! A caller who searched for a wharf road and got an empty answer created a
//! place three lines later, beside a wharf the store already held under another
//! name. The empty answer says what shares a word with the query, so the next
//! step is a question and not a guess. It only says: it never refuses, and an
//! answer with hits carries no such note.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn an_empty_search_names_the_place_that_shares_a_word_with_it() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("place:wonder-wharf", "Wonder Wharf").await;
    writer.add("person:lisa", "Lisa").await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // ── nothing matches the query, and one place shares a word with it ───────
    let empty = reader
        .call("search", json!({"query": "commute bridge wharf road"}))
        .await;
    empty
        .number("/count", 0)
        .says("shares_a_word")
        .says("place:wonder-wharf")
        .says("\"wharf\"");

    // ── a hit carries no such note ───────────────────────────────────────────
    let hit = reader.call("search", json!({"query": "wonder"})).await;
    hit.says("place:wonder-wharf").never_says("shares_a_word");

    // ── an empty answer with no shared word names nothing ────────────────────
    let none = reader
        .call("search", json!({"query": "zzzqx nomatchatall"}))
        .await;
    none.number("/count", 0).never_says("shares_a_word");

    // ── a kind that was asked for narrows what is named ──────────────────────
    let other_kind = reader
        .call(
            "search",
            json!({"query": "commute bridge wharf road", "kind": "person"}),
        )
        .await;
    other_kind
        .number("/count", 0)
        .never_says("place:wonder-wharf");

    story.finish().await;
}

/// "Everything under the atlas project has the word atlas in it." A word a
/// thing wears only because of the project it sits under says nothing about
/// whether two things are the same, so an empty search does not name it.
#[tokio::test]
async fn the_word_a_thing_takes_from_its_parent_is_not_shared_evidence() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("project:atlas", "Atlas").await;
    writer
        .add_under("project:atlas", "work:atlas-x-donut-stand", "Donut Stand")
        .await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // Among works the only candidate wears `atlas` from its parent.
    let from_parent = reader
        .call(
            "search",
            json!({"query": "atlas schedule rota budget", "kind": "work"}),
        )
        .await;
    from_parent
        .number("/count", 0)
        .never_says("work:atlas-x-donut-stand");
    // The positive the absence rests on: the same thing is named for a word of
    // its own.
    let own_word = reader
        .call(
            "search",
            json!({"query": "donut schedule rota budget", "kind": "work"}),
        )
        .await;
    own_word
        .number("/count", 0)
        .says("work:atlas-x-donut-stand");

    story.finish().await;
}
