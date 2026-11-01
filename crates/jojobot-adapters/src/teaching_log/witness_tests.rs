//! **No read of the old teaching rows after boot.** A recording proxy sits in
//! front of a real Dolt. The ledger boots through it, then a contact, a repeat
//! contact, a check, a clear and a recovery after a lost acknowledgement are
//! made with the shadow on. Any statement that reads `session_teaching` fails
//! the case.

use std::sync::{Arc, Mutex};

use jojobot_domain::log::testing::{Fault, FaultLog};
use sqlx::mysql::MySqlPoolOptions;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::tests::{at, sid};
use super::*;
use crate::dolt::Dolt;
use crate::dolt::teaching::DoltTeachings;
use crate::dolt_log::DoltLog;
use crate::testing::{Scratch, free_port, migrated_database};

type Seen = Arc<Mutex<Vec<String>>>;

async fn packet(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header).await?;
    let length = u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize;
    let mut bytes = vec![0; 4 + length];
    bytes[..4].copy_from_slice(&header);
    reader.read_exact(&mut bytes[4..]).await?;
    Ok(bytes)
}

/// Forward every connection to `upstream` and record the text of each query or
/// prepared statement the client sends.
fn recording_proxy(listener: tokio::net::TcpListener, upstream: u16, seen: Seen) {
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let seen = seen.clone();
            tokio::spawn(async move {
                let Ok(server) = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await
                else {
                    return;
                };
                let (mut from_client, mut to_client) = client.into_split();
                let (mut from_server, mut to_server) = server.into_split();
                let requests = async move {
                    loop {
                        let bytes = packet(&mut from_client).await?;
                        // 0x03 is COM_QUERY and 0x16 is COM_STMT_PREPARE.
                        if matches!(bytes.get(4), Some(0x03 | 0x16)) {
                            seen.lock()
                                .unwrap()
                                .push(String::from_utf8_lossy(&bytes[5..]).into_owned());
                        }
                        to_server.write_all(&bytes).await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<(), std::io::Error>(())
                };
                let replies = async move {
                    tokio::io::copy(&mut from_server, &mut to_client).await?;
                    Ok::<(), std::io::Error>(())
                };
                let _ = tokio::try_join!(requests, replies);
            });
        }
    });
}

/// **The options of every connection made through the proxy.** The proxy records
/// the text of a query or a preparation and not an execution by statement id, so a
/// statement the client cached would run again unseen. With no statement cache
/// every run of a statement is prepared again, and the proxy sees its text.
fn through_proxy(direct: &sqlx::MySqlPool, port: u16) -> sqlx::mysql::MySqlConnectOptions {
    (*direct.connect_options())
        .clone()
        .port(port)
        .statement_cache_capacity(0)
}

fn reads_old_rows(statement: &str) -> bool {
    let lower = statement.to_ascii_lowercase();
    lower.contains("session_teaching") && lower.contains("select")
}

/// **The witness sees a statement that runs a second time on the same connection.**
/// A SELECT is run, the record is cleared, and the same SELECT runs again on the
/// same connection. Were the statement cached, the second run would send a
/// statement id and no text, and the witness would miss a repeated boot read.
#[tokio::test]
async fn the_witness_sees_a_repeated_select_on_one_connection() {
    let scratch = Scratch::new("teaching-witness-calibration");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let direct = migrated_database(&server, "alpha").await.unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = Arc::default();
    recording_proxy(listener, direct.connect_options().get_port(), seen.clone());
    let through = MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy_with(through_proxy(&direct, port));

    let select = "SELECT sid FROM session_teaching WHERE domain = ?";
    for _ in 0..2 {
        seen.lock().unwrap().clear();
        sqlx::query(select)
            .bind("claims")
            .fetch_all(&through)
            .await
            .unwrap();
        let statements = seen.lock().unwrap().clone();
        assert!(
            statements.iter().any(|s| reads_old_rows(s)),
            "a run of the SELECT was not seen: {statements:?}"
        );
    }
    drop(through);
    server.stop().await;
}

