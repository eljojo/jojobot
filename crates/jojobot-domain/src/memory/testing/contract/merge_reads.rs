//! **Every read answers a folded thing as its survivor** (card 2332). A merge
//! writes only an event, so each read that meets the folded thing has to ask the
//! one place that knows where it went. These cases are the reads that used to
//! walk the merges themselves, each paired with the read of a thing nobody
//! merged so a build that rewrote every answer cannot pass.

use super::base::thing_fields;
use super::support::{add, capture, other_caller, state, state_patch};
use super::*;

/// The handle of a fixture built from a slug, in the way the neighbouring cases
/// build theirs.
fn person(slug: &str) -> EntityId {
    EntityId::person(format!("person:{slug}"))
}

fn bot(slug: &str) -> EntityId {
    EntityId(format!("bot:{slug}"))
}

/// **What was built on a claim is found from any address the claim has had, and
/// from the one it is served at.** A claim made on the folded thing before the
/// merge names the folded claim's old address; one made after names the address
/// the survivor serves it at. Both are built on the same claim.
/// **Paired**: a claim built on the survivor's own claim is not among them.
pub async fn what_was_built_on_a_folded_claim_is_found_from_every_address_it_has_had<M: Memory>(
    store: &M,
) {
    let survivor = person("contract-built-on-fold-keeper");
    let folded = person("contract-built-on-fold-mover");
    let watcher = person("contract-built-on-fold-watcher");
    for id in [&survivor, &folded, &watcher] {
        add(
            store,
            NewEntity::new(
                id.clone(),
                id.as_str().replace([':', '-'], " "),
                "contract-fixture",
            ),
        )
        .await;
    }
    let own = capture(
        store,
        NewFact::about(
            survivor.clone(),
            "keeps a cobalt notebook",
            date(2026, 6, 1),
        ),
    )
    .await;
    let owed = capture(
        store,
        NewFact::about(folded.clone(), "owes a dollar", date(2026, 6, 2)),
    )
    .await;
    capture(
        store,
        NewFact {
            derived_from: Some(owed.address()),
            ..NewFact::about(watcher.clone(), "so the school bus waits", date(2026, 6, 2))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            derived_from: Some(own.address()),
            ..NewFact::about(watcher.clone(), "so the notebook is lent", date(2026, 6, 2))
        },
    )
    .await;
    store
        .merge(
            &folded,
            &survivor,
            Some("one boy, filed twice"),
            date(2026, 6, 3),
            &other_caller(),
            None,
        )
        .await
        .expect("the fold lands");
    let served = store
        .moved_address(&owed.address())
        .await
        .expect("reads")
        .expect("the claim moved with the merge");
    capture(
        store,
        NewFact {
            derived_from: Some(served.clone()),
            ..NewFact::about(watcher.clone(), "and the bell rings", date(2026, 6, 4))
        },
    )
    .await;

    for from in [owed.address(), served] {
        let mut built: Vec<String> = store
            .built_on(&from)
            .await
            .expect("built_on reads")
            .into_iter()
            .map(|fact| fact.content)
            .collect();
        built.sort();
        assert_eq!(
            built,
            ["and the bell rings", "so the school bus waits"],
            "what was built on the folded claim, from {from}"
        );
    }
    let on_own: Vec<String> = store
        .built_on(&own.address())
        .await
        .expect("built_on reads")
        .into_iter()
        .map(|fact| fact.content)
        .collect();
    assert_eq!(
        on_own,
        ["so the notebook is lent"],
        "the survivor's own claim keeps only what was built on it"
    );
}

