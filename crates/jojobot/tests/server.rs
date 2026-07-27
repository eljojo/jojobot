//! Integration tests for the HTTP + MCP surface. These prove the walking
//! skeleton's features through automated tests rather than a manual run: the
//! public endpoints, the auth guard on `/mcp`, and a real end-to-end `ping`
//! round-trip through an rmcp client.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::auth::Validator;
use jojobot::{AppState, build_app};
use jojobot_adapters::outline::OutlineStore;
use jojobot_adapters::search::IndexedMemory;
use jojobot_adapters::vikunja::VikunjaStore;
use jojobot_adapters::vikunja::sessions::VikunjaSessions;
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::session::Sessions;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Search;
use jojobot_domain::memory::testing::InMemoryMemory;
use tokio_util::sync::CancellationToken;

mod support;

/// The Memory and Search ports for the transport/auth tests, which never call the
/// domain verbs — the real store, left unconfigured (no network), behind the real
/// index. No toy store, and the same one-adapter-two-ports pairing production
/// wires, so these tests can't pass on a shape the binary doesn't build.
/// The four ports a served app needs, as the transport/auth tests want them.
type TestPorts = (
    Arc<dyn Memory>,
    Arc<dyn Search>,
    Arc<dyn Mailboxes>,
    Arc<dyn Sessions>,
);

fn test_ports() -> TestPorts {
    let store: Arc<dyn Memory> = Arc::new(OutlineStore::unconfigured());
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    (
        indexed.clone(),
        indexed,
        Arc::new(VikunjaStore::unconfigured()),
        Arc::new(VikunjaSessions::unconfigured()),
    )
}

/// Bind an ephemeral port, build the app from `make_state`, and serve it on a
/// background task. Returns the bound address and a token that stops the server.
async fn spawn_server(
    make_state: impl FnOnce(SocketAddr) -> AppState,
) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = make_state(addr);
    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown.cancelled().await;
            })
            .await
            .unwrap();
    });
    (addr, ct)
}

fn no_auth_state(addr: SocketAddr) -> AppState {
    let (memory, search, mailboxes, sessions) = test_ports();
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory,
        search,
        mailboxes,
        sessions,
    }
}

/// Auth-enabled state whose validator holds no keys — enough to prove the guard
/// mounts and rejects unauthenticated requests. Token *acceptance* is covered by
/// the unit golden tests in `auth.rs`.
fn auth_state(addr: SocketAddr) -> AppState {
    let (memory, search, mailboxes, sessions) = test_ports();
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: Some("https://issuer.example".to_string()),
        validator: Some(Arc::new(Validator::from_keys(
            "https://issuer.example",
            "https://resource.example/mcp",
            HashMap::new(),
        ))),
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory,
        search,
        mailboxes,
        sessions,
    }
}

#[tokio::test]
async fn health_endpoint_is_public_and_ok() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let resp = reqwest::get(format!("http://{addr}/healthz")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
    ct.cancel();
}

#[tokio::test]
async fn metadata_advertises_the_issuer() {
    let (addr, ct) = spawn_server(auth_state).await;
    let body: serde_json::Value =
        reqwest::get(format!("http://{addr}/.well-known/oauth-protected-resource"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(body["resource"], format!("http://{addr}/mcp"));
    assert_eq!(body["authorization_servers"][0], "https://issuer.example");
    assert_eq!(body["bearer_methods_supported"][0], "header");
    ct.cancel();
}

#[tokio::test]
async fn mcp_rejects_unauthenticated_when_auth_enabled() {
    let (addr, ct) = spawn_server(auth_state).await;
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let challenge = resp
        .headers()
        .get("www-authenticate")
        .expect("401 must carry a WWW-Authenticate challenge")
        .to_str()
        .unwrap();
    assert!(challenge.contains("resource_metadata="), "challenge: {challenge}");
    ct.cancel();
}

#[tokio::test]
async fn mcp_guard_covers_path_and_method_variants() {
    // The bearer guard must cover every path under /mcp and every method — no
    // bypass via trailing slash, sub-path, traversal, encoded slash, or //.
    let (addr, ct) = spawn_server(auth_state).await;
    let client = reqwest::Client::new();
    let paths = ["/mcp", "/mcp/", "/mcp/anything", "/mcp/../mcp", "/mcp%2f", "//mcp"];
    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];
    for p in paths {
        for m in methods {
            let resp = client
                .request(m.parse().unwrap(), format!("http://{addr}{p}"))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), 401, "{m} {p} must require auth");
        }
    }
    ct.cancel();
}

