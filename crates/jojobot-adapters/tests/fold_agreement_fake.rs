//! **Seeded command sequences, over the fake: the fold the writes patched must
//! serve what a fold filled anew from the store serves, and what the store
//! serves.** `tests/fold_agreement_dolt.rs` runs the same draws over the real
//! store.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;

use jojobot_adapters::fold_agreement::{check_seed, landed_enough, run_through_fold};
use jojobot_domain::memory::{EntityId, LinkRule, Memory, set_link_rule};

/// The seeds a default run draws, and the steps each takes.
const SEEDS: std::ops::Range<u64> = 0..60;
const STEPS: usize = 30;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fold_the_writes_patched_serves_what_a_fold_filled_anew_and_the_store_serve() {
    set_link_rule(LinkRule::On);
    let mut failures = Vec::new();
    let mut traces = Vec::new();
    for seed in SEEDS {
        match check_seed(seed, STEPS, Arc::new(InMemoryMemory::booted())).await {
            Ok(trace) => traces.push(trace),
            Err(report) => failures.push(report),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} seeds disagree; replay one with its seed.\n{}",
        failures.len(),
        SEEDS.end - SEEDS.start,
        failures.first().cloned().unwrap_or_default()
    );
    // **Agreement over writes that did not land agrees about nothing.**
    if let Err(why) = landed_enough(&traces) {
        panic!("the writes did not land through the fold: {why}");
    }
}

/// **The run draws every kind of write, applies it THROUGH the fold, and most of
/// it lands.** A fold that refuses what it is given leaves the agreement above
/// comparing empty claims; this reads the fold's own answers to the writes.
#[tokio::test]
async fn a_default_run_draws_every_kind_of_write_through_the_fold_and_most_of_them_land() {
    set_link_rule(LinkRule::On);
    let mut traces = Vec::new();
    for seed in SEEDS {
        let store = Arc::new(InMemoryMemory::booted());
        let (_, trace) = run_through_fold(seed, STEPS, store).await;
        traces.push(trace);
    }
    if let Err(why) = landed_enough(&traces) {
        panic!("{why}");
    }
}

#[tokio::test]
async fn link_rule_leftovers_declarations_keep_the_same_links_through_the_fold() {
    set_link_rule(LinkRule::On);
    let folded = jojobot_adapters::fold::Folded::new(Arc::new(InMemoryMemory::booted()));
    folded.rebuild().await.unwrap();
    jojobot_domain::memory::testing::contract::a_declaration_replacement_and_key_removal_change_no_link(
        &folded,
    )
    .await;
    let targets = [
        EntityId("event:contract-listed-first".into()),
        EntityId("thing:contract-moved-second".into()),
    ];
    let mut before = Vec::new();
    for target in &targets {
        let facts = folded.referring_to(target).await.unwrap();
        assert_eq!(facts.len(), 4, "two holders in each declaration order");
        before.push(
            facts
                .into_iter()
                .map(|fact| (fact.address().to_string(), fact.fields, fact.field_links))
                .collect::<Vec<_>>(),
        );
    }
    folded.rebuild().await.unwrap();
    for (target, expected) in targets.iter().zip(before) {
        let after: Vec<_> = folded
            .referring_to(target)
            .await
            .unwrap()
            .into_iter()
            .map(|fact| (fact.address().to_string(), fact.fields, fact.field_links))
            .collect();
        assert_eq!(after, expected, "a fill preserves values, links and order");
    }
}

#[tokio::test]
async fn link_rule_leftovers_mixed_history_survives_a_fold_reload() {
    set_link_rule(LinkRule::On);
    let folded = jojobot_adapters::fold::Folded::new(Arc::new(InMemoryMemory::booted()));
    folded.rebuild().await.unwrap();
    jojobot_domain::memory::testing::contract::mixed_rename_reuse_and_merge_preserve_the_history(
        &folded,
    )
    .await;
    let holder = EntityId("person:contract-listed-bystander".into());
    let survivor = EntityId("work:theta".into());
    let fields = folded.fields(&holder).await.unwrap();
    let links = folded.referring_to(&survivor).await.unwrap();
    assert_eq!(links.len(), 1);
    let history = folded
        .history(&holder, "contract_link_history")
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    folded.rebuild().await.unwrap();
    assert_eq!(folded.fields(&holder).await.unwrap(), fields);
    assert_eq!(folded.referring_to(&survivor).await.unwrap(), links);
    assert_eq!(
        folded
            .history(&holder, "contract_link_history")
            .await
            .unwrap(),
        history
    );
}
