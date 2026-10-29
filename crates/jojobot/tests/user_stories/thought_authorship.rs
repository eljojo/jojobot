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

/// **A cap set on a thing that is not a bot binds a bot's claim there.** Homer
/// asks that notes about him stay short: gamma's long connection claim about his
/// battery is refused, and its short one lands. The claims are read back as
/// claims, by their words, not found by searching the whole answer.
#[tokio::test]
async fn a_cap_set_on_a_person_binds_the_connection_claim_a_bot_writes_there() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:battery", "The Battery").await;
    s.call(
        "capture",
        json!({"subject": "person:homer", "content": "Homer wants notes about him kept short",
               "provenance": "testimony", "sets": {"thought_body_cap": "40"}}),
    )
    .await;
    let gamma = story.as_bot("bot:gamma").await;
    let claim = |content: &str| {
        json!({"subject": "person:homer", "content": content,
               "shape": "connection", "object": "thing:battery"})
    };
    let short = "Homer swapped the battery today";
    let long = "Homer swapped the battery in the smoke detector today and then forgot to put the cover back";
    assert!(short.chars().count() <= 40 && long.chars().count() > 40);

    gamma.refused("capture", claim(long)).await;
    gamma.call("capture", claim(short)).await;
    let read = s
        .call(
            "recall",
            json!({"subject": "person:homer", "facts": true, "status": "active"}),
        )
        .await
        .json();
    let words: Vec<&str> = read["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| panic!("the person's claims come back: {read}"))
        .iter()
        .filter_map(|fact| fact["content"].as_str())
        .collect();
    assert!(words.contains(&short), "the short claim landed: {words:?}");
    assert!(!words.contains(&long), "the long claim did not: {words:?}");
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

/// **A starred connection claim another bot writes on a bot is neither that bot's
/// rule nor its thought.** Milhouse has one seat, one starred rule of its own and
/// a room of one. Lisa's starred connection claim on Milhouse lands, rides no
/// boot, takes no seat and no place in the room: the boot still carries the one
/// rule, and Milhouse's own second thought is refused only because its own first
/// one fills the room.
#[tokio::test]
async fn a_starred_connection_claim_another_bot_writes_is_neither_the_bots_rule_nor_its_thought() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("bot:milhouse", "Milhouse"),
        ("bot:lisa", "Lisa"),
        ("thing:the-fern", "The Fern"),
        ("thing:the-couch", "The Couch"),
    ] {
        s.add(handle, name).await;
    }
    s.call(
        "capture",
        json!({"subject": "bot:milhouse", "content": "keep the ledger tidy",
               "provenance": "testimony", "fields": {"starred": "true"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "bot:milhouse", "content": "one seat and one thought",
               "sets": {"rule_seats": "1", "thought_capacity": "1"}}),
    )
    .await;
    let (milhouse, lisa) = (
        story.as_bot("bot:milhouse").await,
        story.as_bot("bot:lisa").await,
    );
    milhouse
        .call(
            "capture",
            json!({"subject": "bot:milhouse", "content": "the fern needs water",
                   "shape": "connection", "object": "thing:the-fern"}),
        )
        .await;
    lisa.call(
        "capture",
        json!({"subject": "bot:milhouse", "content": "lisa asks for the couch to be moved",
               "shape": "connection", "object": "thing:the-couch",
               "fields": {"starred": "true"}}),
    )
    .await;

    // The edit path is held to the same line: a star put on another bot's
    // connection claim by an edit lands as well.
    let moved = lisa
        .call(
            "capture",
            json!({"subject": "bot:milhouse", "content": "lisa asks for the fern to be moved",
                   "shape": "connection", "object": "thing:the-fern"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("the capture names its address")
        .to_string();
    lisa.call(
        "update_fact",
        json!({"address": moved, "fields": {"starred": "true"}}),
    )
    .await;

    let (boot, _) = story
        .call("start_here", json!({"bot": "milhouse", "brief": true}))
        .await;
    let rules = boot.json()["identity"]["rules"].to_string();
    assert!(rules.contains("ledger"), "the own rule is carried: {rules}");
    assert!(
        !rules.contains("couch"),
        "the other bot's claim is not a rule: {rules}"
    );
    assert!(
        !rules.contains("fern"),
        "a star put on the other bot's claim by an edit makes no rule either: {rules}"
    );
    s.call(
        "recall",
        json!({"subject": "bot:milhouse", "facts": true, "status": "active"}),
    )
    .await
    .says("lisa asks for the couch to be moved");
    // Milhouse's own room holds its one thought; a second of its own is refused,
    // and the reserve it names lands at two of one: the other bot's two starred
    // claims took no seat.
    let second = json!({"subject": "bot:milhouse", "content": "the couch needs a leg",
                        "shape": "connection", "object": "thing:the-couch"});
    milhouse
        .refused("capture", second.clone())
        .await
        .says("borrow");
    let mut borrowing = second;
    borrowing["borrow"] = json!(true);
    milhouse.call("capture", borrowing).await.says("2 of 1");
    story.finish().await;
}
