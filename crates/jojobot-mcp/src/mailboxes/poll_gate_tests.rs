//! **A poll never waits behind the memory write gate.** A read of one's own
//! mailbox renews the caller's role claim, and a due renewal is a memory write.
//! These cases hold that write the way a long merge holds the gate, and ask
//! whether the read still answers.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::{DeclaredType, Field, Origin};
#[allow(unused_imports)]
use jojobot_domain::memory::*;
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FieldWrite, FormerHandle, Guarded, Memory, MemoryError, Merge, NewEntity, NewFact, Retraction,
};

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

#[async_trait]
impl Memory for HoldsRoleWrites {
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.inner.declare_type(declared, by_session).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: Origin,
        fields: Vec<Field>,
    ) -> Result<(), MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn update_entity(
        &self,
        id: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(id, patch).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: jiff::civil::Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.inner.restore_entity(id, session).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
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
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.inner
            .retract(address, reason, date, caller, session)
            .await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.inner.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
        self.inner.scan().await
    }

    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.inner.add_entity_with_first_claim(new, first).await
    }

    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        self.inner.record_app_event(event).await
    }

    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.inner
            .update_entity_with_claim(handle, patch, claim)
            .await
    }

    async fn numbering_of(
        &self,
        entity: &EntityId,
    ) -> Result<Option<numbered::Numbering>, MemoryError> {
        self.inner.numbering_of(entity).await
    }

    async fn numbers_held(&self, asked: &EntityId) -> Result<Vec<guard::EntityMatch>, MemoryError> {
        self.inner.numbers_held(asked).await
    }

    async fn numbered_scope(
        &self,
        collection: &EntityId,
        all: &[Entity],
    ) -> Result<Option<numbered::Numbered>, MemoryError> {
        self.inner.numbered_scope(collection, all).await
    }

    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        self.inner.backing(entity).await
    }

    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.inner.built_on(source).await
    }

    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.inner.thought_writers(thread).await
    }

    async fn keys_holding(
        &self,
        key: &str,
        test: &filter::Test,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        self.inner.keys_holding(key, test, kind).await
    }

    async fn ordered_first_k(
        &self,
        key: &str,
        holds: types::ValueType,
        direction: filter::Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> Result<holders::OrderedHead, MemoryError> {
        self.inner
            .ordered_first_k(key, holds, direction, kind, among, k)
            .await
    }

    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.referring_to(target).await
    }

    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.inner.children(parent).await
    }

    async fn entity_history(&self, entity: &EntityId) -> Result<Vec<EntityVersion>, MemoryError> {
        self.inner.entity_history(entity).await
    }

    async fn keys_kept_off_the_thing(
        &self,
        _address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        self.inner.keys_kept_off_the_thing(_address).await
    }

    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.inner.claim_histories(entity).await
    }

    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<MergeEvent>, MemoryError> {
        self.inner.merge_events(entity).await
    }

    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.inner.moved_address(address).await
    }

    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<unmapped::UnmappedClaim>, MemoryError> {
        self.inner.unmapped_claims(entity).await
    }

    async fn key_trails(&self, entity: &EntityId) -> Result<KeyTrails, MemoryError> {
        self.inner.key_trails(entity).await
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        self.inner.field_ages(entity).await
    }

    async fn echoed_defaults(
        &self,
        _entity: &EntityId,
        _fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.inner.echoed_defaults(_entity, _fields).await
    }

    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        self.inner.fields_versioned(entity).await
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        self.inner.scan_entity(entity).await
    }

    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        self.inner.has_prose(entity).await
    }

    async fn composed_prose(&self, _entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.inner.composed_prose(_entity, own).await
    }

    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.inner.write_summary().await
    }

    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<types::DeclaredType>>, MemoryError> {
        self.inner.declared_types_shared().await
    }

    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.inner.displaced_type(name).await
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
        server.role_renewals.settled().await;
        let rival = world.rival_at(3000).await;
        assert_lease_was_held(&rival, &run_name(&world.at(0), &holder));
    }
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
    server.role_renewals.settled().await;
    assert_eq!(world.entered.load(Ordering::SeqCst), 1);
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
    server.role_renewals.settled().await;
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
