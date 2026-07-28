//! **Memory** — what jojobot knows: typed entities, and dated claims about them.
//!
//! One file per verb. Each holds that verb's arguments, the description a
//! caller reads, and an entrypoint thin enough to read in one go: it parses the
//! call, chains the systems below it, and renders what came back. The logic
//! those bodies call is next door — [`parse`] the input grammar, [`wire`] the
//! response vocabulary, [`declined`] the refusals.

use rmcp::{
    ErrorData as McpError, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_router,
};

use crate::*;

pub mod add_entity;
pub mod capture;
pub mod declined;
pub mod list_entities;
pub mod parse;
pub mod recall;
pub mod search;
pub mod set_charter;
#[cfg(test)]
pub mod testing;
pub mod update_entity;
pub mod update_fact;
pub mod wire;

// **Re-exported for the verb files, which reach them through `use super::*`.**
// Named modules, so a reader asking "where does `blocked_result` live" gets an
// answer from the import list; the glob is a convenience at ONE hop, not a
// second home for the items.
pub use add_entity::AddEntityArgs;
pub use capture::CaptureArgs;
pub use list_entities::ListEntitiesArgs;
pub use recall::RecallArgs;
pub use search::{EdgeFilterArgs, SearchArgs};
pub use set_charter::SetCharterArgs;
pub use update_entity::UpdateEntityArgs;
pub use update_fact::UpdateFactArgs;

pub(crate) use declined::*;
pub(crate) use parse::*;
pub(crate) use wire::*;

/// This context's half of the surface — one router per verb file, summed.
///
/// **A verb is added by adding a file**, and it reaches the surface by being
/// named here. Nothing scans a directory: an unlisted verb is invisible, which
/// is the same deliberate friction `the_tool_surface_is_exactly_this_list` puts
/// in front of a new tool.
pub(crate) fn router() -> ToolRouter<Jojobot> {
    Jojobot::add_entity_router()
        + Jojobot::capture_router()
        + Jojobot::list_entities_router()
        + Jojobot::recall_router()
        + Jojobot::search_router()
        + Jojobot::set_charter_router()
        + Jojobot::update_entity_router()
        + Jojobot::update_fact_router()
}
