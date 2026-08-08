//! **The shared contract, against the real store.**
//!
//! Not `#[ignore]` and not credential-gated, which is the whole difference from
//! the Outline suite beside it: this one needs a temporary directory and the
//! binary that is already in the toolchain, so it runs in an ordinary
//! `cargo test` and nobody has to remember it.
//!
//! The contract is the specification. Nothing here restates what a session
//! does — it points the existing cases at a third implementation and lets them
//! say whether it behaves.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::carry;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::contract as mailboxes;
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::Memory as _;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::session::testing::contract as sessions;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-contract-{}-{what}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A port no other caller in this process will be given.
///
/// **A cursor, not just a bind.** Asking the OS for `:0` and letting the
/// listener go hands two concurrent callers the same number often enough to
/// matter — measured on this machine at 4 collisions in 400 with two callers,
/// and 339 in 3200 with sixteen. Two servers then get one port: the loser's
/// child cannot bind and dies, and the winner answers its client, so a whole
/// test runs against another test's database.
///
/// Taking a distinct slot first means no two callers here can be offered the
/// same candidate, whatever the kernel would have said. The bind that follows
/// only checks the candidate is free; the window it leaves is somebody outside
/// this process, and `Dolt::start` refuses a port it cannot take.
fn free_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    for _ in 0..40_000 {
        let slot = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let port = 20_000 + slot % 40_000;
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("no free port in the range this suite uses")
}

/// **The contract's cases, each against a store of its own.**
///
/// `fresh` is synchronous and opening a store is not, so the stores are opened
/// up front and handed out one per case. The count is deliberately larger than
/// the contract, and running out is a loud failure rather than a reused store:
/// two cases sharing one store would let one case's rows satisfy another's
/// assertions, which is the failure mode this isolation exists to prevent.
#[tokio::test]
async fn dolt_satisfies_the_session_contract() {
    let scratch = Scratch::new("sessions");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    const ROOM: usize = 32;
    let mut prepared = Vec::with_capacity(ROOM);
    for n in 0..ROOM {
        let pool = store
            .database(&format!("case{n}"))
            .await
            .expect("a database of this case's own");
        migrate::run(&pool).await.expect("the schema");
        prepared.push(DoltSessions::open(pool));
    }

    let handed = AtomicUsize::new(0);
    let fresh = || {
        let n = handed.fetch_add(1, Ordering::SeqCst);
        prepared.get(n).cloned().unwrap_or_else(|| {
            panic!(
                "the contract has more cases than this suite prepared stores for \
                 ({ROOM}). Raise ROOM — never let two cases share one store, or one \
                 case's rows start satisfying another's assertions."
            )
        })
    };

    sessions::run_all(fresh).await;

    store.stop().await;
}

/// **The memory contract, against the real store.**
///
/// One store for every case, which is what this contract is written for: its
/// assertions are subset-based — what was captured comes back, never an exact
/// total — because it also runs against a shared, pre-populated real
/// collection. Handing each case its own database here would prove less than
/// the suite is designed to prove, not more.
#[tokio::test]
async fn dolt_satisfies_the_memory_contract() {
    let scratch = Scratch::new("memory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");

    memory::run_all(&DoltMemory::open(pool)).await;

    store.stop().await;
}

/// **…and the same contract including retrieval**, with the search projection
/// over this store.
///
/// It is the one thing `run_all` cannot reach: `scan` feeds the projection and
/// no verb a caller calls returns it, so a scan that came back empty would
/// leave every case above green and every search answer wrong. The projection
/// itself is unchanged — it sits above the port and does not care which store
/// answers, which is the claim this case actually tests.
#[tokio::test]
async fn the_indexed_dolt_store_satisfies_the_whole_contract() {
    let scratch = Scratch::new("memory-indexed");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memoryindexed")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");

    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(DoltMemory::open(pool))).expect("the search index opens"),
    );
    memory::run_all_searchable(
        indexed.as_ref(),
        &Retrieval::new(indexed.index(), vec![indexed.clone()]),
    )
    .await;

    store.stop().await;
}

