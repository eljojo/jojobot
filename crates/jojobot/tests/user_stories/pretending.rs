//! "Pretend it is the 1st of June. Record that I ate an apple today."
//! …and later: "Pretend it is the 1st of July. When did I eat an apple?"
//!
//! **A run acting out a date should use jojobot as it would on the day.** The
//! frame belongs to the caller, and a caller could already state its day on
//! every call — but stating it is a thing somebody has to remember, and a
//! session reads *it is the 14th of June*, writes perfectly in-period prose,
//! and never thinks to tell the tool what day it is. You do not inform a
//! notebook of the date.
//!
//! So the server takes a day for its whole run and its `now` becomes that day.
//! Four things move with it, and this story asks for all four: the date an
//! undated note gets, whether something recurring has come due, whether a run
//! from June is still open in July, and when jojobot recorded a thing.
//!
//! ⚠️ **A server acting out a day that did not SAY so is the silently-wrong
//! state.** It looks exactly like a healthy one, and every date it fills in is
//! fiction. So the door announces it, and the negative below is half the
//! story: a server nobody told a day to announces nothing and runs on the real
//! clock.
//!
//! Nothing here passes a date on any call. That is what is being measured.

use serde_json::json;

use super::dsl::Story;

/// The day the first sitting acts out.
const JUNE: &str = "2026-06-01";
/// The day the second sitting acts out — the same world, served again.
const JULY: &str = "2026-07-01";

