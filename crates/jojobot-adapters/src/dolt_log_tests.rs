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

/// **A proxy that loses the connection in the middle of a read of the log.** While
/// `armed` is set, the first statement that names the log's table is dropped with
/// its connection, before the server sees it, so the caller is told a connection
/// was lost and not what became of the read. A write is not told apart here: the
/// caller arms it only around reads. Returns the proxy's port, a signal that the
/// connection was dropped, and the proxy task.
pub(crate) async fn dropped_read_proxy(
    upstream: u16,
    armed: Arc<std::sync::atomic::AtomicBool>,
) -> (u16, Arc<tokio::sync::Notify>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let dropped = Arc::new(tokio::sync::Notify::new());
    let signal = dropped.clone();
    let proxy = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let armed = armed.clone();
            let signal = signal.clone();
            tokio::spawn(async move {
                let upstream = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await?;
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_server, mut to_server) = upstream.into_split();
                let requests = async {
                    loop {
                        let bytes = packet(&mut from_client).await?;
                        let statement = matches!(bytes.get(4), Some(0x03 | 0x16));
                        // The log is two tables: the events, and the snapshots kept beside them.
                        let names_the_log = bytes.windows(9).any(|part| part == b"event_log")
                            || bytes.windows(12).any(|part| part == b"log_snapshot");
                        if statement
                            && names_the_log
                            && armed.swap(false, std::sync::atomic::Ordering::SeqCst)
                        {
                            signal.notify_one();
                            return Err::<(), _>(std::io::Error::other("drop the read"));
                        }
                        to_server.write_all(&bytes).await?;
                    }
                };
                let replies = async {
                    loop {
                        let bytes = packet(&mut from_server).await?;
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
            .and_then(|e| e.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
            .map(|e| e.number()),
        Some(1062),
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
    let log = DoltLog::open(pool.clone()).await.unwrap();
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

/// A snapshot stored for the shape `projection`, covering the one batch.
fn snapshot_for(
    projection: u32,
    validity: &Validity,
    covered: Position,
    checksum: [u8; 32],
) -> ValidatedSnapshot {
    use sha2::{Digest, Sha256};
    let blob = projection.to_le_bytes().to_vec();
    ValidatedSnapshot {
        snapshot_format_version: 1,
        projection_version: projection,
        log_identity: validity.log_identity.clone(),
        representation_epoch: validity.representation_epoch,
        erasure_epoch: validity.erasure_epoch,
        covered_position: covered,
        batch_checksum: checksum,
        created_at: jiff::Timestamp::UNIX_EPOCH,
        checksum: Sha256::digest(&blob).into(),
        serialized_length: blob.len() as u64,
        blob,
    }
}

/// **A snapshot is kept as bytes in a table of its own that no commit holds.** The blob is the
/// column's bytes, not a JSON array of numbers; the header is columns; no row of the event log
/// is a snapshot; the table is named in `dolt_ignore`; and after a mark that commits other rows
/// no commit of the store holds the table. A snapshot is derived from the log, which is its
/// history, so a copy of it in a commit would be a copy kept for nothing.
#[tokio::test]
async fn a_snapshot_is_kept_as_bytes_in_an_ignored_table_no_commit_holds() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let own = crate::log_snapshot::PROJECTION_FINGERPRINT;
    let first = batch("alpha", 0, &[b"a"]);
    appended(&log, &pool, Position(0), &first).await;
    let validity = log.validity().await.unwrap();
    let mine = snapshot_for(own, &validity, first.range().last, first.header().checksum);
    put(&pool, &mine).await;

    let (body, identity, projection): (Vec<u8>, String, u32) = sqlx::query_as(
        "SELECT body, log_identity, projection_version FROM log_snapshot WHERE slot = ?",
    )
    .bind("snapshot")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(body, mine.blob, "the blob is the column's bytes");
    assert_eq!((identity, projection), (mine.log_identity.clone(), own));
    let in_the_log: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_log WHERE slot LIKE 'snapshot:%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(in_the_log, 0, "a snapshot is a row of the event log");
    let named: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dolt_ignore WHERE pattern = 'log_snapshot'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(named, 1, "the table is not named in dolt_ignore");

    crate::dolt::snapshot(&pool, "snapshot-table-1f7b").await;
    let commits: Vec<String> = sqlx::query_scalar("SELECT commit_hash FROM dolt_log")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(commits.len() > 1, "the mark committed nothing");
    for commit in &commits {
        let read = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM log_snapshot AS OF '{commit}'"
        )))
        .fetch_one(&pool)
        .await;
        assert!(
            read.is_err(),
            "the commit {commit} holds the snapshot table"
        );
    }
    server.stop().await;
}

async fn put(pool: &MySqlPool, snapshot: &ValidatedSnapshot) {
    let mut tx = pool.begin().await.unwrap();
    DoltLog::put_snapshot(&mut tx, snapshot).await.unwrap();
    tx.commit().await.unwrap();
}

/// A log whose store accepts a connection and never answers it, so a connection
/// that is opened waits for the whole of its time.
async fn a_log_over_a_store_that_never_answers() -> (DoltLog, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let holding = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((client, _)) = listener.accept().await {
            held.push(client);
        }
    });
    let pool = MySqlPool::connect_lazy_with(
        sqlx::mysql::MySqlConnectOptions::new()
            .host("127.0.0.1")
            .port(port),
    );
    (DoltLog::unread(pool), holding)
}

