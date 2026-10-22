//! **A post to a bot folded into another is routed by the store's own walk**
//! (rules 598 and 466), in the real store.
//!
//! The fake keeps no forwarding row of the kind an older build left, so the one
//! state in which the post's answer and the store's answer could differ is made
//! here: a fold with no merge event, whose survivor was renamed afterwards.

use super::*;
use crate::harness::{as_bot, json_of};
use crate::mailboxes::testing::{owning, send};
use crate::memory::testing::SpySearch;
use jojobot_domain::mailbox::MailboxName;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::{EntityId, Memory, Override};
use jojobot_domain::session::testing::InMemorySessions;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **lisa was folded into delta before merge events were kept, and delta was
/// renamed gamma since.** The forwarding row names the handle delta wore at the
/// fold; the store's walk reads it as the thing delta is now. A post to lisa
/// reaches gamma, and the receipt names the fold. Paired with a post to gamma
/// by its own handle, which carries no fold, in the same store.
#[tokio::test]
async fn a_fold_without_an_event_whose_survivor_was_renamed_still_routes_the_post() {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-mcp-folded-over-dolt",
    ));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store =
        jojobot_adapters::dolt::Dolt::start(&scratch.0, jojobot_adapters::testing::free_port())
            .await
            .expect("the real store comes up");
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the schema");
    jojobot_adapters::dolt::migrate::seed_kinds(store.pool())
        .await
        .expect("the kinds are seeded");
    let memory = Arc::new(jojobot_adapters::dolt::memory::DoltMemory::open(
        store.pool().clone(),
    ));
    let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let jojobot = Jojobot::new(
        memory.clone(),
        Arc::new(SpySearch::default()),
        mailboxes.clone(),
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    );
    for bot in ["lisa", "delta", "bart"] {
        owning(&jojobot, bot).await;
    }
    let landed = json_of(
        &jojobot
            .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
                request_id: None,
                duplicate: "bot:lisa".into(),
                survivor: "bot:delta".into(),
                reason: None,
                recorded_at: None,
                sid: Some(as_bot(&jojobot, "delta")),
            }))
            .await
            .expect("merge ok"),
    );
    assert_eq!(landed["merged"], "bot:lisa", "the merge landed: {landed}");

    // The state an older build left: the fold has its forwarding row and no event.
    sqlx::query(
        "DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = 'bot:lisa')",
    )
    .execute(store.pool())
    .await
    .expect("the events of the fold are forgotten");
    memory
        .rename_entity(
            &EntityId("bot:delta".into()),
            &EntityId("bot:gamma".into()),
            None,
            jiff::civil::date(2026, 10, 22),
            Override::none(),
            None,
        )
        .await
        .expect("rename_entity ok");
    // A rename today rewrites the forwarding rows that name the old handle. A
    // store an older build filled has the row as the fold left it, naming the
    // handle the survivor wore then.
    sqlx::query("UPDATE entity SET merged_into = 'bot:delta' WHERE id = 'bot:lisa'")
        .execute(store.pool())
        .await
        .expect("the forwarding row is as the fold left it");
    // The survivor's box, under the name its owner wears now.
    mailboxes
        .create_mailbox(
            &MailboxName("gamma".into()),
            &EntityId("bot:gamma".into()),
            None,
        )
        .await
        .expect("the store writes it");

    let receipt = send(&jojobot, "bot:lisa", "bart", "your turn on the damper").await;
    assert_eq!(receipt["mailbox"], "gamma", "{receipt}");
    assert_eq!(receipt["folded"]["sent_to"], "bot:lisa", "{receipt}");
    assert_eq!(receipt["folded"]["folded_into"], "bot:gamma", "{receipt}");

    let direct = send(&jojobot, "bot:gamma", "bart", "and again").await;
    assert_eq!(direct["mailbox"], "gamma", "{direct}");
    assert!(direct.get("folded").is_none(), "{direct}");
    store.stop().await;
}
