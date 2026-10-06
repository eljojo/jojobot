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

    // **What is Snowball's own job — as opposed to everyone's?** The three
    // loops sit under three different things; asking what is directly under
    // the cat has to reach the worming loop and nothing that belongs to the
    // bike or to Otto.
    let snowballs = s
        .call("list_entities", json!({"parent": "pet:snowball"}))
        .await;
    snowballs.says("rhythm:worming");
    snowballs.never_says("rhythm:chain-check");
    snowballs.never_says("rhythm:weekly-review");

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
            "provenance": "testimony", "recorded_at": "2026-07-05", "check_in": "ran",
        }),
    )
    .await
    // Four keys computed on the way in: the outcome, the day, the date the
    // next cycle counts from, and the due moment that basis now works out to.
    .says("\"fields_count\":4");

    // The worming is not happening this week and it is not being written off:
    // a snooze consumes nothing, so it comes back at its own date.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:worming", "content": "no tablets in the house",
            "provenance": "testimony", "recorded_at": "2026-07-05", "check_in": "snoozed",
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
            "provenance": "testimony", "recorded_at": "2026-08-01", "check_in": "skipped",
        }),
    )
    .await
    // Four again: a skip advances the schedule, and the due moment, exactly
    // as a run does.
    .says("\"fields_count\":4");

    // A cadence is always TIME, so what the check measured rides on the record
    // as the caller's own key. This one ran, and it read something.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:chain-check", "content": "checked it, chain still fine",
            "provenance": "testimony", "recorded_at": "2026-09-20", "check_in": "ran",
            "fields": { "wear_mm": "0.4" },
        }),
    )
    .await
    // Outcome, day, counts_from, wear_mm, and the due moment that basis now
    // works out to.
    .says("\"fields_count\":5");

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
            "provenance": "testimony", "recorded_at": "2026-09-20", "check_in": "ran",
        }),
    )
    .await
    .says("\"wrote\":false");

    // ── the loop was filed under the wrong bike from the start ───────────────
    //
    // It turns out the chain being tracked was always the road bike's, not
    // the gravel bike's — the loop itself is correctly named, only its
    // parent is wrong. `to` names the same handle back: a reparent needs no
    // reslug, since the two are independent axes of this one verb. An
    // earlier draft worked around a since-fixed guard by renaming the loop
    // to `rhythm:road-bike-chain-check` alongside the reparent — the fix
    // below is what let this beat go back to naming the same handle.
    s.add("thing:road-bike", "The Road Bike").await;
    s.call(
        "rename_entity",
        json!({
            "handle": "rhythm:chain-check", "to": "rhythm:chain-check",
            "parent": "thing:road-bike",
        }),
    )
    .await;

    // **The positive `parent` rests on**: asking what is directly under the
    // road bike now reaches the loop, and the gravel bike no longer answers
    // for it.
    s.call("list_entities", json!({"parent": "thing:road-bike"}))
        .await
        .says("rhythm:chain-check");
    s.call("list_entities", json!({"parent": "thing:gravel-bike"}))
        .await
        .never_says("rhythm:chain-check");

    // **The negative: naming one handle moves only that one.** Snowball's
    // own loop was never named in the call, so it is exactly where it was.
    s.call("list_entities", json!({"parent": "pet:snowball"}))
        .await
        .says("rhythm:worming");

    s.wrap("a skip and a run, and only the record tells them apart")
        .await;
    story.finish().await;
}

