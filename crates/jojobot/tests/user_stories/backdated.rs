//! "Work through last March for me. It is the 15th."
//!
//! **A run is not always happening now.** A session catching up on a week
//! nobody recorded, an instance restored from a backup, a run working through
//! a stretch of time — each is standing in a day that is not the server's.
//!
//! The zone beside it says how to NAME a day; this says WHICH DAY the run is
//! in, and no zone can tell the server that. It is stated at the door, once,
//! and everything the run writes is in it.
//!
//! ⚠️ **The failure this closes is the quiet one.** The door already took the
//! day, the beats already carried it, and the sweep already read it — and a
//! CLAIM was still stamped with the day the run actually happened. The prose
//! such a run writes is perfectly in period, so a reader grading the answers
//! sees nothing wrong and the store holds the wrong date for every claim.
//!
//! The days here are fixed rather than worked out from today, because the
//! whole point is a run standing somewhere the clock is not.

use super::dsl::Story;

#[tokio::test]
async fn a_run_working_through_a_past_week_writes_in_that_week() {
    const MARCH: &str = "2026-03-15";

    let story = Story::begin("bot:otto").await;

    // ── the boot says what the day is for, before anybody trips over it ─────
    let (boot, setup) = story.full_boot().await;
    let essay = boot["orientation"]
        .as_str()
        .expect("a full boot carries the orientation")
        .to_string();
    // **The ARGUMENT by name, not a sentence about it.** A phrase of our own
    // prose breaks when the prose improves and proves nothing; `` `today` ``
    // is the thing a session has to send, and the zone paragraph beside it
    // uses the word without ever naming the argument — so this needle is
    // satisfied only by an essay that teaches it.
    for taught in ["`today`", "`timezone`"] {
        assert!(
            essay.contains(taught),
            "the boot teaches the day the way it teaches the zone: {taught} is absent",
        );
    }

    setup.add("person:milhouse", "Milhouse").await;
    setup.wrap("standing the person up").await;

    // ── a run that says it is in March ──────────────────────────────────────
    let march = story.session_on(MARCH, Some("new")).await;

    // A claim with no date on it. The run stated its day once, at the door.
    let stamped = march
        .undated_fact("person:milhouse", "went to the fair")
        .await;
    assert_eq!(
        stamped, MARCH,
        "the claim was stamped with the day the run happened, not the day it is in",
    );

    // **And it reads back in that day**, through the surface a later session
    // uses — the assertion the write's own receipt cannot make.
    march
        .recall("person:milhouse")
        .await
        .says("went to the fair")
        .says(&format!("\"date\":\"{MARCH}\""));

    // ── a claim about another day still goes where it is sent ───────────────
    //
    // Most of what a run working through a period does is record claims about
    // days other than the one it is standing in. A stated day that overrode the
    // caller would take that away.
    march
        .fact_on(
            "person:milhouse",
            "had been ill the week before",
            "2026-03-08",
        )
        .await;
    march
        .recall("person:milhouse")
        .await
        .says("\"date\":\"2026-03-08\"");

    march.wrap("worked through the March week").await;

    // ── a run that says nothing is still answered by the clock ──────────────
    //
    // ⚠️ Without this the frame stops being an option and becomes a thing every
    // session has to know about before it can write a correct date.
    let now = story.session().await;
    let today = now
        .undated_fact("person:milhouse", "said he would come along")
        .await;
    assert_ne!(
        today, MARCH,
        "a run that stated no day inherited another run's frame",
    );
    assert_eq!(
        today,
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            .to_string(),
        "a run that stated no day is answered on the clock, in the stated fallback zone",
    );

    now.wrap("recorded one thing, today").await;

    story.finish().await;
}
