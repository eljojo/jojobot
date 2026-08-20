//! Integration tests for the HTTP + MCP surface. These prove the walking
//! skeleton's features through automated tests rather than a manual run: the
//! public endpoints, the auth guard on `/mcp`, and a real end-to-end `ping`
//! round-trip through an rmcp client.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::auth::Validator;
use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Search;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::Sessions;
use tokio_util::sync::CancellationToken;

mod support;
use support::{event_stream_error, event_stream_result, open_with, request_meta};

/// The Memory and Search ports for the transport/auth tests, which never call the
/// domain verbs — the in-memory double behind the real index, which is the same
/// store-behind-index pairing production wires, so these tests can't pass on a
/// shape the binary doesn't build.
///
/// **The double rather than the real store**, for the reason mail and sessions
/// use one here: the real one is a database process, no case in this file calls
/// a memory verb, and standing a server up per case would buy nothing.
///
/// The cases that DO exercise these stores live in the adapter crate, against
/// the real one.
///
/// The four ports a served app needs, as the transport/auth tests want them.
type TestPorts = (
    Arc<dyn Memory>,
    Arc<dyn Search>,
    Arc<dyn Mailboxes>,
    Arc<dyn Sessions>,
);

fn test_ports() -> TestPorts {
    let store: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    (
        indexed,
        search,
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
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
    // **Every served jojobot arrives with its default identity**, exactly as
    // the composition root does it — before anything can connect. A memory
    // write needs an identity now, so a server without one could not be
    // written to at all, and no real deployment is in that state.
    jojobot_mcp::seed::ensure_default_identity(&state.memory, &state.mailboxes).await;
    // …and with the vocabulary it ships, for the same reason: a caller asking
    // for a shipped type on a real instance finds one there.
    let _ = jojobot_mcp::seed::ensure_shipped_types(&state.memory).await;
    let _ = jojobot_mcp::seed::ensure_kinds(&state.memory).await;
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
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
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
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
    }
}

#[tokio::test]
async fn health_endpoint_is_public_and_ok() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let resp = reqwest::get(format!("http://{addr}/healthz"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
    ct.cancel();
}

#[tokio::test]
async fn metadata_advertises_the_issuer() {
    let (addr, ct) = spawn_server(auth_state).await;
    let body: serde_json::Value = reqwest::get(format!(
        "http://{addr}/.well-known/oauth-protected-resource"
    ))
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
    assert!(
        challenge.contains("resource_metadata="),
        "challenge: {challenge}"
    );
    ct.cancel();
}

#[tokio::test]
async fn mcp_guard_covers_path_and_method_variants() {
    // The bearer guard must cover every path that REACHES the transport, by
    // every method — no bypass via trailing slash, sub-path, traversal,
    // encoded slash, or //.
    //
    // A path that reaches the transport answers 401. `/mcp%2f` and `//mcp`
    // reach nothing: neither matches a route, so the request is over before
    // the handler exists, and each answers 404 for the same reason any other
    // unmounted path does. Both answers keep an unauthenticated caller out of
    // the handler; they differ in which problem they name, and every path is
    // pinned to the one that is true of it.
    let (addr, ct) = spawn_server(auth_state).await;
    let client = reqwest::Client::new();
    let paths = [
        ("/mcp", 401),
        ("/mcp/", 401),
        ("/mcp/anything", 401),
        ("/mcp/../mcp", 401),
        ("/mcp%2f", 404),
        ("//mcp", 404),
    ];
    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];
    for (p, want) in paths {
        for m in methods {
            let resp = client
                .request(m.parse().unwrap(), format!("http://{addr}{p}"))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), want, "{m} {p}");
        }
    }
    ct.cancel();
}

