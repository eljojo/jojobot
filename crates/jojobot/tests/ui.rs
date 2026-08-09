//! Integration tests for the browser listing: the gate, the login round trip
//! through an issuer, and the index of the roots.
//!
//! The issuer here is a real HTTP server that speaks the authorization-code
//! flow — it holds the PKCE challenge it was sent and refuses to exchange a
//! code against the wrong verifier. A login that never left this process would
//! prove the handlers and not the flow.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::Engine;
use jojobot::auth::IssuerEndpoints;
use jojobot::config::UiConfig;
use jojobot::ui::Ui;
use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::search::Search;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};
use jojobot_domain::session::Sessions;
use serde_json::json;
use tokio_util::sync::CancellationToken;

mod support;

const CLIENT_ID: &str = "jojobot-ui";
const READER: &str = "sub-reader";

// --- the issuer ------------------------------------------------------------

/// A minimal authorization server: it hands out a code, remembers the PKCE
/// challenge that code was requested with, and exchanges the code only for the
/// verifier that challenge came from.
#[derive(Clone)]
struct Idp {
    /// code → the `code_challenge` the authorization request carried.
    issued: Arc<Mutex<HashMap<String, String>>>,
    /// The ID token to hand back on a successful exchange.
    id_token: Arc<String>,
}

#[derive(serde::Deserialize)]
struct AuthorizeQuery {
    redirect_uri: String,
    state: String,
    code_challenge: String,
}

async fn authorize(State(idp): State<Idp>, Query(q): Query<AuthorizeQuery>) -> Response {
    let code = format!("code-{}", q.state);
    idp.issued
        .lock()
        .unwrap()
        .insert(code.clone(), q.code_challenge);
    Redirect::to(&format!("{}?code={code}&state={}", q.redirect_uri, q.state)).into_response()
}

async fn token(State(idp): State<Idp>, body: String) -> Response {
    let form: HashMap<String, String> = body
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect();

    let code = form.get("code").cloned().unwrap_or_default();
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    let Some(challenge) = idp.issued.lock().unwrap().remove(&code) else {
        return (axum::http::StatusCode::BAD_REQUEST, "unknown code").into_response();
    };
    // The whole point of PKCE: the code is worthless without the verifier the
    // challenge was derived from.
    if challenge != s256(&verifier) {
        return (axum::http::StatusCode::BAD_REQUEST, "bad verifier").into_response();
    }

    Json(json!({
        "access_token": "unused-by-the-listing",
        "token_type": "Bearer",
        "id_token": idp.id_token.as_str(),
    }))
    .into_response()
}

fn s256(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}

async fn spawn_idp(id_token: String) -> (SocketAddr, IssuerEndpoints) {
    let idp = Idp {
        issued: Arc::new(Mutex::new(HashMap::new())),
        id_token: Arc::new(id_token),
    };
    let app = Router::new()
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .with_state(idp);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        addr,
        IssuerEndpoints {
            authorization_endpoint: format!("http://{addr}/authorize"),
            token_endpoint: format!("http://{addr}/token"),
        },
    )
}

// --- the server under test -------------------------------------------------

/// Two roots and one child under the first, so a page can be wrong in a way a
/// single entity would hide.
async fn seeded_memory() -> Arc<dyn Memory> {
    let store: Arc<dyn Memory> = Arc::new(InMemoryMemory::new());
    for new in [
        NewEntity::new(
            EntityId::new(EntityKind::Person, "alpha"),
            "Alpha",
            "the fixture roster",
        ),
        NewEntity::new(
            EntityId::new(EntityKind::Place, "shelbyville"),
            "Shelbyville",
            "the fixture roster",
        ),
    ] {
        store.add_entity(new).await.expect("the roster is written");
    }
    let mut child = NewEntity::new(
        EntityId::new(EntityKind::Topic, "widgets"),
        "Widgets",
        "the fixture roster",
    );
    child.parent = Some(EntityId::new(EntityKind::Person, "alpha"));
    store.add_entity(child).await.expect("the child is written");
    store
}

async fn spawn_jojobot(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let store = seeded_memory().await;
    let indexed = Arc::new(IndexedMemory::new(store).expect("the search index opens"));
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));

    let ui_cfg = UiConfig {
        client_id: CLIENT_ID.to_string(),
        client_secret: None,
        base_url: format!("http://{addr}"),
    };
    let ui = Ui::new(
        &ui_cfg,
        endpoints,
        idp.validator_for(CLIENT_ID, allowed),
        reqwest::Client::new(),
    );

    let state = AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: Some(support::ISS.to_string()),
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search,
        mailboxes: Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ) as Arc<dyn Mailboxes>,
        sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new())
            as Arc<dyn Sessions>,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: Some(Arc::new(ui)),
    };

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
            .unwrap();
    });
    (addr, ct)
}

/// A browser: it does not follow redirects on its own here, so every hop is
/// asserted rather than assumed.
fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap_or_else(|| panic!("no Location on a {} response", response.status()))
        .to_str()
        .unwrap()
        .to_string()
}

