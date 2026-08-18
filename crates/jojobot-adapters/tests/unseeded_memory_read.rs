//! **What a caller meets when the memory rail READS on a process that loaded
//! no kinds.**
//!
//! The set of kinds is a declaration in the store, loaded at the boot. `main`
//! treats a boot that could not load it as survivable: it logs and serves
//! anyway, because refusing to start over an unreachable store is worse than
//! serving refusals somebody can read. So the store can hold records while this
//! process holds no set — and every read then meets a stored handle it cannot
//! resolve.
//!
//! **The write half of this rail is already covered elsewhere**, through the
//! one validator both rails share, so it is not repeated here. The read half
//! has a resolution of its own: it parses the kind out of a stored id, and the
//! reason it failed was discarded there. A caller was told the RECORD could not
//! be read — which names a repair only a person can perform, on a store that is
//! undamaged — while the repair is a boot (rule 68).
//!
//! **A test binary of its own, because the set is process-wide.** Any case
//! running beside this one would load it, and this one would then be asserting
//! about a state it is not in.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_read_on_an_unloaded_process_says_so_rather_than_blaming_the_record() {
    let path = std::env::temp_dir().join(format!("jojobot-unseeded-read-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let mut server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    let store = DoltMemory::open(server.pool().clone());

    // **A store with a record in it**, written the way any other process writes
    // one: the set is loaded for the write and emptied afterwards. That is the
    // state a process is in when the store was unreachable at its boot and is
    // reachable by the time a caller asks — records on one side, no set on the
    // other.
    kinds::seed(&store).await.expect("the kinds are declared");
    let handle = EntityId("person:bart".into());
    store
        .add_entity(NewEntity::new(handle.clone(), "Bart", "test"))
        .await
        .expect("the entity is written");
    kinds::load::<[&str; 0], &str>([]);

    let refused = store
        .list_entities(None)
        .await
        .expect_err("a process that cannot read a handle cannot read the records carrying one");

    let said = refused.to_string();
    assert!(
        said.contains("never loaded"),
        "the refusal names the failure a caller can act on — nothing seeded this process: {said}",
    );
    // **And it does not blame the stored record.** That sentence sends a reader
    // after damage in a store that holds none, and its repair — a person — is
    // not the repair this needs.
    assert!(
        !said.contains("could not be read"),
        "an unloaded process does not report its own state as a damaged record: {said}",
    );
    // **And it recites no kinds**, because the set is data (rule 213): a
    // refusal naming the shipped ten tells an instance holding an eleventh
    // that its own kind does not exist. **Every shipped kind except the one
    // the stored handle carries** — `person` is in the answer because the
    // record names it, and a check that counted that would be measuring the
    // fixture rather than the sentence.
    for shipped in kinds::SHIPPED
        .into_iter()
        .filter(|kind| *kind != handle.kind_token())
    {
        assert!(
            !said
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == shipped),
            "the refusal recites '{shipped}' as though the kinds were a closed list: {said}",
        );
    }

    server.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
