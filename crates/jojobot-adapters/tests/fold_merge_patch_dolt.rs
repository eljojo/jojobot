//! **A merge and a rename over the real store read as much over a large store as
//! over a small one** (work: q-merge-patches-image).
//!
//! Before the image was patched, each of them read every thing in the store
//! again behind the write gate. The same write over a store twice as large must
//! now ask the store for the same number of reads, and none of them is the scan
//! of every document.

use std::path::PathBuf;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity, NewFact, Override, kinds};

struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = jojobot_adapters::testing::scratch_path(&format!("jojobot-merge-patch-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A word of letters that depends on nothing but `n`, spread far enough apart
/// that the write guard's near-miss screen takes no two for one.
fn word(n: usize) -> String {
    let mut n = n * 7919 + 104_729;
    let mut letters = String::new();
    for _ in 0..8 {
        letters.push((b'a' + (n % 26) as u8) as char);
        n = n / 26 * 31 + 17;
    }
    letters
}

/// A fold over a database of `unrelated` things nothing points at, and the two
/// persons a case merges or renames. The persons hold a claim each and a third
/// thing points at the first.
async fn fold_over(unrelated: usize, name: &str) -> (Dolt, Scratch, Arc<Counted>, Folded) {
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let store = Arc::new(DoltMemory::open(pool));
    let persons = [
        EntityId::person("person:alpha"),
        EntityId::person("person:beta"),
    ];
    for person in &persons {
        store
            .add_entity(NewEntity::new(
                person.clone(),
                person.to_string(),
                "user-named",
            ))
            .await
            .expect("an entity is added")
            .written()
            .expect("not blocked");
        store
            .capture(NewFact::about(person.clone(), "a claim", date(2026, 9, 1)))
            .await
            .expect("a claim lands")
            .written()
            .expect("not blocked");
    }
    for n in 0..unrelated {
        store
            .add_entity(NewEntity::new(
                EntityId::new(EntityKind::THING, word(n)),
                word(n + 1000),
                "user-named",
            ))
            .await
            .expect("an entity is added")
            .written()
            .expect("not blocked");
    }
    let counted = Arc::new(Counted::new(store));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold fills");
    (server, scratch, counted, folded)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_reads_the_real_store_as_much_over_a_large_store_as_over_a_small_one() {
    let mut reads = Vec::new();
    for (unrelated, name) in [(30, "merge-small"), (60, "merge-large")] {
        let (mut server, _scratch, counted, folded) = fold_over(unrelated, name).await;
        counted.reset();
        folded
            .merge(
                &EntityId::person("person:beta"),
                &EntityId::person("person:alpha"),
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("the merge lands");
        reads.push((unrelated, counted.calls(), counted.seen()));
        server.stop().await;
    }
    assert_eq!(
        reads[0].1, reads[1].1,
        "the merge's reads grew with the store: {reads:?}"
    );
    for (unrelated, _, seen) in &reads {
        assert_eq!(
            seen.get("scan"),
            None,
            "a merge over {unrelated} things scanned every document: {seen:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rename_reads_the_real_store_as_much_over_a_large_store_as_over_a_small_one() {
    let mut reads = Vec::new();
    for (unrelated, name) in [(30, "rename-small"), (60, "rename-large")] {
        let (mut server, _scratch, counted, folded) = fold_over(unrelated, name).await;
        counted.reset();
        folded
            .rename_entity(
                &EntityId::person("person:beta"),
                &EntityId::person("person:gamma"),
                None,
                date(2026, 9, 2),
                Override::none(),
                None,
            )
            .await
            .expect("the rename lands")
            .written()
            .expect("not blocked");
        reads.push((unrelated, counted.calls(), counted.seen()));
        server.stop().await;
    }
    assert_eq!(
        reads[0].1, reads[1].1,
        "the rename's reads grew with the store: {reads:?}"
    );
    for (unrelated, _, seen) in &reads {
        assert_eq!(
            seen.get("scan"),
            None,
            "a rename over {unrelated} things scanned every document: {seen:?}"
        );
    }
}
