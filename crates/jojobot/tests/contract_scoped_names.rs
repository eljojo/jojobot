//! Contract: scoped names survive the memory stack the binary assembles.

use std::collections::BTreeSet;
use std::sync::Arc;

use jojobot_adapters::observe::{Observation, Observer, Operation, Outcome};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, Wanted};

struct Observe;
impl Observer for Observe {
    fn finished(&self, _: Operation, _: Outcome, _: std::time::Duration) {}
}

#[tokio::test]
async fn filled_fold_names_survive_the_whole_served_stack() {
    let supplied = jojobot_mcp::provisions();
    let store = Arc::new(InMemoryMemory::booted().knowing(supplied.clone()));
    store
        .add_entity(NewEntity::new(
            EntityId("thing:alpha".into()),
            "Alpha",
            "fixture",
        ))
        .await
        .expect("the entity lands")
        .written()
        .expect("the fixture name is distinct");
    let observation = Observation::new(Arc::new(Observe));
    let (folded, indexed) = jojobot::wiring::assemble_memory(observation.memory(store), supplied)
        .await
        .expect("the production assembly opens");
    jojobot::wiring::fill_memory(&folded, &indexed).await;
    let wanted = Wanted {
        handles: BTreeSet::from([EntityId("thing:alpha".into())]),
        ..Wanted::default()
    };
    let served = indexed.memory().names(&wanted).await.unwrap();
    let observed = observation.memory(folded).names(&wanted).await.unwrap();
    assert!(
        served.is_some(),
        "the memory returned by the production index must forward names"
    );
    assert!(observed.is_some(), "the observer must forward names");
    for names in [served.unwrap(), observed.unwrap()] {
        assert!(
            names
                .known
                .iter()
                .any(|entity| entity.id == EntityId("thing:alpha".into()))
        );
    }
}
