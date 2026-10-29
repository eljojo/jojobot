//! What the writer publishes after a committed write: the changes the State
//! applied, once per write, to every subscriber (rule 676). A subscriber reads
//! current content from the State and never wraps a store port.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jojobot_domain::mailbox::MailboxName;

/// One change the State applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A message was written into this box.
    Posted(MailboxName),
}

/// A reader of the writer's publication. It runs in the writer task, after the
/// State holds the whole write, so it reads the State as the caller will.
#[async_trait]
pub trait Subscriber: Send + Sync {
    async fn changed(&self, changes: &[Change]);
}

#[derive(Default)]
pub(super) struct Publication {
    subscribers: Mutex<Vec<Arc<dyn Subscriber>>>,
    pending: Mutex<Vec<Change>>,
}

impl Publication {
    pub(super) fn subscribe(&self, subscriber: Arc<dyn Subscriber>) {
        self.subscribers
            .lock()
            .expect("subscriber lock")
            .push(subscriber);
    }

    pub(super) fn record(&self, change: Change) {
        self.pending.lock().expect("pending lock").push(change);
    }

    /// Hand the changes of the write that just finished to every subscriber.
    pub(super) async fn deliver(&self) {
        let changes = std::mem::take(&mut *self.pending.lock().expect("pending lock"));
        if changes.is_empty() {
            return;
        }
        let subscribers = self.subscribers.lock().expect("subscriber lock").clone();
        for subscriber in subscribers {
            subscriber.changed(&changes).await;
        }
    }
}
