use super::*;

async fn fixture() -> (Scratch, Dolt, DoltMemory) {
    let scratch = Scratch::new("write-link-results");
    let store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "write_link_results")
        .await
        .unwrap();
    migrate::run(&pool).await.unwrap();
    let memory = DoltMemory::open(pool);
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    (scratch, store, memory)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_write_results_dolt_capture_has_the_stored_metadata() {
    let (_scratch, mut store, memory) = fixture().await;
    memory::write_results::capture_has_the_persisted_links(&memory).await;
    store.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn link_rule_write_results_dolt_edit_replaces_and_clears_the_stored_metadata() {
    let (_scratch, mut store, memory) = fixture().await;
    memory::write_results::edit_replaces_and_clears_the_persisted_links(&memory).await;
    store.stop().await;
}
