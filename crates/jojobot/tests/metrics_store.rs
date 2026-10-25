//! Store failures as the metrics endpoint reports them.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use jojobot::metrics::{Metrics, Ports, router};
use jojobot_domain::mailbox::{MailboxName, Mailboxes};
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

#[tokio::test]
async fn metrics_store_mailbox_labels_use_the_shipped_owner_not_the_stored_name() {
    jojobot_domain::memory::kinds::load(jojobot_domain::memory::kinds::SHIPPED);
    let mailboxes =
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    for (name, owner) in [
        ("quartzite", "bot:assistant"),
        ("defrag", "bot:defrag"),
        ("person-milhouse", "person:milhouse"),
    ] {
        mailboxes
            .create_mailbox(&MailboxName(name.into()), &EntityId(owner.into()), None)
            .await
            .unwrap()
            .written()
            .expect("the stored name and owner are accepted independently");
    }
    let text = scrape(
        Arc::new(Metrics::new()),
        Ports {
            mailboxes,
            memory: Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
            sessions: Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        },
    )
    .await;

    assert!(
        !text.contains("quartzite"),
        "stored names must not become labels"
    );
    for mailbox in ["assistant", "defrag", "operator"] {
        for state in ["new", "read", "processed"] {
            assert_eq!(
                sample(
                    &text,
                    "jojobot_mail_messages",
                    &[("mailbox", mailbox), ("state", state)]
                ),
                Some(0.0),
                "missing bounded mailbox label {mailbox}/{state}"
            );
        }
    }
}

const STORE_CALLS: &[(&str, &str)] = &[
    ("memory", "list_entities"),
    ("mailboxes", "list_mailboxes"),
    ("sessions", "all_summaries"),
    ("teachings", "has_contact"),
    ("requests", "lookup"),
];

/// Both successful and failed calls pass through the assembly's observation
/// boundary. The closed pool reproduces real adapter errors without a server.
async fn observed_calls() -> String {
    use jojobot_adapters::dolt::{
        mailboxes::DoltMailboxes, memory::DoltMemory, requests::DoltRequests,
        sessions::DoltSessions, teaching::DoltTeachings,
    };
    use jojobot_adapters::observe::Observation;
    use jojobot_domain::memory::MemoryError;
    use jojobot_domain::request::RequestId;

    let metrics = Arc::new(Metrics::new());
    let observation = Observation::new(metrics.clone());
    let memory = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let mailboxes =
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner());
    let teachings = Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new());
    let requests = jojobot_domain::request::testing::InMemoryRequests::new();
    let bot = EntityId("bot:marge".into());
    let sid = Sid("hmr1".into());
    let id = RequestId::parse("marble").unwrap();

    assert!(
        observation
            .memory(memory.clone())
            .list_entities(None)
            .await
            .is_ok()
    );
    assert!(
        observation
            .sessions(sessions.clone())
            .all_summaries()
            .await
            .is_ok()
    );
    assert!(
        observation
            .mailboxes(mailboxes.clone())
            .list_mailboxes()
            .await
            .is_ok()
    );
    assert!(
        !observation
            .teachings(teachings)
            .has_contact(&sid, "quartzite")
            .await
            .unwrap()
    );
    assert!(
        observation
            .requests(requests)
            .lookup(&bot, &id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        observation
            .memory(memory.clone())
            .recall(&EntityId("person:milhouse".into()))
            .await,
        Err(MemoryError::UnknownEntity { .. })
    ));
    assert!(
        observation
            .memory(memory.clone())
            .echoed_defaults(&EntityId("person:milhouse".into()), &Default::default())
            .await
            .is_empty()
    );

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .connect_lazy("mysql://localhost/jojobot")
        .unwrap();
    pool.close().await;
    let owners = Arc::new(jojobot_adapters::owners::MemoryOwners::new(memory.clone()));
    assert!(
        observation
            .memory(Arc::new(DoltMemory::open(pool.clone())))
            .list_entities(None)
            .await
            .is_err()
    );
    assert!(
        observation
            .mailboxes(Arc::new(DoltMailboxes::open(pool.clone(), owners)))
            .list_mailboxes()
            .await
            .is_err()
    );
    assert!(
        observation
            .sessions(Arc::new(DoltSessions::open(pool.clone())))
            .all_summaries()
            .await
            .is_err()
    );
    assert!(
        observation
            .teachings(Arc::new(DoltTeachings::open(pool.clone())))
            .has_contact(&sid, "quartzite")
            .await
            .is_err()
    );
    assert!(
        observation
            .requests(Arc::new(DoltRequests::new(pool)))
            .lookup(&bot, &id)
            .await
            .is_err()
    );

    scrape(
        metrics,
        Ports {
            memory,
            sessions,
            mailboxes,
        },
    )
    .await
}

