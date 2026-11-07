//! **The complete-batch contract's fault cases, run on the log the writer owns.**
//! The writer holds the connection, so the faults are made on the wire: a proxy
//! between the writer and the store fails a statement, holds a COMMIT back, holds
//! an acknowledgement back, or keeps the writer's KILL from reaching the old
//! connection. The case bodies are the contract's, unchanged.

use super::*;
use crate::{
    dolt::Dolt,
    testing::{Scratch, free_port, migrated_database},
};
use jojobot_domain::log::testing::{Fault, FaultLog, contract};
use sqlx::mysql::MySqlPoolOptions;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::AsyncWriteExt,
    net::tcp::OwnedWriteHalf,
    sync::{Notify, mpsc, oneshot},
};

/// What the proxy does to the held-back COMMIT of a delayed commit.
enum Delayed {
    /// Send it to the store, then close the connection.
    Commit(oneshot::Sender<()>),
    /// Close the connection without it: the store rolls the transaction back.
    Abort(oneshot::Sender<()>),
}

#[derive(Default)]
struct Plan {
    fault: Mutex<Option<Fault>>,
    /// Present while a COMMIT is held back by the proxy.
    delayed: Mutex<Option<mpsc::UnboundedSender<Delayed>>>,
    /// How many `KILL CONNECTION` statements reached the proxy: only a recovery
    /// that fences an old connection sends one.
    kills: std::sync::atomic::AtomicUsize,
    acknowledged: Notify,
    released: Notify,
    native: Option<Arc<NativeGate>>,
    refuse_event: AtomicBool,
}

struct NativeGate {
    observer: sqlx::MySqlPool,
    armed: AtomicBool,
    old_id: std::sync::atomic::AtomicU64,
    commits: std::sync::atomic::AtomicUsize,
    finished: Notify,
    early_read: AtomicBool,
    killed: AtomicBool,
}

fn error_packet(message: &str) -> Vec<u8> {
    let mut payload = vec![0xff];
    payload.extend(1105u16.to_le_bytes());
    payload.extend(b"#HY000");
    payload.extend(message.as_bytes());
    let mut bytes = (payload.len() as u32).to_le_bytes()[..3].to_vec();
    bytes.push(1);
    bytes.extend(payload);
    bytes
}

