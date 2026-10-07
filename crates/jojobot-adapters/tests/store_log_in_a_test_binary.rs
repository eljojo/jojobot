//! **A store's refusal reaches the log sink in an integration binary.**
//!
//! The error a caller sees keeps the store's own words out (rule 53), so the log
//! is the only place the cause of a refusal can be read. A test binary that
//! starts a store with no sink installed drops that event. This file is a binary
//! of its own, with no `cfg(test)` of the library under it, which is the shape
//! every integration suite and every other crate's tests have.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, store_log};

#[tokio::test]
async fn a_refused_migration_is_in_the_log_with_the_stores_own_words() {
    let scratch = std::env::temp_dir().join(format!("jojobot-store-log-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let mut store = Dolt::start(&scratch, free_port())
        .await
        .expect("the store comes up");
    sqlx::raw_sql("CREATE TABLE session (id VARCHAR(64) NOT NULL PRIMARY KEY)")
        .execute(store.pool())
        .await
        .expect("the obstruction lands");

    let refused = migrate::run(store.pool()).await;
    assert!(
        refused.is_err(),
        "the obstruction should refuse: {refused:?}"
    );

    let logged = store_log();
    assert!(
        logged.contains("a migration failed") && logged.contains("0001_session"),
        "the refusal names its migration in the log: {logged:?}"
    );
    assert!(
        logged.contains("already exists"),
        "the log carries the store's own words, which the error does not: {logged:?}"
    );

    store.stop().await;
    let _ = std::fs::remove_dir_all(&scratch);
}
