//! **A type an older build stored with a describing key still answers on the
//! head** (rules 342 and 466). The deployed build let a caller declare a type
//! that named one of the keys that label a claim; the head refuses that
//! declaration, and must still boot over the row and answer the reads the type
//! answered.

use super::*;

/// **The head boots cleanly over a stored `starred` counter and `subject` text,
/// and answers by that type**: `recall` and `search` name the key the thing
/// lacks instead of refusing the type, and a clean stored type fits the thing in
/// the same case.
#[tokio::test]
async fn a_stored_type_naming_a_describing_key_boots_cleanly_and_answers_on_the_head() {
    use jojobot_adapters::dolt::memory::DoltMemory;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_8_FIXTURE_DIR, "stored-describing-key").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool().clone();
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    // The rows a build that did not yet refuse the keys wrote: a counter on
    // `starred`, text under `subject`, and a clean type beside them.
    for (name, key, holds, folds) in [
        ("legacy-starry", "starred", "number", "sum"),
        ("legacy-pantry", "subject", "text", "newest"),
        ("legacy-clean", "donuts", "number", "newest"),
    ] {
        sqlx::query(
            "INSERT INTO type_field (type_name, key_name, ordinal, holds, origin, folds, owner, \
             required) VALUES (?, ?, 1, ?, 'declared', ?, 'type', 0)",
        )
        .bind(name)
        .bind(key)
        .bind(holds)
        .bind(folds)
        .execute(&pool)
        .await
        .expect("the stored type is planted");
    }
    sqlx::query(
        "INSERT INTO type_field (type_name, key_name, ordinal, holds, origin, folds, owner, \
         required) VALUES ('legacy-pantry', 'donuts', 2, 'number', 'declared', 'newest', 'type', 0)",
    )
    .execute(&pool)
    .await
    .expect("the pantry's plain key is planted");
    let tau = EntityId("thing:tau".into());
    memory
        .add_entity(NewEntity::new(tau.clone(), "Tau", "test"))
        .await
        .expect("tau is written")
        .written()
        .expect("nothing collides");
    memory
        .capture(NewFact {
            fields: [("donuts".to_string(), "2".to_string())]
                .into_iter()
                .collect(),
            sets: ["donuts".to_string()].into_iter().collect(),
            ..NewFact::about(tau, "ate two", "2026-08-01".parse().expect("a day"))
        })
        .await
        .expect("tau holds donuts");
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let recalled = surface
        .call("recall", json!({"answers_type": "legacy-pantry"}))
        .await;
    let searched = surface
        .call("search", json!({"answers_type": "legacy-pantry"}))
        .await;
    let fitted = surface
        .call("search", json!({"fits_type": "legacy-clean"}))
        .await;
    let starry = surface
        .call("recall", json!({"answers_type": "legacy-starry"}))
        .await;
    booted.stop().await;
    for (what, body) in [
        ("recall answers_type", &recalled),
        ("search answers_type", &searched),
        ("search fits_type", &fitted),
    ] {
        assert!(
            !body.contains("\"status\":\"blocked\"") && body.contains("thing:tau"),
            "recorded at {git_ref}: {what} over a stored type answers: {body}"
        );
    }
    assert!(
        recalled.contains("subject"),
        "recorded at {git_ref}: the stored pantry names the key the thing lacks: {recalled}"
    );
    assert!(
        !starry.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: a stored counter on a describing key still answers: {starry}"
    );
    drop(scratch);
}
