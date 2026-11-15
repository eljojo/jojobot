use super::*;

use crate::log_snapshot::take_snapshot;

use jojobot_domain::log::testing::InMemoryLog;
use jojobot_domain::log::{Append, BatchId, SealedBatch};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

use crate::teaching_log::codec as teaching;

const SMALL: Cadence = Cadence {
    events: 3,
    dirty_for: Duration::from_secs(60),
};

/// Append one batch of `count` teaching events and return the new head.
async fn append(log: &InMemoryLog, id: &str, count: u64) -> Position {
    let head = log
        .read_from(Position(0), 256)
        .await
        .unwrap()
        .last()
        .map_or(Position(0), |batch| batch.range().last);
    let members = (0..count)
        .map(|n| {
            teaching::encode(&TeachingEvent::Recorded {
                run: RunId(format!("{id}-{n}")),
                domain: "claims".into(),
                taught_at: jiff::Timestamp::UNIX_EPOCH,
            })
        })
        .collect();
    let batch = SealedBatch::seal(
        BatchId(id.into()),
        head,
        jiff::Timestamp::UNIX_EPOCH,
        1,
        members,
    )
    .unwrap();
    assert_eq!(
        log.append(head, &batch).await.unwrap(),
        Append::Committed(batch.range())
    );
    batch.range().last
}

fn at(second: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_second(second).unwrap()
}

/// A keeper opened as the serving boot opens it: from what a boot of the log restored.
async fn keeper(log: &InMemoryLog, cadence: Cadence) -> SnapshotKeeper {
    let report = crate::log_snapshot::boot(log).await.unwrap().report;
    SnapshotKeeper::open(
        Arc::new(log.clone()),
        cadence,
        crate::log_snapshot::credited(&report),
    )
    .await
    .unwrap()
}

/// **The decision, one row per case.** Nothing past the snapshot is never due;
/// events past it are due at the event count; the first sight of them starts a
/// clock that makes them due after the dirty window.
#[tokio::test]
async fn a_snapshot_is_due_by_events_or_by_time_and_never_when_covered() {
    let log = InMemoryLog::default();
    let keeper = keeper(&log, SMALL).await;
    let t0 = Instant::now();
    //   (head, seconds after t0, due)
    let table = [
        (0, 0, None),
        (2, 0, None),
        (2, 59, None),
        (2, 60, Some(Why::Dirty)),
        (3, 61, Some(Why::Events)),
        (0, 62, None),
    ];
    for (head, after, due) in table {
        assert_eq!(
            keeper.due(Position(head), t0 + Duration::from_secs(after)),
            due,
            "head {head} at +{after}s"
        );
    }
}

/// **A snapshot taken because it is due covers the head and restarts the count.**
/// The keeper takes it through the log's own publication, so the next boot reads
/// it, and the same head is not due again.
#[tokio::test]
async fn keep_takes_a_due_snapshot_and_is_quiet_after() {
    let log = InMemoryLog::default();
    let keeper = keeper(&log, SMALL).await;
    let t0 = Instant::now();
    let head = append(&log, "alpha", 3).await;

    let taken = keeper.keep(head, t0, at(1)).await.unwrap();
    assert_eq!(taken, Some(head));
    assert_eq!(keeper.covered(), head);
    let stored = log.load_snapshot().await.unwrap().expect("published");
    assert_eq!(stored.covered_position, head);

    assert_eq!(keeper.keep(head, t0, at(2)).await.unwrap(), None);
    assert_eq!(
        log.load_snapshot().await.unwrap().unwrap().created_at,
        at(1),
        "nothing was taken again"
    );
}

/// **A flush covers whatever the newest snapshot does not, and only that.** It is
/// the after-import and the shutdown moment: any uncovered event is enough, a
/// covered head takes nothing.
#[tokio::test]
async fn flush_snapshots_an_uncovered_head_and_leaves_a_covered_one() {
    let log = InMemoryLog::default();
    let keeper = keeper(&log, SMALL).await;
    assert_eq!(keeper.flush(Position(0), at(1)).await.unwrap(), None);
    assert!(log.load_snapshot().await.unwrap().is_none());

    let head = append(&log, "alpha", 1).await;
    assert_eq!(keeper.flush(head, at(2)).await.unwrap(), Some(head));
    assert_eq!(keeper.flush(head, at(3)).await.unwrap(), None);
    assert_eq!(
        log.load_snapshot().await.unwrap().unwrap().created_at,
        at(2)
    );
}

