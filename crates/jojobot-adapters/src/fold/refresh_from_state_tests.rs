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
    let folded = Folded::new(counted.clone());
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
        let folded = Folded::new(store.clone());
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

/// **A rename reads the store for nothing but the rename.** The patch after it
/// takes the listing, the rename history, the merge events, the renamed thing's
/// document and the holders of its names from the claims State. The image
/// carries the rename, which a read after it shows.
#[tokio::test]
async fn a_rename_patches_the_image_from_the_claims_state() {
    let (folded, counted, _store) = folded_over_a_counted_fake().await;
    let beta = EntityId::person("person:beta");
    for (id, name) in [(alpha(), "Alpha"), (beta.clone(), "Beta")] {
        folded
            .add_entity(NewEntity::new(id, name, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }
    folded
        .capture(NewFact::about(
            beta.clone(),
            "knows alpha",
            date(2026, 9, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    let gamma = EntityId::person("person:gamma");
    counted.reset();
    folded
        .rename_entity(
            &alpha(),
            &gamma,
            None,
            date(2026, 9, 2),
            jojobot_domain::memory::Override::default(),
            None,
        )
        .await
        .expect("rename ok")
        .written()
        .expect("not blocked");
    let seen = counted.seen();
    assert_eq!(
        seen.keys().copied().collect::<Vec<_>>(),
        ["rename_entity"],
        "the patch read the store: {seen:?}"
    );
    counted.reset();
    let listed = folded.list_entities(None).await.expect("list ok");
    assert_eq!(counted.calls(), 0, "the listing reached the store");
    assert!(
        listed.iter().any(|entity| entity.id == gamma),
        "the image carries the rename: {listed:?}"
    );
}

/// **A role's document is read from the store after a write**, because a lease
/// renewal is a heartbeat kept off the log, which the State does not hold. The
/// case that renews a lease and reads it back is the upgrade gate's
/// `appended-lease-tail`.
#[tokio::test]
async fn a_claim_write_on_a_role_reads_its_document_from_the_store() {
    let (folded, counted, _store) = folded_over_a_counted_fake().await;
    let role = EntityId("role:sigma".into());
    folded
        .add_entity(NewEntity::new(role.clone(), "Sigma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    counted.reset();
    folded
        .capture(NewFact::about(role, "holds the lease", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    let seen = counted.seen();
    assert!(
        seen.contains_key("scan_entity"),
        "the document of a role was not read from the store: {seen:?}"
    );
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
