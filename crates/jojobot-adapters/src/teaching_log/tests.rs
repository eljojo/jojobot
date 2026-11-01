use jojobot_domain::log::testing::{Fault, FaultLog, InMemoryLog};
use jojobot_domain::log::{LogStore, Position, ValidatedSnapshot};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};
use jojobot_domain::teaching::{DOMAIN_WIDTH, TeachingError};

use super::*;

pub(super) fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

pub(super) fn sid(name: &str) -> Sid {
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

/// A log that keeps the legacy rows as the Dolt log does: each batch's
/// statements apply with it, and a row inserted twice is refused, as the
/// table's key refuses it.
#[derive(Clone, Default)]
struct RowLog {
    log: InMemoryLog,
    rows: Arc<std::sync::Mutex<std::collections::BTreeMap<(String, String), String>>>,
}

#[async_trait::async_trait]
impl LogStore for RowLog {
    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        self.log.append(head, batch).await
    }
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.log.read_from(position, bound).await
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log.load_snapshot().await
    }
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.log.store_snapshot(snapshot).await
    }
}

#[async_trait::async_trait]
impl CompanionLog for RowLog {
    async fn append_with(
        &self,
        head: Position,
        batch: &SealedBatch,
        companions: Vec<Companion>,
    ) -> Result<Append, LogError> {
        let mut staged = self.rows.lock().unwrap().clone();
        for companion in &companions {
            if companion.sql.starts_with("INSERT") {
                let key = (companion.binds[0].clone(), companion.binds[1].clone());
                if staged.insert(key, companion.binds[2].clone()).is_some() {
                    return Ok(Append::DefinitelyNotCommitted);
                }
            } else {
                staged.retain(|(sid, _), _| *sid != companion.binds[0]);
            }
        }
        let appended = self.log.append(head, batch).await?;
        if matches!(appended, Append::Committed(_)) {
            *self.rows.lock().unwrap() = staged;
        }
        Ok(appended)
    }
}

fn row_keys(log: &RowLog) -> Vec<(String, String)> {
    log.rows.lock().unwrap().keys().cloned().collect()
}

/// **The legacy rows follow every write in the batch's own commit**, the import
/// adds none, and the shadow compares each contact and finds no disagreement.
#[tokio::test]
async fn the_shadow_keeps_the_rows_and_agrees() {
    let log = RowLog::default();
    let report = Arc::new(crate::fold::shadow::Shadow::named("teachings").unwrap());
    log.rows.lock().unwrap().insert(
        ("homer".to_string(), "claims".to_string()),
        at(3).to_string(),
    );
    let teachings = LoggedTeachings::open_shadowed(
        Arc::new(log.clone()),
        &[mark("homer", "claims", 3)],
        report.clone(),
    )
    .await
    .unwrap();
    let homer = vec![("homer".to_string(), "claims".to_string())];
    assert_eq!(row_keys(&log), homer);
    assert!(
        !teachings
            .first_contact(&sid("homer"), "claims", at(4))
            .await
            .unwrap()
    );
    assert!(
        teachings
            .first_contact(&sid("marge"), "mail", at(5))
            .await
            .unwrap()
    );
    assert!(
        teachings
            .first_contact(&sid("marge"), "claims", at(6))
            .await
            .unwrap()
    );
    assert_eq!(row_keys(&log).len(), 3);
    teachings.forget(&sid("marge")).await.unwrap();
    assert_eq!(row_keys(&log), homer);
    let counts = &report.counts()[0];
    assert_eq!((counts.reads, counts.disagreements), (3, 0));
}

/// **A deliberately divergent old set is detected**, with nonzero comparisons.
/// The boot repairs a divergence it finds, so this one appears after boot: the
/// old held set loses a mark the log still holds.
#[tokio::test]
async fn a_divergent_old_set_is_counted() {
    let log = RowLog::default();
    let report = Arc::new(crate::fold::shadow::Shadow::named("teachings").unwrap());
    let teachings = LoggedTeachings::open_shadowed(
        Arc::new(log.clone()),
        &[mark("homer", "claims", 3)],
        report.clone(),
    )
    .await
    .unwrap();
    teachings
        .held
        .lock()
        .await
        .shadow
        .as_mut()
        .expect("a shadowed ledger")
        .legacy
        .clear();
    teachings
        .first_contact(&sid("homer"), "claims", at(4))
        .await
        .ok();
    let counts = &report.counts()[0];
    assert_eq!((counts.reads, counts.disagreements), (1, 1));
}

/// **A rollback to the old path and a roll forward leaves the log and the rows
/// agreeing.** The old path adds a mark and clears a run in the rows only, the
/// log never sees either, and the next boot opens a log that is not empty. A
/// first contact on the pair the rows hold and the log lacks must answer false,
/// not build an insert the table refuses on every attempt.
#[tokio::test]
async fn a_rollback_and_roll_forward_reconciles_the_log_with_the_rows() {
    let log = RowLog::default();
    let report = Arc::new(crate::fold::shadow::Shadow::named("teachings").unwrap());
    let first = LoggedTeachings::open_shadowed(Arc::new(log.clone()), &[], report.clone())
        .await
        .unwrap();
    first
        .first_contact(&sid("marge"), "claims", at(1))
        .await
        .unwrap();
    first
        .first_contact(&sid("bart"), "claims", at(2))
        .await
        .unwrap();
    drop(first);
    {
        let mut rows = log.rows.lock().unwrap();
        rows.insert(("homer".to_string(), "mail".to_string()), at(5).to_string());
        rows.retain(|(run, _), _| run != "marge");
    }
    let marks: Vec<Mark> = log
        .rows
        .lock()
        .unwrap()
        .iter()
        .map(|((run, domain), taught_at)| Mark {
            sid: sid(run),
            domain: domain.clone(),
            taught_at: taught_at.parse().unwrap(),
        })
        .collect();
    let again = LoggedTeachings::open_shadowed(Arc::new(log.clone()), &marks, report.clone())
        .await
        .unwrap();
    assert_eq!(
        again.taught().await.unwrap().unwrap(),
        vec![
            (sid("bart"), "claims".to_string()),
            (sid("homer"), "mail".to_string())
        ],
        "the log holds what the rows hold"
    );
    assert!(
        !again
            .first_contact(&sid("homer"), "mail", at(6))
            .await
            .expect("a pair the rows hold is not a second insert"),
    );
    assert!(
        again
            .first_contact(&sid("marge"), "claims", at(7))
            .await
            .expect("a run the old path cleared is taught again"),
    );
    assert_eq!(report.counts()[0].disagreements, 0);
}
