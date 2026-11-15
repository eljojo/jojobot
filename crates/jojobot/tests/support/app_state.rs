//! **The server state a case serves from when it needs no auth, no ui and no request log**:
//! every port is its in-memory twin, and the memory and the search are the two ports of one
//! `IndexedMemory`, wired the way `main` wires them.

// A binary that includes this file reaches for one function of it.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::AppState;
use jojobot_adapters::search::{IndexedMemory, Retrieval};

pub fn app_state(addr: SocketAddr, indexed: &Arc<IndexedMemory>) -> AppState {
    AppState {
        apps: Default::default(),
        metrics: None,
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.memory(),
        search: Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()])),
        mailboxes: Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ),
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        requests: None,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
        clock: jojobot_domain::clock::Clock::default(),
        quiet_after: None,
    }
}
