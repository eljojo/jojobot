use super::*;
use crate::memory::{Fact, FactId, FactStatus, Provenance, Standing};

fn claim(edge: Option<Edge>, written_by_another: bool) -> Fact {
    let milhouse = EntityId("bot:milhouse".into());
    Fact {
        app_stamp: None,
        written_by_another,
        sets: Default::default(),
        reaching: Default::default(),
        field_links: Default::default(),
        id: FactId("f1".into()),
        home: milhouse.clone(),
        subject: milhouse,
        content: "keeps the ledger".into(),
        details: None,
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status: FactStatus::Active,
        recorded_at: jiff::civil::date(2026, 10, 16),
        happened_at: None,
        happened_through: None,
        edge,
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: None,
        stale_after: None,
        receipt: None,
    }
}

/// **What a claim on a bot is, by the edge it draws and who wrote it.** Every
/// row is asked of the one test and of the two partitions built on it.
#[test]
fn a_claim_on_a_bot_is_its_thought_or_its_rule_or_neither() {
    let object = EntityId("thing:ledger".into());
    let edge = |shape| Some(Edge::new(shape, object.clone()));
    //   (edge, written by another, thought, rule)
    let table = [
        (None, false, false, true),
        (None, true, false, true),
        (edge(EdgeShape::About), false, false, true),
        (edge(EdgeShape::About), true, false, true),
        (edge(EdgeShape::Connection), false, true, false),
        (edge(EdgeShape::Connection), true, false, false),
    ];
    for (edge, another, thought, rule) in table {
        let fact = claim(edge, another);
        assert_eq!(
            is_thought_of(fact.edge.as_ref(), another),
            thought,
            "{fact:?}"
        );
        assert_eq!(crate::memory::is_thought(&fact), thought, "{fact:?}");
        assert_eq!(crate::memory::review::is_rule(&fact), rule, "{fact:?}");
        assert_eq!(
            crate::memory::rules_in_force(std::slice::from_ref(&fact)).len(),
            usize::from(rule),
            "{fact:?}"
        );
        assert_eq!(
            crate::memory::thought_room(std::slice::from_ref(&fact)).len(),
            usize::from(thought),
            "{fact:?}"
        );
    }
}

/// **A capture returns the flag it computed**, in the fake, through the
/// contract case the real store runs too.
#[tokio::test]
async fn a_capture_returns_whose_claim_it_wrote() {
    let store = crate::memory::testing::InMemoryMemory::booted();
    crate::memory::testing::contract::a_bot_renamed_twice_still_reads_its_own_thought_as_its_own(
        &store,
    )
    .await;
}