#[tokio::test]
async fn no_legacy_teaching_read_is_made_after_boot() {
    let scratch = Scratch::new("teaching-no-other-read");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let direct = migrated_database(&server, "alpha").await.unwrap();
    sqlx::query(
        "INSERT INTO session_teaching (sid, domain, taught_at) VALUES ('homer', 'claims', '2026-01-01T00:00:00Z')",
    )
    .execute(&direct)
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = Arc::default();
    recording_proxy(listener, direct.connect_options().get_port(), seen.clone());
    let through = MySqlPoolOptions::new().connect_lazy_with(through_proxy(&direct, port));

    let log = Arc::new(DoltLog::open(through.clone()).await.unwrap());
    let marks = DoltTeachings::open(through).marks().await.unwrap();
    let report = Arc::new(crate::fold::shadow::Shadow::named("teachings").unwrap());
    let teachings = LoggedTeachings::open_shadowed(log.clone(), &marks, report.clone())
        .await
        .unwrap();
    assert_eq!(marks.len(), 1, "boot read the old row for the import");
    seen.lock().unwrap().clear();

    assert!(
        teachings
            .first_contact(&sid("marge"), "mail", at(1))
            .await
            .unwrap()
    );
    assert!(
        !teachings
            .first_contact(&sid("marge"), "mail", at(2))
            .await
            .unwrap()
    );
    assert!(
        teachings
            .has_contact(&sid("homer"), "claims")
            .await
            .unwrap()
    );
    log.inject(Fault::LostAcknowledgement).await;
    assert!(
        teachings
            .first_contact(&sid("bart"), "mail", at(3))
            .await
            .is_err()
    );
    // The next call recovers from the log's tail and finds the lost commit.
    assert!(
        !teachings
            .first_contact(&sid("bart"), "mail", at(4))
            .await
            .unwrap()
    );
    teachings.forget(&sid("marge")).await.unwrap();
    assert!(!teachings.has_contact(&sid("marge"), "mail").await.unwrap());

    let statements = seen.lock().unwrap().clone();
    assert!(
        statements
            .iter()
            .any(|s| s.contains("INSERT INTO event_log")),
        "the proxy saw the log's writes: {statements:?}"
    );
    let offending: Vec<_> = statements.iter().filter(|s| reads_old_rows(s)).collect();
    assert!(
        offending.is_empty(),
        "legacy reads after boot: {offending:?}"
    );
    assert_eq!(report.counts()[0].disagreements, 0);
    drop(teachings);
    drop(log);
    server.stop().await;
}

/// **A rollback to the old path and a roll forward, on a real store.** The ledger
/// runs and is dropped, the old path writes one mark and clears one run in the
/// same database, and the next boot opens a log that is not empty. The log is
/// brought to the rows, a first contact on a pair the rows hold answers false
/// instead of failing on the primary key, and a run the old path cleared is
/// taught again.
#[tokio::test]
async fn a_rollback_and_roll_forward_on_a_real_store_reconciles_and_writes() {
    let scratch = Scratch::new("teaching-roll-forward");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let report = Arc::new(crate::fold::shadow::Shadow::named("teachings").unwrap());

    let log = Arc::new(DoltLog::open(pool.clone()).await.unwrap());
    let marks = DoltTeachings::open(pool.clone()).marks().await.unwrap();
    let running = LoggedTeachings::open_shadowed(log.clone(), &marks, report.clone())
        .await
        .unwrap();
    for (run, at_second) in [("marge", 1), ("bart", 2)] {
        assert!(
            running
                .first_contact(&sid(run), "claims", at(at_second))
                .await
                .unwrap()
        );
    }
    drop(running);
    drop(log);

    // The old path, with the log away: it writes and clears rows only.
    let old = DoltTeachings::open(pool.clone());
    assert!(
        old.first_contact(&sid("homer"), "mail", at(5))
            .await
            .unwrap()
    );
    old.forget(&sid("marge")).await.unwrap();

    let log = Arc::new(DoltLog::open(pool.clone()).await.unwrap());
    let marks = old.marks().await.unwrap();
    let again = LoggedTeachings::open_shadowed(log.clone(), &marks, report.clone())
        .await
        .unwrap();
    assert_eq!(
        again.taught().await.unwrap().unwrap(),
        vec![
            (sid("bart"), "claims".to_string()),
            (sid("homer"), "mail".to_string())
        ],
        "the log holds what the rows hold"
    );
    assert!(
        !again
            .first_contact(&sid("homer"), "mail", at(6))
            .await
            .expect("a pair the rows hold is not a second insert"),
    );
    assert!(
        again
            .first_contact(&sid("marge"), "claims", at(7))
            .await
            .expect("a run the old path cleared is taught again"),
    );
    let rows: Vec<(String, String)> = old
        .marks()
        .await
        .unwrap()
        .into_iter()
        .map(|mark| (mark.sid.0, mark.domain))
        .collect();
    assert_eq!(
        rows,
        [
            ("bart".to_string(), "claims".to_string()),
            ("homer".to_string(), "mail".to_string()),
            ("marge".to_string(), "claims".to_string()),
        ],
        "the rows follow the log again"
    );
    assert_eq!(report.counts()[0].disagreements, 0);
    drop(again);
    drop(log);
    server.stop().await;
}
