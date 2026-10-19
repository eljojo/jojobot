//! "Pay the rent on the 1st of every month."
//!
//! A rhythm can recur on the calendar, written as an RFC 5545 RRULE, not only
//! every N days. The due day is read the way every rhythm's is: the overdue
//! question is asked of a day, so a due day is the first day the loop is owed
//! and the day before it is not.
//!
//! The roster names are the cast: Homer's rent, Marge's bins, a boiler service,
//! a fortnightly Thursday, and a yearly March the first.

use jiff::civil::Date;
use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// A rhythm under Homer, with the keys it is made with.
async fn a_rhythm(s: &Session, handle: &str, sets: Value) {
    // A name that differs from every other loop's, as a person names them.
    let name = handle
        .trim_start_matches("rhythm:")
        .split('-')
        .map(|word| {
            let mut letters = word.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    s.add_under("person:homer", handle, &name).await;
    s.call(
        "capture",
        json!({"subject": handle, "content": "set it up", "provenance": "testimony",
               "sets": sets}),
    )
    .await;
}

/// A check-in on a day.
async fn checked_in(s: &Session, handle: &str, day: &str, outcome: &str) {
    s.call(
        "capture",
        json!({"subject": handle, "content": "looked at it", "provenance": "testimony",
               "recorded_at": day, "check_in": outcome}),
    )
    .await;
}

/// Whether the loop is owed on that day, by the overdue read.
async fn owed_on(s: &Session, handle: &str, day: Date) -> bool {
    let read = s
        .shape(
            "which loops are owed",
            json!({"kind": "rhythm", "overdue": {"as_of": day.to_string()}}),
        )
        .await;
    read.raw().contains(&format!("\"{handle}\"")) || read.raw().contains(handle)
}

/// **The day the loop falls due**: the first day, after `from`, that it is owed.
async fn due_day(s: &Session, handle: &str, from: Date) -> Date {
    let mut day = from;
    for _ in 0..800 {
        day = day.tomorrow().expect("a day");
        if owed_on(s, handle, day).await {
            return day;
        }
    }
    panic!("{handle} never fell due within 800 days of {from}");
}

fn d(y: i16, m: i8, day: i8) -> Date {
    Date::new(y, m, day).expect("a real day")
}

#[tokio::test]
async fn the_rent_falls_due_on_the_first_of_every_month() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    // No `advances_from` and no basis: the opening check-in supplies the basis.
    a_rhythm(
        &s,
        "rhythm:pay-rent",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}),
    )
    .await;

    checked_in(&s, "rhythm:pay-rent", "2026-01-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:pay-rent", d(2026, 1, 1)).await,
        d(2026, 2, 1)
    );
    checked_in(&s, "rhythm:pay-rent", "2026-02-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:pay-rent", d(2026, 2, 1)).await,
        d(2026, 3, 1)
    );
    // A late check-in leaves the next due on the 1st after the one it was late for.
    checked_in(&s, "rhythm:pay-rent", "2026-03-04", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:pay-rent", d(2026, 3, 4)).await,
        d(2026, 4, 1)
    );

    s.wrap("paid the rent").await;
    story.finish().await;
}

#[tokio::test]
async fn the_last_day_of_the_month_by_setpos_and_by_a_negative_day() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:read-the-meter",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=28,29,30,31;BYSETPOS=-1"}),
    )
    .await;
    a_rhythm(
        &s,
        "rhythm:take-out-bins",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=-1"}),
    )
    .await;

    for handle in ["rhythm:read-the-meter", "rhythm:take-out-bins"] {
        checked_in(&s, handle, "2027-01-01", "ran").await;
        let mut from = d(2027, 1, 1);
        // January 31, February 28 in 2027, March 31, April 30.
        for expected in [
            d(2027, 1, 31),
            d(2027, 2, 28),
            d(2027, 3, 31),
            d(2027, 4, 30),
        ] {
            let due = due_day(&s, handle, from).await;
            assert_eq!(due, expected, "{handle}");
            checked_in(&s, handle, &due.to_string(), "ran").await;
            from = due;
        }
    }

    s.wrap("read the meter and took out the bins").await;
    story.finish().await;
}

#[tokio::test]
async fn the_first_saturday_and_the_last_friday_of_the_month() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:boiler-service",
        json!({"rrule": "FREQ=MONTHLY;BYDAY=1SA"}),
    )
    .await;
    a_rhythm(
        &s,
        "rhythm:close-the-books",
        json!({"rrule": "FREQ=MONTHLY;BYDAY=-1FR"}),
    )
    .await;

    checked_in(&s, "rhythm:boiler-service", "2026-01-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:boiler-service", d(2026, 1, 1)).await,
        d(2026, 1, 3)
    );
    checked_in(&s, "rhythm:close-the-books", "2026-01-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:close-the-books", d(2026, 1, 1)).await,
        d(2026, 1, 30)
    );

    s.wrap("scheduled the service").await;
    story.finish().await;
}

