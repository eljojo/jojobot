//! **What the floor guard of a merge or a rename leaves alone, and what it says
//! when it could not measure**, beside the cases that show it refusing.

use super::floor_merge_tests::a_bot_at_the_brink;
use crate::harness::*;
use crate::memory::merge_entities::MergeArgs;
use crate::memory::testing::*;
use rmcp::handler::server::wrapper::Parameters;

async fn merged(jojobot: &crate::Jojobot, duplicate: &str, survivor: &str) -> serde_json::Value {
    let args = MergeArgs {
        request_id: None,
        duplicate: duplicate.into(),
        survivor: survivor.into(),
        reason: None,
        recorded_at: None,
        sid: Some(writing_as(jojobot)),
    };
    json_of(
        &jojobot
            .merge_entities(Parameters(args))
            .await
            .expect("merge answers"),
    )
}

async fn renamed(jojobot: &crate::Jojobot, handle: &str, to: &str) -> serde_json::Value {
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
    json_of(
        &jojobot
            .rename_entity(Parameters(args))
            .await
            .expect("rename answers"),
    )
}

/// **The names a merge brings grow the survivor's boot too.** The duplicate holds
/// no rule at all, only a long name, and the survivor is at the ceiling, so the
/// refusal is the names' and not a rule's.
#[tokio::test]
async fn a_merge_that_brings_only_a_long_name_takes_a_bots_boot_over_the_ceiling() {
    let jojobot = crate::mailboxes::testing::mailbox_handler();
    a_bot_at_the_brink(&jojobot, "gamma").await;
    let long = "delta-with-a-name-that-is-long-and-carries-no-rule";
    make_bot(&jojobot, long).await;

    let refused = merged(&jojobot, &format!("bot:{long}"), "bot:gamma").await;
    assert_eq!(
        refused["status"], "blocked",
        "the names took the survivor over the ceiling and the merge landed: {refused}"
    );
    assert!(
        refused["floor"].as_u64() > refused["budget"].as_u64(),
        "the refusal is the boot floor's: {refused}"
    );
}

/// **A bot already over the ceiling stays repairable by a write that does not
/// grow it.** Entities are added until the snapshot every boot carries takes the
/// bot over; a rename to a handle of the same length then lands, and one to a
/// longer handle is refused.
#[tokio::test]
async fn a_bot_over_the_ceiling_is_renamed_by_a_handle_that_does_not_grow_its_boot() {
    let jojobot = crate::mailboxes::testing::mailbox_handler();
    a_bot_at_the_brink(&jojobot, "gamma").await;
    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
    let over = || async {
        jojobot
            .boot_floors_of_bots()
            .await
            .into_iter()
            .any(|(bot, floor)| bot.as_str() == "bot:gamma" && floor > budget)
    };
    for n in 0..200 {
        if over().await {
            break;
        }
        jojobot
            .add_entity(Parameters(add_args(
                "thing",
                &format!("thing:filler-{n}"),
                &format!("Filler {n}"),
            )))
            .await
            .expect("adds");
    }
    assert!(
        over().await,
        "the snapshot never took the bot over the ceiling, so the case proves nothing"
    );

    let longer = renamed(&jojobot, "bot:gamma", "bot:gamma-grown").await;
    assert_eq!(
        longer["status"], "blocked",
        "a growing rename landed: {longer}"
    );
    let same = renamed(&jojobot, "bot:gamma", "bot:gammb").await;
    assert_ne!(
        same["status"], "blocked",
        "a rename that does not grow the boot must stay possible: {same}"
    );
}

/// **A floor that could not be measured is said, on the verb that skipped it**
/// (rule 130). The bot's record cannot be read, so the check is skipped and the
/// write lands with `reads_left` naming the floor; the same write over a store
/// that reads carries no such block.
#[tokio::test]
async fn a_merge_or_rename_over_an_unreadable_bot_says_the_floor_was_not_measured() {
    let (healthy, blind) = healthy_and_down(Down::ScanOf("bot:gamma"));
    for slug in ["gamma", "delta", "sigma", "omega"] {
        make_bot(&healthy, slug).await;
    }

    let measured = merged(&healthy, "bot:omega", "bot:delta").await;
    assert_ne!(measured["status"], "blocked", "{measured}");
    assert!(measured.get("reads_left").is_none(), "{measured}");
    let unmeasured = merged(&blind, "bot:sigma", "bot:gamma").await;
    assert_ne!(unmeasured["status"], "blocked", "{unmeasured}");
    assert_eq!(
        unmeasured["reads_left"]["not_worked_out"][0]["what"],
        crate::memory::reads_left::BOOT_FLOOR,
        "the merge landed over an unmeasured floor and said nothing: {unmeasured}"
    );

    let measured = renamed(&healthy, "bot:delta", "bot:delta-renamed").await;
    assert_ne!(measured["status"], "blocked", "{measured}");
    assert!(measured.get("reads_left").is_none(), "{measured}");
    let unmeasured = renamed(&blind, "bot:gamma", "bot:gamma-renamed").await;
    assert_ne!(unmeasured["status"], "blocked", "{unmeasured}");
    assert_eq!(
        unmeasured["reads_left"]["not_worked_out"][0]["what"],
        crate::memory::reads_left::BOOT_FLOOR,
        "the rename landed over an unmeasured floor and said nothing: {unmeasured}"
    );
}
