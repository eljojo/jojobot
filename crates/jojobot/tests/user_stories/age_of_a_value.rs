//! "That sentence is from August. Is it still true?"
//!
//! A recall hands back what a thing holds, and a value written months ago reads
//! exactly like one written this morning. Every object says the day the store
//! last learned anything about it, and a key says its own day only where that
//! day differs from the object's. Nothing is printed where the two agree, so the
//! day that is printed is the one that means something.

use serde_json::json;

use super::dsl::Story;

const AUGUST: &str = "2026-08-03";
const OCTOBER: &str = "2026-10-08";

#[tokio::test]
async fn an_object_says_the_day_it_was_last_learned_about_and_a_stale_key_says_its_own() {
    let (story, clock) = Story::begin_on_a_store_clock_that_moves("bot:otto", AUGUST).await;
    let s = story.session().await;

    // August: the teapot is bought, green and large, and a second thing is
    // written down whole in one sitting.
    s.add("thing:teapot", "Teapot").await;
    s.event_with(
        "thing:teapot",
        "bought it",
        json!({"colour": "green", "size": "large"}),
        &[],
    )
    .await;
    s.add("thing:kettle", "Kettle").await;
    s.event_with(
        "thing:kettle",
        "bought it",
        json!({"colour": "red", "size": "small"}),
        &[],
    )
    .await;

    // October: the teapot is repainted. The size was last written in August.
    clock.stating(OCTOBER.parse().expect("a day"));
    s.event_with("thing:teapot", "repainted", json!({"colour": "blue"}), &[])
        .await;

    let teapot = s.recall("thing:teapot").await.json();
    let object = &teapot["objects"][0];
    assert_eq!(
        object["as_of"], OCTOBER,
        "the day it was last learned: {object}"
    );
    assert_eq!(
        object["fields_as_of"],
        json!({"size": AUGUST}),
        "only the key older than the thing says its own day: {object}"
    );

    // The pair: a thing whose keys were all written on one day says that day
    // once, and carries no map of days at all.
    let kettle = s.recall("thing:kettle").await.json();
    let object = &kettle["objects"][0];
    assert_eq!(object["as_of"], AUGUST, "{object}");
    assert!(
        object.get("fields_as_of").is_none(),
        "same-day keys carry nothing: {object}"
    );

    story.finish().await;
}