/// **A loop that cannot say when its next cycle falls due is refused when it
/// is MADE, not when it is first used.** A cold model made a rhythm carrying a
/// cadence and no `advances_from`; the store took it, and the model learned
/// what was wrong only when it tried to check in. By then the loop existed and
/// was wrong.
///
/// Three loops, three outcomes: the one made the way that model made it is
/// refused, naming the missing key and the values it takes; the one made
/// whole takes its check-in; and the one with no schedule at all — a loop with
/// no cadence is a legitimate loop — is still made.
#[tokio::test]
async fn a_loop_that_cannot_say_when_it_falls_due_is_refused_when_it_is_made() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("thing:kettle", "The Kettle").await;
    s.add_under("thing:kettle", "rhythm:descale", "Descale")
        .await;
    s.add("thing:the-air-filter", "The Air Filter").await;
    s.add_under(
        "thing:the-air-filter",
        "rhythm:swap-the-air-filter",
        "Swap the air filter",
    )
    .await;

    // ── the loop made the way the model made it ─────────────────────────────
    //
    // A cadence, and nothing to say which date the next cycle counts from.
    s.refused(
        "capture",
        json!({
            "subject": "rhythm:descale", "content": "descale the kettle monthly",
            "provenance": "testimony",
            "fields": { "cadence_days": "30", "last_check_in": "2026-08-01" },
        }),
    )
    .await
    // The refusal names the key that is missing and the values it takes, so
    // the next call can succeed.
    .says("advances_from")
    .says("check_in_date")
    .says("due_date")
    .says("\"wrote\":false");
    // **Nothing was written.** The refusal is only worth reading if the loop
    // is not half-made behind it.
    s.recall("rhythm:descale")
        .await
        .never_says("\"cadence_days\"");

    // ── the same loop, made whole ───────────────────────────────────────────
    let descale = s
        .event_with(
            "rhythm:descale",
            "descale the kettle monthly",
            json!({
                "cadence_days": "30", "advances_from": "due_date",
                "last_check_in": "2026-08-01",
            }),
            &[],
        )
        .await;
    s.recall("rhythm:descale").await.says("\"cadence_days\"");

    // It takes its check-in, and the check-in is what opens the loop: the
    // next cycle counts from the thirty-first of August.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:descale", "content": "descaled it",
            "provenance": "testimony", "recorded_at": "2026-08-31", "check_in": "ran",
        }),
    )
    .await;
    let early = s
        .shape(
            "which loops have gone quiet by the fifth of September",
            json!({ "kind": "rhythm", "overdue": { "as_of": "2026-09-05" } }),
        )
        .await;
    early.never_says("rhythm:descale");
    let late = s
        .shape(
            "which loops have gone quiet by the second of October",
            json!({ "kind": "rhythm", "overdue": { "as_of": "2026-10-02" } }),
        )
        .await;
    late.says("rhythm:descale");

    // ── a loop with no schedule at all is still a loop ──────────────────────
    //
    // The air filter is swapped when somebody notices. It carries no cadence,
    // so there is no schedule for the refusal to find incomplete.
    let filter = s
        .event_with(
            "rhythm:swap-the-air-filter",
            "swap the air filter when it looks grey",
            json!({ "last_check_in": "2026-08-01" }),
            &[],
        )
        .await;
    s.recall("rhythm:swap-the-air-filter")
        .await
        .says("\"last_check_in\"");

    // ── shaping a loop is held to the same line ─────────────────────────────
    //
    // Taking `advances_from` off the whole loop would leave a cadence with
    // nothing to count it from.
    s.refused(
        "update_fact",
        json!({ "address": descale, "clear_fields": ["advances_from"] }),
    )
    .await
    .says("advances_from")
    .says("\"wrote\":false");
    s.recall("rhythm:descale")
        .await
        .says("\"advances_from\":\"due_date\"");

    // Giving the schedule-less loop a cadence alone is the same mistake.
    s.refused(
        "update_fact",
        json!({ "address": filter, "fields": { "cadence_days": "90" } }),
    )
    .await
    .says("advances_from")
    .says("\"wrote\":false");
    // Both halves together are a schedule.
    s.correct_fields(
        &filter,
        json!({ "cadence_days": "90", "advances_from": "check_in_date" }),
        &[],
    )
    .await;
    s.recall("rhythm:swap-the-air-filter")
        .await
        .says("\"advances_from\":\"check_in_date\"");

    // ── the other half of a schedule is held to the same line ───────────────
    //
    // `advances_from` says which date a cycle counts from, and with no
    // cadence there is no cycle to count.
    s.add_under("thing:kettle", "rhythm:half-made", "Half made")
        .await;
    s.refused(
        "capture",
        json!({
            "subject": "rhythm:half-made", "content": "count from the due date",
            "provenance": "testimony",
            "fields": { "advances_from": "due_date" },
        }),
    )
    .await
    .says("cadence_days")
    .says("\"wrote\":false");
    s.recall("rhythm:half-made")
        .await
        .never_says("\"advances_from\"");
    // Taking the cadence off a whole loop leaves the same shape, and is
    // refused the same way.
    s.refused(
        "update_fact",
        json!({ "address": descale, "clear_fields": ["cadence_days"] }),
    )
    .await
    .says("cadence_days")
    .says("\"wrote\":false");
    s.recall("rhythm:descale")
        .await
        .says("\"cadence_days\":\"30\"");
    // Switching a loop's schedule off is both halves in one call, and lands.
    s.correct_fields(&descale, json!({}), &["cadence_days", "advances_from"])
        .await;
    s.recall("rhythm:descale")
        .await
        .never_says("\"cadence_days\"");

    s.wrap("a loop with a cadence and no schedule is refused when it is made")
        .await;
    story.finish().await;
}

