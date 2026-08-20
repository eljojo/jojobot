//! "I said the venue is a place. Why has jojobot taken a pet?"
//!
//! **Because a declared type describes and never holds.** A type is the
//! vocabulary a caller asks WITH: it says which keys a thing of that shape
//! carries, so `search` can ask which things answer it and say what each one
//! lacks. Answering a shape is not the same act as being held to one, and
//! nothing a caller declares gates a write.
//!
//! **The narrowing is what the declaration is FOR.** A reference that names no
//! kind means "a handle of some kind jojobot knows", so a key meant for a venue
//! is satisfied by a pet. Saying `reference:place` is what lets the declaration
//! say which kind belongs on the other end — and the answer reports the value
//! that does not, rather than turning the write away.
//!
//! **What IS refused is here too, and it is a different rule.** A reference
//! naming no entity at all is blocked, because a walkable link into something
//! nobody recorded is a hole. Both are in this story because either alone reads
//! as a rule this is not — the first as a type gating every write, the second
//! as nothing being checked anywhere.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_declared_type_describes_a_thing_and_holds_it_to_nothing() {
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
                    { "key": "venue", "holds": "reference:place", "required": true },
                    { "key": "seats", "holds": "number", "required": true },
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

    // ── ① the wrong kind, on the thing that answers the type ────────────────
    //
    // A pet is a perfectly good handle, and the declaration says this key wants
    // a place. **The write lands anyway**: a type nobody was offered and nobody
    // confirmed is a vocabulary, and a vocabulary describes rather than holds.
    // What may be taken off a thing is its own KIND's question, and `event`
    // names no keys.
    s.event_with(
        "event:the-booking",
        "moved it to the dog, apparently",
        json!({ "venue": "pet:santas-little-helper" }),
        &[],
    )
    .await;
    let after = s
        .shape(
            "the booking as it stands",
            json!({ "subject": "event:the-booking" }),
        )
        .await;
    after.says("pet:santas-little-helper");

    // …and the caller puts back what it meant, which is the whole repair: the
    // declaration told it what the key wants and nothing stood in the way of
    // either write.
    s.event_with(
        "event:the-booking",
        "no, the back room after all",
        json!({ "venue": "place:moes" }),
        &[],
    )
    .await;
    let repaired = s
        .shape(
            "the booking with its venue put back",
            json!({ "subject": "event:the-booking" }),
        )
        .await;
    repaired.says("place:moes");

    // ── ② the same write, on a thing that answers the type not at all ───────
    //
    // The jotting carries no key of the type yet, and it makes no difference:
    // how much of a vocabulary a thing answers changes nothing about what may
    // be written on it. Without this beat, beat ① reads as "a half-described
    // thing is the special case" rather than as the rule.
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

    // ── a key is a name, and a name has a length ────────────────────────────
    //
    // The other door onto the same rule: a record's key is bounded where it is
    // written, and a declaration names keys a record will carry, so a type may
    // not name one no record could hold. The refusal carries the number, which
    // is what a caller writes to instead of bisecting against a store error.
    s.refused(
        "declare_type",
        json!({
            "name": "long-winded",
            "fields": [{ "key": "k".repeat(129), "holds": "text" }],
        }),
    )
    .await
    .says("128");

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
