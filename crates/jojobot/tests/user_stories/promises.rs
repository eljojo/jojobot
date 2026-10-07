//! "The wrench has to go back to the neighbour by the first of July."
//!
//! A commitment about a thing is its own object, not a field on the thing: it
//! carries its own events, two commitments can exist about one thing, and the
//! thing's record stays about the thing. A promise sits under whoever it is
//! that person's job to keep, falls due on a date it carries, and arrives in
//! the same owed-and-late read as a loop gone quiet. **Being shown in that
//! read ends nothing**: a promise ends when somebody records how.

use serde_json::json;

use super::dsl::Story;

/// **The owed read, as every carrier is reached by it**: one selection over the
/// stored due moment that each carrier keeps, and the day it is asked about.
fn owed_on(day: &str) -> serde_json::Value {
    json!({ "fields": [{"key": "due_on"}], "overdue": { "as_of": day } })
}

#[tokio::test]
async fn a_borrowed_thing_that_has_to_go_back_is_owed_beside_the_loops() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the people and things it is about ───────────────────────────────────
    s.add("person:ned-flanders", "Ned").await;
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    s.add("thing:standing-desk", "The Standing Desk").await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;

    // ── a promise with nobody whose job it is is refused ────────────────────
    s.refused(
        "add_entity",
        json!({
            "kind": "promise", "handle": "return-the-wrench", "name": "Return the wrench",
            "source": "user-named",
        }),
    )
    .await
    .says("parent")
    .says("\"wrote\":false");
    s.list("promise")
        .await
        .never_says("promise:return-the-wrench");

    // **The same promise with a parent lands**, so the refusal above is about
    // the parent and not about the kind being unwritable.
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-desk",
        "Return the desk",
    )
    .await;
    s.list("promise").await.says("promise:return-the-wrench");

    // ── the dates, and what each is about ───────────────────────────────────
    s.event_with(
        "promise:return-the-wrench",
        "borrowed it, has to go back to the neighbour",
        json!({"promised_by": "2026-07-01", "regarding": "thing:torque-wrench"}),
        &[],
    )
    .await;
    s.event_with(
        "promise:return-the-desk",
        "borrowed it for the summer, back by September",
        json!({"promised_by": "2026-09-01", "regarding": "thing:standing-desk"}),
        &[],
    )
    .await;

    // A loop in the same store, gone quiet on its own cadence.
    s.add_under("thing:gravel-bike", "rhythm:chain-check", "Chain check")
        .await;
    s.event_with(
        "rhythm:chain-check",
        "look at the chain every couple of months",
        json!({
            "cadence_days": "60",
            "advances_from": "due_date",
            "counts_from": "2026-06-01",
        }),
        &[],
    )
    .await;

    // ── one question, one answer ────────────────────────────────────────────
    //
    // The fifth of August: the wrench was due on the first of July and the
    // chain on the thirty-first; the desk is not due until September.
    let owed = s
        .shape("what is owed by the fifth of August", owed_on("2026-08-05"))
        .await;
    owed.says("promise:return-the-wrench");
    owed.says("rhythm:chain-check");
    // **The negative that makes the answer mean anything.**
    owed.never_says("promise:return-the-desk");
    // Each item says what it is, so a reader tells a promise from a loop.
    owed.says("\"type\":\"Promise\"");

    // ── asking ends nothing ─────────────────────────────────────────────────
    s.shape("the same question again", owed_on("2026-08-05"))
        .await
        .says("promise:return-the-wrench");

    // ── how it ends is recorded, and then it is no longer owed ──────────────
    s.event_with(
        "promise:return-the-wrench",
        "gave it back on the second",
        json!({"ended": "delivered"}),
        &[],
    )
    .await;
    let after = s.shape("what is owed now", owed_on("2026-08-05")).await;
    after.never_says("promise:return-the-wrench");
    // The loop it was answered beside is still owed, unchanged.
    after.says("rhythm:chain-check");

    // The desk comes due in September, and is dropped rather than delivered:
    // the three endings are told apart afterwards.
    s.event_with(
        "promise:return-the-desk",
        "the neighbour said keep it",
        json!({"ended": "withdrawn"}),
        &[],
    )
    .await;
    s.shape("what is owed in October", owed_on("2026-10-01"))
        .await
        .never_says("promise:return-the-desk");
    let delivered = s
        .shape(
            "which promises were delivered",
            json!({
                "kind": "promise",
                "fields": [{"key": "ended", "value": "delivered"}],
            }),
        )
        .await;
    delivered.says("promise:return-the-wrench");
    delivered.never_says("promise:return-the-desk");

    s.wrap("two promises made, one kept and one let go").await;
    story.finish().await;
}

