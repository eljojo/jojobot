//! **How jojobot answers** — the success envelope, and the one refusal that
//! belongs to no context.
//!
//! Two functions and one contract: every verb on this surface returns through
//! `json_result`, and a call whose arguments are each fine and wrong together
//! comes back through `misused`. The refusals that ARE a context's — a
//! near-miss handle, an unknown box, a closed session — live with that context.

use super::*;

/// **A call whose arguments are each fine and wrong together.** Not a malformed
/// call — every token parsed — so it is not a protocol error: it is a caller
/// mistake, and those are answers here.
///
/// **No `attempted` and no `candidates`, deliberately.** There is nothing that
/// was nearly right to name and nothing that nearly matched; what a caller needs
/// is the other call to make. [`session_unbound`] is the precedent — the shape
/// has always carried a candidate-free refusal, so this fits it rather than
/// stretching it into something that reads like a near miss.
pub(crate) fn misused(how_to_proceed: String) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// Render a JSON body as a successful tool result.
pub(crate) fn json_result(body: &serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]))
}