async fn serve(
    client: tokio::net::TcpStream,
    upstream: u16,
    plan: Arc<Plan>,
) -> std::io::Result<()> {
    use crate::dolt_log::tests::packet;
    let upstream = tokio::net::TcpStream::connect(("127.0.0.1", upstream)).await?;
    let (mut from_client, to_client) = client.into_split();
    let (mut from_server, mut to_server) = upstream.into_split();
    let to_client = Arc::new(tokio::sync::Mutex::new(to_client));
    let hold_reply = Arc::new(AtomicBool::new(false));
    let quiet = Arc::new(AtomicBool::new(false));
    let native_reply = Arc::new(AtomicBool::new(false));
    let kill_reply = Arc::new(AtomicBool::new(false));
    let mut kill_statement = false;
    let replies = tokio::spawn({
        let (to_client, hold_reply, quiet, plan) = (
            to_client.clone(),
            hold_reply.clone(),
            quiet.clone(),
            plan.clone(),
        );
        let native_reply = native_reply.clone();
        let kill_reply = kill_reply.clone();
        async move {
            while let Ok(bytes) = packet(&mut from_server).await {
                if kill_reply.swap(false, Ordering::SeqCst)
                    && bytes.get(4) == Some(&0)
                    && let Some(native) = &plan.native
                {
                    native.killed.store(true, Ordering::SeqCst);
                }
                if quiet.load(Ordering::SeqCst) {
                    // The gated command contains only SET and COMMIT. Their OK
                    // packets carry zero affected rows and last-insert id; bit
                    // 3 says another result follows. Keep the server connection
                    // alive until its final result, even after the client left.
                    if native_reply.load(Ordering::SeqCst)
                        && (bytes.get(4) == Some(&0xff)
                            || (bytes.get(4..7) == Some(&[0, 0, 0])
                                && bytes.get(7).is_some_and(|status| status & 8 == 0)))
                    {
                        break;
                    }
                    continue;
                }
                if hold_reply.swap(false, Ordering::SeqCst) {
                    plan.acknowledged.notify_one();
                    plan.released.notified().await;
                }
                if to_client.lock().await.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
    });
    loop {
        let mut bytes = packet(&mut from_client).await?;
        let text = String::from_utf8_lossy(&bytes[5..]).into_owned();
        let command = bytes[4];
        if command == 0x16 {
            kill_statement = text.starts_with("KILL CONNECTION");
        }
        if command == 0x17 && kill_statement {
            kill_reply.store(true, Ordering::SeqCst);
        }
        if command == 0x16
            && text.contains("FROM event_log")
            && plan.kills.load(Ordering::SeqCst) > 0
            && let Some(native) = &plan.native
        {
            let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK('alpha-ready')")
                .fetch_one(&native.observer)
                .await
                .expect("observe the old command before a recovery read");
            if owner == Some(native.old_id.load(Ordering::SeqCst)) {
                native.early_read.store(true, Ordering::SeqCst);
            }
        }
        if command == 0x16
            && text.contains("INSERT INTO event_log")
            && plan.refuse_event.swap(false, Ordering::SeqCst)
        {
            to_client
                .lock()
                .await
                .write_all(&error_packet("refused event"))
                .await?;
            continue;
        }
        if command == 0x16 && text.contains("INSERT INTO session_teaching") {
            let armed = plan
                .fault
                .lock()
                .unwrap()
                .take_if(|f| matches!(f, Fault::BetweenMembers))
                .is_some();
            if armed {
                to_client
                    .lock()
                    .await
                    .write_all(&error_packet("refused row"))
                    .await?;
                continue;
            }
        }
        if command == 0x16 && text.starts_with("KILL CONNECTION") {
            plan.kills.fetch_add(1, Ordering::SeqCst);
        }
        if command == 0x16
            && text.starts_with("KILL CONNECTION")
            && plan.delayed.lock().unwrap().is_some()
        {
            // The old connection cannot be reached: the kill does nothing.
            bytes = [&[9, 0, 0, bytes[3]][..], b"\x16SELECT 1"].concat();
        }
        if command == 0x03 && text == "COMMIT" {
            if let Some(native) = &plan.native {
                native.commits.fetch_add(1, Ordering::SeqCst);
                if native.armed.swap(false, Ordering::SeqCst) {
                    let query = b"\x03SET @ready = GET_LOCK('alpha-ready', 0); \
                        SET @gate = GET_LOCK('alpha-gate', 30); COMMIT; \
                        SET @released = RELEASE_LOCK('alpha-gate'); \
                        SET @done = RELEASE_LOCK('alpha-ready')";
                    let mut gated = (query.len() as u32).to_le_bytes()[..3].to_vec();
                    gated.push(bytes[3]);
                    gated.extend(query);
                    quiet.store(true, Ordering::SeqCst);
                    native_reply.store(true, Ordering::SeqCst);
                    to_server.write_all(&gated).await?;
                    let entered = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            let owner: Option<u64> =
                                sqlx::query_scalar("SELECT IS_USED_LOCK('alpha-ready')")
                                    .fetch_one(&native.observer)
                                    .await
                                    .expect("observe server entry");
                            if let Some(owner) = owner {
                                break owner;
                            }
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .expect("the server entered the gated COMMIT");
                    native.old_id.store(entered, Ordering::SeqCst);
                    let _ = to_client.lock().await.shutdown().await;
                    let native = native.clone();
                    tokio::spawn(async move {
                        let _ = replies.await;
                        let _ = to_server.shutdown().await;
                        native.finished.notify_one();
                    });
                    return Ok(());
                }
            }
            let fault =
                plan.fault.lock().unwrap().take_if(|f| {
                    matches!(f, Fault::DelayedCommit | Fault::CancelledAcknowledgement)
                });
            match fault {
                Some(Fault::DelayedCommit) => {
                    let (commands, held) = mpsc::unbounded_channel();
                    *plan.delayed.lock().unwrap() = Some(commands);
                    quiet.store(true, Ordering::SeqCst);
                    let _ = to_client.lock().await.shutdown().await;
                    tokio::spawn(hold(bytes, to_server, held, replies, plan.clone()));
                    return Ok(());
                }
                Some(Fault::CancelledAcknowledgement) => hold_reply.store(true, Ordering::SeqCst),
                _ => {}
            }
        }
        to_server.write_all(&bytes).await?;
    }
}

/// The held-back COMMIT waits here, on a connection the store still sees open
/// and the client has lost, until the test sends it or abandons it.
async fn hold(
    commit: Vec<u8>,
    mut to_server: OwnedWriteHalf,
    mut commands: mpsc::UnboundedReceiver<Delayed>,
    replies: tokio::task::JoinHandle<()>,
    plan: Arc<Plan>,
) {
    let done = match commands.recv().await {
        Some(Delayed::Commit(done)) => {
            let _ = to_server.write_all(&commit).await;
            Some(done)
        }
        Some(Delayed::Abort(done)) => Some(done),
        None => None,
    };
    let _ = to_server.shutdown().await;
    let _ = replies.await;
    *plan.delayed.lock().unwrap() = None;
    if let Some(done) = done {
        let _ = done.send(());
    }
}

struct ReleaseOnDrop(Arc<Plan>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.released.notify_one();
    }
}

/// A [`WriterLog`] behind the fault proxy.
struct Faulted {
    plan: Arc<Plan>,
    store: sqlx::MySqlPool,
    port: u16,
    current: Mutex<Option<(WriterLog, Writer)>>,
}

impl Faulted {
    async fn open(store: sqlx::MySqlPool, port: u16) -> Result<(WriterLog, Writer), LogError> {
        let source = MySqlPoolOptions::new()
            .connect_lazy_with((*store.connect_options()).clone().port(port));
        let writer = Writer::default();
        Ok((WriterLog::open(source, writer.clone()).await?, writer))
    }

    fn log(&self) -> WriterLog {
        self.current.lock().unwrap().as_ref().unwrap().0.clone()
    }

    async fn end_delayed(&self, commit: bool) {
        let commands = self.plan.delayed.lock().unwrap().clone();
        if let Some(commands) = commands {
            let (done, finished) = oneshot::channel();
            let command = if commit {
                Delayed::Commit(done)
            } else {
                Delayed::Abort(done)
            };
            if commands.send(command).is_ok() {
                let _ = finished.await;
            }
        }
    }
}

#[async_trait::async_trait]
impl LogStore for Faulted {
    async fn validity(&self) -> Result<Validity, LogError> {
        self.log().validity().await
    }

    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        let fault = *self.plan.fault.lock().unwrap();
        let _release = matches!(fault, Some(Fault::CancelledAcknowledgement))
            .then(|| ReleaseOnDrop(self.plan.clone()));
        if matches!(fault, Some(Fault::BetweenMembers)) {
            // The batch has a second member: a legacy row, which the proxy refuses.
            let row = RowChange::Record {
                sid: "homer".into(),
                domain: "claims".into(),
                taught_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            };
            return self.log().append_with_rows(head, batch, vec![row]).await;
        }
        self.log().append(head, batch).await
    }
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.log().read_from(position, bound).await
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log().load_snapshot().await
    }
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.log().store_snapshot(snapshot).await
    }
}

