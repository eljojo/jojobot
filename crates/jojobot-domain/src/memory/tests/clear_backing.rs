use super::*;

#[tokio::test]
async fn clear_backing_preserves_the_newer_setting_on_the_fake() {
    contract::a_clear_on_an_older_claim_leaves_the_value_and_one_on_the_backing_claim_takes_the_key_off(
        &InMemoryMemory::booted(),
    )
    .await;
}
