//! **Two captures racing for a thread's last slot go through the owned writer,
//! over the real store, and exactly one lands.**
//!
//! The bare-store race in `dolt_store.rs` proves the store's commit conflict.
//! Production never reaches that conflict: every write passes through the
//! writer, which runs one write at a time. This case sends the race through
//! that mechanism, so it goes red when the writer stops serialising.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, FactStatus, Memory, MemoryError, NewEntity, NewFact};

/// **A store that holds each capture at its door until a second one arrives**,
/// or until it has yielded a fixed number of times, and records how many were
/// inside together. Two captures that can overlap do overlap here, so a writer
/// that lets them in together is seen doing it; the count is the evidence that
/// the race was run and not merely started.
struct Rendezvous {
    inner: Arc<DoltMemory>,
    inside: AtomicUsize,
    peak: AtomicUsize,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for Rendezvous {
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        for _ in 0..100_000 {
            if self.inside.load(Ordering::SeqCst) >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        self.peak
            .fetch_max(self.inside.load(Ordering::SeqCst), Ordering::SeqCst);
        let answer = self.inner.capture(fact).await;
        self.inside.fetch_sub(1, Ordering::SeqCst);
        answer
    }
}

/// **The loser is told the thread is full, never to retry.** A retry answer
/// (`Conflict`) means both captures read the count together and the store
/// settled the race at commit, which is the path the writer exists to close.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_captures_racing_for_the_last_slot_through_the_writer_land_one_and_refuse_one() {
    let scratch = Scratch::new("record-capacity-writer-race");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "record_capacity_writer_race")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let writer = Writer::default();
    let door = Arc::new(Rendezvous {
        inner: Arc::new(DoltMemory::open(pool)),
        inside: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    });
    let served = Folded::new(door.clone()).with_writer(writer.clone());
    served.rebuild().await.expect("the image fills");

    let thread = EntityId("thread:record-capacity-race".into());
    served
        .add_entity(NewEntity::new(thread.clone(), "Race", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides");
    served
        .capture(memory::state(NewFact {
            fields: [("record_capacity".to_string(), "2".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(thread.clone(), "the thread holds two", date(2026, 10, 1))
        }))
        .await
        .expect("the first record lands");

    let before = writer.submitted_jobs();
    let a = served.capture(NewFact::about(thread.clone(), "racer A", date(2026, 10, 2)));
    let b = served.capture(NewFact::about(thread.clone(), "racer B", date(2026, 10, 2)));
    let (ra, rb) = tokio::join!(a, b);

    assert_eq!(
        writer.submitted_jobs() - before,
        2,
        "both captures were accepted by the writer's queue"
    );
    assert_eq!(
        door.peak.load(Ordering::SeqCst),
        1,
        "the writer let one capture in at a time"
    );
    let landed = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        landed, 1,
        "exactly one racer takes the last slot: {ra:?} / {rb:?}"
    );
    let loser = [ra, rb]
        .into_iter()
        .find_map(Result::err)
        .expect("one racer lost");
    assert!(
        matches!(loser, MemoryError::OverCapacity { .. }),
        "the loser is told the thread is full, not to retry: {loser:?}"
    );
    let live = served
        .recall(&thread)
        .await
        .expect("recall ok")
        .into_iter()
        .filter(|f| f.status == FactStatus::Active)
        .count();
    assert_eq!(live, 2, "the thread never holds a third record");

    store.stop().await;
}
