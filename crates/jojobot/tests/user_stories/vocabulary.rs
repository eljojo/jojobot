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
//! where the difference between them stops being a doc note. A trip somebody
//! has planned only half of carries no dates: it ANSWERS the type, with the
//! gaps named, and it does not FIT it.

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
    s.add("event:winter-fest", "The Winter Fest").await;
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
            "counts_from": "2026-06-30",
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

    // Half a trip: where from and where to, and nobody has picked the dates.
    s.event_with(
        "event:winter-fest",
        "we are going, at some point",
        json!({
            "departs_from": "place:springfield",
            "arrives_at": "place:north-trail",
        }),
        &[],
    )
    .await;

    // ── the tolerant question: what is described like a trip ────────────────
    let described = s
        .call("search", json!({ "answers_type": "trip", "limit": 50 }))
        .await;
    // The positive the strict answer below rests on: nobody declared `trip` on
    // this instance and the question still works.
    described.says("event:trail-survey");
    described.says("\"complete\":true");
    // ⭐ The half-planned one comes back, saying what it lacks BY NAME — which
    // is what a reader acts on. A count would leave them nothing to fill in.
    described.says("event:winter-fest");
    described.says("\"lacking\":[\"leaves_on\",\"returns_on\"]");
    // …and the negative in the same answer that just proved it is not empty:
    // the bike shares no key with a trip, so it is not a weak match.
    described.never_says("thing:gravel-bike");

    // ── the strict question, about the same two things ──────────────────────
    //
    // ⭐ **The difference between the two, on one thing.** The winter fest
    // answered the type a moment ago and is not here, because a trip with no
    // dates does not FIT one. Which of the two questions you are asking is
    // yours to choose, and the tolerant one is what you get when you name
    // neither.
    let are_trips = s
        .call("search", json!({ "fits_type": "trip", "limit": 50 }))
        .await;
    are_trips.says("event:trail-survey");
    are_trips.never_says("event:winter-fest");
    are_trips.never_says("thing:gravel-bike");

    // **And a key written on one thing over two sittings reads off the KEY.**
    // The fold takes the newest write of each, so this one row answers without
    // reading a single record back.
    let planned = s
        .shape(
            "the trips and when each leaves",
            json!({ "answers_type": "trip" }),
        )
        .await;
    planned.says("\"leaves_on\":\"2026-09-03\"");
    planned.says("\"arrives_at\":\"place:north-trail\"");

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
    declared.says("\"name\":\"trip\",\"origin\":\"shipped\"");

    // ── and the name the software owns is closed ────────────────────────────
    let refused = s
        .refused(
            "declare_type",
            json!({
                "name": "trip",
                "fields": [{ "key": "cadence_km", "holds": "number" }],
            }),
        )
        .await;
    // It names the type it is about and the verb to call with a name of your
    // own, because re-sending this call will never work.
    refused.says("trip");
    refused.says("declare_type");

    // The write really was refused, and the vocabulary is read back rather than
    // believed — through calls that WRITE NOTHING. A re-read that declared
    // anything would be asking what the types are by changing one of them.
    //
    // The list first: asking a question of a type nobody declared comes back
    // naming every type that does exist, which is how a session learns what the
    // vocabulary is without declaring something to find out.
    let listed = s
        .refused("search", json!({ "answers_type": "warranty" }))
        .await;
    listed.says("Types that do exist");
    listed.says("trip");
    // …and the keys under that name are the ones the software shipped: the
    // half-planned trip still lacks exactly the two it never wrote, and the key the
    // refused call would have replaced them with is nowhere.
    let still = s
        .call("search", json!({ "answers_type": "trip", "limit": 50 }))
        .await;
    still.says("\"lacking\":[\"leaves_on\",\"returns_on\"]");
    still.never_says("cadence_km");

    s.wrap("used the vocabulary the software came with, and found its own name refused")
        .await;
    story.finish().await;
}
