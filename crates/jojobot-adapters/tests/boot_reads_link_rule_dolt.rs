//! **With the link rule on, the boot reader serves a mixed list as the read of one
//! thing does.** A loose word beside a marked id: the fill must compose it to the
//! word and the handle, never serve the raw mark, and agree with the read of the
//! thing on its own.
//!
//! A test binary of its own: the switch is process-wide.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::boot_reader::BootReader;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, LinkRule, Memory, NewEntity, NewFact, set_link_rule};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_reader_and_the_read_of_one_thing_agree_on_a_mixed_list() {
    set_link_rule(LinkRule::On);
    let scratch = Scratch::new("boot-reads-link-rule");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "boot_reads_link_rule")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let store_memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&store_memory)
        .await
        .expect("the kinds are seeded");
    let [holder, alpha] = ["thing:contract-links-holder", "work:contract-links-alpha"]
        .map(|handle| EntityId(handle.into()));
    for (id, name) in [(&holder, "Holder"), (&alpha, "Alpha")] {
        store_memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .expect("add")
            .written()
            .expect("nothing collides");
    }
    store_memory
        .capture(memory::state(NewFact {
            fields: [("labels".to_string(), format!("kitchen, {alpha}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "worded loosely", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");

    let reader = store_memory.boot_reader();
    let reader: &dyn BootReader = &reader;
    let many = reader
        .fields_versioned_many(std::slice::from_ref(&holder))
        .await
        .expect("fields read");
    let alone = store_memory
        .fields_versioned(&holder)
        .await
        .expect("fields of one");
    let got = many.get(&holder).expect("the reader names the holder");
    assert_eq!(got, &alone, "the fill and the read of one thing agree");
    assert!(
        !format!("{got:?}").contains("@#"),
        "no raw mark is served: {got:?}"
    );
    store.stop().await;
}
