//! "Whose thought is it? A bot's own claim about itself is a thought; a claim
//! another bot wrote on it is not, and a claim a bot writes on anything that is
//! not a bot is held to what that thing set."
//!
//! One test says whether a connection claim is a thought of its home, and
//! every site that needs the answer asks it: the body cap, the room, the
//! writers of a thread and the receipt's reserve line. Each failure is paired
//! with its healthy positive in the same read.

use serde_json::json;

use super::dsl::Story;

const THREAD: &str = "thread:milhouse-moves";

/// **A cap set on a thing that is not a bot binds a bot's claim there.** A
/// six-character connection claim on a person who set a cap of five is refused
/// and a five-character one lands.
#[tokio::test]
async fn a_cap_set_on_a_person_binds_the_connection_claim_a_bot_writes_there() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:battery", "The Battery").await;
    s.call(
        "capture",
        json!({"subject": "person:homer", "content": "her notes are short",
               "provenance": "testimony", "sets": {"thought_body_cap": "5"}}),
    )
    .await;
    let gamma = story.as_bot("bot:gamma").await;
    let claim = |content: &str| {
        json!({"subject": "person:homer", "content": content,
               "shape": "connection", "object": "thing:battery"})
    };

    gamma.refused("capture", claim("sixsix")).await;
    gamma.call("capture", claim("five5")).await;
    let read = s
        .call(
            "recall",
            json!({"subject": "person:homer", "facts": true, "status": "active"}),
        )
        .await
        .json();
    let text = read.to_string();
    assert!(
        text.contains("five5"),
        "the five-character claim landed: {read}"
    );
    assert!(
        !text.contains("sixsix"),
        "the six-character claim did not: {read}"
    );
    story.finish().await;
}

/// **A claim another bot writes on a bot does not make that bot a writer of the
/// thread it points at.** Beta's claim on alpha pointing at the thread leaves the
/// thread with no writer, so a bot with no standing over anyone sets its room.
/// Alpha's own thought at the thread makes alpha a writer, and the same bot is
/// then refused.
#[tokio::test]
async fn a_claim_another_bot_writes_on_a_bot_makes_no_writer_of_the_thread_it_points_at() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("bot:alpha", "Alpha"),
        ("bot:beta", "Beta"),
        ("bot:gamma", "Gamma"),
    ] {
        s.add(handle, name).await;
    }
    s.add(THREAD, "Milhouse moves").await;
    let (alpha, beta, gamma) = (
        story.as_bot("bot:alpha").await,
        story.as_bot("bot:beta").await,
        story.as_bot("bot:gamma").await,
    );
    let points_at_the_thread = |on: &str, content: &str| json!({"subject": on, "content": content, "shape": "connection", "object": THREAD});

    beta.call(
        "journal",
        json!({"entry": "beta is up", "focus": "the move"}),
    )
    .await;
    // beta's claim on alpha: an ordinary claim, and no writer of the thread.
    beta.call(
        "capture",
        points_at_the_thread("bot:alpha", "the move is on for spring"),
    )
    .await;
    gamma
        .call(
            "update_entity",
            json!({"handle": THREAD, "sets": {"thought_capacity": "6"}}),
        )
        .await;
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"thought_capacity\":\"6\"");

    // alpha's own thought makes alpha a writer, and gamma is refused.
    alpha
        .call(
            "capture",
            points_at_the_thread("bot:alpha", "alpha will carry the boxes"),
        )
        .await;
    gamma
        .refused(
            "update_entity",
            json!({"handle": THREAD, "sets": {"thought_capacity": "7"}}),
        )
        .await
        .says("thought_capacity")
        .says("\"wrote\":false");
    s.call("recall", json!({"subject": THREAD}))
        .await
        .says("\"thought_capacity\":\"6\"");
    story.finish().await;
}

/// **A claim another bot writes on a bot whose room is over its ceiling gets no
/// reserve line in its receipt, and the bot's own claim does.** Beta has written
/// nothing but this claim, so a read of the claim cannot name its run: the receipt
/// decides from the caller's bot, the input the write used.
#[tokio::test]
async fn a_claim_another_bot_writes_on_an_over_full_room_is_not_told_it_used_the_reserve() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;
    for (handle, name) in [
        ("thing:battery", "The Battery"),
        ("thing:the-fern", "The Fern"),
        ("thing:the-couch", "The Couch"),
    ] {
        s.add(handle, name).await;
    }
    s.call(
        "capture",
        json!({"subject": "bot:alpha", "content": "capacity is one",
               "sets": {"thought_capacity": "1"}}),
    )
    .await;
    let (alpha, beta) = (
        story.as_bot("bot:alpha").await,
        story.as_bot("bot:beta").await,
    );
    let on_alpha = |object: &str, content: &str| {
        json!({"subject": "bot:alpha", "content": content,
               "shape": "connection", "object": object})
    };

    // alpha fills its room, then borrows: its own claim is told it used the reserve.
    alpha
        .call("capture", on_alpha("thing:battery", "the battery is flat"))
        .await;
    let mut borrowing = on_alpha("thing:the-fern", "the fern needs water");
    borrowing["borrow"] = json!(true);
    alpha
        .call("capture", borrowing)
        .await
        .says("emergency reserve");

    // beta's claim lands on the over-full room, and is told nothing of a reserve.
    beta.call(
        "capture",
        on_alpha("thing:the-couch", "the couch needs a leg"),
    )
    .await
    .never_says("emergency reserve");
    story.finish().await;
}
