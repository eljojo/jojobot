//! The in-memory outbox and a notifier that can be taken down, for every test
//! that is not about a real store.
//!
//! Behind the `testing` feature, so it compiles for tests here and downstream
//! but never ships in a production binary.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jiff::Timestamp;

use super::{Notification, Notifier, Outbox, Push, PushError, Undelivered};
use crate::mailbox::MessageId;

/// Where a row stands. The row is kept in every state: nothing is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Waiting for a first or a next try.
    Pending,
    /// Delivered.
    Sent,
    /// Given up on.
    Abandoned,
}

/// One row of the in-memory outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The push, as the port reads it.
    pub push: Push,
    /// Where it stands.
    pub standing: Standing,
}

/// The in-memory [`Outbox`]: rows held in a list.
#[derive(Default)]
pub struct InMemoryOutbox {
    rows: Mutex<Vec<Row>>,
}

impl InMemoryOutbox {
    /// An empty outbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// **Write the intent for one message**, due at once. The mailbox store calls
    /// this in the same step that stores the message.
    pub fn enqueue(&self, message: &MessageId, at: Timestamp) {
        self.rows.lock().expect("outbox lock").push(Row {
            push: Push {
                message: message.clone(),
                enqueued_at: at,
                attempts: 0,
                next_attempt_at: at,
            },
            standing: Standing::Pending,
        });
    }

    /// Every row, in the order they were written.
    pub fn rows(&self) -> Vec<Row> {
        self.rows.lock().expect("outbox lock").clone()
    }

    fn with_row(&self, message: &MessageId, edit: impl FnOnce(&mut Row)) {
        let mut rows = self.rows.lock().expect("outbox lock");
        if let Some(row) = rows.iter_mut().find(|r| &r.push.message == message) {
            edit(row);
        }
    }
}

#[async_trait]
impl Outbox for InMemoryOutbox {
    async fn due(&self, now: Timestamp) -> Result<Vec<Push>, PushError> {
        Ok(self
            .rows
            .lock()
            .expect("outbox lock")
            .iter()
            .filter(|r| r.standing == Standing::Pending && r.push.next_attempt_at <= now)
            .map(|r| r.push.clone())
            .collect())
    }

    async fn next_due(&self) -> Result<Option<Timestamp>, PushError> {
        Ok(self
            .rows
            .lock()
            .expect("outbox lock")
            .iter()
            .filter(|r| r.standing == Standing::Pending)
            .map(|r| r.push.next_attempt_at)
            .min())
    }

    async fn sent(&self, message: &MessageId, _at: Timestamp) -> Result<(), PushError> {
        self.with_row(message, |r| r.standing = Standing::Sent);
        Ok(())
    }

    async fn failed(
        &self,
        message: &MessageId,
        attempts: u32,
        next_attempt_at: Timestamp,
    ) -> Result<(), PushError> {
        self.with_row(message, |r| {
            r.push.attempts = attempts;
            r.push.next_attempt_at = next_attempt_at;
        });
        Ok(())
    }

    async fn abandoned(&self, message: &MessageId, _at: Timestamp) -> Result<(), PushError> {
        self.with_row(message, |r| r.standing = Standing::Abandoned);
        Ok(())
    }
}

/// **A notifier that records what it was given and can be taken down.** While it
/// is down every delivery fails; while it is up every delivery lands once.
pub struct FakeNotifier {
    down: AtomicBool,
    delivered: Mutex<Vec<Notification>>,
}

impl FakeNotifier {
    /// A notifier that is up.
    pub fn new() -> Self {
        FakeNotifier {
            down: AtomicBool::new(false),
            delivered: Mutex::new(Vec::new()),
        }
    }

    /// Fail every delivery from here.
    pub fn go_down(&self) {
        self.down.store(true, Ordering::SeqCst);
    }

    /// Deliver again.
    pub fn come_up(&self) {
        self.down.store(false, Ordering::SeqCst);
    }

    /// Everything delivered so far, in order.
    pub fn delivered(&self) -> Vec<Notification> {
        self.delivered.lock().expect("notifier lock").clone()
    }
}

impl Default for FakeNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Notifier for FakeNotifier {
    async fn deliver(&self, notification: &Notification) -> Result<(), Undelivered> {
        match self.down.load(Ordering::SeqCst) {
            true => Err(Undelivered("the fake notifier is down".into())),
            false => {
                self.delivered
                    .lock()
                    .expect("notifier lock")
                    .push(notification.clone());
                Ok(())
            }
        }
    }
}
