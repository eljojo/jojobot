//! Every migration added after the deployed store needs both interrupted starts.
use super::*;

macro_rules! deaths_beyond_the_last_release {
    ($($case:ident => $version:literal),* $(,)?) => {
        const DEATH_VERSIONS: &[&str] = &[$($version),*];
        $(mod $case {
            use super::*;
            use jojobot_adapters::dolt::migrate::Interruption;

            #[tokio::test]
            async fn the_marker_stands_and_the_change_has_not_landed() {
                a_death_inside_one_migration(Interruption::BeforeTheChange).await;
            }

            #[tokio::test]
            async fn the_change_landed_and_the_ledger_row_is_missing() {
                a_death_inside_one_migration(Interruption::AfterTheChange).await;
            }

            async fn a_death_inside_one_migration(how: Interruption) {
                use jojobot_adapters::dolt::migrate::{leave_interrupted, shipped_versions};
                let version = $version;
                assert!(shipped_versions().contains(&version), "unshipped death case: {version}");
                let fixture = fixture_dir();
                let label = format!("interrupted-{version}-{how:?}");
                let restored = restore_a_fixture(&fixture, &label).await;
                let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
                leave_interrupted(store.pool(), version, how)
                    .await
                    .unwrap_or_else(|e| panic!("leaving {version} interrupted ({how:?}): {e}"));
                let begun: Vec<String> = sqlx::query_scalar(
                    "SELECT version FROM schema_migration_begun",
                )
                .fetch_all(store.pool())
                .await
                .expect("the marker table reads");
                assert!(begun.iter().any(|seen| seen == version), "{version} {how:?}: {begun:?}");
                let ledger: Vec<String> = sqlx::query_scalar(
                    "SELECT version FROM schema_migration",
                )
                .fetch_all(store.pool())
                .await
                .expect("the ledger reads");
                assert!(!ledger.iter().any(|seen| seen == version), "{version} {how:?}: already recorded");
                store.stop().await;
                boots_on_a_restored_store(&fixture, restored).await;
            }
        })*
    };
}

// deploy-9.2.3 already holds every migration this head ships. Add a named pair
// here with each migration introduced beyond that cached store's ledger.
deaths_beyond_the_last_release!();

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
