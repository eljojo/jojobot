//! **A deploy that dies partway through a migration**, one test for each migration
//! beyond a recording and each state a death leaves, kept in a file of its own
//! beside the gate.

use super::*;
use jojobot_adapters::dolt::migrate::Interruption;

/// **A deploy that dies partway through a migration restarts cleanly on the
/// store production holds.** For every migration the head ships beyond what a
/// recording's ledger holds, the head boots over that recording in both states a
/// death inside the migration leaves: the marker committed and the change not
/// landed, and the change landed and the ledger row missing. Each boot is held to
/// the bar of a clean boot (no warning, no error, a second boot that changes
/// nothing, every recorded record read back), and the ledger ends whole with no
/// marker left.
///
/// **Each migration is a case of its own**, so the runner takes them side by
/// side, and a failure names the migration. The list of cases is written out, and
/// the two cases after the lists hold it to the build's own: a migration added
/// beyond a recording with no case here turns its list's case red, naming it.
macro_rules! a_death_inside_each_migration {
    ($walked:ident, $fixture:expr, $name:expr, $label:expr, {
        $($case:ident => $version:literal),+ $(,)?
    }) => {
        /// The migrations these cases walk, in the order the build applies them.
        const $walked: &[&str] = &[$($version),+];
        $(
            mod $case {
                use super::*;

                #[tokio::test]
                async fn the_marker_stands_and_the_change_has_not_landed() {
                    a_death_inside_one_migration(
                        $fixture,
                        $name,
                        $label,
                        $version,
                        Interruption::BeforeTheChange,
                    )
                    .await;
                }

                #[tokio::test]
                async fn the_change_landed_and_the_ledger_row_is_missing() {
                    a_death_inside_one_migration(
                        $fixture,
                        $name,
                        $label,
                        $version,
                        Interruption::AfterTheChange,
                    )
                    .await;
                }
            }
        )+
    };
}

a_death_inside_each_migration!(DEATHS_BEYOND_DEPLOY_6, DEPLOY_6_FIXTURE_DIR, "deploy 6", "interrupted", {
    deploy_6_a_boot_after_a_death_inside_0094_reads_back_as_a_clean_boot => "0094_session_retired_key_hash",
    deploy_6_a_boot_after_a_death_inside_0095_reads_back_as_a_clean_boot => "0095_session_retired_key_hash_index",
    deploy_6_a_boot_after_a_death_inside_0096_reads_back_as_a_clean_boot => "0096_session_retired_at",
    deploy_6_a_boot_after_a_death_inside_0097_reads_back_as_a_clean_boot => "0097_session_retired_by",
    deploy_6_a_boot_after_a_death_inside_0098_reads_back_as_a_clean_boot => "0098_session_taken_at",
    deploy_6_a_boot_after_a_death_inside_0099_reads_back_as_a_clean_boot => "0099_session_key",
    deploy_6_a_boot_after_a_death_inside_0100_reads_back_as_a_clean_boot => "0100_request_part",
    deploy_6_a_boot_after_a_death_inside_0101_reads_back_as_a_clean_boot => "0101_tool_call_count",
    deploy_6_a_boot_after_a_death_inside_0102_reads_back_as_a_clean_boot => "0102_served_day",
    deploy_6_a_boot_after_a_death_inside_0103_reads_back_as_a_clean_boot => "0103_run_call_count",
    deploy_6_a_boot_after_a_death_inside_0104_reads_back_as_a_clean_boot => "0104_app_write",
    deploy_6_a_boot_after_a_death_inside_0105_reads_back_as_a_clean_boot => "0105_fact_app_stamp",
    deploy_6_a_boot_after_a_death_inside_0106_reads_back_as_a_clean_boot => "0106_fact_write_app_stamp",
    deploy_6_a_boot_after_a_death_inside_0107_reads_back_as_a_clean_boot => "0107_trip_kind_takes_its_name",
    deploy_6_a_boot_after_a_death_inside_0108_reads_back_as_a_clean_boot => "0108_session_displaced_by",
    deploy_6_a_boot_after_a_death_inside_0109_reads_back_as_a_clean_boot => "0109_fact_write_bot",
    deploy_6_a_boot_after_a_death_inside_0110_reads_back_as_a_clean_boot => "0110_merge_event_by_survivor",
    deploy_6_a_boot_after_a_death_inside_0111_reads_back_as_a_clean_boot => "0111_type_field_must",
});

