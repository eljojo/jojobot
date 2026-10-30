use super::*;

#[tokio::test]
async fn link_rule_write_results_fake_capture_has_the_stored_metadata() {
    contract::write_results::capture_has_the_persisted_links(&InMemoryMemory::booted()).await;
    contract::write_results::edit_replaces_and_clears_the_persisted_links(&InMemoryMemory::booted())
        .await;
    contract::write_results::capture_naming_a_renamed_target_has_the_current_handle(
        &InMemoryMemory::booted(),
    )
    .await;
    contract::write_results::capture_listing_a_handle_twice_has_one_link(&InMemoryMemory::booted())
        .await;
}
