//! The log-store contract and its fake; never present in production binaries.

use super::*;

mod fake;
pub use fake::InMemoryLog;
pub mod contract;

#[derive(Clone, Copy, Debug)]
pub enum Fault {
    BetweenMembers,
    LostAcknowledgement,
    DelayedCommit,
    CancelledAcknowledgement,
}

/// Fault controls belong to a test harness, not to the durable port. A real
/// backend implements these using its fault injector and terminal-fence protocol.
#[async_trait::async_trait]
pub trait FaultLog: LogStore {
    async fn inject(&self, fault: Fault);
    async fn acknowledgement_pending(&self);
    async fn finish_delayed(&self, commit: bool);
    async fn restart(&self);
}
