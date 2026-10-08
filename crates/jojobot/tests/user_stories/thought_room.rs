//! "How many live thoughts is Gamma even carrying right now — and what
//! happens when it runs out of room?"
//!
//! A bot's own room is bounded on purpose: `thought_capacity` is a ceiling
//! nobody can set on their own identity, so an ordinary write that would
//! push a full room over it is refused rather than growing without limit.
//! Two ways past a full room, never silent about which one was used: the
//! emergency reserve, spent once and visibly over the ceiling until
//! something is archived to repay it; and naming a thought to drop, which
//! frees the slot the new one takes and costs nothing when the room was not
//! full to begin with.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_full_room_borrows_once_and_a_drop_frees_a_slot() {
    let story = Story::begin("bot:otto").await;

    // ── the emergency reserve, on Gamma's room ───────────────────────────────
    let s = story.session().await;

    s.add("bot:gamma", "Gamma").await;
    // Gamma cannot set this on itself — otto is asking, about a different
    // identity, which is what makes the write land at all.
    s.call(
        "capture",
        json!({
            "subject": "bot:gamma",
            "content": "capacity is one",
            "fields": {"thought_capacity": "1"},
        }),
    )
    .await;

    // 🚨 **The negative `borrow` rests on**: the room holds nothing yet, so
    // naming `borrow: true` here costs nothing — it lands like an ordinary
    // write, with no debt and no mention of the reserve.
    s.add("thing:jukebox", "The Jukebox").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:gamma",
            "content": "the jukebox needs a needle",
            "shape": "connection",
            "object": "thing:jukebox",
            "borrow": true,
        }),
    )
    .await
    .never_says("emergency reserve");

    // The room holds one, capacity is one: an ordinary write is refused, and
    // the way forward names the reserve.
    s.add("thing:battery", "The Battery").await;
    let full = s
        .refused(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "the battery needs replacing",
                "shape": "connection",
                "object": "thing:battery",
            }),
        )
        .await;
    full.says("borrow");

    // Spending it lands the write anyway, visibly over the ceiling.
    let borrowed = s
        .call(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "the battery needs replacing",
                "shape": "connection",
                "object": "thing:battery",
                "borrow": true,
            }),
        )
        .await;
    borrowed.says("2 of 1");

    // The debt is outstanding: a second borrow is refused, and this refusal
    // does not invite spending the reserve again — the priority now is
    // getting back to a clean room, not borrowing further against it.
    s.add("thing:the-fern", "The Fern").await;
    let over = s
        .refused(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "the fern needs water",
                "shape": "connection",
                "object": "thing:the-fern",
                "borrow": true,
            }),
        )
        .await;
    over.never_says("borrow: true");

    s.wrap("spent gamma's one emergency reserve and stopped there")
        .await;

    // ── dropping a thought, on Milhouse's room ───────────────────────────────
    let s2 = story.session().await;

    s2.add("bot:milhouse", "Milhouse").await;
    s2.call(
        "capture",
        json!({
            "subject": "bot:milhouse",
            "content": "capacity is two, for now",
            "fields": {"thought_capacity": "2"},
        }),
    )
    .await;

    s2.add("thing:the-couch", "The Couch").await;
    let couch = s2
        .call(
            "capture",
            json!({
                "subject": "bot:milhouse",
                "content": "the couch needs a leg fixed",
                "shape": "connection",
                "object": "thing:the-couch",
            }),
        )
        .await
        .field("address");

    // 🚨 **The negative `drop` rests on**: the room holds one of a capacity
    // of two — not full — so naming a drop here costs nothing. The couch
    // stays live and the new thought simply joins it.
    s2.add("thing:the-radiator", "The Radiator").await;
    let radiator = s2
        .call(
            "capture",
            json!({
                "subject": "bot:milhouse",
                "content": "the radiator is banging again",
                "shape": "connection",
                "object": "thing:the-radiator",
                "drop": &couch,
                "drop_because": "not actually why I'm writing this one",
            }),
        )
        .await
        .field("address");
    let not_full = s2.recall("bot:milhouse").await;
    not_full
        .says("the couch needs a leg fixed")
        .says("the radiator is banging again");
    assert!(
        not_full.raw().matches("\"status\":\"archived\"").count() == 0,
        "the room was not full, so naming a drop must not have archived anything: {}",
        not_full.raw(),
    );

    // ── a thought still matters, and nothing about it needs to change ────────
    //
    // `keep` is the designed way to say a live thought still earns its slot,
    // without writing a change that is not there. It is the one call this
    // verb refuses when nothing else is named — everywhere else, naming no
    // change at all is refused outright.
    s2.call("update_fact", json!({"address": &radiator, "keep": true}))
        .await;
    s2.recall("bot:milhouse")
        .await
        .claim(&radiator)
        .says("the radiator is banging again");

    // **The negative**: `keep` names no change on its own, so pairing it
    // with one that actually changes the claim is refused rather than
    // silently taking the edit and ignoring the flag.
    s2.refused(
        "update_fact",
        json!({
            "address": &radiator, "keep": true,
            "content": "the radiator is fine now, actually",
        }),
    )
    .await;
    s2.recall("bot:milhouse")
        .await
        .claim(&radiator)
        .says("the radiator is banging again")
        .never_says("the radiator is fine now");

    // Now the room genuinely holds two of two: full. Dropping the couch
    // frees the slot the third thought takes.
    s2.add("thing:the-mailbox", "The Mailbox").await;
    s2.call(
        "capture",
        json!({
            "subject": "bot:milhouse",
            "content": "the mailbox flag is stuck up",
            "shape": "connection",
            "object": "thing:the-mailbox",
            "drop": &couch,
            "drop_because": "the couch got fixed on its own, this is more pressing",
        }),
    )
    .await;
    let after_drop = s2.recall("bot:milhouse").await;
    after_drop
        .says("the radiator is banging again")
        .says("the mailbox flag is stuck up")
        .says("the couch got fixed on its own, this is more pressing");
    // The couch's own words are still on the record — a drop archives, it
    // never deletes — but it no longer reads as live, active testimony.
    assert!(
        after_drop.raw().matches("\"status\":\"archived\"").count() > 0,
        "the dropped thought must read as archived, not simply gone: {}",
        after_drop.raw(),
    );

    // And the room is still bounded: two live thoughts at capacity two, so a
    // fourth ordinary write is refused exactly as the first one was.
    s2.add("thing:the-gutter", "The Gutter").await;
    s2.refused(
        "capture",
        json!({
            "subject": "bot:milhouse",
            "content": "the gutter is clogged",
            "shape": "connection",
            "object": "thing:the-gutter",
        }),
    )
    .await;

    s2.wrap("dropped a thought that was no longer pressing to make room for one that was")
        .await;

    story.finish().await;
}

