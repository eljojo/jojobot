//! **A boot that ran the claims import covers it with a snapshot before it
//! serves**, so a process that stops with no orderly stop does not leave the next
//! boot to replay the import. A boot whose import was already done takes no such
//! snapshot: the log holds nothing new for it to cover.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::Position;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

/// A store an earlier build left: one entity with one claim, nothing in the log.
async fn old_store(label: &str) -> (std::path::PathBuf, u16, Scratch) {
    let path = jojobot_adapters::testing::scratch_path(label);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();
    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let homer = EntityId("person:homer".to_string());
    memory
        .add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact::about(homer, "likes donuts", date(2026, 9, 2)))
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    seeding.stop().await;
    drop(memory);
    (path, port, scratch)
}

/// The last position a batch of the claims import covers. The import names its
/// batches by a prefix of their id, which the stored batch carries as text.
async fn import_ends_at(pool: &sqlx::MySqlPool) -> Position {
    let rows: Vec<(u64, Vec<u8>)> = sqlx::query_as(
        "SELECT last_position, payload FROM event_log WHERE first_position IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .expect("the log is readable");
    let ends = rows
        .into_iter()
        .filter(|(_, payload)| String::from_utf8_lossy(payload).contains("claims-import:"))
        .map(|(last, _)| last)
        .max()
        .expect("the log holds a batch of the import");
    Position(ends)
}

const CHILD: &str = "first_boot_in_a_process_of_its_own";
const CHILD_PATH: &str = "JOJOBOT_FIRST_BOOT_PATH";
const CHILD_PORT: &str = "JOJOBOT_FIRST_BOOT_PORT";

/// **The first boot, run in a process of its own** by the case below: a process
/// serves one store, so two boots of one store are two processes. It does nothing
/// when the case did not start it. It writes `head covered imported` beside the
/// store for the case to read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn first_boot_in_a_process_of_its_own() {
    let (Ok(path), Ok(port)) = (std::env::var(CHILD_PATH), std::env::var(CHILD_PORT)) else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let booted = jojobot::wiring::boot_store(&path, port.parse().expect("a port"), Clock::Real)
        .await
        .expect("the first boot stands");
    let head = booted.snapshots.head().expect("the log holds a head");
    let imported = import_ends_at(booted.store.pool()).await;
    let seen = format!("{} {} {}", head.0, booted.snapshots.covered().0, imported.0);
    let mut store = booted.store;
    store.stop().await;
    std::fs::write(path.join("first-boot.txt"), seen).expect("the child's account");
}

/// **The boot that imports the claims leaves a snapshot that covers the import,
/// and a boot whose import was done takes none.** The first boot of a store with
/// rows is the one that imports them; the snapshot the teaching ledger takes at its
/// own import is of a log that holds nothing yet.
///
/// The first boot runs in a child process. The snapshot slot is emptied between
/// the two boots, so a snapshot after the second one could only have been taken by
/// that boot, and there is none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_covers_the_claims_import_with_a_snapshot_and_a_later_boot_takes_none() {
    let (path, port, scratch) = old_store("jojobot-boot-snapshots-after-import").await;

    let child = std::process::Command::new(std::env::current_exe().expect("this binary"))
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(CHILD_PATH, &path)
        .env(CHILD_PORT, port.to_string())
        .output()
        .expect("the child runs");
    assert!(
        child.status.success(),
        "the first boot failed:\n{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let seen = std::fs::read_to_string(path.join("first-boot.txt")).expect("the child's account");
    let [head, covered, imported] = seen
        .split_whitespace()
        .map(|n| Position(n.parse().expect("a position")))
        .collect::<Vec<_>>()[..]
    else {
        panic!("the child's account is malformed: {seen}");
    };
    assert!(
        head > Position(0),
        "the import appended events, or the case proves nothing: {head:?}"
    );
    // The records the build supplies are bound right after the import, so the head
    // is past the import's last batch; the snapshot has to reach at least that far.
    assert!(
        covered >= imported,
        "the snapshot the first boot left covers the import: covered {covered:?}, import ends at {imported:?}"
    );

    let mut store = Dolt::start(&path, port).await.expect("the store comes up");
    sqlx::query("DELETE FROM event_log WHERE slot = 'snapshot'")
        .execute(store.pool())
        .await
        .expect("the snapshot slot is cleared");
    store.stop().await;

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the second boot stands");
    assert!(
        booted.snapshots.head().expect("the log holds a head") >= head,
        "the second boot found the log the first one filled"
    );
    assert_eq!(
        booted.snapshots.covered(),
        Position(0),
        "a boot whose import was already done takes no snapshot"
    );
    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
