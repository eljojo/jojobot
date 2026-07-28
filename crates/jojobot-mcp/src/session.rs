//! **Sessions** — one mortal run of a bot, keeping its own record.
//!
//! Three verbs, three files. [`wire`] renders a session and its chronology;
//! [`declined`] is the context's refusals. The machinery a session write runs
//! through — resolving the caller, materializing the card under the gate — is
//! not here: it belongs to whoever is calling, and lives in `caller`.

use rmcp::{
    ErrorData as McpError, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
};

use crate::*;

pub mod amend_journal;
pub mod declined;
pub mod journal;
#[cfg(test)]
pub mod testing;
pub mod wire;
pub mod wrap_session;

pub use amend_journal::AmendJournalArgs;
pub use journal::JournalArgs;
pub use wrap_session::WrapSessionArgs;

pub(crate) use declined::*;
pub(crate) use wire::*;

/// This context's half of the surface — one router per verb file, summed.
pub(crate) fn router() -> ToolRouter<Jojobot> {
    Jojobot::amend_journal_router() + Jojobot::journal_router() + Jojobot::wrap_session_router()
}
