//! The index forwards what a switched store holds.

use super::*;

/// The index forwards which keys of a record the store holds off the thing.
#[tokio::test]
async fn the_index_forwards_the_keys_a_switched_store_holds_off_a_thing() {
    let store = IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens");
    contract::own_fields_stay_while_settings_and_computed_keys_reach(&store).await;
}

/// The index forwards the writes of a key that stays on its record, with the
/// moment of each.
#[tokio::test]
async fn the_index_forwards_the_writes_that_stay_on_their_record() {
    let store = IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens");
    contract::a_key_that_stays_on_its_record_is_in_the_trails_with_its_moment(&store).await;
}
