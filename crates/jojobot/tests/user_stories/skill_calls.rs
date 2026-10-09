//! "I made the call the skill printed, word for word."
//!
//! A worked call in a skill is a contract: a session copies it, and it has to
//! do what the sentence around it says. Each story below sends the call as the
//! skill prints it, through the served surface as a real client does, and then
//! asks what the call was supposed to leave behind.

use serde_json::json;

use super::dsl::Story;

/// **The asking skill's calls, as printed.** A view with one thing added
/// changes the answer; the people in a place are a walk inward from the place,
/// and a kind plus a `follow` is not that question; a visit costing more than
/// fifty is a record-scoped comparison on a key declared a number; and `keys`
/// narrows what each object brings back.
#[tokio::test]
async fn the_asking_skills_calls_do_what_the_skill_says() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // ── a view with one thing added ─────────────────────────────────────────
    s.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Holds the plan and does not implement." }),
        )
        .await;
    s.call("recall", json!({ "view": "colleagues" }))
        .await
        .never_says("Holds the plan");
    // **The pair**: the added argument is what brings the charter.
    s.call("recall", json!({ "view": "colleagues", "charter": true }))
        .await
        .says("Holds the plan");

    // ── the people in a place ───────────────────────────────────────────────
    s.add("place:springfield", "Springfield").await;
    s.add("place:shelbyville", "Shelbyville").await;
    s.add("person:homer", "Homer").await;
    s.add("person:bart", "Bart").await;
    for (who, place) in [
        ("person:homer", "place:springfield"),
        ("person:bart", "place:shelbyville"),
    ] {
        s.call(
            "capture",
            json!({
                "subject": who, "content": "lives there", "provenance": "testimony",
                "shape": "location", "object": place,
            }),
        )
        .await;
    }
    let walked = s
        .call(
            "recall",
            json!({
                "subject": "place:springfield",
                "follow": { "shape": "location", "direction": "in" },
            }),
        )
        .await;
    walked.says("person:homer").never_says("person:bart");
    // **And the form the skill used to print is not that question**: a kind
    // plus a follow lists everybody of the kind.
    let everyone = s
        .call(
            "recall",
            json!({
                "kind": "person",
                "follow": { "shape": "location", "direction": "in" },
            }),
        )
        .await;
    everyone.says("person:homer").says("person:bart");

    // ── a key compared on the record, and what comes back of each ───────────
    s.call(
        "declare_type",
        json!({ "name": "visit", "fields": [{ "key": "cost", "holds": "number" }] }),
    )
    .await;
    s.add("thing:the-couch", "The Couch").await;
    s.add("thing:floor-pump", "The Floor Pump").await;
    s.event_with(
        "thing:the-couch",
        "a dear visit",
        json!({ "cost": "80", "colour": "green" }),
        &[],
    )
    .await;
    s.event_with(
        "thing:floor-pump",
        "a cheap visit",
        json!({ "cost": "20" }),
        &[],
    )
    .await;
    let dear = s
        .call(
            "recall",
            json!({
                "fields": [{
                    "key": "cost", "compare": "greater", "value": "50", "scope": "record",
                }],
            }),
        )
        .await;
    dear.says("thing:the-couch").never_says("thing:floor-pump");
    let narrowed = s
        .call(
            "recall",
            json!({ "subject": "thing:the-couch", "keys": ["cost"] }),
        )
        .await;
    narrowed
        .says("\"cost\":\"80\"")
        .never_says("\"colour\":\"green\"");

    story.finish().await;
}

/// **The rhythms skill's calls, as printed.** A loop is written with its keys in
/// `sets`; a check-in on a loop that already ran is dated with `recorded_at`; a
/// promise cannot lose `promised_by` and is ended instead; a day on a thing
/// that carries `runs_out` is taken away by clearing that key; and a loop the
/// operator finds stressful is archived.
#[tokio::test]
async fn the_rhythms_skills_calls_do_what_the_skill_says() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;

    // ── a loop, its keys in `sets`, and a check-in dated the day it ran ─────
    s.call(
        "add_entity",
        json!({
            "kind": "rhythm", "handle": "chain-check", "name": "Chain check",
            "source": "user-named", "parent": "thing:gravel-bike",
            "sets": { "cadence_days": "60", "advances_from": "due_date" },
        }),
    )
    .await;
    s.recall("rhythm:chain-check")
        .await
        .says("\"cadence_days\":\"60\"")
        .says("\"advances_from\":\"due_date\"");
    s.call(
        "capture",
        json!({
            "subject": "rhythm:chain-check", "content": "looked at the chain",
            "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-09-20",
        }),
    )
    .await;
    s.recall("rhythm:chain-check")
        .await
        .says("\"last_check_in\":\"2026-09-20\"");

    // ── a promise is ended, not cleared ─────────────────────────────────────
    s.add("person:ned-flanders", "Ned").await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;
    let promised = s
        .call(
            "capture",
            json!({
                "subject": "promise:return-the-wrench", "content": "back by July",
                "provenance": "testimony", "fields": { "promised_by": "2026-07-01" },
            }),
        )
        .await;
    let owed = || json!({ "fields": [{ "key": "due_on" }], "overdue": { "as_of": "2026-08-05" } });
    s.shape("what is owed", owed())
        .await
        .says("promise:return-the-wrench");
    s.refused(
        "update_fact",
        json!({ "address": promised.field("address"), "clear_fields": ["promised_by"] }),
    )
    .await
    .says("promised_by");
    s.event_with(
        "promise:return-the-wrench",
        "gave it back",
        json!({ "ended": "delivered" }),
        &[],
    )
    .await;
    s.shape("what is owed after it ended", owed())
        .await
        .never_says("promise:return-the-wrench");

    // ── a day that is not a promise's is taken away by clearing its key ─────
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    let covered = s
        .call(
            "capture",
            json!({
                "subject": "thing:torque-wrench", "content": "the warranty runs out",
                "provenance": "testimony", "fields": { "runs_out": "2026-06-01" },
            }),
        )
        .await;
    s.shape("what is owed with the warranty", owed())
        .await
        .says("thing:torque-wrench");
    s.call(
        "update_fact",
        json!({ "address": covered.field("address"), "clear_fields": ["runs_out"] }),
    )
    .await;
    s.shape("what is owed without the warranty", owed())
        .await
        .never_says("thing:torque-wrench");

    // ── a loop the operator finds stressful is archived ─────────────────────
    s.call(
        "archive_entity",
        json!({ "handle": "rhythm:chain-check", "reason": "the operator finds it stressful" }),
    )
    .await;
    s.list("rhythm").await.never_says("rhythm:chain-check");

    story.finish().await;
}

/// **The projects skill's call, as printed**: a project's `columns` list holds
/// the five shipped words and lands, and a list that drops one is refused,
/// naming the word it lacks.
#[tokio::test]
async fn the_projects_skills_columns_call_holds_every_shipped_word() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    s.call(
        "capture",
        json!({
            "subject": "project:atlas", "content": "the board's columns",
            "provenance": "testimony",
            "fields": { "columns": "inbox, someday, next, now, waiting, done" },
        }),
    )
    .await;
    s.recall("project:atlas")
        .await
        .says("inbox, someday, next, now, waiting, done");
    s.refused(
        "capture",
        json!({
            "subject": "project:atlas", "content": "a shorter list", "provenance": "testimony",
            "fields": { "columns": "inbox, someday, next, now, done" },
        }),
    )
    .await
    .says("waiting");
    story.finish().await;
}