/// **A path jojobot does not serve answers 404, with auth on.** The bearer
/// guard covers `/mcp`; it must not cover the router's fallback as well. A
/// client probing an endpoint this server never implemented is told the path
/// is not here, rather than that its credentials are wrong — an answer that
/// names the wrong problem sends the caller to fix something that is not
/// broken (rule 68).
#[tokio::test]
async fn an_unmounted_path_is_not_found_rather_than_unauthorized() {
    let (addr, ct) = spawn_server(auth_state).await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/not-a-jojobot-endpoint"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        404,
        "an unmounted path must report itself missing"
    );
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
            registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
            ui: None,
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
    assert_ne!(
        resp.status(),
        401,
        "a listed token must pass authentication"
    );
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
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
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
    assert_eq!(
        spoofed.status(),
        403,
        "an unknown Host must still be forbidden"
    );

    ct.cancel();
}

/// A **live** store behind the real index, wired the way `main` wires it: one
/// `IndexedMemory` serving both ports. The fake stands in for Outline; nothing
/// else about the pairing is faked.
fn searchable_state(addr: SocketAddr) -> AppState {
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("the search index opens"),
    );
    AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: None,
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search: Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()])),
        // **Real session and mailbox ports, unlike the other fixtures here, and
        // not to make a test pass.** A memory write now carries an identity, an
        // identity needs a `sid`, and a `sid` needs a session world that can
        // start one. A deployment that captures facts therefore HAS a session
        // store — so this fixture has one because the deployment it stands for
        // does, not because the assertion below wants it. The half-wired shape
        // is a legitimate scenario and it is `no_auth_state`'s, where losing
        // writes is the correct behaviour rather than a gap.
        mailboxes: Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ),
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: None,
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
    // server does not do — or omits a gate it has — is a bug with no stack
    // trace: the caller writes a call that cannot succeed and has no way to
    // know why. Pinned here, two lines above the flow it describes.
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
        !capture_doc.contains("override_token"),
        "capture has no override_token; promising one offers a way out that does not \
         exist: {capture_doc}"
    );

    // **The boot comes first, because a write with nobody behind it does not
    // land.** Every real client does exactly this: walk through the door,
    // then carry the sid. The identity is the one every jojobot arrives with,
    // which is what makes the gate shippable rather than a bootstrap trap.
    let booted = client
        .call_tool(
            CallToolRequestParams::new("start_here").with_arguments(
                serde_json::json!({ "bot": "assistant", "brief": true })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let booted: serde_json::Value = serde_json::from_str(
        booted
            .content
            .first()
            .and_then(|b| b.as_text())
            .map(|t| t.text.as_str())
            .expect("start_here answers with text"),
    )
    .expect("start_here answers with json");
    let sid = booted["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the default identity must boot: {booted}"))
        .to_string();

    // A subject must exist before a fact about it can land, so the probe is the
    // two deliberate steps a new entity takes — end to end, through the wire.
    let added = client
        .call_tool(
            CallToolRequestParams::new("add_entity").with_arguments(
                serde_json::json!({
                    "kind": "person",
                    "handle": "frontdoor-probe",
                    "name": "Frontdoor Probe",
                    "source": "user-named",
                    "sid": sid,
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();
    let added = serde_json::to_string(&added).unwrap();
    assert!(
        !added.contains("\"status\":\"blocked\""),
        "add_entity must land: {added}"
    );

    let captured = client
        .call_tool(
            CallToolRequestParams::new("capture").with_arguments(
                serde_json::json!({
                    "subject": "person:frontdoor-probe",
                    "content": "keeps a zamboni in the garage",
                    "provenance": "testimony",
                    "date": "2026-07-01",
                    "sid": sid,
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();
    let captured = serde_json::to_string(&captured).unwrap();
    assert!(
        !captured.contains("\"isError\":true"),
        "capture must succeed: {captured}"
    );
    // A blocked write is a *successful* result, so isError alone does not catch
    // one — the body is what says whether anything was written.
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
    assert!(
        json.contains("jojobot"),
        "ping result should name the server: {json}"
    );
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

/// **A client that was connected when the surface changed must be told the
/// list moved.**
///
/// Paid for in production: two sessions against one deployment held different
/// tool lists. The one that had registered before a surface change still had
/// the old seven verbs — no `start_here`, no mailbox verbs — so it could not
/// boot as its bot or read its own box, and the brief waiting for it went
/// unread. A client caches the list at registration; if the server never says
/// the list moved, it never looks again.
///
/// Two halves, and a client needs both. The **capability** says this server's
/// list is not a constant, which is what makes a cached copy something to
/// revalidate rather than a fact. The **notification** is the actual nudge, and
/// it goes out when a client initializes — the one moment jojobot knows a
/// client is holding a list and can still reach it.
mod surface_changed {
    use super::*;
    use rmcp::ServiceExt;
    use rmcp::model::{ClientCapabilities, ClientInfo, Implementation};
    use rmcp::transport::StreamableHttpClientTransport;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A client that records one thing: whether it was ever told.
    #[derive(Clone, Default)]
    struct Listening {
        told: Arc<AtomicBool>,
    }

    impl rmcp::ClientHandler for Listening {
        fn on_tool_list_changed(
            &self,
            _context: rmcp::service::NotificationContext<rmcp::RoleClient>,
        ) -> impl std::future::Future<Output = ()> + Send + '_ {
            self.told.store(true, Ordering::SeqCst);
            std::future::ready(())
        }

        fn get_info(&self) -> ClientInfo {
            ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("jojobot-listening-client", "0.0.1"),
            )
        }
    }

    #[tokio::test]
    async fn a_connected_client_is_told_the_tool_list_can_move() {
        let (addr, ct) = spawn_server(no_auth_state).await;
        let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
        let listening = Listening::default();
        let client = listening.clone().serve(transport).await.unwrap();

        // **The capability.** Without it a client is entitled to treat its
        // cached list as final, and the notification below is one it never
        // agreed to receive.
        let caps = client
            .peer_info()
            .expect("the server answered initialize")
            .capabilities
            .clone();
        assert_eq!(
            caps.tools.as_ref().and_then(|t| t.list_changed),
            Some(true),
            "the server must declare that its tool list can move: {caps:?}"
        );

        // **The notification.** Paired with the positive above: the capability
        // alone is a promise, and this is the server keeping it.
        //
        // It is sent on initialize, which arrives before `serve` returns — but
        // it is a notification and the client dispatches it on its own task, so
        // a poll is the honest way to observe it rather than a bare read that
        // races the dispatcher.
        let told = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !listening.told.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        assert!(
            told,
            "a client that just connected was never told the list can have moved under it"
        );

        client.cancel().await.unwrap();
        ct.cancel();
    }
}

/// **A request as a client on `revision` makes it.** The method it calls, and
/// the verb a call names, ride as headers so a middle box can route without
/// reading the body (SEP-2243); the request states its own version, identity
/// and capabilities in `_meta` (SEP-2575). From the 2026-07-28 revision on,
/// jojobot refuses a request that leaves any of them out.
fn over(
    http: &reqwest::Client,
    url: &str,
    revision: &str,
    session: Option<&str>,
    id: u32,
    method: &str,
    mut params: serde_json::Value,
) -> reqwest::RequestBuilder {
    let name = params["name"].as_str().map(str::to_string);
    params["_meta"] = request_meta(revision);
    let mut asking = http
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", revision.to_string())
        .header("mcp-method", method.to_string())
        .body(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            })
            .to_string(),
        );
    if let Some(name) = name {
        asking = asking.header("mcp-name", name);
    }
    if let Some(session) = session {
        asking = asking.header("mcp-session-id", session.to_string());
    }
    asking
}

/// A revision from after the newest one this build serves. It is not a name the
/// SDK knows, because there is no such name yet: the case under test is a
/// client that arrives speaking something newer than jojobot, which is what
/// every client eventually does.
const BEYOND: &str = "2099-01-01";

/// **jojobot serves the revision it agrees to speak, and refuses the ones it
/// does not.**
///
/// A client opens with the newest revision it knows, and everything after the
/// handshake is read against the revision the two of them agreed — so a result
/// missing a field that revision makes mandatory is discarded whole. A client
/// in that state holds no tool list at all: it reports the server connected and
/// has no memory rail and no mailbox, which looks nothing like an outage from
/// the inside. So a revision jojobot does not serve in full is refused at the
/// handshake, with the list of the ones it does, and the client opens again on
/// one of those.
///
/// What the agreed revision then requires of the payload is
/// `the_newest_revision_is_served_whole_and_not_merely_agreed`.
///
/// It goes over raw HTTP rather than through the typed client, because the
/// typed client deserializes into this SDK's own structs and a field the SDK
/// does not know is a field no assertion could see.
#[tokio::test]
async fn a_revision_jojobot_cannot_serve_is_refused_with_the_ones_it_can() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let url = format!("http://{addr}/mcp");
    let http = reqwest::Client::new();

    let refused = event_stream_error(&open_with(&http, &url, BEYOND).await.1);
    let served: Vec<String> = refused["data"]["supported"]
        .as_array()
        .expect("a refusal names the revisions this server does serve")
        .iter()
        .map(|v| v.as_str().expect("a revision is a string").to_string())
        .collect();
    assert!(
        !served.iter().any(|v| v.as_str() >= BEYOND),
        "a revision this server cannot serve must not be offered back: {served:?}",
    );

    // The client opens again on the newest revision it was offered, which is
    // what a real one does with that refusal.
    let newest = served.iter().max().expect("at least one revision").clone();
    let agreed = event_stream_result(&open_with(&http, &url, &newest).await.1)["protocolVersion"]
        .as_str()
        .expect("the handshake names the revision the two of us will speak")
        .to_string();
    assert_eq!(
        agreed, newest,
        "the second handshake agrees what it was offered"
    );

    ct.cancel();
}

/// **The newest revision jojobot knows is one it serves whole.**
///
/// The cap in `NEWEST_SERVED` exists because a client reads every answer after
/// the handshake against the revision the two of them agreed, and a result
/// missing a field that revision makes mandatory is discarded whole. So the
/// test of "jojobot serves 2026-07-28" is not that the handshake succeeds: it
/// is that the answers carry what that revision demands. `tools/list` under it
/// must carry `ttlMs` and `cacheScope` (SEP-2549), and a call over it must come
/// back with a body.
///
/// It goes over raw HTTP for the same reason the refusal beside it does: a
/// typed client deserializes into this SDK's own structs, so a field the SDK
/// does not know is a field no assertion could see.
#[tokio::test]
async fn the_newest_revision_is_served_whole_and_not_merely_agreed() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let url = format!("http://{addr}/mcp");
    let http = reqwest::Client::new();

    let (session, opened) = open_with(&http, &url, "2026-07-28").await;
    let agreed = event_stream_result(&opened)["protocolVersion"]
        .as_str()
        .expect("the handshake names the revision the two of us will speak")
        .to_string();
    assert_eq!(
        agreed, "2026-07-28",
        "a client asking for the newest revision must be answered with it"
    );

    let listed = over(
        &http,
        &url,
        &agreed,
        session.as_deref(),
        2,
        "tools/list",
        serde_json::json!({}),
    )
    .send()
    .await
    .unwrap();
    let status = listed.status();
    let body = listed.text().await.unwrap();
    assert!(
        status.is_success(),
        "the list a client asks for over the agreed revision must be served: {status} {body}",
    );
    let result = event_stream_result(&body);
    assert!(
        !result["tools"]
            .as_array()
            .expect("a list of tools")
            .is_empty(),
        "the server must advertise its tools: {result}",
    );
    assert!(
        result["ttlMs"].is_number(),
        "this revision requires ttlMs on a tool list, and a client throws the whole list away \
         without it: {result}",
    );
    assert!(
        matches!(result["cacheScope"].as_str(), Some("public" | "private")),
        "this revision requires cacheScope to be public or private: {result}",
    );

    let pinged = over(
        &http,
        &url,
        &agreed,
        session.as_deref(),
        3,
        "tools/call",
        serde_json::json!({ "name": "ping", "arguments": {} }),
    )
    .send()
    .await
    .unwrap();
    let answered = event_stream_result(&pinged.text().await.unwrap());
    // Which half of the answer the body rides in is this revision's own
    // question, and `the_body_rides_where_the_agreed_revision_reads_it` is
    // where it is asked. Here it only has to arrive.
    assert!(
        answered["structuredContent"].is_object() || answered["content"][0]["text"].is_string(),
        "a verb called over the agreed revision must answer with a body: {answered}",
    );

    ct.cancel();
}

/// **One body, in the place the agreed revision reads it.**
///
/// Every verb answers with a JSON body. Where that body rides is the one thing
/// that changes with the revision: a client old enough to know only content
/// blocks gets it as text, and a client on the revision that has
/// `structuredContent` gets it there — as a JSON object it can read, rather
/// than a string it has to parse.
///
/// **It is never sent twice.** The spec permits both, and both is what most
/// servers do; here it would mean every answer on the wire carrying its whole
/// body a second time, for a reader that already has it. What a caller
/// demonstrably holds is not shipped back to them, and a duplicate is the
/// clearest case of that rule there is.
#[tokio::test]
async fn the_body_rides_where_the_agreed_revision_reads_it() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let url = format!("http://{addr}/mcp");
    let http = reqwest::Client::new();

    // ── the newest revision: the body is structured, and only structured ────
    let (session, opened) = open_with(&http, &url, "2026-07-28").await;
    assert_eq!(
        event_stream_result(&opened)["protocolVersion"],
        "2026-07-28",
        "this case is about a client that agreed the newest revision"
    );
    let answered = event_stream_result(
        &over(
            &http,
            &url,
            "2026-07-28",
            session.as_deref(),
            2,
            "tools/call",
            serde_json::json!({ "name": "ping", "arguments": {} }),
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap(),
    );
    assert!(
        answered["structuredContent"]["server"].is_string(),
        "this revision reads the body as an object, not as a string: {answered}",
    );
    assert!(
        answered["content"].as_array().is_none_or(Vec::is_empty),
        "the body a client already holds must not ride twice: {answered}",
    );

    // ── an older revision: the body is text, exactly as it always was ───────
    let (session, opened) = open_with(&http, &url, "2025-11-25").await;
    assert_eq!(
        event_stream_result(&opened)["protocolVersion"],
        "2025-11-25",
        "…and this case is about a client that has no structured content"
    );
    let mut asking = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .body(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "ping", "arguments": {} },
            })
            .to_string(),
        );
    if let Some(session) = &session {
        asking = asking.header("mcp-session-id", session.clone());
    }
    let answered = event_stream_result(&asking.send().await.unwrap().text().await.unwrap());
    let text = answered["content"][0]["text"]
        .as_str()
        .expect("a client without structured content is answered with a text block");
    assert!(
        serde_json::from_str::<serde_json::Value>(text)
            .is_ok_and(|body| body["server"].is_string()),
        "the text block carries the whole body: {answered}",
    );
    assert!(
        answered["structuredContent"].is_null(),
        "a client that never agreed to structured content is not sent any: {answered}",
    );

    ct.cancel();
}

