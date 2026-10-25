use super::*;

#[tokio::test]
async fn a_missing_numbered_mention_offers_the_numbers_held() {
    let bare = std::sync::Arc::new(crate::memory::testing::InMemoryMemory::booted());
    let store = mention::Mentioning::new(bare.clone());
    let o = numbered::ordinances(
        &store,
        "Missed ordinances",
        [
            "topic:contract-ordinances-missed",
            "decision:contract-ordinance-missed-curfew",
            "decision:contract-ordinance-missed-parking",
            "decision:contract-ordinance-missed-leaves",
            "decision:contract-ordinance-missed-fireworks",
        ],
    )
    .await;
    let before = bare.recall(&o.collection).await.unwrap();
    let refused = store
        .capture(NewFact::about(
            o.collection.clone(),
            format!("see @{}/999 and @{}/2", o.collection, o.collection),
            date(2026, 5, 3),
        ))
        .await
        .unwrap();
    let Guarded::Blocked {
        attempted,
        candidates,
    } = refused
    else {
        panic!("a missing member must be refused: {refused:?}");
    };
    assert_eq!(attempted, numbered::numbered(&o.collection, 999));
    assert_eq!(candidates.len(), 4, "each held number is offered once");
    let offered: std::collections::BTreeSet<_> = candidates
        .iter()
        .map(|candidate| {
            assert_eq!(candidate.reason, guard::MatchReason::HeldNumber);
            candidate.handle.clone()
        })
        .collect();
    assert_eq!(
        offered,
        (1..=4)
            .map(|n| numbered::numbered(&o.collection, n))
            .collect(),
        "a missing numbered mention must offer all held numbers, including archived members"
    );
    assert_eq!(bare.recall(&o.collection).await.unwrap(), before);

    let written = store
        .capture(NewFact::about(
            o.collection.clone(),
            format!("see @{}/2", o.collection),
            date(2026, 5, 3),
        ))
        .await
        .unwrap()
        .written()
        .expect("a held numbered mention still lands");
    assert_eq!(written.content, format!("see @{}", o.parking));
}

/// **A numbered collection that holds no number offers none, and the ordinary
/// near-miss screen answers**, as the store's own miss does. Paired with the case
/// above, where the collection holds numbers and offers exactly those.
#[tokio::test]
async fn a_missing_numbered_mention_in_a_collection_holding_no_number_is_screened_as_a_name() {
    let bare = std::sync::Arc::new(crate::memory::testing::InMemoryMemory::booted());
    let store = mention::Mentioning::new(bare.clone());
    let collection = EntityId("topic:contract-ordinances-empty".into());
    store
        .add_entity(NewEntity::new(
            collection.clone(),
            "Empty ordinances",
            "contract-fixture",
        ))
        .await
        .unwrap()
        .written()
        .expect("the collection is added");
    store
        .capture(state(NewFact {
            fields: std::collections::BTreeMap::from([(
                "numbers_by".to_string(),
                "number".to_string(),
            )]),
            ..NewFact::about(collection.clone(), "numbers its members", date(2026, 5, 1))
        }))
        .await
        .unwrap()
        .written()
        .expect("the collection numbers");
    let refused = store
        .capture(NewFact::about(
            collection.clone(),
            format!("see @{collection}/7"),
            date(2026, 5, 3),
        ))
        .await
        .unwrap();
    let Guarded::Blocked {
        attempted,
        candidates,
    } = refused
    else {
        panic!("a missing member must be refused: {refused:?}");
    };
    assert_eq!(attempted, numbered::numbered(&collection, 7));
    let screen = guard::screen(&attempted, &[], &bare.list_entities(None).await.unwrap());
    assert_eq!(
        candidates, screen,
        "an empty collection falls back to the ordinary screen"
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.reason != guard::MatchReason::HeldNumber),
        "no number is held, so none is offered: {candidates:?}"
    );
}

/// **A missing number written under a former handle of its collection offers the
/// numbers the collection holds.** The text names the collection by the handle it
/// wore, which still resolves, and the candidates come back under that spelling,
/// the one the caller wrote. Paired with the case above, where the collection was
/// never renamed.
#[tokio::test]
async fn a_missing_numbered_mention_under_a_former_collection_handle_offers_the_numbers_held() {
    let bare = std::sync::Arc::new(crate::memory::testing::InMemoryMemory::booted());
    let store = mention::Mentioning::new(bare.clone());
    let o = numbered::ordinances(
        &store,
        "Renamed ordinances",
        [
            "topic:contract-ordinances-renamed",
            "decision:contract-ordinance-renamed-curfew",
            "decision:contract-ordinance-renamed-parking",
            "decision:contract-ordinance-renamed-leaves",
            "decision:contract-ordinance-renamed-fireworks",
        ],
    )
    .await;
    let now = EntityId("topic:contract-ordinances-renamed-now".into());
    store
        .rename_entity(
            &o.collection,
            &now,
            None,
            date(2026, 5, 2),
            crate::memory::Override::none(),
            None,
        )
        .await
        .unwrap()
        .written()
        .expect("the collection is renamed");
    let refused = store
        .capture(NewFact::about(
            now.clone(),
            format!("see @{}/999", o.collection),
            date(2026, 5, 3),
        ))
        .await
        .unwrap();
    let Guarded::Blocked {
        attempted,
        candidates,
    } = refused
    else {
        panic!("a missing member must be refused: {refused:?}");
    };
    assert_eq!(attempted, numbered::numbered(&o.collection, 999));
    assert_eq!(candidates.len(), 4, "each held number is offered once");
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.reason == guard::MatchReason::HeldNumber),
        "only held numbers are offered: {candidates:?}"
    );
}
