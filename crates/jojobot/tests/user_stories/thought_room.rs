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
            "sets": {"thought_capacity": "1"},
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
    // The way back is archiving, as many as it takes, and the count is a number
    // a caller reads: the room holds 2 of 1 and one more must fit, so 2. Naming
    // a drop would swap one thought for one and leave the debt where it was.
    over.number("/archive_needed", 2);
    over.never_holds_at("/how_to_proceed", "drop_because");

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
            "sets": {"thought_capacity": "2"},
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
    let after_drop = s2.recall_with_details("bot:milhouse").await;
    after_drop
        .says("the radiator is banging again")
        .says("the mailbox flag is stuck up")
        .says("the couch got fixed on its own, this is more pressing");
    // The couch's own words are still on the record — a drop archives, it
    // never deletes — but it no longer reads as live, active testimony.
    // **The couch's own claim**, picked by its address: an archived mark anywhere
    // in the read would be satisfied by any other archived claim on the bot.
    after_drop
        .claim(&couch)
        .says("the couch needs a leg fixed")
        .says("\"status\":\"archived\"");
    // And the radiator, which was kept and not dropped, is not archived.
    after_drop
        .claim(&radiator)
        .never_says("\"status\":\"archived\"");

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
                "sets": {key: value},
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
            "sets": {"thought_capacity": " 5 "},
        }),
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "bot:delta",
            "content": "a thought may be short",
            "sets": {"thought_body_cap": "120"},
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
            "sets": {"thought_capacity": "0"},
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
            "sets": {"thought_capacity": "1"},
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
            "sets": {"thought_capacity": "1"},
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

/// **A thread a thought points at: what it says, written by somebody who knew.**
const THREADS: [(&str, &str, &str); 3] = [
    (
        "thread:milhouse-moves",
        "Milhouse moves in the spring; the van is booked for the second weekend.",
        "Milhouse is moving in the spring",
    ),
    (
        "thread:krusty-telethon",
        "Krusty plans a telethon; the studio is booked for the last Saturday.",
        "the telethon needs a studio",
    ),
    (
        "thread:simpson-kitchen",
        "The Simpsons ask for a second quote on the same kitchen.",
        "a second quote is wanted",
    ),
];

