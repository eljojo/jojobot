//! Native failure boundaries from the adopted Dolt fence spike.
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

#[tokio::test]
async fn dolt_log_contract_native_commit_outlives_kill_until_its_fence() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let next = batch("late", 0, &[b"a", b"b"]);
    log.inject(Fault::DelayedCommit).await;
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    let mut gate = DoltLog::fresh(&pool).await.unwrap();
    let held: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('log-gate', 0)")
        .fetch_one(&mut gate)
        .await
        .unwrap();
    assert_eq!(held, Some(1));
    let mut state = log.0.session.lock().await;
    let old_id = state.id;
    let mut old = state.connection.take().unwrap();
    state.delayed = false;
    drop(state);
    let ready: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('log-ready', 0)")
        .fetch_one(&mut old)
        .await
        .unwrap();
    assert_eq!(ready, Some(1));
    let commit = tokio::spawn(async move {
        let _ = sqlx::raw_sql("SET @ready = RELEASE_LOCK('log-ready'); SET @gate = GET_LOCK('log-gate', 30); COMMIT; SET @released = RELEASE_LOCK('log-gate')")
            .execute(&mut old).await;
        let _ = old.close().await;
    });
    let entered: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('log-ready', 5)")
        .fetch_one(&mut gate)
        .await
        .unwrap();
    assert_eq!(entered, Some(1));
    let rows = sqlx::query("SHOW FULL PROCESSLIST")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        rows.iter()
            .any(|row| row.get::<i64, _>("Id") as u64 == old_id
                && row.get::<String, _>("Command") == "Query")
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "a fresh SELECT is absent while COMMIT can still land"
    );
    assert_eq!(
        log.read_from(Position(0), 10).await,
        Err(LogError::Indeterminate),
        "KILL returned, but the old server command still owns the fence"
    );
    sqlx::query("SELECT RELEASE_LOCK('log-gate')")
        .execute(&mut gate)
        .await
        .unwrap();
    commit.await.unwrap();
    let tail = log.read_from(Position(0), 10).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tail.len() as i64, count);
    assert!(tail.is_empty() || tail == [next]);
    let state = log.0.session.lock().await;
    let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK(?)")
        .bind(&log.0.lock)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, Some(state.id));
    drop(state);
    drop(log);
    server.stop().await;
}

#[tokio::test]
async fn dolt_log_contract_native_lost_connection_rolls_back_before_replay() {
    let (_scratch, mut server, _pool, log) = fixture().await;
    log.inject(Fault::DelayedCommit).await;
    let old = batch("old", 0, &[b"a", b"b"]);
    assert_eq!(
        log.append(Position(0), &old).await.unwrap(),
        Append::Indeterminate
    );
    let mut state = log.0.session.lock().await;
    state.connection.take().unwrap().close_hard().await.unwrap();
    state.delayed = false;
    drop(state);
    assert!(log.read_from(Position(0), 10).await.unwrap().is_empty());
    let next = batch("new", 0, &[b"replacement"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
    drop(log);
    server.stop().await;
}

#[tokio::test]
async fn dolt_log_contract_native_start_excludes_a_live_predecessor() {
    let (_scratch, mut server, pool, log) = fixture().await;
    log.inject(Fault::DelayedCommit).await;
    let next = batch("predecessor", 0, &[b"pending"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    assert!(matches!(
        DoltLog::open(pool.clone()).await,
        Err(LogError::Indeterminate)
    ));
    log.finish_delayed(true).await;
    drop(log);
    let successor = DoltLog::open(pool).await.unwrap();
    assert_eq!(successor.read_from(Position(0), 10).await.unwrap(), [next]);
    drop(successor);
    server.stop().await;
}

async fn packet(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header).await?;
    let length = u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize;
    let mut bytes = vec![0; 4 + length];
    bytes[..4].copy_from_slice(&header);
    reader.read_exact(&mut bytes[4..]).await?;
    Ok(bytes)
}

#[tokio::test]
async fn dolt_log_contract_native_dropped_commit_reply_recovers_exactly_once() {
    let (_scratch, mut server, pool, first) = fixture().await;
    drop(first);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = pool.connect_options().get_port();
    let once = Arc::new(std::sync::atomic::AtomicBool::new(true));
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
    let source =
        MySqlPoolOptions::new().connect_lazy_with((*pool.connect_options()).clone().port(port));
    let log = DoltLog::open(source).await.unwrap();
    let next = batch("lost-reply", 0, &[b"null", b""]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    tokio::time::timeout(Duration::from_secs(5), dropped.notified())
        .await
        .unwrap();
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap().as_slice(),
        std::slice::from_ref(&next)
    );
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
    drop(log);
    proxy.abort();
    server.stop().await;
}

fn companion_row(sid: &str) -> Companion {
    Companion {
        sql: "INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)".into(),
        binds: vec![sid.into(), "claims".into(), "2026-01-01T00:00:00Z".into()],
    }
}

/// **A companion statement commits with its batch.** The row and the batch are
/// both there after one append.
#[tokio::test]
async fn a_companion_row_commits_with_its_batch() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let next = batch("companion", 0, &[b"a"]);
    assert_eq!(
        log.append_with(Position(0), &next, vec![companion_row("homer")])
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
    server.stop().await;
}

/// **A companion the server refuses leaves neither the row nor the batch**, and
/// the log takes the next append at the same head.
#[tokio::test]
async fn a_refused_companion_leaves_no_batch() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let next = batch("refused", 0, &[b"a"]);
    let broken = Companion {
        sql: "INSERT INTO no_such_table (x) VALUES (?)".into(),
        binds: vec!["1".into()],
    };
    assert_eq!(
        log.append_with(Position(0), &next, vec![companion_row("marge"), broken])
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
    server.stop().await;
}

/// **The import reads every mark with its own taught_at**, including a run with
/// no session card.
#[tokio::test]
async fn the_marks_come_back_with_their_times() {
    let (_scratch, mut server, pool, log) = fixture().await;
    let next = batch("marks", 0, &[b"a"]);
    log.append_with(Position(0), &next, vec![companion_row("homer")])
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
    server.stop().await;
}