/// **A bot that reported to a folded bot reports to the survivor.** Its manager
/// reads as the survivor, and the survivor is licensed to move the bot as the
/// manager it is: the chart is judged against the thing the folded manager
/// became. **Paired** with a stranger, who is refused the same write.
pub async fn a_bot_that_reported_to_a_folded_bot_reports_to_the_survivor<M: Memory>(store: &M) {
    let folded = bot("contract-fold-manager-vanishing");
    let survivor = bot("contract-fold-manager-surviving");
    let report = bot("contract-fold-manager-reporting");
    let stranger = bot("contract-fold-manager-outsider");
    let elsewhere = bot("contract-fold-manager-elsewhere");
    for (id, name) in [
        (&folded, "Fold Manager Vanishing"),
        (&survivor, "Fold Manager Surviving"),
        (&report, "Fold Manager Reporting"),
        (&stranger, "Fold Manager Outsider"),
        (&elsewhere, "Fold Manager Elsewhere"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let reporting = |manager: &EntityId| -> std::collections::BTreeMap<String, String> {
        [(crate::memory::REPORTS_TO.to_string(), manager.to_string())]
            .into_iter()
            .collect()
    };
    let placed = capture(
        store,
        state(NewFact {
            fields: reporting(&folded),
            ..NewFact::about(report.clone(), "reports to the manager", date(2026, 10, 1))
        }),
    )
    .await;
    store
        .merge(&folded, &survivor, None, date(2026, 10, 2), &survivor, None)
        .await
        .expect("the fold lands");

    let fields = thing_fields(store, &report).await;
    assert_eq!(
        fields.get(crate::memory::REPORTS_TO),
        Some(&survivor.to_string()),
        "the bot's manager does not read as the survivor: {fields:?}"
    );
    let at = store
        .moved_address(&placed.address())
        .await
        .expect("reads")
        .unwrap_or_else(|| placed.address());
    let move_to_elsewhere = |caller: EntityId| {
        let patch = state_patch(FactPatch {
            fields: reporting(&elsewhere),
            ..Default::default()
        });
        let at = at.clone();
        async move { store.update_fact(&at, patch, &caller).await }
    };
    let refused = move_to_elsewhere(stranger.clone()).await;
    assert!(
        matches!(refused, Err(MemoryError::KeyNotYours { .. })),
        "a stranger moves the bot: {:?}",
        refused.map(|_| ())
    );
    let landed = move_to_elsewhere(survivor.clone()).await;
    assert!(
        matches!(landed, Ok(Guarded::Written(_))),
        "the survivor is not licensed as the manager the bot had: {:?}",
        landed.map(|_| ())
    );
}

/// **A manager folded through a chain longer than the chart's own bound is the
/// one it became.** The chart reads a chain of reports at most so far; a chain
/// of merges has no such limit, so a bot whose manager was folded thirty-three
/// times still reports to the last survivor, and that survivor is licensed to
/// move it. Standing outside the contract's list because a real store takes a
/// minute to make that many merges.
pub async fn a_manager_folded_through_a_long_chain_is_the_bot_it_became<M: Memory>(store: &M) {
    const MANAGERS: usize = 34;
    let mut chain = Vec::with_capacity(MANAGERS);
    // Slugs within two edits of each other are a near-miss the guard blocks, so
    // each ends in a word of six letters: a letter three times, then another three
    // times, at least three edits from any other word of the set.
    for n in 0..MANAGERS {
        let letter = |digit: usize| char::from(b'a' + digit as u8).to_string().repeat(3);
        let word = format!("{}{}", letter(n / 11), letter(n % 11));
        let id = bot(&format!("contract-fold-chain-manager-{word}"));
        add(
            store,
            NewEntity::new(
                id.clone(),
                format!("Fold Chain Manager {word}"),
                "contract-fixture",
            ),
        )
        .await;
        chain.push(id);
    }
    let report = bot("contract-fold-chain-reporting");
    let elsewhere = bot("contract-fold-chain-elsewhere");
    for (id, name) in [
        (&report, "Fold Chain Reporting"),
        (&elsewhere, "Fold Chain Elsewhere"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }
    let reporting = |manager: &EntityId| -> std::collections::BTreeMap<String, String> {
        [(crate::memory::REPORTS_TO.to_string(), manager.to_string())]
            .into_iter()
            .collect()
    };
    let placed = capture(
        store,
        state(NewFact {
            fields: reporting(&chain[0]),
            ..NewFact::about(report.clone(), "reports to the first", date(2026, 10, 1))
        }),
    )
    .await;
    for pair in chain.windows(2) {
        store
            .merge(&pair[0], &pair[1], None, date(2026, 10, 2), &pair[1], None)
            .await
            .expect("the fold lands");
    }
    let last = &chain[MANAGERS - 1];

    let fields = thing_fields(store, &report).await;
    assert_eq!(
        fields.get(crate::memory::REPORTS_TO),
        Some(&last.to_string()),
        "the bot's manager is not the last survivor of the chain: {fields:?}"
    );
    let at = store
        .moved_address(&placed.address())
        .await
        .expect("reads")
        .unwrap_or_else(|| placed.address());
    let moved = store
        .update_fact(
            &at,
            state_patch(FactPatch {
                fields: reporting(&elsewhere),
                ..Default::default()
            }),
            last,
        )
        .await;
    assert!(
        matches!(moved, Ok(Guarded::Written(_))),
        "the last survivor is not licensed as the manager the bot had: {:?}",
        moved.map(|_| ())
    );
}
