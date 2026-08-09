//! The browser UI — **a directory listing over the entity graph**, the way a
//! web server indexes a directory. Read-only: nothing here writes.
//!
//! Entities form a tree and a handle is a path, so **a handle path is a URL
//! path**: `/` is the index of the roots and `/project:atlas/event:departure-flight/`
//! is that node. A page lists what is at its node and links to what is below
//! it. There is no application here and there is not meant to be one.
//!
//! **Getting in is the same login as everything else.** `/mcp` is a protected
//! resource: it verifies a bearer token some other client obtained. A browser
//! holds no token, so this module makes jojobot a *client* of the same issuer —
//! authorization code with PKCE — and then keeps a session of its own. The
//! token validation is [`crate::auth`]'s, unchanged, bound to the ID token's
//! audience instead of the resource's.
//!
//! **There is no development bypass.** The UI is mounted only when it is
//! configured, and it cannot be configured without an issuer
//! ([`crate::config`]).

pub mod login;
pub mod pages;
pub mod tree;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use jojobot_domain::handle::draw;

use crate::AppState;
use crate::auth::{IssuerEndpoints, Validator};
use crate::config::UiConfig;

/// The cookie naming a logged-in browser. Its value is a drawn handle and
/// nothing else: the session itself lives in this process, so the cookie can be
/// opaque and carries no claim anybody could rewrite.
pub const SESSION_COOKIE: &str = "jojobot_ui";

/// How long a logged-in browser stays logged in. A working day, so reading the
/// graph does not turn into logging in repeatedly.
const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// How long a login may sit half-finished between the redirect out and the
/// browser coming back. Short, because it is one round trip through a login
/// form and an abandoned one is exactly what an attacker replays.
const LOGIN_TTL: Duration = Duration::from_secs(10 * 60);

/// How many half-finished logins are held at once. `/ui/login` is public, and
/// age alone does not bound a table an unauthenticated caller can fill faster
/// than the TTL empties it. A personal instance logs in from a handful of
/// browsers, so the cap is far above real use and far below memory pressure.
///
/// **It is a cap that evicts, never one that refuses.** Filling it costs an
/// unauthenticated caller nothing, so refusing at the brim would trade bounded
/// memory for a lock-out on the only door — the cheaper attack of the two. A
/// dropped half-finished login costs its owner one retry; a refused one costs
/// the operator the way in.
const MAX_PENDING: usize = 64;

/// A login that has been sent to the issuer and not yet come back.
struct Pending {
    /// The PKCE verifier this login's challenge was derived from. It never
    /// leaves this process until the code is exchanged, which is what stops a
    /// stolen code being redeemed by anybody else.
    verifier: String,
    /// Where the browser was going when the gate turned it away.
    next: String,
    started: Instant,
}

/// A browser that has logged in. **Liveness and nothing else** — who it is was
/// settled at the callback, against the same allowlist `/mcp` uses, and the
/// listing is read-only, so no page downstream has a subject to ask for.
struct Live {
    /// When the session was opened. Sessions expire rather than refresh, so a
    /// browser left open overnight logs in again in the morning.
    opened: Instant,
}

/// The UI's OAuth client and its browser sessions.
pub struct Ui {
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    endpoints: IssuerEndpoints,
    /// Validates the ID token the issuer returns, and authorizes its subject
    /// against the same allowlist `/mcp` uses.
    id_tokens: Validator,
    http: reqwest::Client,
    /// Whether the cookie is marked `Secure`. Taken from the configured origin,
    /// so a UI served over https never hands its cookie to a plain-http request.
    secure_cookie: bool,
    pending: Mutex<HashMap<String, Pending>>,
    browsers: Mutex<HashMap<String, Live>>,
}

impl Ui {
    /// Assemble the client from resolved configuration, the issuer's endpoints
    /// and a validator already bound to the ID token's audience.
    pub fn new(
        cfg: &UiConfig,
        endpoints: IssuerEndpoints,
        id_tokens: Validator,
        http: reqwest::Client,
    ) -> Self {
        Self {
            client_id: cfg.client_id.clone(),
            client_secret: cfg.client_secret.clone(),
            redirect_uri: cfg.redirect_uri(),
            endpoints,
            id_tokens,
            http,
            secure_cookie: cfg.base_url.starts_with("https://"),
            pending: Mutex::new(HashMap::new()),
            browsers: Mutex::new(HashMap::new()),
        }
    }

