//! "Alpha runs gamma. Why can a session booted as beta take it?"
//!
//! A role belongs to the bot that carries its name in `claims_role`, or, when
//! no bot does, to the bot whose child the role object is. A claim at the door
//! from any other bot is refused before anything is written: the refusal names
//! the owner and offers to boot as it. A name no bot owns stays free, and the
//! first claim makes its object under the claimant.

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
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;

    // ── the owner by name: alpha carries gamma ─────────────────────────────
    s.event_with(
        "bot:alpha",
        "alpha runs gamma",
        json!({"claims_role": "gamma"}),
        &[],
    )
    .await;

    // Beta asks for gamma and is refused. The answer names alpha and offers to
    // boot as it.
    let (beta, _) = story
        .call(
            "start_here",
            json!({"bot": "beta", "brief": true, "claim": "gamma"}),
        )
        .await;
    let beta = beta.json();
    assert_eq!(claim_of(&beta)["status"], "refused", "{beta}");
    assert_eq!(claim_of(&beta)["owner"], "bot:alpha", "{beta}");
    assert!(
        claim_of(&beta)["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("alpha") && way.contains("start_here")),
        "the refusal offers to boot as the owner: {beta}"
    );
    // Nothing was written anywhere: no role object, no role key on beta.
    s.list("role").await.never_says("role:gamma");
    s.call("recall", json!({"subject": "bot:beta", "facts": true}))
        .await
        .never_says("role/gamma");

    // In the same read, alpha claims gamma and is taken.
    let (alpha, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "gamma"}),
        )
        .await;
    let alpha = alpha.json();
    assert_eq!(claim_of(&alpha)["status"], "taken", "{alpha}");
    s.list("role").await.says("role:gamma");

    // ── the owner by object: omega has no carrier, only a parent ────────────
    let (first, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "omega", "resume": "new"}),
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
            json!({"bot": "beta", "brief": true, "claim": "omega"}),
        )
        .await;
    let stranger = stranger.json();
    assert_eq!(claim_of(&stranger)["status"], "refused", "{stranger}");
    assert_eq!(claim_of(&stranger)["owner"], "bot:alpha", "{stranger}");
    s.call("recall", json!({"subject": "bot:beta", "facts": true}))
        .await
        .never_says("role/omega");

    // ── a name nobody owns is free, and the first claim keeps it ────────────
    let (free, _) = story
        .call(
            "start_here",
            json!({"bot": "beta", "brief": true, "claim": "sigma", "resume": "new"}),
        )
        .await;
    assert_eq!(claim_of(&free.json())["status"], "taken", "{}", free.json());
    s.list("role").await.says("role:sigma");

    s.wrap("kept gamma with alpha").await;
    story.finish().await;
}

fn reporting_to(subject: &str, manager: &str) -> serde_json::Value {
    json!({
        "subject": subject, "content": format!("{subject} reports to {manager}"),
        "provenance": "testimony", "fields": {"reports_to": manager},
    })
}

/// "Beta archived the role object, then claimed the role itself."
///
/// A role object is archived or restored only by its parent bot, the owner, or
/// by the bot heading the chart. Any other caller is refused and told who may.
/// Without this, archiving would free the name and walk around the owner check.
#[tokio::test]
async fn only_the_owner_or_the_chart_head_may_archive_or_restore_a_role_object() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;
    s.add("bot:omega", "Omega").await;
    s.add("person:homer", "Homer").await;
    let alpha = story.as_bot("bot:alpha").await;
    let beta = story.as_bot("bot:beta").await;
    let omega = story.as_bot("bot:omega").await;

    // Omega heads the chart: alpha reports to it and it reports to nobody.
    omega
        .call("capture", reporting_to("bot:alpha", "bot:omega"))
        .await;

    // Alpha claims gamma, which makes the role object under alpha.
    let (claimed, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "gamma", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claimed.json()["session"]["claim"]["status"],
        "taken",
        "{}",
        claimed.json()
    );
    s.list("role").await.says("role:gamma");

    // ── a stranger is refused, and told who may ─────────────────────────────
    beta.refused(
        "archive_entity",
        json!({"handle": "role:gamma", "reason": "i want it"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:alpha")
    .says("bot:omega");
    s.list("role").await.says("role:gamma");

    // The same stranger archives anything that is not a role object.
    beta.call(
        "archive_entity",
        json!({"handle": "person:homer", "reason": "a mistaken write"}),
    )
    .await;

    // ── the owner's archive lands ───────────────────────────────────────────
    alpha
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "made by mistake"}),
        )
        .await;
    s.list("role").await.never_says("role:gamma");

    // Restore follows the same rule.
    beta.refused(
        "archive_entity",
        json!({"handle": "role:gamma", "reason": "back", "restore": true}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:alpha")
    .says("bot:omega");
    s.list("role").await.never_says("role:gamma");
    alpha
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "needed after all", "restore": true}),
        )
        .await;
    s.list("role").await.says("role:gamma");

    // ── the chart head's archive and restore land ───────────────────────────
    omega
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "cleared by the head"}),
        )
        .await;
    s.list("role").await.never_says("role:gamma");
    omega
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "put back", "restore": true}),
        )
        .await;
    s.list("role").await.says("role:gamma");

    s.wrap("kept the role object with its owner").await;
    story.finish().await;
}

