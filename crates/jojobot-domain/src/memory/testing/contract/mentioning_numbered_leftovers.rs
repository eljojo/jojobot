use super::*;

#[tokio::test]
async fn leftovers_a_missing_numbered_mention_offers_the_numbers_held() {
    let bare = std::sync::Arc::new(crate::memory::testing::InMemoryMemory::booted());
    let store = mention::Mentioning::new(bare.clone());
    let o = numbered::ordinances(
        &store,
        "Leftovers ordinances",
        [
            "topic:contract-ordinances-leftovers",
            "decision:contract-ordinance-leftovers-curfew",
            "decision:contract-ordinance-leftovers-parking",
            "decision:contract-ordinance-leftovers-leaves",
            "decision:contract-ordinance-leftovers-fireworks",
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
