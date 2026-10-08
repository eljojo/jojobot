//! **Who wrote a thought into a thread is answered by the store beneath the
//! decorators, not by a default built on top of them.** The port gives the call a
//! default that lists every bot and recalls each one. A decorator that does not
//! forward the call answers that default, which costs a read per bot on every
//! write of a thread's ceiling, and which can differ from the store's own answer.
//! The fake answers the call from its claims and never lists, so the listing
//! counter says which one a decorator reached.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::IndexedMemory;
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{Edge, EdgeShape, EntityId, Memory, NewEntity, NewFact};

/// A store holding a bot and a thread, and a thought of the bot's at the thread.
async fn store_with_a_thought() -> (Arc<InMemoryMemory>, EntityId, EntityId) {
    let store = Arc::new(InMemoryMemory::booted());
    let bot = EntityId("bot:forwarded-writer".into());
    let thread = EntityId("thread:forwarded-line".into());
    for (id, name) in [(&bot, "Forwarded Writer"), (&thread, "Forwarded Line")] {
        store
            .add_entity(NewEntity::new(id.clone(), name, "forwarding-case"))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
    }
    store
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, thread.clone())),
            ..NewFact::about(bot.clone(), "the line goes on", date(2026, 10, 1))
        })
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    (store, bot, thread)
}

/// Ask `layer` who wrote into the thread, and say how many listings it cost.
async fn asked_through(
    layer: &dyn Memory,
    store: &InMemoryMemory,
    thread: &EntityId,
) -> (Vec<EntityId>, usize) {
    let before = store.listings();
    let writers = layer.thought_writers(thread).await.expect("answers");
    (writers, store.listings() - before)
}

#[tokio::test]
async fn each_decorator_forwards_who_wrote_into_a_thread() {
    // The positive: the store itself answers, and lists nothing.
    let (store, bot, thread) = store_with_a_thought().await;
    let (writers, listings) = asked_through(store.as_ref(), &store, &thread).await;
    assert_eq!(writers, vec![bot.clone()], "the store names the writer");
    assert_eq!(listings, 0, "the store answers from its claims");

    let folded = Folded::new(store.clone());
    // `Provisioned` owns its store, so it is stacked over a `Folded` that is held to
    // the same answer on its own just above it.
    let provisioned = Provisioned::new(Folded::new(store.clone()), Provisions::default());
    let mentioning = Mentioning::new(store.clone());
    let indexed = IndexedMemory::new(store.clone()).expect("the index opens");
    let layers: [(&str, &dyn Memory); 4] = [
        ("Folded", &folded),
        ("Provisioned", &provisioned),
        ("Mentioning", &mentioning),
        ("IndexedMemory", &indexed),
    ];
    for (name, layer) in layers {
        let (writers, listings) = asked_through(layer, &store, &thread).await;
        assert_eq!(writers, vec![bot.clone()], "{name} names the writer");
        assert_eq!(
            listings, 0,
            "{name} answered the default, which lists every bot, and not the store beneath it"
        );
    }
}
