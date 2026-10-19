//! **The app API: one route for an outside program with a login of its own.**
//!
//! An app is a service principal the operator registered in the server's
//! configuration (rule 542). It holds an opaque token and one grant. This
//! module holds the three things that make that safe, in the order a request
//! meets them:
//!
//! 1. [`require_app`] — the bearer is an app token, it hashes to a registered
//!    app, and that app is not revoked. Everything else is a 401 and one log
//!    line.
//! 2. [`refuse_app_bearer`] — the same shape of bearer is refused at `/mcp`,
//!    whether or not a token validator is mounted there.
//! 3. [`deploy_event`] — the body is judged by [`jojobot_domain::app::authorize`],
//!    the one check, and only what it returns reaches the store.
//!
//! **A log line carries the reason and the app's name when known, and never a
//! body or a token.**

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jojobot_domain::app::{
    AppError, AppOutcome, AppPrincipal, DeployEvent, RECORD_DEPLOY_EVENT, authorize, hash_token,
    is_token,
};
use jojobot_domain::memory::{Entity, Fact, Guarded, Landed, MemoryError};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::auth::bearer_from_header;

/// The path an app records a deploy event at.
pub const DEPLOY_EVENTS_PATH: &str = "/app/v1/deploy-events";

/// **The registered apps, found by the hash of their token.**
#[derive(Debug, Default)]
pub struct Apps {
    by_hash: HashMap<String, AppPrincipal>,
}

impl Apps {
    /// The apps the operator registered. Two apps with one token hash would
    /// answer as one, so the later is refused at configuration; here the map
    /// simply keeps the last.
    pub fn new(apps: Vec<AppPrincipal>) -> Arc<Self> {
        Arc::new(Apps {
            by_hash: apps
                .into_iter()
                .map(|app| (app.token_hash.clone(), app))
                .collect(),
        })
    }

    /// Whether no app is registered.
    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    fn lookup(&self, token: &str) -> Option<&AppPrincipal> {
        self.by_hash.get(&hash_token(token))
    }
}

fn bearer_of(req: &Request) -> Option<&str> {
    bearer_from_header(
        req.headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
    )
    .ok()
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

/// **Let an app's request through, or refuse it.** The principal travels in the
/// request's extensions.
pub async fn require_app(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(token) = bearer_of(&req).filter(|t| is_token(t)) else {
        tracing::info!(reason = "no app token", "app request refused");
        return unauthorized();
    };
    match state.apps.lookup(token) {
        None => {
            tracing::info!(reason = "unknown token", "app request refused");
            unauthorized()
        }
        Some(app) if app.revoked => {
            tracing::info!(reason = "revoked", app = %app.id, "app request refused");
            unauthorized()
        }
        Some(app) => {
            let app = app.clone();
            req.extensions_mut().insert(app);
            next.run(req).await
        }
    }
}

/// **Refuse an app's token at `/mcp`.** An app is not a bot and has no
/// session, so nothing it holds may open the tool surface. Mounted whether or
/// not a validator is: with the guard off, an app token is just a bearer nobody
/// looks at, and this is what stops it opening the surface.
pub async fn refuse_app_bearer(req: Request, next: Next) -> Response {
    if bearer_of(&req).is_some_and(is_token) {
        tracing::info!(reason = "app token at the tool surface", "request refused");
        return unauthorized();
    }
    next.run(req).await
}

/// What an app sends. `deny_unknown_fields`: a key the shape does not name is
/// refused rather than ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeployEventBody {
    /// The caller's name for this attempt.
    pub attempt_id: String,
    /// The keys and values of the event.
    pub fields: std::collections::BTreeMap<String, String>,
}

fn answer(status: StatusCode, body: serde_json::Value) -> Response {
    (status, Json(body)).into_response()
}

/// **The answer for an event that was stored**: the one builder, so a write whose
/// fold refresh failed afterwards is answered exactly as an ordinary success is.
fn recorded(entity: &Entity, fact: &Fact) -> Response {
    answer(
        StatusCode::CREATED,
        json!({
            "handle": entity.id.to_string(),
            "address": fact.address().to_string(),
            "repeat": false,
        }),
    )
}

