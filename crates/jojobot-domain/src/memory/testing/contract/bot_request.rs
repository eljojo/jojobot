//! **Who may write a bot request, proven against any [`Memory`] adapter.**
//!
//! A request has no manager of its own: its spec is judged by the chain above
//! its target bot, which each store reads inside the edit's own transaction. So
//! the manager of the target writes the spec, a bot outside that chain is
//! refused and the refusal names the manager, the target never writes its own
//! request, and the status is no bot's. The same words on an ordinary record are
//! nobody's business. Every refusal sits beside the same write made where it is
//! allowed, so a store that refused every write of the keys would not pass.

use super::support::{add, capture, other_caller, state, state_patch};
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

/// **The first target of a request is judged by the target's chain, not by a
/// manager the request already holds.** A request that holds a `reports_to` and
/// no target takes its target in a later edit; the bot above the held manager is
/// not above the target, so it does not write it. The target's own manager does.
pub async fn a_first_target_is_judged_by_the_target_and_not_a_held_manager<M: Memory>(store: &M) {
    let request = EntityId("bot-request:contract-bobbin-ask".into());
    let held_boss = EntityId("bot:contract-bobbin-boss".into());
    let held = EntityId("bot:contract-bobbin-held".into());
    let chief = EntityId("bot:contract-bobbin-chief".into());
    let target = EntityId("bot:contract-bobbin-target".into());
    for (id, name) in [
        (&request, "Bobbin Ask"),
        (&held_boss, "Bobbin Boss"),
        (&held, "Bobbin Held"),
        (&chief, "Bobbin Chief"),
        (&target, "Bobbin Target"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let fields = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    for (bot, boss) in [(&held, &held_boss), (&target, &chief)] {
        capture(
            store,
            state(NewFact {
                fields: fields(&[(crate::memory::REPORTS_TO, boss.as_str())]),
                ..NewFact::about(bot.clone(), "reports to its manager", date(2026, 10, 1))
            }),
        )
        .await;
    }
    let claim = capture(
        store,
        state(NewFact {
            fields: fields(&[(crate::memory::REPORTS_TO, held.as_str())]),
            ..NewFact::about(
                request.clone(),
                "a request without a target",
                date(2026, 10, 2),
            )
        }),
    )
    .await;
    let first_target = || {
        state_patch(FactPatch {
            fields: fields(&[(TARGET_BOT, target.as_str())]),
            ..Default::default()
        })
    };

    let err = store
        .update_fact(&claim.address(), first_target(), &held_boss)
        .await
        .expect_err("the manager of the held bot is not above the target");
    assert!(
        matches!(
            err,
            MemoryError::KeyNotYours {
                may: MayWrite::RequestSpec,
                ..
            }
        ),
        "{err:?}"
    );
    let fold = store.fields(&request).await.expect("readable");
    assert_eq!(
        fold.get(TARGET_BOT),
        None,
        "the refused target was not kept"
    );

    store
        .update_fact(&claim.address(), first_target(), &chief)
        .await
        .expect("the manager of the target writes its first target")
        .written()
        .expect("nothing blocks the target's manager");
}

/// **A claim taken back under the handle its request wore before a rename still
/// carries its number.** The retraction resolves the address as the plain
/// retraction does, and the companion claim names the same request by the
/// spelling the caller typed; both land, and the fold names the new spec.
pub async fn a_retraction_under_a_former_handle_carries_its_generation_claim<M: Memory>(store: &M) {
    use crate::memory::bot_request::GENERATION;
    let request = EntityId("bot-request:contract-former-ask".into());
    let renamed = EntityId("bot-request:contract-former-now".into());
    let manager = EntityId("bot:contract-former-manager".into());
    let target = EntityId("bot:contract-former-target".into());
    for (id, name) in [
        (&request, "Former Ask"),
        (&manager, "Former Manager"),
        (&target, "Former Target"),
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
    let spec = |subject: &EntityId, pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(subject.clone(), "a spec", date(2026, 10, day))
        });
        fact.derived.insert(GENERATION.to_string());
        fact
    };
    capture(
        store,
        spec(
            &request,
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    let second = capture(
        store,
        spec(&request, &[(DESIRED, "stopped"), (GENERATION, "2")], 3),
    )
    .await;
    store
        .rename_entity(
            &request,
            &renamed,
            None,
            date(2026, 10, 4),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the new handle");

    let (retraction, claim) = store
        .retract_with_claim(
            &second.address(),
            Some("written in error"),
            date(2026, 10, 5),
            &manager,
            None,
            spec(&request, &[(GENERATION, "3")], 5),
        )
        .await
        .expect("the call answers")
        .written()
        .expect("nothing blocks it");
    assert_eq!(retraction.retracted.subject, renamed);
    assert_eq!(
        claim.subject, renamed,
        "the companion is served under the handle now"
    );
    let fold = store.fields(&renamed).await.expect("readable");
    assert_eq!(fold.get(DESIRED).map(String::as_str), Some("running"));
    assert_eq!(fold.get(GENERATION).map(String::as_str), Some("3"));
}

/// **A number the generation once held is still on the request's history after
/// the claim that carried it is retracted**, so the next generation is one past the
/// highest ever written and no number names two specs. Spec A is written with 1,
/// spec B with 2, B's claim is retracted, and spec C must come out as 3: the fold
/// reads 1 again, and a number taken from it would repeat 2.
pub async fn a_retracted_generation_is_still_the_highest_ever_written<M: Memory>(store: &M) {
    use crate::memory::bot_request::{
        GENERATION, highest_generation_ever, next_generation, spec_after, spec_of,
    };
    let request = EntityId("bot-request:contract-retract-ask".into());
    let manager = EntityId("bot:contract-retract-manager".into());
    let target = EntityId("bot:contract-retract-target".into());
    for (id, name) in [
        (&request, "Retract Ask"),
        (&manager, "Retract Manager"),
        (&target, "Retract Target"),
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
    // The generation rides in the claim of the spec it counts, as a verb writes it.
    let spec = |pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(request.clone(), "a spec", date(2026, 10, day))
        });
        fact.derived.insert(GENERATION.to_string());
        fact
    };
    capture(
        store,
        spec(
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    let second = capture(store, spec(&[(DESIRED, "stopped"), (GENERATION, "2")], 3)).await;
    let read = || async { store.fields(&request).await.expect("readable") };
    let highest = || async {
        highest_generation_ever(
            &store
                .history(&request, GENERATION)
                .await
                .expect("the history reads"),
        )
    };
    assert_eq!(highest().await, 2, "the positive the rest rests on");

    store
        .retract(
            &second.address(),
            Some("written in error"),
            date(2026, 10, 4),
            &manager,
            None,
        )
        .await
        .expect("the manager retracts the second spec");
    let fold = read().await;
    assert_eq!(
        fold.get(GENERATION).map(String::as_str),
        Some("1"),
        "the fold reads the spec that stands"
    );
    assert_eq!(
        highest().await,
        2,
        "the retracted claim's number is still the highest ever written"
    );
    let third = next_generation(
        &request,
        &spec_of(&fold),
        &spec_after(&fold, &fields(&[(DESIRED, "paused")]), &[]),
        highest().await,
    );
    assert_eq!(third.as_deref(), Some("3"), "2 names the retracted spec");
}

/// **An archive and the claim that carries the new number land as one act.** The
/// same sequence as a retraction: taking the second claim off by an archive
/// changes the folded spec, so the claim written beside the edit names it 4. A
/// companion claim a guard refuses takes the archive back with it.
pub async fn an_archive_and_its_generation_claim_land_as_one_act<M: Memory>(store: &M) {
    use crate::memory::bot_request::{GENERATION, MODEL};
    let request = EntityId("bot-request:contract-archive-ask".into());
    let manager = EntityId("bot:contract-archive-manager".into());
    let target = EntityId("bot:contract-archive-target".into());
    for (id, name) in [
        (&request, "Archive Ask"),
        (&manager, "Archive Manager"),
        (&target, "Archive Target"),
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
    let spec = |pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(request.clone(), "a spec", date(2026, 10, day))
        });
        fact.derived.insert(GENERATION.to_string());
        fact
    };
    capture(
        store,
        spec(
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (MODEL, "alpha"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    let second = capture(store, spec(&[(DESIRED, "stopped"), (GENERATION, "2")], 3)).await;
    capture(store, spec(&[(MODEL, "beta"), (GENERATION, "3")], 4)).await;
    let read = || async { store.fields(&request).await.expect("readable") };
    let archive = || FactPatch {
        status: Some(FactStatus::Archived),
        ..Default::default()
    };

    let mut refused = spec(&[(GENERATION, "4")], 5);
    refused
        .fields
        .insert("blocks".into(), "work:contract-archive-nobody".into());
    refused.sets.insert("blocks".into());
    let taken_back = store
        .update_fact_with_claim(&second.address(), archive(), &manager, refused)
        .await
        .expect("the call answers");
    assert!(
        matches!(taken_back, Guarded::Blocked { .. }),
        "a refused companion blocks the act: {taken_back:?}",
    );
    let fold = read().await;
    assert_eq!(
        fold.get(DESIRED).map(String::as_str),
        Some("stopped"),
        "the archive was taken back with the refused claim",
    );
    assert_eq!(fold.get(GENERATION).map(String::as_str), Some("3"));

    let (archived, claim) = store
        .update_fact_with_claim(
            &second.address(),
            archive(),
            &manager,
            spec(&[(GENERATION, "4")], 5),
        )
        .await
        .expect("the call answers")
        .written()
        .expect("nothing blocks it");
    assert_eq!(archived.status, FactStatus::Archived);
    assert_eq!(claim.subject, request);
    let fold = read().await;
    assert_eq!(fold.get(DESIRED).map(String::as_str), Some("running"));
    assert_eq!(fold.get(MODEL).map(String::as_str), Some("beta"));
    assert_eq!(
        fold.get(GENERATION).map(String::as_str),
        Some("4"),
        "the archive and its number landed together",
    );
}

/// **A merge and the claim that carries the new number land as one act.** An
/// ordinary note that holds a `model` is merged into a request that never wrote
/// one, so the merge changes the folded spec whichever side wrote last, and the
/// claim written beside it names 2. A companion claim a guard refuses takes the
/// merge back with it: the note is not merged and the request holds no model.
pub async fn a_merge_and_its_generation_claim_land_as_one_act<M: Memory>(store: &M) {
    use crate::memory::bot_request::{GENERATION, MODEL};
    let request = EntityId("bot-request:contract-fusion-ask".into());
    let note = EntityId("topic:contract-fusion-note".into());
    let manager = EntityId("bot:contract-fusion-manager".into());
    let target = EntityId("bot:contract-fusion-target".into());
    for (id, name) in [
        (&request, "Fusion Ask"),
        (&note, "Fusion Note"),
        (&manager, "Fusion Manager"),
        (&target, "Fusion Target"),
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
    let spec = |subject: &EntityId, pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(subject.clone(), "a spec", date(2026, 10, day))
        });
        if subject == &request {
            fact.derived.insert(GENERATION.to_string());
        }
        fact
    };
    capture(
        store,
        spec(
            &request,
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    capture(store, spec(&note, &[(MODEL, "beta")], 3)).await;
    let read = || async { store.fields(&request).await.expect("readable") };

    let mut refused = spec(&request, &[(GENERATION, "2")], 4);
    refused
        .fields
        .insert("blocks".into(), "work:contract-fusion-nobody".into());
    refused.sets.insert("blocks".into());
    let taken_back = store
        .merge_with_claim(&note, &request, None, date(2026, 10, 4), &manager, refused)
        .await
        .expect("the call answers");
    assert!(
        matches!(taken_back, Guarded::Blocked { .. }),
        "a refused companion blocks the act: {taken_back:?}",
    );
    let fold = read().await;
    assert_eq!(
        fold.get(MODEL),
        None,
        "the merge was taken back with the refused claim",
    );
    assert_eq!(fold.get(GENERATION).map(String::as_str), Some("1"));
    assert_eq!(
        store.fields(&note).await.expect("readable").get(MODEL),
        Some(&"beta".to_string()),
        "the note still holds its own model",
    );

    // A claim about another thing than the survivor is no companion of this
    // merge: the act is refused and the merge is taken back with it.
    let elsewhere = store
        .merge_with_claim(
            &note,
            &request,
            None,
            date(2026, 10, 4),
            &manager,
            spec(&target, &[], 4),
        )
        .await
        .expect_err("a claim about the target is not about the survivor");
    assert!(
        matches!(elsewhere, MemoryError::InvalidFact(_)),
        "{elsewhere:?}"
    );
    assert_eq!(
        read().await.get(MODEL),
        None,
        "the merge was taken back with the misdirected claim",
    );

    let (merged, claim) = store
        .merge_with_claim(
            &note,
            &request,
            None,
            date(2026, 10, 4),
            &manager,
            spec(&request, &[(GENERATION, "2")], 4),
        )
        .await
        .expect("the call answers")
        .written()
        .expect("nothing blocks it");
    assert_eq!(merged.folded, note);
    assert_eq!(claim.subject, request);
    let fold = read().await;
    assert_eq!(fold.get(MODEL).map(String::as_str), Some("beta"));
    assert_eq!(fold.get(DESIRED).map(String::as_str), Some("running"));
    assert_eq!(
        fold.get(GENERATION).map(String::as_str),
        Some("2"),
        "the merge and its number landed together",
    );

    // **The number is read from the request alone, so a duplicate that holds one
    // of its own must not be folded in**: nobody may write a generation, and the
    // merge that would carry one onto the request is refused.
    let stamped = EntityId("topic:contract-fusion-stamped".into());
    add(
        store,
        NewEntity::new(stamped.clone(), "Fusion Stamped", "contract-fixture"),
    )
    .await;
    capture(store, spec(&stamped, &[(GENERATION, "9")], 5)).await;
    let refused = store
        .merge_with_claim(
            &stamped,
            &request,
            None,
            date(2026, 10, 6),
            &manager,
            spec(&request, &[(GENERATION, "3")], 6),
        )
        .await
        .expect_err("a duplicate that holds a generation is not folded into a request");
    assert!(
        matches!(refused, MemoryError::MergeCarriesGuardedKeys { .. }),
        "{refused:?}"
    );
    assert_eq!(read().await.get(GENERATION).map(String::as_str), Some("2"));
}

/// **A retraction and the claim that carries the new number land as one act.**
/// The sequence is a whole spec (1), a partial one (2) and another partial one
/// (3); taking the second back changes the folded spec, so the claim written
/// beside the retraction names it 4. A companion claim a guard refuses takes the
/// retraction back with it, so a withdrawal is never stored without its number.
pub async fn a_retraction_and_its_generation_claim_land_as_one_act<M: Memory>(store: &M) {
    use crate::memory::bot_request::{GENERATION, MODEL};
    let request = EntityId("bot-request:contract-pair-ask".into());
    let manager = EntityId("bot:contract-pair-manager".into());
    let target = EntityId("bot:contract-pair-target".into());
    for (id, name) in [
        (&request, "Pair Ask"),
        (&manager, "Pair Manager"),
        (&target, "Pair Target"),
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
    let spec = |pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(request.clone(), "a spec", date(2026, 10, day))
        });
        fact.derived.insert(GENERATION.to_string());
        fact
    };
    capture(
        store,
        spec(
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (MODEL, "alpha"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    let second = capture(store, spec(&[(DESIRED, "stopped"), (GENERATION, "2")], 3)).await;
    capture(store, spec(&[(MODEL, "beta"), (GENERATION, "3")], 4)).await;
    let read = || async { store.fields(&request).await.expect("readable") };
    let companion = |number: &str| spec(&[(GENERATION, number)], 5);

    // A companion a guard refuses: its key links to a handle nobody holds.
    let mut refused = companion("4");
    refused
        .fields
        .insert("blocks".into(), "work:contract-pair-nobody".into());
    refused.sets.insert("blocks".into());
    let taken_back = store
        .retract_with_claim(
            &second.address(),
            Some("written in error"),
            date(2026, 10, 5),
            &manager,
            None,
            refused,
        )
        .await
        .expect("the call answers");
    assert!(
        matches!(taken_back, Guarded::Blocked { .. }),
        "a refused companion blocks the act: {taken_back:?}",
    );
    let fold = read().await;
    assert_eq!(
        fold.get(DESIRED).map(String::as_str),
        Some("stopped"),
        "the retraction was taken back with the refused claim",
    );
    assert_eq!(fold.get(GENERATION).map(String::as_str), Some("3"));

    // The companion that stands: both land, and the fold names the new spec 4.
    let (retraction, claim) = store
        .retract_with_claim(
            &second.address(),
            Some("written in error"),
            date(2026, 10, 5),
            &manager,
            None,
            companion("4"),
        )
        .await
        .expect("the call answers")
        .written()
        .expect("nothing blocks it");
    assert_eq!(retraction.retracted.address(), second.address());
    assert_eq!(claim.subject, request);
    let fold = read().await;
    assert_eq!(fold.get(DESIRED).map(String::as_str), Some("running"));
    assert_eq!(fold.get(MODEL).map(String::as_str), Some("beta"));
    assert_eq!(
        fold.get(GENERATION).map(String::as_str),
        Some("4"),
        "the withdrawal and its number landed together",
    );
}

/// **Nobody takes back a claim that sets only the generation of a request.** The
/// number is jojobot's own, worked out from each write of the spec and never
/// sent, so a bot that retracted or archived the claim carrying it would roll the
/// folded generation back under a spec that is still the one numbered above it,
/// and a bound app that last acted on the lower number would miss the change.
/// Both ways of taking a claim off are refused for every caller, and the fold
/// stays where it was. The positive is beside it: a claim that holds a spec key
/// as well as the number is still the manager's to retract.
pub async fn a_claim_that_sets_only_the_generation_is_not_taken_back<M: Memory>(store: &M) {
    use crate::memory::bot_request::{GENERATION, MODEL};
    let request = EntityId("bot-request:contract-hole-ask".into());
    let manager = EntityId("bot:contract-hole-manager".into());
    let target = EntityId("bot:contract-hole-target".into());
    for (id, name) in [
        (&request, "Hole Ask"),
        (&manager, "Hole Manager"),
        (&target, "Hole Target"),
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
    let spec = |pairs: &[(&str, &str)], day: i8| {
        let mut fact = state(NewFact {
            fields: fields(pairs),
            ..NewFact::about(request.clone(), "a spec", date(2026, 10, day))
        });
        fact.derived.insert(GENERATION.to_string());
        fact
    };
    capture(
        store,
        spec(
            &[
                (TARGET_BOT, target.as_str()),
                (DESIRED, "running"),
                (GENERATION, "1"),
            ],
            2,
        ),
    )
    .await;
    let second = capture(
        store,
        spec(
            &[(DESIRED, "stopped"), (MODEL, "alpha"), (GENERATION, "2")],
            3,
        ),
    )
    .await;
    // The claim that carries the number alone, as the act that took a spec off
    // writes it: the companion carries the settings label beside the number.
    let mut companion = spec(&[(GENERATION, "3")], 4);
    companion.fields = crate::memory::review::settings_fields(companion.fields);
    let number = capture(store, companion).await;
    let generation = || async {
        store
            .fields(&request)
            .await
            .expect("readable")
            .get(GENERATION)
            .cloned()
    };
    let refused = |err: MemoryError, what: &str| match err {
        MemoryError::KeyNotYours {
            may: MayWrite::RequestGeneration,
            ..
        } => {}
        other => panic!("{what}: expected KeyNotYours for the generation, got {other:?}"),
    };
    assert_eq!(generation().await.as_deref(), Some("3"));

    for who in [&manager, &target] {
        let err = store
            .retract(&number.address(), None, date(2026, 10, 5), who, None)
            .await
            .expect_err("nobody retracts the claim that carries only the generation");
        refused(err, "the retraction");
        assert_eq!(generation().await.as_deref(), Some("3"));

        let err = store
            .update_fact(
                &number.address(),
                state_patch(FactPatch {
                    status: Some(FactStatus::Archived),
                    ..Default::default()
                }),
                who,
            )
            .await
            .expect_err("nobody archives the claim that carries only the generation");
        refused(err, "the archive");
        assert_eq!(generation().await.as_deref(), Some("3"));
    }

    // The claim that holds a spec key as well is still the manager's to retract.
    store
        .retract(&second.address(), None, date(2026, 10, 5), &manager, None)
        .await
        .expect("a claim that carries a spec key is retracted");
    let fold = store.fields(&request).await.expect("readable");
    assert_eq!(fold.get(DESIRED).map(String::as_str), Some("running"));
    assert_eq!(
        fold.get(GENERATION).map(String::as_str),
        Some("3"),
        "the number claim that was refused above still stands"
    );
}

/// **A merge judges the keys a duplicate wrote, not only the keys it holds.** A
/// duplicate that set a guarded key and then cleared it holds nothing under it,
/// but the fold interleaves its whole history, so the clear lands on the
/// survivor and takes the key off it. The merge is refused as if the duplicate
/// still held the key, for a spec key and a status key of a request and for an
/// older ceiling key alike, and nothing moves. The merge lands once the claim
/// that set the key is archived. **Paired**: the same survivor takes a duplicate
/// that never wrote a guarded key.
pub async fn a_merge_judges_a_guarded_key_its_duplicate_set_and_cleared<M: Memory>(store: &M) {
    use crate::memory::THOUGHT_CAPACITY;
    let request = EntityId("bot-request:contract-cleared-ask".into());
    let manager = EntityId("bot:contract-cleared-manager".into());
    let target = EntityId("bot:contract-cleared-target".into());
    let keeper = EntityId("bot:contract-cleared-keeper".into());
    let outsider = EntityId("bot:contract-cleared-outsider".into());
    let plain = EntityId("topic:contract-cleared-plain".into());
    for (id, name) in [
        (&request, "Cleared Ask"),
        (&manager, "Cleared Manager"),
        (&target, "Cleared Target"),
        (&keeper, "Cleared Keeper"),
        (&outsider, "Cleared Outsider"),
        (&plain, "Cleared Plain"),
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
            fields: fields(&[(THOUGHT_CAPACITY, "5")]),
            ..NewFact::about(keeper.clone(), "capacity is five", date(2026, 10, 2))
        }),
    )
    .await;

    // (duplicate, key, value written then cleared, the survivor, the caller who merges)
    let families = [
        (
            "topic:contract-cleared-alpha",
            DESIRED,
            "stopped",
            &request,
            &outsider,
        ),
        (
            "topic:contract-cleared-beta",
            OBSERVED_STATE,
            "bound",
            &request,
            &manager,
        ),
        (
            "topic:contract-cleared-gamma",
            THOUGHT_CAPACITY,
            "9",
            &keeper,
            &keeper,
        ),
    ];
    for (duplicate, key, value, survivor, caller) in families {
        let duplicate = EntityId(duplicate.into());
        add(
            store,
            NewEntity::new(duplicate.clone(), duplicate.slug(), "contract-fixture"),
        )
        .await;
        let set = capture(
            store,
            state(NewFact {
                fields: fields(&[(key, value)]),
                ..NewFact::about(duplicate.clone(), "set a guarded key", date(2026, 10, 3))
            }),
        )
        .await;
        store
            .update_fact(
                &set.address(),
                FactPatch {
                    clear_fields: vec![key.to_string()],
                    ..FactPatch::default()
                },
                &other_caller(),
            )
            .await
            .expect("the key is cleared on the duplicate, where it is nobody's");
        assert!(
            !store
                .fields(&duplicate)
                .await
                .expect("readable")
                .contains_key(key),
            "{key}: the duplicate holds nothing under the key it cleared"
        );
        let before = store.fields(survivor).await.expect("readable");
        let err = store
            .merge(&duplicate, survivor, None, date(2026, 10, 4), caller, None)
            .await
            .expect_err("a clear of a guarded key does not land on the survivor unjudged");
        assert!(
            matches!(
                &err,
                MemoryError::MergeCarriesGuardedKeys { keys, .. } if keys.contains(key)
            ),
            "{key}: {err:?}"
        );
        assert_eq!(
            store.fields(survivor).await.expect("readable"),
            before,
            "{key}: the refused merge moved something onto the survivor"
        );
        // **The route the refusal names**: the clear above left its own write in
        // the history, so the merge was refused after it. Archiving the claim that
        // set the key takes the writes out of the history, and the merge lands
        // without the key on the survivor.
        store
            .update_fact(
                &set.address(),
                FactPatch {
                    status: Some(FactStatus::Archived),
                    ..FactPatch::default()
                },
                caller,
            )
            .await
            .expect("the claim that set the key is archived");
        store
            .merge(&duplicate, survivor, None, date(2026, 10, 4), caller, None)
            .await
            .unwrap_or_else(|err| {
                panic!("{key}: an archived claim wrote nothing that counts: {err:?}")
            });
        assert_eq!(
            store.fields(survivor).await.expect("readable").get(key),
            before.get(key),
            "{key}: the merge put the archived key on the survivor"
        );
    }
    store
        .merge(&plain, &request, None, date(2026, 10, 5), &outsider, None)
        .await
        .expect("a duplicate that never wrote a guarded key folds into the same request");
}
