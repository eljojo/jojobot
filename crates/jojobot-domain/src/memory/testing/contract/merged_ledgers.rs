//! **A merged bot's newest write counts its real writes and none of its ledgers**
//! (card 2845), proven against any [`Memory`] adapter. A bot's claim ids are local
//! to it, so after a merge the survivor's claim `f2` and the folded bot's claim
//! `f2` are two records. The ledger of release notes is found by the label the
//! record holds, so a real write is never taken for a ledger by sharing an id with
//! one, and the folded bot's ledger is not taken for a change.

use super::support::{add, capture, edit, other_caller};
use super::*;
use crate::memory::RELEASE_NOTES_TOLD;
use crate::memory::capacity::{ENGINE_RECORD, RELEASE_NOTES_RECORD};

/// The newest moment of any write on `bot` that counts as a change of it.
async fn newest<M: Memory>(store: &M, bot: &EntityId) -> Option<jiff::Timestamp> {
    store.field_ages(bot).await.expect("ages read").newest
}

/// A bot's record of the release notes it was told.
async fn ledger<M: Memory>(store: &M, bot: &EntityId, told: &str) -> Fact {
    capture(
        store,
        NewFact {
            fields: [
                (ENGINE_RECORD.to_string(), RELEASE_NOTES_RECORD.to_string()),
                (RELEASE_NOTES_TOLD.to_string(), told.to_string()),
            ]
            .into_iter()
            .collect(),
            sets: [RELEASE_NOTES_TOLD.to_string()].into_iter().collect(),
            ..NewFact::about(bot.clone(), "what this bot was told", date(2026, 10, 1))
        },
    )
    .await
}

/// **Two bots, each with a ledger and a real write sharing a local id, merged.**
/// Delta holds a real write `f1` and a ledger `f2`; lisa holds a ledger `f1` and a
/// real write `f2`. Folded into delta, a bot's claim ids are no longer one space:
/// lisa's `f2` is a real write and delta's `f2` is a ledger. After the merge, a
/// rewrite of either ledger alone moves nothing, and a rewrite of lisa's real
/// write moves the merged bot's newest write. The merge is itself the newest
/// write when it lands, so only what follows it can show the difference.
pub async fn a_merged_bots_newest_write_counts_its_real_writes_and_none_of_its_ledgers<
    M: Memory,
>(
    store: &M,
) {
    let delta = EntityId("bot:delta".into());
    let lisa = EntityId("bot:lisa".into());
    for (id, name) in [(&delta, "Delta"), (&lisa, "Lisa")] {
        add(store, NewEntity::new(id.clone(), name, "user-named")).await;
    }
    capture(
        store,
        NewFact::about(delta.clone(), "keeps the shelves", date(2026, 10, 1)),
    )
    .await;
    let delta_ledger = ledger(store, &delta, "first").await;
    let lisa_ledger = ledger(store, &lisa, "first").await;
    let lisa_real = capture(
        store,
        NewFact::about(lisa.clone(), "runs the fryer", date(2026, 10, 2)),
    )
    .await;

    store
        .merge(
            &lisa,
            &delta,
            Some("one bot under two names"),
            date(2026, 10, 3),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("the bots merge");
    let at_the_merge = newest(store, &delta).await;
    assert!(at_the_merge.is_some());

    // A rewrite of either ledger, alone, moves nothing.
    for ledger in [&lisa_ledger, &delta_ledger] {
        let served = store
            .moved_address(&ledger.address())
            .await
            .expect("the address reads")
            .unwrap_or_else(|| ledger.address());
        edit(
            store,
            &served,
            FactPatch {
                fields: [(RELEASE_NOTES_TOLD.to_string(), "second".to_string())]
                    .into_iter()
                    .collect(),
                sets: [RELEASE_NOTES_TOLD.to_string()].into_iter().collect(),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            newest(store, &delta).await,
            at_the_merge,
            "a ledger rewrite moved the merged bot's newest write"
        );
    }

    // The pair: a rewrite of lisa's real write does, though its local id is the
    // one delta's ledger wears.
    let served = store
        .moved_address(&lisa_real.address())
        .await
        .expect("the address reads")
        .unwrap_or_else(|| lisa_real.address());
    edit(
        store,
        &served,
        FactPatch {
            details: Some("fries on Fridays".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        newest(store, &delta).await > at_the_merge,
        "a real write on a folded bot did not move the merged bot's newest write"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn the_fake_holds_a_merged_bots_newest_write_to_its_real_writes() {
        a_merged_bots_newest_write_counts_its_real_writes_and_none_of_its_ledgers(
            &InMemoryMemory::booted(),
        )
        .await;
    }
}
