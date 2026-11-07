//! The log's table, its codec and its held head, on a real store.
use super::*;
use crate::{
    dolt::Dolt,
    testing::{Scratch, free_port, migrated_database},
};
use jojobot_domain::log::testing::contract::batch;
use sqlx::mysql::MySqlPoolOptions;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

async fn fixture() -> (Scratch, Dolt, MySqlPool, DoltLog) {
    let scratch = Scratch::new("log-fence");
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let log = DoltLog::open(pool.clone()).await.unwrap();
    (scratch, server, pool, log)
}

/// A reopen preserves the log's identity; another store starts a different log.
/// Metadata is not a batch and cannot advance its head or appear in its tail.
#[tokio::test]
async fn log_validity_survives_a_reopen_and_belongs_to_one_store() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let before = log.validity().await.unwrap();
    let reopened = DoltLog::open(pool).await.unwrap();
    assert_eq!(reopened.validity().await.unwrap(), before);
    assert!(
        reopened
            .read_from(Position(0), 10)
            .await
            .unwrap()
            .is_empty()
    );
    let other = migrated_database(&server, "beta").await.unwrap();
    let other = DoltLog::open(other).await.unwrap();
    assert_ne!(
        other.validity().await.unwrap().log_identity,
        before.log_identity
    );
    server.stop().await;
}

pub(crate) async fn packet(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header).await?;
    let length = u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize;
    let mut bytes = vec![0; 4 + length];
    bytes[..4].copy_from_slice(&header);
    reader.read_exact(&mut bytes[4..]).await?;
    Ok(bytes)
}

/// **A proxy that holds back the server's answer to the first COMMIT it sees**
/// and drops the connection, after the server has committed: the caller does not
/// learn that its commit landed. Returns the proxy's port, a signal that the
/// answer was dropped, and the proxy task.
pub(crate) async fn lost_commit_reply_proxy(
    upstream: u16,
) -> (u16, Arc<tokio::sync::Notify>, tokio::task::JoinHandle<()>) {
    lost_commit_reply_proxy_when(upstream, Arc::new(std::sync::atomic::AtomicBool::new(true))).await
}

/// The same proxy, losing the answer of the first COMMIT after `armed` is set.
pub(crate) async fn lost_commit_reply_proxy_when(
    upstream: u16,
    once: Arc<std::sync::atomic::AtomicBool>,
) -> (u16, Arc<tokio::sync::Notify>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let dropped = Arc::new(tokio::sync::Notify::new());
    let signal = dropped.clone();
    let proxy = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let once = once.clone();
            let signal = signal.clone();
            tokio::spawn(async move {
                let upstream = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await?;
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_server, mut to_server) = upstream.into_split();
                let withholding = std::sync::atomic::AtomicBool::new(false);
                let requests = async {
                    loop {
                        let bytes = packet(&mut from_client).await?;
                        if bytes[4..] == *b"\x03COMMIT"
                            && once.swap(false, std::sync::atomic::Ordering::SeqCst)
                        {
                            withholding.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        to_server.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>(())
                };
                let replies = async {
                    loop {
                        let bytes = packet(&mut from_server).await?;
                        if withholding.load(std::sync::atomic::Ordering::SeqCst) {
                            assert_eq!(bytes[4], 0, "the server committed before its OK was lost");
                            signal.notify_one();
                            return Err::<(), _>(std::io::Error::other(
                                "drop the COMMIT acknowledgement",
                            ));
                        }
                        to_client.write_all(&bytes).await?;
                    }
                };
                let _ = tokio::try_join!(requests, replies);
                Ok::<_, std::io::Error>(())
            });
        }
    });
    (port, dropped, proxy)
}

/// **A proxy that holds the server's answer to the tail read.** The read runs at
/// the store, and its answer waits until `release` is signalled; `reached` is
/// signalled when it has run. The tail read is recognised by its statement text.
pub(crate) async fn held_tail_reply_proxy(
    upstream: u16,
) -> (
    u16,
    Arc<tokio::sync::Notify>,
    Arc<tokio::sync::Notify>,
    tokio::task::JoinHandle<()>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let reached = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (signal, gate) = (reached.clone(), release.clone());
    let proxy = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let (signal, gate) = (signal.clone(), gate.clone());
            tokio::spawn(async move {
                let upstream = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await?;
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_server, mut to_server) = upstream.into_split();
                let prepared = AtomicBool::new(false);
                let holding = AtomicBool::new(false);
                let requests = async {
                    loop {
                        let bytes = packet(&mut from_client).await?;
                        let text = String::from_utf8_lossy(&bytes[5..]);
                        // 0x16 prepares and 0x17 executes a statement.
                        if bytes[4] == 0x16 && text.contains("MAX(last_position)") {
                            prepared.store(true, Ordering::SeqCst);
                        } else if bytes[4] == 0x17 && prepared.swap(false, Ordering::SeqCst) {
                            holding.store(true, Ordering::SeqCst);
                        }
                        to_server.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>(())
                };
                let replies = async {
                    loop {
                        let bytes = packet(&mut from_server).await?;
                        if holding.swap(false, Ordering::SeqCst) {
                            signal.notify_one();
                            gate.notified().await;
                        }
                        to_client.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<(), std::io::Error>(())
                };
                let _ = tokio::try_join!(requests, replies);
                Ok::<_, std::io::Error>(())
            });
        }
    });
    (port, reached, release, proxy)
}