/// **Record one deploy event under the thread the app's grant names.**
pub async fn deploy_event(
    State(state): State<AppState>,
    axum::Extension(app): axum::Extension<AppPrincipal>,
    Json(body): Json<DeployEventBody>,
) -> Response {
    let judged = authorize(
        &app,
        &DeployEvent {
            attempt_id: body.attempt_id,
            fields: body.fields,
        },
    );
    let write = match judged {
        Ok(write) => write,
        Err(AppError::OutsideGrant(why)) => {
            tracing::info!(app = %app.id, reason = "outside the grant", "app write refused");
            return answer(StatusCode::FORBIDDEN, json!({ "error": why }));
        }
        Err(AppError::Invalid(why) | AppError::Config(why)) => {
            tracing::info!(app = %app.id, reason = "invalid", "app write refused");
            return answer(StatusCode::UNPROCESSABLE_ENTITY, json!({ "error": why }));
        }
    };
    let received_at = state.clock.now().to_string();
    // **The day is the instance's own**, read through the frame the operator's pages
    // read, so an event taken in late evening there is not dated the next UTC day.
    let frame = jojobot_mcp::orientation::instance_zone::unzoned_frame(&*state.memory).await;
    let today = state.clock.today_in(&frame);
    let event = write.into_event(RECORD_DEPLOY_EVENT, received_at, today);
    match state.memory.record_app_event(event).await {
        Ok(AppOutcome::Landed(landed)) => match *landed {
            Guarded::Written((entity, fact)) => {
                tracing::info!(app = %app.id, outcome = "recorded", "app write landed");
                recorded(&entity, &fact)
            }
            Guarded::Blocked { .. } => {
                tracing::warn!(app = %app.id, outcome = "blocked", "app write blocked by the guard");
                answer(
                    StatusCode::CONFLICT,
                    json!({ "error": "the write was blocked and nothing was written" }),
                )
            }
        },
        Ok(AppOutcome::Repeat(receipt)) => {
            tracing::info!(app = %app.id, outcome = "repeat", "app write repeated");
            answer(
                StatusCode::OK,
                json!({
                    "handle": receipt.handle,
                    "address": receipt.address,
                    "repeat": true,
                }),
            )
        }
        Ok(AppOutcome::Conflict) => {
            tracing::info!(app = %app.id, outcome = "conflict", "app attempt reused");
            answer(
                StatusCode::CONFLICT,
                json!({ "error": "this attempt_id already recorded an event with a different body" }),
            )
        }
        Err(MemoryError::Conflict) => answer(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "another write landed at the same moment; send the same call again" }),
        ),
        Err(MemoryError::Store(_)) => answer(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "the store could not be reached; send the same call again" }),
        ),
        // **A write that landed is never reported as failed** (rule 130): the event
        // is stored and only the fold's refresh failed, so the app is told it
        // landed. Its retry answers the same receipt as a repeat.
        Err(MemoryError::FoldBehind {
            landed: Landed::Creation(created),
            source,
            ..
        }) => {
            tracing::warn!(
                app = %app.id,
                error = %source,
                "app write landed and the fold could not be refreshed"
            );
            let (entity, fact) = *created;
            recorded(&entity, &fact)
        }
        Err(other) => {
            tracing::warn!(app = %app.id, error = %other, "app write refused by the store");
            answer(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({ "error": other.to_string() }),
            )
        }
    }
}

/// **What `jojobot app-token` prints**: a fresh token and its hash. It touches no
/// store. The operator puts the hash in the secret the module reads and gives the
/// token to the app.
pub fn token_report() -> String {
    let token = jojobot_domain::app::mint_token();
    let hash = hash_token(&token);
    format!("token: {token}\nhash:  {hash}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `jojobot app-token` prints a token an app can send and the hash the
    /// operator stores, and the hash is the one the server looks the token up by.
    #[test]
    fn the_token_report_prints_a_token_and_the_hash_the_server_looks_it_up_by() {
        let report = token_report();
        let token = report
            .lines()
            .find_map(|l| l.strip_prefix("token: "))
            .expect("a token line");
        let hash = report
            .lines()
            .find_map(|l| l.strip_prefix("hash:  "))
            .expect("a hash line");
        assert!(is_token(token));
        assert_eq!(hash, hash_token(token));
        assert_ne!(token_report(), report, "a fresh token each time");
    }
}
