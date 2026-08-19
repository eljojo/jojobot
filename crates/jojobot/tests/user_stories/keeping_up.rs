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
    s.event_with(
        "rhythm:swap-the-air-filter",
        "swapped it, it was filthier than last time",
        json!({
            "name": "Swap the air filter",
            "last_check_in": "2026-07-19",
            "cadence_days": "90",
            "outcome": "swapped",
            "note": "the spare is the last one in the box",
        }),
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
