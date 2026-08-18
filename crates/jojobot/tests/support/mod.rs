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
pub struct TestIdp {
    enc: EncodingKey,
    n: String,
    e: String,
}

impl TestIdp {
    pub fn new() -> Self {
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
// he reads it.

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
