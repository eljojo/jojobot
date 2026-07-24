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
        let bind_str =
            std::env::var("JOJOBOT_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let bind: SocketAddr = bind_str
            .parse()
            .map_err(|e| anyhow::anyhow!("JOJOBOT_BIND ({bind_str:?}) is not a valid address: {e}"))?;

        let resource = std::env::var("JOJOBOT_RESOURCE")
            .unwrap_or_else(|_| format!("http://{bind}/mcp"));

        let auth = match std::env::var("JOJOBOT_ISSUER") {
            Ok(issuer) if !issuer.is_empty() => {
                let audience =
                    std::env::var("JOJOBOT_AUDIENCE").unwrap_or_else(|_| resource.clone());
                let jwks_uri = std::env::var("JOJOBOT_JWKS_URI").ok().filter(|s| !s.is_empty());
                Some(AuthConfig {
                    issuer,
                    audience,
                    jwks_uri,
                })
            }
            _ => None,
        };

        Ok(Config {
            bind,
            resource,
            auth,
        })
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
    use super::origin_of;

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
