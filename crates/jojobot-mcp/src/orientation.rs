//! **Orientation** — coming online: what jojobot is, what exists right now,
//! and, when a bot is named, who you are and which run you are on.
//!
//! `start_here` is the one door and there is deliberately no second. The verb
//! file is thin; the work is next to it — [`orient`] assembles the world and
//! the snapshot, [`identity`] the charter, rules and owned box, [`attach`] the
//! session. [`essay`] is the prose itself.

use rmcp::{
    ErrorData as McpError, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
};

use crate::*;

pub mod attach;
pub mod essay;
pub mod identity;
pub mod orient;
pub mod ping;
pub mod start_here;

pub use start_here::OrientArgs;

pub(crate) use identity::booting_unknown;

/// This context's half of the surface — one router per verb file, summed.
pub(crate) fn router() -> ToolRouter<Jojobot> {
    Jojobot::ping_router() + Jojobot::start_here_router()
}
