//! **The way forward a caller gets when nothing loaded the kinds.**
//!
//! A refusal carries a repair (rule 68). Every other malformed call on this
//! rail is repaired by sending the same call again with the bad argument
//! fixed, and the answer says so, naming the verb. **This fault is not one of
//! those.** Nothing in the call reaches it: the set of kinds is loaded at
//! startup and no verb re-reads it, so the repair is a person restarting the
//! server. An answer that tells the caller to fix the call sends a model round
//! a loop with no end.
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
async fn the_never_loaded_refusal_does_not_send_the_caller_back_into_the_same_call() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    // **Booted on purpose, and emptied again below.** Two preconditions used
    // to arrive on one line: the records this case needs, and the set it
    // exists to find missing. Seeding an identity is a write that parses a
    // handle, so the store is booted for it — and the set is emptied before
    // the call under test, which is the state this case is about.
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
        teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
        clock: jojobot_domain::clock::Clock::default(),
    };
    // The store is fine and the set is not loaded — what `main` tolerates when
    // the store was unreachable at startup, and serves anyway.
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
    let never_loaded = advice(&ask(&http, &url, &session, &capture(&sid, "a claim")).await);

    // **The control, taken from the same verb on the same server.** With the
    // set loaded, the same call carrying an empty claim is refused for a
    // mistake the caller really can fix, and that answer NAMES THE VERB
    // because it is telling the caller to send it again. Without this half,
    // the assertion below passes on a build where no answer ever names a verb.
    jojobot_domain::memory::kinds::load_shipped();
    let ordinary = advice(&ask(&http, &url, &session, &capture(&sid, "   ")).await);

    assert!(
        never_loaded.contains("never loaded"),
        "the refusal does not name the fault: {never_loaded}",
    );
    assert!(
        ordinary.contains("capture"),
        "the ordinary refusal stopped naming the verb to send again, so the check below \
         proves nothing: {ordinary}",
    );
    assert!(
        !never_loaded.contains("capture"),
        "the never-loaded refusal sends the caller back into the same call, which cannot \
         reach the fault: {never_loaded}",
    );

    ct.cancel();
}

/// The `how_to_proceed` line of a blocked answer.
fn advice(answered: &str) -> String {
    let body: serde_json::Value = serde_json::from_str(answered).expect("a JSON-RPC message");
    let text = body["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("an answer with a body: {answered}"));
    let inner: serde_json::Value = serde_json::from_str(text).expect("the answer is JSON");
    assert_eq!(inner["wrote"], false, "the call wrote something: {inner}");
    inner["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a blocked answer carries a way forward: {inner}"))
        .to_string()
}

fn capture(sid: &str, content: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"capture","arguments":{{"subject":"bot:assistant","content":"{content}","sid":"{sid}"}}}}}}"#
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
    let body: serde_json::Value = serde_json::from_str(&booted).expect("a JSON-RPC message");
    let text = body["result"]["content"][0]["text"]
        .as_str()
        .expect("the door answers with a body");
    let inner: serde_json::Value = serde_json::from_str(text).expect("the answer is JSON");
    inner["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the door hands back a sid: {inner}"))
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