/// **The carry, over a source that disagrees with itself.**
///
/// The one thing only the carry can do is re-derive a fact's owner from the
/// page its row sat on, because the old store records it nowhere else. So the
/// source here hands back a document whose fact carries a DIFFERENT subject
/// cell from the page holding it — the disagreement a real board can carry —
/// and the case asserts the row lands under the page, not under the cell.
///
/// A fake source rather than a real Outline collection, deliberately: the
/// disagreement is what is under test, no verb can produce it, and a case that
/// needed credentials would not run in an ordinary `cargo test`.
#[tokio::test]
async fn the_carry_files_a_fact_under_the_page_it_sat_on() {
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Boot, Entity, EntityKind, Fact, FactId, NewEntity, Provenance};
    use jojobot_domain::memory::{FactStatus, Standing, search::DocScan};

    /// A source whose scan carries the disagreement. Only `scan` is this
    /// carry's read, so only `scan` is answered.
    struct Placed(InMemoryMemory);

    #[async_trait::async_trait]
    impl jojobot_domain::memory::Memory for Placed {
        async fn scan(&self) -> Result<Vec<DocScan>, jojobot_domain::memory::MemoryError> {
            let alpha = EntityId::person("carried-alpha");
            let beta = EntityId::person("carried-beta");
            let entity = |id: &EntityId| Entity {
                kind: EntityKind::Person,
                id: id.clone(),
                name: id.slug().to_string(),
                aliases: Vec::new(),
                source: "carry-fixture".into(),
                crm: None,
                parent: None,
                boot: Boot::OnDemand,
            };
            Ok(vec![
                DocScan {
                    doc_id: alpha.to_string(),
                    title: "alpha".into(),
                    prose: "what somebody wrote on this page".into(),
                    entity: Some(entity(&alpha)),
                    facts: vec![Fact {
                        id: FactId("f1".into()),
                        // The page is alpha's; the cell says beta. A carry that
                        // read the cell would file this under beta for good.
                        home: alpha.clone(),
                        subject: beta.clone(),
                        content: "sat on alpha's page and named beta".into(),
                        details: None,
                        provenance: Provenance::Testimony,
                        standing: Standing::Settled,
                        status: FactStatus::Active,
                        date: jiff::civil::date(2026, 3, 8),
                        edge: None,
                        event: None,
                        derived_from: None,
                    }],
                },
                DocScan {
                    doc_id: beta.to_string(),
                    title: "beta".into(),
                    prose: String::new(),
                    entity: Some(entity(&beta)),
                    facts: Vec::new(),
                },
            ])
        }
        async fn add_entity(
            &self,
            new: NewEntity,
        ) -> Result<jojobot_domain::memory::Guarded<Entity>, jojobot_domain::memory::MemoryError>
        {
            self.0.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, jojobot_domain::memory::MemoryError> {
            self.0.list_entities(kind).await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: jojobot_domain::memory::EntityPatch,
        ) -> Result<jojobot_domain::memory::Guarded<Entity>, jojobot_domain::memory::MemoryError>
        {
            self.0.update_entity(handle, patch).await
        }
        async fn capture(
            &self,
            fact: jojobot_domain::memory::NewFact,
        ) -> Result<jojobot_domain::memory::Guarded<Fact>, jojobot_domain::memory::MemoryError>
        {
            self.0.capture(fact).await
        }
        async fn recall(
            &self,
            subject: &EntityId,
        ) -> Result<Vec<Fact>, jojobot_domain::memory::MemoryError> {
            self.0.recall(subject).await
        }
        async fn update_fact(
            &self,
            address: &jojobot_domain::memory::FactAddress,
            patch: jojobot_domain::memory::FactPatch,
        ) -> Result<jojobot_domain::memory::Guarded<Fact>, jojobot_domain::memory::MemoryError>
        {
            self.0.update_fact(address, patch).await
        }
        async fn retract(
            &self,
            address: &jojobot_domain::memory::FactAddress,
            reason: Option<&str>,
            date: jiff::civil::Date,
        ) -> Result<jojobot_domain::memory::Retraction, jojobot_domain::memory::MemoryError>
        {
            self.0.retract(address, reason, date).await
        }
        async fn set_prose(
            &self,
            entity: &EntityId,
            prose: &str,
        ) -> Result<String, jojobot_domain::memory::MemoryError> {
            self.0.set_prose(entity, prose).await
        }
    }

    let scratch = Scratch::new("carry");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("carried")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());

    let carried = carry::carry_over(&Placed(InMemoryMemory::new()), &memory, &pool).await;
    let carry::Carried::Carried(report) = carried else {
        panic!("the carry completes over a readable source: {carried:?}");
    };
    assert_eq!(report.entities, 2);
    assert_eq!(report.facts, 1);
    assert_eq!(report.prose, 1);
    assert_eq!(
        report.verified, 3,
        "every record was compared, not merely written: {report:?}"
    );

    // **The owner came from the page.** Read through the port, which is the
    // only reader whose answer a caller will ever see.
    let alpha = EntityId::person("carried-alpha");
    let beta = EntityId::person("carried-beta");
    let on_alpha = memory.recall(&alpha).await.expect("recall ok");
    assert_eq!(
        on_alpha
            .iter()
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>(),
        vec!["sat on alpha's page and named beta"],
        "the row is filed under the page it sat on"
    );
    assert!(
        memory.recall(&beta).await.expect("recall ok").is_empty(),
        "…and not under the subject cell it happened to carry"
    );

    // **An old row declared no standing, so the column says nobody did.** Read
    // as an operator would rather than through the adapter that wrote it.
    let standing: Option<String> =
        sqlx::query_scalar("SELECT standing FROM fact WHERE entity = ? AND id = 'f1'")
            .bind(alpha.as_str())
            .fetch_one(&pool)
            .await
            .expect("the row is readable");
    assert_eq!(
        standing, None,
        "an old row carries no declared standing, and NULL is what says so"
    );

    // **The prose came with its entity**, onto the column.
    let prose: String = sqlx::query_scalar("SELECT prose FROM entity WHERE id = ?")
        .bind(alpha.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row is readable");
    assert_eq!(prose, "what somebody wrote on this page");

    // **The second boot carries nothing and says so**, which is the steady
    // state: a verified record is the whole of what it reads.
    let again = carry::carry_over(&Placed(InMemoryMemory::new()), &memory, &pool).await;
    assert!(
        matches!(again, carry::Carried::AlreadyCarried),
        "a verified record means an earlier boot already did this: {again:?}"
    );

    store.stop().await;
}

