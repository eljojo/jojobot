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

/// **Replace prose the caller just wrote with what the caller does not have.**
///
/// A write verb answers with a receipt: enough to know the write landed, to
/// recognize WHICH write it was, and to address it later — and none of the
/// body, because its author is the one reader it teaches nothing.
///
/// `post_message` is the shape this follows and the reason it is one function:
/// four keys in a fixed relation — the payload gone, a marker saying so, the
/// byte count of what was stored, and enough of the opening to tell two writes
/// apart. **Eliding is never silent**, so `how_to_read` names the call that
/// returns the whole thing; a reader that had to infer withheld-from-absent
/// would eventually infer wrong.
///
/// The byte count is of what was STORED, so a caller comparing it against what
/// it sent learns that the store trimmed it.
pub(crate) fn elide_prose(body: &mut serde_json::Value, key: &str, prose: &str, how_to_read: &str) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(key.into(), serde_json::Value::Null);
    fields.insert(format!("{key}_elided"), true.into());
    fields.insert(format!("{key}_bytes"), prose.len().into());
    fields.insert(
        format!("{key}_head"),
        jojobot_domain::text::BODY_DIGEST.render(prose).into(),
    );
    fields.insert("how_to_read".into(), how_to_read.into());
}

/// Render a JSON body as a successful tool result.
pub(crate) fn json_result(body: &serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]))
}
