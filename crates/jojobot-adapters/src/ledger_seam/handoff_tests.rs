//! **One boot serves every family.** The seam boots from the snapshot and its tail, and the
//! teaching ledger takes its half of that boot instead of booting again. These cases hold the
//! handoff to what it promises: no second boot, the State handed over at the position it was
//! restored at, and a cursor that starts at the boot's head and not at zero.

use std::sync::Arc;

use jojobot_domain::log::{BatchId, LogStore, SealedBatch};
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::Teachings;
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

use crate::dolt::Dolt;
use crate::teaching_log::LoggedTeachings;
use crate::testing::{Scratch, free_port, imported_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

/// **A teaching event that lands between the boot and the handoff is not lost.** The seam's own
/// cursor moves with every later commit and recovery; the State the boot restored does not. A
/// State paired with the cursor after the event would start beyond an event it never applied,
/// and a mark would be missing until the next restart.
#[tokio::test]
async fn a_teaching_event_between_the_boot_and_the_handoff_is_not_lost() {
    let scratch = Scratch::new("seam-handoff-between");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let log = Arc::new(
        WriterLog::open(store.clone(), writer.clone())
            .await
            .unwrap(),
    );
    let seam = writer.seam().expect("opening the log opened the seam");

    // A mark lands after the boot, and the seam's cursor passes it.
    let head = log.head().expect("the log is settled");
    let now = jiff::Timestamp::now();
    let landed = SealedBatch::seal(
        BatchId("teaching:between".into()),
        head,
        now,
        1,
        vec![crate::teaching_log::codec::encode(
            &TeachingEvent::Recorded {
                run: RunId("homer".into()),
                domain: "mail".into(),
                taught_at: now,
            },
        )],
    )
    .unwrap();
    log.append(head, &landed).await.unwrap();
    seam.catch_up().await.unwrap();

    let ledger = LoggedTeachings::open_from(log.clone(), &[], seam.take_teaching())
        .await
        .unwrap();
    assert!(
        ledger
            .has_contact(&Sid("homer".into()), "mail")
            .await
            .unwrap(),
        "the mark that landed after the boot is missing from the ledger the handoff opened"
    );
    drop(ledger);
    drop(log);
    writer.drain().await;
    server.stop().await;
}

/// **The process boots once.** A ledger handed the boot's State makes no boot of its own; one
/// opened with nothing handed over makes exactly one, so the count is not a number that stays at
/// zero whatever happens.
#[tokio::test]
async fn the_serving_boot_makes_one_boot_and_the_teaching_ledger_none() {
    let scratch = Scratch::new("seam-handoff-once");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let served = crate::teaching_log::serving(&store, writer.clone())
        .await
        .unwrap();
    assert_eq!(
        served.boots_made(),
        0,
        "the serving boot booted the teaching ledger a second time"
    );

    let log = Arc::new(
        WriterLog::open(store.clone(), writer.clone())
            .await
            .unwrap(),
    );
    let alone = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
    assert_eq!(
        alone.boots_made(),
        1,
        "a ledger with nothing handed over did not boot on its own"
    );
    drop((served, alone, log));
    writer.drain().await;
    server.stop().await;
}

/// **A boot over a snapshot knows the log holds ledger events, and does not import the old tables
/// again.** The ledger State evicts, so it cannot say whether the log ever held a ledger event;
/// the projection State carries that fact, and the seam reads it from the State it restored. A
/// seam that took it as unknown would fill the ledgers from the old tables a second time into a
/// log that already holds their import.
#[tokio::test]
async fn a_boot_over_a_snapshot_does_not_import_the_old_tables_again() {
    use jojobot_domain::log::Position;
    let scratch = Scratch::new("seam-seen-snapshot");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    sqlx::query(
        "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, written_at) \
         VALUES ('bot:homer', 'alpha', 'capture', 'd', '{}', '2026-11-01T00:00:00.000000000Z')",
    )
    .execute(&store)
    .await
    .expect("an old ledger row the first open imports");
    let counted = |writer: &Writer| {
        let log = writer.shared_log(&store);
        async move { log.read_from(Position(0), 10_000).await.unwrap().len() }
    };
    let probe = Writer::default();
    let before = counted(&probe).await;

    // The first process opens, imports the row, and takes a snapshot that covers the import.
    let first = Writer::default();
    {
        let log = WriterLog::open(store.clone(), first.clone()).await.unwrap();
        crate::log_snapshot::take_snapshot(&log, jiff::Timestamp::now())
            .await
            .unwrap();
    }
    let imported = counted(&first).await;
    assert!(
        imported > before,
        "the first open did not import the old row, so the case proves nothing"
    );
    first.drain().await;
    drop(first);

    // The second process boots from the snapshot over the same, still populated, old tables.
    let second = Writer::default();
    let pool = crate::writer::sql::Pool::from(store.clone());
    let seam = {
        let (opener, pool) = (second.clone(), pool);
        second
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens over the snapshot")
    };
    assert!(
        seam.boot_report().from_snapshot,
        "the boot did not start from the snapshot"
    );
    assert_eq!(
        counted(&second).await,
        imported,
        "the boot imported the old tables into a log that already holds their import"
    );
    second.drain().await;
    server.stop().await;
}
