//! **The memory contract through a fold that was never filled, one test per
//! part**, each part on a store of its own.

use std::sync::Arc;

use jojobot_adapters::fold::Folded;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::testing::contract;

async fn folded_fake_contract_part(part: usize) {
    let ran = contract::run_part(
        &Folded::new(Arc::new(InMemoryMemory::booted())),
        part,
        contract::PARTS,
    )
    .await;
    contract::assert_part_ran(
        "the folded fake",
        part,
        ran,
        contract::part_share(contract::case_count(), part),
    );
}

jojobot_domain::contract_parts!(tokio::test, folded_fake_contract_part:
    fake_satisfies_the_contract_when_folded_part_00 = 0,
    fake_satisfies_the_contract_when_folded_part_01 = 1,
    fake_satisfies_the_contract_when_folded_part_02 = 2,
    fake_satisfies_the_contract_when_folded_part_03 = 3,
    fake_satisfies_the_contract_when_folded_part_04 = 4,
    fake_satisfies_the_contract_when_folded_part_05 = 5,
    fake_satisfies_the_contract_when_folded_part_06 = 6,
    fake_satisfies_the_contract_when_folded_part_07 = 7,
    fake_satisfies_the_contract_when_folded_part_08 = 8,
    fake_satisfies_the_contract_when_folded_part_09 = 9,
    fake_satisfies_the_contract_when_folded_part_10 = 10,
    fake_satisfies_the_contract_when_folded_part_11 = 11,
    fake_satisfies_the_contract_when_folded_part_12 = 12,
    fake_satisfies_the_contract_when_folded_part_13 = 13,
    fake_satisfies_the_contract_when_folded_part_14 = 14,
    fake_satisfies_the_contract_when_folded_part_15 = 15,
);
