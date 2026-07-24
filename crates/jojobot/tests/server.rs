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
use tokio_util::sync::CancellationToken;

/// The Memory port for the transport/auth tests, which never call the memory
/// verbs — the real adapter, left unconfigured (no network). No toy store.
fn test_memory() -> std::sync::Arc<dyn jojobot_domain::memory::Memory> {
    std::sync::Arc::new(OutlineStore::unconfigured())
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
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: test_memory(),
    }
}

/// Auth-enabled state whose validator holds no keys — enough to prove the guard
/// mounts and rejects unauthenticated requests. Token *acceptance* is covered by
/// the unit golden tests in `auth.rs`.
fn auth_state(addr: SocketAddr) -> AppState {
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: Some("https://issuer.example".to_string()),
        validator: Some(Arc::new(Validator::from_keys(
            "https://issuer.example",
            "https://resource.example/mcp",
            HashMap::new(),
        ))),
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: test_memory(),
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
    AppState {
        resource: "https://jojobot.example/mcp".to_string(),
        issuer: None,
        validator: None,
        metadata_url: "https://jojobot.example/.well-known/oauth-protected-resource".to_string(),
        memory: test_memory(),
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

    client.cancel().await.unwrap();
    ct.cancel();
}
