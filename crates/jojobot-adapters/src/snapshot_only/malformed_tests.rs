use super::tests::batch;
use super::*;
use std::sync::Arc;

use jojobot_domain::log::Position;

use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

/// **A snapshot that cannot be read does not stop the boot or the pre-build**, the
/// newest one of another build or this build's own. Replaying a log that is whole
/// is always available, so a damaged snapshot is a snapshot not used.
#[tokio::test]
async fn a_malformed_snapshot_is_not_used_and_does_not_stop_the_boot() {
    let scratch = Scratch::new("snapshot-malformed");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let head = {
        let log = WriterLog::open(pool.clone(), writer.clone()).await.unwrap();
        let first = batch("alpha", Position(0), 0, 3);
        log.append(Position(0), &first).await.unwrap();
        first.range().last
    };
    writer.drain().await;
    drop(writer);

    for slot in [
        "snapshot:7".to_string(),
        format!("snapshot:{}", crate::log_snapshot::PROJECTION_FINGERPRINT),
    ] {
        sqlx::query(
            "REPLACE INTO event_log (slot, first_position, last_position, payload) \
             VALUES (?, NULL, ?, ?)",
        )
        .bind(&slot)
        .bind(head.0)
        .bind(b"not a snapshot".to_vec())
        .execute(&pool)
        .await
        .unwrap();
        let booting = Writer::default();
        let log: Arc<dyn LogStore> = Arc::new(
            WriterLog::open(pool.clone(), booting.clone())
                .await
                .unwrap(),
        );
        let booted = crate::log_snapshot::boot(log.as_ref())
            .await
            .unwrap_or_else(|e| panic!("a malformed snapshot in {slot} stopped the boot: {e}"));
        assert_eq!(booted.position, head, "{slot}: the log was replayed whole");
        drop(log);
        booting.drain().await;
        let rebuilt = build(pool.clone(), jiff::Timestamp::UNIX_EPOCH).await;
        assert_eq!(
            rebuilt.unwrap_or_else(|e| panic!("{slot} stopped the pre-build: {e}")),
            head
        );
        sqlx::query("DELETE FROM event_log WHERE slot LIKE 'snapshot%'")
            .execute(&pool)
            .await
            .unwrap();
    }
    server.stop().await;
}
