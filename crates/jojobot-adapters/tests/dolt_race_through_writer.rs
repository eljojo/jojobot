//! **Two captures racing for a thread's last slot go through the owned writer,
//! over the real store, and exactly one lands.**
//!
//! The bare-store race in `dolt_store.rs` proves the store's commit conflict.
//! Production never reaches that conflict: every write passes through the
//! writer, which runs one write at a time. This case sends the race through
//! that mechanism, so it goes red when the writer stops serialising.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, FactStatus, Memory, MemoryError, NewEntity, NewFact};
use tokio::sync::Barrier;
use tokio::time::timeout;

/// **A store that holds each capture at its door until a second one arrives**,
/// and records how many were inside together. The two captures meet at a
/// barrier, so a writer that lets them in together is forced to overlap them,
/// not left to the scheduler. A writer that keeps them apart leaves the first
/// capture alone at the barrier, which gives up after a bound and goes on; the
/// bound costs time only when the writer works. The count is the evidence that
/// the race was run and not merely started.
struct Rendezvous {
    inner: Arc<DoltMemory>,
    armed: AtomicBool,
    meet: Barrier,
    inside: AtomicUsize,
    peak: AtomicUsize,
}

/// How long a capture waits for a second one when the writer keeps them apart.
const WAIT_FOR_THE_OTHER: Duration = Duration::from_secs(2);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for Rendezvous {
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inside.fetch_add(1, Ordering::SeqCst);
        if self.armed.load(Ordering::SeqCst) {
            let _ = timeout(WAIT_FOR_THE_OTHER, self.meet.wait()).await;
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
    timeout(Duration::from_secs(180), the_race())
        .await
        .expect("the case finished: a hang is a failure, not a wait");
}

async fn the_race() {
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
        armed: AtomicBool::new(false),
        meet: Barrier::new(2),
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

    door.armed.store(true, Ordering::SeqCst);
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
    let live = door
        .inner
        .recall(&thread)
        .await
        .expect("recall ok")
        .into_iter()
        .filter(|f| f.status == FactStatus::Active)
        .count();
    assert_eq!(live, 2, "the thread never holds a third record");

    store.stop().await;
}
