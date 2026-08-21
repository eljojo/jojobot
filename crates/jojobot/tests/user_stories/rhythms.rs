//! "Which of my loops have gone quiet?"
//!
//! Three recurring things on three different cadences, kept in one place. The
//! operator asks one question and is told which have fallen due — and the
//! answer is worth having only because a loop that has NOT fallen due sits in
//! the same store and stays out of it.
//!
//! **A rhythm is an entity, and its parent says whose job it is.** A
//! maintenance loop sits under the thing maintained; a review loop sits under
//! the bot that has to carry it. Two loops on one object would be two handles,
//! which is why they are entities rather than a label. A rhythm with no parent
//! is refused: that is a modelling failure and not a valid shape.
//!
//! **The three outcomes are the second half.** A run and a skip advance the
//! schedule identically, and only the record tells them apart — which is the
//! whole reason there are three words instead of two. A snooze consumes
//! nothing, so the rhythm comes back at its own date.
//!
//! **Every date here is written down.** The read takes the day it is asked
//! about, so this story says nothing about today and nothing rots when the
//! calendar moves.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn which_of_the_loops_have_gone_quiet() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the things the loops are ON ─────────────────────────────────────────
    s.add("pet:snowball", "Snowball").await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;

    // ── a rhythm with nothing to hang on is refused ──────────────────────────
    //
    // The parent is what says whose job the loop is. Without it the record is a
    // recurring nothing, so the write is turned back rather than filed flat.
    s.refused(
        "add_entity",
        json!({
            "kind": "rhythm", "handle": "orphan", "name": "Orphan",
            "source": "user-named",
        }),
    )
    .await
    .says("parent")
    .says("\"wrote\":false");
    s.list("rhythm").await.never_says("rhythm:orphan");

    // ── three loops, three cadences, each under whoever owns it ─────────────
    s.add_under("pet:snowball", "rhythm:worming", "Worming")
        .await;
    s.add_under("thing:gravel-bike", "rhythm:chain-check", "Chain check")
        .await;
    // A review loop belongs to the bot that has to carry it through. A bot is
    // an entity like any other, so this needs nothing of its own.
    s.add_under("bot:otto", "rhythm:weekly-review", "Weekly review")
        .await;

    // The schedules. `advances_from` has no default and each of these picks its
    // own, because the two answers only diverge when a check-in is late — and
    // that is exactly when picking wrong stops being visible.
    s.event_with(
        "rhythm:worming",
        "the vet said every ninety days",
        json!({
            "cadence_days": "90",
            "advances_from": "due_date",
            "counts_from": "2026-04-01",
        }),
        &[],
    )
    .await;
    s.event_with(
        "rhythm:chain-check",
        "look at the chain every couple of months",
        json!({
            "cadence_days": "60",
            "advances_from": "check_in_date",
            "counts_from": "2026-06-01",
        }),
        &[],
    )
    .await;
    s.event_with(
        "rhythm:weekly-review",
        "look back over the week",
        json!({
            "cadence_days": "7",
            "advances_from": "due_date",
            "counts_from": "2026-06-25",
        }),
        &[],
    )
    .await;

    // ── the question ────────────────────────────────────────────────────────
    //
    // The fifth of July. The worming fell due on the thirtieth of June and the
    // review on the second of July; the chain is not due until the thirty-first.
    let quiet = s
        .shape(
            "which loops have gone quiet by the fifth of July",
            json!({ "kind": "rhythm", "overdue": { "as_of": "2026-07-05" } }),
        )
        .await;
    quiet.says("rhythm:worming");
    quiet.says("rhythm:weekly-review");
    // **The negative that makes the answer mean anything.** A read that came
    // back with everything would read exactly like this one.
    quiet.never_says("rhythm:chain-check");
    // And the answer says which day it was asked about, so a reader can check
    // the arithmetic instead of trusting it.
    quiet.says("\"overdue_as_of\":\"2026-07-05\"");

    s.journal("offered the two that had gone quiet; the operator took one")
        .await;

    // ── closing them, three different ways ──────────────────────────────────
    //
    // The review ran. It advances from the date it FELL DUE, so the next one is
    // a week after the second of July and not a week after today — which is
    // what stops a late review from walking the whole schedule forward.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:weekly-review", "content": "did the review, an hour",
            "provenance": "testimony", "date": "2026-07-05", "check_in": "ran",
        }),
    )
    .await
    // Three keys computed on the way in: the outcome, the day, and the date the
    // next cycle counts from.
    .says("\"fields_count\":3");

    // The worming is not happening this week and it is not being written off:
    // a snooze consumes nothing, so it comes back at its own date.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:worming", "content": "no tablets in the house",
            "provenance": "testimony", "date": "2026-07-05", "check_in": "snoozed",
        }),
    )
    .await
    // TWO keys, not three. A snooze records that it was looked at and moves no
    // date, so there is no write of the schedule at all — which is the same
    // fact the read below states as an answer.
    .says("\"fields_count\":2");

    // ── the same question, a day later ──────────────────────────────────────
    let after = s
        .shape(
            "and now, on the sixth",
            json!({ "kind": "rhythm", "overdue": { "as_of": "2026-07-06" } }),
        )
        .await;
    // The review is closed and gone from the answer…
    after.never_says("rhythm:weekly-review");
    // …and the worming is still here, which is the difference between the two
    // outcomes stated as an answer rather than as a field.
    after.says("rhythm:worming");

    s.wrap("two loops closed, and they closed differently")
        .await;

    // ── session 2 · a skip is not a run, and the record is where it shows ───
    let s = story.session().await;

    // The chain check falls due on the thirty-first of July. It did not happen,
    // and the cycle moves on anyway.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:chain-check", "content": "away all month, did not look at it",
            "provenance": "testimony", "date": "2026-08-01", "check_in": "skipped",
        }),
    )
    .await
    // Three again: a skip advances the schedule exactly as a run does.
    .says("\"fields_count\":3");

    // A cadence is always TIME, so what the check measured rides on the record
    // as the caller's own key. This one ran, and it read something.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:chain-check", "content": "checked it, chain still fine",
            "provenance": "testimony", "date": "2026-09-20", "check_in": "ran",
            "fields": { "wear_mm": "0.4" },
        }),
    )
    .await
    .says("\"fields_count\":4");

    // **The statistics question.** A skipped cycle and a completed one moved
    // the schedule identically, and the history of the outcome key is the only
    // place they are still told apart.
    let history = s
        .shape(
            "how the chain check has gone",
            json!({
                "subject": "rhythm:chain-check",
                "history": "outcome",
            }),
        )
        .await;
    history.says("skipped");
    history.says("ran");
    history.number("/objects/0/history/count", 2);
    // The measurement is on the thing, kept as it was written.
    history.says("\"wear_mm\":\"0.4\"");

    // ── a check-in on something that is not a loop ──────────────────────────
    //
    // The vocabulary belongs to rhythms. On anything else it is refused rather
    // than written as keys that mean nothing where they landed.
    s.refused(
        "capture",
        json!({
            "subject": "pet:snowball", "content": "fed her",
            "provenance": "testimony", "date": "2026-09-20", "check_in": "ran",
        }),
    )
    .await
    .says("\"wrote\":false");

    s.wrap("a skip and a run, and only the record tells them apart")
        .await;
    story.finish().await;
}