/// **A ceiling that is not a number is refused where it is written.** A room's
/// capacity and a thought's body cap are read as whole numbers, and a value
/// that does not read as one used to leave the container with no ceiling at
/// all, silently. The refusal names the key and what it holds, so the next
/// call can succeed.
///
/// Paired with the values that do read, whitespace around one included, so the
/// refusal is about the value and not about the key.
#[tokio::test]
async fn a_ceiling_that_is_not_a_number_is_refused_where_it_is_written() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;

    for (key, value) in [
        ("thought_capacity", "unlimited"),
        ("thought_capacity", "5.0"),
        ("thought_capacity", "-1"),
        ("thought_body_cap", "short"),
    ] {
        s.refused(
            "capture",
            json!({
                "subject": "bot:delta",
                "content": "a ceiling nobody can read",
                "fields": {key: value},
            }),
        )
        .await
        .says(key)
        .says("whole number")
        .says("\"wrote\":false");
    }
    // Nothing was written by any of them.
    s.recall("bot:delta").await.never_says("unlimited");

    // The values that read land, whitespace included.
    s.call(
        "capture",
        json!({
            "subject": "bot:delta",
            "content": "the room holds five",
            "fields": {"thought_capacity": " 5 "},
        }),
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "bot:delta",
            "content": "a thought may be short",
            "fields": {"thought_body_cap": "120"},
        }),
    )
    .await;

    s.wrap("ceilings that read, and ones that do not").await;
    story.finish().await;
}

