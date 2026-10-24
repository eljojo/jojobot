//! **The searchable contract against the fake, one test per part**, each part on
//! an index of its own, so a read costs what its own part holds and not what
//! every case wrote.

use std::sync::Arc;

use jojobot_domain::memory::testing::{InMemoryMemory, contract};

use crate::search::{IndexedMemory, Retrieval};

async fn fake_searchable_contract_part(part: usize) {
    let store =
        Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens"));
    let ran = contract::run_part_searchable(
        store.as_ref(),
        &Retrieval::new(store.index(), vec![store.clone()]),
        part,
        contract::PARTS,
    )
    .await;
    contract::assert_part_ran(
        "the searchable contract",
        part,
        ran,
        contract::searchable_part_share(part),
    );
}

jojobot_domain::contract_parts!(tokio::test, fake_searchable_contract_part:
    the_contract_holds_over_the_fake_part_00 = 0,
    the_contract_holds_over_the_fake_part_01 = 1,
    the_contract_holds_over_the_fake_part_02 = 2,
    the_contract_holds_over_the_fake_part_03 = 3,
    the_contract_holds_over_the_fake_part_04 = 4,
    the_contract_holds_over_the_fake_part_05 = 5,
    the_contract_holds_over_the_fake_part_06 = 6,
    the_contract_holds_over_the_fake_part_07 = 7,
    the_contract_holds_over_the_fake_part_08 = 8,
    the_contract_holds_over_the_fake_part_09 = 9,
    the_contract_holds_over_the_fake_part_10 = 10,
    the_contract_holds_over_the_fake_part_11 = 11,
    the_contract_holds_over_the_fake_part_12 = 12,
    the_contract_holds_over_the_fake_part_13 = 13,
    the_contract_holds_over_the_fake_part_14 = 14,
    the_contract_holds_over_the_fake_part_15 = 15,
);

/// **The searchable contract's parts cover every case once**, retrieval
/// included.
#[test]
fn the_parts_of_the_searchable_contract_cover_every_case() {
    contract::searchable_parts_cover_every_case(contract::PARTS);
}