/// A proxy that forwards every connection to the store until `stalled` is set, and
/// from then on accepts a connection and holds it without a word, so a connection
/// opened after that waits for the whole of its time. Connections already open keep
/// working.
pub(crate) async fn stalling_proxy(
    upstream: u16,
) -> (
    u16,
    Arc<std::sync::atomic::AtomicBool>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let stalled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stalled.clone();
    let proxy = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((mut client, _)) = listener.accept().await {
            if flag.load(std::sync::atomic::Ordering::SeqCst) {
                held.push(client);
                continue;
            }
            tokio::spawn(async move {
                let Ok(mut store) = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await
                else {
                    return;
                };
                let _ = tokio::io::copy_bidirectional(&mut client, &mut store).await;
            });
        }
    });
    (port, stalled, proxy)
}

/// **A connection that times out answers a store error, never the outcome of a
/// write.** The wait wraps the opening of a connection, before any statement is
/// sent, so nothing is unknown about what the caller wrote: the caller tries
/// again. Both callers of that wait, a read and the creation of the log's table,
/// answer it.
#[tokio::test]
async fn a_connection_that_times_out_answers_a_store_error_to_a_read_and_to_the_creation() {
    let (log, _holding) = a_log_over_a_store_that_never_answers().await;
    let read = log.read_from(Position(0), 10).await;
    assert!(
        matches!(read, Err(LogError::Store(_))),
        "a read that times out is a store error: {read:?}"
    );
    let creation = log.create_validity().await;
    assert!(
        matches!(creation, Err(LogError::Store(_))),
        "the creation that times out is a store error: {creation:?}"
    );
}

/// Old fingerprint slots are disposable. A publication leaves one row.
#[tokio::test]
async fn one_slot_publication_replaces_old_slots() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let validity = log.validity().await.unwrap();
    put(&pool, &snapshot_for(0, &validity, Position(0), [0; 32])).await;
    for version in [1, 2, 3] {
        sqlx::query(
            "INSERT INTO log_snapshot SELECT ?, covered_position, snapshot_format_version, ?, \
             log_identity, representation_epoch, erasure_epoch, batch_checksum, created_at, \
             checksum, serialized_length, body FROM log_snapshot WHERE slot = 'snapshot'",
        )
        .bind(format!("snapshot:{version}"))
        .bind(version)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("DELETE FROM log_snapshot WHERE slot = 'snapshot'")
        .execute(&pool)
        .await
        .unwrap();
    let legacy: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM log_snapshot")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(legacy, 3, "all legacy slots exist before publication");
    put(&pool, &snapshot_for(4, &validity, Position(0), [0; 32])).await;
    let slots: Vec<String> = sqlx::query_scalar("SELECT slot FROM log_snapshot")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(slots, ["snapshot"], "only the single snapshot slot remains");
    server.stop().await;
}

