//! "I wrote down a fact about a record the software ships, and searching for
//! its words found nothing."
//!
//! **A claim about a supplied record is found by its words exactly as a claim
//! about a stored one is**, right after the write and after a restart rebuilt
//! the index. The pair is the point: a build that found only the stored
//! person's claim would pass any case written about the person.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The addresses a search answer lists, in its order.
fn addresses(answer: &Value) -> Vec<String> {
    answer["results"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists no results: {answer}"))
        .iter()
        .filter_map(|hit| hit["address"].as_str().map(str::to_string))
        .collect()
}

/// How many times the view's own record is a hit for its name.
async fn record_hits(s: &Session) -> usize {
    s.call("search", json!({"query": "loops"})).await.json()["results"]
        .as_array()
        .expect("the answer lists results")
        .iter()
        .filter(|hit| hit["hit"] == "entity" && hit["id"] == "view:loops")
        .count()
}

async fn found(s: &Session, word: &str) -> Vec<String> {
    addresses(&s.call("search", json!({"query": word})).await.json())
}

#[tokio::test]
async fn a_claim_about_a_supplied_view_is_found_by_its_words_like_one_about_a_stored_person() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    let stored = s
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "keeps a quuxwort jar",
                   "provenance": "testimony"}),
        )
        .await
        .field("address");
    let supplied = s
        .call(
            "capture",
            json!({"subject": "view:loops", "content": "is read every morning, a zarbleflux habit",
                   "provenance": "testimony"}),
        )
        .await
        .field("address");

    assert_eq!(
        found(&s, "quuxwort").await,
        vec![stored.clone()],
        "stored, after the write"
    );
    assert_eq!(
        found(&s, "zarbleflux").await,
        vec![supplied.clone()],
        "supplied, after the write"
    );

    assert_eq!(
        record_hits(&s).await,
        1,
        "the supplied record, after the write"
    );

    let again = story.restarted().await;
    let t = again.session_in("UTC", Some("new")).await;
    assert_eq!(
        found(&t, "quuxwort").await,
        vec![stored],
        "stored, after a restart"
    );
    assert_eq!(
        found(&t, "zarbleflux").await,
        vec![supplied],
        "supplied, after a restart"
    );

    assert_eq!(
        record_hits(&t).await,
        1,
        "the supplied record, after a restart"
    );

    again.finish().await;
    story.finish().await;
}
