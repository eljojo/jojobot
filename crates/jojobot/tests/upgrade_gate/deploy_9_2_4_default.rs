//! **The shipped default of the link rule, proved through the real binary.** Every
//! other gate boot names the rule in `JOJOBOT_LINK_RULE`, so none of them can tell
//! a build whose default is on from one whose default is log-only. This one boots
//! with the variable not set at all.
use super::super::*;

/// Whether the marking pass has finished in the store, read with no binary over it.
async fn links_are_marked(state_dir: &std::path::Path, store_port: u16) -> bool {
    let mut store = open_the_store(state_dir, store_port).await;
    let markers: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass")
        .fetch_all(store.pool())
        .await
        .expect("the markers read");
    store.stop().await;
    markers
        .iter()
        .any(|name| name == "field value links marked")
}

/// **Boot the current binary with `JOJOBOT_LINK_RULE` removed from its environment**
/// (not blank: a blank word and no variable are different inputs to the parser),
/// and keep every line it logged up to the end of its release notes pass.
async fn boot_with_the_variable_unset(state_dir: &std::path::Path, store_port: u16) -> Booted {
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let mut child = tokio::process::Command::new(&binary)
        .env_remove("JOJOBOT_LINK_RULE")
        .env("STATE_DIRECTORY", state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", "127.0.0.1:0")
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the current binary runs");
    let serving = jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the current binary {failure} with the rule variable unset"));
    let http_port = serving.port();
    let mut seen = serving.seen;
    let mut lines = serving.stdout;
    // The release notes pass runs after the serving line; the boot is over when it
    // says so, and the field-link step is long done by then.
    let passed = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            let done = line.contains("release notes pass finished");
            seen.push(line);
            if done {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        passed,
        "the boot never finished sending the release notes: {seen:?}"
    );
    // Keep reading: a pipe nobody reads fills, and the binary blocks in a write.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Booted {
        child,
        state_dir: state_dir.to_path_buf(),
        http_port,
        log: seen,
    }
}

/// **With `JOJOBOT_LINK_RULE` unset the real binary runs the link rule on.** The
/// recorded store is the last deployed release's, which ran the rule log-only and
/// so holds no marking marker. The boot says the rule is on, logs no log-only
/// summary, and marks the store; a build whose default were log-only would log the
/// summary and leave the marker out.
#[tokio::test]
async fn a_boot_with_the_link_rule_variable_unset_runs_the_rule_on() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "link-rule-variable-unset").await;
    assert!(
        !links_are_marked(&state_dir, store_port).await,
        "recorded at {git_ref}: the recorded store is already marked, so the case proves nothing"
    );

    let booted = boot_with_the_variable_unset(&state_dir, store_port).await;
    let log = booted.log.clone();
    booted.stop().await;

    assert!(
        log.iter().any(|line| line.contains("link rule (on)")),
        "recorded at {git_ref}: the boot with the variable unset does not say the rule is on: {log:?}"
    );
    assert!(
        !log.iter().any(|line| line.contains("link rule (log-only)")),
        "recorded at {git_ref}: the boot with the variable unset ran the rule log-only: {log:?}"
    );
    assert!(
        links_are_marked(&state_dir, store_port).await,
        "recorded at {git_ref}: the boot with the variable unset did not mark the store"
    );
    drop(scratch);
}
