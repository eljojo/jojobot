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
//! **And it shows the payoff.** `rename_entity` is now a verb a caller can
//! reach, so the case that watches a mention follow a thing after a rename no
//! longer has to live only in the shared contract — it is reachable through
//! the served surface, and below it is.

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

    // ⭐ **And what search holds is what a reader reads.** The query is a word
    // out of the claim, and what is asserted is the HANDLE inside the hit — so
    // an index holding the stored form instead would answer with a badge here
    // and fail. Asserting the query's own word back would pass either way.
    s.call("search", json!({"query": "driving"}))
        .await
        .says("pet:santas-little-helper");

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

/// **The payoff this file's own header used to say it could not show.** For
/// months the operator filed Shelbyville under a typo — `place:shelbyvile` —
/// and wrote about it from that name: a claim on the place itself, and a
/// separate claim on Milhouse mentioning it in prose, four months apart.
/// Correcting the typo has to reach both without anybody rewriting either
/// claim.
///
/// **Paired with a control**, on the same subject as the mention: a rename
/// that repointed every pointer on the page rather than the one it was asked
/// to move would pass every assertion above it by accident.
#[tokio::test]
async fn a_rename_reaches_months_of_mentions_written_under_the_old_name() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.add("place:shelbyvile", "Shelbyville").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("thing:handcart", "The Handcart").await;

    // January: a claim about the place itself, under the typo.
    s.call(
        "capture",
        json!({
            "subject": "place:shelbyvile",
            "content": "is where the operator's parents still live",
            "provenance": "testimony",
            "recorded_at": "2026-01-12",
        }),
    )
    .await;

    // April: a separate claim, on a different subject, mentioning the place
    // in prose — the shape a mention actually arrives in.
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "grew up in @place:shelbyvile",
            "provenance": "testimony",
            "recorded_at": "2026-04-19",
        }),
    )
    .await;

    // The control, on the same subject: a claim mentioning something the
    // rename below never touches.
    s.call(
        "capture",
        json!({
            "subject": "person:milhouse",
            "content": "rode @thing:handcart the same summer",
            "provenance": "testimony",
            "recorded_at": "2026-04-19",
        }),
    )
    .await;

    // The correction, months later — the typo is retired.
    s.call(
        "rename_entity",
        json!({
            "handle": "place:shelbyvile",
            "to": "place:shelbyville",
            "recorded_at": "2026-08-02",
        }),
    )
    .await;

    // **The old handle still resolves.** A caller who only ever knew it as
    // `place:shelbyvile` and tries to act on it again is refused — but the
    // refusal recognizes the staleness and names what it is called now,
    // never a bare miss that reads the same as a handle that never existed.
    s.refused(
        "rename_entity",
        json!({"handle": "place:shelbyvile", "to": "place:x"}),
    )
    .await
    .says("place:shelbyville");

    // **And it is the same thing.** The claim recorded in January, on the
    // place itself, is exactly where it was left — under its current name.
    s.recall("place:shelbyville")
        .await
        .says("is where the operator's parents still live");

    // **The stored mention, four months old, renders under the new name.**
    // Neither January's nor April's claim was rewritten; what changed is
    // what a read of the pointer renders back.
    let milhouse = s.recall("person:milhouse").await;
    milhouse
        .says("place:shelbyville")
        .never_says("place:shelbyvile");

    // **The control is untouched.**
    milhouse.says("thing:handcart");

    s.wrap("corrected a four-month-old typo; everything written under it still finds the place")
        .await;
}
