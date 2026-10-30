//! A declaration whose answer is unknown, because the write may have landed, is
//! read at once: the recovery reloads the State, which moves its count, and the
//! next search folds the fields under the declaration that landed.

use super::*;

/// A store whose declaration verbs land the change and then answer
/// [`MemoryError::Indeterminate`], as a store does when the commit state was
/// unknown after the write ran.
struct LandsThenIndeterminate(Arc<InMemoryMemory>);

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

/// Two captures of `laps` on one thing, an index over a fold of the store above,
/// and the fold's image loaded.
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
    let indexed =
        IndexedMemory::new(Arc::new(LandsThenIndeterminate(inner.clone()))).expect("index opens");
    indexed.state().rebuild().await.expect("the fold loads");
    (inner, indexed, thing)
}

/// What a search's scan serves for `laps` on the thing.
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
        .state()
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
        .state()
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
    indexed.state().rebuild().await.expect("the fold reloads");
    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("2"));

    let reclaimed = indexed.state().reclaim_kind("thing").await;
    assert!(matches!(reclaimed, Err(MemoryError::Indeterminate)));

    assert_eq!(laps_served(&indexed, &thing).await.as_deref(), Some("1"));
}
