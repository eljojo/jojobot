use super::*;

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

async fn keeper(log: &InMemoryLog, cadence: Cadence) -> SnapshotKeeper {
    SnapshotKeeper::open(Arc::new(log.clone()), cadence)
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

/// **A stop takes one last snapshot of whatever is uncovered.** The cadence is
/// far off, so only the shutdown moment can publish it.
#[tokio::test]
async fn the_loop_flushes_an_uncovered_head_when_it_stops() {
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
    let stored = log.load_snapshot().await.unwrap().expect("published");
    assert_eq!(stored.covered_position, head);
}
