//! **What stands on a claim is served in the domain's one claim order.** Claim
//! labels carry a number, so `f2` comes before `f10`; a lexical order of the
//! label puts `f10` first.

use super::*;
use jiff::civil::date;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{NewFact, Provenance};

#[tokio::test]
async fn what_stands_on_a_claim_is_served_with_f2_before_f10() {
    let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
    folded.rebuild().await.expect("the fold loads");
    let alpha = EntityId::person("person:alpha");
    let beta = EntityId::person("person:beta");
    for (id, name) in [(&alpha, "Alpha"), (&beta, "Beta")] {
        folded
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
    }
    let mut source = NewFact::about(
        alpha.clone(),
        "the claim the rest stand on",
        date(2026, 9, 1),
    );
    source.provenance = Provenance::Inference;
    let source = folded
        .capture(source)
        .await
        .expect("captured")
        .written()
        .expect("not blocked")
        .address();
    for number in 0..11 {
        let mut stands = NewFact::about(
            beta.clone(),
            format!("stands on the source, number {number}"),
            date(2026, 9, 2),
        );
        stands.provenance = Provenance::Inference;
        stands.derived_from = Some(source.clone());
        folded
            .capture(stands)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }
    let served = folded.built_on(&source).await.expect("what stands on it");
    let labels: Vec<&str> = served.iter().map(|fact| fact.id.as_str()).collect();
    let mut by_number = labels.clone();
    by_number.sort_by(|a, b| jojobot_domain::memory::claim_label_order(a, b));
    assert_eq!(labels.len(), 11, "every claim that stands on it is served");
    assert_eq!(
        labels, by_number,
        "the claims are not in the order of their numbers"
    );
}
