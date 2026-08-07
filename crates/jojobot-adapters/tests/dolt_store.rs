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
use std::sync::atomic::{AtomicUsize, Ordering};

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
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

/// A port nothing is listening on.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free port")
        .local_addr()
        .expect("a bound address")
        .port()
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
