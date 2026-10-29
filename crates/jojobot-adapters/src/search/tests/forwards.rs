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

/// A store whose declaration verbs land the change and then answer
/// [`MemoryError::Indeterminate`], as a guarded recovery does when the commit
/// state was unknown and the fold was reloaded. It also wears the fixed write
/// summary, which never counts a declaration, so the scan cache is the only
/// thing that can serve a scan taken before the declaration.
struct LandsThenIndeterminate(FixedWriteSummary);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for LandsThenIndeterminate {
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.0.declare_type(declared, by_session).await?;
        Err(MemoryError::Indeterminate)
    }

    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<bool, MemoryError> {
        self.0.declare_kind(token, origin, fields).await?;
        Err(MemoryError::Indeterminate)
    }

    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.0.reclaim_kind(token).await?;
        Err(MemoryError::Indeterminate)
    }
}

/// Two captures of `laps` on one thing, under a store that lands each
/// declaration verb and then answers indeterminate.
async fn a_thing_with_two_laps() -> (Arc<InMemoryMemory>, IndexedMemory, EntityId) {
    let inner = Arc::new(InMemoryMemory::booted());
    let thing = EntityId("thing:alpha".into());
    inner
        .add_entity(NewEntity::new(thing.clone(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    for (words, day) in [("lap one", 1), ("lap two", 2)] {
        inner
            .capture(contract::state(NewFact {
                fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
                ..NewFact::about(thing.clone(), words, date(2026, 9, day))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
    }
    let indexed = IndexedMemory::new(Arc::new(LandsThenIndeterminate(FixedWriteSummary(
        inner.clone(),
    ))))
    .expect("index opens");
    (inner, indexed, thing)
}

/// What the index serves for `laps` on the thing after a real or cached scan.
async fn laps_served(indexed: &IndexedMemory, thing: &EntityId) -> Option<String> {
    let scanned = indexed.rescan().await.expect("rescan ok");
    scanned
        .iter()
        .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == thing))
        .and_then(|d| d.fields.get("laps").cloned())
}

/// A declared type that landed under an indeterminate answer is read at once.
#[tokio::test]
async fn an_indeterminate_type_declaration_does_not_leave_a_scan_from_before_it() {
    let (_inner, indexed, thing) = a_thing_with_two_laps().await;
    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("1"));

    let declared = indexed
        .declare_type(
            DeclaredType::new(
                "running",
                vec![jojobot_domain::memory::types::Field::summing("laps")],
            ),
            None,
        )
        .await;
    assert!(matches!(declared, Err(MemoryError::Indeterminate)));

    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("2"));
}

/// A declared kind that landed under an indeterminate answer is read at once.
#[tokio::test]
async fn an_indeterminate_kind_declaration_does_not_leave_a_scan_from_before_it() {
    let (_inner, indexed, thing) = a_thing_with_two_laps().await;
    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("1"));

    let declared = indexed
        .declare_kind(
            "thing",
            jojobot_domain::memory::types::Origin::Shipped,
            vec![jojobot_domain::memory::types::Field::summing("laps")],
        )
        .await;
    assert!(matches!(declared, Err(MemoryError::Indeterminate)));

    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("2"));
}

/// A reclaimed kind that landed under an indeterminate answer is read at once.
#[tokio::test]
async fn an_indeterminate_kind_reclaim_does_not_leave_a_scan_from_before_it() {
    let (inner, indexed, thing) = a_thing_with_two_laps().await;
    inner
        .declare_kind(
            "thing",
            jojobot_domain::memory::types::Origin::Shipped,
            vec![jojobot_domain::memory::types::Field::summing("laps")],
        )
        .await
        .expect("declared");
    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("2"));

    let reclaimed = indexed.reclaim_kind("thing").await;
    assert!(matches!(reclaimed, Err(MemoryError::Indeterminate)));

    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("1"));
}
