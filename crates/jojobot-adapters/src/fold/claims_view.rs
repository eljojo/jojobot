//! **Where the fold reads the claims State from.** The fold refreshes a touched
//! thing from the State a write has just changed, and falls back to the store
//! only when the State cannot answer (see [`ClaimsLender::lend`]).

use jojobot_domain::log::claims::{ClaimsLender, State};

/// The seam of the shared writer lends its State when the log is read to its end
/// and the import is in it.
impl ClaimsLender for crate::ledger_seam::Seam {
    fn lend(&self, read: &mut dyn FnMut(&State)) -> bool {
        self.read_claims_if_current(read)
    }
}