#[tokio::test]
async fn a_fortnightly_thursday_keeps_its_phase_across_a_late_check_in() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    // Counted from the day it was done, so a late check-in is off the Thursdays.
    a_rhythm(
        &s,
        "rhythm:water-the-fern",
        json!({"rrule": "FREQ=WEEKLY;INTERVAL=2;BYDAY=TH", "advances_from": "check_in_date"}),
    )
    .await;

    // Opened on Thursday 2026-01-01: the Thursdays are the 1st, 15th, 29th.
    checked_in(&s, "rhythm:water-the-fern", "2026-01-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:water-the-fern", d(2026, 1, 1)).await,
        d(2026, 1, 15)
    );
    // Checked in late, on the Thursday of the OFF week: the next one is still on the
    // opening Thursdays, the 29th, and not a week after the check-in.
    checked_in(&s, "rhythm:water-the-fern", "2026-01-22", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:water-the-fern", d(2026, 1, 22)).await,
        d(2026, 1, 29)
    );
    // And a skipped cycle after that stays on the same Thursdays.
    checked_in(&s, "rhythm:water-the-fern", "2026-02-03", "skipped").await;
    assert_eq!(
        due_day(&s, "rhythm:water-the-fern", d(2026, 2, 3)).await,
        d(2026, 2, 12)
    );

    s.wrap("watered the fern").await;
    story.finish().await;
}

#[tokio::test]
async fn every_march_the_first() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:renew-the-licence",
        json!({"rrule": "FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=1"}),
    )
    .await;
    checked_in(&s, "rhythm:renew-the-licence", "2026-01-10", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:renew-the-licence", d(2026, 1, 10)).await,
        d(2026, 3, 1)
    );
    checked_in(&s, "rhythm:renew-the-licence", "2026-03-05", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:renew-the-licence", d(2026, 3, 5)).await,
        d(2027, 3, 1)
    );

    s.wrap("renewed it").await;
    story.finish().await;
}

#[tokio::test]
async fn a_bare_day_thirty_one_skips_the_months_without_one() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:pay-the-water",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=31"}),
    )
    .await;
    checked_in(&s, "rhythm:pay-the-water", "2026-01-01", "ran").await;
    let mut from = d(2026, 1, 1);
    // The RFC skips February and April; it does not fall back to the month's last day.
    for expected in [d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31)] {
        let due = due_day(&s, "rhythm:pay-the-water", from).await;
        assert_eq!(due, expected);
        checked_in(&s, "rhythm:pay-the-water", &due.to_string(), "ran").await;
        from = due;
    }

    s.wrap("paid the water").await;
    story.finish().await;
}

#[tokio::test]
async fn a_number_of_days_reads_as_it_always_did() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:swap-the-filter",
        json!({"cadence_days": "30", "advances_from": "due_date"}),
    )
    .await;
    checked_in(&s, "rhythm:swap-the-filter", "2026-01-01", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:swap-the-filter", d(2026, 1, 1)).await,
        d(2026, 1, 31)
    );

    s.wrap("swapped the filter").await;
    story.finish().await;
}

#[tokio::test]
async fn a_rhythm_repeats_one_way_and_a_rule_a_rhythm_cannot_use_is_refused_with_the_fix() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:mow-the-lawn", "Mow the lawn")
        .await;
    let refuse = |sets: Value| {
        s.refused(
            "capture",
            json!({"subject": "rhythm:mow-the-lawn", "content": "mow it", "provenance": "testimony",
                   "sets": sets}),
        )
    };

    refuse(json!({"cadence_days": "7", "rrule": "FREQ=WEEKLY"}))
        .await
        .says("cadence_days")
        .says("rrule");
    refuse(json!({"rrule": "FREQ=HOURLY"})).await.says("day");
    refuse(json!({"rrule": "FREQ=WEEKLY;COUNT=3"}))
        .await
        .says("ended");
    refuse(json!({"rrule": "FREQ=WEEKLY;UNTIL=20271231"}))
        .await
        .says("ended");
    refuse(json!({"rrule": "FREQ=DAILY;BYHOUR=9"}))
        .await
        .says("date");
    refuse(json!({"rrule": "every other tuesday"}))
        .await
        .says("RFC 5545");
    // Nothing was written behind any of the refusals.
    s.recall("rhythm:mow-the-lawn").await.never_says("rrule");

    s.wrap("tried the lawn five ways").await;
    story.finish().await;
}

