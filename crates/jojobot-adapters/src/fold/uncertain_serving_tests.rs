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

/// **A scan that lists a thing the State declines is refused whole while the rail
/// is uncertain** (architect #f56): it never crosses the rail for that one thing,
/// and the things the State does serve are not returned beside the refusal. The
/// loss is named by the TEMPORARY(9.3.4) markers on the declines.
#[tokio::test]
async fn a_scan_listing_a_declined_thing_is_refused_whole_while_the_rail_is_uncertain() {
    let (folded, counted, _, _role) = uncertain_fold().await;
    let refused = folded.scan().await;
    assert!(
        matches!(&refused, Err(MemoryError::Store(why)) if why.contains("uncertain")),
        "a scan listing a role went on: {refused:?}"
    );
    assert_eq!(counted.calls(), 0, "the refused scan crossed the rail");
}

/// **Paired: a scan that lists nothing the State declines is served whole from the
/// State**, so the refusal above is the declined thing's and not every scan's.
#[tokio::test]
async fn a_scan_listing_no_declined_thing_is_served_from_the_state_while_the_rail_is_uncertain() {
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let thing = EntityId::person("person:alpha");
    folded
        .add_entity(NewEntity::new(thing.clone(), "Alpha", "user-named"))
        .await
        .expect("added")
        .written()
        .expect("not blocked");
    folded.writer().mark_uncertain("memory");
    counted.reset();
    let served = folded.scan().await.expect("served from the State");
    assert_eq!(served.len(), 1, "the one thing the fold lists");
    assert_eq!(counted.calls(), 0, "the scan crossed the rail");
}

/// **A holder query that must read a declined thing is refused while the rail is
/// uncertain**, as a scan listing one is: the State does not decide a role or a thing a
/// merge touched, and the store beyond the rail is not read for it.
#[tokio::test]
async fn a_holder_query_over_a_declined_thing_is_refused_while_the_rail_is_uncertain() {
    let (folded, counted, _, _role) = uncertain_fold().await;
    let test = jojobot_domain::memory::filter::Test::NotEquals(String::new());
    for key in ["agent", jojobot_domain::session::ROLE_CLAIMED_AT] {
        let refused = folded.keys_holding(key, &test, None).await;
        assert!(
            matches!(&refused, Err(MemoryError::Store(why)) if why.contains("uncertain")),
            "a holder query of {key} over a role went on: {refused:?}"
        );
        assert_eq!(counted.calls(), 0, "the refused query crossed the rail");
    }
}

/// **A holder query that must read a merge-touched survivor is refused while the rail
/// is uncertain**, as one over a role is: the survivor of a merge is a thing the State
/// does not decide a read of, and the store beyond the rail is not read for it.
#[tokio::test]
async fn a_holder_query_over_a_merge_survivor_is_refused_while_the_rail_is_uncertain() {
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let (survivor, gone, caller) = (
        EntityId::person("person:alpha"),
        EntityId::person("person:beta"),
        EntityId("bot:sigma".to_string()),
    );
    for (id, name) in [(&survivor, "Alpha"), (&gone, "Beta"), (&caller, "Sigma")] {
        folded
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
    }
    folded
        .merge(
            &gone,
            &survivor,
            Some("filed twice"),
            date(2026, 9, 3),
            &caller,
            None,
        )
        .await
        .expect("the merge lands");
    folded.writer().mark_uncertain("memory");
    counted.reset();
    let test = jojobot_domain::memory::filter::Test::NotEquals(String::new());
    let refused = folded.keys_holding("agent", &test, None).await;
    assert!(
        matches!(&refused, Err(MemoryError::Store(why)) if why.contains("uncertain")),
        "a holder query over a merge survivor went on: {refused:?}"
    );
    assert_eq!(counted.calls(), 0, "the refused query crossed the rail");
}

/// **Paired: with nothing declined the holder query is served from the State.**
#[tokio::test]
async fn a_holder_query_with_nothing_declined_is_served_from_the_state_while_the_rail_is_uncertain()
{
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let thing = EntityId::person("person:alpha");
    folded
        .add_entity(NewEntity::new(thing.clone(), "Alpha", "user-named"))
        .await
        .expect("added")
        .written()
        .expect("not blocked");
    folded.writer().mark_uncertain("memory");
    counted.reset();
    let held = folded
        .keys_holding(
            "agent",
            &jojobot_domain::memory::filter::Test::NotEquals(String::new()),
            None,
        )
        .await
        .expect("served from the State");
    assert!(held.is_empty(), "{held:?}");
    assert_eq!(counted.calls(), 0, "the query crossed the rail");
}
