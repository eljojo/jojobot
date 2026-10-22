//! **An app's attempt is spent in the transaction of the write it records, in
//! the real store.**
//!
//! The fake spends under its own lock, so it cannot show that the attempt's row
//! and the event land together, or that calls racing on one attempt land one
//! event. This does, against Dolt, and runs the app contract through the plain
//! store and through the fold.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::app::{
    AppOutcome, AppPrincipal, AppSpec, DeployEvent, authorize, hash_token, mint_token,
};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};

use jojobot_adapters::testing::Scratch;

async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

/// **The app contract, against the real store.**
#[tokio::test]
async fn dolt_satisfies_the_app_contract() {
    let scratch = Scratch::new("contract");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "appcontract")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::run_all_app(&DoltMemory::open(pool)).await;

    store.stop().await;
}

/// **The app contract, through the fold.** The image answers a claim from what
/// the store gave it, so a mark the store kept and the image dropped passes the
/// case above and fails here.
#[tokio::test]
async fn dolt_satisfies_the_app_contract_when_folded() {
    let scratch = Scratch::new("contract-folded");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "appcontractfolded")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let folded = Folded::new(Arc::new(DoltMemory::open(pool)));
    folded.rebuild().await.expect("the image fills");

    memory::run_all_app(&folded).await;

    store.stop().await;
}

fn principal() -> AppPrincipal {
    AppPrincipal::from_spec(
        &AppSpec {
            name: "springfield-ci".into(),
            operation: "record_deploy_event".into(),
            thread: "thread:deploys-alpha".into(),
            kind: "event".into(),
            keys: vec!["commit".into(), "host".into()],
            revoked: false,
        },
        &hash_token(&mint_token()),
    )
    .expect("a plain grant")
}

fn event_for(attempt: &str) -> jojobot_domain::app::AppEvent {
    authorize(
        &principal(),
        &DeployEvent {
            attempt_id: attempt.into(),
            fields: [
                ("commit".to_string(), "abc1234".to_string()),
                ("host".to_string(), "host-alpha".to_string()),
            ]
            .into(),
        },
    )
    .expect("inside the grant")
    .into_event(
        "record_deploy_event",
        "2026-10-18T12:00:00Z".into(),
        date(2026, 10, 18),
    )
}

/// **Calls racing on one attempt land one event**, and every call is answered
/// with that event's receipt. Each call builds its own event with its own
/// handle, so the only thing that can stop the second is the attempt's row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn calls_racing_on_one_attempt_land_one_event() {
    let scratch = Scratch::new("race");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "app_race")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = Arc::new(DoltMemory::open(pool.clone()));
    let thread = EntityId("thread:deploys-alpha".into());
    memory
        .add_entity(NewEntity::new(thread.clone(), "Deploys Alpha", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");

    // Several rounds, so a race that went one way by luck would not pass.
    for round in 0..6 {
        let attempt = format!("race-{round}");
        let calls = (0..4).map(|_| {
            let memory = Arc::clone(&memory);
            let event = event_for(&attempt);
            tokio::spawn(async move { memory.record_app_event(event).await })
        });
        let mut landed = 0;
        let mut receipts = Vec::new();
        for call in calls {
            match call.await.expect("the call ran").expect("answered") {
                AppOutcome::Repeat(receipt) => receipts.push((receipt.handle, receipt.address)),
                other => {
                    let (entity, fact) = other
                        .landed()
                        .unwrap_or_else(|| panic!("round {round}: neither landed nor repeated"));
                    landed += 1;
                    receipts.push((entity.id.to_string(), fact.address().to_string()));
                }
            }
        }
        assert_eq!(
            landed, 1,
            "round {round}: exactly one call landed the event"
        );
        assert!(
            receipts.iter().all(|r| r == &receipts[0]),
            "round {round}: every call got the one receipt: {receipts:?}"
        );
        let events = memory
            .list_entities(Some(EntityKind::EVENT))
            .await
            .expect("lists");
        assert_eq!(
            events.len(),
            round + 1,
            "round {round}: one event per attempt"
        );
    }

    server.stop().await;
}