/// One batch put into the table and committed on the caller's own transaction,
/// as the owner does it, with the head moved after the acknowledged COMMIT.
async fn appended(log: &DoltLog, pool: &MySqlPool, head: Position, batch: &SealedBatch) {
    let mut tx = pool.begin().await.unwrap();
    let staged = log.stage(&mut tx, head, batch).await.unwrap();
    tx.commit().await.unwrap();
    staged.committed();
}

async fn row_in(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, sid: &str) {
    sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, 'claims', '2026-01-01T00:00:00Z')")
        .bind(sid)
        .execute(&mut **tx)
        .await
        .unwrap();
}

async fn rows_of(pool: &MySqlPool, sid: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM session_teaching WHERE sid = ?")
        .bind(sid)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// **A batch staged in a caller's transaction commits with that transaction's
/// rows**, and the head the owner holds moves with the acknowledged COMMIT, so
/// the next batch stages at the new head.
#[tokio::test]
async fn a_staged_batch_commits_with_the_rows_of_its_transaction() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let first = batch("staged-1", 0, &[b"a"]);
    let second = batch("staged-2", 1, &[b"b"]);
    let mut tx = pool.begin().await.unwrap();
    row_in(&mut tx, "homer").await;
    let staged = log.stage(&mut tx, Position(0), &first).await.unwrap();
    tx.commit().await.unwrap();
    staged.committed();
    assert_eq!(rows_of(&pool, "homer").await, 1);
    let mut tx = pool.begin().await.unwrap();
    let staged = log.stage(&mut tx, Position(1), &second).await.unwrap();
    tx.commit().await.unwrap();
    staged.committed();
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap(),
        [first, second]
    );
    drop(log);
    server.stop().await;
}

/// **A transaction that rolls back leaves neither its rows nor its batch**, and
/// the owner's head stays where it was, so the next batch lands at the same
/// position.
#[tokio::test]
async fn a_rolled_back_transaction_leaves_neither_rows_nor_batch() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let lost = batch("lost", 0, &[b"a"]);
    let mut tx = pool.begin().await.unwrap();
    row_in(&mut tx, "marge").await;
    let _staged = log.stage(&mut tx, Position(0), &lost).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(rows_of(&pool, "marge").await, 0);
    assert!(log.read_from(Position(0), 10).await.unwrap().is_empty());
    let next = batch("next", 0, &[b"b"]);
    appended(&log, &pool, Position(0), &next).await;
    drop(log);
    server.stop().await;
}

/// **A refused companion row rolls back the staged batch with it.** The next
/// transaction can stage at the same head and publish only its committed batch.
#[tokio::test]
async fn a_staged_batch_rolls_back_when_a_companion_row_is_refused() {
    let (_scratch, mut server, pool, log) = fixture().await;
    sqlx::query("CREATE TABLE stage_companion (id INT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    let lost = batch("refused-companion", 0, &[b"lost"]);
    let mut tx = pool.begin().await.unwrap();
    let staged = log.stage(&mut tx, Position(0), &lost).await.unwrap();
    sqlx::query("INSERT INTO stage_companion (id) VALUES (1)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let refused = sqlx::query("INSERT INTO stage_companion (id) VALUES (1)")
        .execute(&mut *tx)
        .await
        .expect_err("the companion row repeats a primary key");
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23000"),
        "a real duplicate-key refusal: {refused}"
    );
    tx.rollback().await.unwrap();
    drop(staged);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage_companion")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert!(
        log.read_from(Position(0), 10).await.unwrap().is_empty(),
        "the refused transaction published no batch"
    );
    let kept = batch("after-refused-companion", 0, &[b"kept"]);
    appended(&log, &pool, Position(0), &kept).await;
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [kept]);
    drop(log);
    server.stop().await;
}

/// **A read inside a recovery's fence uses the reader that fence opened, and no
/// other.** A closed reader makes the read fail, so the read cannot have used
/// the log's own connection; a live one answers; and with no fence in scope the
/// read is the ordinary one.
#[tokio::test]
async fn a_read_inside_a_fence_uses_the_fenced_reader() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let next = batch("fenced", 0, &[b"a"]);
    appended(&log, &pool, Position(0), &next).await;
    let options = (*pool.connect_options()).clone();
    let closed = MySqlPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    closed.close().await;
    let refused = crate::writer::sql::reloading(closed, log.read_from(Position(0), 10)).await;
    assert!(
        refused.is_err(),
        "the read did not use the fenced reader: {refused:?}"
    );
    let live = MySqlPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let answered = crate::writer::sql::reloading(live, log.read_from(Position(0), 10)).await;
    assert_eq!(answered.unwrap(), std::slice::from_ref(&next));
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
    drop(log);
    server.stop().await;
}

