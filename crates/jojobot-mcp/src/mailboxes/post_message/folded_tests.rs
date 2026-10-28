//! **Mail posted to a bot that was folded into another reaches the survivor**
//! and the receipt names the fold (rule 466). A fold leaves the folded bot's
//! handle answering, so a sender who wrote the old handle must not send mail to
//! a box nobody reads.

use super::*;
use crate::harness::{as_bot, handler_over, json_of};
use crate::mailboxes::testing::{counts, mailbox_handler, owning, send};
use crate::memory::testing::{Down, healthy_and_down};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{Boot, Entity, EntityId, EntityKind, Memory, MemoryError};

/// Fold `duplicate` into `survivor`, as the survivor's own bot.
async fn fold(jojobot: &Jojobot, duplicate: &str, survivor: &str) {
    let landed = json_of(
        &jojobot
            .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
                request_id: None,
                duplicate: duplicate.into(),
                survivor: survivor.into(),
                reason: None,
                recorded_at: None,
                sid: Some(as_bot(jojobot, survivor.trim_start_matches("bot:"))),
            }))
            .await
            .expect("merge ok"),
    );
    assert_eq!(landed["merged"], duplicate, "the merge landed: {landed}");
}

fn waiting(counts: &serde_json::Value) -> u64 {
    counts["counts"]["new"].as_u64().expect("a count")
}

/// **A message to the folded handle lands in the survivor's box**, the receipt
/// says which bot it was sent to and which it reached, and the folded bot's own
/// box takes nothing. Paired with a message to the survivor by its own handle,
/// which carries no fold.
#[tokio::test]
async fn mail_to_a_folded_bot_lands_in_the_survivors_box_and_the_receipt_names_the_fold() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "lisa").await;
    owning(&jojobot, "delta").await;
    owning(&jojobot, "bart").await;
    fold(&jojobot, "bot:lisa", "bot:delta").await;

    let receipt = send(&jojobot, "bot:lisa", "bart", "your turn on the damper").await;
    assert_eq!(receipt["mailbox"], "delta", "{receipt}");
    assert_eq!(receipt["folded"]["sent_to"], "bot:lisa", "{receipt}");
    assert_eq!(receipt["folded"]["folded_into"], "bot:delta", "{receipt}");
    assert_eq!(waiting(&counts(&jojobot, "delta").await), 1);
    assert_eq!(waiting(&counts(&jojobot, "lisa").await), 0);

    let direct = send(&jojobot, "bot:delta", "bart", "and again").await;
    assert_eq!(direct["mailbox"], "delta", "{direct}");
    assert!(direct.get("folded").is_none(), "{direct}");
    assert_eq!(waiting(&counts(&jojobot, "delta").await), 2);
}

/// **A bot archived, not folded, is answered as before**: the mail lands in its
/// own box and the receipt names no fold.
#[tokio::test]
async fn mail_to_an_archived_bot_is_answered_as_before() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "marge").await;
    owning(&jojobot, "bart").await;
    jojobot
        .archive_entity(Parameters(
            crate::memory::archive_entity::ArchiveEntityArgs {
                handle: "bot:marge".into(),
                reason: "retired".into(),
                restore: None,
                sid: Some(as_bot(&jojobot, "bart")),
                request_id: None,
            },
        ))
        .await
        .expect("archive ok");

    let receipt = send(&jojobot, "bot:marge", "bart", "still here?").await;
    assert_eq!(receipt["mailbox"], "marge", "{receipt}");
    assert!(receipt.get("folded").is_none(), "{receipt}");
    assert_eq!(
        jojobot
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .expect("list ok")
            .iter()
            .filter(|bot| bot.merged_into.is_some())
            .count(),
        0
    );
}

/// **A chain of folds is followed to its end.**
#[tokio::test]
async fn mail_to_a_bot_folded_twice_reaches_the_last_survivor() {
    let jojobot = mailbox_handler();
    for bot in ["lisa", "marge", "delta", "bart"] {
        owning(&jojobot, bot).await;
    }
    fold(&jojobot, "bot:lisa", "bot:marge").await;
    fold(&jojobot, "bot:marge", "bot:delta").await;

    let receipt = send(&jojobot, "bot:lisa", "bart", "down the line").await;
    assert_eq!(receipt["mailbox"], "delta", "{receipt}");
    assert_eq!(receipt["folded"]["folded_into"], "bot:delta", "{receipt}");
}

/// Paired handlers over a store whose bots listing the second cannot read.
fn over_a_store_that_can_go_down() -> (Jojobot, Jojobot) {
    healthy_and_down(Down::ListingKind("bot"))
}

