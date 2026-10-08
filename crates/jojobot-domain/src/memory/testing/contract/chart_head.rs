//! **The head of a chart places itself, and no other bot places it, proven
//! against any [`Memory`] adapter.**
//!
//! A thing with no manager and reports of its own heads a chart. The edit is
//! judged against the chart read inside its own transaction, so the head has to
//! be recognised there, by the fake and by the real store alike.

use super::support::{add, capture};
use super::*;

/// **The head of a chart places itself, and no other bot places it.** A bot with
/// no manager cannot name itself the manager of a chart's head, by an edit or by a
/// merge, and the refusal names the head as the one who may. The head itself
/// writes its own manager and it lands. A thing with no reports is still adopted
/// by the manager it names. Paired, or a store that refused every adoption would
/// pass the first half.
pub async fn the_head_of_a_chart_places_itself<M: Memory>(store: &M) {
    let omega = EntityId("bot:contract-head-omega".into());
    let alpha = EntityId("bot:contract-head-alpha".into());
    let beta = EntityId("bot:contract-head-beta".into());
    let gamma = EntityId("bot:contract-head-gamma".into());
    let twin = EntityId("bot:contract-head-twin".into());
    let second = EntityId("bot:contract-head-second".into());
    let follower = EntityId("bot:contract-head-follower".into());
    for (id, name) in [
        (&omega, "Head Omega"),
        (&alpha, "Head Alpha"),
        (&beta, "Head Beta"),
        (&gamma, "Head Gamma"),
        (&twin, "Head Twin"),
        (&second, "Head Second"),
        (&follower, "Head Follower"),
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
    // The manager a fold holds, as the handle it names.
    let manager_in = |fold: &std::collections::BTreeMap<String, String>| -> Option<String> {
        fold.get(crate::memory::REPORTS_TO).map(|manager| {
            manager
                .trim_start_matches(crate::memory::mention::MARK)
                .to_string()
        })
    };
    let names_only = |err: &MemoryError, head: &EntityId| match err {
        MemoryError::KeyNotYours { allowed, key, .. } => {
            allowed == &vec![head.to_string()] && key == crate::memory::REPORTS_TO
        }
        MemoryError::MergeCarriesGuardedKeys { allowed, .. } => allowed == &vec![head.to_string()],
        _ => false,
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
            .expect_err("no other bot places the head of a chart");
        assert!(
            names_only(&err, &alpha),
            "{caller}: expected a refusal naming the head alone, got {err:?}"
        );
    }
    let held = store.fields(&alpha).await.expect("alpha is readable");
    assert!(
        !held.contains_key(crate::memory::REPORTS_TO),
        "the refused edit wrote nothing: {held:?}"
    );

    // The head places itself, and it reads back.
    store
        .update_fact(&head.address(), to(&omega), &alpha)
        .await
        .expect("the head writes its own manager")
        .written()
        .expect("the guard must not block the head itself");
    let placed = store.fields(&alpha).await.expect("alpha is readable");
    assert_eq!(
        manager_in(&placed),
        Some(omega.to_string()),
        "the head's own placement reads back: {placed:?}"
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
        manager_in(&adopted),
        Some(omega.to_string()),
        "the adoption reads back: {adopted:?}"
    );

    // A merge that would carry a manager onto another head: a second chart,
    // headed by `second`, with `follower` reporting to it. The twin carries a
    // manager. A stranger merging it in is refused, naming the head alone.
    capture(
        store,
        NewFact {
            fields: reporting(&second),
            ..NewFact::about(
                follower.clone(),
                "follower reports to second",
                date(2026, 10, 3),
            )
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: reporting(&omega),
            ..NewFact::about(twin.clone(), "twin reports to omega", date(2026, 10, 3))
        },
    )
    .await;
    let err = store
        .merge(&twin, &second, None, date(2026, 10, 3), &omega)
        .await
        .expect_err("no other bot places a head by merging a manager onto it");
    assert!(
        names_only(&err, &second),
        "expected a refusal naming the head alone, got {err:?}"
    );
    store
        .merge(&twin, &second, None, date(2026, 10, 3), &second)
        .await
        .expect("the head may merge a manager onto itself");
    let merged = store.fields(&second).await.expect("second is readable");
    assert_eq!(
        manager_in(&merged),
        Some(omega.to_string()),
        "the merge carried the twin's manager onto the head, and it reads back: {merged:?}"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("listable")
            .iter()
            .any(|entity| entity.id == twin && entity.merged_into.is_some()),
        "the twin was folded into the head"
    );
}
