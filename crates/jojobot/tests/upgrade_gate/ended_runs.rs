//! **A run an older build left ended is not exchanged by its old name on the
//! head**, one test for each recording and each way a run ends, kept in a file of
//! its own beside the gate.

use super::*;

/// **A run an older build began, that has since ended, is not exchanged by its
/// old name.** The recorded holder is a run with no key. Here its state is set to
/// the two ways a run ends, which an older build left in a store as a row with
/// no key, and the current binary boots over each. The old name is turned away,
/// hands out no key, and the boot says nothing the gate treats as a warning.
macro_rules! an_ended_run_is_not_exchanged {
    ($($case:ident => ($fixture:expr, $deploy:literal, $ended:literal)),+ $(,)?) => {
        $(
            #[tokio::test]
            async fn $case() {
                an_ended_run_is_not_exchanged_by_its_old_name($fixture, $deploy, $ended).await;
            }
        )+
    };
}

// **One case for each recording and each way a run ends**, so the runner takes
// them side by side and a failure names the pair.
an_ended_run_is_not_exchanged! {
    a_wrapped_run_of_the_deploy_3_store_is_not_exchanged_by_its_old_name => (DEPLOY_3_FIXTURE_DIR, "d3", "wrapped"),
    an_abandoned_run_of_the_deploy_3_store_is_not_exchanged_by_its_old_name => (DEPLOY_3_FIXTURE_DIR, "d3", "abandoned"),
    a_wrapped_run_of_the_deploy_4_store_is_not_exchanged_by_its_old_name => (DEPLOY_4_FIXTURE_DIR, "d4", "wrapped"),
    an_abandoned_run_of_the_deploy_4_store_is_not_exchanged_by_its_old_name => (DEPLOY_4_FIXTURE_DIR, "d4", "abandoned"),
}

async fn an_ended_run_is_not_exchanged_by_its_old_name(
    fixture_dir: &str,
    deploy: &str,
    ended: &str,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, &format!("ended-{ended}-{deploy}")).await;
    let role_holder_file = format!("{fixture_dir}/role_holder.txt");
    let role_holder = std::fs::read_to_string(&role_holder_file)
        .unwrap_or_else(|e| panic!("reading {role_holder_file}: {e}"))
        .trim()
        .to_string();
    let mut store = open_the_store(&state_dir, store_port).await;
    let changed = sqlx::query("UPDATE session SET state = ? WHERE sid = ? OR id = ?")
        .bind(ended)
        .bind(&role_holder)
        .bind(&role_holder)
        .execute(store.pool())
        .await
        .expect("the recorded run is ended")
        .rows_affected();
    assert_eq!(
        changed, 1,
        "recorded at {git_ref}: the recorded holder is one row of the board"
    );
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call(
            "journal",
            json!({"entry": "the old name of an ended run", "sid": role_holder}),
        )
        .await;
    surface.finish().await;
    booted.stop().await;
    let parsed: serde_json::Value = serde_json::from_str(&answer).unwrap_or_else(|_| {
        panic!("recorded at {git_ref}: {ended}: the answer is not json: {answer}")
    });
    assert!(
        parsed["status"] == "blocked" && parsed["exchanged"] != true && parsed.get("sid").is_none(),
        "recorded at {git_ref}: a run that {ended} was exchanged by its old name: {answer}"
    );
    drop(scratch);
}
