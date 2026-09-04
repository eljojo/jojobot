//! "@person:milhouse rode @thing:handcart" — a sentence that names things.
//!
//! **An agent writes handles into prose because that is how it talks.** The
//! sentence is the claim, and the handles in it are meant as the things they
//! name rather than as words that happen to look like them.
//!
//! What this story shows is that the naming survives the sentence: a claim
//! reads back with every handle it was written with, and a mention of something
//! that is not there is refused rather than filed as a spelling nobody can
//! follow.
//!
//! ⚠️ **What it cannot show is the payoff.** The point of storing a pointer is
//! that a rename, a reparent or a retype leaves it working, and there is no
//! verb that does any of those — so the case that watches a mention follow a
//! thing lives in the shared contract, where a handle can be moved the way it
//! really moves: an edit outside jojobot. This story proves the capability is
//! reachable through the served surface; it does not prove what it is for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_writes_handles_into_a_sentence_and_reads_them_all_back() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;
    s.add("pet:santas-little-helper", "The Dog").await;
    s.add("place:shelbyville", "Shelbyville").await;
    s.add("thing:handcart", "The Handcart").await;

    // **Four mentions across four kinds in one claim**, which is how the
    // sentence was actually asked for: a mechanism that stopped at the first
    // would pass a case with one.
    let written = s
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "took @pet:santas-little-helper to @place:shelbyville \
                            on @thing:handcart with @person:milhouse driving",
                "provenance": "testimony",
            }),
        )
        .await;
    written.says("person:milhouse");

    // Every handle comes back, out of the read a later session makes.
    let read = s.recall("person:milhouse").await;
    for named in [
        "pet:santas-little-helper",
        "place:shelbyville",
        "thing:handcart",
    ] {
        read.says(named);
    }

    // ⭐ **And the claim is findable by the thing it names.** A session looking
    // for what happened to the dog searches its handle; the index reads what a
    // reader reads, so it is there.
    s.call("search", json!({"query": "santas-little-helper"}))
        .await
        .says("person:milhouse");

    s.wrap("recorded one outing, naming everyone who was on it")
        .await;
}

/// **A sentence that names something jojobot has never heard of is refused**,
/// with nothing written — the rule an edge's object already faces.
///
/// **Paired with the write that lands**, because a build refusing every claim
/// with an `@` in it would satisfy the first half and serve nobody.
#[tokio::test]
async fn a_sentence_naming_something_that_is_not_there_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;

    s.refused(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "went out with @person:nelson",
            "provenance": "testimony",
        }),
    )
    .await
    .says("person:nelson");

    // …and the same sentence lands once the thing it names is there.
    s.add("person:nelson", "Nelson").await;
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "went out with @person:nelson",
            "provenance": "testimony",
        }),
    )
    .await
    .says("person:milhouse");
    s.recall("person:milhouse").await.says("person:nelson");

    s.wrap("named somebody jojobot did not know, then introduced them")
        .await;
}
