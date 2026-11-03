//! **The contract against the fake, one test per part**, each part on a store of
//! its own: a read through the fake costs as much as the store is large, so one
//! store holding every case's rows is the slow way to run them.

use crate::memory::testing::{InMemoryMemory, contract};

async fn fake_contract_part(part: usize) {
    let ran = contract::run_part(&InMemoryMemory::booted(), part, contract::PARTS).await;
    contract::assert_part_ran(
        "the fake",
        part,
        ran,
        contract::part_share(contract::case_count(), part),
    );
}

crate::contract_parts!(tokio::test, fake_contract_part:
    fake_satisfies_the_contract_part_00 = 0,
    fake_satisfies_the_contract_part_01 = 1,
    fake_satisfies_the_contract_part_02 = 2,
    fake_satisfies_the_contract_part_03 = 3,
    fake_satisfies_the_contract_part_04 = 4,
    fake_satisfies_the_contract_part_05 = 5,
    fake_satisfies_the_contract_part_06 = 6,
    fake_satisfies_the_contract_part_07 = 7,
    fake_satisfies_the_contract_part_08 = 8,
    fake_satisfies_the_contract_part_09 = 9,
    fake_satisfies_the_contract_part_10 = 10,
    fake_satisfies_the_contract_part_11 = 11,
    fake_satisfies_the_contract_part_12 = 12,
    fake_satisfies_the_contract_part_13 = 13,
    fake_satisfies_the_contract_part_14 = 14,
    fake_satisfies_the_contract_part_15 = 15,
);

/// **The contract's parts cover every case once.** Every part also has something
/// to run, or a part is a store started for nothing.
#[test]
fn the_parts_of_the_contract_cover_every_case() {
    contract::parts_cover_every_case(contract::PARTS);
}

/// **A rewrite of a claim a newer claim displaced is refused**, on the fake. The
/// real store runs the same case in the adapters' tests.
#[tokio::test]
async fn the_fake_refuses_a_rewrite_of_a_claim_a_newer_claim_displaced() {
    let store = InMemoryMemory::booted();
    contract::a_rewrite_of_a_claim_a_newer_claim_displaced_is_refused(&store).await;
    contract::a_rewrite_naming_the_keys_it_must_back_is_refused_without_a_revision(&store).await;
}

/// **A capture with two missing names is told the first in the decided order**,
/// on the fake: the subject, then the edge's object, then the refs. The real store
/// runs the same cases in the adapters' tests.
#[tokio::test]
async fn the_fake_judges_the_subject_the_edge_and_the_refs_in_the_decided_order() {
    let store = InMemoryMemory::booted();
    contract::a_missing_subject_is_refused_before_a_missing_edge_object(&store).await;
    contract::a_missing_edge_object_is_refused_before_a_missing_ref(&store).await;
    contract::a_setting_with_no_value_is_refused_before_a_missing_subject(&store).await;
    contract::a_thought_over_its_cap_is_refused_before_a_full_room(&store).await;
    contract::a_full_room_is_refused_before_a_full_thread(&store).await;
    contract::a_missing_source_is_refused_before_a_held_role(&store).await;
}

/// **The fake judges a reference field's value before the refs, and the decided
/// order is the other way round.** This pins what the fake does today: the case
/// the real store passes reddens here. The admission that moves in 9.3.3-c2 makes
/// the fake follow the decided order; that change removes `should_panic` and runs
/// the case on the fake like the others.
#[tokio::test]
#[should_panic(expected = "the refs are judged before a reference field's value")]
async fn the_fake_still_judges_a_reference_field_before_the_refs() {
    let store = InMemoryMemory::booted();
    contract::a_missing_ref_is_refused_before_a_missing_reference_field(&store).await;
}
