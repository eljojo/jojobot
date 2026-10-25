//! **The fixture recorded from the build deploy 9.1 shipped (b1409450)**, the
//! build the operator runs now. It is the first store recorded after 0108, so
//! a migration numbered past it (0109 on) is proved here against the store it
//! lands on in production.

use super::*;

/// The fixture recorded from the build deploy 9.1 shipped.
pub(super) const DEPLOY_9_1_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy9_1";

#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_9_1_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_9_1_FIXTURE_DIR, "boots-deploy9-1").await;
}

/// **A bot booted on the upgraded store is not told again a note its ledger
/// holds.** The 9.1 store holds `release_notes_told` on `bot:assistant`. The boot
/// sends each bot the shipped notes its ledger does not hold, so a boot that lost
/// the ledger would list a held note in a new message from the system sender.
/// The case reads the ids a message lists off its last line and never the text of
/// a note.
#[tokio::test]
async fn a_bot_is_not_told_again_a_note_it_held_before_the_upgrade() {
    let restored = restore_a_fixture(DEPLOY_9_1_FIXTURE_DIR, "told-notes-not-resent").await;
    let held: Vec<String> = restored
        .held_before
        .iter()
        .find(|(handle, key, _)| handle == "bot:assistant" && key == "release_notes_told")
        .map(|(_, _, ids)| ids.split(',').map(|id| id.trim().to_string()).collect())
        .expect("the recording holds a ledger of told notes on bot:assistant");
    assert!(!held.is_empty(), "the ledger holds at least one note");

    let before = system_messages_in_box(&restored, "assistant").await;
    boot_current(&restored.state_dir, restored.store_port, &restored.git_ref)
        .await
        .stop()
        .await;
    let after = system_messages_in_box(&restored, "assistant").await;

    let listed_now: Vec<String> = after
        .iter()
        .filter(|(id, _)| !before.iter().any(|(was, _)| was == id))
        .flat_map(|(_, body)| {
            body.trim_end()
                .lines()
                .last()
                .and_then(|line| line.strip_prefix("Notes: "))
                .map(|ids| {
                    ids.split(',')
                        .map(|id| id.trim().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    let again: Vec<&String> = listed_now.iter().filter(|id| held.contains(id)).collect();
    assert!(
        again.is_empty(),
        "recorded at {}: bot:assistant held {held:?} and the boot told it {again:?} again",
        restored.git_ref
    );
}

/// The system sender's messages in one box, as `(id, body)`, read from the store
/// on its own with no binary over it.
async fn system_messages_in_box(restored: &Restored, mailbox: &str) -> Vec<(String, String)> {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, body FROM message WHERE mailbox = ? AND sender = 'jojobot'")
            .bind(mailbox)
            .fetch_all(store.pool())
            .await
            .expect("the box's system messages read");
    store.stop().await;
    rows
}

/// **A boot with the link rule on, over the store the deploy 9.1 binary filled.**
/// The repairs of stored rows run first, the marking pass runs once and is marked
/// finished, and a second boot changes no row. The gate's ordinary boot cases
/// use the default, log-only; these cases run the marking over the recordings.
#[tokio::test]
async fn the_current_binary_boots_with_the_link_rule_on_over_the_deploy_9_1_store() {
    the_link_rule_preserves_a_recorded_store(
        DEPLOY_9_1_FIXTURE_DIR,
        "boots-deploy9-1-link-rule-on",
    )
    .await;
}

#[tokio::test]
async fn the_link_rule_on_preserves_every_other_recorded_store() {
    for (fixture, label) in [
        (DEPLOY_3_FIXTURE_DIR, "boots-deploy3-link-rule-on"),
        (DEPLOY_4_FIXTURE_DIR, "boots-deploy4-link-rule-on"),
        (DEPLOY_5_FIXTURE_DIR, "boots-deploy5-link-rule-on"),
        (DEPLOY_6_FIXTURE_DIR, "boots-deploy6-link-rule-on"),
        (DEPLOY_8_FIXTURE_DIR, "boots-deploy8-link-rule-on"),
        (
            deploy_8_1::DEPLOY_8_1_FIXTURE_DIR,
            "boots-deploy8-1-link-rule-on",
        ),
        (
            deploy_9_2_1::DEPLOY_9_2_1_FIXTURE_DIR,
            "boots-deploy9-2-1-link-rule-on",
        ),
    ] {
        the_link_rule_preserves_a_recorded_store(fixture, label).await;
    }
}

async fn the_link_rule_preserves_a_recorded_store(fixture: &str, label: &str) {
    let restored = restore_a_fixture(fixture, label).await;
    let on = [("JOJOBOT_LINK_RULE", "on".to_string())];
    let first = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &["FIELD VALUE LEFT AS TEXT"],
        &on,
    )
    .await;
    first.stop().await;
    let after_first = dump_store(&restored.state_dir).await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let markers: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the markers read");
    store.stop().await;
    assert!(
        markers
            .iter()
            .any(|name| name == "field value links marked"),
        "recorded at {}: the marking pass finished: {markers:?}",
        restored.git_ref
    );
    let second = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &["FIELD VALUE LEFT AS TEXT"],
        &on,
    )
    .await;
    second.stop().await;
    assert!(
        dump_store(&restored.state_dir).await == after_first,
        "recorded at {}: a second boot with the rule on changed the store",
        restored.git_ref
    );
    let third = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &["FIELD VALUE LEFT AS TEXT"],
        &on,
    )
    .await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", third.http_port))
        .await
        .unwrap();
    assert_every_thing_holds_what_it_held(&surface, &restored.git_ref, &restored.held_before).await;
    surface.finish().await;
    third.stop().await;
}
