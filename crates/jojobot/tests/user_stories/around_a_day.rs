//! "What did Milhouse say back in August?"
//!
//! A real question arrives as a time and a subject together. Things near in
//! time cue one another, and the store has carried the day of every claim since
//! the beginning — with no way to be asked about it.
//!
//! `// GAP —` marks what a beat needed and could not have.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn what_was_recorded_around_a_day() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;
    s.add("org:north-trail-club", "North Trail Club").await;

    // A year of claims about one person, spread across it. Read back by
    // subject they are one undifferentiated list; what makes any of them
    // findable is when they happened.
    s.fact_on("person:milhouse", "took up cycling", "2026-02-11")
        .await;
    s.fact_on(
        "person:milhouse",
        "said the club needs a new pump",
        "2026-08-16",
    )
    .await;
    s.fact_on(
        "person:milhouse",
        "is standing for the committee",
        "2026-08-19",
    )
    .await;
    s.fact_on("person:milhouse", "moved to Shelbyville", "2026-12-02")
        .await;

    // ── the question, asked the way it arrives ──────────────────────────────

    // Two claims sit within a week of the 18th and two do not. Nothing about
    // the question names them — the day is the whole of what the caller knows.
    let august = s
        .call(
            "recall",
            json!({
                "subject": "person:milhouse",
                "facts": true,
                "near": {"day": "2026-08-18", "within_days": 7},
            }),
        )
        .await;
    august.says("new pump");
    august.says("standing for the committee");
    august.never_says("took up cycling");
    august.never_says("moved to Shelbyville");

    // **The answer says which day and which clock it read**, because a caller
    // that let jojobot supply today has no other way to learn either, and an
    // answer about an unnamed day is one nobody can check.
    august.says("\"near_day\":\"2026-08-18\"");
    august.says("\"near_clock\":\"true_of\"");

    // **Nothing was unreadable.** Zero is a claim of its own here: every record
    // was placed, so the two that did not come back are absent because they
    // were far from the day rather than because nothing could be looked at.
    august.says("\"near_unplaced\":0");

    // ── the other clock, and the honest empty ───────────────────────────────

    // These records were captured just now, so on the day they are true OF
    // they sit across a whole year — but every one of them was taken in today.
    // The two clocks disagree, which is the point rather than an edge case.
    let filed = s
        .call(
            "recall",
            json!({
                "subject": "person:milhouse",
                "facts": true,
                "near": {"day": "2026-08-18", "within_days": 7, "clock": "taken_in"},
            }),
        )
        .await;
    filed.says("\"near_clock\":\"taken_in\"");

    // **A clock nobody serves is refused, and the refusal names the two that
    // exist.** Guessing at it would answer a question the caller did not ask,
    // on a clock they did not choose.
    s.refused(
        "recall",
        json!({
            "subject": "person:milhouse",
            "near": {"day": "2026-08-18", "clock": "whenever"},
        }),
    )
    .await
    .says("true_of");

    // GAP — the question arrives as "back in August", not as a day and a
    // window. Turning a month into a centre and a radius is the caller's
    // arithmetic, and every caller will do it differently.
    //   s.call("recall", json!({"near": {"month": "2026-08"}})).await;

    // GAP — nothing asks the question the other way round: given this claim,
    // what else was recorded near it. The day is on the record already, so a
    // caller has to read it, then ask a second call about it.
    //   s.call("recall", json!({"near": {"like": "person:milhouse#f2"}})).await;

    s.wrap("asked what surrounded a day, and got the week rather than the year")
        .await;
}