/// "Pick up where the last session left off, and tell me what has gone quiet."
///
/// **A coordinator carries a few thoughts from one run to the next, each a short
/// pointer at a thread that holds the detail.** Its session wraps. A fresh
/// session of the same bot boots, reads its room, follows each pointer and
/// finds the detail on the thread. The session keeps the thoughts that are
/// still live and leaves one alone. A later session reads the room and finds
/// that one has gone quiet: it is marked, it is counted apart from the live
/// ones, and the thread it pointed at is still there, so nothing was lost.
/// Keeping it brings it back.
///
/// ⛔️ **Every negative here sits beside the positive it depends on, in the
/// same read.** A room that showed nothing would pass "the quiet thought is not
/// live" on a build where nothing is carried across a boot at all.
#[tokio::test]
async fn a_coordinators_room_crosses_a_boot_and_the_thought_nobody_touched_goes_quiet() {
    let story = Story::begin("bot:otto").await;

    // ── the people who set the room up ──────────────────────────────────────
    //
    // **Otto cannot set its own ceilings**, so the assistant does: room for
    // three thoughts, and a thought untouched through two runs goes quiet. The
    // threads carry the detail, written as the operator's own words.
    let owner = story.as_bot("bot:assistant").await;
    owner
        .call(
            "capture",
            json!({
                "subject": "bot:otto", "content": "how big otto's room is and how long a thought lasts",
                "sets": {"thought_capacity": "3", "thought_ages_after_runs": "2"},
            }),
        )
        .await;
    for (thread, detail, _) in THREADS {
        owner.add(thread, thread).await;
        owner
            .call(
                "capture",
                json!({"subject": thread, "content": detail, "provenance": "testimony"}),
            )
            .await;
    }

    // ── the first session: three pointers, and the session wraps ────────────
    let first = story.session().await;
    // **The session says what it set out to do before it writes anything**, as a
    // session is taught to. The first write of a run is also the write that makes
    // the run exist.
    first
        .call(
            "journal",
            json!({"entry": "taking over the move, the telethon and the kitchen quote"}),
        )
        .await;
    let mut thoughts = Vec::new();
    for (thread, _, pointer) in THREADS {
        thoughts.push(
            first
                .call(
                    "capture",
                    json!({
                        "subject": "bot:otto", "content": pointer,
                        "shape": "connection", "object": thread,
                    }),
                )
                .await
                .field("address"),
        );
    }
    let [moving, telethon, quote] = [&thoughts[0], &thoughts[1], &thoughts[2]];
    first
        .wrap("wrote down the three lines I am coordinating")
        .await;

    // ── the second session: boot, read the room, follow each pointer ────────
    let (boot, second) = story.full_boot().await;
    assert_eq!(
        boot["identity"]["bot"]["id"], "bot:otto",
        "the fresh session is the same bot: {boot}"
    );
    // **The same session starts by saying what it picked up.** It is a beat
    // like any other, and it is also what makes this run exist before it
    // touches a thought.
    second
        .call(
            "journal",
            json!({"entry": "picked up the room from last time"}),
        )
        .await;
    let room = second
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await;
    for (thread, _, pointer) in THREADS {
        room.says(pointer).says(thread);
    }
    // Nothing has gone quiet yet: the room block is only sent when something has.
    room.never_says("aged_out");
    // Each pointer leads to its thread, and the detail is there.
    for (thread, detail, _) in THREADS {
        second
            .call("recall", json!({"subject": thread, "facts": true}))
            .await
            .says(detail);
    }
    // The room is full: a fourth thought is refused, and the refusal names the
    // two ways past a full room.
    second.add("thing:jukebox", "The Jukebox").await;
    second
        .refused(
            "capture",
            json!({
                "subject": "bot:otto", "content": "the jukebox needs a needle",
                "shape": "connection", "object": "thing:jukebox",
            }),
        )
        .await
        .says("borrow")
        .says("drop");
    // The session keeps the two lines that are still live, and leaves the third.
    for kept in [moving, telethon] {
        second
            .call("update_fact", json!({"address": kept, "keep": true}))
            .await;
    }
    second
        .wrap("kept the move and the telethon, left the quote")
        .await;

    // ── the third session: the quote has gone quiet ─────────────────────────
    let (_, third) = story.full_boot().await;
    third
        .call("journal", json!({"entry": "picked up the room again"}))
        .await;
    let room = third
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await;
    let body = room.json();
    let object = &body["objects"][0];
    let thought = |address: &str| {
        object["facts"]
            .as_array()
            .and_then(|all| all.iter().find(|fact| fact["address"] == address))
            .unwrap_or_else(|| panic!("{address} is in the room read: {body}"))
    };
    // **The positive**: the two that were kept read as live thoughts.
    for kept in [moving, telethon] {
        assert!(
            thought(kept).get("aged_out").is_none(),
            "{kept} was kept and is live: {body}"
        );
    }
    // **The negative it rests on**: the one nobody touched is marked, and the
    // room block counts the live and the quiet apart.
    assert_eq!(thought(quote)["aged_out"], true, "{body}");
    assert_eq!(object["room"]["live"], 2, "{body}");
    assert_eq!(object["room"]["aged_out"], 1, "{body}");
    assert_eq!(object["room"]["capacity"], 3, "{body}");
    assert_eq!(object["room"]["ages_after_runs"], 2, "{body}");
    // It is still there, whole, in the same answer: gone quiet, not gone.
    room.says("a second quote is wanted");

    // **The thread it pointed at is still reachable**, and says what it said.
    third
        .call(
            "recall",
            json!({"subject": "thread:simpson-kitchen", "facts": true}),
        )
        .await
        .says("a second quote on the same kitchen");

    // **The room block says, once, how to bring a quiet thought back.** A session
    // that reads `aged_out: true` learns the call from the same answer.
    let bring_back = object["room"]["bring_back"]
        .as_str()
        .unwrap_or_else(|| panic!("the room block names the way back: {body}"));
    assert!(
        bring_back.contains("update_fact") && bring_back.contains("keep"),
        "the way back names the call: {bring_back}"
    );

    // **A quiet thought is outside the count, so a new thought fits without the
    // reserve.** The room holds two live of three; the fourth thought lands, and
    // its receipt does not claim the emergency reserve was spent. The room read
    // after it says three of three, with the quiet one still outside.
    third.add("thing:the-fern", "The Fern").await;
    let fern = third
        .call(
            "capture",
            json!({
                "subject": "bot:otto", "content": "the fern needs water",
                "shape": "connection", "object": "thing:the-fern",
            }),
        )
        .await;
    fern.never_says("emergency reserve");
    let fern = fern.field("address");
    let read = third
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await
        .json();
    assert_eq!(read["objects"][0]["room"]["live"], 3, "{read}");
    assert_eq!(read["objects"][0]["room"]["aged_out"], 1, "{read}");

    // **Keeping the quiet thought into a full room is refused as a capture would
    // be**, naming the live thoughts to archive; and it stays quiet.
    let refused = third
        .refused("update_fact", json!({"address": quote, "keep": true}))
        .await;
    refused.says(&fern).says("archive");
    third
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await
        .says("\"aged_out\":true");

    // **The positive it rests on: with a slot free, the same keep lands.** The
    // fern thought is archived, and the quote comes back: the marker and the block
    // go, and the room is full of live thoughts again.
    third
        .call(
            "update_fact",
            json!({"address": fern, "status": "archived", "details": "the fern was watered"}),
        )
        .await;
    third
        .call("update_fact", json!({"address": quote, "keep": true}))
        .await;
    let back = third
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await;
    back.says("a second quote is wanted").never_says("aged_out");

    // **A room over its ceiling by a borrow shows its block with nothing aged in
    // it**, and a capture into a truly full room still claims the reserve.
    third.add("thing:the-couch", "The Couch").await;
    let borrowed = third
        .call(
            "capture",
            json!({
                "subject": "bot:otto", "content": "the couch needs a leg",
                "shape": "connection", "object": "thing:the-couch", "borrow": true,
            }),
        )
        .await;
    borrowed.says("emergency reserve").says("4 of 3");
    let over = third
        .call(
            "recall",
            json!({"subject": "bot:otto", "facts": true, "status": "active"}),
        )
        .await
        .json();
    assert_eq!(over["objects"][0]["room"]["live"], 4, "{over}");
    assert_eq!(over["objects"][0]["room"]["aged_out"], 0, "{over}");
    assert_eq!(over["objects"][0]["room"]["capacity"], 3, "{over}");
    assert!(
        !over.to_string().contains("\"aged_out\":true"),
        "nothing is marked quiet in a room that is over: {over}"
    );

    third.wrap("the quote line is live again").await;
    story.finish().await;
}

