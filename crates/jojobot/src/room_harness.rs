//! The grader lives beside the occupant in this room's server process.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json, Router, extract::State, http::StatusCode, middleware, response::IntoResponse,
    routing::post,
};
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
    Refusals,
}

struct Door {
    harness: Harness,
    token: Option<String>,
    refused: AtomicU64,
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
    route(
        Harness::new(handler),
        std::env::var("JOJOBOT_ROOM_HARNESS_TOKEN").ok(),
    )
    .with_state(())
}

fn route(harness: Harness, token: Option<String>) -> Router {
    let door = Arc::new(Door {
        harness,
        token,
        refused: AtomicU64::new(0),
    });
    Router::new()
        .route(
            PATH,
            post(answer).layer(middleware::from_fn_with_state(door.clone(), require_token)),
        )
        .with_state(door)
}

async fn require_token(
    State(door): State<Arc<Door>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    if let Some(expected) = door.token.as_deref().filter(|token| !token.is_empty())
        && request
            .headers()
            .get("x-jojobot-room-harness")
            .is_some_and(|sent| sent.as_bytes() == expected.as_bytes())
    {
        return next.run(request).await;
    }
    door.refused.fetch_add(1, Ordering::SeqCst);
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"harness_error":"a room harness credential is required"})),
    )
        .into_response()
}

async fn answer(
    State(door): State<Arc<Door>>,
    Json(request): Json<Request>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let answer = match request {
        Request::Read { verb, arguments } => door.harness.read(&verb, arguments).await,
        Request::Seed { author, writes } => door.harness.seed(&author, writes).await,
        Request::Changes { position } => door.harness.changes(position).await,
        Request::Refusals => Ok(json!({"refused":door.refused.load(Ordering::SeqCst)})),
    };
    answer.map(Json).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"harness_error":error})),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use tower::ServiceExt;

    fn handler() -> jojobot_mcp::Jojobot {
        let memory = Arc::new(
            jojobot_adapters::search::IndexedMemory::new(Arc::new(
                jojobot_domain::memory::testing::InMemoryMemory::booted(),
            ))
            .expect("index"),
        );
        let index = memory.index();
        jojobot_mcp::Jojobot::new(
            memory.clone(),
            Arc::new(jojobot_adapters::search::Retrieval::new(
                index,
                vec![memory],
            )),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        )
    }

    async fn call(app: Router, token: Option<&str>, body: Value) -> axum::response::Response {
        let mut request =
            axum::http::Request::post(PATH).header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("x-jojobot-room-harness", token);
        }
        app.oneshot(request.body(Body::from(body.to_string())).expect("request"))
            .await
            .expect("route")
    }

    #[tokio::test]
    async fn room_harness_token_is_room_scoped_and_closed_without_configuration() {
        let handler = handler();
        let read = json!({"operation":"read", "verb":"ping", "arguments":{}});
        for token in [None, Some(String::new()), Some("beta".into())] {
            let app = route(Harness::new(handler.clone()), token);
            for sent in [None, Some("alpha")] {
                assert_eq!(
                    call(app.clone(), sent, read.clone()).await.status(),
                    StatusCode::UNAUTHORIZED
                );
            }
        }
    }

    #[tokio::test]
    async fn room_harness_token_refuses_and_records_uncredentialled_requests() {
        let app = route(Harness::new(handler()), Some("alpha".into()));
        let read = json!({"operation":"read", "verb":"ping", "arguments":{}});
        let refused = json!({"operation":"refusals"});
        let before = call(app.clone(), Some("alpha"), refused.clone()).await;
        assert_eq!(before.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(before.into_body(), 1024).await.expect("body"))
                .expect("JSON");
        assert_eq!(body, json!({"refused":0}));
        for token in [None, Some("beta")] {
            assert_eq!(
                call(app.clone(), token, read.clone()).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let probe = axum::http::Request::get(PATH)
            .body(Body::empty())
            .expect("probe");
        assert_eq!(
            app.clone().oneshot(probe).await.expect("route").status(),
            StatusCode::UNAUTHORIZED
        );
        for _ in 0..2 {
            let response = call(app.clone(), Some("alpha"), refused.clone()).await;
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.expect("body"))
                    .expect("JSON");
            assert_eq!(body, json!({"refused":3}));
        }
        assert_eq!(
            call(app, Some("alpha"), read).await.status(),
            StatusCode::OK
        );
    }
}
