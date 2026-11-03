//! **One witness for each route that writes the spec of a bot request**, each
//! reddened by removing that route's lock and by nothing else.
//!
//! A route reads the highest generation ever written, adds one and stores it, so
//! two writes of one request's spec that read at the same moment name two specs
//! with one number. The lock is what stops that. A witness holds the request's
//! lock from outside, starts the route and sees it wait; when the lock is let go
//! the route lands and the generation is the one past the highest. A route whose
//! acquisition is removed runs while the lock is held, and its witness fails.

use super::*;
use crate::harness::*;
use crate::memory::testing::{capture_args, capture_ok, ensure};
use std::time::Duration;

const REQUEST: &str = "bot-request:theta";

/// A request whose spec bot:otto may write: lisa is the target and reports to
/// otto. It holds a first spec, so it has a generation.
async fn a_request_otto_manages(jojobot: &Jojobot) {
    for handle in ["bot:otto", "bot:lisa", REQUEST] {
        ensure(jojobot, handle).await;
    }
    jojobot
        .memory
        .capture(jojobot_domain::memory::NewFact {
            fields: [("reports_to".to_string(), "bot:otto".to_string())].into(),
            sets: ["reports_to".to_string()].into(),
            ..jojobot_domain::memory::NewFact::about(
                EntityId::person("bot:lisa"),
                "reports to otto",
                jiff::civil::date(2026, 10, 1),
            )
        })
        .await
        .expect("the chart is placed");
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("target_bot".to_string(), "bot:lisa".to_string()),
                    ("desired".to_string(), "running".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args(REQUEST, "the first spec")
        },
    )
    .await;
}

async fn generation_of(jojobot: &Jojobot, request: &str) -> String {
    // A request not yet made has no fields to read, and no generation.
    jojobot
        .memory
        .fields(&EntityId::person(request))
        .await
        .ok()
        .and_then(|fields| fields.get("generation").cloned())
        .unwrap_or_default()
}

/// **Start a route while the request's lock is held from outside, see it wait,
/// let the lock go and see it land.** The generation is the same while it waits
/// and one past the highest after.
async fn waits_for_the_requests_lock<F>(jojobot: &Jojobot, request: &str, route: F)
where
    F: std::future::Future,
{
    let before: u64 = generation_of(jojobot, request).await.parse().unwrap_or(0);
    let held = jojobot
        .hold_placing(&EntityId::person(request))
        .await
        .expect("the lock is free");
    tokio::pin!(route);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), &mut route)
            .await
            .is_err(),
        "the route ran while the request's lock was held"
    );
    assert_eq!(
        generation_of(jojobot, request).await,
        if before == 0 {
            String::new()
        } else {
            before.to_string()
        },
        "nothing was written while the lock was held"
    );
    drop(held);
    tokio::time::timeout(Duration::from_secs(5), route)
        .await
        .expect("the route lands once the lock is let go");
    assert_eq!(
        generation_of(jojobot, request).await,
        (before + 1).to_string(),
        "the write named the next generation"
    );
}

fn args<T: serde::de::DeserializeOwned>(body: serde_json::Value) -> T {
    serde_json::from_value(body).expect("the arguments of the route")
}

#[tokio::test]
async fn a_capture_of_a_spec_waits_for_the_requests_lock() {
    let jojobot = handler();
    writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    waits_for_the_requests_lock(
        &jojobot,
        REQUEST,
        jojobot.capture(Parameters(CaptureArgs {
            sets: Some(
                [("desired".to_string(), "stopped".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args(REQUEST, "stop it")
        })),
    )
    .await;
}

#[tokio::test]
async fn the_creation_of_a_request_waits_for_the_requests_lock() {
    let jojobot = handler();
    writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    waits_for_the_requests_lock(
        &jojobot,
        "bot-request:alpha",
        jojobot.add_entity(Parameters(args(serde_json::json!({
            "kind": "bot-request", "handle": "alpha", "name": "Alpha asks",
            "source": "user-named", "sid": TEST_SID,
            "sets": {"target_bot": "bot:lisa", "desired": "running"},
        })))),
    )
    .await;
}

#[tokio::test]
async fn an_update_of_a_requests_spec_waits_for_the_requests_lock() {
    let jojobot = handler();
    writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    waits_for_the_requests_lock(
        &jojobot,
        REQUEST,
        jojobot.update_entity(Parameters(args(serde_json::json!({
            "handle": REQUEST, "sid": TEST_SID,
            "sets": {"desired": "stopped"},
        })))),
    )
    .await;
}

#[tokio::test]
async fn an_edit_of_a_requests_spec_claim_waits_for_the_requests_lock() {
    let jojobot = handler();
    writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    let claim = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("desired".to_string(), "stopped".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args(REQUEST, "stop it")
        },
    )
    .await;
    let address = claim["address"].as_str().expect("an address").to_string();
    waits_for_the_requests_lock(
        &jojobot,
        REQUEST,
        jojobot.update_fact(Parameters(crate::memory::UpdateFactArgs {
            sets: Some(
                [("desired".to_string(), "paused".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..crate::memory::testing::update_args(&address)
        })),
    )
    .await;
}

#[tokio::test]
async fn taking_back_a_requests_spec_claim_waits_for_the_requests_lock() {
    let jojobot = handler();
    writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    let claim = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("desired".to_string(), "stopped".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args(REQUEST, "stop it")
        },
    )
    .await;
    let address = claim["address"].as_str().expect("an address").to_string();
    waits_for_the_requests_lock(
        &jojobot,
        REQUEST,
        jojobot.retract(Parameters(args(serde_json::json!({
            "address": address, "reason": "written in error", "sid": TEST_SID,
        })))),
    )
    .await;
}

#[tokio::test]
async fn a_merge_into_a_request_waits_for_the_requests_lock() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    a_request_otto_manages(&jojobot).await;
    ensure(&jojobot, "topic:tarps").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("model".to_string(), "model-b".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("topic:tarps", "a model")
        },
    )
    .await;
    waits_for_the_requests_lock(
        &jojobot,
        REQUEST,
        jojobot.merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
            request_id: None,
            duplicate: "topic:tarps".into(),
            survivor: REQUEST.into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid),
        })),
    )
    .await;
}