/// **An owner index that answers for the contract's roster and nothing else.**
///
/// Strict on purpose. A resolver that says yes to everything makes
/// `Guarded::UnknownOwner` unreachable, so the case that proves a box cannot be
/// opened for a stranger would pass over a store that never checks — a green
/// bar wearing a costume. This one holds exactly the handles the suite's
/// precondition names.
struct RosterOnly;

#[async_trait::async_trait]
impl OwnerIndex for RosterOnly {
    async fn look_up(&self, owner: &EntityId) -> Result<OwnerLookup, MailboxError> {
        if mailboxes::OWNERS.contains(&owner.as_str()) {
            return Ok(OwnerLookup::Known);
        }
        // The near misses a real index would find, from the roster it does
        // hold — so a typo comes back with the handle it probably meant rather
        // than an empty list that says nothing.
        let index: Vec<jojobot_domain::memory::Entity> = mailboxes::OWNERS
            .iter()
            .map(|handle| {
                let id = EntityId((*handle).to_string());
                jojobot_domain::memory::Entity {
                    kind: id.kind().expect("the roster's handles are well-formed"),
                    name: id.slug().to_string(),
                    id,
                    aliases: Vec::new(),
                    source: "contract-fixture".into(),
                    crm: None,
                    parent: None,
                    boot: Default::default(),
                }
            })
            .collect();
        Ok(OwnerLookup::Unknown(jojobot_domain::memory::guard::screen(
            owner,
            &[],
            &index,
        )))
    }
}

/// The mailbox contract's cases, each against a store of its own.
#[tokio::test]
async fn dolt_satisfies_the_mailbox_contract() {
    let scratch = Scratch::new("mailboxes");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    const ROOM: usize = 48;
    let mut prepared = Vec::with_capacity(ROOM);
    for n in 0..ROOM {
        let pool = store
            .database(&format!("mail{n}"))
            .await
            .expect("a database of this case's own");
        migrate::run(&pool).await.expect("the schema");
        prepared.push(DoltMailboxes::open(pool, Arc::new(RosterOnly)));
    }

    let handed = AtomicUsize::new(0);
    mailboxes::run_all(|| {
        let n = handed.fetch_add(1, Ordering::SeqCst);
        let store = prepared.get(n).cloned().unwrap_or_else(|| {
            panic!(
                "the contract has more cases than this suite prepared stores for \
                 ({ROOM}). Raise ROOM — never let two cases share one store, or one \
                 case's rows start satisfying another's assertions."
            )
        });
        async move { store }
    })
    .await;

    store.stop().await;
}
