//! **A companion record that the store refuses INSIDE the batch leaves nothing
//! behind, in the real store.**
//!
//! A creation writes the thing, its first claim and the engine's companion
//! records in one transaction. A companion is checked for shape before the
//! transaction opens, so a companion that passes that check and fails a rule the
//! transaction reads (here the thing's record ceiling) is refused after the
//! thing's row and the first claim are already in the batch. The plain
//! sequential contract case refuses before the batch and passes on a store that
//! commits in steps. This case does not.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};

/// A first claim that sets the thing's ceiling to `ceiling`, and one companion.
fn creation(thing: &EntityId, ceiling: &str) -> (NewEntity, NewFact) {
    let mut first = NewFact::about(thing.clone(), "the first claim", date(2026, 10, 7));
    first.provenance = Provenance::Testimony;
    first
        .fields
        .insert("record_capacity".to_string(), ceiling.to_string());
    first.sets.insert("record_capacity".to_string());
    let mut companion = NewFact::about(thing.clone(), "the companion", date(2026, 10, 7));
    companion.provenance = Provenance::Testimony;
    first.also_written.push(companion);
    (
        NewEntity::new(thing.clone(), "Companion", "contract-fixture"),
        first,
    )
}

#[tokio::test]
async fn a_companion_refused_inside_the_transaction_leaves_nothing() {
    let scratch = Scratch::new("companion-atomic");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "companionatomic")
        .await
        .expect("a database of its own");
    let memory = DoltMemory::open(pool);
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    // **A ceiling of one holds the first claim and no companion.** The companion
    // is well formed, so only the batch's own read of the thing's records can
    // refuse it, after the thing and the first claim are in the batch.
    let thing = EntityId("work:theta".to_string());
    let (entity, first) = creation(&thing, "1");
    let refused = memory.add_entity_with_first_claim(entity, first).await;
    let said = format!("{refused:?}");
    assert!(
        refused.map_or(true, |answer| answer.written().is_none()),
        "the companion goes over the ceiling, so the creation does not land: {said}"
    );
    let roster = memory.list_entities(None).await.expect("the roster reads");
    assert!(
        roster.iter().all(|entity| entity.id != thing),
        "a companion refused inside the batch left {thing} on the roster: {roster:?}"
    );
    assert!(
        memory
            .recall(&thing)
            .await
            .map_or(true, |facts| facts.is_empty()),
        "a read of the handle finds a claim of a thing a refused batch made"
    );

    // **The same creation with room for the companion makes all three**, so the
    // miss above is the refusal and not a handle this store turns away.
    let roomy = EntityId("work:iota".to_string());
    let (entity, first) = creation(&roomy, "2");
    memory
        .add_entity_with_first_claim(entity, first)
        .await
        .expect("the creation is allowed")
        .written()
        .expect("nothing collides with it");
    assert!(
        memory
            .list_entities(None)
            .await
            .expect("the roster reads")
            .iter()
            .any(|entity| entity.id == roomy),
        "the thing is made when its companion fits"
    );
    let stored = memory.recall(&roomy).await.expect("the claims read back");
    for words in ["the first claim", "the companion"] {
        assert!(
            stored.iter().any(|fact| fact.content == words),
            "{words:?} is stored on the new thing: {stored:?}"
        );
    }

    server.stop().await;
}