    /// Start a login: draw a state and a PKCE verifier, remember where the
    /// browser was going, and return the URL to send it to. **A login always
    /// starts** — a full table makes room by dropping the login nearest expiry
    /// rather than turning this one away ([`MAX_PENDING`]).
    fn begin_login(&self, next: &str) -> String {
        let state = draw(24);
        let verifier = draw(64);
        let challenge = pkce_challenge(&verifier);

        {
            let mut pending = self
                .pending
                .lock()
                .expect("the pending map is not poisoned");
            pending.retain(|_, p| p.started.elapsed() < LOGIN_TTL);
            // **Oldest by `started`** — the one the TTL was about to take
            // anyway, so eviction only ever runs the clock forward on the login
            // least likely to still have a browser waiting on it.
            while pending.len() >= MAX_PENDING {
                let Some(oldest) = pending
                    .iter()
                    .min_by_key(|(_, p)| p.started)
                    .map(|(state, _)| state.clone())
                else {
                    break;
                };
                pending.remove(&oldest);
            }
            pending.insert(
                state.clone(),
                Pending {
                    verifier,
                    next: next.to_string(),
                    started: Instant::now(),
                },
            );
        }

        let mut url = format!("{}?", self.endpoints.authorization_endpoint);
        for (key, value) in [
            ("response_type", "code"),
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("scope", "openid"),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ] {
            url.push_str(&format!("{key}={}&", encode_component(value)));
        }
        url.pop();
        url
    }

    /// Take back the login this `state` began — **once**. A state is removed as
    /// it is claimed, so a callback replayed with the same one finds nothing and
    /// is refused rather than opening a second session.
    fn claim_login(&self, state: &str) -> Option<Pending> {
        let mut pending = self
            .pending
            .lock()
            .expect("the pending map is not poisoned");
        pending.retain(|_, p| p.started.elapsed() < LOGIN_TTL);
        pending.remove(state)
    }

    /// Open a session for a subject the issuer vouched for, and return the
    /// cookie value that addresses it.
    fn open_session(&self) -> String {
        let token = draw(32);
        let mut browsers = self
            .browsers
            .lock()
            .expect("the session map is not poisoned");
        browsers.retain(|_, live| live.opened.elapsed() < SESSION_TTL);
        browsers.insert(
            token.clone(),
            Live {
                opened: Instant::now(),
            },
        );
        token
    }

    /// Whether this cookie value still addresses a logged-in browser.
    fn is_live(&self, token: &str) -> bool {
        let mut browsers = self
            .browsers
            .lock()
            .expect("the session map is not poisoned");
        browsers.retain(|_, live| live.opened.elapsed() < SESSION_TTL);
        browsers.contains_key(token)
    }

    /// The `Set-Cookie` value for a freshly opened session.
    fn session_cookie(&self, token: &str) -> String {
        let mut cookie = format!(
            "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            SESSION_TTL.as_secs()
        );
        if self.secure_cookie {
            cookie.push_str("; Secure");
        }
        cookie
    }
}

/// The PKCE `S256` challenge for a verifier: base64url, no padding, of its
/// SHA-256 (RFC 7636 §4.2).
fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// Percent-encode one query-string value. Everything outside the unreserved set
/// is escaped, so a redirect URI with a scheme and slashes survives the trip.
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The value of one cookie in a `Cookie` header.
fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').map(str::trim).find_map(|pair| {
        pair.split_once('=')
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value)
    })
}

/// Require a logged-in browser. Mounted only on the listing, never on the login
/// itself — a gate over the way in has nowhere to send anybody.
///
/// A browser with no session is **sent to the login**, not refused: it is a
/// person following a link, and a 401 in a browser is a dead end. The path it
/// was going to rides along so the login lands it where it meant to be.
pub async fn require_browser(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let ui = state
        .ui
        .as_ref()
        .expect("require_browser mounted without a UI");

    // **A path that could never name an entity is missing, login or no login.**
    // The listing is mounted on a catch-all, so every path this server does not
    // implement arrives here — and answering those with "go and log in" names
    // the wrong problem to a client probing for an endpoint (rule 68). Handle
    // grammar is public, so refusing on shape leaks nothing; whether anything
    // is filed at a well-formed path stays behind the gate.
    let path = req.uri().path();
    if path != "/" && tree::segments(path).is_none() {
        return pages::not_found();
    }

    let logged_in = req
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| cookie_value(header, SESSION_COOKIE))
        .is_some_and(|token| ui.is_live(token));

    if logged_in {
        return next.run(req).await;
    }

    let wanted = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    Redirect::to(&format!("/ui/login?next={}", encode_component(wanted))).into_response()
}

