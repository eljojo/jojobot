//! **Who may write the operator key, proven against any [`Memory`] adapter.**
//!
//! On the instance's own record, anybody names the operator while nobody is
//! named. Once somebody is, only the bot that heads the chart changes it or
//! takes it off, and a refusal names that bot, or says what makes one. On any
//! other record the key is nobody's business. The edit, the retraction and the
//! merge each read the chart inside their own transaction, so each is held here,
//! by the fake and by the real store alike. Every refusal sits beside the same
//! write made where it is allowed, so a store that refused every write of the key
//! would not pass.

use super::support::{add, capture, ensure};
use super::*;
use crate::memory::MayWrite;

/// **The edit, the retraction and the merge answer the same way.**
pub async fn the_operator_is_named_by_any_bot_and_changed_only_by_the_head_of_the_chart<
    M: Memory,
>(
    store: &M,
) {
    let record = EntityId(crate::memory::INSTANCE_RECORD.into());
    let spare = EntityId("topic:contract-operator-spare".into());
    let second = EntityId("topic:contract-operator-second".into());
    let plain = EntityId("topic:contract-operator-plain".into());
    let one = EntityId("person:contract-operator-one".into());
    let two = EntityId("person:contract-operator-two".into());
    let head = EntityId("bot:contract-operator-head".into());
    let report = EntityId("bot:contract-operator-report".into());
    let other = EntityId("bot:contract-operator-other".into());
    let leaf = EntityId("bot:contract-operator-underling".into());
    ensure(store, &record).await;
    for (id, name) in [
        (&spare, "Spare Carrier"),
        (&second, "Second Courier"),
        (&plain, "Ordinary Plank"),
        (&one, "Operator One"),
        (&two, "Operator Two"),
        (&head, "Operator Head"),
        (&report, "Operator Report"),
        (&other, "Operator Other"),
        (&leaf, "Bottom Rung"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let naming = |who: &EntityId| -> std::collections::BTreeMap<String, String> {
        [(crate::memory::OPERATOR.to_string(), who.to_string())]
            .into_iter()
            .collect()
    };
    let edit = |who: &EntityId| FactPatch {
        fields: naming(who),
        ..Default::default()
    };
    let off = FactPatch {
        clear_fields: vec![crate::memory::OPERATOR.to_string()],
        ..Default::default()
    };
    // The refusal, and the bots it names as the ones that may make the write.
    let refused = |err: MemoryError, what: &str| -> Vec<String> {
        match err {
            MemoryError::KeyNotYours {
                key,
                may: MayWrite::HeadOnceHeld { .. },
                allowed,
                ..
            } if key == crate::memory::OPERATOR => allowed,
            other => panic!("{what}: expected KeyNotYours for the operator key, got {other:?}"),
        }
    };
    let operator_of = |fold: std::collections::BTreeMap<String, String>| {
        fold.get(crate::memory::OPERATOR).map(|held| {
            held.trim_start_matches(crate::memory::mention::MARK)
                .to_string()
        })
    };
    let held_on =
        |id: EntityId| async move { operator_of(store.fields(&id).await.expect("readable")) };

    // A claim on the record that names nobody, and two carriers that each name
    // somebody. The carriers are other records, where the key is free.
    let claim = capture(
        store,
        NewFact::about(record.clone(), "who the operator is", date(2026, 10, 1)),
    )
    .await;
    for (carrier, who) in [(&spare, &one), (&second, &two)] {
        capture(
            store,
            NewFact {
                fields: naming(who),
                ..NewFact::about(carrier.clone(), "who the operator is", date(2026, 10, 1))
            },
        )
        .await;
    }

    // ── nobody is named: any bot names the operator, by a merge too ─────────
    store
        .merge(&spare, &record, None, date(2026, 10, 2), &other)
        .await
        .expect("a merge that names the operator lands while nobody is named");
    assert_eq!(held_on(record.clone()).await, Some(one.to_string()));

    // ── a head appears: report reports to head, and head reports to none ────
    //
    // Leaf reports to report, so report has a report of its own and still has a
    // manager above it: it is not the head.
    for (who, to, said) in [
        (&report, &head, "report reports to head"),
        (&leaf, &report, "leaf reports to report"),
    ] {
        capture(
            store,
            NewFact {
                fields: [(crate::memory::REPORTS_TO.to_string(), to.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(who.clone(), said, date(2026, 10, 3))
            },
        )
        .await;
    }

    // ── named: a bot that is not the head cannot change it, and the refusal
    //    names the head ──────────────────────────────────────────────────────
    for who in [&other, &report] {
        let err = store
            .update_fact(&claim.address(), edit(&two), who)
            .await
            .expect_err("only the head changes a named operator");
        assert!(
            refused(err, &format!("an edit by {who}")).contains(&head.to_string()),
            "the refusal names the head"
        );
    }
    assert_eq!(held_on(record.clone()).await, Some(one.to_string()));

    // ── the head changes it ─────────────────────────────────────────────────
    store
        .update_fact(&claim.address(), edit(&two), &head)
        .await
        .expect("the head changes a named operator")
        .written()
        .expect("nothing blocks the head");
    assert_eq!(held_on(record.clone()).await, Some(two.to_string()));

    // ── nor take it off, now that the record's own claim wrote it ───────────
    let err = store
        .update_fact(&claim.address(), off.clone(), &other)
        .await
        .expect_err("only the head takes a named operator off");
    refused(err, "taking it off");
    assert_eq!(held_on(record.clone()).await, Some(two.to_string()));

    // ── nor can another bot retract the record that now names it ────────────
    let err = store
        .retract(
            &claim.address(),
            Some("a mistake"),
            date(2026, 10, 4),
            &other,
        )
        .await
        .expect_err("only the head retracts the record that names the operator");
    refused(err, "a retraction");
    assert_eq!(held_on(record.clone()).await, Some(two.to_string()));

    // ── a merge cannot carry the key onto a record that holds it ────────────
    let err = store
        .merge(&second, &record, None, date(2026, 10, 5), &other)
        .await
        .expect_err("only the head merges a named operator onto a named record");
    assert!(
        matches!(
            &err,
            MemoryError::MergeCarriesGuardedKeys { keys, may: MayWrite::HeadOnceHeld { .. }, .. }
                if keys.contains(crate::memory::OPERATOR)
        ),
        "expected MergeCarriesGuardedKeys for the operator key, got {err:?}"
    );
    assert!(
        store
            .fields(&second)
            .await
            .expect("readable")
            .contains_key(crate::memory::OPERATOR),
        "the refused merge moved something off the duplicate"
    );

    // ── the head takes it off ───────────────────────────────────────────────
    store
        .update_fact(&claim.address(), off, &head)
        .await
        .expect("the head takes the operator off")
        .written()
        .expect("nothing blocks the head");
    assert_eq!(held_on(record.clone()).await, None);

    // ── on any other record the key is nobody's business ────────────────────
    //
    // Named, changed by a bot that heads nothing, in a store that has a head.
    let note = capture(
        store,
        NewFact::about(plain.clone(), "who runs it", date(2026, 10, 6)),
    )
    .await;
    store
        .update_fact(&note.address(), edit(&one), &other)
        .await
        .expect("a bot names it on an ordinary record")
        .written()
        .expect("nothing blocks it");
    store
        .update_fact(&note.address(), edit(&two), &other)
        .await
        .expect("and changes it, because it is not guarded there")
        .written()
        .expect("nothing blocks it");
    assert_eq!(held_on(plain).await, Some(two.to_string()));
    // The head's own merge onto a record that holds the key is not asked here:
    // merging two records that both hold one key fails in the real store for a
    // reason that has nothing to do with this guard.
}