// **The same proof over the deploy 8 recording, the store production holds.**
// The migrations beyond its ledger are the ones the next deploy applies.
a_death_inside_each_migration!(DEATHS_BEYOND_DEPLOY_8, DEPLOY_8_FIXTURE_DIR, "deploy 8", "interrupted-deploy8", {
    deploy_8_a_boot_after_a_death_inside_0103_reads_back_as_a_clean_boot => "0103_run_call_count",
    deploy_8_a_boot_after_a_death_inside_0104_reads_back_as_a_clean_boot => "0104_app_write",
    deploy_8_a_boot_after_a_death_inside_0105_reads_back_as_a_clean_boot => "0105_fact_app_stamp",
    deploy_8_a_boot_after_a_death_inside_0106_reads_back_as_a_clean_boot => "0106_fact_write_app_stamp",
    deploy_8_a_boot_after_a_death_inside_0107_reads_back_as_a_clean_boot => "0107_trip_kind_takes_its_name",
    deploy_8_a_boot_after_a_death_inside_0108_reads_back_as_a_clean_boot => "0108_session_displaced_by",
    deploy_8_a_boot_after_a_death_inside_0109_reads_back_as_a_clean_boot => "0109_fact_write_bot",
    deploy_8_a_boot_after_a_death_inside_0110_reads_back_as_a_clean_boot => "0110_merge_event_by_survivor",
    deploy_8_a_boot_after_a_death_inside_0111_reads_back_as_a_clean_boot => "0111_type_field_must",
});

/// **Every migration the head ships beyond the deploy 6 recording has a case.**
#[tokio::test]
async fn every_migration_beyond_the_deploy_6_recording_has_a_death_case() {
    the_death_cases_cover_every_migration_beyond(
        DEPLOY_6_FIXTURE_DIR,
        "deploy 6",
        "interrupted-ledger",
        DEATHS_BEYOND_DEPLOY_6,
    )
    .await;
}

/// **Every migration the head ships beyond the deploy 8 recording has a case.**
#[tokio::test]
async fn every_migration_beyond_the_deploy_8_recording_has_a_death_case() {
    the_death_cases_cover_every_migration_beyond(
        DEPLOY_8_FIXTURE_DIR,
        "deploy 8",
        "interrupted-deploy8-ledger",
        DEATHS_BEYOND_DEPLOY_8,
    )
    .await;
}

/// The migrations the head ships beyond a recording's ledger are exactly the ones
/// that have a death case, in the order the build applies them. The list is the
/// build's own, so a migration added later with no case turns this red and names
/// the migration.
async fn the_death_cases_cover_every_migration_beyond(
    fixture_dir: &str,
    name: &str,
    ledger_label: &str,
    cases: &[&str],
) {
    use jojobot_adapters::dolt::migrate::shipped_versions;

    let recorded = recorded_ledger(fixture_dir, ledger_label).await;
    let beyond: Vec<&str> = shipped_versions()
        .iter()
        .copied()
        .filter(|version| !recorded.iter().any(|seen| seen == version))
        .collect();
    assert_eq!(
        beyond, cases,
        "the migrations the head ships beyond the {name} recording and the migrations with a \
         death case differ: add a case for each one only the left holds, and remove each one \
         only the right holds"
    );
}

/// **One migration's pass in one state.** The head boots over a restore of the recording in
/// the state `how` a death inside `version` leaves, and the boot is held to the bar
/// of a clean boot. `name` is how the recording is called in a failure, and
/// `label` is the start of the directory each boot restores into.
async fn a_death_inside_one_migration(
    fixture_dir: &str,
    name: &str,
    label: &str,
    version: &str,
    how: Interruption,
) {
    use jojobot_adapters::dolt::migrate::{leave_interrupted, shipped_versions};

    assert!(
        shipped_versions().contains(&version),
        "the {name} death case names {version}, which the build does not ship"
    );
    {
        let label = format!("{label}-{version}-{how:?}");
        let restored = restore_a_fixture(fixture_dir, &label).await;
        let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
        leave_interrupted(store.pool(), version, how)
            .await
            .unwrap_or_else(|e| panic!("leaving {version} interrupted ({how:?}): {e}"));
        // **The case built what it claims**: the marker stands and the ledger
        // does not hold the version, so a helper that did nothing cannot pass.
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
            "{version} {how:?}: the ledger already holds it"
        );
        store.stop().await;

        boots_on_a_restored_store(fixture_dir, restored).await;
    }
}

/// The versions a fixture's ledger holds, read from its dump on a store of the
/// case's own.
async fn recorded_ledger(fixture_dir: &str, label: &str) -> Vec<String> {
    let restored = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let ledger: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(store.pool())
        .await
        .expect("the recording's ledger reads");
    store.stop().await;
    ledger
}