/// A refusal a person reads, rather than a JSON body a program would.
pub fn refuse(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        pages::plain_page("jojobot", message),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The challenge is the RFC 7636 §4.2 worked example, read from the RFC
    /// rather than written from what the transform ought to do.
    #[test]
    fn the_pkce_challenge_matches_the_rfc_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_redirect_uri_survives_being_a_query_value() {
        assert_eq!(
            encode_component("https://jojobot.test/ui/callback"),
            "https%3A%2F%2Fjojobot.test%2Fui%2Fcallback"
        );
    }

    /// A UI whose issuer is never reached — every assertion below stops at the
    /// pending table, which is filled before a browser goes anywhere.
    fn a_ui() -> Ui {
        Ui::new(
            &UiConfig {
                client_id: "jojobot-ui".to_string(),
                client_secret: None,
                base_url: "https://jojobot.test".to_string(),
            },
            IssuerEndpoints {
                authorization_endpoint: "https://issuer.test/authorize".to_string(),
                token_endpoint: "https://issuer.test/token".to_string(),
            },
            Validator::from_keys("https://issuer.test", "jojobot-ui", HashMap::new()),
            reqwest::Client::new(),
        )
    }

    fn state_of(url: &str) -> String {
        url.split('&')
            .find_map(|pair| pair.strip_prefix("state="))
            .expect("a login URL carries its state")
            .to_string()
    }

    /// `/ui/login` is unauthenticated, so the only thing standing between it and
    /// an unbounded map is a cap on how many half-finished logins are held. The
    /// TTL does not do it: ten minutes is long enough to insert a great many.
    ///
    /// **But the cap may never cost anybody the way in.** Filling the table is
    /// cheap and takes no credentials, so a cap that refuses hands an
    /// unauthenticated caller a lock-out on the only door — worse than the
    /// memory it bounds. That half is [`Ui::begin_login`]'s return type: it
    /// hands back a URL and has no way to say no. What is left for a test is
    /// the other half — one login past the cap, and the table still does not
    /// grow.
    #[test]
    fn a_full_table_evicts_rather_than_refusing_a_login() {
        let ui = a_ui();
        for _ in 0..=MAX_PENDING {
            let _ = ui.begin_login("/");
        }

        assert_eq!(
            ui.pending.lock().unwrap().len(),
            MAX_PENDING,
            "the table grew past the cap of {MAX_PENDING}"
        );
    }

    /// Which one goes matters: the honest choice is the login closest to
    /// expiring anyway. A flood cannot then push out a login started *during*
    /// it, which is the one a person is most likely still sitting in front of.
    #[test]
    fn the_login_a_full_table_evicts_is_the_oldest() {
        let ui = a_ui();
        let before = state_of(&ui.begin_login("/"));
        // `started` is what "oldest" reads, so put this login unambiguously
        // behind everything the flood inserts.
        std::thread::sleep(Duration::from_millis(2));

        let survivor = state_of(&ui.begin_login("/"));
        // The rest of the flood: how far the table fills is the other test's
        // business, so nothing is asserted about these.
        for _ in 2..=MAX_PENDING {
            let _ = ui.begin_login("/");
        }

        assert!(
            ui.claim_login(&before).is_none(),
            "the oldest half-finished login survived the flood, so something \
             younger was evicted instead"
        );
        assert!(
            ui.claim_login(&survivor).is_some(),
            "a login started during the flood was evicted before the older one"
        );
    }

    #[test]
    fn a_cookie_is_read_by_name_and_not_by_prefix() {
        let header = "other=1; jojobot_ui_extra=zzz; jojobot_ui=abc";
        assert_eq!(cookie_value(header, SESSION_COOKIE), Some("abc"));
        assert_eq!(cookie_value("nothing=1", SESSION_COOKIE), None);
    }
}
