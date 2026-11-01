//! A key's value says how it is known only when the domain's backing names a write
//! for it, and says nothing rather than guess when it does not.

use super::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{FactId, Memory, NewEntity, NewFact, Provenance, Standing};

/// **A key the backing names a write for says how it is known; a key it names none
/// for says nothing, which is how a summing key reads.** The verb omits a total
/// from the backing because no one write holds it. Paired in one read of a real
/// record: `price` is backed by the record and says "said", `donuts` has no
/// backing and says nothing, and a backing that names a record the page does not
/// list says nothing either, since a fold the page cannot place would be a guess.
#[tokio::test]
async fn a_key_says_how_it_is_known_only_when_the_backing_names_a_write_for_it() {
    let memory = InMemoryMemory::booted();
    let thing = EntityId("thing:handcart".into());
    memory
        .add_entity(NewEntity::new(
            thing.clone(),
            "Handcart",
            "the fixture roster",
        ))
        .await
        .expect("the thing is written");
    let mut claim = NewFact::about(
        thing.clone(),
        "the price is five",
        jiff::civil::date(2026, 3, 10),
    );
    claim.provenance = Provenance::Testimony;
    claim.fields = BTreeMap::from([
        ("price".to_string(), "5".to_string()),
        ("donuts".to_string(), "3".to_string()),
    ]);
    claim.sets = claim.fields.keys().cloned().collect();
    memory.capture(claim).await.expect("the claim is written");
    let facts = memory.recall(&thing).await.expect("the claim is read back");
    let held = facts[0].id.clone();

    let backing = |fact: FactId| {
        BTreeMap::from([(
            "price".to_string(),
            FieldBacking {
                fact,
                provenance: Provenance::Testimony,
                standing: Standing::Settled,
                note: None,
                recorded_by: None,
            },
        )])
    };
    let named = how_known(&facts, &backing(held), "price");
    assert!(
        named.contains("said") && named.contains("2026-03-10"),
        "{named}"
    );
    assert_eq!(
        how_known(&facts, &backing(facts[0].id.clone()), "donuts"),
        "",
        "a key the backing names no write for says nothing"
    );
    assert_eq!(
        how_known(&facts, &backing(FactId("unlisted".into())), "price"),
        "",
        "a backing that names a record the page does not list says nothing"
    );
}
