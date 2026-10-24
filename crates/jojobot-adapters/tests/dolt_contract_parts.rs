//! **The shared memory contract, against the real store, one test per part.**
//!
//! The contract is dealt into sixteen parts of neighbouring cases and each part
//! is a test of its own, so a red names its part and no test runs a minute. Four
//! families run it: the bare store, the store under a fold, the store under the
//! search index and the `scan_entity` comparison over what each part leaves
//! behind. Each part starts its own Dolt server and database.

use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::testing::contract as memory;

/// **The step a boot takes after the schema**: write the kinds this build ships.
/// A process that never seeded cannot read a handle at all.
async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

/// **One part of a contract, on a database of its own.**
///
/// The contract is dealt into [`memory::PARTS`] parts of neighbouring cases, and
/// each part is a test of its own that starts its own server and a database that
/// only its own cases fill. **A store per case is ruled out**: the assertions are
/// subset-based — what was captured comes back, never an exact total — because
/// the contract also runs against a shared, pre-populated real collection, and a
/// case handed a store to itself would prove less than the suite is designed to
/// prove. **A shared store per part is the unit**, so every case still runs
/// among the writes of the cases beside it, and a red names its part.
///
/// `run_part` answers how many cases it ran. That must be the part's whole share
/// of the list, `expected`, and not none: a case dropped from the part turns this
/// red, and the parts' shares are held to cover the list by the pure
/// `the_parts_of_every_contract_family_cover_every_case`.
async fn one_part_of_the_contract<F, Fut>(what: &str, part: usize, expected: usize, run_part: F)
where
    F: FnOnce(sqlx::MySqlPool) -> Fut,
    Fut: std::future::Future<Output = usize> + Send + 'static,
{
    let scratch = Scratch::new(&format!("{what}-part{part}"));
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, &format!("{what}part{part}"))
        .await
        .expect("a database of this part's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let ran = tokio::spawn(run_part(pool))
        .await
        .expect("a part of the contract held");
    memory::assert_part_ran(what, part, ran, expected);

    store.stop().await;
}

/// **Every part of every family covers its list once.** Pure: no store.
#[test]
fn the_parts_of_every_contract_family_cover_every_case() {
    memory::parts_cover_every_case(memory::PARTS);
    memory::searchable_parts_cover_every_case(memory::PARTS);
}

/// **The memory contract, against the real store.** One test per part; see
/// [`one_part_of_the_contract`] for what that holds.
async fn memory_contract_part(part: usize) {
    let expected = memory::part_share(memory::case_count(), part);
    one_part_of_the_contract("memory", part, expected, move |pool| async move {
        memory::run_part(&DoltMemory::open(pool), part, memory::PARTS).await
    })
    .await;
}

jojobot_domain::contract_parts!(tokio::test(flavor = "multi_thread", worker_threads = 4), memory_contract_part:
    dolt_satisfies_the_memory_contract_part_00 = 0,
    dolt_satisfies_the_memory_contract_part_01 = 1,
    dolt_satisfies_the_memory_contract_part_02 = 2,
    dolt_satisfies_the_memory_contract_part_03 = 3,
    dolt_satisfies_the_memory_contract_part_04 = 4,
    dolt_satisfies_the_memory_contract_part_05 = 5,
    dolt_satisfies_the_memory_contract_part_06 = 6,
    dolt_satisfies_the_memory_contract_part_07 = 7,
    dolt_satisfies_the_memory_contract_part_08 = 8,
    dolt_satisfies_the_memory_contract_part_09 = 9,
    dolt_satisfies_the_memory_contract_part_10 = 10,
    dolt_satisfies_the_memory_contract_part_11 = 11,
    dolt_satisfies_the_memory_contract_part_12 = 12,
    dolt_satisfies_the_memory_contract_part_13 = 13,
    dolt_satisfies_the_memory_contract_part_14 = 14,
    dolt_satisfies_the_memory_contract_part_15 = 15,
);

/// **The same contract, wrapped in a fold, against the real store.** `Folded`
/// forwards every guard, every read and every write it does not itself
/// refresh straight to Dolt, so this is the regression test for that claim
/// against the store the fold exists to spare, not only against the fake. One
/// test per part.
async fn folded_contract_part(part: usize) {
    let expected = memory::part_share(memory::case_count(), part);
    one_part_of_the_contract("memory_folded", part, expected, move |pool| async move {
        let folded = Folded::new(Arc::new(DoltMemory::open(pool)));
        folded.rebuild().await.expect("the image fills");
        memory::run_part(&folded, part, memory::PARTS).await
    })
    .await;
}

