//! **A poll never waits behind the memory write gate.** A read of one's own
//! mailbox renews the caller's role claim, and a due renewal is a memory write.
//! These cases hold that write the way a long merge holds the gate, and ask
//! whether the read still answers.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;
#[allow(unused_imports)]
use jojobot_domain::memory::*;

/// **The gate, as a case holds it.** Open until a case holds it; while it is
/// held a renewal waits, as a write waits behind a merge. Releasing lets every
/// waiting renewal through.
#[derive(Default)]
pub(crate) struct Gate {
    held: std::sync::atomic::AtomicBool,
    changed: tokio::sync::Notify,
}

impl Gate {
    pub(crate) fn hold(&self) {
        self.held.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn release(&self) {
        self.held.store(false, std::sync::atomic::Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    async fn passes(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if !self.held.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }
}

/// **A store whose role writes wait for a [`Gate`]**, and that counts how many
/// reached it. Everything else goes straight to the fake.
pub(crate) struct HoldsRoleWrites {
    pub(crate) inner: Arc<InMemoryMemory>,
    pub(crate) held: Arc<Gate>,
    pub(crate) entered: Arc<std::sync::atomic::AtomicUsize>,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for HoldsRoleWrites {
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        // A renewal of a role claim is the one write a poll makes. It counts
        // here, and waits while a case holds the gate.
        if patch
            .role_move
            .as_ref()
            .is_some_and(|m| m.kind == jojobot_domain::session::RoleMoveKind::Renew)
        {
            self.entered
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.held.passes().await;
        }
        self.inner.update_fact(address, patch, caller).await
    }
}

// --- the cases ---------------------------------------------------------------

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Jojobot;
use crate::harness::{assert_lease_was_held, json_of, make_bot, run_name, sid_of};
use crate::mailboxes::read_mailbox::ReadMailboxArgs;
use crate::memory::testing::SpySearch;
use crate::orientation::OrientArgs;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::session::testing::InMemorySessions;
use rmcp::handler::server::wrapper::Parameters;

/// One world: a store whose renewals wait for a gate, and a server for each
/// moment a case asks about.
struct World {
    memory: Arc<InMemoryMemory>,
    gate: Arc<Gate>,
    entered: Arc<AtomicUsize>,
    mailboxes: Arc<InMemoryMailboxes>,
    sessions: Arc<InMemorySessions>,
    teachings: Arc<jojobot_domain::teaching::testing::InMemoryTeachings>,
    registry: Arc<crate::sid::SessionRegistry>,
}

impl World {
    fn new() -> Self {
        Self {
            memory: Arc::new(InMemoryMemory::booted()),
            gate: Arc::default(),
            entered: Arc::default(),
            mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
            sessions: Arc::new(InMemorySessions::new()),
            teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            registry: Arc::new(crate::sid::SessionRegistry::new()),
        }
    }

    /// The server `elapsed_secs` after the claim was taken.
    fn at(&self, elapsed_secs: i64) -> Jojobot {
        Jojobot::new(
            Arc::new(HoldsRoleWrites {
                inner: self.memory.clone(),
                held: self.gate.clone(),
                entered: self.entered.clone(),
            }),
            Arc::new(SpySearch::default()),
            self.mailboxes.clone(),
            self.sessions.clone(),
            self.teachings.clone(),
            self.registry.clone(),
        )
        .on_clock(jojobot_domain::clock::Clock::Stated {
            day: "2026-06-01".parse().expect("a date"),
            began: jiff::Timestamp::now() - jiff::SignedDuration::from_secs(elapsed_secs),
        })
    }

    /// A session of `gamma` that claimed the role `dev-dispatch` at the start.
    async fn claimed(&self) -> String {
        let start = self.at(0);
        make_bot(&start, "gamma").await;
        let args: OrientArgs = serde_json::from_value(serde_json::json!({
            "bot": "gamma", "claim": "dev-dispatch", "resume": "new"
        }))
        .expect("boot arguments");
        let claimed = json_of(&start.start_here(Parameters(args)).await.expect("boot"));
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
        sid_of(&claimed).expect("a handle")
    }

    /// A second session claiming the same role at `secs`: it says whether the
    /// first run's lease was still fresh then.
    async fn rival_at(&self, secs: i64) -> serde_json::Value {
        let args: OrientArgs = serde_json::from_value(serde_json::json!({
            "bot": "gamma", "claim": "dev-dispatch", "resume": "new"
        }))
        .expect("boot arguments");
        json_of(
            &self
                .at(secs)
                .start_here(Parameters(args))
                .await
                .expect("boot"),
        )
    }
}

fn poll(sid: &str, counts_only: bool) -> ReadMailboxArgs {
    ReadMailboxArgs {
        counts_only: Some(counts_only),
        new_only: None,
        sid: Some(sid.to_string()),
    }
}

/// A poll that must answer without the gate: it fails rather than hangs.
async fn polled(server: &Jojobot, sid: &str, counts_only: bool) -> serde_json::Value {
    let answered = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.read_mailbox(Parameters(poll(sid, counts_only))),
    )
    .await
    .unwrap_or_else(|_| panic!("the poll (counts_only {counts_only}) waited for the gate"))
    .expect("the poll answers");
    json_of(&answered)
}

/// Wait until `count` renewals have reached the store, yielding to the task that
/// makes them.
async fn until_entered(world: &World, count: usize) {
    for _ in 0..1000 {
        if world.entered.load(Ordering::SeqCst) >= count {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!(
        "{count} renewals never reached the store; {} did",
        world.entered.load(Ordering::SeqCst)
    );
}

/// **The red case.** The gate is held, as a merge holds it, and a poll is made
/// when its renewal is due. The poll answers, in both of its shapes, while the
/// renewal waits. The lease still renews once the gate lets go: a rival at 3000
/// seconds, past the 2700 of the lease from the claim, finds the claim held.
#[tokio::test]
async fn a_poll_answers_while_the_gate_is_held_and_the_lease_renews_after() {
    for counts_only in [true, false] {
        let world = World::new();
        let holder = world.claimed().await;
        world.gate.hold();
        let server = world.at(2000);

        let body = polled(&server, &holder, counts_only).await;
        assert_ne!(body["status"], "blocked", "{body}");

        until_entered(&world, 1).await;
        world.gate.release();
        server.registry.role_renewals().settled().await;
        let rival = world.rival_at(3000).await;
        assert_lease_was_held(&rival, &run_name(&world.at(0), &holder));
    }
}

/// Wait for the renewals running on `server` to end, and fail rather than hang
/// when one never does: a renewal that leaves its claimant behind blocks every
/// later one.
async fn settled_soon(server: &Jojobot) {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.registry.role_renewals().settled(),
    )
    .await
    .expect("the renewal ended, and its claimant was taken off");
}

/// **A session that only polls renews again after each renewal ends.** One poll
/// renews at 2000 seconds and its renewal finishes; a second poll at 4000
/// renews again. The proof is the lease: a rival at 6000 finds it held, which
/// only the second renewal (to 4000, so held to 6700) can have done, since the
/// first (held to 4700) had run out by then.
#[tokio::test]
async fn a_poll_renews_again_after_the_renewal_before_it_has_finished() {
    let world = World::new();
    let holder = world.claimed().await;

    let first = world.at(2000);
    polled(&first, &holder, true).await;
    until_entered(&world, 1).await;
    settled_soon(&first).await;

    // The renewals running belong to the process (the one registry the world
    // builds every handler from), not to a clock: the second poll meets the
    // record the first left.
    let second = world.at(4000);
    polled(&second, &holder, true).await;
    until_entered(&world, 2).await;
    settled_soon(&second).await;
    assert_eq!(world.entered.load(Ordering::SeqCst), 2);

    let rival = world.rival_at(6000).await;
    assert_lease_was_held(&rival, &run_name(&world.at(0), &holder));
}

/// **Two polls while the renewal waits start one renewal**, not two racing.
#[tokio::test]
async fn two_polls_in_a_row_start_one_renewal() {
    let world = World::new();
    let holder = world.claimed().await;
    world.gate.hold();
    let server = world.at(2000);
    for _ in 0..2 {
        polled(&server, &holder, true).await;
    }
    until_entered(&world, 1).await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    world.gate.release();
    server.registry.role_renewals().settled().await;
    assert_eq!(world.entered.load(Ordering::SeqCst), 1);
}

/// **Two handlers built from one registry, as production builds one for each
/// connection, start one renewal between them** while the first is running.
#[tokio::test]
async fn polls_from_two_handlers_start_one_renewal() {
    let world = World::new();
    let holder = world.claimed().await;
    world.gate.hold();
    let first = world.at(2000);
    let second = world.at(2000);
    polled(&first, &holder, true).await;
    polled(&second, &holder, true).await;
    until_entered(&world, 1).await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    world.gate.release();
    first.registry.role_renewals().settled().await;
    assert_eq!(world.entered.load(Ordering::SeqCst), 1);
}

/// **A renewal that hangs is cut off**, so the poll after it starts a new one.
#[tokio::test]
async fn a_hung_renewal_is_cut_off_at_its_timeout() {
    let world = World::new();
    let holder = world.claimed().await;
    world.gate.hold();
    let server = world.at(2000);
    polled(&server, &holder, true).await;
    until_entered(&world, 1).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.registry.role_renewals().settled(),
    )
    .await
    .expect("the hung renewal was never cut off");
    polled(&server, &holder, true).await;
    until_entered(&world, 2).await;
    world.gate.release();
    server.registry.role_renewals().settled().await;
}

/// **A renewal the poll skipped or delayed is said at debug, once each**, and is
/// no part of the answer.
#[tokio::test]
async fn a_delayed_or_skipped_renewal_is_logged_at_debug_and_not_answered() {
    #[derive(Clone, Default)]
    struct Lines(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Lines {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("the log lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let world = World::new();
    let holder = world.claimed().await;
    world.gate.hold();
    let server = world.at(2000);
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let mut answers = Vec::new();
    {
        let _for_this_thread = tracing::subscriber::set_default(subscriber);
        for _ in 0..2 {
            answers.push(polled(&server, &holder, true).await.to_string());
        }
    }
    world.gate.release();
    server.registry.role_renewals().settled().await;
    let log = String::from_utf8(lines.0.lock().expect("the log lock").clone()).expect("text");
    assert_eq!(
        log.matches("role_renewal=\"after_the_answer\"").count(),
        1,
        "{log}"
    );
    assert_eq!(log.matches("role_renewal=\"skipped\"").count(), 1, "{log}");
    for answer in answers {
        assert!(
            !answer.contains("renew"),
            "the answer mentions it: {answer}"
        );
    }
}
