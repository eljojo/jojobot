//! Runtime configuration. Everything instance-specific enters here from the
//! environment — the binary compiles no issuer, audience, or hostname.

use std::net::SocketAddr;

/// OIDC resource-server settings. Present only when auth is enabled.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// The OIDC issuer (authorization server). Its discovery document yields
    /// the JWKS. Example shape: `https://id.example.org`.
    pub issuer: String,
    /// The audience this server requires in every token (RFC 8707). Tokens
    /// minted for a different resource are rejected.
    pub audience: String,
    /// Optional explicit JWKS URI. When absent it is discovered from the issuer.
    pub jwks_uri: Option<String>,
    /// Per-user authorization allowlist of subject ids (OIDC `sub` claim values,
    /// e.g. Pocket ID user ids). Empty means no allowlist is configured — any
    /// validated token passes, preserving the authentication-only behaviour.
    pub allowed_subjects: Vec<String>,
}

/// Full server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// This server's resource identifier, advertised in the RFC 9728
    /// protected-resource metadata.
    pub resource: String,
    /// `None` means auth is disabled — a dev-only mode that logs loudly.
    pub auth: Option<AuthConfig>,
}

impl Config {
    /// Build configuration from the environment.
    ///
    /// - `JOJOBOT_BIND` — listen address (default `127.0.0.1:8080`).
    /// - `JOJOBOT_RESOURCE` — this server's resource id (default derived from bind).
    /// - `JOJOBOT_ISSUER` — set to enable auth (OIDC issuer URL).
    /// - `JOJOBOT_AUDIENCE` — required audience (default: the resource id).
    /// - `JOJOBOT_JWKS_URI` — optional JWKS override.
    /// - `JOJOBOT_ALLOWED_SUBJECTS` — optional comma-separated `sub` allowlist.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::build(RawEnv::from_env())
    }

    /// Validate and assemble configuration from already-read env values. Pure —
    /// no environment access — so the fail-closed rules are unit-testable.
    fn build(raw: RawEnv) -> anyhow::Result<Self> {
        let bind: SocketAddr = raw.bind.parse().map_err(|e| {
            anyhow::anyhow!("JOJOBOT_BIND ({:?}) is not a valid address: {e}", raw.bind)
        })?;

        let resource = raw
            .resource
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("http://{bind}/mcp"));

        let allowlist_set = raw
            .allowed_subjects
            .as_deref()
            .is_some_and(|s| !s.is_empty());

        let auth = match raw.issuer.filter(|s| !s.is_empty()) {
            Some(issuer) => Some(AuthConfig {
                issuer,
                audience: raw
                    .audience
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| resource.clone()),
                jwks_uri: raw.jwks_uri.filter(|s| !s.is_empty()),
                allowed_subjects: parse_allowed_subjects(raw.allowed_subjects.as_deref())?,
            }),
            None => None,
        };

        // An allowlist without an issuer can never be enforced (there are no
        // validated tokens to check). Fail loud rather than silently drop it, so
        // an operator who set it isn't left believing they're protected.
        if auth.is_none() && allowlist_set {
            anyhow::bail!(
                "JOJOBOT_ALLOWED_SUBJECTS is set but JOJOBOT_ISSUER is not — an allowlist only \
                 takes effect with authentication enabled. Set JOJOBOT_ISSUER, or unset \
                 JOJOBOT_ALLOWED_SUBJECTS."
            );
        }

        // Fail closed: never infer "auth disabled" from a missing issuer alone.
        if auth.is_none() {
            if !raw.allow_no_auth {
                anyhow::bail!(
                    "refusing to start without authentication: set JOJOBOT_ISSUER, or set \
                     JOJOBOT_ALLOW_NO_AUTH=1 to run open on localhost for development"
                );
            }
            if !bind.ip().is_loopback() {
                anyhow::bail!(
                    "refusing to serve unauthenticated on non-loopback address {bind}: bind to \
                     localhost, or set JOJOBOT_ISSUER to enable authentication"
                );
            }
        }

        Ok(Config {
            bind,
            resource,
            auth,
        })
    }
}

/// Raw environment values, read once. Kept separate from [`Config::build`] so
/// the validation logic can be tested without mutating process env.
struct RawEnv {
    bind: String,
    resource: Option<String>,
    issuer: Option<String>,
    audience: Option<String>,
    jwks_uri: Option<String>,
    allowed_subjects: Option<String>,
    allow_no_auth: bool,
}

