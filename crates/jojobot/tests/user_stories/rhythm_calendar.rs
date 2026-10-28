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

use super::dsl::{Answer, Session, Story};

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
    lists(&read, handle)
}

/// Whether a read's objects include the thing at exactly this handle. A handle
/// that is a prefix of another (`rhythm:pay-rent` and `rhythm:pay-rent-late`)
/// is a different thing.
fn lists(read: &Answer, handle: &str) -> bool {
    read.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a read answers with objects: {}", read.raw()))
        .iter()
        .any(|object| object["id"] == handle)
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

    // Each refusal is told from the others by the part of the rule it names.
    let named = |refusal: Answer, part: &str, others: &[&str]| {
        refusal
            .holds_at("/fix_by", "change")
            .says("\"wrote\":false");
        refusal.holds_at("/how_to_proceed", part);
        for other in others {
            refusal.never_holds_at("/how_to_proceed", other);
        }
    };
    named(
        refuse(json!({"cadence_days": "7", "rrule": "FREQ=WEEKLY"})).await,
        "cadence_days",
        &["COUNT", "UNTIL", "BYHOUR", "HOURLY"],
    );
    named(
        refuse(json!({"rrule": "FREQ=HOURLY"})).await,
        "HOURLY",
        &["BYHOUR", "COUNT", "UNTIL", "cadence_days"],
    );
    named(
        refuse(json!({"rrule": "FREQ=WEEKLY;COUNT=3"})).await,
        "COUNT",
        &["UNTIL", "BYHOUR", "HOURLY"],
    );
    named(
        refuse(json!({"rrule": "FREQ=WEEKLY;UNTIL=20271231"})).await,
        "UNTIL",
        &["COUNT", "BYHOUR", "HOURLY"],
    );
    named(
        refuse(json!({"rrule": "FREQ=DAILY;BYHOUR=9"})).await,
        "BYHOUR",
        &["COUNT", "UNTIL", "HOURLY"],
    );
    named(
        refuse(json!({"rrule": "every other tuesday"})).await,
        "RFC 5545",
        &["COUNT", "UNTIL", "BYHOUR", "HOURLY"],
    );
    // The move that ends a loop is archiving it: `ended` finishes no rhythm.
    for rule in ["FREQ=WEEKLY;COUNT=3", "FREQ=WEEKLY;UNTIL=20271231"] {
        refuse(json!({"rrule": rule}))
            .await
            .says("archive_entity")
            .never_says("`ended`");
    }
    // Nothing was written behind any of the refusals, and a rule none of them
    // names is taken.
    s.recall("rhythm:mow-the-lawn").await.never_says("rrule");
    s.call(
        "capture",
        json!({"subject": "rhythm:mow-the-lawn", "content": "mow it", "provenance": "testimony",
               "sets": {"rrule": "FREQ=WEEKLY"}}),
    )
    .await;
    s.recall("rhythm:mow-the-lawn").await.says("\"rrule\"");

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
    // A loop whose handle begins with the first one's, never snoozed: it is
    // owed on the 9th while the first is held back.
    // The near-miss screen refuses a handle that contains another's, so the
    // loop is made with the token the refusal carries.
    let late = json!({"kind": "rhythm", "handle": "pay-rent-late", "name": "Late fee check",
                      "source": "user-named", "parent": "person:homer"});
    let screened = s.refused("add_entity", late.clone()).await;
    let mut forced = late;
    forced["override_token"] = json!(screened.advised("override_token"));
    forced["override_reason"] = json!("a different loop that shares a prefix");
    s.call("add_entity", forced).await;
    s.call(
        "capture",
        json!({"subject": "rhythm:pay-rent-late", "content": "set it up", "provenance": "testimony",
               "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}}),
    )
    .await;
    checked_in(&s, "rhythm:pay-rent-late", "2026-01-01", "ran").await;

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
        owed_on(&s, "rhythm:pay-rent-late", d(2026, 2, 9)).await,
        "the loop that was not snoozed is owed"
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
    assert!(lists(&due, "rhythm:pay-rent"), "{}", due.raw());
    // The snooze moved nothing: the next cycle still counts from the 1st.
    checked_in(&s, "rhythm:pay-rent", "2026-02-10", "ran").await;
    assert_eq!(
        due_day(&s, "rhythm:pay-rent", d(2026, 2, 10)).await,
        d(2026, 3, 1)
    );

    s.wrap("snoozed and paid").await;
    story.finish().await;
}

/// **A cadence alone is a whole schedule on every verb that writes keys, so the
/// half that is refused is `advances_from` with no cadence.** `add_entity`,
/// `update_entity`, `update_fact` and `capture` each take a cadence with no
/// `advances_from` and land. **Paired**: `advances_from` alone is refused, naming
/// the cadence to send.
#[tokio::test]
async fn a_cadence_alone_lands_on_every_writing_verb_and_advances_from_alone_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;

    // A new rhythm takes its cadence in its first claim.
    s.call(
        "add_entity",
        json!({"kind": "rhythm", "handle": "pay-rent", "name": "Pay the rent",
               "source": "user-named", "parent": "person:homer",
               "sets": {"cadence_days": "30"}}),
    )
    .await;
    let rent = s.recall("rhythm:pay-rent").await;
    assert_eq!(rent.json()["objects"][0]["id"], "rhythm:pay-rent");
    rent.says("\"cadence_days\":\"30\"");

    // A rhythm that exists takes it through update_entity …
    s.add_under("person:homer", "rhythm:descale", "Descale the kettle")
        .await;
    s.call(
        "update_entity",
        json!({"handle": "rhythm:descale", "sets": {"cadence_days": "90"}}),
    )
    .await;
    s.recall("rhythm:descale")
        .await
        .says("\"cadence_days\":\"90\"");

    // … and through update_fact, on a claim it already holds.
    s.add_under("person:homer", "rhythm:mow-the-lawn", "Mow the lawn")
        .await;
    let claim = s
        .call(
            "capture",
            json!({"subject": "rhythm:mow-the-lawn", "content": "the first note",
                   "provenance": "testimony"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    s.call(
        "update_fact",
        json!({"address": claim, "sets": {"cadence_days": "7"}}),
    )
    .await;
    s.recall("rhythm:mow-the-lawn")
        .await
        .says("\"cadence_days\":\"7\"");

    // The half that is refused: `advances_from` with no cadence at all.
    s.add_under("person:homer", "rhythm:worming", "Worm the dog")
        .await;
    s.refused(
        "capture",
        json!({"subject": "rhythm:worming", "content": "from the due day",
               "provenance": "testimony", "sets": {"advances_from": "due_date"}}),
    )
    .await
    .says("cadence_days")
    .says("rrule");
    s.recall("rhythm:worming")
        .await
        .never_says("\"advances_from\"");

    s.wrap("a cadence alone lands, half a schedule does not")
        .await;
    story.finish().await;
}

/// **A late check-in counts from the default for its cadence when the rhythm
/// names none.** A number of days counts from the day of the check-in, a calendar
/// rule from the day the cycle fell due. The days loop is ten days late and puts
/// its next due day ten days on. The rule loop is a whole occurrence late, so
/// counted from the day it fell due it is already owed on the day of the check-in.
#[tokio::test]
async fn a_late_check_in_counts_from_the_default_for_its_cadence_when_none_is_named() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:descale", "Descale the kettle")
        .await;
    s.add_under("person:homer", "rhythm:pay-rent", "Pay the rent")
        .await;
    for (handle, sets, late) in [
        (
            "rhythm:descale",
            json!({"cadence_days": "30"}),
            "2026-02-10",
        ),
        (
            "rhythm:pay-rent",
            json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}),
            "2026-03-10",
        ),
    ] {
        s.call(
            "capture",
            json!({"subject": handle, "content": "set the cadence",
                   "provenance": "testimony", "sets": sets}),
        )
        .await;
        checked_in(&s, handle, "2026-01-01", "ran").await;
        checked_in(&s, handle, late, "ran").await;
    }
    // Thirty days from the day it was done (the 10th), not from the day it fell due.
    assert_eq!(
        due_day(&s, "rhythm:descale", d(2026, 2, 10)).await,
        d(2026, 3, 12)
    );
    // Counted from the day the cycle fell due (February 1st), the next 1st is March
    // 1st, which has passed by the check-in on March 10th: the loop is owed at once.
    // Counted from the check-in it would not fall due until April 1st.
    assert!(owed_on(&s, "rhythm:pay-rent", d(2026, 3, 10)).await);

    s.wrap("two cadences, two defaults").await;
    story.finish().await;
}

/// **A loop holding part of a schedule and no cadence comes back overdue, and a
/// loop holding no schedule key does not.** `counts_from` with no cadence reads
/// as unreadable, with the keys it lacks; the loop beside it that holds nothing
/// of a schedule is not late.
#[tokio::test]
async fn a_loop_holding_counts_from_and_no_cadence_is_overdue_and_one_holding_nothing_is_not() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:descale", "Descale the kettle")
        .await;
    s.add_under("person:homer", "rhythm:worming", "Worm the dog")
        .await;
    s.call(
        "capture",
        json!({"subject": "rhythm:descale", "content": "counted from the first",
               "provenance": "testimony", "sets": {"counts_from": "2026-01-01"}}),
    )
    .await;

    let read = s
        .shape(
            "which loops are owed",
            json!({"kind": "rhythm", "overdue": {"as_of": "2026-01-02"}}),
        )
        .await;
    read.says("rhythm:descale").says("unreadable_because");
    read.never_says("rhythm:worming");

    s.wrap("one half-built loop, one with no schedule").await;
    story.finish().await;
}

