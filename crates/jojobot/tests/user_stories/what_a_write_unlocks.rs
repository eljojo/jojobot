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

#[tokio::test]
async fn a_write_says_what_the_thing_could_answer_with_one_more_key() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("thing:the-air-filter", "The Furnace").await;
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
                "sets": {"cadence_days": "90", "advances_from": "check_in_date"},
            }),
        )
        .await
        .json();
    let steps = partial["unlocks"]["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("a half-built loop says what it could answer: {partial}"));
    assert!(
        steps
            .iter()
            .any(|step| step["add"] == json!(["last_check_in"])),
        "the step names the check-in: {partial}"
    );
    assert_eq!(partial["unlocks"]["holds"]["keys"], 2, "{partial}");

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
