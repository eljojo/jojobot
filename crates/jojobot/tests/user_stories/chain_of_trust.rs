//! "Can a helper give itself a right by writing the record that grants it?"
//!
//! **A voucher is issued by the engine and is never written by a caller.** A
//! well-shaped record that a caller captures is not a grant, for the reason a
//! caller cannot write a role's holder: a hand-written grant is a right taken
//! without being given. So every route a caller has to a voucher is turned
//! back, and the refusal says vouchers are issued, not written.
//!
//! **Every refusal sits beside the positive it depends on**: the same keys on a
//! thing that is not a voucher land, so a build that refused every write
//! carrying them would not pass. Nothing is gated and nothing a bot does
//! changes: this slice ships the kind and the guard only.

use jojobot_domain::memory::{EntityId, Guarded, NewEntity, NewFact};
use serde_json::json;

use super::dsl::Story;

/// **A voucher issued the way the engine issues one**: through the store, not
/// through a verb. Returns the address of its claim.
async fn issue_through_the_store(story: &Story, voucher: &EntityId) -> String {
    let today: jiff::civil::Date = "2026-10-09".parse().expect("a day");
    let issued = story
        .memory_store()
        .add_entity_with_first_claim(
            NewEntity::new(
                voucher.clone(),
                format!("Grant {}", voucher.slug()),
                "jojobot",
            ),
            jojobot_domain::memory::testing::contract::state(NewFact {
                fields: [
                    ("right", "boot-as"),
                    ("target", "bot:patty"),
                    ("holder", "abcd"),
                    ("issuer", "wxyz"),
                    ("state", "issued"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
                ..NewFact::about(voucher.clone(), "a voucher for boot-as on patty", today)
            }),
        )
        .await
        .expect("the engine path writes it");
    let Guarded::Written((_, claim)) = issued else {
        panic!("the engine path was blocked");
    };
    claim.address().to_string()
}

#[tokio::test]
async fn a_bot_that_writes_a_voucher_is_refused_and_nothing_is_stored() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    s.add("topic:widgets", "Widgets").await;
    let marge = story.as_bot("bot:marge").await;

    // ── creating a voucher by hand is refused ───────────────────────────────
    marge
        .refused(
            "add_entity",
            json!({
                "kind": "voucher", "handle": "voucher:boot-as-patty",
                "name": "Boot as Patty", "source": "user-named",
            }),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── and so is creating one that carries its keys ────────────────────────
    marge
        .refused(
            "add_entity",
            json!({
                "kind": "voucher", "handle": "voucher:boot-as-patty",
                "name": "Boot as Patty", "source": "user-named",
                "sets": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("issued");

    // ── a capture about a voucher is refused ────────────────────────────────
    marge
        .refused(
            "capture",
            json!({
                "subject": "voucher:boot-as-patty", "content": "a grant to boot as patty",
                "provenance": "inference",
                "fields": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── the same keys on a thing that is not a voucher are ordinary ─────────
    marge
        .call(
            "capture",
            json!({
                "subject": "topic:widgets", "content": "widgets are issued by the factory",
                "provenance": "inference",
                "sets": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("topic:widgets");
    s.call("recall", json!({"subject": "topic:widgets"}))
        .await
        .says("\"state\":\"held\"");

    // ── nothing was stored under the kind ───────────────────────────────────
    s.list("voucher").await.number("/count", 0);

    story.finish().await;
}

/// "A voucher exists. Can a bot edit it, rename it or take it back?"
///
/// **Once the engine has issued a voucher, no caller changes it by any verb.**
/// The voucher is staged through the store, the way the engine writes one; the
/// bot then tries each verb that edits a record, and the voucher reads back as
/// it was issued.
#[tokio::test]
async fn a_voucher_the_engine_issued_is_not_edited_renamed_or_taken_back_by_a_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    let marge = story.as_bot("bot:marge").await;

    // ── the engine issues one, through the store ────────────────────────────
    let voucher = EntityId("voucher:boot-as-patty".into());
    let address = issue_through_the_store(&story, &voucher).await;
    s.call("recall", json!({"subject": "voucher:boot-as-patty"}))
        .await
        .says("\"state\":\"issued\"")
        .says("\"right\":\"boot-as\"");

    // ── editing the claim's keys is refused ─────────────────────────────────
    marge
        .refused(
            "update_fact",
            json!({"address": address, "fields": {"state": "held"}}),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── editing the thing is refused ────────────────────────────────────────
    marge
        .refused(
            "update_entity",
            json!({"handle": "voucher:boot-as-patty", "name": "Boot as anybody"}),
        )
        .await
        .says("issued");

    // ── taking the claim back is refused ────────────────────────────────────
    marge
        .refused(
            "retract",
            json!({"address": address, "reason": "taking it back"}),
        )
        .await
        .says("issued");

    // ── and it reads back exactly as it was issued ──────────────────────────
    s.call(
        "recall",
        json!({"subject": "voucher:boot-as-patty", "facts": true}),
    )
    .await
    .says("\"state\":\"issued\"")
    .says("a voucher for boot-as on patty")
    .never_says("\"state\":\"held\"")
    .never_says("Boot as anybody");

    // ── the same verbs still work on a thing that is not a voucher ──────────
    marge
        .call(
            "update_entity",
            json!({"handle": "bot:patty", "name": "Patty the second"}),
        )
        .await;

    story.finish().await;
}

/// "A voucher exists. Can a bot rename it, fold it into something, archive it,
/// or make one by renaming?"
///
/// **Rename, merge and archive are turned back like every other write to a
/// voucher**, on either side of a rename or a merge: a voucher moved onto
/// another handle, or another thing renamed onto a voucher's kind, is a grant
/// taken without being given. **Each refusal sits beside the positive it
/// depends on**: the same verbs land on a bot.
#[tokio::test]
async fn a_bot_cannot_rename_merge_or_archive_a_voucher_nor_rename_into_one() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    s.add("bot:selma", "Selma").await;
    let marge = story.as_bot("bot:marge").await;
    let first = EntityId("voucher:boot-as-patty".into());
    let second = EntityId("voucher:boot-as-selma".into());
    issue_through_the_store(&story, &first).await;
    issue_through_the_store(&story, &second).await;

    // ── renaming a voucher is refused ───────────────────────────────────────
    marge
        .refused(
            "rename_entity",
            json!({"handle": "voucher:boot-as-patty", "to": "voucher:boot-as-anybody"}),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");
    marge
        .refused(
            "rename_entity",
            json!({"handle": "voucher:boot-as-patty", "to": "topic:widgets"}),
        )
        .await
        .says("issued");

    // ── and so is renaming anything onto a voucher's kind ───────────────────
    marge
        .refused(
            "rename_entity",
            json!({"handle": "bot:selma", "to": "voucher:made-by-hand"}),
        )
        .await
        .says("issued");

    // ── a merge is refused with the voucher on either side ──────────────────
    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "voucher:boot-as-patty", "survivor": "bot:patty"}),
        )
        .await
        .says("issued");
    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "bot:selma", "survivor": "voucher:boot-as-selma"}),
        )
        .await
        .says("issued");

    // ── archiving and restoring are refused ─────────────────────────────────
    marge
        .refused(
            "archive_entity",
            json!({"handle": "voucher:boot-as-patty", "reason": "tidying"}),
        )
        .await
        .says("issued");
    marge
        .refused(
            "archive_entity",
            json!({"handle": "voucher:boot-as-patty", "reason": "back", "restore": true}),
        )
        .await
        .says("issued");

    // ── both vouchers are exactly where they were, live, and nothing new ────
    let listed = s.list("voucher").await;
    listed
        .number("/count", 2)
        .says("voucher:boot-as-patty")
        .says("voucher:boot-as-selma")
        .never_says("anybody")
        .never_says("made-by-hand");

    // ── the same verbs still land on a bot ──────────────────────────────────
    marge
        .call(
            "rename_entity",
            json!({"handle": "bot:selma", "to": "bot:selma-the-second"}),
        )
        .await;
    marge
        .call(
            "archive_entity",
            json!({"handle": "bot:patty", "reason": "tidying"}),
        )
        .await;

    story.finish().await;
}
