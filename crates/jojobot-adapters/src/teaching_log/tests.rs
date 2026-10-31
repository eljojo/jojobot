use jojobot_domain::log::testing::{Fault, FaultLog, InMemoryLog};
use jojobot_domain::log::{LogStore, Position};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};
use jojobot_domain::teaching::{DOMAIN_WIDTH, TeachingError};

use super::*;

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn sid(name: &str) -> Sid {
    Sid(name.to_string())
}

fn mark(name: &str, domain: &str, second: i64) -> Mark {
    Mark {
        sid: sid(name),
        domain: domain.to_string(),
        taught_at: at(second),
    }
}

async fn head(log: &InMemoryLog) -> u64 {
    log.read_from(Position::default(), 1000)
        .await
        .unwrap()
        .last()
        .map_or(0, |batch| batch.range().last.0)
}

/// **The stored spelling is pinned.** The kind and version are written into the
/// log and nothing outside this process declares them, so a rename here leaves
/// every stored record unreadable.
#[test]
fn the_stored_spelling_is_pinned() {
    let cleared = String::from_utf8(codec::encode(&TeachingEvent::Cleared {
        run: RunId("homer".into()),
    }))
    .unwrap();
    assert!(
        cleared.contains("\"event_kind\":\"TeachingCleared\""),
        "{cleared}"
    );
    assert!(cleared.contains("\"meaning_version\":1"), "{cleared}");
    let recorded = String::from_utf8(codec::encode(&TeachingEvent::Recorded {
        run: RunId("homer".into()),
        domain: "claims".into(),
        taught_at: at(0),
    }))
    .unwrap();
    assert!(
        recorded.contains("\"event_kind\":\"TeachingRecorded\""),
        "{recorded}"
    );
}

/// **An event survives its stored encoding**, and an unknown kind or version is
/// refused rather than skipped.
#[test]
fn an_event_survives_its_encoding_and_an_unknown_one_is_refused() {
    for event in [
        TeachingEvent::Recorded {
            run: RunId("homer".into()),
            domain: "claims".into(),
            taught_at: at(5),
        },
        TeachingEvent::Cleared {
            run: RunId("homer".into()),
        },
    ] {
        assert_eq!(codec::decode(&codec::encode(&event)).unwrap(), event);
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(&codec::encode(&TeachingEvent::Cleared {
            run: RunId("homer".into()),
        }))
        .unwrap();
    value["meaning_version"] = 2.into();
    assert!(codec::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    value["meaning_version"] = 1.into();
    value["event_kind"] = "TeachingForgotten".into();
    assert!(codec::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}

/// **The import keeps each mark's own source time and its run name, with no
/// session card anywhere**, and a second open on the same log imports nothing.
#[tokio::test]
async fn the_import_keeps_source_times_and_runs_once() {
    let log = InMemoryLog::default();
    let marks = [mark("marge", "mail", 9), mark("homer", "claims", 3)];
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &marks)
        .await
        .unwrap();
    assert_eq!(
        teachings.taught().await.unwrap().unwrap(),
        vec![
            (sid("homer"), "claims".to_string()),
            (sid("marge"), "mail".to_string())
        ]
    );
    let held = teachings.held.lock().await;
    let mut times: Vec<_> = held.state.history().to_vec();
    times.sort();
    assert_eq!(
        times,
        vec![
            (RunId("homer".into()), "claims".to_string(), at(3)),
            (RunId("marge".into()), "mail".to_string(), at(9)),
        ]
    );
    drop(held);
    let imported = head(&log).await;
    assert_eq!(imported, 2);
    let again = LoggedTeachings::open(Arc::new(log.clone()), &marks)
        .await
        .unwrap();
    assert_eq!(head(&log).await, imported);
    assert_eq!(
        again.taught().await.unwrap(),
        teachings.taught().await.unwrap()
    );
}

/// **A duplicate contact and an empty clear append nothing**; a clear then a
/// contact records once more; another run's marks are untouched.
#[tokio::test]
async fn contacts_and_clears_append_only_what_changes() {
    let log = InMemoryLog::default();
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    assert!(
        teachings
            .first_contact(&sid("homer"), "claims", at(0))
            .await
            .unwrap()
    );
    assert!(
        teachings
            .first_contact(&sid("bart"), "claims", at(1))
            .await
            .unwrap()
    );
    assert_eq!(head(&log).await, 2);
    assert!(
        !teachings
            .first_contact(&sid("homer"), "claims", at(2))
            .await
            .unwrap()
    );
    teachings.forget(&sid("lisa")).await.unwrap();
    assert_eq!(head(&log).await, 2, "nothing appended");
    teachings.forget(&sid("homer")).await.unwrap();
    assert!(
        !teachings
            .has_contact(&sid("homer"), "claims")
            .await
            .unwrap()
    );
    assert!(teachings.has_contact(&sid("bart"), "claims").await.unwrap());
    assert!(
        teachings
            .first_contact(&sid("homer"), "claims", at(3))
            .await
            .unwrap()
    );
}

/// **A domain of 64 characters is admitted and one of 65 is refused with
/// nothing appended.**
#[tokio::test]
async fn the_domain_width_is_enforced_before_any_append() {
    let log = InMemoryLog::default();
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    let fits = "d".repeat(DOMAIN_WIDTH);
    assert!(
        teachings
            .first_contact(&sid("homer"), &fits, at(0))
            .await
            .unwrap()
    );
    let wide = "d".repeat(DOMAIN_WIDTH + 1);
    assert!(matches!(
        teachings.first_contact(&sid("homer"), &wide, at(1)).await,
        Err(TeachingError::Refused(_))
    ));
    assert_eq!(head(&log).await, 1);
}

/// **A restart replays the log to the same ledger.**
#[tokio::test]
async fn a_restart_replays_to_the_same_ledger() {
    let log = InMemoryLog::default();
    let first = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    first
        .first_contact(&sid("homer"), "claims", at(0))
        .await
        .unwrap();
    first
        .first_contact(&sid("marge"), "mail", at(1))
        .await
        .unwrap();
    first.forget(&sid("marge")).await.unwrap();
    let second = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    assert_eq!(
        second.taught().await.unwrap(),
        first.taught().await.unwrap()
    );
    assert!(second.has_contact(&sid("homer"), "claims").await.unwrap());
}

/// **A commit whose outcome is unknown is not answered, and the next write is
/// refused until the log resolves it.** The contact is not reported as made.
#[tokio::test]
async fn an_unresolved_append_is_not_answered_and_blocks_the_next_write() {
    let log = InMemoryLog::default();
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    log.inject(Fault::DelayedCommit).await;
    assert!(matches!(
        teachings
            .first_contact(&sid("homer"), "claims", at(0))
            .await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(
        teachings
            .first_contact(&sid("bart"), "claims", at(1))
            .await
            .is_err()
    );
    log.finish_delayed(true).await;
    assert!(
        !teachings
            .first_contact(&sid("homer"), "claims", at(2))
            .await
            .unwrap(),
        "the commit that landed is read back from the log, so the mark is already held"
    );
}
