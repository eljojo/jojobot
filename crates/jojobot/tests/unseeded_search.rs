//! **A search on a process that loaded no kinds is not a damaged index.**
//!
//! The index stores each hit's record as JSON, and reading one back parses the
//! kind out of it. On a process that never loaded the set, that parse fails on
//! every document — and the failure was reported as the store failing, which
//! comes back to a caller as a protocol error saying jojobot's own storage
//! broke and a person must look at it. The index is intact, and the fault is
//! the one every other verb on this rail already names.
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
async fn a_search_on_an_unloaded_process_is_an_answer_rather_than_a_broken_store() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    // **Booted on purpose, and emptied again below.** The index only has
    // documents because records were written while the set was loaded, and
    // that write is the reason the store is booted here. The set is emptied
    // before the search, which is the half this case is about.
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
    // Records written and indexed while the set was loaded, which is where the
    // index gets documents in the first place.
    jojobot_mcp::seed::ensure_default_identity(&state.memory, &state.mailboxes).await;

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
            .unwrap();
    });

    let http = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");
    let session = open(&http, &url).await;
    let sid = boot(&http, &url, &session).await;

    jojobot_domain::memory::kinds::load::<[&str; 0], &str>([]);
    let unloaded: serde_json::Value =
        serde_json::from_str(&ask(&http, &url, &session, &search(&sid)).await)
            .expect("a JSON-RPC message");

    // **An answer, not a protocol error.** The old failure never reached the
    // body at all: it came back as an error object, which is the channel this
    // rail keeps for the store actually being down.
    assert!(
        unloaded["error"].is_null(),
        "a process that loaded no kinds reports its own state as a failure: {unloaded}",
    );
    let body = body_of(&unloaded);
    assert_eq!(
        body["wrote"], false,
        "a refused search wrote something: {body}"
    );
    let advice = body["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a blocked answer carries a way forward: {body}"));
    assert!(
        advice.contains("never loaded"),
        "the refusal does not name the fault: {advice}",
    );

    // **The control: the same call, once the set is loaded.** Without it, an
    // assertion about a refusal passes identically on a build where this
    // search can never answer at all.
    jojobot_domain::memory::kinds::load_shipped();
    let loaded: serde_json::Value =
        serde_json::from_str(&ask(&http, &url, &session, &search(&sid)).await)
            .expect("a JSON-RPC message");
    let served = body_of(&loaded);
    assert!(
        served["count"].as_u64().is_some_and(|count| count > 0),
        "the same search finds nothing with the set loaded, so the refusal above says \
         nothing about the set: {served}",
    );

    ct.cancel();
}

/// The answer's own body, parsed out of the tool result.
fn body_of(message: &serde_json::Value) -> serde_json::Value {
    let text = message["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("an answer with a body: {message}"));
    serde_json::from_str(text).expect("the answer is JSON")
}

fn search(sid: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"search","arguments":{{"query":"assistant","sid":"{sid}"}}}}}}"#
    )
}

async fn open(http: &reqwest::Client, url: &str) -> Option<String> {
    let opened = http
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"unseeded","version":"0.0.1"}}}"#,
        )
        .send()
        .await
        .expect("the door answers");
    opened
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string())
}

async fn boot(http: &reqwest::Client, url: &str, session: &Option<String>) -> String {
    let booted = ask(
        http,
        url,
        session,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_here","arguments":{"bot":"assistant","brief":true}}}"#,
    )
    .await;
    let message: serde_json::Value = serde_json::from_str(&booted).expect("a JSON-RPC message");
    body_of(&message)["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the door hands back a sid: {booted}"))
        .to_string()
}

/// One tools/call, answered as the first JSON message on the event stream.
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
