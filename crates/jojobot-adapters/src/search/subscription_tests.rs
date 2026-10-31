//! An index is subscribed to its writer without being owned by it: dropping the
//! index releases the writer, and a live index is still told of every write.

use jojobot_domain::memory::NewEntity;
use jojobot_domain::memory::testing::InMemoryMemory;

use super::*;

/// **Dropping every owner of an index releases the index, the fold under it and
/// the writer's queue.** Weak handles to each are kept; once the owners are
/// gone none upgrades. Paired with the live index, which hears a write and
/// finds it: without the pair the drop could pass over an index that never
/// subscribed.
#[tokio::test]
async fn a_dropped_index_releases_its_writer_and_a_live_one_still_hears_writes() {
    let store = IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens");
    store
        .add_entity(NewEntity::new(
            EntityId("person:homer".into()),
            "Zeppelin pilot Homer",
            "fixture",
        ))
        .await
        .expect("adds")
        .written()
        .expect("lands");
    store.writer().drain().await;
    let heard = store
        .index()
        .search(&SearchQuery::text("zeppelin"))
        .expect("search ok");
    assert!(
        !heard.is_empty(),
        "a live index did not hear the write: {heard:?}"
    );

    let index = Arc::downgrade(&store.index());
    let fold = Arc::downgrade(&store.state());
    let queue_alive = store.writer().alive_probe();
    drop(store);
    tokio::task::yield_now().await;
    assert!(index.upgrade().is_none(), "the index outlived its owners");
    assert!(fold.upgrade().is_none(), "the fold outlived its owners");
    assert!(!queue_alive(), "the writer outlived its owners");
}
