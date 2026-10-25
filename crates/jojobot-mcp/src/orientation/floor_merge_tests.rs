//! **A merge or a rename of a bot is measured against the boot ceiling**, as every
//! other write that grows a bot's boot is.

use crate::harness::*;
use crate::memory::merge_entities::MergeArgs;
use crate::memory::reads_left::ReadsLeft;
use crate::memory::testing::*;
use jojobot_domain::memory::{EntityId, NewFact};
use rmcp::handler::server::wrapper::Parameters;

fn starred(jojobot: &crate::Jojobot, bot: &str, content: String) -> NewFact {
    let mut new = NewFact::about(
        EntityId(bot.into()),
        content,
        jojobot.clock().today_in(&jiff::tz::TimeZone::UTC),
    );
    new.fields = [("starred".to_string(), "true".to_string())].into();
    new
}

/// **A bot holding the longest starred rule its boot floor lets it hold**, found
/// by asking the capture guard, and the length it found.
pub(super) async fn a_bot_at_the_brink(jojobot: &crate::Jojobot, slug: &str) -> usize {
    make_bot(jojobot, slug).await;
    let handle = EntityId(format!("bot:{slug}"));
    let (mut fits, mut over) = (1usize, jojobot_domain::text::BOOT_ANSWER.budget);
    while over - fits > 1 {
        let middle = (fits + over) / 2;
        match jojobot
            .refuses_a_boot_floor_for_capture(
                &starred(jojobot, handle.as_str(), "x".repeat(middle)),
                &handle,
                &ReadsLeft::default(),
            )
            .await
        {
            None => fits = middle,
            Some(_) => over = middle,
        }
    }
    let landed = capture_ok(
        jojobot,
        crate::memory::CaptureArgs {
            fields: Some([("starred".to_string(), "true".to_string())].into()),
            ..capture_args(handle.as_str(), &"x".repeat(fits))
        },
    )
    .await;
    assert_ne!(
        landed["status"], "blocked",
        "the brink rule lands: {landed}"
    );
    fits
}

/// **A merge that folds a starred rule into a bot already at the ceiling is
/// refused for the boot floor, with nothing moved.** Paired with the same merge
/// into a bot with room, which lands, so the refusal is the floor's.
#[tokio::test]
async fn a_merge_that_takes_a_bots_boot_over_the_ceiling_is_refused() {
    let jojobot = crate::mailboxes::testing::mailbox_handler();
    a_bot_at_the_brink(&jojobot, "gamma").await;
    for slug in ["delta", "sigma", "omega"] {
        make_bot(&jojobot, slug).await;
    }
    for duplicate in ["bot:delta", "bot:omega"] {
        capture_ok(
            &jojobot,
            crate::memory::CaptureArgs {
                fields: Some([("starred".to_string(), "true".to_string())].into()),
                ..capture_args(duplicate, "a rule the duplicate carries")
            },
        )
        .await;
    }
    let merge = |duplicate: &str, survivor: &str| {
        let jojobot = &jojobot;
        let args = MergeArgs {
            request_id: None,
            duplicate: duplicate.into(),
            survivor: survivor.into(),
            reason: None,
            recorded_at: None,
            sid: Some(as_bot(jojobot, "sigma")),
        };
        async move {
            json_of(
                &jojobot
                    .merge_entities(Parameters(args))
                    .await
                    .expect("merge answers"),
            )
        }
    };

    let refused = merge("bot:delta", "bot:gamma").await;
    assert_eq!(
        refused["status"], "blocked",
        "the survivor's boot floor went over the ceiling and the merge landed: {refused}"
    );
    assert!(
        refused["floor"].as_u64() > refused["budget"].as_u64(),
        "the refusal is the boot floor's: {refused}"
    );

    // **Nothing moved**: the duplicate is no folded thing, and its claims are its own.
    let listed = jojobot.memory.list_entities(None).await.expect("lists");
    let delta = listed
        .iter()
        .find(|entity| entity.id.as_str() == "bot:delta")
        .expect("the duplicate is listed");
    assert_eq!(
        delta.merged_into, None,
        "the refused merge folded the duplicate"
    );
    let claims = jojobot
        .memory
        .recall(&EntityId("bot:delta".into()))
        .await
        .expect("recall reads");
    assert!(
        claims
            .iter()
            .any(|claim| claim.content.contains("duplicate carries"))
            && claims
                .iter()
                .all(|claim| claim.home.as_str() == "bot:delta"),
        "the duplicate's claims moved: {claims:?}"
    );

    let roomy = merge("bot:omega", "bot:sigma").await;
    assert_ne!(roomy["status"], "blocked", "sigma has room: {roomy}");
}

/// **A rename that takes a bot's boot over the ceiling is refused, with nothing
/// moved.** The handle rides in the bot's id, its box and the address of every
/// rule it carries. Paired with a rename of a bot with room, which lands.
#[tokio::test]
async fn a_rename_that_takes_a_bots_boot_over_the_ceiling_is_refused() {
    let jojobot = crate::mailboxes::testing::mailbox_handler();
    a_bot_at_the_brink(&jojobot, "gamma").await;
    make_bot(&jojobot, "delta").await;
    let rename = |handle: &str, to: &str| {
        let jojobot = &jojobot;
        let args = crate::memory::rename_entity::RenameEntityArgs {
            request_id: None,
            handle: handle.into(),
            to: to.into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            override_reason: None,
            sid: Some(writing_as(jojobot)),
        };
        async move {
            json_of(
                &jojobot
                    .rename_entity(Parameters(args))
                    .await
                    .expect("rename answers"),
            )
        }
    };

    let refused = rename("bot:gamma", "bot:gamma-with-a-much-longer-handle").await;
    assert_eq!(
        refused["status"], "blocked",
        "the bot's boot floor went over the ceiling and the rename landed: {refused}"
    );
    assert!(
        refused["floor"].as_u64() > refused["budget"].as_u64(),
        "the refusal is the boot floor's: {refused}"
    );

    // **Nothing moved**: the old handle still answers and the new one names nothing.
    let listed = jojobot.memory.list_entities(None).await.expect("lists");
    assert!(
        listed
            .iter()
            .any(|entity| entity.id.as_str() == "bot:gamma"),
        "the refused rename took the old handle away"
    );
    assert!(
        listed
            .iter()
            .all(|entity| entity.id.as_str() != "bot:gamma-with-a-much-longer-handle"),
        "the refused rename made the new handle"
    );

    let roomy = rename("bot:delta", "bot:delta-with-a-much-longer-handle").await;
    assert_ne!(roomy["status"], "blocked", "delta has room: {roomy}");
}
