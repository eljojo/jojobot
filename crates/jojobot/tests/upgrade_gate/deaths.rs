//! Every migration added after the deployed store needs both interrupted starts.
use super::*;
use jojobot_adapters::dolt::migrate::Interruption;
use jojobot_domain::log::{
    Append, LogError, LogStore, Position, SealedBatch, ValidatedSnapshot, Validity,
};

/// The old publisher's SQL and eleven-member encoder at carry 4a5c332b1,
/// dolt_log.rs:423-427 and 458-475, fed by the real snapshot producer.
/// Fixture fidelity cannot redden 0126: its predicate reads only the slot prefix.
struct OldPublisher {
    log: BeforeSnapshots,
    pool: sqlx::MySqlPool,
}

#[async_trait::async_trait]
impl LogStore for OldPublisher {
    async fn validity(&self) -> Result<Validity, LogError> {
        self.log.validity().await
    }

    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        self.log.append(head, batch).await
    }

    async fn read_from(&self, after: Position, limit: usize) -> Result<Vec<SealedBatch>, LogError> {
        self.log.read_from(after, limit).await
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        Ok(None)
    }

    async fn store_snapshot(&self, s: &ValidatedSnapshot) -> Result<(), LogError> {
        let bytes = serde_json::to_vec(&(
            s.snapshot_format_version,
            s.projection_version,
            &s.log_identity,
            s.representation_epoch,
            s.erasure_epoch,
            s.covered_position,
            s.batch_checksum,
            s.created_at,
            s.checksum,
            s.serialized_length,
            &s.blob,
        ))
        .map_err(|e| LogError::Store(e.to_string()))?;
        sqlx::query("REPLACE INTO event_log (slot, first_position, last_position, payload) VALUES (?, NULL, ?, ?)")
            .bind(format!("snapshot:{}", s.projection_version))
            .bind(s.covered_position.0)
            .bind(bytes)
            .execute(&self.pool)
            .await
            .map_err(|e| LogError::Store(e.to_string()))?;
        Ok(())
    }
}

type LogRow = (String, Option<u64>, u64, Vec<u8>);

pub(super) struct RetainedLog {
    validity: LogRow,
    batch: LogRow,
    identity: String,
}

impl RetainedLog {
    async fn read(pool: &sqlx::MySqlPool) -> Self {
        let validity: LogRow = sqlx::query_as(
            "SELECT slot, first_position, last_position, payload FROM event_log WHERE slot = 'validity'",
        )
        .fetch_one(pool)
        .await
        .expect("the original validity row reads before interruption");
        let decoded: jojobot_domain::log::Validity =
            serde_json::from_slice(&validity.3).expect("the original validity bytes decode");
        let batch: LogRow = sqlx::query_as(
            "SELECT slot, first_position, last_position, payload FROM event_log
             WHERE first_position IS NOT NULL ORDER BY first_position LIMIT 1",
        )
        .fetch_one(pool)
        .await
        .expect("a real batch from the restore's teaching writer exists before interruption");
        assert!(!decoded.log_identity.is_empty());
        assert!(!batch.3.is_empty());
        Self {
            validity,
            batch,
            identity: decoded.log_identity,
        }
    }

    pub(super) async fn assert_retained(&self, pool: &sqlx::MySqlPool) {
        for (label, original) in [("validity", &self.validity), ("batch", &self.batch)] {
            let found: Option<LogRow> = sqlx::query_as(
                "SELECT slot, first_position, last_position, payload FROM event_log WHERE slot = ?",
            )
            .bind(&original.0)
            .fetch_optional(pool)
            .await
            .expect("the retained row reads");
            assert!(
                found.as_ref() == Some(original),
                "original {label} row and bytes changed"
            );
            if label == "validity" {
                let decoded: jojobot_domain::log::Validity =
                    serde_json::from_slice(&found.unwrap().3).unwrap();
                assert_eq!(
                    decoded.log_identity, self.identity,
                    "the original log identity changed"
                );
            }
        }
    }
}

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

// Each named pair restores the deployed fixture and interrupts its new migration.
// Once the deployed ledger holds every migration, the pairs below are removed and
// `deaths_beyond_the_last_release!` is invoked with none; nothing then calls this
// function, and the allow keeps that release from failing on dead code.
#[allow(dead_code)]
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
    let retained = if version == "0126_event_log_snapshots_leave" {
        Some(RetainedLog::read(store.pool()).await)
    } else {
        None
    };
    if version == "0126_event_log_snapshots_leave" {
        // **The migration's own target, in the shape the build before wrote it**: the restore
        // removes recorded snapshots before seeding real teaching batches, so without this the migration has nothing
        // to remove in either state and a no-op or a wrong predicate would pass. The boot
        // that follows must remove it; `boots_on_a_restored_store` reads that it did.
        let publisher = OldPublisher {
            log: BeforeSnapshots(
                jojobot_adapters::writer_log::WriterLog::open(
                    store.pool().clone(),
                    jojobot_adapters::writer::Writer::default(),
                )
                .await
                .expect("the original log opens"),
            ),
            pool: store.pool().clone(),
        };
        jojobot_adapters::log_snapshot::take_snapshot(&publisher, jiff::Timestamp::UNIX_EPOCH)
            .await
            .expect("the old publisher writes a snapshot of the real log");
        drop(publisher);
        let held: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM event_log WHERE slot LIKE 'snapshot:%'")
                .fetch_one(store.pool())
                .await
                .expect("the planted row counts");
        assert_eq!(held, 1, "the migration's target is planted");
    }
    leave_interrupted(store.pool(), version, how)
        .await
        .unwrap_or_else(|e| panic!("leaving {version} interrupted ({how:?}): {e}"));
    if let Some(retained) = &retained {
        retained.assert_retained(store.pool()).await;
    }
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
    boots_on_a_restored_store_with_retained_log(&fixture, restored, retained.as_ref()).await;
}

// Add a named pair here with each migration introduced beyond the cached
// store's ledger. The deploy-9.3.3 store holds every migration shipped so far.
deaths_beyond_the_last_release!(
    log_snapshot => "0125_log_snapshot",
    event_log_snapshots_leave => "0126_event_log_snapshots_leave",
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
