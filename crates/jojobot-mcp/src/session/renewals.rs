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

/// How long a renewal may run. A renewal that waits longer than this for the
/// store is cut off, so one hung write cannot hold its claimant's slot, and
/// with it every later poll's renewal, for the life of the process.
///
/// Short in this crate's own unit cases, so the case that holds it need not wait
/// a minute.
pub(crate) const RENEWAL_TIMEOUT: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(200)
} else {
    std::time::Duration::from_secs(60)
};

/// The renewals running now, one for each claimant at most.
#[derive(Default, Debug)]
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
