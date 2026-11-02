//! **Who may write a bot request, proven against any [`Memory`] adapter.**
//!
//! A request has no manager of its own: its spec is judged by the chain above
//! its target bot, which each store reads inside the edit's own transaction. So
//! the manager of the target writes the spec, a bot outside that chain is
//! refused and the refusal names the manager, the target never writes its own
//! request, and the status is no bot's. The same words on an ordinary record are
//! nobody's business. Every refusal sits beside the same write made where it is
//! allowed, so a store that refused every write of the keys would not pass.

use super::support::{add, capture, state, state_patch};
use super::*;
use crate::memory::MayWrite;
use crate::memory::bot_request::{DESIRED, OBSERVED_STATE, TARGET_BOT};

/// **The edit judges a request by its target's chain.**
pub async fn a_bot_requests_spec_is_judged_by_the_chain_above_its_target<M: Memory>(store: &M) {
    let request = EntityId("bot-request:contract-request-ask".into());
    let plain = EntityId("topic:contract-request-plain".into());
    let manager = EntityId("bot:contract-request-manager".into());
    let target = EntityId("bot:contract-request-target".into());
    let outsider = EntityId("bot:contract-request-outsider".into());
    for (id, name) in [
        (&request, "Request Ask"),
        (&plain, "Request Plain"),
        (&manager, "Request Manager"),
        (&target, "Request Target"),
        (&outsider, "Request Outsider"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let fields = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    // The target reports to the manager; the outsider is not in that chain.
    capture(
        store,
        state(NewFact {
            fields: fields(&[(crate::memory::REPORTS_TO, manager.as_str())]),
            ..NewFact::about(
                target.clone(),
                "target reports to manager",
                date(2026, 10, 1),
            )
        }),
    )
    .await;
    // The request holds its spec, written as a store takes a creation.
    let claim = capture(
        store,
        state(NewFact {
            fields: fields(&[(TARGET_BOT, target.as_str()), (DESIRED, "running")]),
            ..NewFact::about(request.clone(), "the target should run", date(2026, 10, 2))
        }),
    )
    .await;
    let edit = |pairs: &[(&str, &str)]| {
        state_patch(FactPatch {
            fields: fields(pairs),
            ..Default::default()
        })
    };
    let desired = |fold: std::collections::BTreeMap<String, String>| fold.get(DESIRED).cloned();
    let held = || async { desired(store.fields(&request).await.expect("readable")) };
    // The refusal, and the bots it names as the ones that may write.
    let refused = |err: MemoryError, what: &str| -> Vec<String> {
        match err {
            MemoryError::KeyNotYours {
                may: MayWrite::RequestSpec,
                allowed,
                ..
            } => allowed,
            other => panic!("{what}: expected KeyNotYours for the spec, got {other:?}"),
        }
    };

    // ── a bot outside the target's chain is refused, and the refusal names the
    //    manager ─────────────────────────────────────────────────────────────
    let err = store
        .update_fact(&claim.address(), edit(&[(DESIRED, "stopped")]), &outsider)
        .await
        .expect_err("a bot outside the target's chain does not write the spec");
    assert!(
        refused(err, "the outsider's edit").contains(&manager.to_string()),
        "the refusal names the manager"
    );
    assert_eq!(held().await.as_deref(), Some("running"));

    // ── the target does not write its own request ───────────────────────────
    let err = store
        .update_fact(&claim.address(), edit(&[(DESIRED, "stopped")]), &target)
        .await
        .expect_err("the target does not write its own request");
    refused(err, "the target's edit");
    assert_eq!(held().await.as_deref(), Some("running"));

    // ── the manager of the target does ──────────────────────────────────────
    store
        .update_fact(&claim.address(), edit(&[(DESIRED, "stopped")]), &manager)
        .await
        .expect("the manager of the target writes the spec")
        .written()
        .expect("nothing blocks the manager");
    assert_eq!(held().await.as_deref(), Some("stopped"));

    // ── the target is fixed, whoever asks ───────────────────────────────────
    for who in [&manager, &outsider] {
        let err = store
            .update_fact(
                &claim.address(),
                edit(&[(TARGET_BOT, outsider.as_str())]),
                who,
            )
            .await
            .expect_err("the target of a request never moves");
        refused(err, "moving the target");
    }

    // ── the status is no bot's ──────────────────────────────────────────────
    let err = store
        .update_fact(
            &claim.address(),
            edit(&[(OBSERVED_STATE, "running")]),
            &manager,
        )
        .await
        .expect_err("no bot writes the status of a request");
    assert!(
        matches!(
            err,
            MemoryError::KeyNotYours {
                may: MayWrite::RequestStatus,
                ..
            }
        ),
        "{err:?}"
    );

    // ── on any other record the same words are nobody's business ────────────
    let note = capture(
        store,
        NewFact::about(plain.clone(), "who runs it", date(2026, 10, 3)),
    )
    .await;
    store
        .update_fact(
            &note.address(),
            edit(&[(DESIRED, "x"), (OBSERVED_STATE, "y")]),
            &outsider,
        )
        .await
        .expect("an ordinary record takes the keys from any bot")
        .written()
        .expect("nothing blocks it");
}

/// **A request's spec does not move onto another thing by a merge.** The
/// survivor of a merge is no request, so the keys of the spec would land
/// unjudged; the merge is refused for every caller, the manager included.
/// **Paired**: two ordinary records fold with the same words.
pub async fn a_bot_request_is_not_folded_into_another_thing<M: Memory>(store: &M) {
    let request = EntityId("bot-request:contract-swallow-ask".into());
    let survivor = EntityId("topic:contract-swallow-survivor".into());
    let note = EntityId("topic:contract-swallow-note".into());
    let manager = EntityId("bot:contract-swallow-manager".into());
    let target = EntityId("bot:contract-swallow-target".into());
    let outsider = EntityId("bot:contract-swallow-outsider".into());
    for (id, name) in [
        (&request, "Swallow Ask"),
        (&survivor, "Swallow Survivor"),
        (&note, "Swallow Note"),
        (&manager, "Swallow Manager"),
        (&target, "Swallow Target"),
        (&outsider, "Swallow Outsider"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let fields = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    capture(
        store,
        state(NewFact {
            fields: fields(&[(crate::memory::REPORTS_TO, manager.as_str())]),
            ..NewFact::about(
                target.clone(),
                "target reports to manager",
                date(2026, 10, 1),
            )
        }),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: fields(&[(TARGET_BOT, target.as_str()), (DESIRED, "running")]),
            ..NewFact::about(request.clone(), "the target should run", date(2026, 10, 2))
        }),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: fields(&[(DESIRED, "x"), (OBSERVED_STATE, "y")]),
            ..NewFact::about(note.clone(), "an ordinary record", date(2026, 10, 3))
        }),
    )
    .await;

    for who in [&outsider, &manager] {
        let err = store
            .merge(&request, &survivor, None, date(2026, 10, 4), who, None)
            .await
            .expect_err("a request is not folded into another thing");
        assert!(
            matches!(err, MemoryError::MergeCarriesGuardedKeys { .. }),
            "{who}: {err:?}"
        );
    }
    store
        .merge(&note, &survivor, None, date(2026, 10, 4), &outsider, None)
        .await
        .expect("an ordinary record folds with the same words");
}
