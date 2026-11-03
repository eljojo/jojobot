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
    acknowledged: Notify,
    released: Notify,
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
    let replies = tokio::spawn({
        let (to_client, hold_reply, quiet, plan) = (
            to_client.clone(),
            hold_reply.clone(),
            quiet.clone(),
            plan.clone(),
        );
        async move {
            while let Ok(bytes) = packet(&mut from_server).await {
                if quiet.load(Ordering::SeqCst) {
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
        if command == 0x16
            && text.starts_with("KILL CONNECTION")
            && plan.delayed.lock().unwrap().is_some()
        {
            // The old connection cannot be reached: the kill does nothing.
            bytes = [&[9, 0, 0, bytes[3]][..], b"\x16SELECT 1"].concat();
        }
        if command == 0x03 && text == "COMMIT" {
            let fault = plan.fault.lock().unwrap().take_if(|f| {
                matches!(
                    f,
                    Fault::DelayedCommit
                        | Fault::CancelledAcknowledgement
                        | Fault::LostAcknowledgement
                )
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
                Some(Fault::LostAcknowledgement) => {
                    quiet.store(true, Ordering::SeqCst);
                    to_server.write_all(&bytes).await?;
                    let _ = to_client.lock().await.shutdown().await;
                    continue;
                }
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
