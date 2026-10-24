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
    pub(crate) jwks_uri: Option<String>,
    /// Per-user authorization allowlist of subject ids (OIDC `sub` claim values,
    /// e.g. Pocket ID user ids). Empty means no allowlist is configured — any
    /// validated token passes, preserving the authentication-only behaviour.
    pub allowed_subjects: Vec<String>,
}

/// The browser UI's OAuth **client** settings. Present only when the UI is
/// enabled.
///
/// The MCP surface is a resource server: it verifies a token somebody else
/// obtained. A browser holds no token, so serving pages to a person means
/// jojobot is also a client of the same issuer — authorization code with PKCE,
/// then a session of its own. The client must be registered at the issuer,
/// which is why every field here is runtime configuration.
///
/// **A public client, always.** PKCE binds the code to the process that started
/// the login, so there is no secret to hold — and a secret a browser-facing
/// client does not need is one nobody has to deploy, rotate or leak.
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// The client id the issuer registered for this UI.
    pub client_id: String,
    /// The origin a browser reaches this server on, scheme included. The
    /// redirect URI is built from it, so it must match what the issuer holds.
    pub base_url: String,
}

impl UiConfig {
    /// Where the issuer sends the browser back to. Derived rather than
    /// configured separately: two settings that must agree are one setting that
    /// can disagree.
    pub fn redirect_uri(&self) -> String {
        format!("{}/ui/callback", self.base_url.trim_end_matches('/'))
    }
}

/// Full server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// **Which computed lines a write's receipt carries.** Each is switched on
    /// its own so a run can hold one still and measure the other; both are on
    /// unless an operator turns one off.
    /// This server's resource identifier, advertised in the RFC 9728
    /// protected-resource metadata.
    pub resource: String,
    /// `None` means auth is disabled — a dev-only mode that logs loudly.
    pub auth: Option<AuthConfig>,
    /// `None` means the browser UI is not served at all.
    pub ui: Option<UiConfig>,
    /// **The clock this server runs on.** The real one unless an operator
    /// stated a day for the whole run.
    pub clock: jojobot_domain::clock::Clock,
    /// `None` means no push service is set up: nothing is pushed and nothing
    /// is logged about it.
    pub ntfy: Option<crate::push::NtfyConfig>,
    /// **The apps the operator registered**, each with the hash of its token and
    /// one grant. Empty means no app is registered: the app route is not
    /// mounted and no app token opens anything.
    pub apps: Vec<jojobot_domain::app::AppPrincipal>,
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
    /// - `JOJOBOT_UI_CLIENT_ID` — set to serve the browser UI (OAuth client id).
    /// - `JOJOBOT_UI_BASE_URL` — the origin a browser reaches this server on.
    /// - `JOJOBOT_UI_CLIENT_SECRET` — gone; setting it refuses to start.
    /// - `JOJOBOT_TODAY` — a `YYYY-MM-DD` day for this server to act out.
    ///   Unset is the real clock, which is what every deployment gets.
    /// - `JOJOBOT_SHADOW` — a comma-separated list of projection families the fold
    ///   computes both ways on every read and compares, counting and logging each
    ///   difference and serving the image. Read by `wiring`, not part of `Config`.
    ///   Unset shadows none; a name no family answers to refuses to start.
    /// - `JOJOBOT_LINK_RULE` — `log-only` (the default) or `on`: whether a field
    ///   value's links are decided at the write and stored as the mark (rule 646).
    ///   Log-only computes that decision, logs the counts and changes nothing.
    ///   Read by `wiring`, not part of `Config`; any other word refuses to start.
    /// - `JOJOBOT_NTFY_URL` and `JOJOBOT_NTFY_TOPIC` — set both to push the
    ///   operator's mail to their phone through that ntfy server and topic.
    ///   Neither set pushes nothing; one without the other refuses to start.
    /// - `JOJOBOT_NTFY_TOKEN` — the access token for the topic, when it needs
    ///   one. A credential: it is never logged.
    /// - `JOJOBOT_APPS_FILE` — the path of the JSON file the NixOS module renders
    ///   for the apps the operator registered. Each app's token hash is read from
    ///   the service manager's credentials directory (`CREDENTIALS_DIRECTORY`),
    ///   from a file named `app-token-hash-` and the app's name. A malformed
    ///   file, a missing credential or a grant the server cannot hold refuses to
    ///   start.
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

        // The UI is a public client and PKCE is what protects it, so no secret
        // is read anywhere. Refusing beats ignoring: an operator who left one in
        // an environment file would otherwise keep deploying a credential in the
        // belief it authenticates something.
        if raw.ui_client_secret_set {
            anyhow::bail!(
                "JOJOBOT_UI_CLIENT_SECRET is set but the browser UI is a public client — its \
                 login is authorization code with PKCE and sends no client secret. Unset it, and \
                 delete the secret at the issuer if nothing else uses it."
            );
        }

        // **A day that is no day refuses the boot.** Falling back to the real
        // clock would serve exactly the instance the operator did not ask for,
        // and every date it filled in would be the wrong one with nothing on
        // the surface saying so — the silently-wrong state this whole setting
        // exists to keep out.
        let clock = match raw
            .today
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            None => jojobot_domain::clock::Clock::default(),
            Some(day) => jojobot_domain::clock::Clock::stating(day.parse().map_err(|e| {
                anyhow::anyhow!(
                    "JOJOBOT_TODAY ({day:?}) is not a day: {e}. It takes YYYY-MM-DD, and unset \
                     is the real clock."
                )
            })?),
        };

        // Field by field rather than by reference: `auth` above has already taken
        // ownership of the fields it needed, so the struct as a whole is no
        // longer borrowable.
        let ui = build_ui(
            raw.ui_client_id,
            raw.ui_base_url,
            auth.as_ref().map(|auth| auth.audience.as_str()),
        )?;

        let ntfy = build_ntfy(raw.ntfy_url, raw.ntfy_topic, raw.ntfy_token)?;
        let apps = build_apps(raw.apps_file, raw.credentials_dir)?;

        Ok(Config {
            bind,
            resource,
            auth,
            ui,
            clock,
            ntfy,
            apps,
        })
    }
}

