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

pub(super) async fn head(log: &dyn LogStore) -> u64 {
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
    contacts_and_clears_on(Arc::new(InMemoryLog::default())).await;
}

pub(super) async fn contacts_and_clears_on(log: Arc<dyn LogStore>) {
    let teachings = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
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
    assert_eq!(head(&*log).await, 2);
    assert!(
        !teachings
            .first_contact(&sid("homer"), "claims", at(2))
            .await
            .unwrap()
    );
    teachings.forget(&sid("lisa")).await.unwrap();
    assert_eq!(head(&*log).await, 2, "nothing appended");
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

#[derive(Default)]
struct PausedTail {
    log: InMemoryLog,
    pause: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl LogStore for PausedTail {
    async fn validity(&self) -> Result<jojobot_domain::log::Validity, LogError> {
        self.log.validity().await
    }

    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        self.log.append(head, batch).await
    }

    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        if self.pause.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.log.read_from(position, bound).await
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log.load_snapshot().await
    }

    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.log.store_snapshot(snapshot).await
    }
}

/// **Certain reads stay ready while another run waits for the log.** The
/// writer has entered a real tail read, so a free mutex before admission cannot
/// make this pass. Once committed, the same reads must see the new mark.
#[tokio::test]
async fn certain_teaching_reads_do_not_wait_for_another_runs_tail_read() {
    use futures_util::FutureExt;

    let log = Arc::new(PausedTail::default());
    let teachings = LoggedTeachings::open(log.clone(), &[mark("lisa", "claims", 0)])
        .await
        .unwrap();
    log.pause.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut contact = tokio::spawn({
        let teachings = teachings.clone();
        async move { teachings.first_contact(&sid("bart"), "mail", at(1)).await }
    });
    tokio::select! {
        () = log.entered.notified() => {}
        result = &mut contact => panic!("the contact must enter the paused tail read: {result:?}"),
    }
    let held = teachings.has_contact(&sid("lisa"), "claims").now_or_never();
    let absent = teachings
        .has_contact(&sid("homer"), "claims")
        .now_or_never();
    let taught = teachings.taught().now_or_never();
    log.release.notify_one();
    assert!(contact.await.unwrap().unwrap());
    assert!(matches!(held, Some(Ok(true))), "held: {held:?}");
    assert!(matches!(absent, Some(Ok(false))), "absent: {absent:?}");
    assert_eq!(
        taught.map(Result::unwrap),
        Some(Some(vec![(sid("lisa"), "claims".to_string())]))
    );
    assert!(teachings.has_contact(&sid("bart"), "mail").await.unwrap());
    assert_eq!(teachings.taught().await.unwrap().unwrap().len(), 2);
}

/// **A domain of 64 characters is admitted and one of 65 is refused with
/// nothing appended.**
#[tokio::test]
async fn the_domain_width_is_enforced_before_any_append() {
    domain_width_on(Arc::new(InMemoryLog::default())).await;
}

pub(super) async fn domain_width_on(log: Arc<dyn LogStore>) {
    let teachings = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
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
    assert_eq!(head(&*log).await, 1);
}

/// **A restart replays the log to the same ledger.**
#[tokio::test]
async fn a_restart_replays_to_the_same_ledger() {
    restart_replay_on(Arc::new(InMemoryLog::default())).await;
}

