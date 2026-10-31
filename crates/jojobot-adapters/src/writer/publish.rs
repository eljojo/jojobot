//! What the writer publishes after a committed write: the changes the State
//! applied, once per write, to every subscriber (rule 676). A subscriber reads
//! current content from the State and never wraps a store port.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, Weak};

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

/// How the publication holds a subscriber. A subscriber that holds the writer
/// itself is held weakly, so the publication is not the owner that closes the
/// cycle back to the writer.
enum Held {
    Owned(Arc<dyn Subscriber>),
    Borrowed(Weak<dyn Subscriber>),
}

#[derive(Default)]
pub(super) struct Publication {
    subscribers: Mutex<Vec<Held>>,
    pending: Mutex<Vec<Change>>,
}

impl Publication {
    pub(super) fn subscribe(&self, subscriber: Arc<dyn Subscriber>) {
        self.subscribers
            .lock()
            .expect("subscriber lock")
            .push(Held::Owned(subscriber));
    }

    /// Deliver to a subscriber some other owner keeps alive. The delivery ends
    /// with that owner: a subscriber that is gone is dropped from the list.
    pub(super) fn subscribe_borrowed(&self, subscriber: Weak<dyn Subscriber>) {
        self.subscribers
            .lock()
            .expect("subscriber lock")
            .push(Held::Borrowed(subscriber));
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
        let subscribers: Vec<Arc<dyn Subscriber>> = {
            let mut held = self.subscribers.lock().expect("subscriber lock");
            held.retain(|held| match held {
                Held::Owned(_) => true,
                Held::Borrowed(weak) => weak.strong_count() > 0,
            });
            held.iter()
                .filter_map(|held| match held {
                    Held::Owned(subscriber) => Some(subscriber.clone()),
                    Held::Borrowed(weak) => weak.upgrade(),
                })
                .collect()
        };
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