/// **The name of the credential that holds an app's token hash.**
pub fn app_credential_name(app: &str) -> String {
    format!("app-token-hash-{app}")
}

/// Read the apps the operator registered, or none when no file is named.
///
/// **Fail-closed.** An app file that cannot be read, an app whose grant the
/// server cannot hold (a kind other than event, a guarded, reserved or
/// due-making key) and an app with no token-hash credential each refuse to
/// start: the other reading serves a grant nobody checked, or an app that
/// can never log in while the operator believes it can.
fn build_apps(
    file: Option<String>,
    credentials_dir: Option<String>,
) -> anyhow::Result<Vec<jojobot_domain::app::AppPrincipal>> {
    let Some(file) = file.map(|f| f.trim().to_string()).filter(|f| !f.is_empty()) else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("JOJOBOT_APPS_FILE ({file:?}) cannot be read: {e}"))?;
    let specs: Vec<jojobot_domain::app::AppSpec> = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("JOJOBOT_APPS_FILE ({file:?}) is not the apps file: {e}"))?;
    let mut hashes = std::collections::BTreeMap::new();
    for spec in &specs {
        let Some(dir) = credentials_dir.as_deref().filter(|d| !d.is_empty()) else {
            anyhow::bail!(
                "app {:?} needs its token hash as a credential, and CREDENTIALS_DIRECTORY is not \
                 set. Run the server under the service manager that loads the credential.",
                spec.name
            );
        };
        let path = std::path::Path::new(dir).join(app_credential_name(&spec.name));
        let hash = std::fs::read_to_string(&path).map_err(|e| {
            anyhow::anyhow!(
                "app {:?} has no readable token hash credential at {}: {e}",
                spec.name,
                path.display()
            )
        })?;
        hashes.insert(spec.name.clone(), hash.trim().to_string());
    }
    Ok(jojobot_domain::app::build_apps(&specs, &hashes)?)
}