/// Build auth-enabled state around a validator that already carries a subject
/// allowlist. Consumes the validator (it is not `Clone`), so each test builds
/// its own.
fn allowlist_state(validator: Validator) -> impl FnOnce(SocketAddr) -> AppState {
    move |addr| {
        let (memory, search, mailboxes, sessions) = test_ports();
        AppState {
            resource: format!("http://{addr}/mcp"),
            issuer: Some(support::ISS.to_string()),
            validator: Some(Arc::new(validator)),
            metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
            memory,
            search,
            mailboxes,
            sessions,
        }
    }
}

#[tokio::test]
async fn allowlist_forbids_a_validated_but_unlisted_token() {
    // The end-to-end guarantee: a token that *passes* validation but whose sub
    // is off the allowlist is denied at the edge with 403 — exercising the
    // authorize() call and the Forbidden→403 arm in require_bearer, not just the
    // unit-level authorize() decision.
    let idp = support::TestIdp::new();
    let token = idp.token("sub-stranger");
    let (addr, ct) = spawn_server(allowlist_state(idp.validator(&["sub-allowed"]))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        403,
        "a validly-signed token off the allowlist must be forbidden at the edge"
    );
    ct.cancel();
}

#[tokio::test]
async fn allowlist_admits_a_listed_token_to_the_handler() {
    // A listed sub clears both authentication and authorization, so it reaches
    // the MCP handler — neither 401 nor 403.
    let idp = support::TestIdp::new();
    let token = idp.token("sub-allowed");
    let (addr, ct) = spawn_server(allowlist_state(idp.validator(&["sub-allowed"]))).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(resp.status(), 401, "a listed token must pass authentication");
    assert_ne!(resp.status(), 403, "a listed token must pass authorization");
    ct.cancel();
}

#[tokio::test]
async fn mcp_is_open_when_auth_disabled() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    // A bare POST is not a valid MCP request, but with auth off it must never be
    // a 401 — the guard is simply not mounted.
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .send()
        .await
        .unwrap();
    assert_ne!(resp.status(), 401);
    ct.cancel();
}

/// Auth-off state whose resource is a *public* URL, so the transport's Host
/// allowlist must accept that hostname rather than only loopback.
fn public_no_auth_state(_addr: SocketAddr) -> AppState {
    let (memory, search, mailboxes, sessions) = test_ports();
    AppState {
        resource: "https://jojobot.example/mcp".to_string(),
        issuer: None,
        validator: None,
        metadata_url: "https://jojobot.example/.well-known/oauth-protected-resource".to_string(),
        memory,
        search,
        mailboxes,
        sessions,
    }
}

#[tokio::test]
async fn mcp_accepts_public_host_but_still_guards_dns_rebinding() {
    // Behind a tunnel the inbound Host is our public hostname. The transport
    // must accept the host derived from the resource, while still rejecting a
    // stray/spoofed Host — the DNS-rebinding guard stays on. A disallowed Host
    // yields 403; anything else means the request got past the guard.
    let (addr, ct) = spawn_server(public_no_auth_state).await;
    let client = reqwest::Client::new();

    let allowed = client
        .post(format!("http://{addr}/mcp"))
        .header("host", "jojobot.example")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(
        allowed.status(),
        403,
        "the resource's own host must not be rejected as DNS rebinding"
    );

    let spoofed = client
        .post(format!("http://{addr}/mcp"))
        .header("host", "evil.example")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(spoofed.status(), 403, "an unknown Host must still be forbidden");

    ct.cancel();
}

/// A **live** store behind the real index, wired the way `main` wires it: one
/// `IndexedMemory` serving both ports. The fake stands in for Outline; nothing
/// else about the pairing is faked.
fn searchable_state(addr: SocketAddr) -> AppState {
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("the search index opens"),
    );
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search: indexed,
        mailboxes: Arc::new(VikunjaStore::unconfigured()),
        sessions: Arc::new(VikunjaSessions::unconfigured()),
    }
}

