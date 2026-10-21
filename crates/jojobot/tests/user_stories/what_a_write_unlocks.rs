//! "I wrote down a cadence for the filter change. What can it tell me now?"
//!
//! A half-built thing answers less than its writer thinks, and the writer
//! learns that only by asking and getting less back. The receipt of a write
//! that landed now says what the thing could answer with a little more: a
//! loop with a cadence and no check-in says what a check-in would let it
//! say, and a thing that holds part of a declared type names the type and the
//! keys it lacks, once.
//!
//! **Silence is the other half.** A loop written whole, and a thing that
//! answers no type, get no block at all, so the block is worth reading when it
//! is there. The story reads the receipts and nothing else.

use serde_json::json;

use super::dsl::Story;

/// **A thing that is not a rhythm is not offered a check-in.** A check-in is
/// taken only on a rhythm, so a step that names it on any other thing teaches a
/// route the engine refuses. **Paired**: the same schedule on a rhythm is
/// offered it, and the refusal on the thing is real, so the two agree.
#[tokio::test]
async fn a_check_in_is_offered_only_where_one_can_be_taken() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-air-filter", "The air filter").await;
    s.add_under(
        "thing:the-air-filter",
        "rhythm:swap-the-air-filter",
        "Filter change",
    )
    .await;
    let schedule = json!({"cadence_days": "90", "advances_from": "check_in_date"});

    // On the thing holding a loop's keys: no step names a check-in.
    let on_thing = s
        .call(
            "capture",
            json!({
                "subject": "thing:the-air-filter",
                "content": "change the filter every quarter",
                "provenance": "testimony",
                "sets": schedule,
            }),
        )
        .await
        .json();
    assert!(
        !on_thing["unlocks"].to_string().contains("check_in"),
        "a thing cannot take a check-in, so none is offered: {on_thing}"
    );
    // …and the engine does refuse one there.
    s.refused(
        "capture",
        json!({
            "subject": "thing:the-air-filter", "content": "changed it",
            "provenance": "testimony", "check_in": "ran",
        }),
    )
    .await;

    // The pair: on a rhythm the same schedule is offered it.
    let on_rhythm = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter",
                "content": "change the filter every quarter",
                "provenance": "testimony",
                "sets": {"name": "Swap the air filter", "cadence_days": "90",
                         "advances_from": "check_in_date"},
            }),
        )
        .await
        .json();
    assert!(
        on_rhythm["unlocks"]["steps"]
            .as_array()
            .is_some_and(|steps| steps.iter().any(|step| step["add"] == json!(["check_in"]))),
        "{on_rhythm}"
    );
    story.finish().await;
}