/// Assemble where pushes go, or `None` when no push service is set up.
///
/// **Half of it refuses to start.** A server with no topic, or a topic with no
/// server, or a token with neither, would push nothing while the operator
/// believed their phone was told: the same silent failure the allowlist guards
/// against above.
fn build_ntfy(
    url: Option<String>,
    topic: Option<String>,
    token: Option<String>,
) -> anyhow::Result<Option<crate::push::NtfyConfig>> {
    let url = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    let topic = topic
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let token = token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    match (url, topic, token) {
        (None, None, None) => Ok(None),
        (Some(url), Some(topic), token) => {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                anyhow::bail!(
                    "JOJOBOT_NTFY_URL ({url:?}) is not an http or https address. It takes the \
                     ntfy server's address with its scheme, such as https://ntfy.example."
                );
            }
            Ok(Some(crate::push::NtfyConfig {
                url: url.trim_end_matches('/').to_string(),
                topic,
                token,
            }))
        }
        (Some(_), None, _) => anyhow::bail!(
            "JOJOBOT_NTFY_URL is set but JOJOBOT_NTFY_TOPIC is not — a push needs both a server \
             and a topic. Set the topic, or unset JOJOBOT_NTFY_URL."
        ),
        (None, Some(_), _) => anyhow::bail!(
            "JOJOBOT_NTFY_TOPIC is set but JOJOBOT_NTFY_URL is not — a push needs both a server \
             and a topic. Set the server, or unset JOJOBOT_NTFY_TOPIC."
        ),
        (None, None, Some(_)) => anyhow::bail!(
            "JOJOBOT_NTFY_TOKEN is set but JOJOBOT_NTFY_URL and JOJOBOT_NTFY_TOPIC are not — a \
             token authorises a push to a topic. Set both, or unset the token."
        ),
    }
}