/// **The tail is read when the log opens**, so the first write after it reads
/// nothing: one read after open, still one after a write.
#[tokio::test]
async fn the_head_is_read_when_the_log_opens() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let reads = || log.0.tail_reads.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(reads(), 1, "the open reads the tail");
    let next = batch("first", 0, &[b"a"]);
    appended(&log, &pool, Position(0), &next).await;
    assert_eq!(reads(), 1, "the first write reads nothing");
    drop(log);
    server.stop().await;
}

/// **A commit whose answer was lost is settled by one read of the tail.** The
/// batch did commit; the caller does not know it. After the head is settled the
/// next batch stages at the position after it, and a batch at the old position
/// is refused: nothing is applied twice.
#[tokio::test]
async fn a_commit_with_a_lost_answer_is_settled_by_one_tail_read() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let first = batch("settled-1", 0, &[b"a"]);
    let mut tx = pool.begin().await.unwrap();
    let staged = log.stage(&mut tx, Position(0), &first).await.unwrap();
    tx.commit().await.unwrap();
    staged.unknown();
    log.settle(&pool).await.unwrap();
    assert_eq!(
        log.0.tail_reads.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the open and the settlement"
    );
    let again = batch("settled-again", 0, &[b"b"]);
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        log.stage(&mut tx, Position(0), &again).await.err(),
        Some(LogError::Conflict)
    );
    tx.rollback().await.unwrap();
    let second = batch("settled-2", 1, &[b"c"]);
    let mut tx = pool.begin().await.unwrap();
    let staged = log.stage(&mut tx, Position(1), &second).await.unwrap();
    tx.commit().await.unwrap();
    staged.committed();
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap(),
        [first, second]
    );
    drop(log);
    server.stop().await;
}

/// **The owner reads the log's tail once**, not before each write: three
/// batches, one read.
#[tokio::test]
async fn the_tail_is_read_once_for_any_number_of_writes() {
    let (_scratch, mut server, pool, log) = fixture().await;
    for (id, head) in [("one", 0), ("two", 1), ("three", 2)] {
        let next = batch(id, head, &[b"a"]);
        appended(&log, &pool, Position(head), &next).await;
    }
    assert_eq!(
        log.0.tail_reads.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    drop(log);
    server.stop().await;
}

/// **A tail read that finishes after an append never sets the head back.** A
/// settle reads the tail, an append commits and moves the head while that read is
/// in flight, and the read is then applied. The head stays at the newer batch,
/// and the next append stages after it: a head set back would refuse every
/// later append as a conflict until a restart.
#[tokio::test]
async fn an_older_settle_read_never_moves_the_head_back() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let (port, reached, release, _proxy) =
        held_tail_reply_proxy(pool.connect_options().get_port()).await;
    let reader = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy_with((*pool.connect_options()).clone().port(port));
    // The settle runs the tail read through the proxy, which holds its answer.
    let settling = {
        let (log, reader) = (log.clone(), reader.clone());
        tokio::spawn(async move { log.settle(&reader).await })
    };
    tokio::time::timeout(Duration::from_secs(10), reached.notified())
        .await
        .expect("the settle's tail read reached the store");
    // The read has run against an empty log; an append commits while its answer
    // is held.
    let older_read = 0;
    let newer = batch("newer", 0, &[b"a"]);
    appended(&log, &pool, Position(0), &newer).await;
    let moved_to = newer.range().last.0;
    assert!(moved_to > older_read, "the append moved the head");
    release.notify_one();
    settling.await.unwrap().unwrap();
    assert_eq!(
        *log.0.head.lock().unwrap(),
        Some(moved_to),
        "the older read did not set the head back"
    );
    let after = batch("after", moved_to, &[b"b"]);
    appended(&log, &pool, Position(moved_to), &after).await;
    drop(log);
    server.stop().await;
}

/// **A stored time that cannot be read names its row.** The import never invents a
/// time, so the boot stops; the error says which run and domain to repair.
#[tokio::test]
async fn an_unreadable_taught_at_names_its_run_and_domain() {
    let (_scratch, mut server, pool, log) = fixture().await;
    sqlx::query(
        "INSERT INTO session_teaching (sid, domain, taught_at) VALUES ('homer', 'claims', 'not a time')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let said = crate::teaching_log::import_marks(&pool)
        .await
        .expect_err("a time that does not parse stops the import")
        .to_string();
    assert!(said.contains("homer") && said.contains("claims"), "{said}");
    drop(log);
    server.stop().await;
}
