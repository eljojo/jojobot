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
