//! What the writer publishes after a committed write: the changes the State
//! applied, once per write, to every subscriber (rule 676). A subscriber reads
//! current content from the State and never wraps a store port.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::FutureExt;
use jojobot_domain::mailbox::{MailboxName, MessageId};
use jojobot_domain::memory::{EntityId, WriteSummary};

/// One change the State applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A message was written into this box.
    Posted(MailboxName),
    /// These messages were written or changed. The position that follows in the
    /// same publication closes the write that changed them.
    Messages(Vec<MessageId>),
    /// The mail State's count of writes after one write finished. A write that
    /// changes no message publishes it alone.
    MailPosition(u64),
    /// The store's own change signal from before a write, published when the memory
    /// State held no image to count the write in.
    Began(WriteSummary),
    /// These things changed in the memory State, each to be read again from it.
    /// `names_changed` says a handle moved, so dependents re-read under the new
    /// names. `generation` is the State's change count after this change was
    /// applied, or `None` when the State held no image to count it in.
    Things {
        touched: Vec<EntityId>,
        names_changed: bool,
        generation: Option<u64>,
    },
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
        // A subscriber that panics must not take the change from the ones after
        // it: each is told, and the first panic is raised once all have been.
        let mut failed = None;
        for subscriber in subscribers {
            if let Err(panic) = AssertUnwindSafe(subscriber.changed(&changes))
                .catch_unwind()
                .await
            {
                failed.get_or_insert(panic);
            }
        }
        if let Some(panic) = failed {
            std::panic::resume_unwind(panic);
        }
    }
}
