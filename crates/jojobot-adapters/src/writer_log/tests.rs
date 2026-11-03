use super::*;
use crate::{
    dolt::Dolt,
    testing::{Scratch, free_port, migrated_database},
};
use jojobot_domain::log::testing::contract::batch;
use sqlx::mysql::MySqlPoolOptions;
use std::time::Duration;

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