/// **A keeper starts from the snapshot the log already holds**, so a restart does
/// not take a snapshot of a head the last process had covered.
#[tokio::test]
async fn a_new_keeper_starts_from_the_stored_snapshot() {
    let log = InMemoryLog::default();
    let head = append(&log, "alpha", 2).await;
    keeper(&log, SMALL).await.flush(head, at(1)).await.unwrap();

    let restarted = keeper(&log, SMALL).await;
    assert_eq!(restarted.covered(), head);
    assert_eq!(restarted.flush(head, at(2)).await.unwrap(), None);
}

/// **A snapshot of another build covers nothing for this build.** The log hands
/// back the newest foreign snapshot when this build has none, so the keeper must
/// not count its position: it would publish nothing and every restart would
/// replay the log. The envelope is a valid one of another projection version.
#[tokio::test]
async fn a_foreign_snapshot_does_not_suppress_this_builds_own() {
    let log = InMemoryLog::default();
    let head = append(&log, "alpha", 2).await;
    keeper(&log, SMALL).await.flush(head, at(1)).await.unwrap();
    let mut foreign = log.load_snapshot().await.unwrap().expect("published");
    foreign.projection_version = foreign.projection_version.wrapping_add(1);
    log.store_snapshot(&foreign).await.unwrap();

    let restarted = keeper(&log, SMALL).await;
    assert_eq!(restarted.covered(), Position(0));
    assert_eq!(restarted.flush(head, at(2)).await.unwrap(), Some(head));
    assert_eq!(
        log.load_snapshot()
            .await
            .unwrap()
            .unwrap()
            .projection_version,
        crate::log_snapshot::PROJECTION_FINGERPRINT
    );
}

/// **A snapshot the boot refuses covers nothing.** It is this build's by
/// fingerprint but its blob does not read, so the boot replays the whole log. The
/// keeper then holds no coverage, publishes a snapshot of the head with no new
/// write, and the next boot restores from it instead of replaying again.
#[tokio::test]
async fn a_snapshot_the_boot_refuses_is_not_coverage() {
    use sha2::{Digest, Sha256};
    let log = InMemoryLog::default();
    let head = append(&log, "alpha", 2).await;
    take_snapshot(&log, at(1)).await.unwrap();
    let mut refused = log.load_snapshot().await.unwrap().expect("published");
    refused.blob = b"not a state".to_vec();
    refused.serialized_length = refused.blob.len() as u64;
    refused.checksum = Sha256::digest(&refused.blob).into();
    log.replace_snapshot(Some(refused));
    assert!(
        !crate::log_snapshot::boot(&log)
            .await
            .unwrap()
            .report
            .from_snapshot,
        "the control: the boot refuses this snapshot"
    );

    let restarted = keeper(&log, SMALL).await;
    assert_eq!(restarted.covered(), Position(0));
    assert_eq!(restarted.flush(head, at(2)).await.unwrap(), Some(head));
    assert!(
        crate::log_snapshot::boot(&log)
            .await
            .unwrap()
            .report
            .from_snapshot,
        "the next boot restores from the snapshot the keeper published"
    );
}

/// **The serving loop takes a due snapshot on a tick, before it is stopped.** The
/// head holds as many events as the cadence names, so the tick takes it; a loop
/// that only flushed at its stop would never publish while the watcher waits.
#[tokio::test]
async fn the_loop_takes_a_due_snapshot_on_a_tick() {
    let log = InMemoryLog::default();
    let keeper = keeper(&log, SMALL).await;
    let head = append(&log, "alpha", 3).await;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = keeper.run(|| Some(head), Duration::from_millis(5), async move {
        let _ = stopped.await;
    });
    let watching = async {
        while log.load_snapshot().await.unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        stop.send(()).unwrap();
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(running, watching)
    })
    .await
    .expect("a tick published the snapshot before the stop");
    assert_eq!(keeper.covered(), head);
}

