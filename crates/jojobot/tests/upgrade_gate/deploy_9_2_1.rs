//! **The fixture recorded from the build deploy 9.2.1 shipped (10097a43)**, the
//! build the operator runs now. A store that build filled is what this build
//! takes on its first boot, so every pass this build adds is proved against it.

use super::*;

/// The fixture recorded from the build deploy 9.2.1 shipped.
pub(super) const DEPLOY_9_2_1_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy9_2_1";

#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_9_2_1_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_9_2_1_FIXTURE_DIR, "boots-deploy9-2-1").await;
}

/// **Mail the 9.2.1 build left read and open is closed by the current binary in
/// one call.** The recorded box of the assistant holds a read message. The
/// current binary boots over the store, the assistant closes that message with
/// `message_ids`, and the store read with no binary over it holds it processed
/// with the note: read mail made by the old build is closed by the batch close of
/// the new one, and a message closed twice is left as it was.
#[tokio::test]
async fn read_mail_the_deploy_9_2_1_build_left_open_is_closed_by_the_batch_close() {
    let restored = restore_a_fixture(DEPLOY_9_2_1_FIXTURE_DIR, "batch-close-read-mail").await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let open: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM message WHERE mailbox = 'assistant' AND state = 'read' ORDER BY id",
    )
    .fetch_all(store.pool())
    .await
    .expect("the read mail reads");
    store.stop().await;
    assert!(
        !open.is_empty(),
        "the recording holds read mail in the assistant box: the witness needs one"
    );

    let booted = boot_current(&restored.state_dir, restored.store_port, &restored.git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let started = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await
        .expect("the assistant boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot handed back no handle: {started}"))
        .to_string();
    let closed = surface
        .must(
            "mark_processed",
            json!({"sid": sid, "message_ids": open, "notes": "closed after the upgrade"}),
        )
        .await
        .expect("the batch close answers");
    assert_eq!(closed["processed"], open.len(), "{closed}");
    let again = surface
        .must(
            "mark_processed",
            json!({"sid": sid, "message_ids": open, "notes": "closed twice"}),
        )
        .await
        .expect("a second batch close answers");
    assert_eq!(again["processed"], 0, "{again}");
    assert_eq!(
        again["already_processed"].as_array().map(Vec::len),
        Some(open.len()),
        "{again}"
    );
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let stored: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, state, notes FROM message WHERE mailbox = 'assistant' ORDER BY id",
    )
    .fetch_all(store.pool())
    .await
    .expect("the closed mail reads");
    store.stop().await;
    for id in &open {
        let held = stored
            .iter()
            .find(|(held, _, _)| held == id)
            .unwrap_or_else(|| panic!("{id} is gone: {stored:?}"));
        assert_eq!(held.1, "processed", "{held:?}");
        assert_eq!(
            held.2.as_deref(),
            Some("closed after the upgrade"),
            "the second close left the notes alone: {held:?}"
        );
    }
}
