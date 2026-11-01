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
            .boot_floors_of_bots(&crate::memory::reads_left::ReadsLeft::default())
            .await
            .into_iter()
            .any(|(bot, floor)| bot.as_str() == "bot:gamma" && floor > budget)
    };
    for name in [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
        "chi", "psi", "omega",
    ] {
        if over().await {
            break;
        }
        jojobot
            .add_entity(Parameters(add_args(
                "thing",
                &format!("thing:{name}"),
                &format!("Filler {name}"),
            )))
            .await
            .expect("adds");
    }
    assert!(
        over().await,
        "the snapshot never took the bot over the ceiling, so the case proves nothing"
    );

    let longer = renamed(&jojobot, "bot:gamma", "bot:epsilon").await;
    assert_eq!(
        longer["status"], "blocked",
        "a growing rename landed: {longer}"
    );
    let same = renamed(&jojobot, "bot:gamma", "bot:alpha").await;
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

    let measured = renamed(&healthy, "bot:delta", "bot:rho").await;
    assert_ne!(measured["status"], "blocked", "{measured}");
    assert!(measured.get("reads_left").is_none(), "{measured}");
    let unmeasured = renamed(&blind, "bot:gamma", "bot:psi").await;
    assert_ne!(unmeasured["status"], "blocked", "{unmeasured}");
    assert_eq!(
        unmeasured["reads_left"]["not_worked_out"][0]["what"],
        crate::memory::reads_left::BOOT_FLOOR,
        "the rename landed over an unmeasured floor and said nothing: {unmeasured}"
    );
}

/// **A bot pushed over the ceiling by the off-the-tree warning can file the rules
/// the warning names.** Two starred rules name no purpose and the floor is over the
/// ceiling, with the warning counted in it. Giving one of them a purpose lands,
/// because the warning is what the filing is meant to remove; a write that makes a
/// rule itself longer is refused on the same bot.
#[tokio::test]
async fn a_bot_over_the_ceiling_by_the_warning_can_still_file_its_rules() {
    use jojobot_domain::memory::{EntityId, NewFact};
    let jojobot = crate::mailboxes::testing::mailbox_handler();
    a_bot_at_the_brink(&jojobot, "gamma").await;
    let bot = EntityId("bot:gamma".into());
    // **Written past the guard, as a store written before the guard holds it.**
    let mut second = NewFact::about(
        bot.clone(),
        "a second rule that names no purpose",
        jiff::civil::Date::constant(2026, 3, 10),
    );
    second.fields = [("starred".to_string(), "true".to_string())].into();
    jojobot
        .memory
        .capture(second)
        .await
        .expect("the rule is written");
    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
    let floor = jojobot
        .boot_floors_of_bots(&crate::memory::reads_left::ReadsLeft::default())
        .await
        .into_iter()
        .find(|(held, _)| held == &bot)
        .expect("the bot's floor is measured")
        .1;
    assert!(
        floor > budget,
        "the floor is not over the ceiling, so the case proves nothing: {floor} of {budget}"
    );
    let held = jojobot.memory.recall(&bot).await.expect("the bot's claims");
    let address_of = |content: &str| {
        held.iter()
            .find(|fact| fact.content == content)
            .unwrap_or_else(|| panic!("no claim says {content}"))
            .address()
            .to_string()
    };
    let update = |args: crate::memory::update_fact::UpdateFactArgs| {
        let jojobot = &jojobot;
        async move {
            json_of(
                &jojobot
                    .update_fact(Parameters(crate::memory::update_fact::UpdateFactArgs {
                        sid: Some(writing_as(jojobot)),
                        ..args
                    }))
                    .await
                    .expect("update answers"),
            )
        }
    };

    let growing = update(crate::memory::update_fact::UpdateFactArgs {
        content: Some("x".repeat(3000)),
        provenance: Some("inference".into()),
        ..update_args(&address_of("a second rule that names no purpose"))
    })
    .await;
    assert_eq!(
        growing["status"], "blocked",
        "a write that grows a rule landed over the ceiling: {growing}"
    );
    // **The refusal is the floor's**, which names the floor and its parts; a claim
    // too big to read back is refused as blocked too, without them.
    assert!(
        growing.get("floor_parts").is_some(),
        "the refusal is not the boot floor's: {growing}"
    );
    // A purpose is a label: a long one is growth like any other.
    let long = update(crate::memory::update_fact::UpdateFactArgs {
        fields: Some([("purpose".to_string(), "x".repeat(400))].into()),
        ..update_args(&address_of("a second rule that names no purpose"))
    })
    .await;
    assert!(
        long.get("floor_parts").is_some(),
        "a long purpose was taken for a filing: {long}"
    );
    let filing = update(crate::memory::update_fact::UpdateFactArgs {
        fields: Some([("purpose".to_string(), "boot".to_string())].into()),
        ..update_args(&address_of("a second rule that names no purpose"))
    })
    .await;
    assert_ne!(
        filing["status"], "blocked",
        "the filing the warning asks for was refused: {filing}"
    );
    let filed = jojobot.memory.recall(&bot).await.expect("the bot's claims");
    assert!(
        filed.iter().any(|fact| {
            fact.content == "a second rule that names no purpose"
                && fact.fields.get("purpose").map(String::as_str) == Some("boot")
        }),
        "the purpose landed: {filed:?}"
    );
}
