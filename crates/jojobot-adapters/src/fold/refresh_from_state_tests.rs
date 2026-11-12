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

/// **The image is still held after the writes**, so what the case read came from
/// the image and not from the store a dropped image sends a read to. A case that
/// compares with a fold filled anew through the Memory port cannot tell the two
/// apart, because the store holds what the fresh fold holds.
fn assert_held(folded: &Folded, case: &str) {
    assert!(
        folded.held(|_| ()).is_some(),
        "{case}: the refresh dropped the image, and the reads went to the store"
    );
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
        // After each write, since the next write's refresh fills a dropped image
        // from the store and an end check would not see the drop.
        let trace = crate::fold_agreement::run_sequence(seed, 40, &folded, &|step| {
            assert_held(&folded, &format!("seed {seed}, step {step}"));
        })
        .await;
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

/// A store that lends no claims State, as the store is at boot before the claims
/// import.
struct LendsNothing(Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for LendsNothing {
    fn claims_lender(&self) -> Option<Arc<dyn jojobot_domain::log::claims::ClaimsLender>> {
        None
    }
}

/// **Before the claims import a write refreshes the fold from the store.** The
/// boot's repair passes write then, when the log holds no State; the refresh reads
/// the store, as it did, and the image carries the write.
#[tokio::test]
async fn a_write_before_the_import_refreshes_the_fold_from_the_store() {
    let folded = Folded::new(Arc::new(LendsNothing(Arc::new(InMemoryMemory::booted()))));
    folded.rebuild().await.expect("the fold loads");
    folded.before_the_import(true);
    folded
        .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    assert_held(&folded, "an entity added before the import");
    folded
        .capture(NewFact::about(alpha(), "is early", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    assert_held(&folded, "a write before the import");
    let served = folded.recall(&alpha()).await.expect("recall ok");
    assert!(
        served.iter().any(|fact| fact.content == "is early"),
        "the image carries the write: {served:?}"
    );
}

/// **A page written onto a thing is served by the refreshed fold as a fold filled
/// anew serves it**: the prose the State holds is the prose the store holds.
#[tokio::test]
async fn a_prose_write_is_served_as_a_fold_filled_anew_serves_it() {
    let store = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(store.clone());
    folded.rebuild().await.expect("the fold loads");
    folded
        .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    assert_held(&folded, "an entity added before the prose");
    folded
        .set_prose(&alpha(), "walks the quokka at dawn", None)
        .await
        .expect("prose lands");
    assert_held(&folded, "a prose write");
    let fresh = Folded::new(store.clone());
    fresh.rebuild().await.expect("the fold loads again");
    let served = folded.scan_entity(&alpha()).await.expect("scan ok");
    let anew = fresh.scan_entity(&alpha()).await.expect("scan ok");
    assert_eq!(
        served.as_ref().map(|doc| doc.prose.clone()),
        anew.as_ref().map(|doc| doc.prose.clone()),
        "the prose differs between the refreshed fold and a fold filled anew"
    );
    assert_eq!(
        served.map(|doc| doc.prose).as_deref(),
        Some("walks the quokka at dawn")
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