pub(super) async fn restart_replay_on(log: Arc<dyn LogStore>) {
    let first = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
    first
        .first_contact(&sid("homer"), "claims", at(0))
        .await
        .unwrap();
    first
        .first_contact(&sid("marge"), "mail", at(1))
        .await
        .unwrap();
    first.forget(&sid("marge")).await.unwrap();
    let second = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
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

/// **After an append whose outcome is unknown, the batch's runs refuse to read
/// and every other run serves.** The refusal lasts until a tail read the log
/// itself resolves; the next write that resolves it lifts it.
#[tokio::test]
async fn an_unresolved_append_makes_its_runs_refuse_and_leaves_the_others_serving() {
    let log = InMemoryLog::default();
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    teachings
        .first_contact(&sid("lisa"), "claims", at(0))
        .await
        .unwrap();
    log.inject(Fault::DelayedCommit).await;
    assert!(matches!(
        teachings
            .first_contact(&sid("homer"), "claims", at(1))
            .await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(matches!(
        teachings.has_contact(&sid("homer"), "claims").await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(matches!(
        teachings.taught().await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(teachings.has_contact(&sid("lisa"), "claims").await.unwrap());
    assert!(!teachings.has_contact(&sid("bart"), "claims").await.unwrap());
    log.finish_delayed(true).await;
    teachings
        .first_contact(&sid("bart"), "claims", at(2))
        .await
        .unwrap();
    assert!(
        teachings
            .has_contact(&sid("homer"), "claims")
            .await
            .unwrap()
    );
    assert_eq!(teachings.taught().await.unwrap().unwrap().len(), 3);
}

/// **Recovery runs straight after the unknown answer, not at the next write.**
/// The commit landed and its acknowledgement was lost; no other call follows.
#[tokio::test]
async fn an_unresolved_append_is_recovered_without_another_write() {
    let log = InMemoryLog::default();
    let teachings = LoggedTeachings::open(Arc::new(log.clone()), &[])
        .await
        .unwrap();
    log.inject(Fault::LostAcknowledgement).await;
    assert!(matches!(
        teachings
            .first_contact(&sid("homer"), "claims", at(0))
            .await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(
        teachings
            .has_contact(&sid("homer"), "claims")
            .await
            .unwrap()
    );
}

/// A log whose append commits and then answers `Err(LogError::Indeterminate)`,
/// the way the Dolt log does when its session loses the connection: the
/// in-memory log answers `Ok(Append::Indeterminate)` for every unknown outcome
/// and never takes this path.
#[derive(Clone, Default)]
struct ErrLog(InMemoryLog);

#[async_trait::async_trait]
impl LogStore for ErrLog {
    async fn validity(&self) -> Result<jojobot_domain::log::Validity, LogError> {
        self.0.validity().await
    }

    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        self.0.append(head, batch).await?;
        Err(LogError::Indeterminate)
    }
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.0.read_from(position, bound).await
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.0.load_snapshot().await
    }
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.0.store_snapshot(snapshot).await
    }
}

/// **An append that answers with the error is recovered like one that answers
/// with the unknown outcome.** The commit landed and the answer was lost as an
/// error; no other call follows. Paired with the case above, which loses the
/// acknowledgement as `Ok(Append::Indeterminate)`: both read the commit back.
#[tokio::test]
async fn an_append_that_errs_indeterminate_is_recovered() {
    let teachings = LoggedTeachings::open(Arc::new(ErrLog::default()), &[])
        .await
        .unwrap();
    assert!(matches!(
        teachings
            .first_contact(&sid("homer"), "claims", at(0))
            .await,
        Err(TeachingError::Indeterminate)
    ));
    assert!(
        teachings
            .has_contact(&sid("homer"), "claims")
            .await
            .unwrap(),
        "the commit that landed is read back, not served as 'not taught'"
    );
}

/// A batch holding one member whose envelope names `kind`, appended at the
/// log's head, as another family's writer would append it.
pub(super) async fn append_member_of_kind(log: &dyn LogStore, kind: &str) {
    append_members(log, vec![teaching_member_of_kind(kind)]).await;
}

/// A member of teaching's own family that names `kind`, built from teaching's
/// own envelope: it carries a run, as every teaching member does.
fn teaching_member_of_kind(kind: &str) -> Vec<u8> {
    let mut value: serde_json::Value =
        serde_json::from_slice(&codec::encode(&TeachingEvent::Cleared {
            run: RunId("homer".into()),
        }))
        .unwrap();
    value["event_kind"] = kind.into();
    serde_json::to_vec(&value).unwrap()
}

/// A member another family emits: the envelope of the first event the claims
/// import makes for one entity row, so it carries no run and the family's own
/// kind. The claims payload has no encoder yet, so the payload is a stand-in
/// object; teaching can pass the member only by reading the family before the
/// rest.
fn foreign_member() -> Vec<u8> {
    use jojobot_domain::log::ObjectId;
    use jojobot_domain::log::claims::import::{EntityRow, Rows, import};
    let rows = Rows {
        entities: vec![EntityRow {
            key: ObjectId("alpha".into()),
            handle: "person:alpha".into(),
            kind: "person".into(),
            name: "Alpha".into(),
            source: "fixture".into(),
            boot: None,
            parent: None,
            aliases: Vec::new(),
            crm: None,
            prose: String::new(),
            badge: None,
            archived: None,
            merged_into: None,
            taken_in: None,
        }],
        ..Rows::default()
    };
    let made = import(&rows).expect("one entity row imports").remove(0);
    let envelope = jojobot_domain::log::Event {
        event_kind: made.event_kind,
        meaning_version: made.meaning_version,
        actor: made.actor,
        run: made.run,
        session_card: made.session_card,
        request: made.request,
        part: made.part,
        occurred_at: made.occurred_at,
        written_at: made.written_at,
        imported_source_time: made.imported_source_time,
        payload: serde_json::json!({ "ObjectCreated": {} }),
    };
    serde_json::to_vec(&envelope).unwrap()
}

/// One batch of these members, appended at the log's head.
pub(super) async fn append_members(log: &dyn LogStore, members: Vec<Vec<u8>>) {
    let previous = Position(head(log).await);
    let batch = SealedBatch::seal(
        BatchId(format!("mixed:{}", previous.0)),
        previous,
        at(9),
        REPRESENTATION_VERSION,
        members,
    )
    .unwrap();
    assert!(matches!(
        log.append(previous, &batch).await.unwrap(),
        Append::Committed(_)
    ));
}

/// **A teaching member that shares a batch with another family's member is
/// applied, in either order, and the position moves past the whole batch.** A
/// replay that stopped at the foreign member would drop the teaching member that
/// follows it.
pub(super) async fn a_mixed_batch_is_applied(log: Arc<dyn LogStore>, foreign_first: bool) {
    let recorded = codec::encode(&TeachingEvent::Recorded {
        run: RunId("marge".into()),
        domain: "mail".into(),
        taught_at: at(3),
    });
    let foreign = foreign_member();
    let members = if foreign_first {
        vec![foreign, recorded]
    } else {
        vec![recorded, foreign]
    };
    append_members(&*log, members).await;
    let opened = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
    assert!(
        opened.has_contact(&sid("marge"), "mail").await.unwrap(),
        "the teaching member of the mixed batch is applied: {:?}",
        opened.taught().await
    );
    assert!(
        opened
            .first_contact(&sid("homer"), "claims", at(4))
            .await
            .unwrap(),
        "a write after the batch lands"
    );
    // The batch holds two events and the later write one.
    assert_eq!(
        head(&*log).await,
        3,
        "it appends at the head past the batch"
    );
}

#[tokio::test]
async fn a_teaching_member_after_a_foreign_one_in_one_batch_is_applied() {
    a_mixed_batch_is_applied(Arc::new(InMemoryLog::default()), true).await;
}

#[tokio::test]
async fn a_teaching_member_before_a_foreign_one_in_one_batch_is_applied() {
    a_mixed_batch_is_applied(Arc::new(InMemoryLog::default()), false).await;
}

/// **A member of another family is not an error for teaching.** The log holds
/// every family's batches in one order; teaching replays its own and passes the
/// rest, the position still moving past them, so a write after one lands.
#[tokio::test]
async fn a_member_of_another_family_leaves_the_replay_equal() {
    another_family_leaves_the_replay_equal(Arc::new(InMemoryLog::default())).await;
}

pub(super) async fn another_family_leaves_the_replay_equal(log: Arc<dyn LogStore>) {
    let first = LoggedTeachings::open(log.clone(), &[]).await.unwrap();
    first
        .first_contact(&sid("homer"), "claims", at(0))
        .await
        .unwrap();
    append_members(&*log, vec![foreign_member()]).await;
    let second = LoggedTeachings::open(log.clone(), &[])
        .await
        .expect("a foreign member does not stop the replay");
    assert_eq!(
        second.taught().await.unwrap(),
        first.taught().await.unwrap()
    );
    assert!(
        second
            .first_contact(&sid("marge"), "mail", at(2))
            .await
            .unwrap(),
        "a write after a foreign batch appends at the head past it"
    );
    assert_eq!(head(&*log).await, 3);
}

/// **An unknown kind of teaching's own family still fails the replay**, and
/// names the kind: meaning is fixed per kind, so a member that is teaching's
/// and not understood is never passed.
#[tokio::test]
async fn an_unknown_kind_of_teachings_own_family_fails_the_replay() {
    an_unknown_own_kind_fails_the_replay(Arc::new(InMemoryLog::default())).await;
}

pub(super) async fn an_unknown_own_kind_fails_the_replay(log: Arc<dyn LogStore>) {
    append_member_of_kind(&*log, "TeachingForgotten").await;
    let opened = LoggedTeachings::open(log, &[]).await;
    let Err(TeachingError::Store(why)) = opened else {
        panic!("the replay must fail on an unknown teaching kind");
    };
    assert!(why.contains("TeachingForgotten"), "{why}");
}
