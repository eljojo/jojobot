//! **A write refreshes the fold from the claims State, and the store is read for
//! nothing the State can answer.** The store beneath is counted by method name,
//! so a case names the read that reached it.

use super::*;
use crate::counted::Counted;
use jiff::civil::date;
use jojobot_domain::memory::testing::InMemoryMemory;

fn alpha() -> EntityId {
    EntityId::person("person:alpha")
}

/// A folded store over a counted fake, with the fake lending its claims State.
async fn folded_over_a_counted_fake() -> (Folded, Arc<Counted>, Arc<InMemoryMemory>) {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone()).with_claims_view(store.clone());
    folded.rebuild().await.expect("an empty store fills");
    (folded, counted, store)
}

/// **A fold refreshed from the claims State serves what a fold filled anew
/// serves**, after sequences of writes nobody wrote by hand. The running fold
/// reads its State for each write; the fold it is compared with and the store
/// are read as ever.
#[tokio::test]
async fn a_fold_refreshed_from_the_state_agrees_with_one_filled_anew() {
    for seed in 1..=40u64 {
        let store = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(store.clone()).with_claims_view(store.clone());
        folded.rebuild().await.expect("the fold loads");
        let trace = crate::fold_agreement::run_sequence(seed, 40, &folded).await;
        let fresh = Folded::new(store.clone());
        fresh.rebuild().await.expect("the fold loads again");
        if let Some(differs) = crate::fold_agreement::disagreement(&folded, &fresh).await {
            panic!(
                "seed {seed}: the running fold differs from one filled anew\n{differs}\nsteps:\n  {}",
                trace.steps.join("\n  ")
            );
        }
    }
}

/// **A claim write reads the store for nothing but the write.** The refresh
/// after it builds the thing's document from the State: no scan of the thing, no
/// listing, no holders. The image carries the write, which a read after it shows.
#[tokio::test]
async fn a_claim_write_refreshes_its_thing_from_the_claims_state() {
    let (folded, counted, _store) = folded_over_a_counted_fake().await;
    folded
        .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    counted.reset();
    folded
        .capture(NewFact::about(alpha(), "says hello", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    let seen = counted.seen();
    assert_eq!(
        seen.keys().copied().collect::<Vec<_>>(),
        ["capture"],
        "the refresh read the store: {seen:?}"
    );
    counted.reset();
    let claims = folded.recall(&alpha()).await.expect("recall ok");
    assert_eq!(counted.calls(), 0, "the read reached the store");
    assert!(
        claims.iter().any(|fact| fact.content == "says hello"),
        "the image carries the write: {claims:?}"
    );
}
