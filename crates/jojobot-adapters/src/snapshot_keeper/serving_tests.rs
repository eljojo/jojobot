use crate::dolt::Dolt;
use crate::dolt_log::DoltLog;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use jojobot_domain::log::Position;

/// **A boot that imports the old teaching rows covers the import with a snapshot,
/// and a quiet boot after it takes none.** The first boot finds a log with no
/// event and one old mark, so it appends the import and snapshots at its head.
/// The second boot opens the same store with nothing to import: its keeper starts
/// from the stored snapshot and publishes no other.
#[tokio::test]
async fn an_import_is_snapshotted_and_a_quiet_second_boot_is_not() {
    let scratch = Scratch::new("serving-snapshots");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
        .bind("homer")
        .bind("claims")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

    let writer = Writer::default();
    let (teachings, snapshots) = crate::teaching_log::serving_kept(&pool, writer.clone())
        .await
        .unwrap();
    let head = snapshots.head().expect("the log holds a head");
    assert!(head > Position(0), "the import appended its event");
    assert_eq!(snapshots.covered(), head, "the import is covered");
    let first = DoltLog::unread(pool.clone())
        .load_snapshot()
        .await
        .unwrap()
        .expect("the snapshot is stored");
    assert_eq!(first.covered_position, head);
    drop((teachings, snapshots));
    writer.drain().await;
    drop(writer);

    let writer = Writer::default();
    let (teachings, snapshots) = crate::teaching_log::serving_kept(&pool, writer.clone())
        .await
        .unwrap();
    assert_eq!(
        snapshots.covered(),
        head,
        "the keeper starts from the store"
    );
    let second = DoltLog::unread(pool.clone())
        .load_snapshot()
        .await
        .unwrap()
        .expect("the snapshot is still stored");
    assert_eq!(second.created_at, first.created_at, "no second snapshot");
    drop((teachings, snapshots));
    writer.drain().await;
    server.stop().await;
}

/// **A stored snapshot the boot refuses is no coverage, so the stop of that boot
/// replaces it.** The snapshot is this build's by fingerprint, but its body no
/// longer matches its checksum, so the boot replays the log. The keeper starts at
/// zero, the shutdown snapshot covers the head with no new write, and the boot
/// after it restores from that one.
#[tokio::test]
async fn a_refused_stored_snapshot_is_replaced_by_the_stop_of_the_boot_that_refused_it() {
    let scratch = Scratch::new("serving-refused-snapshot");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
        .bind("homer")
        .bind("claims")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
    let writer = Writer::default();
    let (teachings, snapshots) = crate::teaching_log::serving_kept(&pool, writer.clone())
        .await
        .unwrap();
    let head = snapshots.head().expect("the log holds a head");
    drop((teachings, snapshots));
    writer.drain().await;
    drop(writer);

    let garbage = b"not a state".to_vec();
    sqlx::query("UPDATE log_snapshot SET body = ?")
        .bind(garbage.clone())
        .execute(&pool)
        .await
        .unwrap();

    let writer = Writer::default();
    let (teachings, snapshots) = crate::teaching_log::serving_kept(&pool, writer.clone())
        .await
        .unwrap();
    assert_eq!(
        snapshots.covered(),
        Position(0),
        "the boot refused the stored snapshot, so it covers nothing"
    );
    snapshots.shutdown().await;
    drop((teachings, snapshots));
    writer.drain().await;
    drop(writer);
    let stored = DoltLog::unread(pool.clone())
        .load_snapshot()
        .await
        .unwrap()
        .expect("a snapshot is stored");
    assert_ne!(
        stored.blob, garbage,
        "the stop replaced the refused snapshot"
    );
    assert_eq!(stored.covered_position, head);

    let writer = Writer::default();
    let (teachings, snapshots) = crate::teaching_log::serving_kept(&pool, writer.clone())
        .await
        .unwrap();
    assert_eq!(snapshots.covered(), head, "the next boot restores from it");
    drop((teachings, snapshots));
    writer.drain().await;
    server.stop().await;
}
