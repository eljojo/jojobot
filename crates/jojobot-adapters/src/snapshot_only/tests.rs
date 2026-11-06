use super::*;
use std::sync::Arc;

use jojobot_domain::log::{BatchId, Position, SealedBatch};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

use crate::dolt::Dolt;
use crate::teaching_log::codec as teaching;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

pub(super) fn batch(id: &str, head: Position, first: u64, count: u64) -> SealedBatch {
    let members = (first..first + count)
        .map(|n| {
            teaching::encode(&TeachingEvent::Recorded {
                run: RunId(format!("r{n}")),
                domain: "claims".into(),
                taught_at: jiff::Timestamp::UNIX_EPOCH,
            })
        })
        .collect();
    SealedBatch::seal(
        BatchId(id.into()),
        head,
        jiff::Timestamp::UNIX_EPOCH,
        1,
        members,
    )
    .unwrap()
}

/// **The pre-build writes the snapshot of the log and nothing else.** The log is
/// filled through a writer that is gone before the pre-build runs, as the old
/// process's store is read by the new binary. Afterwards the log holds the same
/// batches, the snapshot is in the slot of this build's shape at the head, and a
/// fresh writer's boot loads it.
#[tokio::test]
async fn the_pre_build_writes_one_snapshot_at_the_head_and_appends_nothing() {
    let scratch = Scratch::new("snapshot-only");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let head = {
        let log = WriterLog::open(pool.clone(), writer.clone()).await.unwrap();
        let first = batch("alpha", Position(0), 0, 3);
        log.append(Position(0), &first).await.unwrap();
        let second = batch("beta", first.range().last, 3, 2);
        log.append(first.range().last, &second).await.unwrap();
        second.range().last
    };
    writer.drain().await;
    drop(writer);

    let before = DoltLog::unread(pool.clone())
        .read_from(Position(0), 100)
        .await
        .unwrap();
    assert!(
        DoltLog::unread(pool.clone())
            .load_snapshot()
            .await
            .unwrap()
            .is_none(),
        "the old process left no snapshot"
    );

    let covered = build(pool.clone(), jiff::Timestamp::UNIX_EPOCH)
        .await
        .expect("the pre-build runs");
    assert_eq!(covered, head);

    let reader = DoltLog::unread(pool.clone());
    let stored = reader.load_snapshot().await.unwrap().expect("a snapshot");
    assert_eq!(stored.covered_position, head);
    assert_eq!(
        stored.projection_version,
        crate::log_snapshot::PROJECTION_FINGERPRINT
    );
    assert_eq!(
        reader.read_from(Position(0), 100).await.unwrap(),
        before,
        "the pre-build appended nothing"
    );

    // A boot by a writer that opens afterwards starts from the snapshot.
    let booting = Writer::default();
    let log: Arc<dyn LogStore> = Arc::new(WriterLog::open(pool, booting.clone()).await.unwrap());
    let booted = crate::log_snapshot::boot(log.as_ref()).await.unwrap();
    assert_eq!(booted.position, head);
    drop(log);
    booting.drain().await;
    server.stop().await;
}

/// **The mode cannot append.** The log it reads is handed to code that appends,
/// and the append is refused, so no path of the replay writes a batch.
#[tokio::test]
async fn the_read_only_log_refuses_an_append() {
    let scratch = Scratch::new("snapshot-only-append");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let log = ReadOnlyLog {
        log: DoltLog::unread(pool.clone()),
        pool,
    };
    let refused = log
        .append(Position(0), &batch("alpha", Position(0), 0, 1))
        .await;
    assert!(matches!(refused, Err(LogError::Store(_))), "{refused:?}");
    server.stop().await;
}
