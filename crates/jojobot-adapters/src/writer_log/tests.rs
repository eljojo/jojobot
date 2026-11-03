use super::*;
use crate::{
    dolt::Dolt,
    testing::{Scratch, free_port, migrated_database},
};
use jojobot_domain::log::testing::contract::batch;
use sqlx::mysql::MySqlPoolOptions;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// **A second log over the same store is refused by the writer's lock.** The
/// first owner holds the lock from the moment it opens, so a second one cannot
/// read a head beside a predecessor that can still commit.
#[tokio::test]
async fn a_second_owner_is_refused_by_the_lock() {
    let scratch = Scratch::new("writer-log-second-owner");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let first = WriterLog::open(pool.clone(), Writer::default())
        .await
        .unwrap();
    let second = WriterLog::open(pool, Writer::default()).await;
    assert!(
        matches!(second, Err(LogError::Store(_))),
        "a second owner must be refused while the first holds the lock"
    );
    drop(first);
    server.stop().await;
}

/// **Many tasks submit at once through one log and every batch lands whole.**
/// Each task reads the tail and appends at it, retrying on a conflict. Positions
/// are contiguous, every batch reads back whole, and an append at the tail is
/// admitted, so the head the log holds equals its tail.
#[tokio::test]
async fn many_tasks_submitting_at_once_leave_whole_batches_and_a_true_head() {
    let scratch = Scratch::new("writer-log-many-tasks");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let log = Arc::new(WriterLog::open(pool, writer.clone()).await.unwrap());
    let tasks = 12usize;
    let mut set = tokio::task::JoinSet::new();
    for n in 0..tasks {
        let log = log.clone();
        set.spawn(async move {
            let id = format!("task-{n}");
            let members: Vec<Vec<u8>> = vec![id.clone().into_bytes(), b"x".to_vec()];
            let refs: Vec<&[u8]> = members.iter().map(Vec::as_slice).collect();
            loop {
                let tail = log
                    .read_from(Position(0), 1000)
                    .await
                    .unwrap()
                    .last()
                    .map_or(0, |b| b.range().last.0);
                let sealed = batch(&id, tail, &refs);
                match log.append(Position(tail), &sealed).await {
                    Ok(Append::Committed(_)) => return sealed,
                    Ok(Append::DefinitelyNotCommitted) | Err(LogError::Conflict) => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
        });
    }
    let mut sent = Vec::new();
    while let Some(done) = set.join_next().await {
        sent.push(done.unwrap());
    }
    let read = log.read_from(Position(0), 1000).await.unwrap();
    assert_eq!(read.len(), tasks, "every batch landed once");
    let mut next = 1;
    for stored in &read {
        assert_eq!(stored.range().first.0, next, "positions are contiguous");
        assert!(sent.contains(stored), "a batch reads back whole");
        next = stored.range().last.0 + 1;
    }
    let tail = next - 1;
    let after = batch("after-all", tail, &[b"z"]);
    assert_eq!(
        log.append(Position(tail), &after).await.unwrap(),
        Append::Committed(after.range()),
        "the held head equals the log's tail"
    );
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **A commit whose answer is lost is settled once, by the writer's fence.** The
/// proxy lets the server commit and drops its answer, so the caller cannot tell.
/// The append answers `Indeterminate`; the writer fences and settles the head
/// with one tail read; the batch is read back once, the same batch sent again is
/// committed and not applied twice, and the next batch lands at the position
/// after it.
#[tokio::test]
async fn a_commit_whose_answer_is_lost_recovers_exactly_once() {
    let scratch = Scratch::new("writer-log-lost-answer");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let (port, dropped, proxy) =
        crate::dolt_log::tests::lost_commit_reply_proxy(pool.connect_options().get_port()).await;
    let source =
        MySqlPoolOptions::new().connect_lazy_with((*pool.connect_options()).clone().port(port));
    let writer = Writer::default();
    let log = WriterLog::open(source, writer.clone()).await.unwrap();
    let lost = batch("lost-answer", 0, &[b"null", b""]);
    assert_eq!(
        log.append(Position(0), &lost).await.unwrap(),
        Append::Indeterminate
    );
    tokio::time::timeout(Duration::from_secs(5), dropped.notified())
        .await
        .unwrap();
    assert_eq!(
        writer.recovery_counts().recovered,
        1,
        "the writer's fence settled it"
    );
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap().as_slice(),
        std::slice::from_ref(&lost)
    );
    assert_eq!(
        log.append(Position(0), &lost).await.unwrap(),
        Append::Committed(lost.range())
    );
    // The lost batch holds two events, positions 1 and 2.
    let after = batch("after-answer", 2, &[b"c"]);
    assert_eq!(
        log.append(Position(2), &after).await.unwrap(),
        Append::Committed(after.range()),
        "the head after the settlement is the committed batch's"
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [lost, after]);
    drop(log);
    writer.drain().await;
    proxy.abort();
    server.stop().await;
}

async fn fixture() -> (Scratch, Dolt, sqlx::MySqlPool, WriterLog, Writer) {
    let scratch = Scratch::new("writer-log-race");
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let log = WriterLog::open(pool.clone(), writer.clone()).await.unwrap();
    (scratch, server, pool, log, writer)
}

async fn batches(pool: &sqlx::MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM event_log WHERE first_position IS NOT NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// **Two appends at one head: exactly one lands, and the other is refused by
/// the head check.** Both go through the one writer, which runs them one after
/// the other, so the second finds the head moved and is a conflict, not a store
/// error: the position was never offered to it.
#[tokio::test]
async fn two_appends_at_one_head_land_once_and_the_other_conflicts() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    let first = batch("race-1", 0, &[b"a"]);
    let second = batch("race-2", 0, &[b"b"]);
    let (one, two) = tokio::join!(
        log.append(Position(0), &first),
        log.append(Position(0), &second)
    );
    let answers = [one, two];
    let landed = answers
        .iter()
        .filter(|answer| matches!(answer, Ok(Append::Committed(_))))
        .count();
    let conflicts = answers
        .iter()
        .filter(|answer| matches!(answer, Err(LogError::Conflict)))
        .count();
    assert_eq!((landed, conflicts), (1, 1), "{answers:?}");
    assert_eq!(batches(&pool).await, 1);
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **A proxy that holds the first append statement it sees at the store's door**
/// until the test lets it through, and says when it holds it. The statement is
/// the one every append sends first, so the append is inside the store, past the
/// writer's queue and its one connection, and not yet committed.
async fn append_gate_proxy(
    upstream: u16,
    held: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let once = Arc::new(AtomicBool::new(true));
    let proxy = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let (held, release, once) = (held.clone(), release.clone(), once.clone());
            tokio::spawn(async move {
                let upstream = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await?;
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_server, mut to_server) = upstream.into_split();
                let requests = async {
                    loop {
                        let bytes = crate::dolt_log::tests::packet(&mut from_client).await?;
                        if bytes[4] == 0x16
                            && String::from_utf8_lossy(&bytes[5..])
                                .contains("SELECT payload FROM event_log WHERE slot")
                            && once.swap(false, Ordering::SeqCst)
                        {
                            held.notify_one();
                            release.notified().await;
                        }
                        to_server.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>(())
                };
                let replies = async {
                    loop {
                        let bytes = crate::dolt_log::tests::packet(&mut from_server).await?;
                        to_client.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>(())
                };
                let _ = tokio::try_join!(requests, replies);
                Ok::<_, std::io::Error>(())
            });
        }
    });
    (port, proxy)
}

/// **A read waits in the writer's queue behind an append in flight.** The
/// append is held inside the store before it commits. A read asked meanwhile is
/// accepted by the queue and does not run: the writer starts one job at a time,
/// and a read does not use the connection an append holds, so only the queue
/// stands between the read and the store. Released, the append commits and the
/// read, which ran after it, returns the batch. Without the queue the read
/// answers at once with an empty log. (Two appends cannot tell the queue from
/// the writer's one connection: the second waits for that connection either way.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_waits_in_the_writers_queue_behind_an_append_in_flight() {
    let scratch = Scratch::new("writer-log-queue");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let held = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (port, proxy) = append_gate_proxy(
        pool.connect_options().get_port(),
        held.clone(),
        release.clone(),
    )
    .await;
    let source =
        MySqlPoolOptions::new().connect_lazy_with((*pool.connect_options()).clone().port(port));
    let writer = Writer::default();
    let log = WriterLog::open(source, writer.clone()).await.unwrap();
    let before = writer.submitted_jobs();
    let appended = batch("queue-1", 0, &[b"a"]);
    let append = tokio::spawn({
        let (log, appended) = (log.clone(), appended.clone());
        async move { log.append(Position(0), &appended).await }
    });
    tokio::time::timeout(Duration::from_secs(10), held.notified())
        .await
        .expect("the append reached the store");
    let mut read = Box::pin(log.read_from(Position(0), 10));
    assert!(
        tokio::time::timeout(Duration::from_millis(500), &mut read)
            .await
            .is_err(),
        "the read ran while the append was in flight"
    );
    assert_eq!(
        writer.submitted_jobs() - before,
        2,
        "the queue accepted both jobs"
    );
    release.notify_one();
    assert_eq!(
        append.await.unwrap().unwrap(),
        Append::Committed(appended.range())
    );
    assert_eq!(read.await.unwrap(), [appended]);
    drop(log);
    writer.drain().await;
    proxy.abort();
    server.stop().await;
}

