//! "Is there a spare pair of glasses? Put the answer on the glasses."
//!
//! **The answer already on record is last season's answer.** A session that
//! reads a claim from January and finds the spare there can take the question
//! as answered, say so in chat, and write nothing. The record then holds an
//! answer dated in January and none dated today, and a later reader cannot tell
//! that anyone was asked.
//!
//! The read that returns a claim from an earlier day than the session's own
//! carries one more teaching than the same read of a claim from the session's
//! day, and the session that follows it writes today's answer on the thing.

use serde_json::json;

use super::dsl::Story;

const TODAY: &str = "2026-11-08";

/// **The teachings the first read of the glasses carried**, for a claim dated
/// `claim_day`, read by a session standing in [`TODAY`].
async fn first_read_teachings(claim_day: &str) -> usize {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("thing:glasses", "The glasses").await;
    setup
        .fact_on(
            "thing:glasses",
            "a spare pair is in the hall drawer",
            claim_day,
        )
        .await;
    setup.wrap("the glasses written up").await;
    let reader = story.session_on(TODAY, Some("new")).await;
    reader.recall("thing:glasses").await.teachings().len()
}

#[tokio::test]
async fn a_read_of_last_seasons_answer_teaches_one_more_than_a_read_of_todays() {
    let from_january = first_read_teachings("2026-01-05").await;
    let from_today = first_read_teachings(TODAY).await;
    assert_eq!(
        from_january,
        from_today + 1,
        "the January claim carries the one extra teaching, the same-day claim does not"
    );
}

/// **The teachings the first default read of the glasses carried.** The answer
/// is the thing's fields and holds no claim; the fields were written when the
/// story ran, so a reader on a day after it stands later than the write and a
/// reader on a day before it stands earlier.
async fn first_field_read_teachings(reader_day: &str) -> usize {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("thing:glasses", "The glasses").await;
    setup
        .call(
            "capture",
            json!({
                "subject": "thing:glasses",
                "content": "a spare pair is in the hall drawer",
                "provenance": "testimony",
                "sets": {"spare": "hall drawer"},
            }),
        )
        .await;
    setup.wrap("the glasses written up").await;
    let reader = story.session_on(reader_day, Some("new")).await;
    let answer = reader
        .call("recall", json!({"subject": "thing:glasses"}))
        .await;
    answer
        .says("\"spare\":\"hall drawer\"")
        .never_says("\"facts\"");
    answer.teachings().len()
}

#[tokio::test]
async fn a_default_read_of_an_earlier_days_fields_teaches_one_more_than_a_read_of_a_later_days() {
    let after_the_write = first_field_read_teachings("2099-01-01").await;
    let before_the_write = first_field_read_teachings("2020-01-01").await;
    assert_eq!(
        after_the_write,
        before_the_write + 1,
        "fields older than the session's day carry the extra teaching, newer ones do not"
    );
}

#[tokio::test]
async fn an_answer_asked_today_is_written_on_the_thing_and_read_back_dated_today() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("thing:glasses", "The glasses").await;
    setup
        .fact_on(
            "thing:glasses",
            "a spare pair is in the hall drawer",
            "2026-01-05",
        )
        .await;
    setup.wrap("the glasses written up").await;

    // ── the session reads the thing and finds January's answer ──────────────
    let today = story.session_on(TODAY, Some("new")).await;
    let before = today.recall("thing:glasses").await;
    before
        .says("\"recorded_at\":\"2026-01-05\"")
        .never_says(&format!("\"recorded_at\":\"{TODAY}\""));
    assert!(
        !before.teachings().is_empty(),
        "the first read of a claim carries its teachings"
    );

    // ── the operator asked for today's answer on the glasses: it writes it ──
    today
        .call(
            "capture",
            json!({
                "subject": "thing:glasses",
                "content": "a spare pair is in the hall drawer",
                "provenance": "testimony",
                "sets": {"spare": "hall drawer"},
            }),
        )
        .await;

    // ── and a later read returns today's answer. January's claim was served ─
    // ── already, so it comes back as its address alone. ──────────────────────
    let after = today.recall("thing:glasses").await;
    after
        .says(&format!("\"recorded_at\":\"{TODAY}\""))
        .says("thing:glasses#f1");
    assert!(
        after.teachings().is_empty(),
        "nothing is taught twice in one session: {:?}",
        after.teachings()
    );
}
