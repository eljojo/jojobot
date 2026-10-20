//! "Every link on my pages opens what it names."
//!
//! A key with a character a query string reads differently opens its own
//! history. A browser sent to the canonical path keeps what it asked. A page
//! whose read failed says it is blind, once and in plain words.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_key_with_reserved_characters_opens_its_own_history() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("topic:widgets", "Widgets").await;
    s.call(
        "capture",
        json!({
            "subject": "topic:widgets", "content": "what the stall took",
            "provenance": "testimony",
            "sets": {"cost+tax": "31", "a#b": "47", "plain": "5"},
        }),
    )
    .await;

    let page = story.page("topic:widgets").await;
    for (key, value, encoded) in [("cost+tax", "31", "%2B"), ("a#b", "47", "%23")] {
        let link = page.link_to(key);
        assert!(
            link.contains(encoded),
            "the link to {key:?} encodes it: {link}"
        );
        let opened = story.follow(&link).await;
        opened
            .says(&format!("Writes of {key}"))
            .says(&format!(">{value}<"));
        opened.never_says("Nothing has been written under this key");
    }
    // The positive the encoding must not disturb: a plain key is still plain.
    assert!(page.link_to("plain").ends_with("?history=plain"));
    story.finish().await;
}

#[tokio::test]
async fn the_canonical_redirect_keeps_the_query() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "topic:widgets", "Widgets")
        .await;

    let wanted = story
        .redirect_of("/topic:widgets/?history=takings")
        .await
        .expect("a handle reached off its path is sent to where it lives");
    assert!(
        wanted.starts_with("/person:homer/topic:widgets"),
        "the canonical path: {wanted}"
    );
    assert!(
        wanted.ends_with("?history=takings"),
        "the question the browser asked goes with it: {wanted}"
    );
    // The positive: a path already canonical is answered, not redirected.
    assert_eq!(
        story
            .redirect_of("/person:homer/topic:widgets/?history=takings")
            .await,
        None
    );
    story.finish().await;
}
