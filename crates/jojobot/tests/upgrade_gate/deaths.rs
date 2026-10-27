//! Every migration added after the deployed store needs both interrupted starts.
use super::*;
use jojobot_adapters::dolt::migrate::Interruption;

macro_rules! deaths_beyond_the_last_release {
    ($($case:ident => $version:literal),* $(,)?) => {
        const DEATH_VERSIONS: &[&str] = &[$($version),*];
        $(mod $case {
            use super::*;

            #[tokio::test]
            async fn the_marker_stands_and_the_change_has_not_landed() {
                a_death_inside_one_migration($version, Interruption::BeforeTheChange).await;
            }

            #[tokio::test]
            async fn the_change_landed_and_the_ledger_row_is_missing() {
                a_death_inside_one_migration($version, Interruption::AfterTheChange).await;
            }

        })*
    };
}

async fn a_death_inside_one_migration(version: &str, how: Interruption) {
    use jojobot_adapters::dolt::migrate::{leave_interrupted, shipped_versions};
    assert!(
        shipped_versions().contains(&version),
        "unshipped death case: {version}"
    );
    let fixture = fixture_dir();
    let label = format!("interrupted-{version}-{how:?}");
    let restored = restore_a_fixture(&fixture, &label).await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    leave_interrupted(store.pool(), version, how)
        .await
        .unwrap_or_else(|e| panic!("leaving {version} interrupted ({how:?}): {e}"));
    let begun: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration_begun")
        .fetch_all(store.pool())
        .await
        .expect("the marker table reads");
    assert!(
        begun.iter().any(|seen| seen == version),
        "{version} {how:?}: {begun:?}"
    );
    let ledger: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(store.pool())
        .await
        .expect("the ledger reads");
    assert!(
        !ledger.iter().any(|seen| seen == version),
        "{version} {how:?}: already recorded"
    );
    store.stop().await;
    boots_on_a_restored_store(&fixture, restored).await;
}

// Every migration beyond the recorded ledger gets both interrupted starts.
deaths_beyond_the_last_release!(
    tool_call_refusal_code => "0112_tool_call_refusal_code",
    run_call_refusal_code => "0113_run_call_refusal_code",
    tool_call_code_key_drop => "0114_tool_call_code_key_drop",
    tool_call_code_key_add => "0115_tool_call_code_key_add",
    run_call_code_key_drop => "0116_run_call_code_key_drop",
    run_call_code_key_add => "0117_run_call_code_key_add",
);

#[tokio::test]
async fn every_migration_added_since_the_last_release_has_a_death_case() {
    use jojobot_adapters::dolt::migrate::shipped_versions;
    let restored = restore_a_fixture(&fixture_dir(), "interrupted-ledger").await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(store.pool())
        .await
        .expect("the cached recording's ledger reads");
    store.stop().await;
    let beyond: Vec<_> = shipped_versions()
        .into_iter()
        .filter(|version| !recorded.iter().any(|seen| seen == version))
        .collect();
    assert_eq!(
        beyond, DEATH_VERSIONS,
        "each migration beyond the last-release ledger needs both death states: \
         add the missing version's named pair; remove pairs the deployed store already holds"
    );
}
