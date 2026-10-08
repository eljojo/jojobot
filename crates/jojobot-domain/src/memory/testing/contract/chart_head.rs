//! **The head of a chart is placed by no bot, proven against any [`Memory`]
//! adapter.**
//!
//! A thing with no manager and reports of its own heads a chart. The edit is
//! judged against the chart read inside its own transaction, so the head has to
//! be recognised there, by the fake and by the real store alike.

use super::support::{add, capture};
use super::*;

/// **A bot with no manager cannot name itself the manager of a chart's head,
/// and still adopts a thing that has no reports.** Paired, or a store that
/// refused every adoption would pass the first half.
pub async fn the_head_of_a_chart_is_placed_by_no_bot<M: Memory>(store: &M) {
    let omega = EntityId("bot:contract-head-omega".into());
    let alpha = EntityId("bot:contract-head-alpha".into());
    let beta = EntityId("bot:contract-head-beta".into());
    let gamma = EntityId("bot:contract-head-gamma".into());
    for (id, name) in [
        (&omega, "Head Omega"),
        (&alpha, "Head Alpha"),
        (&beta, "Head Beta"),
        (&gamma, "Head Gamma"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let reporting = |manager: &EntityId| -> std::collections::BTreeMap<String, String> {
        [(crate::memory::REPORTS_TO.to_string(), manager.to_string())]
            .into_iter()
            .collect()
    };
    let to = |manager: &EntityId| FactPatch {
        fields: reporting(manager),
        ..Default::default()
    };

    // Beta reports to alpha, and alpha reports to none: alpha heads the chart.
    capture(
        store,
        NewFact {
            fields: reporting(&alpha),
            ..NewFact::about(beta.clone(), "beta reports to alpha", date(2026, 10, 1))
        },
    )
    .await;
    let head = capture(
        store,
        NewFact::about(alpha.clone(), "alpha heads the chart", date(2026, 10, 1)),
    )
    .await;

    // Omega is the bot named, which adoption alone would allow. Neither side
    // has a chain, so the cycle check has nothing to read.
    for caller in [&omega, &beta] {
        let err = store
            .update_fact(&head.address(), to(&omega), caller)
            .await
            .expect_err("no bot places the head of a chart");
        assert!(
            matches!(err, MemoryError::ChartHead { .. }),
            "{caller}: expected ChartHead, got {err:?}"
        );
    }
    let held = store.fields(&alpha).await.expect("alpha is readable");
    assert!(
        !held.contains_key(crate::memory::REPORTS_TO),
        "the refused edit wrote nothing: {held:?}"
    );

    // A thing with no reports is adopted by the manager it names.
    let leaf = capture(
        store,
        NewFact::about(gamma.clone(), "gamma has no manager", date(2026, 10, 2)),
    )
    .await;
    store
        .update_fact(&leaf.address(), to(&omega), &omega)
        .await
        .expect("omega adopts a thing with no reports")
        .written()
        .expect("the guard must not block the manager named");
    let adopted = store.fields(&gamma).await.expect("gamma is readable");
    assert_eq!(
        adopted
            .get(crate::memory::REPORTS_TO)
            .map(|manager| manager.trim_start_matches(crate::memory::mention::MARK)),
        Some(omega.as_str()),
        "the adoption reads back: {adopted:?}"
    );
}