#[async_trait::async_trait]
impl FaultLog for Faulted {
    async fn inject(&self, fault: Fault) {
        *self.plan.fault.lock().unwrap() = Some(fault);
    }
    async fn acknowledgement_pending(&self) {
        self.plan.acknowledged.notified().await;
    }
    async fn finish_delayed(&self, commit: bool) {
        self.end_delayed(commit).await;
    }
    /// The process dies: its connection closes, the store rolls the transaction
    /// back and frees the lock, and a successor opens on the same store.
    async fn restart(&self) {
        self.end_delayed(false).await;
        drop(self.current.lock().unwrap().take());
        let opened = Self::open(self.store.clone(), self.port).await;
        *self.current.lock().unwrap() = Some(opened.unwrap());
    }
}

macro_rules! case {
    ($($name:ident),+ $(,)?) => {$(
        #[tokio::test]
        async fn $name() {
            let scratch = Scratch::new("writer-log-faults");
            let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
            let store = migrated_database(&server, "alpha").await.unwrap();
            let plan = Arc::new(Plan::default());
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let upstream = store.connect_options().get_port();
            let proxy = tokio::spawn({
                let plan = plan.clone();
                async move {
                    while let Ok((client, _)) = listener.accept().await {
                        tokio::spawn(serve(client, upstream, plan.clone()));
                    }
                }
            });
            let current = Faulted::open(store.clone(), port).await.unwrap();
            let log = Faulted { plan, store, port, current: Mutex::new(Some(current)) };
            contract::$name(&log).await;
            drop(log);
            proxy.abort();
            server.stop().await;
        }
    )+};
}

