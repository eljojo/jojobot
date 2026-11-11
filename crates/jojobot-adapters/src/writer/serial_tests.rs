//! The writer runs one job at a time, found without a race.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::sync::Notify;

use super::Writer;

/// **A second job does not start while the first holds the queue.** The first
/// job signals that it is inside and waits to be released; the second is
/// submitted behind it and must not have started after the runtime has been given
/// many turns to start it. Once the first is released the second runs, so the
/// wait was the queue's and not a job that never ran. Two jobs inside the queue
/// at once are counted, whatever the order they finish in.
#[tokio::test]
async fn a_second_job_waits_while_the_first_holds_the_queue() {
    let writer = Writer::default();
    let inside = Arc::new(AtomicUsize::new(0));
    let most_inside = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let second_started = Arc::new(AtomicBool::new(false));

    let first = tokio::spawn({
        let (writer, inside, most_inside) = (writer.clone(), inside.clone(), most_inside.clone());
        let (entered, release) = (entered.clone(), release.clone());
        async move {
            writer
                .run(async move {
                    let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                    most_inside.fetch_max(now, Ordering::SeqCst);
                    entered.notify_one();
                    release.notified().await;
                    inside.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
        }
    });
    entered.notified().await;

    let second = tokio::spawn({
        let (writer, inside, most_inside) = (writer.clone(), inside.clone(), most_inside.clone());
        let started = second_started.clone();
        async move {
            writer
                .run(async move {
                    started.store(true, Ordering::SeqCst);
                    let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                    most_inside.fetch_max(now, Ordering::SeqCst);
                    inside.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
        }
    });
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
    assert!(
        !second_started.load(Ordering::SeqCst),
        "the second job started while the first held the queue"
    );

    release.notify_one();
    first.await.expect("the first job finished");
    second.await.expect("the second job finished");
    assert!(
        second_started.load(Ordering::SeqCst),
        "the second job runs once the first is released"
    );
    assert_eq!(
        most_inside.load(Ordering::SeqCst),
        1,
        "two jobs were inside the writer at once"
    );
}