/// **Stopping the periodic loop leaves the shutdown snapshot to the writer.**
/// The cadence is far off and other shutdown tasks can still append.
#[tokio::test]
async fn the_loop_leaves_the_shutdown_snapshot_to_the_writer() {
    let log = InMemoryLog::default();
    let far = Cadence {
        events: 1_000_000,
        dirty_for: Duration::from_secs(3600),
    };
    let keeper = keeper(&log, far).await;
    let head = append(&log, "alpha", 2).await;
    keeper
        .run(|| Some(head), Duration::from_secs(3600), async {})
        .await;
    assert!(
        log.load_snapshot().await.unwrap().is_none(),
        "the periodic loop snapshotted before the writer drained"
    );
}

/// **The shutdown snapshot follows accepted writes even when their callers were
/// abandoned.** Its writer refuses later work and the next boot has no tail.
#[tokio::test]
async fn the_shutdown_snapshot_is_the_writers_last_act() {
    let log = InMemoryLog::default();
    let keeper = keeper(&log, SMALL).await;
    let writer = crate::writer::Writer::default();
    let (release, held) = tokio::sync::oneshot::channel();
    let writing = tokio::spawn({
        let (writer, log) = (writer.clone(), log.clone());
        async move {
            writer
                .run(async move {
                    held.await.unwrap();
                    append(&log, "alpha", 2).await
                })
                .await
        }
    });
    while writer.submitted_jobs() < 1 {
        tokio::task::yield_now().await;
    }
    writing.abort();
    let _ = writing.await;

    let (entered, final_started) = tokio::sync::oneshot::channel();
    let (snapshot, take) = tokio::sync::oneshot::channel();
    let finishing = tokio::spawn({
        let (writer, log) = (writer.clone(), log.clone());
        async move {
            let nested = writer.clone();
            writer
                .finish(async move {
                    entered.send(()).unwrap();
                    take.await.unwrap();
                    // Snapshot I/O uses the same writer as the accepted writes.
                    nested
                        .run(async move {
                            let head = append(&log, "beta", 1).await;
                            keeper.flush(head, at(1)).await.unwrap();
                            head
                        })
                        .await
                })
                .await
        }
    });
    while writer.submitted_jobs() < 2 {
        tokio::task::yield_now().await;
    }
    release.send(()).unwrap();
    final_started.await.unwrap();
    let late = tokio::spawn({
        let writer = writer.clone();
        async move { writer.run(async {}).await }
    });
    let rejected = tokio::time::timeout(Duration::from_secs(1), late).await;
    // Release the final job even on a broken admission gate.
    snapshot.send(()).unwrap();
    let head = finishing.await.unwrap();
    assert!(
        rejected.is_ok_and(|ended| ended.is_err()),
        "the writer admitted work after its final operation"
    );
    let stored = log.load_snapshot().await.unwrap().expect("published");
    assert_eq!(stored.covered_position, head);
    assert_eq!(head, Position(3), "the abandoned caller's write landed");
    assert!(
        log.read_from(stored.covered_position, 256)
            .await
            .unwrap()
            .is_empty()
    );
    let boot = crate::log_snapshot::boot(&log).await.unwrap();
    assert_eq!(boot.position, head);
    assert_eq!(boot.state.teaching.history().len(), 3);
}

/// **Admission closes when the final operation is queued, not when it starts.**
/// A write offered while the final operation still waits behind an accepted job
/// is refused: it could otherwise land behind the shutdown snapshot.
#[tokio::test]
async fn the_writer_refuses_work_from_the_moment_the_final_operation_is_queued() {
    let writer = crate::writer::Writer::default();
    let (release, held) = tokio::sync::oneshot::channel::<()>();
    let writing = tokio::spawn({
        let writer = writer.clone();
        async move {
            writer
                .run(async move {
                    held.await.unwrap();
                })
                .await
        }
    });
    while writer.submitted_jobs() < 1 {
        tokio::task::yield_now().await;
    }
    let finishing = tokio::spawn({
        let writer = writer.clone();
        async move { writer.finish(async {}).await }
    });
    while writer.submitted_jobs() < 2 {
        tokio::task::yield_now().await;
    }
    // The first job still holds the queue, so the final operation has not started.
    let late = tokio::spawn({
        let writer = writer.clone();
        async move { writer.run(async {}).await }
    });
    let rejected = tokio::time::timeout(Duration::from_secs(1), late).await;
    // Release the queue even on a broken admission gate.
    release.send(()).unwrap();
    writing.await.unwrap();
    finishing.await.unwrap();
    assert!(
        rejected.is_ok_and(|ended| ended.is_err()),
        "the writer admitted work while its final operation was only queued"
    );
}