/// **jojobot introduces itself, and the library does not introduce it.**
///
/// `serverInfo` is the FIRST thing a client learns about this server: what it
/// displays, what it logs, and what somebody reads out when they are asked
/// what they are connected to. It named the MCP library and the library's
/// version.
///
/// **The handler always set the right thing.** `get_info` builds its identity
/// with the SDK's own `Implementation::from_build_env`, and that constructor is
/// a function compiled INSIDE the library, so its `env!` reads the library's
/// crate name and version rather than the caller's. Nothing was lost between
/// the handler and the wire: the value was the library's from the moment it was
/// made.
///
/// **Rule 53 is what it breaks.** An agent may know how jojobot is arranged and
/// never what is true only of the machinery underneath it — and this is that
/// leak arriving at the one door nobody swept.
///
/// **The two doors must also agree.** `ping` exists so a caller can ask which
/// build answered, and a server with two answers about its own identity is the
/// real defect: the version that shows in a client's window is the one somebody
/// quotes in an incident.
///
/// Read off raw HTTP because the typed client deserializes into the library's
/// own structs, and a field it does not know is one no assertion could see.
#[tokio::test]
async fn the_handshake_names_jojobot_and_agrees_with_what_ping_reports() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let url = format!("http://{addr}/mcp");
    let http = reqwest::Client::new();

    let (session, opened) = open_with(&http, &url, "2025-06-18").await;
    let served = event_stream_result(&opened);
    let name = served["serverInfo"]["name"]
        .as_str()
        .expect("the handshake says who is answering");
    let version = served["serverInfo"]["version"]
        .as_str()
        .expect("…and which build of it");
    assert_eq!(
        name, "jojobot",
        "the handshake introduces this server as {name:?}. It is the product's name that a \
         client displays — not the library's, and not the crate that happens to serve the \
         surface, which is jojobot's own arrangement and no business of a caller's (rule 53)"
    );
    // **The version is a fact about the binary and is not hand-maintained.**
    // A literal would pass every check above while going stale the first time
    // nobody remembered it, so it is compared against the build's own.
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "the version served must be the one this was built at, read from the build rather than \
         written down"
    );

    // **The other door, over the same connection.** Either half could be right
    // while the other is wrong, so both are read and compared.
    let mut asking = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .body(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ping","arguments":{}}}"#,
        );
    if let Some(session) = &session {
        asking = asking.header("mcp-session-id", session.clone());
    }
    let pinged = asking.send().await.expect("ping reaches jojobot");
    let text = event_stream_result(&pinged.text().await.expect("a body"))["content"][0]["text"]
        .as_str()
        .expect("ping answers with a body")
        .to_string();
    let ping: serde_json::Value = serde_json::from_str(&text).expect("ping answers json");

    assert_eq!(
        ping["server"], name,
        "the two doors must not disagree about who this server is: {ping}"
    );
    assert_eq!(
        ping["version"], version,
        "…nor about which build of it answered: {ping}"
    );

    ct.cancel();
}