case!(
    partial_batch_is_invisible,
    absence_without_a_terminal_fence_is_uncertain,
    a_fenced_abort_leaves_the_same_head,
    restart_excludes_a_late_predecessor,
    cancelling_an_acknowledgement_does_not_cancel_the_commit,
);

fn committed(id: &str) -> jojobot_domain::request::events::LedgerEvent {
    use jojobot_domain::log::{ObjectId, RequestId};
    use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};
    let at = jiff::Timestamp::from_second(1_700_000_000).unwrap();
    LedgerEvent::RequestCommitted(Commit {
        request: Request {
            key: RequestKey {
                actor: ObjectId("bot:homer".into()),
                id: RequestId(id.into()),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest-alpha".into(),
        },
        landed: "{}".into(),
        receipt: None,
        written_at: at,
        expires_at: at + std::time::Duration::from_secs(60),
    })
}

/// The proxy sends COMMIT into Dolt's own GET_LOCK gate before losing the
/// acknowledgement. KILL can answer while that command is still executing.
async fn teaching_native_gate(release_before_receipt: bool) {
    use crate::teaching_log::LoggedTeachings;
    use jojobot_domain::session::Sid;
    use jojobot_domain::teaching::{TeachingError, Teachings};

    let scratch = Scratch::new("teaching-native-fence");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let native = Arc::new(NativeGate {
        observer: store.clone(),
        armed: AtomicBool::new(false),
        old_id: std::sync::atomic::AtomicU64::new(0),
        commits: std::sync::atomic::AtomicUsize::new(0),
        finished: Notify::new(),
        early_read: AtomicBool::new(false),
        killed: AtomicBool::new(false),
    });
    let plan = Arc::new(Plan {
        native: Some(native.clone()),
        ..Plan::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = store.connect_options().get_port();
    let proxy = tokio::spawn({
        let plan = plan.clone();
        async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(serve(client, upstream, plan.clone()));
            }
        }
    });
    let (log, writer) = Faulted::open(store.clone(), port).await.unwrap();
    let log = Arc::new(log);
    let teachings = LoggedTeachings::open(log.clone(), &[])
        .await
        .unwrap()
        .with_writer(writer.clone());
    let control = Sid("homer".into());
    let pending = Sid("marge".into());
    let next = Sid("bart".into());
    let at = jiff::Timestamp::UNIX_EPOCH;
    teachings
        .first_contact(&control, "claims", at)
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&store)
        .await
        .unwrap();
    let mut gate = store.acquire().await.unwrap();
    let locked: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('alpha-gate', 0)")
        .fetch_one(&mut *gate)
        .await
        .unwrap();
    assert_eq!(locked, Some(1));
    native.armed.store(true, Ordering::SeqCst);
    let mut caller = tokio::spawn({
        let teachings = teachings.clone();
        let pending = pending.clone();
        async move { teachings.first_contact(&pending, "claims", at).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !native.killed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Dolt acknowledged recovery's KILL before the old command ended");
    tokio::select! {
        () = writer.fence_contended() => {},
        answer = &mut caller => panic!("the receipt returned without waiting on the old connection: {answer:?}"),
        () = tokio::time::sleep(std::time::Duration::from_secs(5)) => panic!("recovery never requested the native fence wait"),
    }
    let old = native.old_id.load(Ordering::SeqCst);
    assert_ne!(old, 0);
    let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK('alpha-ready')")
        .fetch_one(&store)
        .await
        .unwrap();
    assert_eq!(owner, Some(old), "the old command is executing inside Dolt");
    assert!(writer.rail_uncertain("event-log"));
    assert_eq!(writer.recovery_counts().recovered, 0);
    assert!(teachings.has_contact(&control, "claims").await.unwrap());
    assert!(!teachings.has_contact(&pending, "claims").await.unwrap());
    let during: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&store)
        .await
        .unwrap();
    assert_eq!(during, before, "the gated batch is not committed yet");
    if release_before_receipt {
        sqlx::query("SELECT RELEASE_LOCK('alpha-gate')")
            .execute(&mut *gate)
            .await
            .unwrap();
    }
    let answer = tokio::time::timeout(std::time::Duration::from_secs(15), &mut caller)
        .await
        .expect("bounded uncertain receipt")
        .unwrap();
    assert!(
        matches!(answer, Err(TeachingError::Indeterminate)),
        "{answer:?}"
    );
    if !release_before_receipt {
        assert!(writer.rail_uncertain("event-log"));
        // The native append tries once; teaching's four eager tail reads each
        // retry that same still-held fence before returning its receipt.
        assert_eq!(writer.recovery_counts().failed, 5);
        assert_eq!(writer.recovery_counts().recovered, 0);
        let commits = native.commits.load(Ordering::SeqCst);
        assert!(teachings.first_contact(&next, "claims", at).await.is_err());
        assert_eq!(
            native.commits.load(Ordering::SeqCst),
            commits,
            "an uncertain rail accepted a write"
        );
        assert!(teachings.has_contact(&control, "claims").await.unwrap());
        assert!(matches!(
            teachings.has_contact(&pending, "claims").await,
            Err(TeachingError::Indeterminate)
        ));
        sqlx::query("SELECT RELEASE_LOCK('alpha-gate')")
            .execute(&mut *gate)
            .await
            .unwrap();
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        native.finished.notified(),
    )
    .await
    .expect("old native command ends");
    if !release_before_receipt {
        assert!(teachings.first_contact(&next, "claims", at).await.unwrap());
        assert!(teachings.has_contact(&next, "claims").await.unwrap());
    }
    assert!(!writer.rail_uncertain("event-log"));
    assert_eq!(writer.recovery_counts().recovered, 1);
    assert!(
        !native.early_read.load(Ordering::SeqCst),
        "recovery read the log before the old server command ended"
    );
    let replay = LoggedTeachings::open(log, &[]).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&store)
        .await
        .unwrap();
    let next_batches = i64::from(!release_before_receipt);
    assert!((before + next_batches..=before + next_batches + 1).contains(&after));
    let pending_committed = after == before + next_batches + 1;
    assert!(replay.has_contact(&control, "claims").await.unwrap());
    assert_eq!(
        replay.has_contact(&next, "claims").await.unwrap(),
        !release_before_receipt
    );
    assert_eq!(
        replay.has_contact(&pending, "claims").await.unwrap(),
        pending_committed,
        "the pending mark follows the native transaction's persisted outcome"
    );
    assert_eq!(
        teachings.has_contact(&pending, "claims").await.unwrap(),
        replay.has_contact(&pending, "claims").await.unwrap()
    );
    drop(teachings);
    writer.drain().await;
    proxy.abort();
    server.stop().await;
}

