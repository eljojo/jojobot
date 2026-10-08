//! "Marge's assistant keeps the weekly dinner plan, and Bart's assistant runs
//! the carpool. Can the carpool assistant take the dinner plan over?"
//!
//! The dinner plan is a role, a standing job one assistant holds. A role
//! belongs to the bot that carries its name in `claims_role`, or, when no bot
//! does, to the bot whose child the role object is. A claim at the door from
//! any other bot is refused before anything is written: the refusal names the
//! owner and offers to boot as it. A name no bot owns stays free, and the first
//! claim makes its object under the claimant.

use serde_json::json;

use super::dsl::Story;

/// What the boot answered about the claim, whole.
fn claim_of(booted: &serde_json::Value) -> &serde_json::Value {
    &booted["session"]["claim"]
}

#[tokio::test]
async fn a_claim_at_the_door_is_refused_when_the_role_belongs_to_another_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;

    // ── the owner by name: marge's assistant carries the dinner plan ─────────
    s.event_with(
        "bot:marge",
        "marge runs dinners",
        json!({"claims_role": "dinners"}),
        &[],
    )
    .await;

    // Bart's assistant asks for the dinner plan and is refused. The answer names
    // marge and offers to boot as it.
    let (bart, _) = story
        .call(
            "start_here",
            json!({"bot": "bart", "brief": true, "claim": "dinners"}),
        )
        .await;
    let bart = bart.json();
    assert_eq!(claim_of(&bart)["status"], "refused", "{bart}");
    assert_eq!(claim_of(&bart)["owner"], "bot:marge", "{bart}");
    assert!(
        claim_of(&bart)["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("marge") && way.contains("start_here")),
        "the refusal offers to boot as the owner: {bart}"
    );
    // Nothing was written anywhere: no role object, no role key on bart.
    s.list("role").await.never_says("role:dinners");
    s.call("recall", json!({"subject": "bot:bart", "facts": true}))
        .await
        .never_says("role/dinners");

    // In the same read, marge's assistant claims the dinner plan and is taken.
    let (marge, _) = story
        .call(
            "start_here",
            json!({"bot": "marge", "brief": true, "claim": "dinners"}),
        )
        .await;
    let marge = marge.json();
    assert_eq!(claim_of(&marge)["status"], "taken", "{marge}");
    s.list("role").await.says("role:dinners");

    // ── the owner by object: nobody carries the grocery run, only a parent ──
    let (first, _) = story
        .call(
            "start_here",
            json!({"bot": "marge", "brief": true, "claim": "groceries", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claim_of(&first.json())["status"],
        "taken",
        "{}",
        first.json()
    );

    let (stranger, _) = story
        .call(
            "start_here",
            json!({"bot": "bart", "brief": true, "claim": "groceries"}),
        )
        .await;
    let stranger = stranger.json();
    assert_eq!(claim_of(&stranger)["status"], "refused", "{stranger}");
    assert_eq!(claim_of(&stranger)["owner"], "bot:marge", "{stranger}");
    s.call("recall", json!({"subject": "bot:bart", "facts": true}))
        .await
        .never_says("role/groceries");

    // ── a job nobody owns is free, and the first claim keeps it ─────────────
    let (free, _) = story
        .call(
            "start_here",
            json!({"bot": "bart", "brief": true, "claim": "carpool", "resume": "new"}),
        )
        .await;
    assert_eq!(claim_of(&free.json())["status"], "taken", "{}", free.json());
    s.list("role").await.says("role:carpool");

    s.wrap("kept dinners with marge").await;
    story.finish().await;
}

fn reporting_to(subject: &str, manager: &str) -> serde_json::Value {
    json!({
        "subject": subject, "content": format!("{subject} reports to {manager}"),
        "provenance": "testimony", "fields": {"reports_to": manager},
    })
}

/// "Can the carpool assistant archive the dinner plan, then claim it for itself?"
///
/// A role object is archived or restored only by its parent bot, the owner, or
/// by the bot heading the chart. Any other caller is refused and told who may.
/// Without this, archiving would free the name and walk around the owner check.
#[tokio::test]
async fn only_the_owner_or_the_chart_head_may_archive_or_restore_a_role_object() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:smithers", "Smithers").await;
    s.add("person:homer", "Homer").await;
    let marge = story.as_bot("bot:marge").await;
    let bart = story.as_bot("bot:bart").await;
    let smithers = story.as_bot("bot:smithers").await;

    // Smithers is the household's coordinator and heads the chart: marge reports
    // to it and it reports to nobody.
    smithers
        .call("capture", reporting_to("bot:marge", "bot:smithers"))
        .await;

    // Marge's assistant claims the dinner plan, which makes the role object
    // under marge.
    let (claimed, _) = story
        .call(
            "start_here",
            json!({"bot": "marge", "brief": true, "claim": "dinners", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claimed.json()["session"]["claim"]["status"],
        "taken",
        "{}",
        claimed.json()
    );
    s.list("role").await.says("role:dinners");

    // ── a stranger is refused, and told who may ─────────────────────────────
    bart.refused(
        "archive_entity",
        json!({"handle": "role:dinners", "reason": "i want it"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge")
    .says("bot:smithers");
    s.list("role").await.says("role:dinners");

    // The same stranger archives anything that is not a role object.
    bart.call(
        "archive_entity",
        json!({"handle": "person:homer", "reason": "a mistaken write"}),
    )
    .await;

    // ── the owner's archive lands ───────────────────────────────────────────
    marge
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "made by mistake"}),
        )
        .await;
    s.list("role").await.never_says("role:dinners");

    // Restore follows the same rule.
    bart.refused(
        "archive_entity",
        json!({"handle": "role:dinners", "reason": "back", "restore": true}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge")
    .says("bot:smithers");
    s.list("role").await.never_says("role:dinners");
    marge
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "needed after all", "restore": true}),
        )
        .await;
    s.list("role").await.says("role:dinners");

    // ── the chart head's archive and restore land ───────────────────────────
    smithers
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "cleared by the head"}),
        )
        .await;
    s.list("role").await.never_says("role:dinners");
    smithers
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "put back", "restore": true}),
        )
        .await;
    s.list("role").await.says("role:dinners");

    s.wrap("kept the role object with its owner").await;
    story.finish().await;
}

