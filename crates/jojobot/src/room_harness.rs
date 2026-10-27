//! The grader lives beside the occupant in this room's server process.

use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use jojobot_mcp::room_harness::{Harness, SeedWrite};
use serde::Deserialize;
use serde_json::{Value, json};

pub const PATH: &str = "/_room_harness";

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    Read {
        verb: String,
        arguments: Value,
    },
    Seed {
        author: String,
        writes: Vec<SeedWrite>,
    },
    Changes {
        position: Value,
    },
}

pub(super) fn router(state: crate::AppState) -> Router<crate::AppState> {
    let handler = jojobot_mcp::Jojobot::new(
        state.memory,
        state.search,
        state.mailboxes,
        state.sessions,
        state.teachings,
        state.registry,
    )
    .on_clock(state.clock);
    Router::new()
        .route(PATH, post(answer))
        .with_state(Arc::new(Harness::new(handler)))
}

async fn answer(
    State(harness): State<Arc<Harness>>,
    Json(request): Json<Request>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let answer = match request {
        Request::Read { verb, arguments } => harness.read(&verb, arguments).await,
        Request::Seed { author, writes } => harness.seed(&author, writes).await,
        Request::Changes { position } => harness.changes(position).await,
    };
    answer.map(Json).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"harness_error":error})),
        )
    })
}
