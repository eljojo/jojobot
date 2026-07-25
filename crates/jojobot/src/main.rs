//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::outline::{OutlineConfig, OutlineStore, Secret};
use jojobot_domain::memory::Memory;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use jojobot::auth::Validator;
use jojobot::config::{Config, origin_of};
use jojobot::{AppState, build_app};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env().context("loading configuration")?;
    tracing::info!(
        bind = %config.bind,
        resource = %config.resource,
        auth = config.auth.is_some(),
        "starting jojobot"
    );

    let http = reqwest::Client::builder()
        // Bound the JWKS/discovery fetch so a hung issuer can't stall startup.
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("building HTTP client")?;

    let (validator, issuer) = match &config.auth {
        Some(auth_cfg) => {
            let validator = Validator::discover(auth_cfg, &http)
                .await
                .context("building the token validator from the issuer JWKS")?;
            let allowlist = if auth_cfg.allowed_subjects.is_empty() {
                "open (any authenticated user)".to_string()
            } else {
                format!("{} subject(s)", auth_cfg.allowed_subjects.len())
            };
            tracing::info!(
                issuer = %auth_cfg.issuer,
                audience = %auth_cfg.audience,
                %allowlist,
                "resource-server auth enabled"
            );
            (Some(std::sync::Arc::new(validator)), Some(auth_cfg.issuer.clone()))
        }
        None => {
            tracing::warn!(
                "AUTH DISABLED — JOJOBOT_ISSUER is unset, so /mcp is open. Development use only."
            );
            (None, None)
        }
    };

    // The Memory port. Always the real Outline adapter — no toy store ships. It
    // discovers/creates its own `jojobot` collection by name; the only config is
    // credentials. Unset credentials yield an unconfigured store: the server
    // still boots and serves `ping`, but `capture`/`recall` refuse loudly until
    // Outline is wired (see the fail-soft rationale in the handoff/report).
    let memory: Arc<dyn Memory> = match outline_from_env() {
        Some(cfg) => {
            tracing::info!(base_url = %cfg.base_url, "memory: Outline store wired");
            Arc::new(OutlineStore::new(http.clone(), cfg))
        }
        None => {
            tracing::warn!(
                "MEMORY DISABLED — set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN to enable \
                 capture/recall. Serving ping only; memory verbs return a NotConfigured error."
            );
            Arc::new(OutlineStore::unconfigured())
        }
    };

    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource",
        origin_of(&config.resource)
    );
    let state = AppState {
        resource: config.resource.clone(),
        issuer,
        validator,
        metadata_url,
        memory,
    };

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!("listening on http://{}/mcp", config.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await
        .context("server error")?;

    Ok(())
}

/// Read the Outline store's **credentials** from the environment — the only
/// config there is; the collection and docs are discovered by convention. Both
/// must be set; either missing → `None` (memory disabled). The adapter never
/// reads env itself — the operator sets it here, at the composition root.
fn outline_from_env() -> Option<OutlineConfig> {
    let nonempty = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty());
    Some(OutlineConfig {
        base_url: nonempty("JOJOBOT_OUTLINE_URL")?,
        token: Secret::new(nonempty("JOJOBOT_OUTLINE_TOKEN")?),
    })
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