/// "Marge's assistant archived the dinner plan. Can anyone claim it now?"
///
/// **A claim whose role object is archived is refused, and the refusal names the
/// way back.** The object is archived but its claim record is still on it, and a
/// claim would be written onto an object nobody reads. The way forward is to
/// restore the object, which only its parent or the chart head may do; the
/// refusal says so. Restored, the object is the owner's again and the other bot
/// is told the dinner plan belongs to marge, so a build that refused every claim
/// of this role would not pass.
#[tokio::test]
async fn a_claim_on_an_archived_role_object_is_refused_naming_the_restore() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let marge = story.as_bot("bot:marge").await;
    let claim_as = |bot: &'static str| {
        let story = &story;
        async move {
            let (booted, _) = story
                .call(
                    "start_here",
                    json!({"bot": bot, "brief": true, "claim": "dinners", "resume": "new"}),
                )
                .await;
            booted.json()
        }
    };

    let first = claim_as("marge").await;
    assert_eq!(claim_of(&first)["status"], "taken", "{first}");
    marge
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "made by mistake"}),
        )
        .await;

    // ── nobody claims it while it is archived, and the answer says how back ──
    for bot in ["bart", "marge"] {
        let claim = claim_as(bot).await;
        let said = claim_of(&claim);
        assert_eq!(said["status"], "refused", "{claim}");
        assert_eq!(said["fix_by"], "change", "{claim}");
        let how = said["how_to_proceed"].as_str().unwrap_or_default();
        assert!(
            how.contains("role:dinners") && how.contains("restore") && how.contains("bot:marge"),
            "the refusal names the object, the restore and who may: {claim}"
        );
    }
    s.list("role").await.never_says("role:dinners");

    // ── restored, the object is marge's again and the answer changes ────────
    marge
        .call(
            "archive_entity",
            json!({"handle": "role:dinners", "reason": "needed after all", "restore": true}),
        )
        .await;
    let after = claim_as("bart").await;
    assert_eq!(claim_of(&after)["owner"], "bot:marge", "{after}");
    assert!(
        !claim_of(&after)["how_to_proceed"]
            .as_str()
            .unwrap_or_default()
            .contains("restore"),
        "a restored object is no longer sent to be restored: {after}"
    );

    s.wrap("archived and restored the role object").await;
    story.finish().await;
}