/// **A schedule is offered only where a check-in can follow.** Adding a cadence
/// to a thing that is not a rhythm writes a schedule that no check-in can ever
/// advance, so the receipt of a half-built schedule on a work item or a thing
/// names no step that adds one. **Paired in the same case**: the same write on a
/// rhythm is offered both the schedule and the check-in, so an empty answer
/// cannot pass for the right one.
#[tokio::test]
async fn a_schedule_is_offered_only_where_a_check_in_can_follow() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-air-filter", "The air filter").await;
    s.add("work:phi", "Phi").await;
    s.add_under(
        "thing:the-air-filter",
        "rhythm:swap-the-air-filter",
        "Filter change",
    )
    .await;
    let policy = json!({"advances_from": "check_in_date"});

    // A work item and a thing holding part of a schedule: no schedule step.
    for subject in ["work:phi", "thing:the-air-filter"] {
        let receipt = s
            .call(
                "capture",
                json!({
                    "subject": subject, "content": "half a schedule",
                    "provenance": "testimony", "sets": policy,
                }),
            )
            .await
            .json();
        let said = receipt["unlocks"].to_string();
        assert!(
            !said.contains("cadence_days") && !said.contains("rrule"),
            "{subject} cannot be advanced by a check-in, so no schedule is offered: {receipt}"
        );
    }
    // The same through update_fact on a record already there.
    let plain = s
        .call(
            "capture",
            json!({
                "subject": "thing:the-air-filter", "content": "a plain note",
                "provenance": "testimony",
            }),
        )
        .await
        .field("address");
    let edited = s
        .call(
            "update_fact",
            json!({"address": plain, "sets": {"advances_from": "check_in_date"}}),
        )
        .await
        .json();
    assert!(
        !edited["unlocks"].to_string().contains("cadence_days"),
        "{edited}"
    );

    // The pair, in the same case: on a rhythm the schedule and the check-in are both offered.
    let on_rhythm = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter", "content": "half a schedule",
                "provenance": "testimony",
                "sets": {"name": "Swap the air filter", "counts_from": "2026-08-01"},
            }),
        )
        .await
        .json();
    let steps: Vec<_> = on_rhythm["unlocks"]["steps"]
        .as_array()
        .unwrap_or_else(|| {
            panic!("a half-built loop on a rhythm says what it could answer: {on_rhythm}")
        })
        .iter()
        .map(|step| step["add"].clone())
        .collect();
    assert!(
        steps.contains(&json!(["cadence_days", "rrule"])),
        "the schedule is offered on a rhythm: {on_rhythm}"
    );
    assert!(
        steps.contains(&json!(["check_in"])),
        "and so is the check-in: {on_rhythm}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_write_says_what_the_thing_could_answer_with_one_more_key() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("thing:the-air-filter", "The air filter").await;
    s.add_under(
        "thing:the-air-filter",
        "rhythm:swap-the-air-filter",
        "Filter change",
    )
    .await;

    // ── ① a cadence and a policy, and no check-in yet ───────────────────────
    let partial = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter",
                "content": "change the filter every quarter",
                "provenance": "testimony",
                "sets": {
                    "name": "Swap the air filter",
                    "cadence_days": "90",
                    "advances_from": "check_in_date",
                },
            }),
        )
        .await
        .json();
    let steps = partial["unlocks"]["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("a half-built loop says what it could answer: {partial}"));
    assert!(
        steps.iter().any(|step| step["add"] == json!(["check_in"])),
        "the step names the route that opens the loop: {partial}"
    );
    // A key the server computes is never offered: writing it by hand opens no loop.
    assert!(
        !partial["unlocks"].to_string().contains("last_check_in"),
        "no step names a computed key: {partial}"
    );
    assert_eq!(partial["unlocks"]["holds"]["keys"], 3, "{partial}");

    // ── ② the check-in itself, which closes the gap ─────────────────────────
    let checked = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:swap-the-air-filter",
                "content": "changed it this morning",
                "provenance": "testimony",
                "check_in": "ran",
                "recorded_at": "2026-10-01",
            }),
        )
        .await
        .json();
    assert!(
        checked.get("unlocks").is_none(),
        "a loop that can say when it is next due has nothing left to unlock: {checked}"
    );

    // ── ③ a thing that answers no type is left alone ────────────────────────
    let filter = s
        .call(
            "capture",
            json!({
                "subject": "thing:the-air-filter",
                "content": "takes a 20 by 25 filter",
                "provenance": "testimony",
                "sets": {"filter_size": "20x25"},
            }),
        )
        .await
        .json();
    assert!(
        filter.get("unlocks").is_none(),
        "a key no type or schedule mentions earns no line: {filter}"
    );

    // ── ④ part of a declared type is named once ─────────────────────────────
    s.call(
        "declare_type",
        json!({
            "name": "service-visit",
            "fields": [
                {"key": "visited_on", "holds": "date"},
                {"key": "technician", "holds": "text"},
            ],
        }),
    )
    .await;
    let first = s
        .call(
            "capture",
            json!({
                "subject": "thing:the-air-filter",
                "content": "serviced last week",
                "provenance": "testimony",
                "sets": {"visited_on": "2026-09-24"},
            }),
        )
        .await
        .json();
    let typed: Vec<_> = first["unlocks"]["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("a partly held type is named: {first}"))
        .iter()
        .filter(|step| step["type"] == "service-visit")
        .collect();
    assert_eq!(typed.len(), 1, "{first}");
    assert_eq!(typed[0]["add"], json!(["technician"]), "{first}");

    let again = s
        .call(
            "capture",
            json!({
                "subject": "thing:the-air-filter",
                "content": "serviced again",
                "provenance": "testimony",
                "sets": {"visited_on": "2026-09-25"},
            }),
        )
        .await
        .json();
    assert!(
        again.get("unlocks").is_none(),
        "the same thing is not told twice in one session: {again}"
    );

    story.finish().await;
}
