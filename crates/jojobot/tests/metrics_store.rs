//! Store failures as the metrics endpoint reports them.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use jojobot::metrics::{Metrics, Ports, router};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewSession, Sessions, Sid};
use tower::ServiceExt;

async fn scrape(metrics: Arc<Metrics>, ports: Ports) -> String {
    let response = router(metrics, ports)
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("the metrics endpoint answers");
    assert_eq!(response.status(), 200);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    openmetrics_parser::openmetrics::parse_openmetrics(&text)
        .unwrap_or_else(|error| panic!("the scrape parses: {error}\n{text}"));
    text
}

fn live(text: &str) -> Option<f64> {
    text.lines()
        .find(|line| line.starts_with("jojobot_sessions_live"))
        .and_then(|line| line.split_whitespace().last()?.parse().ok())
}

/// A failed first read is unknown, and a failed later read cannot keep the
/// previous live count. Recovery serves the count again through the same route.
#[tokio::test]
async fn metrics_store_failed_session_refresh_clears_the_previous_sample() {
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    sessions
        .begin(NewSession {
            bot: EntityId("bot:marge".into()),
            sid: Sid("hmr1".into()),
            id: None,
            key_hash: None,
            focus: "working".into(),
            started_at: "2026-10-24T12:00:00Z".parse().unwrap(),
            timezone: None,
            started_on: None,
        })
        .await
        .expect("one live run");
    // A closed pool fails immediately without starting or contacting a store.
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .connect_lazy("mysql://localhost/jojobot")
        .unwrap();
    pool.close().await;
    let ports = Ports {
        sessions,
        memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
        mailboxes: Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ),
    };
    let failed = Ports {
        sessions: Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(pool)),
        ..ports.clone()
    };
    let metrics = Arc::new(Metrics::new().refreshing_every(Duration::ZERO));
    let first_failed = scrape(metrics.clone(), failed.clone()).await;
    let healthy = scrape(metrics.clone(), ports.clone()).await;
    let later_failed = scrape(metrics.clone(), failed).await;
    let recovered = scrape(metrics, ports).await;
    assert_eq!(
        [
            live(&first_failed),
            live(&healthy),
            live(&later_failed),
            live(&recovered)
        ],
        [None, Some(1.0), None, Some(1.0)],
        "failed refreshes must not report a default zero or the previous count"
    );
    assert!(
        later_failed.lines().any(|line| {
            line.starts_with("jojobot_scrape_read_ok{read=\"sessions\"}") && line.ends_with(" 0")
        }),
        "{later_failed}"
    );
}
