//! "I said the venue is a place. Why has jojobot just turned my write down?"
//!
//! **Because on this thing the declaration had stopped describing and started
//! holding.** A type says what its keys hold; once a thing carries every one of
//! them, that is what the thing IS, and a write that would leave it otherwise
//! is refused rather than stored and reported.
//!
//! **The narrowing is the new half.** A reference used to mean "a handle of
//! some kind jojobot knows", so a key meant for a venue was satisfied by a pet.
//! Saying `reference:place` is what lets the declaration say which kind is on
//! the other end — and the refusal quotes that same spelling back, so what the
//! caller is told is what the caller would write.
//!
//! **And the boundary is the whole story, not a footnote.** The identical write
//! that is refused on the finished booking lands on the half-written one. A
//! thing that fits nothing has nothing to protect, so it stays messy and stays
//! repairable; a thing that fits is held to what it fits. Both are here because
//! either alone reads as a rule this is not — the first as a gate over every
//! write, the second as no rule at all.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_declared_key_is_held_to_on_the_thing_that_answers_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("place:moes", "Moe's").await;
    s.add("pet:santas-little-helper", "Santa's Little Helper")
        .await;
    s.add("event:the-booking", "The Booking").await;
    s.add("event:the-jotting", "The Jotting").await;

    // ── the declaration, and the kind is part of it ─────────────────────────
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "booking",
                "fields": [
                    { "key": "venue", "holds": "reference:place" },
                    { "key": "seats", "holds": "number" },
                ],
            }),
        )
        .await;
    // It comes back in the spelling it was sent in, which is what makes the
    // declaration something a caller can read back and re-send.
    declared.says("\"holds\":\"reference:place\"");

    // ── the thing that fits, written over two sittings ──────────────────────
    s.event_with(
        "event:the-booking",
        "booked the back room",
        json!({ "venue": "place:moes" }),
        &[],
    )
    .await;
    s.event_with(
        "event:the-booking",
        "and told them how many",
        json!({ "seats": "12" }),
        &[],
    )
    .await;

    // ── ① the wrong kind, on the thing that fits ────────────────────────────
    //
    // A pet is a perfectly good handle, which is exactly why the value type
    // alone could never have caught this: what is wrong with it is the kind.
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": "event:the-booking",
                "content": "moved it to the dog, apparently",
                "provenance": "testimony",
                "fields": { "venue": "pet:santas-little-helper" },
            }),
        )
        .await;
    // The refusal names the key, what the key wanted, and the type it would
    // stop fitting — the three things a caller needs to write the call again.
    refused.says("venue");
    refused.says("reference:place");
    refused.says("booking");

    // …and the record is untouched: a refusal writes nothing.
    let after = s
        .shape(
            "the booking as it stands",
            json!({ "subject": "event:the-booking" }),
        )
        .await;
    after.says("place:moes");
    after.never_says("pet:santas-little-helper");

    // ── ② the identical write, on the thing that fits nothing ───────────────
    //
    // **The pair the boundary turns on.** The jotting carries no key of the
    // type yet, so there is no fit to protect and the very value refused above
    // lands here. Without this beat the refusal above reads as a gate over
    // every reference on the surface.
    s.event_with(
        "event:the-jotting",
        "at the dog's place, apparently",
        json!({ "venue": "pet:santas-little-helper" }),
        &[],
    )
    .await;
    // …and it goes on being writable afterwards. **The seats land on a thing
    // whose venue is already wrong**, which is the promise that a thing broken
    // once can still be finished: a check on the CHANGE would measure this
    // write against a rule the thing already breaks and refuse the repair.
    //
    // It leaves the jotting carrying every key `booking` names, with one of
    // them holding a pet — which is the state beat ⑤ needs and the only state
    // where the strict question turns on the value rather than on a gap.
    s.event_with(
        "event:the-jotting",
        "four of us, wherever it is",
        json!({ "seats": "4" }),
        &[],
    )
    .await;

    // ── ③ adding a key no type mentions, on the thing that DOES fit ─────────
    //
    // Strict is a floor and never a ceiling: a type says what has to survive,
    // not what may be there.
    s.event_with(
        "event:the-booking",
        "they do a set menu",
        json!({ "menu": "the set one" }),
        &[],
    )
    .await;

    // ── ④ a reference naming nobody ─────────────────────────────────────────
    //
    // A reference is a walkable link, so a value naming nothing leaves the hole
    // an edge into a missing entity leaves — and it is refused the same way,
    // with the near handles the caller needs rather than a bare no.
    let dangling = s
        .refused(
            "capture",
            json!({
                "subject": "event:the-jotting",
                "content": "at the other place",
                "provenance": "testimony",
                "fields": { "venue": "place:moes-tavern" },
            }),
        )
        .await;
    dangling.says("place:moes-tavern");
    // The candidate it should have meant, which is what makes this a repair
    // rather than a dead end.
    dangling.says("place:moes");

    // ── ⑤ and the two questions, over everything above ──────────────────────
    //
    // **The jotting now carries BOTH of the type's keys, and lacks neither.**
    // The only thing between it and a booking is that its venue holds a pet.
    // So this is the pair that turns on the definition itself rather than on a
    // gap: the tolerant question keeps it and names the bad value, the strict
    // question drops it, and a build that decided fitting by counting missing
    // keys would put it in both answers.
    let described = s
        .call("search", json!({ "answers_type": "booking", "limit": 50 }))
        .await;
    described.says("event:the-booking");
    described.says("event:the-jotting");
    described.says("pet:santas-little-helper");
    // It lacks nothing — said here, because it is the fact a `lacking`-only
    // definition would have gone by.
    described.says("\"lacking\":[]");

    let are_bookings = s
        .call("search", json!({ "fits_type": "booking", "limit": 50 }))
        .await;
    are_bookings.says("event:the-booking");
    are_bookings.never_says("event:the-jotting");

    s.wrap("said what a booking's venue is, and found out the hard way that it now means it")
        .await;
    story.finish().await;
}