#[tokio::test]
async fn a_server_acting_out_a_day_answers_in_that_day_without_being_told_the_date() {
    let june = Story::begin_pretending_it_is("bot:otto", JUNE).await;

    // ── the door says which clock this is, before anything is written ───────
    let (boot, sitting) = june.full_boot().await;
    assert_eq!(
        boot["clock"]["stated_day"], JUNE,
        "a server acting out a day says so at the door: {boot}"
    );

    // ── the thing, and a loop that has NOT gone quiet on the 1st of June ────
    //
    // Its cadence runs out on the 4th, so the June sitting is early and the
    // July sitting is a month late. On the real clock both sittings would find
    // it quiet and this story would prove nothing.
    sitting.add("person:milhouse", "Milhouse").await;
    sitting.add("thing:kettle", "The kettle").await;
    sitting
        .add_under("thing:kettle", "rhythm:descale", "Descale the kettle")
        .await;
    sitting
        .event_with(
            "rhythm:descale",
            "descaled it",
            json!({
                "name": "Descale the kettle",
                "last_check_in": "2026-05-28",
                "counts_from": "2026-05-28",
                "advances_from": "due_date",
                "cadence_days": "7",
            }),
            &[],
        )
        .await;

    // ── ONE: an undated note gets the stated day, and nobody typed it ───────
    let stamped = sitting
        .undated_fact("person:milhouse", "ate an apple")
        .await;
    assert_eq!(
        stamped, JUNE,
        "the note was stamped with the day the run really happened",
    );

    // ── FOUR: and when jojobot took it in is that day too ───────────────────
    //
    // 🚨 **The stamp is the store's own and no caller can name it.** A build
    // that moved the caller-facing dates and left this one would date every
    // record of the simulation to the day somebody ran it, which is the
    // reading this column exists to give correctly.
    let took_it_in = sitting
        .recall("person:milhouse")
        .await
        .says("ate an apple")
        // The first claim on this thing, so the first address on its page.
        .claim("person:milhouse#f1")
        .field("inserted_at");
    assert!(
        took_it_in.starts_with(JUNE),
        "jojobot recorded it on the day it is acting out, not on the wall clock: {took_it_in}",
    );

    // ── TWO: the loop has not come due, because it is the 1st of June ───────
    sitting
        .shape(
            "what has gone quiet",
            json!({ "kind": "rhythm", "overdue": {} }),
        )
        .await
        .says(&format!("\"overdue_as_of\":\"{JUNE}\""))
        .never_says("rhythm:descale");

    // The sitting ends without being wrapped, the way a sitting that ran out of
    // time does — it is what the next sitting has to decide about.
    sitting.journal("ate an apple, wrote it down").await;

    // ── and the sitting's own run is still open, inside the day ─────────────
    //
    // 🚨 **The other half of THREE, and it is the half that bites.** A server
    // whose sweep decided on the wall clock would judge a run it began minutes
    // ago against a date two months later and abandon it — a simulated
    // instance quietly throwing away its own live sessions. It is offered
    // back, which is what a run with work in flight is for.
    let (offered, none) = june
        .call("start_here", json!({ "bot": "otto", "brief": true }))
        .await;
    assert!(
        none.is_none(),
        "a boot with a run worth picking up hands back the choice, not a handle",
    );
    offered
        .says("\"working_on\"")
        // **And the run's own record began in June.** A card stamped on the
        // wall clock would say the run started two months after the day it did
        // its work in, in the one field the offer shows a caller.
        .says(&format!("\"started_at\":\"{JUNE}"))
        // **And so did its last beat**, which is the moment jojobot itself
        // stamped when it noticed the write — the other half of the run's own
        // record, and the one the sweep reads.
        .says(&format!("\"last_beat\":\"{JUNE}"));

    // ── the second sitting: the same world, a month later ───────────────────
    let july = june.restarted_pretending_it_is(JULY).await;
    let (boot, later) = july.full_boot().await;
    assert_eq!(
        boot["clock"]["stated_day"], JULY,
        "the second sitting acts out its own day: {boot}"
    );

    // ── THREE: June's run is not still open in July ─────────────────────────
    //
    // The boot hands back a handle rather than the resume-or-new choice: a run
    // whose last beat was a month ago is not work in flight. On the real clock
    // both sittings happen within seconds of each other and June's run is
    // offered straight back.
    assert!(
        boot["session"]["choices"].is_null(),
        "a run from June was offered back to a July that never arrived: {boot}",
    );
    assert!(
        !boot["session"]["swept"]
            .as_array()
            .expect("the boot says which runs it swept")
            .is_empty(),
        "June's run was neither swept nor offered — the sweep never saw it: {boot}",
    );

    // ── "when did I eat an apple?" — in June ────────────────────────────────
    later
        .recall("person:milhouse")
        .await
        .says("ate an apple")
        .says(&format!("\"recorded_at\":\"{JUNE}\""));

    // ── and TWO from the other side: the loop has gone quiet now ────────────
    //
    // The pair is what says the June answer was a reading of June rather than
    // a read that finds nothing for anybody.
    later
        .shape(
            "what has gone quiet",
            json!({ "kind": "rhythm", "overdue": {} }),
        )
        .await
        .says(&format!("\"overdue_as_of\":\"{JULY}\""))
        .says("rhythm:descale");

    // ── a caller that names its own date still wins ─────────────────────────
    //
    // The stated day is a second source under the caller, never a replacement:
    // a sitting recording what happened on another day says so and is obeyed.
    later
        .fact_on(
            "person:milhouse",
            "had been ill the week before",
            "2026-06-22",
        )
        .await;
    later
        .recall("person:milhouse")
        .await
        .says("\"recorded_at\":\"2026-06-22\"");

    later.wrap("read June back from July").await;
    july.finish().await;
    june.finish().await;
}

/// **The control, and the whole feature rests on it.** An instance nobody told
/// a day to must answer exactly as it did before one could be told: the real
/// clock, and no announcement to read.
///
/// ⛔️ A fictional clock leaking into an ordinary run is worse than the bug it
/// fixes.
#[tokio::test]
async fn a_server_nobody_told_a_day_runs_on_the_real_clock_and_says_nothing() {
    let story = Story::begin("bot:otto").await;

    let (boot, session) = story.full_boot().await;
    assert!(
        boot["clock"].is_null(),
        "an ordinary server announces no stated day: {boot}",
    );

    session.add("person:milhouse", "Milhouse").await;
    let stamped = session
        .undated_fact("person:milhouse", "ate an apple")
        .await;
    assert_eq!(
        stamped,
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            .to_string(),
        "an undated note on an ordinary server is stamped with today",
    );

    session.wrap("recorded one thing, today").await;
    story.finish().await;
}
