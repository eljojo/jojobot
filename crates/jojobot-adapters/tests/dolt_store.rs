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
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_domain::mailbox::testing::contract as mailboxes;
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::EntityId;
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