/// **A bots listing that fails refuses the post with retry, and writes nothing**
/// (rule 130). It is never read as a bot that was not folded, which would post
/// to the folded bot's own box. Paired with the same post once the listing reads.
#[tokio::test]
async fn a_post_over_an_unreadable_bots_listing_is_refused_with_retry_and_writes_nothing() {
    let (jojobot, blind) = over_a_store_that_can_go_down();
    for bot in ["lisa", "delta", "bart"] {
        owning(&jojobot, bot).await;
    }
    fold(&jojobot, "bot:lisa", "bot:delta").await;

    let refused = blind
        .post_message(Parameters(PostMessageArgs {
            request_id: None,
            to: "bot:lisa".into(),
            sid: as_bot(&jojobot, "bart"),
            subject: None,
            body: "the damper is stuck".into(),
            in_reply_to: None,
            supersedes: None,
        }))
        .await
        .expect_err("a store that cannot be read is an error to retry");
    assert_eq!(
        refused
            .data
            .as_ref()
            .and_then(|data| data["fix_by"].as_str()),
        Some("retry"),
        "{refused:?}"
    );
    assert_eq!(waiting(&counts(&jojobot, "delta").await), 0);
    assert_eq!(waiting(&counts(&jojobot, "lisa").await), 0);

    let landed = send(&jojobot, "bot:lisa", "bart", "the damper is stuck").await;
    assert_eq!(landed["mailbox"], "delta", "{landed}");
}

/// **The bots a store holds, as the store serves them**, for the walk to follow.
async fn the_bots_of(jojobot: &Jojobot) -> Vec<jojobot_domain::memory::Entity> {
    jojobot
        .memory
        .list_entities(Some(EntityKind::BOT))
        .await
        .expect("list ok")
}

/// **A chain of folds that leads back to where it began is stored damage**, and
/// it is refused, never followed to whichever bot the walk stopped on. The loop
/// is planted on the bots a real store served; the unplanted folds are the
/// positive it rests on.
#[tokio::test]
async fn folds_that_lead_back_to_where_they_began_are_damage_not_an_addressee() {
    let jojobot = mailbox_handler();
    for bot in ["lisa", "marge"] {
        owning(&jojobot, bot).await;
    }
    fold(&jojobot, "bot:lisa", "bot:marge").await;
    let mut bots = the_bots_of(&jojobot).await;
    let lisa = EntityId("bot:lisa".into());

    let healthy = super::folded::follow(&lisa, &bots)
        .expect("the folds read")
        .expect("lisa was folded");
    assert_eq!(healthy.survivor.as_str(), "bot:marge");

    for bot in &mut bots {
        if bot.id.as_str() == "bot:marge" {
            bot.merged_into = Some(lisa.clone());
        }
    }
    let looped = super::folded::follow(&lisa, &bots);
    assert!(
        matches!(looped, Err(MemoryError::StoredDamage { .. })),
        "{:?}",
        looped.map(|fold| fold.map(|fold| fold.survivor))
    );
}

/// **A survivor that is no bot is damage too**, not a handle to post to.
#[tokio::test]
async fn a_fold_into_a_handle_that_is_no_bot_is_damage_not_an_addressee() {
    let jojobot = mailbox_handler();
    for bot in ["lisa", "marge"] {
        owning(&jojobot, bot).await;
    }
    fold(&jojobot, "bot:lisa", "bot:marge").await;
    let mut bots = the_bots_of(&jojobot).await;
    bots.retain(|bot| bot.id.as_str() != "bot:marge");
    let refused = super::folded::follow(&EntityId("bot:lisa".into()), &bots)
        .map(|fold| fold.map(|f| f.survivor));
    assert!(
        matches!(refused, Err(MemoryError::StoredDamage { .. })),
        "{refused:?}"
    );
}

/// **A forwarding row from before merge events follows a renamed survivor.**
/// A modern merge cannot create this fixture: it always writes an event.
#[tokio::test]
async fn a_fold_from_before_events_reaches_the_survivor_after_its_rename() {
    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over(memory.clone());
    for bot in ["marge", "bart"] {
        owning(&jojobot, bot).await;
    }
    memory.past_the_guard(Entity {
        id: EntityId("bot:lisa".into()),
        merged_into: Some(EntityId("bot:marge".into())),
        kind: EntityKind::BOT,
        name: "Lisa".into(),
        aliases: Vec::new(),
        folded_names: Vec::new(),
        source: "legacy fixture".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        badge: None,
        archived: None,
    });
    assert!(memory.merge_events(None).await.unwrap().is_empty());
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(
                serde_json::from_value::<crate::memory::RenameEntityArgs>(serde_json::json!({
                    "handle": "bot:marge", "to": "bot:delta", "sid": as_bot(&jojobot, "marge"),
                }))
                .unwrap(),
            ))
            .await
            .expect("the survivor is renamed"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");
    assert!(memory.merge_events(None).await.unwrap().is_empty());
    let bots = memory.list_entities(Some(EntityKind::BOT)).await.unwrap();
    assert_eq!(
        bots.iter()
            .find(|bot| bot.id.as_str() == "bot:lisa")
            .unwrap()
            .merged_into,
        Some(EntityId("bot:delta".into())),
        "the forwarding row serves the survivor's current handle",
    );
    let receipt = send(&jojobot, "bot:lisa", "bart", "after the rename").await;
    assert_eq!(receipt["mailbox"], "delta", "{receipt}");
    assert_eq!(receipt["folded"]["folded_into"], "bot:delta", "{receipt}");
    assert_eq!(waiting(&counts(&jojobot, "delta").await), 1);
    let direct = send(&jojobot, "bot:delta", "bart", "directly").await;
    assert!(direct.get("folded").is_none(), "{direct}");
    assert_eq!(waiting(&counts(&jojobot, "delta").await), 2);
}