fn sample(text: &str, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    text.lines()
        .find(|line| {
            line.starts_with(&format!("{name}{{"))
                && labels
                    .iter()
                    .all(|(key, value)| line.contains(&format!("{key}=\"{value}\"")))
        })
        .and_then(|line| line.split_whitespace().last()?.parse().ok())
}

#[tokio::test]
async fn metrics_store_observer_counts_store_errors_on_all_five_rails_only() {
    let text = observed_calls().await;
    for (rail, operation) in STORE_CALLS {
        assert_eq!(
            sample(
                &text,
                "jojobot_store_operation_errors_total",
                &[
                    ("rail", rail),
                    ("operation", operation),
                    ("outcome", "store_error"),
                ]
            ),
            Some(1.0),
            "{rail}.{operation}: {text}"
        );
        for outcome in ["ok", "refused"] {
            assert_eq!(
                sample(
                    &text,
                    "jojobot_store_operation_errors_total",
                    &[
                        ("rail", rail),
                        ("operation", operation),
                        ("outcome", outcome),
                    ]
                ),
                None,
                "{rail}.{operation}: {text}"
            );
        }
    }
    assert_eq!(
        sample(
            &text,
            "jojobot_store_operation_errors_total",
            &[("rail", "memory"), ("operation", "recall")]
        ),
        None,
        "a domain refusal is not a store error: {text}"
    );
}

#[tokio::test]
async fn metrics_store_observer_times_the_operation_on_all_five_rails() {
    let text = observed_calls().await;
    for (rail, operation) in STORE_CALLS {
        for outcome in ["ok", "store_error"] {
            let labels = [
                ("rail", *rail),
                ("operation", *operation),
                ("outcome", outcome),
            ];
            assert_eq!(
                sample(
                    &text,
                    "jojobot_store_operation_duration_seconds_count",
                    &labels
                ),
                Some(1.0),
                "{rail}.{operation}: {text}"
            );
            assert!(
                sample(
                    &text,
                    "jojobot_store_operation_duration_seconds_sum",
                    &labels
                )
                .is_some_and(|elapsed| elapsed > 0.0),
                "{rail}.{operation}: {text}"
            );
        }
    }
    assert_eq!(
        sample(
            &text,
            "jojobot_store_operation_duration_seconds_count",
            &[
                ("rail", "memory"),
                ("operation", "recall"),
                ("outcome", "refused"),
            ]
        ),
        Some(1.0),
        "domain refusals are timed too: {text}"
    );
    assert_eq!(
        sample(
            &text,
            "jojobot_store_operation_duration_seconds_count",
            &[
                ("rail", "memory"),
                ("operation", "echoed_defaults"),
                ("outcome", "ok"),
            ]
        ),
        Some(1.0),
        "a call without a fallible result is observed too: {text}"
    );
}

#[tokio::test]
async fn metrics_store_observer_labels_are_closed_and_carry_no_record_data() {
    let text = observed_calls().await;
    let exposition = openmetrics_parser::openmetrics::parse_openmetrics(&text).unwrap();
    let mut count = 0;
    for (family, metrics) in &exposition.families {
        if !family.starts_with("jojobot_store_operation") {
            continue;
        }
        for metric in metrics.iter_samples() {
            let labels: std::collections::BTreeMap<_, _> = metric
                .get_labelset()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let rail = labels.get("rail").expect("a rail label");
            let operation = labels.get("operation").expect("an operation label");
            assert!(
                STORE_CALLS.contains(&(rail.as_str(), operation.as_str()))
                    || (rail == "memory"
                        && matches!(operation.as_str(), "recall" | "echoed_defaults")),
                "{family}: {labels:?}"
            );
            for (key, value) in labels {
                assert!(
                    ![
                        "milhouse",
                        "marge",
                        "hmr1",
                        "marble",
                        "quartzite",
                        "localhost"
                    ]
                    .iter()
                    .any(|word| value.contains(word)),
                    "{family}: {key}={value}"
                );
                assert!(
                    matches!(key.as_str(), "rail" | "operation")
                        || (key == "outcome"
                            && matches!(value.as_str(), "ok" | "store_error" | "refused"))
                        || (key == "le" && (value == "+Inf" || value.parse::<f64>().is_ok())),
                    "{family}: {key}={value}"
                );
            }
            count += 1;
        }
    }
    assert!(
        count > 0,
        "the store observations reached the scrape: {text}"
    );
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