/// "Can the carpool assistant move the dinner plan under itself, or fold it into
/// something of its own?"
///
/// **Moving a role object to another parent, or merging it into another thing, is
/// the owner's or the chart head's, by the same rule as archiving it.** Either
/// act hands the name to somebody else and walks around the owner check. A
/// stranger is refused and told who may; the owner's and the head's own acts land
/// beside them, so a build that refused every reparent and every merge of a role
/// object would not pass.
#[tokio::test]
async fn only_the_owner_or_the_chart_head_may_move_or_merge_a_role_object() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("bot:marge", "Marge"),
        ("bot:bart", "Bart"),
        ("bot:smithers", "Smithers"),
    ] {
        s.add(handle, name).await;
    }
    s.add("thing:handcart", "Handcart").await;
    let marge = story.as_bot("bot:marge").await;
    let bart = story.as_bot("bot:bart").await;
    let smithers = story.as_bot("bot:smithers").await;
    smithers
        .call("capture", reporting_to("bot:marge", "bot:smithers"))
        .await;

    for role in ["dinners", "carpool", "laundry", "bills", "garden"] {
        let (claimed, _) = story
            .call(
                "start_here",
                json!({"bot": "marge", "brief": true, "claim": role, "resume": "new"}),
            )
            .await;
        assert_eq!(
            claim_of(&claimed.json())["status"],
            "taken",
            "{}",
            claimed.json()
        );
    }

    // ── a stranger's reparent is refused, and told who may ──────────────────
    bart.refused(
        "rename_entity",
        json!({"handle": "role:dinners", "to": "role:dinners", "parent": "bot:bart"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge")
    .says("bot:smithers");
    s.call("recall", json!({"subject": "role:dinners"}))
        .await
        .says("\"parent\":\"bot:marge\"");

    // ── a stranger's merge of a role object is refused, from either side ────
    bart.refused(
        "merge_entities",
        json!({"duplicate": "role:carpool", "survivor": "thing:handcart", "reason": "tidy"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge");
    bart.refused(
        "merge_entities",
        json!({"duplicate": "thing:handcart", "survivor": "role:dinners", "reason": "tidy"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge");
    // Read back: the thing a stranger tried to fold into a role object is still
    // there, whole, and was folded into nothing.
    s.call("recall", json!({"subject": "thing:handcart"}))
        .await
        .says("\"id\":\"thing:handcart\"")
        .never_says("merged_into");
    s.call("recall", json!({"subject": "role:carpool"}))
        .await
        .never_says("merged_into");

    // ── the owner's and the head's own acts land ────────────────────────────
    //
    // Each act is tried, the owner's reparent and merge and the head's merge and
    // reparent, and the first of each pair is the one a build that refused only
    // that licence would stop at.
    marge
        .call(
            "rename_entity",
            json!({"handle": "role:bills", "to": "role:bills", "parent": "bot:bart"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:bills"}))
        .await
        .says("\"parent\":\"bot:bart\"");
    marge
        .call(
            "merge_entities",
            json!({"duplicate": "role:carpool", "survivor": "role:dinners", "reason": "the same seat"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:carpool"}))
        .await
        .says("merged_into");
    smithers
        .call(
            "merge_entities",
            json!({"duplicate": "role:garden", "survivor": "role:dinners", "reason": "the same seat"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:garden"}))
        .await
        .says("merged_into");
    smithers
        .call(
            "rename_entity",
            json!({"handle": "role:laundry", "to": "role:laundry", "parent": "bot:smithers"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:laundry"}))
        .await
        .says("\"parent\":\"bot:smithers\"");

    s.wrap("kept the role objects with their owner").await;
    story.finish().await;
}

/// "Can the carpool assistant rename the dinner plan out of the way, then claim
/// the name?"
///
/// **Any change to a role object is judged as archiving it is**, whatever the
/// call changes: a new parent, a new slug, a new kind. A slug or a kind that
/// changes frees the old name, and the stranger who freed it could then claim
/// it. A stranger's slug change and kind change are refused, naming who may; the
/// owner's slug change lands, so a build that refused every rename of a role
/// object would not pass.
#[tokio::test]
async fn a_stranger_may_not_rename_or_retype_a_role_object_and_the_owner_may() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    let (claimed, _) = story
        .call(
            "start_here",
            json!({"bot": "marge", "brief": true, "claim": "dinners", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claim_of(&claimed.json())["status"],
        "taken",
        "{}",
        claimed.json()
    );

    // A slug change, then a kind change.
    for to in ["role:groceries", "thing:dinners"] {
        let refused = bart
            .refused("rename_entity", json!({"handle": "role:dinners", "to": to}))
            .await;
        refused.says("\"wrote\":false").says("bot:marge");
        s.call("recall", json!({"subject": "role:dinners"}))
            .await
            .says("\"parent\":\"bot:marge\"");
    }

    let marge = story.as_bot("bot:marge").await;
    marge
        .call(
            "rename_entity",
            json!({"handle": "role:dinners", "to": "role:groceries"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:groceries"}))
        .await
        .says("\"parent\":\"bot:marge\"");
    story.finish().await;
}