/// "Stop the leap check."
///
/// **Clearing the rule, the policy and the basis from a calendar loop a
/// check-in has anchored leaves it overdue, and the receipt of the clearing
/// names the key that holds it so.** The first check-in wrote `anchored_on`,
/// which none of the three cleared keys is, so the loop reads as half a
/// schedule until that key goes too.
#[tokio::test]
async fn clearing_a_checked_in_calendar_loop_names_the_anchor_that_keeps_it_overdue() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:leap-check", "Leap check")
        .await;
    let setup = s
        .call(
            "capture",
            json!({"subject": "rhythm:leap-check", "content": "the first of every month",
                   "provenance": "testimony",
                   "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1", "advances_from": "due_date",
                            "counts_from": "2026-01-01"}}),
        )
        .await;
    let setup = setup.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    let check_in = s
        .call(
            "capture",
            json!({"subject": "rhythm:leap-check", "content": "looked at it",
                   "provenance": "testimony", "recorded_at": "2026-01-01",
                   "check_in": "ran"}),
        )
        .await;
    let source = check_in.json()["address"].clone();
    let read = s.recall("rhythm:leap-check").await.json();
    let check_in = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|fact| fact["derived_from"] == source)
        .expect("the computed schedule cites this check-in")["address"]
        .as_str()
        .expect("the schedule has an address")
        .to_string();
    s.recall("rhythm:leap-check").await.says("anchored_on");
    assert!(
        !owed_on(&s, "rhythm:leap-check", d(2026, 1, 2)).await,
        "checked in on the 1st, the loop is not owed on the 2nd before anything is cleared"
    );

    // The schedule's basis comes off its derived record first; it is not a key
    // that starts the advisory, so clearing the setup record is what answers.
    s.call(
        "update_fact",
        json!({"address": check_in, "clear_fields": ["counts_from"]}),
    )
    .await;
    let cleared = s
        .call(
            "update_fact",
            json!({"address": setup,
                   "clear_fields": ["rrule", "advances_from", "counts_from"]}),
        )
        .await;
    cleared.says("anchored_on");
    assert!(
        owed_on(&s, "rhythm:leap-check", d(2026, 1, 2)).await,
        "the loop the advisory is about reads overdue"
    );
    // The anchor is the last key holding it, so clearing it ends the advisory:
    // the loop then holds no part of a schedule and is not owed.
    s.call(
        "update_fact",
        json!({"address": check_in, "clear_fields": ["anchored_on"]}),
    )
    .await;
    assert!(
        !owed_on(&s, "rhythm:leap-check", d(2026, 1, 2)).await,
        "with the anchor cleared nothing holds the loop"
    );

    s.wrap("cleared a checked-in calendar loop").await;
    story.finish().await;
}