/// "I did the descale today, and it comes round every sixty days."
///
/// **A whole schedule and its first check-in are one act.** A model sent one
/// capture on a new loop carrying the check-in AND the cadence and the policy,
/// and was refused for lacking the cadence it had just sent: the check-in read
/// the stored loop before that write's own fields landed. The arithmetic runs on
/// the schedule the call carries, so the capture lands once and the loop is
/// opened on the day of the check-in.
///
/// **Three claims in one run, so each is the positive another rests on.** The
/// whole schedule plus the check-in lands and falls due on the day the cadence
/// works out to; a loop that already held its schedule still takes a bare
/// check-in; and a check-in carrying only half a schedule is still refused,
/// because the half it lacks is not there to read.
#[tokio::test]
async fn a_whole_schedule_and_its_first_check_in_land_in_one_capture() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "The Kettle").await;
    s.add_under("thing:kettle", "rhythm:descale", "Descale")
        .await;
    s.add_under("thing:kettle", "rhythm:polish", "Polish").await;
    s.add_under("thing:kettle", "rhythm:deep-clean", "Deep clean")
        .await;

    // ── one capture: the schedule and the day it was first done ─────────────
    s.call(
        "capture",
        json!({
            "subject": "rhythm:descale", "content": "descaled the kettle",
            "provenance": "testimony", "recorded_at": "2026-08-01", "check_in": "ran",
            "fields": {"advances_from": "due_date", "cadence_days": "60"},
        }),
    )
    .await;
    // Sixty days after the first of August is the thirtieth of September, so
    // the loop is owed on the first of October and not on the twenty-ninth of
    // September. Both sides, because a loop that fell due on any other day
    // would answer one of them wrongly.
    let owed = |as_of: &str| json!({"kind": "rhythm", "overdue": {"as_of": as_of}});
    s.shape("what is owed on the first of October", owed("2026-10-01"))
        .await
        .says("rhythm:descale");
    s.shape(
        "what is owed on the twenty-ninth of September",
        owed("2026-09-29"),
    )
    .await
    .never_says("rhythm:descale");

    // ── the positive: a loop that already held its schedule still works ─────
    s.event_with(
        "rhythm:polish",
        "polish it every week",
        json!({"cadence_days": "7", "advances_from": "due_date", "counts_from": "2026-08-01"}),
        &[],
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "rhythm:polish", "content": "polished it",
            "provenance": "testimony", "recorded_at": "2026-08-08", "check_in": "ran",
        }),
    )
    .await;

    // ── a key the call sends wins over the one stored ───────────────────────
    //
    // The same loop is checked in again on the day it fell due and the cadence
    // is changed to fourteen days in the same capture. The arithmetic runs on
    // the new cadence: the cycle fell due fourteen days after the eighth, the
    // twenty-second, and the next one is due fourteen days after that, the
    // fifth of September. On the stored seven the cycle would count from the
    // fifteenth and the loop would be due on the twenty-ninth of August, so it
    // would be owed on the second of September: not being owed then is the new
    // cadence read.
    s.call(
        "capture",
        json!({
            "subject": "rhythm:polish", "content": "polished it, and it can wait longer now",
            "provenance": "testimony", "recorded_at": "2026-08-15", "check_in": "ran",
            "fields": {"cadence_days": "14"},
        }),
    )
    .await;
    s.shape(
        "what is owed on the second of September",
        owed("2026-09-02"),
    )
    .await
    .never_says("rhythm:polish");
    s.shape("what is owed on the sixth of September", owed("2026-09-06"))
        .await
        .says("rhythm:polish");

    // ── the negative: half a schedule with the check-in is still refused ────
    s.refused(
        "capture",
        json!({
            "subject": "rhythm:deep-clean", "content": "deep cleaned it",
            "provenance": "testimony", "recorded_at": "2026-08-01", "check_in": "ran",
            "fields": {"advances_from": "due_date"},
        }),
    )
    .await
    .says("cadence_days");

    s.wrap("one capture carried a whole schedule and its first check-in")
        .await;
    story.finish().await;
}
