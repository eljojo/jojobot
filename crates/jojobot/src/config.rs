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

        let auth = raw
            .issuer
            .filter(|s| !s.is_empty())
            .map(|issuer| AuthConfig {
                issuer,
                audience: raw
                    .audience
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| resource.clone()),
                jwks_uri: raw.jwks_uri.filter(|s| !s.is_empty()),
            });

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
            allow_no_auth: std::env::var("JOJOBOT_ALLOW_NO_AUTH")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{Config, RawEnv, origin_of};

    fn raw(bind: &str, issuer: Option<&str>, allow_no_auth: bool) -> RawEnv {
        RawEnv {
            bind: bind.to_string(),
            resource: None,
            issuer: issuer.map(str::to_string),
            audience: None,
            jwks_uri: None,
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
        assert_eq!(origin_of("http://127.0.0.1:8080/mcp"), "http://127.0.0.1:8080");
    }

    #[test]
    fn origin_passthrough_without_scheme() {
        assert_eq!(origin_of("not-a-url"), "not-a-url");
    }
}
