//! **The role renewals a read leaves running after its answer.**
//!
//! A read of one's own mailbox renews the caller's role claims, and a renewal
//! that is due is a memory write. That write waits for the same gate every
//! write waits for, so a read that waited for it sat behind a long write, a merge
//! for instance, for as long as the write took. The read now starts the renewal
//! and answers. This holds which renewals are running, so a poll that arrives
//! while one runs starts no second one: a renewal only moves the lease moment
//! forward, so the one running is the one that counts.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// The renewals running now, one for each claimant at most.
#[derive(Default)]
pub(crate) struct RoleRenewals {
    running: Mutex<HashSet<String>>,
    ended: tokio::sync::Notify,
}

/// A renewal's place in [`RoleRenewals`]. Dropping it ends the renewal, whether
/// it finished, failed or was cut off with the runtime.
pub(crate) struct Running {
    renewals: Arc<RoleRenewals>,
    claimant: String,
}

impl RoleRenewals {
    /// **Start a renewal for `claimant`**, or `None` when one is running.
    pub(crate) fn begin(self: &Arc<Self>, claimant: &str) -> Option<Running> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.insert(claimant.to_string()).then(|| Running {
            renewals: Arc::clone(self),
            claimant: claimant.to_string(),
        })
    }

    /// Wait until no renewal is running. For cases that read the lease after a
    /// poll.
    #[cfg(test)]
    pub(crate) async fn settled(&self) {
        loop {
            let ended = self.ended.notified();
            tokio::pin!(ended);
            ended.as_mut().enable();
            if self
                .running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
            {
                return;
            }
            ended.await;
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.renewals
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.claimant);
        self.renewals.ended.notify_waiters();
    }
}
