//! The browser UI — **a directory listing over the entity graph**, the way a
//! web server indexes a directory. Read-only except for two mail actions.
//!
//! Entities form a tree and a handle is a path, so **a handle path is a URL
//! path**: `/` is the index of the roots and `/project:atlas/event:departure-flight/`
//! is that node. A page lists what is at its node and links to what is below
//! it. There is no application here and there is not meant to be one.
//!
//! **The pages read through the query API and nothing else.** This crate depends
//! on no store adapter and no MCP crate, and its lint config forbids naming the
//! store port, so a page that reaches for the store does not pass the lint. The
//! composition root builds every handle, owns the login gate, and mounts the
//! two routers below behind it.

pub mod document;
pub mod home;
pub mod mail;
pub mod org;
pub mod pages;
pub mod search;
pub mod shell;
pub mod tree;
pub mod typed;

use axum::{
    Router,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use jojobot_domain::clock::Clock;
use jojobot_query_api::{MailHandle, OperatorMailCommands, QueryHandle};

/// **Everything a page holds**, built by the composition root. The read handles
/// are the pages' one door to the graph; the last two fields are what the mail
/// actions check before they ask the command, as plain values.
#[derive(Clone)]
pub struct PageState {
    pub queries: QueryHandle,
    pub mail: MailHandle,
    pub commands: OperatorMailCommands,
    pub clock: Clock,
    /// Whether the allowlist admits exactly one person. A UI session carries no
    /// subject, so the operator's private mail is served only when the one
    /// person who can log in is the operator. An open allowlist counts as none.
    pub admits_exactly_one_person: bool,
    /// The origin this UI is served at: the scheme and the authority of the
    /// configured base URL, without its path, because that is all a browser's
    /// `Origin` header carries.
    pub origin: String,
}

/// **The listing**: the index, the org chart, a kind, search and a node. No gate
/// is on it; the root layers its login over the router it gets here. Generic in
/// the outer state, which the pages never read, so the root merges it with
/// routes that have one.
pub fn pages_router<S: Clone + Send + Sync + 'static>(state: PageState) -> Router<S> {
    Router::new()
        .route("/", get(pages::index))
        .route(org::PATH, get(org::chart))
        .route("/ui/kind/{kind}/", get(home::kind_page))
        .route(search::PATH, get(search::results))
        .route("/{*path}", get(pages::node))
        .with_state(state)
}

/// **The actions the UI has**, which the root layers behind a session of their
/// own gate.
pub fn actions_router<S: Clone + Send + Sync + 'static>(state: PageState) -> Router<S> {
    Router::new()
        .route("/ui/mail/open", post(mail::open))
        .route("/ui/mail/processed", post(mail::processed))
        .with_state(state)
}

/// **Whether a state-changing request came from this UI's own origin.**
///
/// The session cookie is `SameSite=Lax`, so a cross-site POST arrives without it;
/// this is the second check, for a client that sends the cookie anyway. The
/// `Origin` header must equal the configured origin exactly, or, when a client
/// sends none, the `Referer` must sit under it. A request with neither is
/// refused: a browser sends `Origin` on a POST, so a request without one is not a
/// browser acting for a person on this page.
pub fn same_origin(own: &str, headers: &HeaderMap) -> bool {
    let text = |name: header::HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(origin) = text(header::ORIGIN) {
        return origin == own;
    }
    text(header::REFERER).is_some_and(|referer| {
        referer
            .strip_prefix(own)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Percent-encode one query-string value. Everything outside the unreserved set
/// is escaped, so a redirect URI with a scheme and slashes survives the trip.
/// The one encoder of the UI: the login, the authorize URL and a view's address
/// ([`shell::view_url`]) all use it.
pub fn encode_component(value: &str) -> String {
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

    /// **No section of this crate's manifest names a crate that holds the store or
    /// the MCP surface**, dev edges included, so a page cannot reach either. The
    /// query API and the domain's value types are the only way in.
    #[test]
    fn the_manifest_names_no_store_crate() {
        let forbidden = ["jojobot", "jojobot-adapters", "jojobot-mcp", "sqlx"];
        let named: Vec<&str> = include_str!("../Cargo.toml")
            .lines()
            .filter(|line| !line.starts_with(['#', '[', ' ']))
            .filter_map(|line| line.split(['=', '.', ' ']).next())
            .filter(|name| forbidden.contains(name))
            .collect();
        assert!(named.is_empty(), "the manifest names {named:?}");
    }

    /// The `Origin` must equal the configured origin exactly; with none, the
    /// `Referer` must sit UNDER it, so a longer port or host that merely starts
    /// like the origin is another origin; with neither, the request is refused.
    #[test]
    fn a_request_is_ours_by_its_origin_or_else_by_a_referer_under_it() {
        let sent = |origin: Option<&str>, referer: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(origin) = origin {
                headers.insert(header::ORIGIN, origin.parse().unwrap());
            }
            if let Some(referer) = referer {
                headers.insert(header::REFERER, referer.parse().unwrap());
            }
            same_origin("https://jojobot.test", &headers)
        };
        assert!(sent(Some("https://jojobot.test"), None));
        assert!(!sent(Some("https://elsewhere.example"), None));
        assert!(!sent(Some("https://jojobot.test.elsewhere.example"), None));
        // An Origin that is present decides alone: a good Referer cannot rescue it.
        assert!(!sent(
            Some("https://elsewhere.example"),
            Some("https://jojobot.test/")
        ));
        assert!(sent(None, Some("https://jojobot.test")));
        assert!(sent(None, Some("https://jojobot.test/person:lisa/")));
        assert!(!sent(None, Some("https://jojobot.test:8443/person:lisa/")));
        assert!(!sent(
            None,
            Some("https://jojobot.test.elsewhere.example/person:lisa/")
        ));
        assert!(!sent(None, Some("https://elsewhere.example/")));
        assert!(!sent(None, None));
    }
}