/// **A snoozed calendar loop whose schedule is cleared names both keys that
/// hold it**: the anchor the check-in wrote and the day the snooze lasts until.
/// The pair is the receipt of the clearing before the snooze, which names the
/// anchor alone.
#[tokio::test]
async fn clearing_a_snoozed_calendar_loop_names_the_anchor_and_the_snooze() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:leap-check", "Leap check")
        .await;
    let setup = s
        .call(
            "capture",
            json!({"subject": "rhythm:leap-check", "content": "the first of every month",
                   "provenance": "testimony",
                   "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1", "advances_from": "due_date",
                            "counts_from": "2026-01-01"}}),
        )
        .await;
    let setup = setup.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    let check_in = s
        .call(
            "capture",
            json!({"subject": "rhythm:leap-check", "content": "looked at it",
                   "provenance": "testimony", "recorded_at": "2026-01-01",
                   "check_in": "ran"}),
        )
        .await;
    let check_in = check_in.json()["address"]
        .as_str()
        .expect("a check-in answers with the address")
        .to_string();

    // The pair: before any snooze the clearing names the anchor and not the snooze.
    let before = s
        .call(
            "update_fact",
            json!({"address": check_in, "clear_fields": ["counts_from"]}),
        )
        .await;
    before.never_says("snoozed_until");
    let unsnoozed = s
        .call(
            "update_fact",
            json!({"address": setup,
                   "clear_fields": ["rrule", "advances_from", "counts_from"]}),
        )
        .await;
    unsnoozed.says("anchored_on").never_says("snoozed_until");

    // A second loop, snoozed to the 10th of February.
    s.add_under("person:homer", "rhythm:boiler-service", "Boiler service")
        .await;
    let setup = s
        .call(
            "capture",
            json!({"subject": "rhythm:boiler-service", "content": "the first of every month",
                   "provenance": "testimony",
                   "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1", "advances_from": "due_date",
                            "counts_from": "2026-01-01"}}),
        )
        .await;
    let setup = setup.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    checked_in(&s, "rhythm:boiler-service", "2026-01-01", "ran").await;
    s.call(
        "capture",
        json!({"subject": "rhythm:boiler-service", "content": "not this week",
               "provenance": "testimony", "recorded_at": "2026-02-02",
               "check_in": "snoozed", "sets": {"snoozed_until": "2026-02-10"}}),
    )
    .await;
    let snoozed = s
        .call(
            "update_fact",
            json!({"address": setup,
                   "clear_fields": ["rrule", "advances_from", "counts_from"]}),
        )
        .await;
    snoozed.says("anchored_on").says("snoozed_until");

    s.wrap("cleared a snoozed calendar loop").await;
    story.finish().await;
}