fn query_of(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

/// Walk the whole login: the gate turns the browser away, the login sends it to
/// the issuer, the issuer sends it back with a code, and the callback opens a
/// session. Returns the session cookie.
async fn log_in(client: &reqwest::Client, jojobot: SocketAddr, from: &str) -> String {
    let turned_away = client
        .get(format!("http://{jojobot}{from}"))
        .send()
        .await
        .unwrap();
    assert!(
        turned_away.status().is_redirection(),
        "a browser with no session must be sent to the login, got {}",
        turned_away.status()
    );
    let to_login = location(&turned_away);

    let sent_out = client
        .get(format!("http://{jojobot}{to_login}"))
        .send()
        .await
        .unwrap();
    let at_issuer = location(&sent_out);

    let back = client.get(&at_issuer).send().await.unwrap();
    let callback = location(&back);

    let opened = client.get(&callback).send().await.unwrap();
    assert!(
        opened.status().is_redirection(),
        "a completed login must land the browser back at the page it wanted, got {}",
        opened.status()
    );
    opened
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("a completed login sets a session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

// --- the tests -------------------------------------------------------------

#[tokio::test]
async fn a_browser_with_no_session_is_sent_to_the_login() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[], &idp).await;

    let response = browser()
        .get(format!("http://{addr}/"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_redirection());
    let to = location(&response);
    assert!(
        to.starts_with("/ui/login"),
        "the gate must send a person to the login, not refuse them: {to}"
    );
    assert!(
        to.contains("next="),
        "the login must be told where the browser was going: {to}"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_login_sends_the_browser_to_the_issuer_with_a_pkce_challenge() {
    let idp = support::TestIdp::new();
    let (idp_addr, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[], &idp).await;

    let response = browser()
        .get(format!("http://{addr}/ui/login?next=%2F"))
        .send()
        .await
        .unwrap();

    let to = location(&response);
    assert!(
        to.starts_with(&format!("http://{idp_addr}/authorize?")),
        "the login must go to the issuer's authorization endpoint: {to}"
    );
    let query = query_of(&to);
    assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
    assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(
        query.get("redirect_uri").map(String::as_str),
        Some(format!("http://{addr}/ui/callback").as_str())
    );
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256"),
        "a login without PKCE is a code anybody who intercepts it can redeem"
    );
    assert!(query.contains_key("code_challenge"));
    assert!(query.contains_key("state"));
    ct.cancel();
}

#[tokio::test]
async fn a_logged_in_browser_reads_the_index_of_the_roots() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let cookie = log_in(&client, addr, "/").await;

    let page = client
        .get(format!("http://{addr}/"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let body = page.text().await.unwrap();

    assert!(
        body.contains("href=\"/person:alpha/\""),
        "a root must be listed and be a link into its own index: {body}"
    );
    assert!(
        body.contains("href=\"/place:shelbyville/\""),
        "every root is listed, not just the first: {body}"
    );
    // The pairing that makes the two assertions above mean something: the page
    // is the index of the ROOTS, so a child is not on it.
    assert!(
        !body.contains("topic:widgets"),
        "a child belongs under its parent, not at the top: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_subject_off_the_allowlist_is_refused_and_gets_no_session() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for("sub-somebody-else", CLIENT_ID)).await;
    // The allowlist names the reader; the issuer will vouch for somebody else.
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let turned_away = client.get(format!("http://{addr}/")).send().await.unwrap();
    let sent_out = client
        .get(format!("http://{addr}{}", location(&turned_away)))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let refused = client.get(location(&back)).send().await.unwrap();

    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    assert!(
        refused.headers().get(reqwest::header::SET_COOKIE).is_none(),
        "a refused login must not open a session"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_callback_this_server_never_started_is_refused() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;

    let response = browser()
        .get(format!(
            "http://{addr}/ui/callback?code=made-up&state=made-up"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(
        response
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .is_none()
    );
    ct.cancel();
}

#[tokio::test]
async fn a_login_is_spent_once() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    // Walk the flow by hand so the callback URL is still in hand to replay.
    let turned_away = client.get(format!("http://{addr}/")).send().await.unwrap();
    let sent_out = client
        .get(format!("http://{addr}{}", location(&turned_away)))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let callback = location(&back);

    let first = client.get(&callback).send().await.unwrap();
    assert!(first.status().is_redirection(), "the first use logs in");

    let replayed = client.get(&callback).send().await.unwrap();
    assert_eq!(
        replayed.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a callback replayed with a spent state must not open a second session"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_login_will_not_land_a_browser_off_this_server() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let cookie = log_in(&client, addr, "/").await;
    let _ = cookie;

    // A `next` pointing off-site is what turns a real login into an open
    // redirector, so it is dropped rather than honoured.
    let sent_out = client
        .get(format!(
            "http://{addr}/ui/login?next=https%3A%2F%2Felsewhere.example%2F"
        ))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let landed = client.get(location(&back)).send().await.unwrap();

    assert_eq!(location(&landed), "/");
    ct.cancel();
}
