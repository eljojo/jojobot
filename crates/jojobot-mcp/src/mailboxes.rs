//! **Mailboxes** — the async rail between sessions: named boxes where one
//! session leaves a message another will find.
//!
//! One file per verb, each holding its arguments, its description and an
//! entrypoint. [`wire`] is the response vocabulary and [`declined`] the
//! refusals; the machinery with a single caller lives with that caller —
//! `my_box` in [`read_mailbox`]. `Ownership` is the exception and lives in
//! [`wire`] instead; see its doc comment for why.

use rmcp::{
    ErrorData as McpError, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
};

use crate::*;

pub mod declined;
pub mod list_sent;
pub mod mark_processed;
pub mod post_message;
pub mod read_mailbox;
pub mod read_message;
#[cfg(test)]
pub mod testing;
pub mod wire;

pub use list_sent::ListSentArgs;
pub use mark_processed::MarkProcessedArgs;
pub use post_message::PostMessageArgs;
pub use read_mailbox::ReadMailboxArgs;
pub use read_message::ReadMessageArgs;

pub(crate) use declined::*;
pub(crate) use wire::*;

/// This context's half of the surface — one router per verb file, summed.
pub(crate) fn router() -> ToolRouter<Jojobot> {
    Jojobot::list_sent_router()
        + Jojobot::mark_processed_router()
        + Jojobot::post_message_router()
        + Jojobot::read_mailbox_router()
        + Jojobot::read_message_router()
}
