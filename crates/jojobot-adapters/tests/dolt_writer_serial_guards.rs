//! **Two guards that decide from a value read inside the write are held by the
//! one writer, over the real store.**
//!
//! In production every write passes through the owned writer, one at a time.
//! That is what makes "the chain is read inside the write" and "the revision is
//! counted inside the transaction" true: no other write lands between a guard's
//! read and the commit. A case that calls the store one write after another
//! proves the decision and never the order. These cases submit the writes
//! together, through the writer, and assert the outcome a serial order gives.
//! Sending one of the two writes around the writer, to the store directly,
//! lets the two transactions overlap, and the cases go red.

use std::sync::Arc;
use std::time::Duration;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{
    EntityId, FactAddress, FactPatch, Memory, MemoryError, NewEntity, NewFact, Provenance,
};
use tokio::time::timeout;

/// **How many times a race is run.** One run can pass by luck on a store that
/// overlaps its writes; a handful of fresh runs cannot all do so.
const ROUNDS: usize = 8;

async fn store_and_writer(
    name: &str,
    database: &str,
) -> (Scratch, Dolt, Arc<DoltMemory>, Folded, Writer) {
    let scratch = Scratch::new(name);
    let store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, database)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let direct = Arc::new(DoltMemory::open(pool));
    jojobot_domain::memory::kinds::seed(&*direct)
        .await
        .expect("the kinds are seeded");
    let writer = Writer::default();
    let served = Folded::new(direct.clone()).with_writer(writer.clone());
    served.rebuild().await.expect("the image fills");
    (scratch, store, direct, served, writer)
}

async fn make(memory: &impl Memory, handle: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let answer = memory
        .add_entity(NewEntity::new(id.clone(), handle, "contract-fixture"))
        .await
        .expect("add_entity ok");
    let said = format!("{answer:?}");
    answer
        .written()
        .unwrap_or_else(|| panic!("{handle} collides with a name already held: {said}"));
    id
}

fn edit(content: &str, revision: usize) -> FactPatch {
    FactPatch {
        content: Some(content.to_string()),
        provenance: Some(Provenance::Inference),
        expected_revision: Some(revision),
        ..Default::default()
    }
}

/// Two edits of one claim, both written against revision 1: exactly one lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stale_revision_is_counted_inside_the_write() {
    timeout(Duration::from_secs(300), stale_revision())
        .await
        .expect("the case finished: a hang is a failure, not a wait");
}

async fn stale_revision() {
    let (_scratch, mut store, direct, served, writer) =
        store_and_writer("stale-revision-writer", "stale_revision_writer").await;
    let thing = make(&served, "work:lambda").await;
    let caller = make(&served, "bot:sigma").await;

    for round in 0..ROUNDS {
        let captured = served
            .capture(NewFact::about(
                thing.clone(),
                format!("works at the old place {round}"),
                date(2026, 10, 1),
            ))
            .await
            .expect("the claim lands")
            .written()
            .expect("nothing collides with it");
        let address = FactAddress::new(thing.clone(), captured.id.clone());
        let first = edit("the first edit", 1);
        let second = edit("the second edit", 1);

        let before = writer.submitted_jobs();
        let a = served.update_fact(&address, first, &caller);
        let b = served.update_fact(&address, second, &caller);
        let (ra, rb) = tokio::join!(a, b);
        let jobs = writer.submitted_jobs() - before;

        let landed = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            landed, 1,
            "round {round}: exactly one edit of revision 1 lands: {ra:?} / {rb:?}"
        );
        let loser = [ra, rb]
            .into_iter()
            .find_map(Result::err)
            .expect("one edit lost");
        assert!(
            matches!(
                loser,
                MemoryError::StaleRevision {
                    expected: 1,
                    current: 2,
                    ..
                }
            ),
            "round {round}: the loser is told its revision is stale, not to retry: {loser:?}"
        );
        // Read last, so a write sent around the writer fails on the outcome first.
        assert_eq!(jobs, 2, "both edits were accepted by the writer's queue");
        let held = direct.recall(&thing).await.expect("recall ok");
        let claim = held
            .iter()
            .find(|fact| fact.id == captured.id)
            .expect("the claim is held");
        assert!(
            claim.content == "the first edit" || claim.content == "the second edit",
            "round {round}: the claim holds the edit that landed: {}",
            claim.content
        );
    }

    store.stop().await;
}

/// A chain change and a guarded write under it, submitted together: the guarded
/// write is judged against the chain the change committed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chain_change_during_the_write_is_seen_by_the_guard() {
    timeout(Duration::from_secs(300), chain_change())
        .await
        .expect("the case finished: a hang is a failure, not a wait");
}

async fn chain_change() {
    let (_scratch, mut store, direct, served, writer) =
        store_and_writer("chain-change-writer", "chain_change_writer").await;
    let otto = make(&served, "bot:otto").await;
    let sigma = make(&served, "bot:sigma").await;
    // Each report is a bot under sigma, sigma under otto.
    let report = |handle: &'static str| {
        let served = &served;
        let sigma = sigma.clone();
        async move {
            let bot = make(served, handle).await;
            let claim = served
                .capture(memory::state(NewFact {
                    fields: [("reports_to".to_string(), sigma.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(bot.clone(), "reports up", date(2026, 8, 1))
                }))
                .await
                .expect("capture ok")
                .written()
                .expect("the report lands");
            (
                bot,
                FactAddress::new(claim.subject.clone(), claim.id.clone()),
            )
        }
    };
    served
        .capture(memory::state(NewFact {
            fields: [("reports_to".to_string(), otto.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(sigma.clone(), "reports up", date(2026, 8, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("sigma's report lands");

    // **With no change under it, sigma, who is above the report, may take the
    // report back**, so the refusal below is the chain and not the write.
    let (_, kept) = report("bot:delta").await;
    served
        .retract(&kept, Some("moved on"), date(2026, 8, 2), &sigma, None)
        .await
        .expect("sigma is above the report, so the retraction lands");

    // **With otto's change ahead of it in the writer's queue, the same
    // retraction is judged against the chain otto committed**, which no longer
    // runs through sigma.
    let reports = [
        "bot:homer",
        "bot:marge",
        "bot:milhouse",
        "bot:smithers",
        "bot:gamma",
        "bot:epsilon",
    ];
    for (round, handle) in reports.into_iter().enumerate() {
        let (bot, address) = report(handle).await;
        let change = FactPatch {
            fields: [("reports_to".to_string(), otto.to_string())]
                .into_iter()
                .collect(),
            provenance: Some(Provenance::Inference),
            ..Default::default()
        };
        let change = memory::state_patch(change);

        let before = writer.submitted_jobs();
        let moved = served.update_fact(&address, change, &otto);
        let retracted = served.retract(&address, Some("moved on"), date(2026, 8, 2), &sigma, None);
        let (moved, retracted) = tokio::join!(moved, retracted);
        let jobs = writer.submitted_jobs() - before;
        assert!(
            moved.is_ok(),
            "round {round}: otto's change lands: {moved:?}"
        );
        assert!(
            matches!(retracted, Err(MemoryError::KeyNotYours { .. })),
            "round {round}: the retraction is judged against the new chain: {retracted:?}"
        );
        let fields = direct.fields(&bot).await.expect("the fields read");
        assert_eq!(
            fields.get("reports_to").map(String::as_str),
            Some(otto.as_str()),
            "round {round}: the report stands where otto put it"
        );
        // Read last, so a write sent around the writer fails on the outcome first.
        assert_eq!(jobs, 2, "both writes were accepted by the writer's queue");
    }

    store.stop().await;
}