impl RawEnv {
    fn from_env() -> Self {
        RawEnv {
            bind: std::env::var("JOJOBOT_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            resource: std::env::var("JOJOBOT_RESOURCE").ok(),
            issuer: std::env::var("JOJOBOT_ISSUER").ok(),
            audience: std::env::var("JOJOBOT_AUDIENCE").ok(),
            jwks_uri: std::env::var("JOJOBOT_JWKS_URI").ok(),
            allowed_subjects: std::env::var("JOJOBOT_ALLOWED_SUBJECTS").ok(),
            allow_no_auth: std::env::var("JOJOBOT_ALLOW_NO_AUTH")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}

/// Parse the comma-separated `JOJOBOT_ALLOWED_SUBJECTS` value into a list,
/// trimming each entry and dropping empties. `sub` values are compared exactly
/// at authorization time; no case-folding here.
///
/// Distinguishes *unset* from *set-but-empty*, which decide opposite policies:
/// - unset (absent, or literal empty string) → empty list → the deliberate
///   "no allowlist, allow any authenticated user" case;
/// - a non-empty value that parses to zero valid entries (`",,"`, `" "`) → error,
///   so a fat-fingered secret refuses to start rather than serving wide-open.
fn parse_allowed_subjects(raw: Option<&str>) -> anyhow::Result<Vec<String>> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    let subjects: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect();
    if subjects.is_empty() {
        anyhow::bail!(
            "JOJOBOT_ALLOWED_SUBJECTS is set ({raw:?}) but lists no valid subject; refusing to \
             start rather than serve wide-open. Unset it to allow any authenticated user, or \
             list at least one subject id."
        );
    }
    Ok(subjects)
}

/// The scheme-and-authority origin of a URL (`https://host:port`), without any
/// path. Used to place the well-known metadata endpoint on the same origin as
/// the resource. Falls back to returning the input unchanged if it has no
/// recognizable scheme.
pub fn origin_of(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    format!("{scheme}://{authority}")
}

/// The authority (`host` or `host:port`) of a URL — scheme and path stripped.
/// Used to allow the public `Host` header through the transport's DNS-rebinding
/// guard. Returns `None` when the input has no recognizable scheme/authority.
pub fn authority_of(url: &str) -> Option<String> {
    let (_scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.is_empty() {
        None
    } else {
        Some(authority.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, RawEnv, authority_of, origin_of, parse_allowed_subjects};

    fn raw_subjects(subjects: Option<&str>) -> RawEnv {
        RawEnv {
            bind: "127.0.0.1:8080".to_string(),
            resource: None,
            issuer: Some("https://issuer.example".to_string()),
            audience: None,
            jwks_uri: None,
            allowed_subjects: subjects.map(str::to_string),
            allow_no_auth: false,
        }
    }

    #[test]
    fn allowed_subjects_parse_contract() {
        // Unset, and literal empty string, both mean "no allowlist".
        assert!(parse_allowed_subjects(None).unwrap().is_empty());
        assert!(parse_allowed_subjects(Some("")).unwrap().is_empty());
        // Valid entries: trimmed, blank fragments dropped, case preserved.
        assert_eq!(
            parse_allowed_subjects(Some("sub-abc, Sub-XYZ ,, ")).unwrap(),
            vec!["sub-abc".to_string(), "Sub-XYZ".to_string()]
        );
        // Set to a non-empty value that yields nothing → hard error.
        assert!(parse_allowed_subjects(Some(",,")).is_err());
        assert!(parse_allowed_subjects(Some(" , ")).is_err());
    }

    #[test]
    fn refuses_allowlist_set_but_empty() {
        // A fat-fingered secret that lists no valid subject must not silently
        // collapse to allow-all on an auth-enabled server.
        assert!(Config::build(raw_subjects(Some(",,"))).is_err());
        assert!(Config::build(raw_subjects(Some(" "))).is_err());
    }

    #[test]
    fn refuses_allowlist_without_issuer() {
        // An allowlist can only be enforced with authentication on. Set but no
        // issuer is a misconfiguration that must fail loud, not silently drop —
        // even in loopback dev mode.
        let raw = RawEnv {
            bind: "127.0.0.1:8080".to_string(),
            resource: None,
            issuer: None,
            audience: None,
            jwks_uri: None,
            allowed_subjects: Some("sub-1".to_string()),
            allow_no_auth: true,
        };
        assert!(
            Config::build(raw).is_err(),
            "an allowlist without an issuer must refuse to start"
        );
    }

    #[test]
    fn accepts_a_valid_allowlist() {
        let cfg = Config::build(raw_subjects(Some("sub-1"))).expect("valid allowlist");
        assert_eq!(
            cfg.auth.unwrap().allowed_subjects,
            vec!["sub-1".to_string()]
        );
    }

    #[test]
    fn unset_allowlist_allows_all() {
        let cfg = Config::build(raw_subjects(None)).expect("no allowlist is valid");
        assert!(cfg.auth.unwrap().allowed_subjects.is_empty());
    }

    #[test]
    fn authority_strips_scheme_and_path() {
        assert_eq!(
            authority_of("https://jojobot.net/mcp").as_deref(),
            Some("jojobot.net")
        );
        assert_eq!(
            authority_of("http://127.0.0.1:8080/mcp").as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(authority_of("not-a-url"), None);
    }

    fn raw(bind: &str, issuer: Option<&str>, allow_no_auth: bool) -> RawEnv {
        RawEnv {
            bind: bind.to_string(),
            resource: None,
            issuer: issuer.map(str::to_string),
            audience: None,
            jwks_uri: None,
            allowed_subjects: None,
            allow_no_auth,
        }
    }

    #[test]
    fn refuses_no_auth_without_explicit_optin() {
        // Missing issuer and no JOJOBOT_ALLOW_NO_AUTH must fail closed.
        assert!(Config::build(raw("127.0.0.1:8080", None, false)).is_err());
    }

    #[test]
    fn allows_no_auth_on_loopback_with_optin() {
        let cfg = Config::build(raw("127.0.0.1:8080", None, true)).expect("loopback dev mode");
        assert!(cfg.auth.is_none());
    }

    #[test]
    fn refuses_no_auth_on_public_bind_even_with_optin() {
        // The opt-in permits open dev on localhost, never on a public interface.
        assert!(Config::build(raw("0.0.0.0:8080", None, true)).is_err());
    }

    #[test]
    fn allows_public_bind_when_auth_enabled() {
        let cfg = Config::build(raw("0.0.0.0:8080", Some("https://issuer.example"), false))
            .expect("auth protects the public bind");
        assert!(cfg.auth.is_some());
    }

    #[test]
    fn origin_strips_path() {
        assert_eq!(origin_of("https://a.example/mcp"), "https://a.example");
        assert_eq!(
            origin_of("https://a.example:8443/mcp/x?y=1"),
            "https://a.example:8443"
        );
        assert_eq!(
            origin_of("http://127.0.0.1:8080/mcp"),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn origin_passthrough_without_scheme() {
        assert_eq!(origin_of("not-a-url"), "not-a-url");
    }
}
