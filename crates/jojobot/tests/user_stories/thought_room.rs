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
    s2.call(
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
    .await;
    let not_full = s2.recall("bot:milhouse").await;
    not_full
        .says("the couch needs a leg fixed")
        .says("the radiator is banging again");
    assert!(
        not_full.raw().matches("\"status\":\"archived\"").count() == 0,
        "the room was not full, so naming a drop must not have archived anything: {}",
        not_full.raw(),
    );

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
