//! **Where the fold reads the claims State from.** The fold refreshes a touched
//! thing from the State a write has just changed, and falls back to the store
//! only when the State cannot answer (see [`ClaimsView::read`]).

use jojobot_domain::log::claims::State;

/// A holder of the claims State that lends it to a read.
pub trait ClaimsView: Send + Sync {
    /// Run `read` on the State and say so, or return `false` and run nothing
    /// when the State cannot answer: it is behind the log, or the import has
    /// not filled it yet. The caller then reads the store, slower and correct.
    fn read(&self, read: &mut dyn FnMut(&State)) -> bool;
}

impl ClaimsView for crate::ledger_seam::Seam {
    fn read(&self, read: &mut dyn FnMut(&State)) -> bool {
        self.read_claims_if_current(read)
    }
}

/// **The fake's State is always current**: it is fed by the same emitters, and
/// there is no log for it to fall behind.
#[cfg(any(test, feature = "testing"))]
impl ClaimsView for jojobot_domain::memory::testing::InMemoryMemory {
    fn read(&self, read: &mut dyn FnMut(&State)) -> bool {
        self.read_claims(|state| read(state));
        true
    }
}
