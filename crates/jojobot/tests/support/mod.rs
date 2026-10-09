//! Shared helpers for the integration crate: a throwaway RSA issuer that mints
//! validly-signed tokens and builds a matching [`Validator`]. The JWT/JWKS
//! plumbing mirrors `auth.rs`'s own `#[cfg(test)]` helpers, which the integration
//! crate can't reach. It lives here rather than behind a shipped constructor so
//! the no-toy-store / hexagonal discipline holds — nothing test-only leaks into
//! the library's public surface; these tests use only the real public API
//! (`Validator::from_keys` + `with_allowed_subjects`).

// Each integration binary compiles this module separately and reaches for a
// different part of it, so what one uses reads as dead to another.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use jojobot::auth::Validator;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, encode};
use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Serialize;

pub const ISS: &str = "https://issuer.test";
pub const AUD: &str = "https://resource.test/mcp";
const KID: &str = "test-key-1";

#[derive(Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: u64,
}

/// A throwaway RSA issuer. Holds the signing key and the public `n`/`e`
/// components a JWKS would publish, so the validator it builds decodes from the
/// same material production does.
#[derive(Clone)]
pub struct TestIdp {
    enc: EncodingKey,
    n: String,
    e: String,
}

/// **One key for every issuer a test binary makes.** Generating an RSA key
/// searches for two large primes, and in an unoptimised build that costs
/// seconds. A story makes an issuer of its own, so a binary of a hundred
/// stories spent most of its processor time on keys. Every issuer here is
/// built with the same name and key id, so a second key adds nothing: no case
/// tells one issuer's key from another's.
static SHARED_KEY: OnceLock<TestIdp> = OnceLock::new();

impl TestIdp {
    pub fn new() -> Self {
        SHARED_KEY.get_or_init(Self::generate).clone()
    }

    fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let pub_key = RsaPublicKey::from(&priv_key);

        let pem = priv_key.to_pkcs1_pem(LineEnding::LF).unwrap();
        let enc = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let n = b64.encode(pub_key.n().to_bytes_be());
        let e = b64.encode(pub_key.e().to_bytes_be());

