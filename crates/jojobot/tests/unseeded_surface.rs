//! **What a caller meets at the served surface when nothing seeded the kinds.**
//!
//! The set of kinds is data the boot writes and loads. A process that never
//! took that step cannot read a handle, and `main` treats that as survivable:
//! it logs and serves anyway, because refusing to start over an unreachable
//! store is worse than serving refusals somebody can read.
//!
//! **So the refusals have to be readable.** This drives `add_entity` — the
//! most-used write on the surface — against a server whose kinds were never
//! loaded, and holds the answer to that: it says the set was never loaded, and
//! it does not recite the kinds the software happens to ship. A caller told
//! *kind must be one of person, …, got 'person'* has been handed a
//! contradiction and no way forward.
//!
//! **A test binary of its own, because the set is process-wide.** Any case
//! running beside this one would load it, and this one would then be asserting
//! about a state it is not in.

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::memory::testing::InMemoryMemory;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn an_unseeded_surface_says_so_and_recites_no_kinds() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    // **Booted on purpose, and emptied again below.** Seeding the identity is a
    // write that parses a handle, so it needs the set — the state this case is
    // about is a process that LOST the set, not one that never had it while
    // its store was being filled.
    let store = Arc::new(InMemoryMemory::booted());
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    let state = AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search: Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()])),
        mailboxes: Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ),
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
    };
    // **A store with its records, and a process that never loaded the set.**
    // That is the state `main` tolerates and logs: the store was unreachable
    // when the boot reached for the kinds, and it serves anyway rather than
    // refusing to start. So the identity is seeded here — the store is fine —
    // and the set is emptied afterwards, which is the half that failed.
    jojobot_mcp::seed::ensure_default_identity(&state.memory, &state.mailboxes).await;
    jojobot_domain::memory::kinds::load::<[&str; 0], &str>([]);

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
            .unwrap();
    });

    let said = call_a_verb_naming_a_kind(addr).await;
    assert!(
        said.contains("never loaded"),
        "the surface names the failure a caller can act on — nothing seeded this process: {said}",
    );
    // **Every shipped kind except the one the caller sent.** `person` is in the
    // answer because the caller wrote it, and a check that counted that would
    // be measuring the call rather than the sentence.
    for shipped in jojobot_domain::memory::kinds::SHIPPED
        .into_iter()
        .filter(|kind| *kind != "person")
    {
        assert!(
            !said
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == shipped),
            "the refusal recites '{shipped}' as though the kinds were a closed list: {said}",
        );
    }

    ct.cancel();
    jojobot_domain::memory::kinds::load_shipped();
}

/// One tools/call, answered as text.
async fn ask(http: &reqwest::Client, url: &str, session: &Option<String>, body: &str) -> String {
    let mut asking = http
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(body.to_string());
    if let Some(session) = session {
        asking = asking.header("mcp-session-id", session.clone());
    }
    let answered = asking
        .send()
        .await
        .expect("the verb answers")
        .text()
        .await
        .expect("a body");
    answered
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find(|json| serde_json::from_str::<serde_json::Value>(json).is_ok())
        .unwrap_or_else(|| panic!("one JSON-RPC message: {answered}"))
        .to_string()
}

/// The text a caller gets back from a verb that names a KIND, on a process
/// whose store is fine and whose kind set was never loaded.
///
/// **The door rather than a write**, because it is the first call every session
/// makes and it needs no session of its own: a write is refused for having no
/// `sid` before it ever reaches a kind, so the door is where an unseeded
/// process actually meets a caller.
async fn call_a_verb_naming_a_kind(addr: SocketAddr) -> String {
    let http = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");
    let opened = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"unseeded","version":"0.0.1"}}}"#,
        )
        .send()
        .await
        .expect("the door answers");
    let session = opened
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string());

    // The door first, for the `sid` every other verb carries. It answers from
    // records the store already holds, so an unloaded set does not stop it —
    // which is why the case has to reach a verb that takes a KIND.
    let booted = ask(
        &http,
        &url,
        &session,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_here","arguments":{"bot":"assistant","brief":true}}}"#,
    )
    .await;
    let sid = booted
        .split("\\\"sid\\\":\\\"")
        .nth(1)
        .and_then(|rest| rest.split("\\\"").next())
        .unwrap_or_else(|| panic!("the door hands back a sid: {booted}"))
        .to_string();

    let mut asking = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"list_entities","arguments":{{"kind":"person","sid":"{sid}"}}}}}}"#
        ));
    if let Some(session) = &session {
        asking = asking.header("mcp-session-id", session.clone());
    }
    let body = asking
        .send()
        .await
        .expect("the verb answers")
        .text()
        .await
        .expect("a body");
    // The transport answers on an event stream whose first event carries no
    // data, so the answer is the first `data:` line that is JSON.
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find(|json| serde_json::from_str::<serde_json::Value>(json).is_ok())
        .unwrap_or_else(|| panic!("one JSON-RPC message: {body}"))
        .to_string()
}