/// **A new owner cannot open while its predecessor has a commit in flight, and
/// opens on the predecessor's tail once it is gone.** The predecessor's append is
/// held inside the store, so it holds the lock and has not committed. A second
/// log over the same store is refused. Released, the append commits; the
/// predecessor is dropped and a successor opens, reads the head the predecessor
/// left and appends at it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_owner_waits_out_a_predecessor_with_a_commit_in_flight() {
    let scratch = Scratch::new("writer-log-successor");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let held = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (port, proxy) = append_gate_proxy(
        pool.connect_options().get_port(),
        held.clone(),
        release.clone(),
    )
    .await;
    let source =
        MySqlPoolOptions::new().connect_lazy_with((*pool.connect_options()).clone().port(port));
    let writer = Writer::default();
    let predecessor = WriterLog::open(source, writer.clone()).await.unwrap();
    let appended = batch("predecessor", 0, &[b"a", b"b"]);
    let append = tokio::spawn({
        let (log, appended) = (predecessor.clone(), appended.clone());
        async move { log.append(Position(0), &appended).await }
    });
    tokio::time::timeout(Duration::from_secs(10), held.notified())
        .await
        .expect("the append reached the store");
    let refused = WriterLog::open(pool.clone(), Writer::default()).await;
    assert!(
        matches!(refused, Err(LogError::Store(_))),
        "a new owner must be refused while the predecessor's commit is in flight"
    );
    release.notify_one();
    assert_eq!(
        append.await.unwrap().unwrap(),
        Append::Committed(appended.range())
    );
    drop(predecessor);
    writer.drain().await;
    // The writer owns the connection that holds the lock, so the predecessor is
    // gone when its writer is dropped; the store frees the lock a moment later.
    drop(writer);
    let successor_writer = Writer::default();
    let successor = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match WriterLog::open(pool.clone(), successor_writer.clone()).await {
                Ok(log) => return log,
                Err(LogError::Store(_)) => tokio::time::sleep(Duration::from_millis(100)).await,
                Err(other) => panic!("unexpected {other:?}"),
            }
        }
    })
    .await
    .expect("the successor opens once the predecessor is gone");
    let tail = appended.range().last;
    let next = batch("successor", tail.0, &[b"c"]);
    assert_eq!(
        successor.append(tail, &next).await.unwrap(),
        Append::Committed(next.range()),
        "the successor's head is the predecessor's tail"
    );
    assert_eq!(
        successor.read_from(Position(0), 10).await.unwrap(),
        [appended, next]
    );
    drop(successor);
    successor_writer.drain().await;
    proxy.abort();
    server.stop().await;
}