jojobot_domain::contract_parts!(tokio::test(flavor = "multi_thread", worker_threads = 4), folded_contract_part:
    dolt_satisfies_the_memory_contract_when_folded_part_00 = 0,
    dolt_satisfies_the_memory_contract_when_folded_part_01 = 1,
    dolt_satisfies_the_memory_contract_when_folded_part_02 = 2,
    dolt_satisfies_the_memory_contract_when_folded_part_03 = 3,
    dolt_satisfies_the_memory_contract_when_folded_part_04 = 4,
    dolt_satisfies_the_memory_contract_when_folded_part_05 = 5,
    dolt_satisfies_the_memory_contract_when_folded_part_06 = 6,
    dolt_satisfies_the_memory_contract_when_folded_part_07 = 7,
    dolt_satisfies_the_memory_contract_when_folded_part_08 = 8,
    dolt_satisfies_the_memory_contract_when_folded_part_09 = 9,
    dolt_satisfies_the_memory_contract_when_folded_part_10 = 10,
    dolt_satisfies_the_memory_contract_when_folded_part_11 = 11,
    dolt_satisfies_the_memory_contract_when_folded_part_12 = 12,
    dolt_satisfies_the_memory_contract_when_folded_part_13 = 13,
    dolt_satisfies_the_memory_contract_when_folded_part_14 = 14,
    dolt_satisfies_the_memory_contract_when_folded_part_15 = 15,
);

/// **`scan_entity` must answer exactly what `scan` would answer for the same
/// row, over a real corpus rather than a hand-picked one.**
///
/// The contract's own run leaves behind entities with aliases, parents,
/// merges, retractions and folded fields — every shape `scan`'s per-entity
/// loop builds a document out of. Comparing `scan_entity`'s answer against
/// `scan`'s own for every one of them, whole value against whole value, is
/// what catches an override that gets the easy fields right and drops the
/// harder ones — a per-field check would only catch what this case's author
/// thought to name.
async fn scan_entity_part(part: usize) {
    // **Every part's database is compared in full**, as the one database was:
    // the contract runs in its parts, and each part leaves behind the documents
    // its own cases made.
    let expected = memory::part_share(memory::case_count(), part);
    one_part_of_the_contract("scan-entity", part, expected, move |pool| async move {
        let memory = DoltMemory::open(pool);
        let ran = memory::run_part(&memory, part, memory::PARTS).await;

        let whole = memory.scan().await.expect("the full scan answers");
        // The floor of the whole run was fifty documents; each part holds its
        // share, so the parts together hold at least that.
        let floor = SCAN_ENTITY_DOCUMENTS.div_ceil(memory::PARTS);
        assert!(
            whole.len() >= floor,
            "part {part} should have left at least {floor} documents behind, not {}",
            whole.len()
        );
        for doc in &whole {
            let Some(entity) = &doc.entity else { continue };
            let by_id = memory
                .scan_entity(&entity.id)
                .await
                .expect("scan_entity answers for a row scan just found");
            assert_eq!(
                by_id.as_ref(),
                Some(doc),
                "scan_entity disagreed with scan over {}",
                entity.id
            );
        }

        let missing = EntityId("person:contract-nobody".into());
        assert_eq!(
            memory
                .scan_entity(&missing)
                .await
                .expect("the read answers"),
            None,
            "an entity scan never heard of is a miss, not an error",
        );
        ran
    })
    .await;
}

/// The whole run must leave more documents than this behind; each part is held
/// to its share of it.
const SCAN_ENTITY_DOCUMENTS: usize = 50;

jojobot_domain::contract_parts!(tokio::test(flavor = "multi_thread", worker_threads = 4), scan_entity_part:
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_00 = 0,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_01 = 1,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_02 = 2,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_03 = 3,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_04 = 4,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_05 = 5,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_06 = 6,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_07 = 7,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_08 = 8,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_09 = 9,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_10 = 10,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_11 = 11,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_12 = 12,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_13 = 13,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_14 = 14,
    scan_entity_agrees_with_scan_for_every_document_the_real_store_holds_part_15 = 15,
);

/// **…and the same contract including retrieval**, with the search projection
/// over this store. One test per part; see [`one_part_of_the_contract`].
///
/// It is the one thing `run_all` cannot reach: `scan` feeds the projection and
/// no verb a caller calls returns it, so a scan that came back empty would
/// leave every case above green and every search answer wrong. The projection
/// itself is unchanged — it sits above the port and does not care which store
/// answers, which is the claim this case actually tests.
async fn indexed_contract_part(part: usize) {
    let expected = memory::searchable_part_share(part);
    one_part_of_the_contract("memory-indexed", part, expected, move |pool| async move {
        let indexed = Arc::new(
            IndexedMemory::new(Arc::new(DoltMemory::open(pool))).expect("the search index opens"),
        );
        memory::run_part_searchable(
            indexed.as_ref(),
            &Retrieval::new(indexed.index(), vec![indexed.clone()]),
            part,
            memory::PARTS,
        )
        .await
    })
    .await;
}

jojobot_domain::contract_parts!(tokio::test(flavor = "multi_thread", worker_threads = 4), indexed_contract_part:
    the_indexed_dolt_store_satisfies_the_whole_contract_part_00 = 0,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_01 = 1,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_02 = 2,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_03 = 3,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_04 = 4,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_05 = 5,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_06 = 6,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_07 = 7,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_08 = 8,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_09 = 9,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_10 = 10,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_11 = 11,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_12 = 12,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_13 = 13,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_14 = 14,
    the_indexed_dolt_store_satisfies_the_whole_contract_part_15 = 15,
);
