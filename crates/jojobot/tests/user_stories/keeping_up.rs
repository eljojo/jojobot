//! "What do I keep having to do, and when did I last do it?"
//!
//! **Three loops the operator actually keeps: a plant to water, a bill to pay
//! every month, and a filter to swap.** They are here because they are the
//! shape the schema was checked against — a cadence that cannot say *the bill
//! is monthly* is the wrong cadence, and a loop that cannot be written down
//! until somebody knows every answer is a loop nobody writes down.
//!
//! **A rhythm is a KIND now, and its keys are its own.** Two of them are
//! required and they are the two a loop cannot be one without: what it is
//! called, and the day somebody last looked at it. Everything else — how often,
//! what happened, the one thing to carry forward — is welcome and never
//! demanded.
//!
//! **The plant is the beat that matters.** It is watered without anybody having
//! decided how often, and jojobot takes it: a loop with a name and a date is a
//! loop. A schema that asked for the cadence first would have turned that away
//! and the watering would have gone unrecorded.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_loops_a_person_actually_keeps() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the things the loops are ON ─────────────────────────────────────────
    s.add("thing:the-fern", "The Fern").await;
    s.add("thing:jukebox", "The Jukebox").await;
    s.add("thing:the-air-filter", "The Air Filter").await;

    s.add_under("thing:the-fern", "rhythm:water-the-fern", "Water the fern")
        .await;
    s.add_under(
        "thing:jukebox",
        "rhythm:pay-the-jukebox-lease",
        "Pay the jukebox lease",
    )
    .await;
    s.add_under(
        "thing:the-air-filter",
        "rhythm:swap-the-air-filter",
        "Swap the air filter",
    )
    .await;

    // ── ① the plant: a name and a day, and nothing else ─────────────────────
    //
    // ⭐ **Nobody has decided how often.** The fern gets watered when it looks
    // like it needs it, and that is a real loop rather than an unfinished one —
    // so this write is taken whole, with no cadence and no note.
    s.event_with(
        "rhythm:water-the-fern",
        "watered it, the soil was dry again",
        json!({ "name": "Water the fern", "last_check_in": "2026-08-14" }),
        &[],
    )
    .await;

    // ── ② the bill: the one that needs a cadence ────────────────────────────
    //
    // **A month, said in days.** This is the loop that decides whether a
    // cadence is worth having at all: a bill that is paid monthly is not
    // something anybody wants to be reminded of by looking.
    s.event_with(
        "rhythm:pay-the-jukebox-lease",
        "paid it, same as every month",
        json!({
            "name": "Pay the jukebox lease",
            "last_check_in": "2026-08-01",
            "cadence_days": "30",
            "note": "the standing order covers it unless the amount changes",
        }),
        &[],
    )
    .await;

    // ── ③ the housework: what happened, carried forward ─────────────────────
    //
    // ⭐ **The outcome is one of three words, and the story is what happened.**
    // A loop either ran, or was skipped with the cycle moving on anyway, or was
    // snoozed with nothing moving. What was actually swapped and how filthy it
    // was is the claim and the note; the key says which of the three this was.
    s.event_with(
        "rhythm:swap-the-air-filter",
        "swapped it, it was filthier than last time",
        json!({
            "name": "Swap the air filter",
            "last_check_in": "2026-07-19",
            "cadence_days": "90",
            "outcome": "ran",
            "note": "the spare is the last one in the box",
        }),
        &[],
    )
    .await;

    // ── writing the loop's own word into that key is turned back ────────────
    //
    // **The refusal names the three, which is what makes it a way forward** — a
    // caller told only that the value is wrong has to go and find the
    // vocabulary, and there is nowhere obvious to look. This is the plain write
    // path rather than the check-in verb: the verb has always parsed its own
    // token, and what is new is that the KEY holds the vocabulary, so a write
    // that never goes near the verb is held to it too.
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter",
                "content": "swapped it again",
                "provenance": "testimony",
                "fields": { "outcome": "swapped" },
            }),
        )
        .await;
    refused.says("ran");
    refused.says("skipped");
    refused.says("snoozed");

    // ── and the same holds for which date a late cycle counts from ─────────
    //
    // ⚠️ **Free text here was the expensive kind.** The arithmetic reads two
    // words; anything else was stored silently and could not be read, so the
    // loop came back overdue every single day — a nag nobody asked for, from a
    // typo nothing reported.
    let policy = s
        .refused(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter",
                "content": "counts from whenever, I suppose",
                "provenance": "testimony",
                "fields": { "advances_from": "whenever" },
            }),
        )
        .await;
    policy.says("due_date");
    policy.says("check_in_date");

    // …and one of the two the arithmetic reads goes through.
    s.event_with(
        "rhythm:swap-the-air-filter",
        "it counts from the day it fell due",
        json!({ "advances_from": "due_date" }),
        &[],
    )
    .await;

    // …and the same write with a word the key names goes through, so the
    // refusal above is the SET talking rather than a key nobody may write.
    s.event_with(
        "rhythm:swap-the-air-filter",
        "let it go this cycle, the spare is spoken for",
        json!({ "outcome": "skipped" }),
        &[],
    )
    .await;

    // ── the question a person actually asks ─────────────────────────────────
    let loops = s
        .shape(
            "what do I keep having to do, and when did I last do it",
            json!({ "kind": "rhythm" }),
        )
        .await;
    loops.says("\"last_check_in\":\"2026-08-14\"");
    loops.says("\"last_check_in\":\"2026-08-01\"");
    loops.says("\"cadence_days\":\"30\"");
    // ⭐ **The carry-forward is the reason a note is a key.** A person coming
    // back to this loop needs the one thing they left themselves, and it is
    // beside the date rather than buried in a record.
    loops.says("the spare is the last one in the box");

    // ── the loop with no cadence is a loop, and says so ─────────────────────
    //
    // **The negative that makes the beat above mean something.** A build that
    // demanded every key would have refused the fern's write, and this answer
    // would be missing it entirely rather than carrying it without a cadence.
    let ferns = s
        .shape(
            "the fern's loop as it stands",
            json!({ "subject": "rhythm:water-the-fern" }),
        )
        .await;
    ferns.says("\"last_check_in\":\"2026-08-14\"");
    ferns.never_says("cadence_days");

    // ── and what a loop IS may not be taken back off it ─────────────────────
    //
    // The two required keys are the floor: a loop that has run cannot stop
    // holding the day it ran, because then it is not a loop anybody can use.
    let held = s
        .shape(
            "the bill's loop, with its records",
            json!({ "subject": "rhythm:pay-the-jukebox-lease", "facts": true }),
        )
        .await;
    held.says("rhythm:pay-the-jukebox-lease#");

    s.wrap("wrote down the three loops, one of them with nothing but a name and a date")
        .await;
    story.finish().await;
}