/// **The operator's page words a rule it can word and shows the rule itself
/// when it cannot.** A day of the month under MONTHLY reads as words; a weekday
/// under MONTHLY would be worded as something the rule does not say, so the page
/// shows the rule.
#[tokio::test]
async fn the_page_words_a_monthly_day_and_shows_a_monthly_weekday_as_the_rule() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:pay-rent",
        json!({"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}),
    )
    .await;
    a_rhythm(
        &s,
        "rhythm:take-out-bins",
        json!({"rrule": "FREQ=MONTHLY;BYDAY=MO"}),
    )
    .await;

    // The rhythm's box carries the cadence row: the page's words for the rule.
    let cadence = |page: &Answer| -> String {
        page.raw()
            .split_once("<dt>cadence</dt><dd>")
            .and_then(|(_, rest)| rest.split_once("</dd>"))
            .map(|(said, _)| said.to_string())
            .unwrap_or_else(|| panic!("the page has a cadence row: {}", page.raw()))
    };
    let rent = story.page("person:homer/rhythm:pay-rent").await;
    assert_eq!(cadence(&rent), "on day 1 of every month", "{}", rent.raw());
    let bins = story.page("person:homer/rhythm:take-out-bins").await;
    let shown = cadence(&bins);
    assert!(
        shown.contains("FREQ=MONTHLY;BYDAY=MO") && !shown.contains("Monday"),
        "a weekday under MONTHLY is not worded, the rule itself is shown: {shown}"
    );
    assert_ne!(shown, cadence(&rent));

    s.wrap("two rules, one worded").await;
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

/// "The first time I looked at the leap check was in an odd year."
///
/// **The caller's check-in lands even when its computed schedule cannot.**
/// The receipt names that partial write; the loop keeps its old schedule, and
/// a check-in on a day the rule recurs from can still open it.
#[tokio::test]
async fn the_check_in_claim_lands_when_its_opening_schedule_cannot_recur() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    a_rhythm(
        &s,
        "rhythm:leap-check",
        json!({"rrule": "FREQ=YEARLY;INTERVAL=2;BYMONTH=2;BYMONTHDAY=29"}),
    )
    .await;
    s.recall("rhythm:leap-check").await.says("rrule");

    // The first check-in, on an odd-year day the rule never recurs from.
    let opening = s
        .call(
            "capture",
            json!({"subject": "rhythm:leap-check", "content": "looked at it",
                   "provenance": "testimony", "recorded_at": "2027-03-01",
                   "check_in": "ran"}),
        )
        .await;
    let receipt = opening.json();
    assert_eq!(receipt["success"], true, "{receipt}");
    assert_eq!(receipt["fix_by"], "change", "{receipt}");
    assert!(receipt["schedule_not_written"].is_string(), "{receipt}");
    let way = opening.text_at("/schedule_not_written");
    assert!(
        way.contains("2027-03-01"),
        "the failed schedule names the day the rule would run from: {way}"
    );
    let read = s.recall("rhythm:leap-check").await.json();
    let object = &read["objects"][0];
    assert!(object["fields"]["counts_from"].is_null(), "{read}");
    assert!(object["fields"]["anchored_on"].is_null(), "{read}");
    assert!(
        object["facts"]
            .as_array()
            .expect("claims")
            .iter()
            .any(
                |fact| fact["address"] == receipt["address"] && fact["recorded_at"] == "2027-03-01"
            ),
        "the caller's claim remains readable: {read}"
    );

    // The same check-in on an even-year day lands.
    checked_in(&s, "rhythm:leap-check", "2028-03-01", "ran").await;
    let read = s.recall("rhythm:leap-check").await.json();
    let fields = &read["objects"][0]["fields"];
    assert_eq!(fields["counts_from"], "2028-03-01", "{read}");
    assert_eq!(fields["anchored_on"], "2028-03-01", "{read}");

    s.wrap("checked in on the wrong year and the right one")
        .await;
    story.finish().await;
}