/// Assemble the UI's client settings, or `None` when the UI is not enabled.
///
/// **The UI is never served without authentication.** It is a page a person
/// reads, so it cannot carry a bearer token the way `/mcp` does; its login IS
/// the issuer, and without one there is nobody to log in against. A UI that
/// served anyway would be the development bypass this design refuses to have —
/// so a client id with no issuer refuses to start rather than opening a door.
fn build_ui(
    client_id: Option<String>,
    base_url: Option<String>,
    audience: Option<&str>,
) -> anyhow::Result<Option<UiConfig>> {
    let Some(client_id) = client_id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    let Some(audience) = audience else {
        anyhow::bail!(
            "JOJOBOT_UI_CLIENT_ID is set but JOJOBOT_ISSUER is not — the browser UI logs in \
             through the issuer, so without one it could only be served open. Set \
             JOJOBOT_ISSUER, or unset JOJOBOT_UI_CLIENT_ID."
        );
    };

    // **The UI's client id may not be the agent surface's audience.** The login
    // checks its ID token against the client id and `/mcp` checks a bearer token
    // against the audience, so with one value for both the token the login
    // obtains would open `/mcp`. A warning would be read once and ignored; the
    // server does not start.
    if client_id == audience {
        anyhow::bail!(
            "JOJOBOT_UI_CLIENT_ID ({client_id:?}) equals JOJOBOT_AUDIENCE, the audience /mcp \
             requires (which is the resource id when JOJOBOT_AUDIENCE is unset). The UI's login \
             token would then be a valid bearer token for /mcp. Register the UI at the issuer \
             under a client id of its own, and set JOJOBOT_UI_CLIENT_ID to it."
        );
    }

    // The redirect URI is derived from this, and the issuer only sends a browser
    // to a URI it holds. A guessed origin sends the login somewhere the issuer
    // will refuse, and the failure surfaces at the issuer rather than here.
    let base_url = base_url.filter(|s| !s.is_empty()).ok_or_else(|| {
        anyhow::anyhow!(
            "JOJOBOT_UI_CLIENT_ID is set but JOJOBOT_UI_BASE_URL is not — the login's redirect URI \
             is built from it and must match the one registered at the issuer. Set it to the \
             origin a browser reaches this server on, scheme included."
        )
    })?;

    Ok(Some(UiConfig {
        client_id,
        base_url,
    }))
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
    ui_client_id: Option<String>,
    /// Whether a client secret was set, never its value — the flow has no use
    /// for one, and the only thing left to do with it is refuse to start.
    ui_client_secret_set: bool,
    ui_base_url: Option<String>,
    /// Unparsed, because a day that is no day refuses the boot rather than
    /// being read as *not set* — see [`Config::build`].
    today: Option<String>,
    ntfy_url: Option<String>,
    ntfy_topic: Option<String>,
    ntfy_token: Option<String>,
    /// The path of the rendered apps file; read in [`Config::build`].
    apps_file: Option<String>,
    /// Where the service manager put the credentials it loaded.
    credentials_dir: Option<String>,
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
            ui_client_id: std::env::var("JOJOBOT_UI_CLIENT_ID").ok(),
            ui_client_secret_set: std::env::var("JOJOBOT_UI_CLIENT_SECRET")
                .is_ok_and(|v| !v.is_empty()),
            ui_base_url: std::env::var("JOJOBOT_UI_BASE_URL").ok(),
            today: std::env::var("JOJOBOT_TODAY").ok(),
            ntfy_url: std::env::var("JOJOBOT_NTFY_URL").ok(),
            ntfy_topic: std::env::var("JOJOBOT_NTFY_TOPIC").ok(),
            ntfy_token: std::env::var("JOJOBOT_NTFY_TOKEN").ok(),
            apps_file: std::env::var("JOJOBOT_APPS_FILE").ok(),
            credentials_dir: std::env::var("CREDENTIALS_DIRECTORY").ok(),
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
            apps_file: None,
            credentials_dir: None,
            today: None,
            ntfy_url: None,
            ntfy_topic: None,
            ntfy_token: None,
            bind: "127.0.0.1:8080".to_string(),
            resource: None,
            issuer: Some("https://issuer.example".to_string()),
            audience: None,
            jwks_uri: None,
            allowed_subjects: subjects.map(str::to_string),
            allow_no_auth: false,
            ui_client_id: None,
            ui_client_secret_set: false,
            ui_base_url: None,
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
            apps_file: None,
            credentials_dir: None,
            today: None,
            ntfy_url: None,
            ntfy_topic: None,
            ntfy_token: None,
            bind: "127.0.0.1:8080".to_string(),
            resource: None,
            issuer: None,
            audience: None,
            jwks_uri: None,
            allowed_subjects: Some("sub-1".to_string()),
            allow_no_auth: true,
            ui_client_id: None,
            ui_client_secret_set: false,
            ui_base_url: None,
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
            authority_of("https://jojobot.example/mcp").as_deref(),
            Some("jojobot.example")
        );
        assert_eq!(
            authority_of("http://127.0.0.1:8080/mcp").as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(authority_of("not-a-url"), None);
    }

    fn raw(bind: &str, issuer: Option<&str>, allow_no_auth: bool) -> RawEnv {
        RawEnv {
            apps_file: None,
            credentials_dir: None,
            today: None,
            ntfy_url: None,
            ntfy_topic: None,
            ntfy_token: None,
            bind: bind.to_string(),
            resource: None,
            issuer: issuer.map(str::to_string),
            audience: None,
            jwks_uri: None,
            allowed_subjects: None,
            allow_no_auth,
            ui_client_id: None,
            ui_client_secret_set: false,
            ui_base_url: None,
        }
    }

    /// **A stated day that is no day refuses the boot.**
    ///
    /// Falling back to the real clock would serve exactly the instance the
    /// operator did not ask for: every date it filled in would be wrong, and
    /// nothing on the surface would say so. Loud beats plausible.
    ///
    /// **Three states**, because each of the other two is how this one could
    /// pass while doing nothing: a day that parses is taken, and an operator
    /// who set nothing gets the real clock.
    #[test]
    fn a_stated_day_that_is_no_day_refuses_to_start() {
        let refused = Config::build(RawEnv {
            today: Some("the first of June".to_string()),
            ..raw("127.0.0.1:8080", None, true)
        })
        .expect_err("a day that is no day refuses the boot");
        assert!(
            format!("{refused:#}").contains("JOJOBOT_TODAY"),
            "the refusal names the setting the operator has to fix: {refused:#}"
        );

        let acting = Config::build(RawEnv {
            today: Some("2026-06-01".to_string()),
            ..raw("127.0.0.1:8080", None, true)
        })
        .expect("a day that parses is taken");
        assert_eq!(
            acting.clock.stated().map(|d| d.to_string()),
            Some("2026-06-01".to_string()),
        );

        let ordinary = Config::build(raw("127.0.0.1:8080", None, true))
            .expect("a server with nothing set starts");
        assert_eq!(
            ordinary.clock.stated(),
            None,
            "an operator who set nothing gets the real clock",
        );
    }

    fn raw_ui(issuer: Option<&str>, client_id: Option<&str>, base_url: Option<&str>) -> RawEnv {
        RawEnv {
            apps_file: None,
            credentials_dir: None,
            today: None,
            ntfy_url: None,
            ntfy_topic: None,
            ntfy_token: None,
            bind: "127.0.0.1:8080".to_string(),
            resource: None,
            issuer: issuer.map(str::to_string),
            audience: None,
            jwks_uri: None,
            allowed_subjects: None,
            allow_no_auth: true,
            ui_client_id: client_id.map(str::to_string),
            ui_client_secret_set: false,
            ui_base_url: base_url.map(str::to_string),
        }
    }

    #[test]
    fn the_ui_refuses_to_be_served_without_an_issuer() {
        // The UI's login IS the issuer. Serving it without one could only mean
        // serving it open, which is the bypass this design does not have.
        let raw = raw_ui(None, Some("jojobot-ui"), Some("https://jojobot.test"));
        assert!(Config::build(raw).is_err());
    }

    /// A configured UI whose client id is `client_id`, on a server whose agent
    /// surface requires `audience` (`None` leaves it to default to the resource).
    fn raw_ui_on_audience(audience: Option<&str>, client_id: Option<&str>) -> RawEnv {
        RawEnv {
            audience: audience.map(str::to_string),
            ..raw_ui(
                Some("https://issuer.example"),
                client_id,
                Some("https://jojobot.test"),
            )
        }
    }

    /// **The UI's client id may not be the audience of the agent surface.** The
    /// login checks its ID token against the client id, and `/mcp` checks a
    /// bearer token against the audience. With the two equal, the token the login
    /// obtains would be a valid bearer for `/mcp`. The refusal names both
    /// variables, so the operator knows which two to change.
    #[test]
    fn a_ui_client_id_equal_to_the_audience_refuses_to_start_naming_both() {
        let refused = Config::build(raw_ui_on_audience(Some("shared-id"), Some("shared-id")))
            .expect_err("one value for both would open the agent surface to the UI's token");
        let said = format!("{refused:#}");
        assert!(said.contains("JOJOBOT_UI_CLIENT_ID"), "{said}");
        assert!(said.contains("JOJOBOT_AUDIENCE"), "{said}");
    }

    /// **The audience that defaults to the resource id counts too.** An operator
    /// who sets no audience still has one, so a client id equal to the resource id
    /// is the same opening.
    #[test]
    fn a_ui_client_id_equal_to_the_defaulted_audience_refuses_to_start() {
        let raw = RawEnv {
            resource: Some("shared-id".to_string()),
            ..raw_ui_on_audience(None, Some("shared-id"))
        };
        let refused = Config::build(raw).expect_err("the default audience is an audience");
        assert!(format!("{refused:#}").contains("JOJOBOT_UI_CLIENT_ID"));
    }

    /// The positives the refusals lean on: distinct values build, and a server
    /// with no UI builds whatever its audience is.
    #[test]
    fn distinct_values_build_and_with_no_ui_any_audience_builds() {
        let distinct = Config::build(raw_ui_on_audience(
            Some("https://jojobot.test/mcp"),
            Some("jojobot-ui"),
        ))
        .expect("a client id of its own builds");
        assert!(distinct.ui.is_some());

        let no_ui = Config::build(raw_ui_on_audience(Some("jojobot-ui"), None))
            .expect("with the UI unset no client id is compared");
        assert!(no_ui.ui.is_none());
    }

    #[test]
    fn the_ui_refuses_to_guess_its_own_origin() {
        // The redirect URI is derived from the base URL and the issuer only
        // honours a URI it holds, so an unset origin is a misconfiguration
        // rather than something to default.
        let raw = raw_ui(Some("https://issuer.example"), Some("jojobot-ui"), None);
        assert!(Config::build(raw).is_err());
    }

    #[test]
    fn the_ui_is_off_unless_a_client_id_names_it() {
        let cfg = Config::build(raw_ui(Some("https://issuer.example"), None, None))
            .expect("no UI is a valid configuration");
        assert!(cfg.ui.is_none());
    }

    #[test]
    fn the_redirect_uri_is_the_callback_on_the_configured_origin() {
        let cfg = Config::build(raw_ui(
            Some("https://issuer.example"),
            Some("jojobot-ui"),
            Some("https://jojobot.test/"),
        ))
        .expect("a complete UI configuration");
        let ui = cfg.ui.expect("the UI is configured");
        assert_eq!(ui.client_id, "jojobot-ui");
        assert_eq!(ui.redirect_uri(), "https://jojobot.test/ui/callback");
    }

    #[test]
    fn refuses_a_client_secret_the_login_cannot_use() {
        // The login sends no secret. A variable that does nothing must not be
        // quietly ignored: left in an environment file it would go on being
        // deployed as if it authenticated something.
        let raw = RawEnv {
            ui_client_secret_set: true,
            ..raw_ui(
                Some("https://issuer.example"),
                Some("jojobot-ui"),
                Some("https://jojobot.test"),
            )
        };
        assert!(Config::build(raw).is_err());
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

    /// A loopback development environment with the three push settings as given.
    fn with_ntfy(url: Option<&str>, topic: Option<&str>, token: Option<&str>) -> RawEnv {
        RawEnv {
            ntfy_url: url.map(str::to_string),
            ntfy_topic: topic.map(str::to_string),
            ntfy_token: token.map(str::to_string),
            ..raw("127.0.0.1:8080", None, true)
        }
    }

    /// **No push settings push nothing.** The default instance, and blank values
    /// (an environment file with the keys present and empty) read as not set.
    #[test]
    fn no_ntfy_settings_means_no_push() {
        assert!(
            Config::build(with_ntfy(None, None, None))
                .unwrap()
                .ntfy
                .is_none()
        );
        assert!(
            Config::build(with_ntfy(Some(" "), Some(""), Some("")))
                .unwrap()
                .ntfy
                .is_none()
        );
    }

    /// **A server and a topic set a push up**, the token is optional, and a
    /// trailing slash on the server is dropped so the call is made to the root.
    #[test]
    fn a_server_and_a_topic_set_a_push_up_with_or_without_a_token() {
        let open = Config::build(with_ntfy(
            Some("https://ntfy.example/"),
            Some("phone"),
            None,
        ))
        .unwrap()
        .ntfy
        .expect("a push is set up");
        assert_eq!(
            (open.url.as_str(), open.topic.as_str(), open.token.clone()),
            ("https://ntfy.example", "phone", None)
        );
        let closed = Config::build(with_ntfy(
            Some("http://127.0.0.1:2586"),
            Some("phone"),
            Some("tk_example"),
        ))
        .unwrap()
        .ntfy
        .expect("a push is set up");
        assert_eq!(closed.token.as_deref(), Some("tk_example"));
    }

    /// **Half a setup refuses to start, naming the one that is missing.** And the
    /// credential never prints, in the refusal or in the config's Debug.
    #[test]
    fn half_a_push_setup_refuses_to_start_and_the_token_never_prints() {
        let url_only =
            Config::build(with_ntfy(Some("https://ntfy.example"), None, None)).unwrap_err();
        assert!(format!("{url_only:#}").contains("JOJOBOT_NTFY_TOPIC"));
        let topic_only = Config::build(with_ntfy(None, Some("phone"), None)).unwrap_err();
        assert!(format!("{topic_only:#}").contains("JOJOBOT_NTFY_URL"));
        let token_only = Config::build(with_ntfy(None, None, Some("tk_wiggum"))).unwrap_err();
        assert!(format!("{token_only:#}").contains("JOJOBOT_NTFY_TOKEN"));
        assert!(!format!("{token_only:#}").contains("tk_wiggum"));

        let configured = Config::build(with_ntfy(
            Some("https://ntfy.example"),
            Some("phone"),
            Some("tk_wiggum"),
        ))
        .unwrap()
        .ntfy
        .unwrap();
        assert!(!format!("{configured:?}").contains("tk_wiggum"));
    }

    /// A server that is no http address is refused rather than called.
    #[test]
    fn a_server_without_a_scheme_is_refused() {
        let refused =
            Config::build(with_ntfy(Some("ntfy.example"), Some("phone"), None)).unwrap_err();
        assert!(format!("{refused:#}").contains("JOJOBOT_NTFY_URL"));
    }
    /// A scratch directory holding an apps file and credentials, removed on drop.
    struct Apps(std::path::PathBuf);

    impl Apps {
        fn with(name: &str, file: &str, credentials: &[(&str, &str)]) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("jojobot-config-apps-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("creds")).expect("a scratch directory");
            std::fs::write(dir.join("apps.json"), file).expect("the apps file");
            for (app, hash) in credentials {
                std::fs::write(
                    dir.join("creds").join(super::app_credential_name(app)),
                    hash,
                )
                .expect("a credential");
            }
            Apps(dir)
        }

        fn load(&self) -> anyhow::Result<Vec<jojobot_domain::app::AppPrincipal>> {
            super::build_apps(
                Some(self.0.join("apps.json").display().to_string()),
                Some(self.0.join("creds").display().to_string()),
            )
        }
    }

    impl Drop for Apps {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn app_json(keys: &str, kind: &str, revoked: bool) -> String {
        format!(
            r#"[{{"name":"springfield-ci","operation":"record_deploy_event",
            "thread":"thread:deploys-alpha","kind":"{kind}","keys":[{keys}],"revoked":{revoked}}}]"#
        )
    }

    fn a_hash() -> String {
        jojobot_domain::app::hash_token(&jojobot_domain::app::mint_token())
    }

    #[test]
    fn no_apps_file_registers_no_app() {
        assert!(super::build_apps(None, None).unwrap().is_empty());
        assert!(
            super::build_apps(Some("  ".into()), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn an_app_is_read_with_its_credential_and_its_revoked_mark() {
        let hash = a_hash();
        let live = Apps::with(
            "live",
            &app_json("\"commit\",\"host\"", "event", false),
            &[("springfield-ci", &format!("{hash}\n"))],
        );
        let apps = live.load().expect("a good file");
        assert_eq!(apps.len(), 1);
        assert_eq!(
            apps[0].token_hash, hash,
            "the credential's newline is trimmed"
        );
        assert!(!apps[0].revoked);
        let off = Apps::with(
            "off",
            &app_json("\"commit\"", "event", true),
            &[("springfield-ci", &hash)],
        );
        assert!(off.load().expect("a good file")[0].revoked);
    }

    #[test]
    fn the_server_refuses_to_start_on_a_grant_or_file_it_cannot_hold() {
        let hash = a_hash();
        // **Each file is refused for its own fault**, so the reason names the
        // offender and none of the others: a file refused for the wrong cause
        // would pass a check that it was refused at all.
        let cases = [
            (
                "guarded",
                app_json("\"commit\",\"reports_to\"", "event", false),
                "reports_to",
            ),
            ("due", app_json("\"runs_out\"", "event", false), "runs_out"),
            ("kind", app_json("\"commit\"", "person", false), "person"),
            ("malformed", "{".to_string(), "is not the apps file"),
        ];
        for (name, file, offender) in cases.clone() {
            let apps = Apps::with(name, &file, &[("springfield-ci", &hash)]);
            let reason = apps.load().expect_err(name).to_string();
            assert!(
                reason.contains(offender),
                "{name} names {offender}: {reason}"
            );
            for (other, _, other_offender) in &cases {
                assert!(
                    *other == name || !reason.contains(other_offender),
                    "{name} is refused for {offender}, not for {other_offender}: {reason}"
                );
            }
        }
        let positive = Apps::with(
            "positive",
            &app_json("\"commit\"", "event", false),
            &[("springfield-ci", &hash)],
        );
        assert!(
            positive.load().is_ok(),
            "the same shape without the fault loads"
        );
        // **Two apps whose credentials hold one hash** refuse the start and name both.
        let two = format!(
            "[{},{}]",
            app_json("\"commit\"", "event", false).trim_matches(|c| c == '[' || c == ']'),
            app_json("\"commit\"", "event", false)
                .trim_matches(|c| c == '[' || c == ']')
                .replace("springfield-ci", "shelbyville-ci"),
        );
        let shared = Apps::with(
            "shared",
            &two,
            &[("springfield-ci", &hash), ("shelbyville-ci", &hash)],
        );
        let refused = format!("{:#}", shared.load().expect_err("one hash for two apps"));
        assert!(
            refused.contains("springfield-ci") && refused.contains("shelbyville-ci"),
            "the refusal names both apps: {refused}"
        );
        let no_credential = Apps::with("nocred", &app_json("\"commit\"", "event", false), &[]);
        let missing = no_credential
            .load()
            .expect_err("a missing credential")
            .to_string();
        assert!(
            missing.contains("springfield-ci") && missing.contains("credential"),
            "the app whose credential is missing is named: {missing}"
        );
        let no_directory = super::build_apps(
            Some(no_credential.0.join("apps.json").display().to_string()),
            None,
        )
        .expect_err("no credentials directory")
        .to_string();
        assert!(
            no_directory.contains("CREDENTIALS_DIRECTORY"),
            "the directory that is not set is named: {no_directory}"
        );
        assert!(
            !no_directory.contains("no readable token hash"),
            "an unset directory is not a missing file: {no_directory}"
        );
    }
}
