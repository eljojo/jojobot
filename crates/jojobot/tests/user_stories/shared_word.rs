//! "I searched for a place, found nothing, and made it. Was it already there?"
//!
//! **A search that finds nothing names the things that share a word with it.**
//! A caller who searched for a wharf road and got an empty answer created a
//! place three lines later, beside a wharf the store already held under another
//! name. The empty answer says what shares a word with the query, so the next
//! step is a question and not a guess. It only says: it never refuses, and an
//! answer with hits carries no such note.

use serde_json::json;

use super::dsl::{Answer, Story};

/// **What an empty search names**, read from the field it lives in: each thing
/// it says shares a word with the query, and the words shared. Empty when the
/// answer carries no note, which is not the same as a note naming nobody.
fn named(answer: &Answer) -> Vec<(String, Vec<String>)> {
    answer.json()["shares_a_word"]["things"]
        .as_array()
        .map(|things| {
            things
                .iter()
                .map(|thing| {
                    let shared = thing["shares"]
                        .as_array()
                        .map(|words| {
                            words
                                .iter()
                                .filter_map(|word| word.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    (thing["id"].as_str().unwrap_or_default().to_string(), shared)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the answer carries a `shares_a_word` note at all.
fn carries_the_note(answer: &Answer) -> bool {
    answer.json().get("shares_a_word").is_some()
}

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
    empty.number("/count", 0);
    assert_eq!(
        named(&empty),
        [("place:wonder-wharf".to_string(), vec!["wharf".to_string()])],
        "{}",
        empty.raw()
    );

    // ── a hit carries no such note ───────────────────────────────────────────
    let hit = reader.call("search", json!({"query": "wonder"})).await;
    hit.says("place:wonder-wharf");
    assert!(!carries_the_note(&hit), "{}", hit.raw());

    // ── an empty answer with no shared word names nothing ────────────────────
    let none = reader
        .call("search", json!({"query": "zzzqx nomatchatall"}))
        .await;
    none.number("/count", 0);
    assert!(!carries_the_note(&none), "{}", none.raw());

    // ── a kind that was asked for narrows what is named ──────────────────────
    let other_kind = reader
        .call(
            "search",
            json!({"query": "commute bridge wharf road", "kind": "person"}),
        )
        .await;
    other_kind.number("/count", 0);
    assert!(named(&other_kind).is_empty(), "{}", other_kind.raw());

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
    from_parent.number("/count", 0);
    assert!(named(&from_parent).is_empty(), "{}", from_parent.raw());
    // The positive the absence rests on: the same thing is named for a word of
    // its own.
    let own_word = reader
        .call(
            "search",
            json!({"query": "donut schedule rota budget", "kind": "work"}),
        )
        .await;
    own_word.number("/count", 0);
    assert_eq!(
        named(&own_word),
        [(
            "work:atlas-x-donut-stand".to_string(),
            vec!["donut".to_string()]
        )],
        "{}",
        own_word.raw()
    );

    story.finish().await;
}

/// "Half my works have 'with' in the name." A word many things of the kind
/// carry says nothing about whether a searched-for thing is already there, so
/// an empty search does not name them for it. A word one thing carries still
/// does.
#[tokio::test]
async fn an_empty_search_does_not_name_things_for_a_common_word() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    let crowd = [
        "alpha", "omega", "kappa", "sigma", "theta", "lambda", "epsilon", "iota",
    ];
    for slug in crowd {
        writer
            .add(&format!("work:{slug}"), &format!("{slug} with batter"))
            .await;
    }
    writer.add("work:tau", "Tau kwikmart").await;
    writer.add("bot:rho", "Rho").await;
    let reader = story.as_bot("bot:rho").await;

    // Eight works carry `with`; none is named for it.
    let common = reader
        .call(
            "search",
            json!({"query": "commute bridge subway with", "kind": "work"}),
        )
        .await;
    common.number("/count", 0);
    assert!(!carries_the_note(&common), "{}", common.raw());

    // The positive the absence rests on: the one work carrying `kwikmart` is.
    let rare = reader
        .call(
            "search",
            json!({"query": "commute bridge subway kwikmart", "kind": "work"}),
        )
        .await;
    rare.number("/count", 0);
    assert_eq!(
        named(&rare),
        [("work:tau".to_string(), vec!["kwikmart".to_string()])],
        "{}",
        rare.raw()
    );

    story.finish().await;
}
