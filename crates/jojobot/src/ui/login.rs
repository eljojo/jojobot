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
    pub next: Option<String>,
}

/// `/ui/callback` — what the issuer sends back. Either a code and the state it
/// was asked with, or an error the issuer wants reported.
#[derive(Debug, Deserialize)]
pub struct Callback {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
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
    let cookie = ui.session_cookie(&ui.open_session(claims.sub));
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

    let mut request = ui.http.post(&ui.endpoints.token_endpoint).form(&form);
    // `client_secret_basic` — the OIDC default for a confidential client. A
    // public client sends nothing, and PKCE is what stands in its place.
    if let Some(secret) = &ui.client_secret {
        request = request.basic_auth(&ui.client_id, Some(secret));
    }

    let response = request.send().await?.error_for_status()?;
    let tokens: TokenResponse = response.json().await?;
    tokens.id_token.ok_or_else(|| {
        anyhow::anyhow!("the issuer returned no id_token, so there is nothing saying who logged in")
    })
}

/// Whether this is a path on this server and not somewhere else. A path, one
/// leading slash, and no second one — `//elsewhere.example` is a URL a browser
/// follows off-site.
fn is_local_path(next: &str) -> bool {
    next.starts_with('/') && !next.starts_with("//") && !next.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::is_local_path;

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
