//! **The in-memory image held to the real store** (decision log 292, 435).
//!
//! The same walk the fake runs in `fold_image_tests`, over a real Dolt store:
//! every kind of write goes through the fold, and after each one every read is
//! asked of the fold and of the store, and the reads the image serves must have
//! made no call to the store.

use std::path::PathBuf;
use std::sync::Arc;

use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::fold_battery::{Probe, walk_every_kind_of_write};
use jojobot_adapters::testing::{free_port, migrated_database};

/// The reads the image serves. Each is added in the commit that switches it.
const SWITCHED: &[&str] = &[
    "list_entities",
    "former_handles",
    "declared_types",
    "declared_kinds",
    "recall",
    "fields",
    "fields_versioned",
    "field_ages",
    "claim_histories",
    "claim_history",
    "scan_entity",
    "scan",
    "history",
    "keys_kept_off_the_thing",
    "key_trails",
    "backing",
    "built_on",
    "referring_to",
    "children",
    "thought_writers",
    "entity_history",
    "write_summary",
];

struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = jojobot_adapters::testing::scratch_path(&format!("jojobot-fold-image-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **A fold over the real store, loaded, with a counter between them.**
async fn loaded(pool: sqlx::MySqlPool) -> (Arc<DoltMemory>, Arc<Counted>, Folded) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let store = Arc::new(DoltMemory::open(pool));
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    (store, counted, folded)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_image_answers_as_the_real_store_does_after_every_kind_of_write() {
    let scratch = Scratch::new("walk");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "walk")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let (store, counted, folded) = loaded(pool).await;
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED,
        kept_off_misses_are_empty: true,
        last_signal: Default::default(),
    };
    walk_every_kind_of_write(&probe).await;
    server.stop().await;
}