#[tokio::test]
async fn a_snooze_never_moves_a_calendar_schedule_and_the_due_view_agrees() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:pay-rent",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}),
    )
    .await;
    checked_in(&s, "rhythm:pay-rent", "2026-01-01", "ran").await;

    // Due 2026-02-01. A snooze to the 10th holds it back; it is owed again on the 10th.
    s.call(
        "capture",
        json!({"subject": "rhythm:pay-rent", "content": "not this week", "provenance": "testimony",
               "recorded_at": "2026-02-02", "check_in": "snoozed",
               "sets": {"snoozed_until": "2026-02-10"}}),
    )
    .await;
    assert!(
        !owed_on(&s, "rhythm:pay-rent", d(2026, 2, 9)).await,
        "held back"
    );
    assert!(
        owed_on(&s, "rhythm:pay-rent", d(2026, 2, 10)).await,
        "back on the day it named"
    );
    // The shipped view lists it by the same moment.
    let due = s
        .call(
            "recall",
            json!({"view": "due", "inputs": {"by": "2026-02-10"}}),
        )
        .await;
    due.says("rhythm:pay-rent");
    // The snooze moved nothing: the next cycle still counts from the 1st.
    checked_in(&s, "rhythm:pay-rent", "2026-02-10", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:pay-rent", d(2026, 2, 10)).await,
        d(2026, 3, 1)
    );

    s.wrap("snoozed and paid").await;
    story.finish().await;
}

#[tokio::test]
async fn a_rhythm_with_no_advances_from_gets_the_default_for_its_cadence_and_says_so() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:pay-rent", "Pay the rent")
        .await;
    s.add_under("person:homer", "rhythm:descale", "Descale the kettle")
        .await;

    // A calendar rule counts from the day it fell due; the receipt names it.
    s.call(
        "capture",
        json!({"subject": "rhythm:pay-rent", "content": "monthly", "provenance": "testimony",
               "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}}),
    )
    .await
    .says("advances_from")
    .says("due_date");
    // A number of days counts from the day it was done.
    s.call(
        "capture",
        json!({"subject": "rhythm:descale", "content": "monthly", "provenance": "testimony",
               "sets": {"cadence_days": "30"}}),
    )
    .await
    .says("advances_from")
    .says("check_in_date");
    // The default is not written onto the rhythm, so a named policy still wins.
    s.recall("rhythm:descale")
        .await
        .never_says("\"advances_from\"");
    s.call(
        "capture",
        json!({"subject": "rhythm:descale", "content": "from the due day", "provenance": "testimony",
               "sets": {"advances_from": "due_date"}}),
    )
    .await;
    checked_in(&s, "rhythm:descale", "2026-01-01", "ran").await;
    checked_in(&s, "rhythm:descale", "2026-02-10", "ran").await;
    // Counted from the due day (the 31st), not from the 10th: due on March 2.
    assert_eq!(
        due_day(&s, "rhythm:descale", d(2026, 2, 10)).await,
        d(2026, 3, 2)
    );

    s.wrap("defaulted and named").await;
    story.finish().await;
}

/// "Every other year, on the 29th of February."
///
/// **A rule is checked against the day the loop counts from.** The same rule
/// recurs from an even year, where the 29th of February falls in the second
/// year, and never from an odd one. Written with the day it counts from, the
/// rule that never recurs is refused naming that day, and the one that recurs
/// lands. Nothing is left behind by the refusal.
#[tokio::test]
async fn a_rule_that_never_recurs_from_the_loops_own_day_is_refused_when_written() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:leap-check", "Leap check")
        .await;
    let sets = |counts_from: &str| {
        json!({
            "subject": "rhythm:leap-check", "content": "every other leap day",
            "provenance": "testimony",
            "sets": {
                "rrule": "FREQ=YEARLY;INTERVAL=2;BYMONTH=2;BYMONTHDAY=29",
                "advances_from": "due_date", "counts_from": counts_from,
            },
        })
    };

    let refused = s.refused("capture", sets("2027-03-01")).await;
    let way = refused.text_at("/how_to_proceed");
    assert!(
        way.contains("2027-03-01"),
        "the refusal names the day the rule would run from: {way}"
    );
    s.recall("rhythm:leap-check").await.never_says("rrule");

    // The positive the refusal rests on: the same rule from an even year lands.
    s.call("capture", sets("2028-03-01")).await;
    s.recall("rhythm:leap-check").await.says("rrule");

    s.wrap("tried a rule from the wrong year and the right one")
        .await;
    story.finish().await;
}
