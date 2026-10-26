//! Cases of the fake store.

use super::*;

#[tokio::test]
async fn rules_index_counts_only_rules_in_the_fake() {
    contract::rules_index_counts_only_rules(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn rules_index_preserves_authored_settings_in_the_fake() {
    contract::rules_index_preserves_authored_settings(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn fake_holds_a_number_to_one_numbered_member_and_refuses_a_bad_value() {
    contract::a_number_is_held_by_one_member_and_a_bad_value_is_refused(&InMemoryMemory::booted())
        .await;
}

#[tokio::test]
async fn a_claim_on_a_supplied_record_reads_back_against_the_fake() {
    contract::a_claim_on_a_supplied_record_reads_back(&fake_knowing_a_supplied_view()).await;
}