/// **An edit that would add a thought to a room already over its capacity is
/// told to archive, and not to write the thought through `capture`.** Over its
/// capacity, a capture is refused too, and a drop would leave the debt where it
/// was. Paired: exactly at capacity the refusal still points at `capture`, which
/// can swap one thought out as it writes this one.
#[tokio::test]
async fn an_edit_into_a_room_over_its_capacity_is_told_to_archive_and_not_to_capture() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "capacity is one",
               "sets": {"thought_capacity": "1"}}),
    )
    .await;
    let g = story.as_bot("bot:gamma").await;
    for (handle, name) in [
        ("thing:battery", "The Battery"),
        ("thing:the-fern", "The Fern"),
        ("thing:the-couch", "The Couch"),
    ] {
        g.add(handle, name).await;
    }
    // An ordinary claim about Gamma, which an edit can turn into a thought.
    let note = g
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "the shed wants a lock"}),
        )
        .await
        .field("address");
    let becomes_a_thought =
        json!({"address": note, "shape": "connection", "object": "thing:battery"});
    g.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "the fern needs water",
               "shape": "connection", "object": "thing:the-fern"}),
    )
    .await;

    // ── exactly at capacity: the edit is refused and capture is still named ──
    let at = g.refused("update_fact", becomes_a_thought.clone()).await;
    at.holds_at("/how_to_proceed", "capture");

    // ── over capacity by one borrow: archive the count, and no capture ───────
    g.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "the couch needs a leg",
               "shape": "connection", "object": "thing:the-couch", "borrow": true}),
    )
    .await;
    let over = g.refused("update_fact", becomes_a_thought).await;
    over.number("/archive_needed", 2);
    over.never_holds_at("/how_to_proceed", "capture");
    story.finish().await;
}

/// **A merge into your own bot is refused for any key a bot cannot set about
/// itself, not only for the two thought ceilings.** The ageing setting is one of
/// them. Taking the key off the duplicate first, as the refusal says, lets the
/// same merge land.
#[tokio::test]
async fn a_merge_into_your_own_bot_is_refused_for_a_duplicate_carrying_the_ageing_setting() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-couch", "The Couch").await;
    let setting = s
        .call(
            "capture",
            json!({"subject": "thing:the-couch", "content": "ages after five runs",
                   "sets": {"thought_ages_after_runs": "5"}}),
        )
        .await
        .field("address");

    let refused = s
        .refused(
            "merge_entities",
            json!({"duplicate": "thing:the-couch", "survivor": "bot:otto"}),
        )
        .await;
    refused
        .holds_at("/fix_by", "change")
        .holds_at("/how_to_proceed", "thought_ages_after_runs");

    s.call(
        "update_fact",
        json!({"address": setting, "clear_fields": ["thought_ages_after_runs"]}),
    )
    .await;
    s.call(
        "merge_entities",
        json!({"duplicate": "thing:the-couch", "survivor": "bot:otto"}),
    )
    .await
    .says("thing:the-couch");
    story.finish().await;
}