/// "Move the day it counts from."
///
/// **A write that moves the day a rule runs from is asked the same question**,
/// with the rule unchanged. A day the rule never recurs from is refused and the
/// old day stays; a day it does recur from lands.
#[tokio::test]
async fn moving_the_day_a_rule_runs_from_to_a_day_it_never_recurs_from_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.add_under("person:homer", "rhythm:leap-check", "Leap check")
        .await;
    let made = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:leap-check", "content": "every other leap day",
                "provenance": "testimony",
                "sets": {
                    "rrule": "FREQ=YEARLY;INTERVAL=2;BYMONTH=2;BYMONTHDAY=29",
                    "advances_from": "due_date", "counts_from": "2028-03-01",
                },
            }),
        )
        .await;
    let address = made.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();

    let refused = s
        .refused(
            "update_fact",
            json!({"address": address, "sets": {"counts_from": "2029-03-01"}}),
        )
        .await;
    let way = refused.text_at("/how_to_proceed");
    assert!(
        way.contains("2029-03-01"),
        "the refusal names the day the rule would run from: {way}"
    );
    s.recall("rhythm:leap-check")
        .await
        .says("2028-03-01")
        .never_says("2029-03-01");

    s.correct_fields(&address, json!({"counts_from": "2030-03-01"}), &[])
        .await;
    s.recall("rhythm:leap-check").await.says("2030-03-01");

    s.wrap("moved the day the rule runs from, wrongly and rightly")
        .await;
    story.finish().await;
}