/// Both upgrade paths re-project quietly, then the next boot reads the published State.
#[tokio::test]
async fn one_slot_foreign_boot_logs_info_and_the_next_boot_loads() {
    one_slot_upgrade_boot(false).await;
}

#[tokio::test]
async fn one_slot_new_table_boot_logs_info_and_the_next_boot_loads() {
    one_slot_upgrade_boot(true).await;
}

async fn one_slot_upgrade_boot(newly_created: bool) {
    use tracing::instrument::WithSubscriber;
    let _counting = crate::log_snapshot::FINGERPRINT_SKIPS.lock().await;
    let (_scratch, mut server, pool, log, applied) = if newly_created {
        let scratch = Scratch::new("snapshot-migration-account");
        let (server, applied) = Dolt::ready(&scratch.0, free_port()).await.unwrap();
        let pool = server.pool().clone();
        let log = DoltLog::open(pool.clone()).await.unwrap();
        (scratch, server, pool, log, applied)
    } else {
        let (scratch, server, pool, log) = fixture().await;
        (scratch, server, pool, log, vec![])
    };
    let event = crate::teaching_log::codec::encode(
        &jojobot_domain::teaching::events::TeachingEvent::Recorded {
            run: jojobot_domain::teaching::events::RunId("homer".into()),
            domain: "claims".into(),
            taught_at: jiff::Timestamp::UNIX_EPOCH,
        },
    );
    let first = batch("alpha", 0, &[&event]);
    appended(&log, &pool, Position(0), &first).await;
    let writer = crate::writer::Writer::default();
    let serving = crate::writer_log::WriterLog::open(pool.clone(), writer.clone())
        .await
        .unwrap();
    crate::log_snapshot::take_snapshot(&serving, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let expected_head = serving.head().unwrap();
    sqlx::query("UPDATE log_snapshot SET slot = 'snapshot:1', projection_version = 1")
        .execute(&pool)
        .await
        .unwrap();
    drop(serving);
    writer.drain().await;
    drop(writer);
    let writer = crate::writer::Writer::default();
    {
        if newly_created {
            sqlx::query("DELETE FROM log_snapshot")
                .execute(&pool)
                .await
                .unwrap();
        }
        let captured = crate::log_capture::Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::INFO)
            .finish();
        let (boot_pool, boot_writer, boot_applied) =
            (pool.clone(), writer.clone(), applied.clone());
        let (restored, _keeper) = writer
            .run(async move {
                crate::teaching_log::serving_kept_after_migrations(
                    &boot_pool,
                    boot_writer,
                    &boot_applied,
                )
                .with_subscriber(subscriber)
                .await
            })
            .await
            .unwrap();
        assert!(
            jojobot_domain::teaching::Teachings::has_contact(
                &restored,
                &jojobot_domain::session::Sid("homer".into()),
                "claims",
            )
            .await
            .unwrap()
        );
        let said = captured.text();
        assert!(
            said.lines()
                .any(|line| line.contains("INFO") && line.contains("fingerprint")),
            "{said}"
        );
        assert!(!said.contains("WARN"), "{said}");
        let serving = crate::writer_log::WriterLog::open(pool.clone(), writer.clone())
            .await
            .unwrap();
        crate::log_snapshot::take_snapshot(&serving, jiff::Timestamp::UNIX_EPOCH)
            .await
            .unwrap();
        let captured = crate::log_capture::Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::INFO)
            .finish();
        let restored = crate::log_snapshot::boot(&serving)
            .with_subscriber(subscriber)
            .await
            .unwrap();
        assert_eq!(restored.position, expected_head);
        assert!(
            captured.text().contains("from_snapshot=true"),
            "{}",
            captured.text()
        );
    }
    writer.drain().await;
    drop(writer);
    server.stop().await;
}