#[tokio::test]
async fn teaching_native_fence_precedes_reload_even_when_kill_returns_first() {
    teaching_native_gate(true).await;
}

#[tokio::test]
async fn teaching_native_fence_timeout_keeps_the_rail_uncertain_and_published() {
    teaching_native_gate(false).await;
}

/// **A standalone ledger commit whose answer is lost closes the old connection
/// before anything reads the tail.** The writer's recovery fences the connection
/// with a `KILL CONNECTION`; only a commit the writer classifies as uncertain
/// sends one. An acknowledged standalone commit sends none, so the control shows
/// the count moves for this cause alone.
#[tokio::test]
async fn a_lost_standalone_ledger_commit_fences_the_old_connection() {
    let scratch = Scratch::new("writer-log-faults");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let plan = Arc::new(Plan::default());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = store.connect_options().get_port();
    let proxy = tokio::spawn({
        let plan = plan.clone();
        async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(serve(client, upstream, plan.clone()));
            }
        }
    });
    let pool = crate::writer::sql::Pool::from(
        MySqlPoolOptions::new().connect_lazy_with((*store.connect_options()).clone().port(port)),
    );
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the ledger opens")
    };
    let at = jiff::Timestamp::now();

    // Control: an acknowledged commit fences nothing.
    {
        let (seam, pool) = (seam.clone(), pool.clone());
        writer
            .run(async move {
                seam.append_event_batch(&pool, vec![committed("alpha")], at)
                    .await
            })
            .await
            .expect("an acknowledged commit lands");
    }
    assert_eq!(plan.kills.load(Ordering::SeqCst), 0);

    // The COMMIT of the next one is held back and the connection is lost.
    *plan.fault.lock().unwrap() = Some(Fault::DelayedCommit);
    let lost = tokio::spawn({
        let (seam, pool, writer) = (seam.clone(), pool.clone(), writer.clone());
        async move {
            writer
                .run(async move {
                    seam.append_event_batch(&pool, vec![committed("beta")], at)
                        .await
                })
                .await
        }
    });
    let fenced = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while plan.kills.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    // Release the held COMMIT whatever the verdict, so nothing waits on it.
    let commands = plan.delayed.lock().unwrap().clone();
    if let Some(commands) = commands {
        let (done, finished) = oneshot::channel();
        if commands.send(Delayed::Abort(done)).is_ok() {
            let _ = finished.await;
        }
    }
    let answer = tokio::time::timeout(std::time::Duration::from_secs(30), lost)
        .await
        .expect("the job ends")
        .expect("the task ends");
    assert!(
        matches!(answer, Err(crate::ledger_log::LedgerError::Indeterminate)),
        "{answer:?}"
    );
    assert!(
        fenced.is_ok(),
        "the lost commit sent no KILL CONNECTION: the old connection was never fenced"
    );
    proxy.abort();
    server.stop().await;
}