/// **The front door, over the wire.** A fact captured through the `capture` tool
/// is findable through the `search` tool on the next call — no restart — and
/// comes back as a fact hit carrying its address and provenance.
///
/// This is the one claim the composition root makes that no lower test can:
/// that the port `search` reads is the projection over the store the memory
/// verbs write to. Wire two stores together and everything below still passes
/// while the assistant can't find what it just wrote.
#[tokio::test]
async fn a_fact_captured_through_the_front_door_is_findable_there() {
    use rmcp::ServiceExt;
    use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
    use rmcp::transport::StreamableHttpClientTransport;

    let (addr, ct) = spawn_server(searchable_state).await;
    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("jojobot-test-client", "0.0.1"),
    )
    .serve(transport)
    .await
    .unwrap();

    let tools = client.list_tools(Default::default()).await.unwrap().tools;
    assert!(
        tools.iter().any(|t| t.name == "search"),
        "the server must advertise the search tool"
    );

    // **The advertised schema text is the only spec the caller ever reads**, and
    // it is what an AI plans against. A description that promises screening the
    // server no longer does — or omits a gate it now has — is a bug with no
    // stack trace: the caller writes a call that cannot succeed and has no way
    // to know why. Pinned here, two lines above the flow it describes.
    let capture_doc = tools
        .iter()
        .find(|t| t.name == "capture")
        .and_then(|t| t.description.clone())
        .expect("capture must be advertised, with a description");
    assert!(
        capture_doc.contains("add_entity"),
        "capture's description must name the two-step flow its gate forces: {capture_doc}"
    );
    assert!(
        !capture_doc.contains("create_new"),
        "capture has no create_new; promising one sends the caller round a loop: {capture_doc}"
    );

    // A subject must exist before a fact about it can land, so the probe is the
    // two deliberate steps a new entity takes — end to end, through the wire.
    let added = client
        .call_tool(CallToolRequestParams::new("add_entity").with_arguments(
            serde_json::json!({
                "kind": "person",
                "handle": "frontdoor-probe",
                "name": "Frontdoor Probe",
                "source": "user-named",
            })
            .as_object()
            .unwrap()
            .clone(),
        ))
        .await
        .unwrap();
    let added = serde_json::to_string(&added).unwrap();
    assert!(!added.contains("\"status\":\"blocked\""), "add_entity must land: {added}");

    let captured = client
        .call_tool(CallToolRequestParams::new("capture").with_arguments(
            serde_json::json!({
                "subject": "person:frontdoor-probe",
                "content": "keeps a zamboni in the garage",
                "provenance": "testimony",
                "date": "2026-07-01",
            })
            .as_object()
            .unwrap()
            .clone(),
        ))
        .await
        .unwrap();
    let captured = serde_json::to_string(&captured).unwrap();
    assert!(!captured.contains("\"isError\":true"), "capture must succeed: {captured}");
    // A blocked write is a *successful* result now, so isError alone no longer
    // catches one — the body is what says whether anything was written.
    assert!(
        !captured.contains("\"status\":\"blocked\""),
        "capture must not have been blocked: {captured}"
    );

    let found = client
        .call_tool(
            CallToolRequestParams::new("search").with_arguments(
                serde_json::json!({ "query": "zamboni" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let found = serde_json::to_string(&found).unwrap();
    assert!(
        found.contains("person:frontdoor-probe#f1"),
        "the fact just captured must be findable, with its address: {found}"
    );
    assert!(found.contains("testimony"), "…and its provenance: {found}");

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
async fn ping_tool_round_trips_end_to_end() {
    use rmcp::ServiceExt;
    use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
    use rmcp::transport::StreamableHttpClientTransport;

    let (addr, ct) = spawn_server(no_auth_state).await;

    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client_info = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("jojobot-test-client", "0.0.1"),
    );
    let client = client_info.serve(transport).await.unwrap();

    // The server must advertise the ping tool. If the tool router weren't wired,
    // this list would be empty — the test that catches the "never read" warning.
    let tools = client.list_tools(Default::default()).await.unwrap();
    assert!(
        tools.tools.iter().any(|t| t.name == "ping"),
        "server must advertise the ping tool, got: {:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // And it must actually answer.
    let result = client
        .call_tool(CallToolRequestParams::new("ping"))
        .await
        .unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("jojobot"), "ping result should name the server: {json}");
    assert!(json.contains("ok"), "ping result should report ok: {json}");

    // **The boot verb, called the way a client with nothing to say calls it —
    // no `arguments` member at all.** `start_here` grew optional arguments; if
    // an absent `arguments` were ever rejected, every session would come up
    // blind, and no test that constructs `Parameters(..)` in-process would see
    // it because none of them cross the JSON boundary. This one does.
    let oriented = client
        .call_tool(CallToolRequestParams::new("start_here"))
        .await
        .expect("start_here must answer a call that carries no arguments at all");
    let json = serde_json::to_string(&oriented).unwrap();
    assert!(
        json.contains("orientation_elided"),
        "orientation should land on a bare call: {json}"
    );

    client.cancel().await.unwrap();
    ct.cancel();
}