/// **A position another writer already took is refused by the store itself.**
/// A batch is put at position 1 beneath the owner, as a second writer would. The
/// owner's head still says 0, so its head check passes; the table's unique key
/// on the first position refuses the insert, nothing lands, and the refusal is
/// an error, never a second batch at the same position.
#[tokio::test]
async fn a_position_another_writer_took_is_refused_by_the_store() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    sqlx::query(
        "INSERT INTO event_log (slot, first_position, last_position, payload) VALUES ('other', 1, 1, ?)",
    )
    .bind(b"x".to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let mine = batch("mine", 0, &[b"a"]);
    let answer = log.append(Position(0), &mine).await;
    assert!(
        matches!(answer, Err(LogError::Store(_))),
        "the store refused the taken position: {answer:?}"
    );
    assert_eq!(batches(&pool).await, 1, "only the other writer's batch");
    drop(log);
    writer.drain().await;
    server.stop().await;
}

fn recorded_row(sid: &str) -> RowChange {
    RowChange::Record {
        sid: sid.into(),
        domain: "claims".into(),
        taught_at: "2026-01-01T00:00:00Z".parse().unwrap(),
    }
}

async fn rows_of(pool: &sqlx::MySqlPool, sid: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM session_teaching WHERE sid = ?")
        .bind(sid)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// **A change of the legacy rows commits with its batch.** The row and the
/// batch are both there after one append.
#[tokio::test]
async fn a_row_change_commits_with_its_batch() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    let next = batch("rows", 0, &[b"a"]);
    assert_eq!(
        log.append_with_rows(Position(0), &next, vec![recorded_row("homer")])
            .await
            .unwrap(),
        Append::Committed(next.range())
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_teaching WHERE sid = 'homer'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **Forgetting a run takes every one of its rows, in the batch's own commit**,
/// and leaves the rows of another run.
#[tokio::test]
async fn a_forget_row_change_takes_all_the_rows_of_its_run() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    let first = batch("rows-1", 0, &[b"a"]);
    let other_domain = RowChange::Record {
        sid: "homer".into(),
        domain: "mail".into(),
        taught_at: "2026-01-01T00:00:00Z".parse().unwrap(),
    };
    log.append_with_rows(
        Position(0),
        &first,
        vec![recorded_row("homer"), other_domain, recorded_row("lisa")],
    )
    .await
    .unwrap();
    let second = batch("rows-2", 1, &[b"b"]);
    assert_eq!(
        log.append_with_rows(
            Position(1),
            &second,
            vec![RowChange::Forget {
                sid: "homer".into()
            }]
        )
        .await
        .unwrap(),
        Append::Committed(second.range())
    );
    assert_eq!(rows_of(&pool, "homer").await, 0);
    assert_eq!(rows_of(&pool, "lisa").await, 1);
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **A change the server refuses leaves neither the row nor the batch**, and
/// the log takes the next append at the same head. The second change repeats
/// the first row's key, which the table refuses.
#[tokio::test]
async fn a_refused_row_change_leaves_no_batch() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    let next = batch("refused", 0, &[b"a"]);
    assert_eq!(
        log.append_with_rows(
            Position(0),
            &next,
            vec![recorded_row("marge"), recorded_row("marge")]
        )
        .await
        .unwrap(),
        Append::DefinitelyNotCommitted
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_teaching WHERE sid = 'marge'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert!(log.read_from(Position(0), 10).await.unwrap().is_empty());
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **The import reads every mark with its own taught_at**, including a run with
/// no session card.
#[tokio::test]
async fn the_marks_come_back_with_their_times() {
    let (_scratch, mut server, pool, log, writer) = fixture().await;
    let next = batch("marks", 0, &[b"a"]);
    log.append_with_rows(Position(0), &next, vec![recorded_row("homer")])
        .await
        .unwrap();
    let marks = crate::dolt::teaching::DoltTeachings::open(pool.clone())
        .marks()
        .await
        .unwrap();
    assert_eq!(
        marks,
        [crate::teaching_log::Mark {
            sid: jojobot_domain::session::Sid("homer".into()),
            domain: "claims".into(),
            taught_at: "2026-01-01T00:00:00Z".parse().unwrap(),
        }]
    );
    drop(log);
    writer.drain().await;
    server.stop().await;
}
