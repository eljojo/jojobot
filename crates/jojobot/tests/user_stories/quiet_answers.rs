//! "Why does every answer carry a dozen keys that say nothing?"
//!
//! **An answer says what is there, and a key with no value is not there.** A
//! claim recorded without a day, an entity with no parent, a boot with no
//! session yet: each used to arrive as `"key":null`, on every answer, for every
//! verb. An absent key says the same and costs nothing. The few nulls that mean
//! something stay, by name.
//!
//! The pin is over the ordinary answers of the ordinary verbs, because a
//! null-free recall proves nothing about a null-free capture.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_ordinary_answers_of_the_ordinary_verbs_carry_no_null_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // A write of each shape the verbs have: an entity, a claim, a correction.
    let made = s
        .call(
            "add_entity",
            json!({"kind": "person", "handle": "lisa", "name": "Lisa", "source": "user-named"}),
        )
        .await;
    let captured = s
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "likes the diner", "provenance": "testimony"}),
        )
        .await;
    let recalled = s
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await;
    let found = s.call("search", json!({"query": "diner"})).await;
    let listed = s.call("list_entities", json!({"kind": "person"})).await;

    for (verb, answer) in [
        ("add_entity", &made),
        ("capture", &captured),
        ("recall", &recalled),
        ("search", &found),
        ("list_entities", &listed),
    ] {
        answer.never_says(":null");
        // The positive each absence rests on: the answer is not empty.
        assert!(
            answer.size() > 40,
            "{verb} answered nothing: {}",
            answer.raw()
        );
    }
    s.wrap("looked at what a plain answer carries").await;

    // The claim is there and says what it holds; what it does not hold is not
    // spelled out.
    recalled.says("likes the diner");
    recalled.never_says("happened_at");

    story.finish().await;
}

/// "I read a lot before I wrote anything. Did I miss what the claims are for?"
/// You did not: the teaching about claims rides on the write verbs, so reading
/// does not spend it, and the first write after any number of reads carries it.
#[tokio::test]
async fn a_session_that_reads_first_is_taught_on_its_first_write() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    writer.add("person:lisa", "Lisa").await;
    writer.fact("person:lisa", "likes the diner").await;
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // ── reading teaches nothing about claims, however much of it there is ────
    reader
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await
        .says("likes the diner")
        .never_says("\"teaching\"");
    reader
        .call("search", json!({"query": "diner"}))
        .await
        .says("likes the diner")
        .never_says("\"teaching\"");

    // ── the first write carries it ───────────────────────────────────────────
    reader
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "orders the pie", "provenance": "testimony"}),
        )
        .await
        .says("\"teaching\"")
        .says("does not destroy the one already there");

    story.finish().await;
}

/// "I searched twice and it said the same long caveat twice." It says it once:
/// the caveat about what matching can miss rides the first search of a session
/// and any answer too thin to trust, and is left out of a second full answer.
#[tokio::test]
async fn a_second_full_search_leaves_the_caveats_out_and_an_empty_one_brings_them_back() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    for slug in ["alpha", "beta", "gamma"] {
        writer
            .add(&format!("place:{slug}"), &format!("The {slug} Diner"))
            .await;
    }
    writer.add("bot:sigma", "Sigma").await;
    let reader = story.as_bot("bot:sigma").await;

    // Three hits is a full answer; the first of the session carries the caveat.
    let first = reader.call("search", json!({"query": "diner"})).await;
    first.number("/count", 3).says("\"corpus\"");
    // The second does not, and still says what the query was.
    let second = reader.call("search", json!({"query": "diner"})).await;
    second.number("/count", 3).never_says("\"corpus\"");
    second.says("\"exact\"");
    // An empty answer is where the gap shows, so it brings the caveat back.
    let empty = reader
        .call("search", json!({"query": "zzzqxnomatch"}))
        .await;
    empty.number("/count", 0).says("\"corpus\"");

    story.finish().await;
}
