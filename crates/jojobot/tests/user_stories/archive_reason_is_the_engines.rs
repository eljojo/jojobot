//! "A claim of mine is still standing. Why does it read as archived, with a
//! reason I never gave?"
//!
//! **Only an archive writes a claim's `archive_reason`.** The reason rides on
//! the archived record's own fields, so a caller who sends the key beside a
//! claim that stands would make an active claim read as archived with a reason,
//! and a caller who clears it would take a real reason off. Copies and clears
//! of the key are dropped; an edit with nothing left still needs `keep`. The
//! way a reason is given, the
//! details of the archive, still records it and reads it back.
//!
//! The roster names are the cast: a thread of deploys, Marge's word, and two
//! bots.

use serde_json::json;

use super::dsl::Story;

/// **A caller cannot overwrite or clear `archive_reason`, and an archive's details
/// still record one.** Paired in one case: the empty-edit refusal on a claim that stands,
/// the refusal of a clear on an archived claim whose reason must survive, and the
/// archive with a reason that reads back.
#[tokio::test]
async fn a_caller_cannot_send_or_clear_archive_reason_and_an_archive_still_records_one() {
    let story = Story::begin("bot:otto").await;
    let writer = story.session().await;
    let other = story.as_bot("bot:assistant").await;
    writer.add("thread:deploys-alpha", "Deploys Alpha").await;
    let said = |content: &'static str| {
        let writer = &writer;
        async move {
            writer
                .call(
                    "capture",
                    json!({
                        "subject": "thread:deploys-alpha", "content": content,
                        "provenance": "testimony",
                    }),
                )
                .await
                .json()["address"]
                .as_str()
                .expect("an address")
                .to_string()
        }
    };
    let standing = said("Marge said the picnic is on").await;
    let archived = said("Marge said the picnic moved").await;
    writer
        .call(
            "update_fact",
            json!({"address": &archived, "status": "archived",
                   "details": "the picnic is back on"}),
        )
        .await;

    // ── another session cannot make a claim that stands read as archived ─────
    other
        .refused(
            "update_fact",
            json!({"address": &standing, "fields": {"archive_reason": "not true"}}),
        )
        .await
        .says("keep");
    // ── nor take a real reason off an archived one ───────────────────────────
    other
        .refused(
            "update_fact",
            json!({"address": &archived, "clear_fields": ["archive_reason"]}),
        )
        .await
        .says("keep");

    // ── nothing moved: the claim stands, the reason is there ─────────────────
    let read = writer
        .call(
            "recall",
            json!({"subject": "thread:deploys-alpha", "facts": true, "details": true}),
        )
        .await
        .json();
    let facts = read["objects"][0]["facts"].as_array().expect("facts");
    let find = |address: &str| {
        facts
            .iter()
            .find(|fact| fact["address"] == address)
            .unwrap_or_else(|| panic!("{address} in {read}"))
    };
    assert_eq!(find(&standing)["status"], "active", "{read}");
    assert!(
        find(&standing)["fields"].get("archive_reason").is_none(),
        "{read}"
    );
    assert_eq!(
        find(&archived)["fields"]["archive_reason"],
        "the picnic is back on",
        "{read}"
    );
    story.finish().await;
}