#[tokio::test]
async fn teaching_definite_native_rollback_keeps_the_published_rail() {
    use crate::teaching_log::LoggedTeachings;
    use jojobot_domain::session::Sid;
    use jojobot_domain::teaching::{TeachingError, Teachings};
    let scratch = Scratch::new("teaching-definite-rollback");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let plan = Arc::new(Plan::default());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = store.connect_options().get_port();
    let proxy = tokio::spawn({
        let plan = plan.clone();
        async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(serve(client, upstream, plan.clone()));
            }
        }
    });
    let source = MySqlPoolOptions::new().connect_lazy_with(
        (*store.connect_options())
            .clone()
            .port(port)
            .statement_cache_capacity(0),
    );
    let writer = Writer::default();
    let log = Arc::new(WriterLog::open(source, writer.clone()).await.unwrap());
    let teachings = LoggedTeachings::open(log.clone(), &[])
        .await
        .unwrap()
        .with_writer(writer.clone());
    let control = Sid("homer".into());
    let pending = Sid("marge".into());
    let at = jiff::Timestamp::UNIX_EPOCH;
    assert!(
        teachings
            .first_contact(&control, "claims", at)
            .await
            .unwrap()
    );
    let head = log.read_from(Position(0), 100).await.unwrap();
    plan.refuse_event.store(true, Ordering::SeqCst);
    assert!(matches!(
        teachings.first_contact(&pending, "claims", at).await,
        Err(TeachingError::Store(_))
    ));
    assert!(!writer.rail_uncertain("event-log"));
    assert_eq!(
        writer.recovery_counts().attempted,
        0,
        "a definite failure reloaded the published rail"
    );
    assert!(teachings.has_contact(&control, "claims").await.unwrap());
    assert!(!teachings.has_contact(&pending, "claims").await.unwrap());
    assert_eq!(
        log.read_from(Position(0), 100).await.unwrap(),
        head,
        "the failed transaction left a batch"
    );
    assert!(
        teachings
            .first_contact(&pending, "claims", at)
            .await
            .unwrap()
    );
    writer.drain().await;
    proxy.abort();
    server.stop().await;
}