/// **A room with no capacity says so, and never tells the caller to archive or
/// drop a thought it does not hold.** At capacity zero the room is empty, so
/// the refusal that names a thought to drop, or says how many to archive,
/// points at nothing. What is true is that no thought can be written until a
/// different identity gives the room a capacity.
///
/// Paired with a room that IS full: it still names a drop, which is what tells
/// "the refusal got quieter" from "the refusal got honest".
#[tokio::test]
async fn a_room_with_no_capacity_does_not_offer_a_thought_to_drop() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:epsilon", "Epsilon").await;
    s.add("thing:the-fern", "The Fern").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:epsilon",
            "content": "the room holds nothing",
            "fields": {"thought_capacity": "0"},
        }),
    )
    .await;

    // A capture that would be a thought.
    let none = s
        .refused(
            "capture",
            json!({
                "subject": "bot:epsilon",
                "content": "the fern needs water",
                "shape": "connection",
                "object": "thing:the-fern",
            }),
        )
        .await;
    none.says("thought_capacity")
        .says("\"room\":[]")
        .never_says("naming drop");

    // An edit that would make an ordinary claim into one.
    let claim = s.fact("bot:epsilon", "the fern is on the sill").await;
    let edit = s
        .refused(
            "update_fact",
            json!({
                "address": claim,
                "shape": "connection",
                "object": "thing:the-fern",
            }),
        )
        .await;
    edit.says("thought_capacity")
        .never_says("\"archive_needed\":");

    // The room that is full still names a drop.
    s.add("bot:sigma", "Sigma").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:sigma",
            "content": "the room holds one",
            "fields": {"thought_capacity": "1"},
        }),
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "bot:sigma",
            "content": "the fern needs water",
            "shape": "connection",
            "object": "thing:the-fern",
        }),
    )
    .await;
    s.refused(
        "capture",
        json!({
            "subject": "bot:sigma",
            "content": "the fern needs light",
            "shape": "connection",
            "object": "thing:the-fern",
        }),
    )
    .await
    .says("drop");

    s.wrap("a room with nothing in it, and one that is full")
        .await;
    story.finish().await;
}

/// **A merge is a way to write thoughts, so a full room refuses it.** A
/// duplicate that holds a thought brings it to the survivor, and a room at
/// capacity has no slot for it. The refusal names the thoughts to archive and
/// moves nothing; archiving one, as it says, lets the same merge land.
#[tokio::test]
async fn a_merge_that_would_overfill_a_room_is_refused_until_a_slot_is_free() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("bot:milhouse", "Milhouse").await;
    s.call(
        "capture",
        json!({
            "subject": "bot:milhouse",
            "content": "capacity is one",
            "fields": {"thought_capacity": "1"},
        }),
    )
    .await;
    s.add("thing:the-mailbox", "The Mailbox").await;
    let held = s
        .call(
            "capture",
            json!({
                "subject": "bot:milhouse",
                "content": "the mailbox flag is stuck up",
                "shape": "connection",
                "object": "thing:the-mailbox",
            }),
        )
        .await
        .field("address");

    // The duplicate is an ordinary thing that happens to hold a thought.
    s.add("thing:the-couch", "The Couch").await;
    s.add("thing:the-fern", "The Fern").await;
    s.call(
        "capture",
        json!({
            "subject": "thing:the-couch",
            "content": "the couch needs a leg fixed",
            "shape": "connection",
            "object": "thing:the-fern",
        }),
    )
    .await;

    // **Refused, with the room beside it, and nothing moved.**
    let refused = s
        .refused(
            "merge_entities",
            json!({"duplicate": "thing:the-couch", "survivor": "bot:milhouse"}),
        )
        .await;
    refused.says(&held).says("update_fact");
    s.recall("bot:milhouse")
        .await
        .says("the mailbox flag is stuck up")
        .never_says("the couch needs a leg fixed");

    // **The way forward, taken as the refusal says**: one archive, then the
    // same merge lands and the thought is the survivor's.
    s.call(
        "update_fact",
        json!({
            "address": &held, "status": "archived",
            "details": "the flag got fixed on its own",
        }),
    )
    .await;
    s.call(
        "merge_entities",
        json!({"duplicate": "thing:the-couch", "survivor": "bot:milhouse"}),
    )
    .await
    .says("thing:the-couch");
    s.recall("bot:milhouse")
        .await
        .says("the couch needs a leg fixed");

    s.wrap("merged a thing with a thought into a bot once the room had a slot")
        .await;

    story.finish().await;
}
