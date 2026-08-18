//! "I have never declared a type. Why does jojobot already know what a trip
//! is?"
//!
//! **Because the software ships its own vocabulary.** A session that was told
//! nothing writes `leaves_on` and `cadence_days` and finds that the questions
//! over them already work — where a vocabulary each instance had to declare for
//! itself is one every instance spells differently, and nothing written under
//! one spelling is reachable from another.
//!
//! It is also what gives the protection over shipped names something to guard.
//! Until the software declared a type of its own, no caller could meet that
//! refusal and no caller could see an origin that was not its own: the only way
//! to learn a name was closed was to write over it and read the answer.
//!
//! **And the two type questions are answered here on one thing**, which is
//! where the difference between them stops being a doc note. A rhythm nobody
//! has run yet carries no `last_check_in`: it ANSWERS the type, with the gap
//! named, and it does not FIT it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_software_ships_a_vocabulary_a_session_never_declared() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // ── the things ──────────────────────────────────────────────────────────
    s.add("thing:gravel-bike", "The Gravel Bike").await;
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    s.add("event:trail-survey", "The Trail Survey").await;
    s.add("place:springfield", "Springfield").await;
    s.add("place:north-trail", "The North Trail").await;

    // ── a session writes the shipped keys, having declared nothing ───────────
    //
    // **A cadence is TIME.** The chain gets looked at every sixty days; how far
    // the bike went between them is something a check-in measures, never a unit
    // of the schedule — so there is no distance here and no unit key.
    s.event_with(
        "thing:gravel-bike",
        "chain checked, still within wear",
        json!({
            "cadence_days": "60",
            "advances_from": "the day it happened",
            "last_check_in": "2026-06-30",
            "outcome": "no play in it",
        }),
        &[],
    )
    .await;

    // The same loop, set up and never run. This is the one both questions get
    // asked about.
    s.event_with(
        "thing:torque-wrench",
        "wants calibrating twice a year, nobody has yet",
        json!({ "cadence_days": "180", "advances_from": "the day it fell due" }),
        &[],
    )
    .await;

    // A trip, in the shipped words: where from, where to, and the two dates.
    s.event_with(
        "event:trail-survey",
        "four days up the trail",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:north-trail",
            "leaves_on": "2026-09-03",
            "returns_on": "2026-09-07",
        }),
        &[],
    )
    .await;

    // ── the tolerant question: what is described like a rhythm ──────────────
    let described = s
        .call("search", json!({ "answers_type": "rhythm", "limit": 50 }))
        .await;
    // The positive the strict answer below rests on: nobody declared `rhythm`
    // on this instance and the question still works.
    described.says("thing:gravel-bike");
    described.says("\"complete\":true");
    // ⭐ The rhythm that has never run comes back, saying what it lacks BY
    // NAME — which is what a reader acts on. A count would leave them nothing
    // to fill in.
    described.says("thing:torque-wrench");
    described.says("\"lacking\":[\"last_check_in\",\"outcome\"]");
    // …and the negative in the same answer that just proved it is not empty:
    // the trip shares no key with a rhythm, so it is not a weak match.
    described.never_says("event:trail-survey");

    // ── the strict question, about the same two things ──────────────────────
    //
    // ⭐ **The difference between the two, on one thing.** The wrench answered
    // the type a moment ago and is not here, because a loop nobody has run does
    // not FIT one. Which of the two questions you are asking is yours to
    // choose, and the tolerant one is what you get when you name neither.
    let are_rhythms = s
        .call("search", json!({ "fits_type": "rhythm", "limit": 50 }))
        .await;
    are_rhythms.says("thing:gravel-bike");
    are_rhythms.never_says("thing:torque-wrench");

    // The trip fits its own type, whole, and the bikes are not trips — the
    // negative that keeps "it fits" from meaning "everything came back".
    let are_trips = s
        .call("search", json!({ "fits_type": "trip", "limit": 50 }))
        .await;
    are_trips.says("event:trail-survey");
    are_trips.never_says("thing:gravel-bike");

    // **And the check-in date is read off the KEY.** The fold takes the newest
    // write of each key, so this one row answers "when did this last happen"
    // without reading a single record back.
    let quiet = s
        .shape(
            "the loops and when each was last run",
            json!({ "answers_type": "rhythm" }),
        )
        .await;
    quiet.says("\"last_check_in\":\"2026-06-30\"");
    quiet.says("\"cadence_days\":\"60\"");

    // ── a type of the session's own, beside the ones it was given ───────────
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "loan",
                "fields": [
                    { "key": "borrowed_by", "holds": "reference" },
                    { "key": "due_back", "holds": "date" },
                ],
            }),
        )
        .await;
    // **Both origins in one read.** An answer saying `declared` on everything
    // would pass a beat that only ever looked at the caller's own type, and a
    // caller that cannot tell the two apart learns which names are closed by
    // being refused — the refusal springs rather than being avoidable.
    declared.says("\"name\":\"loan\",\"origin\":\"declared\"");
    declared.says("\"name\":\"rhythm\",\"origin\":\"shipped\"");
    declared.says("\"name\":\"trip\",\"origin\":\"shipped\"");

    // ── and the name the software owns is closed ────────────────────────────
    let refused = s
        .refused(
            "declare_type",
            json!({
                "name": "rhythm",
                "fields": [{ "key": "cadence_km", "holds": "number" }],
            }),
        )
        .await;
    // It names the type it is about and the verb to call with a name of your
    // own, because re-sending this call will never work.
    refused.says("rhythm");
    refused.says("declare_type");

    // The write really was refused: the shipped keys are still what they were,
    // read back through the surface rather than believed.
    let after = s
        .call(
            "declare_type",
            json!({ "name": "loan", "fields": [{ "key": "due_back" }] }),
        )
        .await;
    after.says("\"name\":\"rhythm\",\"origin\":\"shipped\"");
    let still = s
        .call("search", json!({ "answers_type": "rhythm", "limit": 50 }))
        .await;
    still.says("\"lacking\":[\"last_check_in\",\"outcome\"]");
    still.never_says("cadence_km");

    s.wrap("used the vocabulary the software came with, and found its own name refused")
        .await;
    story.finish().await;
}