        Self { enc, n, e }
    }

    /// A validator trusting this issuer's key, bound to `ISS`/`AUD`, carrying the
    /// given subject allowlist — the exact construction path `discover()` uses.
    pub fn validator(&self, allowed: &[&str]) -> Validator {
        self.validator_for(AUD, allowed)
    }

    /// The same validator bound to another audience — what an ID token carries,
    /// which is the client it was minted for rather than the resource.
    pub fn validator_for(&self, audience: &str, allowed: &[&str]) -> Validator {
        let decoding = DecodingKey::from_rsa_components(&self.n, &self.e).unwrap();
        let mut keys = HashMap::new();
        keys.insert(KID.to_string(), decoding);
        Validator::from_keys(ISS, audience, keys)
            .with_allowed_subjects(allowed.iter().map(|s| s.to_string()))
    }

    /// Mint a validly-signed RS256 token for the given subject id.
    pub fn token(&self, sub: &str) -> String {
        self.token_for(sub, AUD)
    }

    /// Mint a validly-signed RS256 token for a subject and an audience.
    pub fn token_for(&self, sub: &str, audience: &str) -> String {
        let claims = Claims {
            sub: sub.to_string(),
            iss: ISS.to_string(),
            aud: audience.to_string(),
            exp: now() + 3600,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(KID.to_string());
        encode(&header, &claims, &self.enc).unwrap()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// --- the browser's issuer, and the login round trip -------------------------
//
// **A real HTTP authorization server**, because a login that never left the
// process would prove the handlers and not the flow: it holds the PKCE
// challenge a code was requested with and refuses to exchange that code against
// any other verifier.
//
// It lives here rather than in one suite because two of them need it — the
// listing's own tests, and the stories, which read the operator's page the way
// a browser does.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use jojobot::auth::IssuerEndpoints;
use serde_json::json;

/// A minimal authorization server: it hands out a code, remembers the PKCE
/// challenge that code was requested with, and exchanges the code only for the
/// verifier that challenge came from.
#[derive(Clone)]
pub struct Idp {
    /// code → the `code_challenge` the authorization request carried.
    issued: Arc<Mutex<HashMap<String, String>>>,
    /// The ID token to hand back on a successful exchange.
    id_token: Arc<String>,
}

#[derive(serde::Deserialize)]
pub struct AuthorizeQuery {
    redirect_uri: String,
    state: String,
    code_challenge: String,
}

pub async fn authorize(State(idp): State<Idp>, Query(q): Query<AuthorizeQuery>) -> Response {
    let code = format!("code-{}", q.state);
    idp.issued
        .lock()
        .unwrap()
        .insert(code.clone(), q.code_challenge);
    Redirect::to(&format!("{}?code={code}&state={}", q.redirect_uri, q.state)).into_response()
}

pub async fn token(State(idp): State<Idp>, body: String) -> Response {
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

pub fn s256(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn decode(raw: &str) -> String {
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

pub async fn spawn_idp(id_token: String) -> (SocketAddr, IssuerEndpoints) {
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

/// A browser: it does not follow redirects on its own here, so every hop is
/// asserted rather than assumed.
pub fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

pub fn location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap_or_else(|| panic!("no Location on a {} response", response.status()))
        .to_str()
        .unwrap()
        .to_string()
}

pub fn query_of(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

/// What a browser sends back: the pair, without the attributes that govern
/// *when* it sends it. Those attributes are the subject of a test of their own,
/// so they are separated here rather than thrown away here.
pub fn session_pair(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("a cookie header has a first field")
        .to_string()
}

/// Walk the whole login and hand back the session cookie a browser would send.
pub async fn log_in(client: &reqwest::Client, jojobot: SocketAddr, from: &str) -> String {
    session_pair(&log_in_raw(client, jojobot, from).await)
}

/// Walk the whole login: the gate turns the browser away, the login sends it to
/// the issuer, the issuer sends it back with a code, and the callback opens a
/// session. Returns the `Set-Cookie` the server sent, **whole**.
pub async fn log_in_raw(client: &reqwest::Client, jojobot: SocketAddr, from: &str) -> String {
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
        .to_string()
}

// --- the handshake, as a client makes it -------------------------------------
//
// **Shared because two suites need the same door.** The protocol contract is
// asserted over raw HTTP — a typed client deserializes into its SDK's own
// structs, so a field the SDK does not know is a field no assertion could see
// — and a story about a client that cannot connect opens the same way.
/// Open a connection asking for one revision. Answers the session id, if the
/// agreed revision has sessions at all, and the body.
pub async fn open_with(
    http: &reqwest::Client,
    url: &str,
    revision: &str,
) -> (Option<String>, String) {
    let opened = http
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{revision}","capabilities":{{}},"clientInfo":{{"name":"jojobot-test-client","version":"0.0.1"}}}}}}"#
        ))
        .send()
        .await
        .unwrap();
    let session = opened
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_string());
    (session, opened.text().await.unwrap())
}

/// **What a request carries from the 2026-07-28 revision on** (SEP-2575): its
/// own version, identity and capabilities, so it stands alone whether or not a
/// handshake happened. jojobot refuses a request on that revision without them,
/// so a test client that left them out would prove the revision unserved rather
/// than served.
///
/// The method, and the verb a call names, ride as headers instead (SEP-2243) so
/// a middle box can route without reading the body.
pub fn request_meta(revision: &str) -> serde_json::Value {
    serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": revision,
        "io.modelcontextprotocol/clientInfo": {
            "name": "jojobot-test-client",
            "version": "0.0.1",
        },
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// **The body a call answered with, from wherever the agreed revision puts
/// it.** From `2026-07-28` on it is `structuredContent`, an object the caller
/// reads directly; below that it is a text block holding the same JSON as a
/// string. A caller that took the revision jojobot offered rather than choosing
/// one reads whichever arrived.
pub fn tool_body(result: &serde_json::Value) -> serde_json::Value {
    if result["structuredContent"].is_object() {
        return result["structuredContent"].clone();
    }
    let text = result["content"][0]["text"]
        .as_str()
        .expect("a call answers with a body");
    serde_json::from_str(text).expect("the body is JSON")
}

/// The JSON-RPC error carried by an event stream.
pub fn event_stream_error(body: &str) -> serde_json::Value {
    let message = event_stream_message(body);
    assert!(
        !message["error"].is_null(),
        "this call was expected to be refused: {message}",
    );
    message["error"].clone()
}

/// The JSON-RPC result carried by an event stream, which is how this transport
/// answers a POST.
pub fn event_stream_result(body: &str) -> serde_json::Value {
    let message = event_stream_message(body);
    assert!(
        message["error"].is_null(),
        "the call was refused: {}",
        message["error"],
    );
    message["result"].clone()
}

/// The one JSON-RPC message an event stream carries in answer to a POST.
pub fn event_stream_message(body: &str) -> serde_json::Value {
    // Either shape of answer this transport gives: an event stream, whose first
    // event is a priming one carrying no data, or one plain JSON body.
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .chain(std::iter::once(body))
        .find_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .unwrap_or_else(|| panic!("one JSON-RPC message, streamed or plain: {body}"))
}

// --- a fake ntfy server -------------------------------------------------------
//
// **A real HTTP server that can be taken down**, because a push that never left
// the process would prove the policy and not the call: the call is a JSON POST
// to the server's root with an Authorization header, and a server that is down
// answers a 503, which is the answer ntfy gives when its host is up and the
// service is not.

/// One publish the server accepted.
#[derive(Clone, Debug)]
pub struct Published {
    /// The `Authorization` header, as sent.
    pub authorization: Option<String>,
    /// The JSON body, as sent.
    pub body: serde_json::Value,
}

struct NtfyState {
    down: std::sync::atomic::AtomicBool,
    /// The status a file is refused with, or 0 when files are taken. A server
    /// with no attachment cache answers 400, one with a size limit 413, and one
    /// whose storage is used up 507.
    refuses_files_with: std::sync::atomic::AtomicU16,
    attempts: std::sync::atomic::AtomicUsize,
    accepted: Mutex<Vec<Published>>,
}

/// A running fake ntfy server.
pub struct FakeNtfy {
    /// Its address, scheme included, with no trailing slash.
    pub url: String,
    state: Arc<NtfyState>,
}

impl FakeNtfy {
    /// A server that is up.
    pub async fn start() -> Self {
        let state = Arc::new(NtfyState {
            down: std::sync::atomic::AtomicBool::new(false),
            refuses_files_with: std::sync::atomic::AtomicU16::new(0),
            attempts: std::sync::atomic::AtomicUsize::new(0),
            accepted: Mutex::new(Vec::new()),
        });
        async fn publish(
            axum::extract::State(state): axum::extract::State<Arc<NtfyState>>,
            headers: axum::http::HeaderMap,
            body: axum::body::Bytes,
        ) -> axum::http::StatusCode {
            use std::sync::atomic::Ordering;
            state.attempts.fetch_add(1, Ordering::SeqCst);
            if state.down.load(Ordering::SeqCst) {
                return axum::http::StatusCode::SERVICE_UNAVAILABLE;
            }
            let Ok(body) = serde_json::from_slice(&body) else {
                return axum::http::StatusCode::BAD_REQUEST;
            };
            state.accepted.lock().unwrap().push(Published {
                authorization: headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
                body,
            });
            axum::http::StatusCode::OK
        }
        // **A body too long for a message is sent as a file**, to the topic's
        // own address, with the title and the notification text in headers.
        async fn upload(
            axum::extract::State(state): axum::extract::State<Arc<NtfyState>>,
            axum::extract::Path(topic): axum::extract::Path<String>,
            headers: axum::http::HeaderMap,
            body: axum::body::Bytes,
        ) -> axum::http::StatusCode {
            use std::sync::atomic::Ordering;
            state.attempts.fetch_add(1, Ordering::SeqCst);
            if state.down.load(Ordering::SeqCst) {
                return axum::http::StatusCode::SERVICE_UNAVAILABLE;
            }
            let refusal = state.refuses_files_with.load(Ordering::SeqCst);
            if refusal != 0 {
                return axum::http::StatusCode::from_u16(refusal).unwrap();
            }
            let header = |name: &str| {
                headers
                    .get(name)
                    .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
            };
            state.accepted.lock().unwrap().push(Published {
                authorization: header("authorization"),
                body: serde_json::json!({
                    "attachment": true,
                    "topic": topic,
                    "title": header("title"),
                    "filename": header("filename"),
                    "message": header("message"),
                    "text": String::from_utf8_lossy(&body),
                }),
            });
            axum::http::StatusCode::OK
        }
        let app = Router::new()
            .route("/", post(publish))
            .route("/{topic}", axum::routing::put(upload))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        FakeNtfy {
            url: format!("http://{addr}"),
            state,
        }
    }

    /// Answer every publish with a 503 from here.
    pub fn go_down(&self) {
        self.state
            .down
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Refuse every file with `status` from here, as a server that will not take
    /// attachments does. Text still goes through.
    pub fn refuse_files_with(&self, status: u16) {
        self.state
            .refuses_files_with
            .store(status, std::sync::atomic::Ordering::SeqCst);
    }

    /// Accept publishes again.
    pub fn come_up(&self) {
        self.state
            .down
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// How many publishes were attempted, accepted or not.
    pub fn attempts(&self) -> usize {
        self.state
            .attempts
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Every publish the server accepted, in order.
    pub fn published(&self) -> Vec<Published> {
        self.state.accepted.lock().unwrap().clone()
    }
}