/// "Alpha archived the role object. Can anyone claim the role now?"
///
/// **A claim whose role object is archived is refused, and the refusal names the
/// way back.** The object is archived but its claim record is still on it, and a
/// claim would be written onto an object nobody reads. The way forward is to
/// restore the object, which only its parent or the chart head may do; the
/// refusal says so. Restored, the object is the owner's again and the other bot
/// is told the role belongs to alpha, so a build that refused every claim of
/// this role would not pass.
#[tokio::test]
async fn a_claim_on_an_archived_role_object_is_refused_naming_the_restore() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;
    let alpha = story.as_bot("bot:alpha").await;
    let claim_as = |bot: &'static str| {
        let story = &story;
        async move {
            let (booted, _) = story
                .call(
                    "start_here",
                    json!({"bot": bot, "brief": true, "claim": "gamma", "resume": "new"}),
                )
                .await;
            booted.json()
        }
    };

    let first = claim_as("alpha").await;
    assert_eq!(claim_of(&first)["status"], "taken", "{first}");
    alpha
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "made by mistake"}),
        )
        .await;

    // ── nobody claims it while it is archived, and the answer says how back ──
    for bot in ["beta", "alpha"] {
        let claim = claim_as(bot).await;
        let said = claim_of(&claim);
        assert_eq!(said["status"], "refused", "{claim}");
        assert_eq!(said["fix_by"], "change", "{claim}");
        let how = said["how_to_proceed"].as_str().unwrap_or_default();
        assert!(
            how.contains("role:gamma") && how.contains("restore") && how.contains("bot:alpha"),
            "the refusal names the object, the restore and who may: {claim}"
        );
    }
    s.list("role").await.never_says("role:gamma");

    // ── restored, the object is alpha's again and the answer changes ────────
    alpha
        .call(
            "archive_entity",
            json!({"handle": "role:gamma", "reason": "needed after all", "restore": true}),
        )
        .await;
    let after = claim_as("beta").await;
    assert_eq!(claim_of(&after)["owner"], "bot:alpha", "{after}");
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

/// "Beta moved the role object under itself, then claimed the role."
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
        ("bot:alpha", "Alpha"),
        ("bot:beta", "Beta"),
        ("bot:omega", "Omega"),
    ] {
        s.add(handle, name).await;
    }
    s.add("thing:handcart", "Handcart").await;
    let alpha = story.as_bot("bot:alpha").await;
    let beta = story.as_bot("bot:beta").await;
    let omega = story.as_bot("bot:omega").await;
    omega
        .call("capture", reporting_to("bot:alpha", "bot:omega"))
        .await;

    for role in ["gamma", "sigma", "theta"] {
        let (claimed, _) = story
            .call(
                "start_here",
                json!({"bot": "alpha", "brief": true, "claim": role, "resume": "new"}),
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
    beta.refused(
        "rename_entity",
        json!({"handle": "role:gamma", "to": "role:gamma", "parent": "bot:beta"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:alpha")
    .says("bot:omega");
    s.call("recall", json!({"subject": "role:gamma"}))
        .await
        .says("\"parent\":\"bot:alpha\"");

    // ── a stranger's merge of a role object is refused, from either side ────
    beta.refused(
        "merge_entities",
        json!({"duplicate": "role:sigma", "survivor": "thing:handcart", "reason": "tidy"}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:alpha");
    beta.refused(
        "merge_entities",
        json!({"duplicate": "thing:handcart", "survivor": "role:gamma", "reason": "tidy"}),
    )
    .await
    .says("bot:alpha");
    s.call("recall", json!({"subject": "role:sigma"}))
        .await
        .never_says("merged_into");

    // ── the owner's and the head's own acts land ────────────────────────────
    alpha
        .call(
            "merge_entities",
            json!({"duplicate": "role:sigma", "survivor": "role:gamma", "reason": "the same seat"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:sigma"}))
        .await
        .says("merged_into");
    omega
        .call(
            "rename_entity",
            json!({"handle": "role:theta", "to": "role:theta", "parent": "bot:omega"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:theta"}))
        .await
        .says("\"parent\":\"bot:omega\"");

    s.wrap("kept the role objects with their owner").await;
    story.finish().await;
}

/// "Beta renamed the role object out of the way, then claimed the role."
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
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;
    let beta = story.as_bot("bot:beta").await;
    let (claimed, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "gamma", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claim_of(&claimed.json())["status"],
        "taken",
        "{}",
        claimed.json()
    );

    // A slug change, then a kind change.
    for to in ["role:omega", "thing:gamma"] {
        let refused = beta
            .refused("rename_entity", json!({"handle": "role:gamma", "to": to}))
            .await;
        refused.says("\"wrote\":false").says("bot:alpha");
        s.call("recall", json!({"subject": "role:gamma"}))
            .await
            .says("\"parent\":\"bot:alpha\"");
    }

    let alpha = story.as_bot("bot:alpha").await;
    alpha
        .call(
            "rename_entity",
            json!({"handle": "role:gamma", "to": "role:omega"}),
        )
        .await;
    s.call("recall", json!({"subject": "role:omega"}))
        .await
        .says("\"parent\":\"bot:alpha\"");
    story.finish().await;
}
