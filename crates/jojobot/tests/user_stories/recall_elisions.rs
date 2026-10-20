//! "Did that read leave anything out, and how do I get it?"
//!
//! **A recall that leaves prose out says so once, in the answer.** A page's
//! prose is the human half of a thing and is off by default, because a page is
//! bigger than a claim. Facts, the charter, backing, stood_for and review marks
//! each say what they left out; prose said nothing, so a reader of a run saw no
//! sign that its chronology was there to read. The note counts the objects it
//! left prose out of and names the argument, once per answer and never per
//! object, and only when an object in the answer has prose to leave out.
//!
//! The roster names are the cast: people, a run and a bot.

use jojobot_domain::memory::EntityId;
use serde_json::{Value, json};

use super::dsl::Story;

/// The note an answer carries when it left prose out, and none when it did not.
fn note(read: &Value) -> Option<&Value> {
    read.get("prose_left_out")
}

/// **A run has a chronology, so a read of runs without prose says once that it
/// left prose out.** Asked for, the prose comes back and the note does not.
#[tokio::test]
async fn a_read_of_runs_says_once_that_it_left_their_prose_out_and_how_to_ask() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;

    let read = s.call("recall", json!({"kind": "session"})).await.json();
    let left_out =
        note(&read).unwrap_or_else(|| panic!("the answer says nothing of prose: {read}"));
    assert!(left_out["count"].as_u64().unwrap_or(0) >= 1, "{read}");
    assert!(
        left_out["how_to_ask"]
            .as_str()
            .is_some_and(|how| how.contains("prose: true")),
        "the note names the argument: {read}"
    );
    for object in read["objects"].as_array().expect("objects") {
        assert!(
            object.get("prose_left_out").is_none() && object.get("_prose_left_out").is_none(),
            "the note rides the answer once, not each object: {object}"
        );
    }

    let asked = s
        .call("recall", json!({"kind": "session", "prose": true}))
        .await
        .json();
    assert!(note(&asked).is_none(), "prose was asked for: {asked}");
    assert!(asked["objects"][0]["prose"].is_string(), "{asked}");
    story.finish().await;
}

/// **Things with no prose say nothing about it.** Three people hold claims and
/// no page, so there is nothing left out to report.
#[tokio::test]
async fn a_read_of_things_with_no_prose_says_nothing_of_prose() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("person:homer", "Homer"),
        ("person:lisa", "Lisa"),
        ("person:bart", "Bart"),
    ] {
        s.add(handle, name).await;
        s.call(
            "capture",
            json!({"subject": handle, "content": "lives on Evergreen Terrace",
                   "provenance": "testimony"}),
        )
        .await;
    }
    let read = s
        .call("recall", json!({"kind": "person", "facts": true}))
        .await
        .json();
    assert_eq!(read["count"].as_u64(), Some(3), "{read}");
    assert!(note(&read).is_none(), "{read}");
    story.finish().await;
}

/// **A bot's page is its charter, and the charter already says it was left
/// out**, so the prose note does not repeat it.
#[tokio::test]
async fn a_bots_charter_note_is_not_said_twice() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    s.call(
        "set_charter",
        json!({"bot": "bot:delta", "prose": "Written for delta."}),
    )
    .await;
    let read = s
        .call("recall", json!({"subject": "bot:delta"}))
        .await
        .json();
    assert_eq!(read["objects"][0]["charter_elided"], true, "{read}");
    assert!(note(&read).is_none(), "{read}");
    story.finish().await;
}

/// **A thing's page is its prose**, written beneath the server because no
/// served verb writes the page of a thing that is not a bot. A store a
/// deployed build filled holds such pages.
async fn with_a_page(story: &Story, handle: &str, page: &str) {
    story
        .memory_store()
        .set_prose(&EntityId(handle.into()), page, None)
        .await
        .expect("the page is set");
    story.the_store_changed_underneath().await;
}

/// **A thing with a page, read without prose, says once that its prose was
/// left out and how to ask.** Asked for, the page comes back and the note
/// does not.
#[tokio::test]
async fn a_read_of_a_thing_with_a_page_says_its_prose_was_left_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    with_a_page(&story, "person:milhouse", "Milhouse lives next door.").await;

    let read = s
        .call("recall", json!({"subject": "person:milhouse"}))
        .await
        .json();
    let left_out =
        note(&read).unwrap_or_else(|| panic!("the answer says nothing of prose: {read}"));
    assert_eq!(left_out["count"].as_u64(), Some(1), "{read}");
    assert!(
        left_out["how_to_ask"]
            .as_str()
            .is_some_and(|how| how.contains("prose: true")),
        "the note names the argument: {read}"
    );
    assert!(read["objects"][0].get("prose").is_none(), "{read}");
    assert!(
        read["objects"][0].get("_prose_left_out").is_none(),
        "the private mark leaves the answer: {read}"
    );

    let asked = s
        .call(
            "recall",
            json!({"subject": "person:milhouse", "prose": true}),
        )
        .await
        .json();
    assert!(note(&asked).is_none(), "prose was asked for: {asked}");
    assert_eq!(
        asked["objects"][0]["prose"].as_str(),
        Some("Milhouse lives next door."),
        "{asked}"
    );
    story.finish().await;
}

/// **A read of several things counts only the ones whose page it left out.**
/// Two people have a page and one has none, so the one note counts two; the
/// thing with no page says nothing of prose on its own.
#[tokio::test]
async fn a_kind_read_counts_only_the_pages_it_left_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("person:milhouse", "Milhouse"),
        ("person:lisa", "Lisa"),
        ("person:nelson", "Nelson"),
    ] {
        s.add(handle, name).await;
    }
    with_a_page(&story, "person:milhouse", "Milhouse lives next door.").await;
    with_a_page(&story, "person:lisa", "Lisa plays the saxophone.").await;

    let read = s.call("recall", json!({"kind": "person"})).await.json();
    assert_eq!(read["count"].as_u64(), Some(3), "{read}");
    let left_out =
        note(&read).unwrap_or_else(|| panic!("the answer says nothing of prose: {read}"));
    assert_eq!(
        left_out["count"].as_u64(),
        Some(2),
        "the person with no page is not counted: {read}"
    );
    for object in read["objects"].as_array().expect("objects") {
        assert!(
            object.get("prose_left_out").is_none() && object.get("_prose_left_out").is_none(),
            "the note rides the answer once, not each object: {object}"
        );
    }

    let alone = s
        .call("recall", json!({"subject": "person:nelson"}))
        .await
        .json();
    assert_eq!(alone["objects"][0]["id"], "person:nelson", "{alone}");
    assert!(note(&alone).is_none(), "nelson has no page: {alone}");
    story.finish().await;
}