/// **The same refusal at the other door.**
///
/// From the 2026-07-28 revision on, a request carries its own version, so a
/// caller can ask for a tool list with no handshake behind it at all. That door
/// has to hold the same line as the handshake — otherwise the revision jojobot
/// refuses to agree to is served anyway, to the caller who never asked.
#[tokio::test]
async fn a_request_declaring_a_revision_jojobot_cannot_serve_is_refused_too() {
    let (addr, ct) = spawn_server(no_auth_state).await;
    let listed = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", BEYOND)
        .header("mcp-method", "tools/list")
        // The revision carries its own version, client info and capabilities on
        // every request (SEP-2575), which is what makes a handshake optional.
        .body(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": { "_meta": {
                    "io.modelcontextprotocol/protocolVersion": BEYOND,
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "jojobot-test-client",
                        "version": "0.0.1",
                    },
                    "io.modelcontextprotocol/clientCapabilities": {},
                }},
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();

    let refused = event_stream_error(&listed.text().await.unwrap());
    assert_eq!(
        refused["data"]["requested"], BEYOND,
        "the refusal names the revision that was asked for: {refused}",
    );
    assert!(
        !refused["data"]["supported"]
            .as_array()
            .expect("a refusal names the revisions this server does serve")
            .iter()
            .any(|v| v.as_str().is_some_and(|v| v >= BEYOND)),
        "a revision this server cannot serve must not be offered back: {refused}",
    );

    ct.cancel();
}