/// "It is due the first of July — and nobody can tell jojobot otherwise."
///
/// The stored due moment is jojobot's own key. A promise the operator dated is
/// filed as the operator's word and falls due on that day in the cross-kind
/// owed question and in the per-kind one alike. A session that tries to write
/// the stored key, or to clear it, is refused with the keys that do set a day,
/// and the promise is left as it was, so the two reads cannot come to
/// disagree.
#[tokio::test]
async fn the_stored_due_moment_is_jojobots_and_a_promise_keeps_the_operators_word() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:ned-flanders", "Ned").await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;

    // ── the operator's own day is filed as the operator's word ───────────────
    let dated = s
        .call(
            "capture",
            json!({
                "subject": "promise:return-the-wrench",
                "content": "has to go back to the neighbour",
                "provenance": "testimony",
                "fields": {"promised_by": "2026-07-01"},
            }),
        )
        .await;
    dated.says("\"provenance\":\"testimony\"");
    let address = dated.field("address");

    // ── writing the stored key is refused, and says what sets a day ──────────
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": "promise:return-the-wrench",
                "content": "back by the sixteenth",
                "provenance": "testimony",
                "fields": {"promised_by": "2026-01-16", "due_on": "2026-01-16"},
            }),
        )
        .await;
    for key in ["promised_by", "runs_out", "decide_by"] {
        refused.says(key);
    }
    // The refusals name the key to write instead, which the call did not send.
    s.refused(
        "update_fact",
        json!({"address": address, "fields": {"due_on": "2026-01-16"}}),
    )
    .await
    .says("promised_by");
    // ── clearing it is refused too ───────────────────────────────────────────
    s.refused(
        "update_fact",
        json!({"address": address, "clear_fields": ["due_on"]}),
    )
    .await
    .says("promised_by");

    // ── the promise is as it was, and both reads say it is owed ──────────────
    let held = s.recall("promise:return-the-wrench").await;
    held.says("\"due_on\":\"2026-07-01\"");
    held.says("\"promised_by\":\"2026-07-01\"");
    held.never_says("2026-01-16");
    let by_key = s.shape("owed by key", owed_on("2026-07-05")).await;
    by_key.says("promise:return-the-wrench");
    let by_kind = s
        .shape(
            "owed by kind",
            json!({"kind": "promise", "overdue": {"as_of": "2026-07-05"}}),
        )
        .await;
    by_kind.says("promise:return-the-wrench");

    s.wrap("a promise dated and two hand-written copies refused")
        .await;
    story.finish().await;
}

/// **A refused hand-written due moment tells finished work how to stop falling
/// due.** The way out for a promise is to end it. A work item or a project has
/// no such word, and the advice to clear the key it came from would leave a
/// finished item looking owed, so the refusal there says to set the status to
/// done, which keeps its dates. The promise's refusal does not say it, so the
/// advice is told apart by kind.
#[tokio::test]
async fn the_refusal_for_a_hand_written_due_moment_tells_finished_work_to_be_marked_done() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add_under("project:atlas", "work:phi", "Phi").await;
    s.add("person:ned-flanders", "Ned").await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;

    let refusal = |subject: &str, fields: serde_json::Value| {
        json!({
            "subject": subject,
            "content": "back by the sixteenth",
            "provenance": "testimony",
            "fields": fields,
        })
    };
    let work = s
        .refused(
            "capture",
            refusal(
                "work:phi",
                json!({"decide_by": "2026-01-16", "due_on": "2026-01-16"}),
            ),
        )
        .await;
    work.says("done");

    let promise = s
        .refused(
            "capture",
            refusal(
                "promise:return-the-wrench",
                json!({"promised_by": "2026-01-16", "due_on": "2026-01-16"}),
            ),
        )
        .await;
    promise.says("promised_by").never_says("done");

    story.finish().await;
}

/// **The refusal for a hand-written due moment on a promise names how a promise
/// ends, and following it works.** A promise cannot clear its day, because its
/// kind requires one, so the advice to clear the key the moment came from would
/// loop. A promise stops falling due when `ended` is written. The refusal names
/// that key and its words, and the story follows it: the promise keeps its day,
/// is no longer owed, and the owed read says so.
#[tokio::test]
async fn a_promises_refusal_names_ended_and_ending_it_stops_it_falling_due() {
    use jojobot_domain::attention::PromiseEnd;
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:ned-flanders", "Ned").await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;
    let dated = s
        .call(
            "capture",
            json!({
                "subject": "promise:return-the-wrench",
                "content": "has to go back to the neighbour",
                "provenance": "testimony",
                "fields": {"promised_by": "2026-07-01"},
            }),
        )
        .await;
    let address = dated.json()["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let owed = |day: &str| owed_on(day);
    s.shape("owed before it ends", owed("2026-07-05"))
        .await
        .says("promise:return-the-wrench");

    // ── the refusal names the key and its words ─────────────────────────────
    let refused = s
        .refused(
            "update_fact",
            json!({"address": address, "fields": {"due_on": "2026-01-16"}}),
        )
        .await;
    refused.says("ended");
    for end in PromiseEnd::ALL {
        refused.says(end.as_token());
    }
    // The words are STORED as keys' values and nothing outside this process
    // declares them, so one of them is pinned as the literal it is.
    refused.says("delivered");

    // ── following it ends the promise, which keeps its day ──────────────────
    s.call(
        "update_fact",
        json!({"address": address, "fields": {"ended": PromiseEnd::Delivered.as_token()}}),
    )
    .await;
    let held = s.recall("promise:return-the-wrench").await;
    held.says("\"promised_by\":\"2026-07-01\"");
    s.shape("owed after it ends", owed("2026-07-05"))
        .await
        .never_says("promise:return-the-wrench");

    s.wrap("ended a promise the way the refusal said").await;
    story.finish().await;
}
