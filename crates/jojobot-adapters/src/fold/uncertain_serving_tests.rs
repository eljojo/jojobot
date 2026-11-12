//! **While a commit's outcome is unknown, a served read answers from the State at
//! its last applied batch, and a read the State does not serve is refused.** It
//! never crosses the uncertain rail into the store: the store may hold a write the
//! State does not, and a caller that reads to learn whether a write landed is told
//! the document from before it.

use super::*;
use crate::counted::Counted;
use jiff::civil::date;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{NewEntity, NewFact};

/// A fold over a counted store, with a thing and a claim, a role, and the rail
/// then made uncertain.
async fn uncertain_fold() -> (Folded, Arc<Counted>, EntityId, EntityId) {
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let thing = EntityId::person("person:alpha");
    let role = EntityId("role:kappa".to_string());
    for (id, name) in [(&thing, "Alpha"), (&role, "Kappa")] {
        folded
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
    }
    folded
        .capture(NewFact::about(
            thing.clone(),
            "before the unknown commit",
            date(2026, 9, 1),
        ))
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    folded.writer().mark_uncertain("memory");
    counted.reset();
    (folded, counted, thing, role)
}

#[tokio::test]
async fn a_read_while_the_rail_is_uncertain_is_served_from_the_state_and_asks_the_store_nothing() {
    let (folded, counted, thing, _) = uncertain_fold().await;
    let served = folded.recall(&thing).await.expect("served from the State");
    assert_eq!(
        served.len(),
        1,
        "the document from before the unknown commit"
    );
    assert_eq!(
        counted.calls(),
        0,
        "the read crossed the rail into the store"
    );
}

#[tokio::test]
async fn a_read_the_state_does_not_serve_is_refused_while_the_rail_is_uncertain() {
    let (folded, counted, _, role) = uncertain_fold().await;
    let refused = folded.recall(&role).await;
    assert!(
        matches!(&refused, Err(MemoryError::Store(why)) if why.contains("uncertain")),
        "a declined read went on: {refused:?}"
    );
    assert_eq!(counted.calls(), 0, "the refused read crossed the rail");
}
