//! **A boot names each pointer the import kept as written, at info.** A claim whose
//! edge names a thing nothing holds is imported with the name as stored, and the
//! boot says so with the table, the row, the column and the value. The level is
//! read back too: the upgrade gate fails a boot on any warning, and this is a state
//! the import tolerates.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use std::sync::{Arc, Mutex};

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, Memory, NewEntity, NewFact, Provenance, kinds,
};

#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
    type Writer = Kept;
    fn make_writer(&'a self) -> Kept {
        self.clone()
    }
}

async fn thing(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_names_a_kept_dangling_pointer_at_info() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-dangling-pointer");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    kinds::seed(&memory).await.expect("the kinds are seeded");
    let marge = thing(&memory, "person:marge", "Marge").await;
    let homer = thing(&memory, "person:homer", "Homer").await;
    let mut friend = NewFact::about(marge, "has a friend", date(2026, 9, 1));
    friend.provenance = Provenance::Testimony;
    friend.edge = Some(Edge::new(EdgeShape::About, homer));
    memory
        .capture(friend)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    sqlx::query("UPDATE fact SET edge_object = 'person:ralph' WHERE content = 'has a friend'")
        .execute(seeding.pool())
        .await
        .expect("the pointer is broken by hand");
    seeding.stop().await;
    drop(memory);
    kinds::load(Vec::<String>::new());

    let kept = Kept::default();
    let guard = tracing::subscriber::set_default(jojobot::logging::logging(
        kept.clone(),
        false,
        jojobot::logging::filter_for(None),
    ));
    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("a pointer at nothing does not stop the boot");
    drop(guard);

    let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
    let line = text
        .lines()
        .find(|line| line.contains("column=edge_object"))
        .unwrap_or_else(|| panic!("no line names the kept pointer:\n{text}"));
    assert!(
        line.contains("INFO"),
        "the kept pointer is logged at info: {line}"
    );
    assert!(
        line.contains("table=\"fact\"") && line.contains("value=person:ralph"),
        "the line names the table and the value: {line}"
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
