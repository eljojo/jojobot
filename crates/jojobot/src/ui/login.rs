//! The browser's way in: authorization code with PKCE, against the issuer that
//! already guards `/mcp`.
//!
//! Two handlers and no third. `/ui/login` sends the browser to the issuer;
//! `/ui/callback` takes the code back, exchanges it, and opens a session. Both
//! are public, because a gate over the way in has nowhere to send anybody.
//!
//! **What proves who is reading is the ID token**, verified by
//! [`crate::auth::Validator`] — the same signature, issuer and expiry checks the
//! resource server makes, bound to the client id an ID token is minted for. The
//! subject then faces the same allowlist. There is one place that decides who
//! may in this server, and this is not a second one.

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::AppState;
use crate::ui::{Ui, refuse};

/// `/ui/login` — start a login. `next` is where to land afterwards.
#[derive(Debug, Deserialize)]
pub struct Start {
    #[serde(default)]
    pub(crate) next: Option<String>,
}

/// `/ui/callback` — what the issuer sends back. Either a code and the state it
/// was asked with, or an error the issuer wants reported.
#[derive(Debug, Deserialize)]
pub struct Callback {
    #[serde(default)]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) state: Option<String>,
    #[serde(default)]
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) error_description: Option<String>,
}

/// The issuer's answer at the token endpoint. An access token may be there;
/// this flow does not use one, because the UI reads jojobot's own store rather
/// than calling `/mcp`.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: Option<String>,
}

/// Send the browser to the issuer.
pub async fn begin(State(state): State<AppState>, Query(start): Query<Start>) -> Response {
    let ui = state.ui.as_ref().expect("login mounted without a UI");
    // **Only a path of our own.** A `next` carrying a scheme or a host would
    // make this login an open redirector: an attacker sends somebody through a
    // real login and lands them somewhere else entirely.
    let next = start
        .next
        .as_deref()
        .filter(|n| is_local_path(n))
        .unwrap_or("/");
    Redirect::to(&ui.begin_login(next)).into_response()
}

/// Take the code back, exchange it, and open a session.
pub async fn finish(State(state): State<AppState>, Query(callback): Query<Callback>) -> Response {
    let ui = state.ui.as_ref().expect("callback mounted without a UI");

    if let Some(error) = callback.error {
        let detail = callback.error_description.unwrap_or_default();
        tracing::debug!(%error, %detail, "the issuer refused a UI login");
        return refuse(
            StatusCode::UNAUTHORIZED,
            "The login did not complete. Start again from the link you followed.",
        );
    }

    let (Some(code), Some(returned_state)) = (callback.code, callback.state) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "That is not a login this server started.",
        );
    };

    // The state is claimed once. A callback replayed with a state already spent
    // — or one this process never minted — finds nothing here.
    let Some(pending) = ui.claim_login(&returned_state) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "That is not a login this server started.",
        );
    };

    let id_token = match exchange(ui, &code, &pending.verifier).await {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!(error = %err, "a UI login could not be exchanged at the issuer");
            return refuse(
                StatusCode::BAD_GATEWAY,
                "The issuer could not be asked who you are. Try again.",
            );
        }
    };

    let claims = match ui
        .id_tokens
        .validate(&id_token)
        .and_then(|claims| ui.id_tokens.authorize(&claims).map(|()| claims))
    {
        Ok(claims) => claims,
        Err(err) => {
            tracing::debug!(%err, "rejected a UI login");
            return refuse(
                StatusCode::FORBIDDEN,
                "That account may not read this jojobot.",
            );
        }
    };

    tracing::info!(subject = %claims.sub, "a browser logged in to the listing");
    let cookie = ui.session_cookie(&ui.open_session());
    ([(header::SET_COOKIE, cookie)], Redirect::to(&pending.next)).into_response()
}

/// Exchange the authorization code for tokens, proving the login is the one
/// this process started by sending the PKCE verifier.
async fn exchange(ui: &Ui, code: &str, verifier: &str) -> anyhow::Result<String> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", ui.redirect_uri.as_str()),
        ("client_id", ui.client_id.as_str()),
        ("code_verifier", verifier),
    ];

    // A public client: nothing authenticates this request but the verifier,
    // which only the process that started the login holds.
    let response = ui
        .http
        .post(&ui.endpoints.token_endpoint)
        .form(&form)
        .send()
        .await?
        .error_for_status()?;
    let tokens: TokenResponse = response.json().await?;
    tokens.id_token.ok_or_else(|| {
        anyhow::anyhow!("the issuer returned no id_token, so there is nothing saying who logged in")
    })
}

/// Whether this is a path on this server and not somewhere else.
///
/// **A character class, not a list of bad prefixes.** What a redirect target
/// may contain is decided here positively: one leading slash and no second one,
/// then printable ASCII only, with the backslash a browser reads as a slash
/// left out. A prefix rule cannot hold this line — ASCII tab and newline are
/// removed by the URL parser *before* it parses, so `/<TAB>/elsewhere.example`
/// has the shape of a local path and the meaning of a protocol-relative one.
/// Anything a real path needs beyond this set arrives percent-encoded.
fn is_local_path(next: &str) -> bool {
    next.len() <= MAX_NEXT
        && next.starts_with('/')
        && !next.starts_with("//")
        && next.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
}

/// The longest `next` this login will carry. `/ui/login` is public and each hit
/// retains one verbatim until the login is claimed or times out, so what an
/// unauthenticated caller can park here is bounded. Long enough for any handle
/// path this server serves.
const MAX_NEXT: usize = 512;

#[cfg(test)]
mod tests {
    use super::{MAX_NEXT, is_local_path};

    /// The bytes as they arrive, not the shapes they resemble:
    /// `?next=/%09/elsewhere.example/` decodes to a real tab, and `HeaderValue`
    /// carries it. Step 2 of WHATWG URL parsing removes ASCII tab and newline
    /// *before* parsing, so a browser handed `Location: /<TAB>/elsewhere.example/`
    /// resolves `//elsewhere.example/` and leaves this origin — having just
    /// watched a genuine login succeed.
    #[test]
    fn a_next_carrying_a_stripped_byte_is_not_a_path_on_this_server() {
        for stripped in ["/\t/elsewhere.example/", "/\n/elsewhere.example/"] {
            assert!(
                !is_local_path(stripped),
                "{stripped:?} was accepted as a local path"
            );
        }
        // The positive these lean on: an ordinary path still round-trips, so a
        // green bar here is not a predicate that refuses everything.
        assert!(is_local_path("/person:alpha/"));
    }

    #[test]
    fn a_next_longer_than_a_path_is_not_carried() {
        let long = format!("/{}", "a".repeat(MAX_NEXT));
        assert!(
            !is_local_path(&long),
            "a next of {} bytes was accepted",
            long.len()
        );
        assert!(is_local_path(&format!("/{}", "a".repeat(MAX_NEXT - 1))));
    }

    #[test]
    fn only_a_path_on_this_server_is_followed_after_a_login() {
        assert!(is_local_path("/"));
        assert!(is_local_path("/person:alpha/"));
        // Every one of these leaves this origin, which is what makes an open
        // redirector out of a login.
        assert!(!is_local_path("//elsewhere.example/"));
        assert!(!is_local_path("https://elsewhere.example/"));
        assert!(!is_local_path("/\\elsewhere.example"));
        assert!(!is_local_path("person:alpha"));
    }
}
