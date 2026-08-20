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

/// **Move the body to where the agreed revision reads it.**
///
/// Every verb writes one JSON body into a text block, which is the only place
/// a client older than `2026-07-28` can read it. That revision added
/// `structuredContent`, where the same body arrives as an object rather than
/// as a string the caller has to parse.
///
/// **The body moves rather than being copied.** The spec permits a server to
/// send both, and most do; here it would put the whole body on the wire twice
/// for a reader that has it already. Nothing else this surface does ships a
/// caller what it demonstrably holds, and a duplicate of the answer is the
/// plainest case of that.
///
/// **An answer this cannot read is left exactly as the verb wrote it**, for the
/// reason the status bar leaves one alone: rewriting a body it does not
/// understand is worse than not touching it.
pub(crate) fn structure(answered: &mut CallToolResponse) {
    // Only a completed call carries a body. The other responses are the
    // protocol asking the client for something.
    let CallToolResponse::Complete(result) = answered else {
        return;
    };
    let Some(found) = result.content.iter().position(|block| {
        block.as_text().is_some_and(|text| {
            serde_json::from_str::<serde_json::Value>(&text.text).is_ok_and(|body| body.is_object())
        })
    }) else {
        return;
    };
    let block = result.content.remove(found);
    let text = block.as_text().expect("the block just matched as text");
    result.structured_content = serde_json::from_str(&text.text).ok();
}

impl Jojobot {
    /// **The last two things that happen to every answer, in the one order
    /// that works.** The status bar is written into the body, so it has to be
    /// added while the body is still where the verb put it; moving the body to
    /// `structuredContent` first would leave the bar with nothing to ride on.
    ///
    /// They are one call so that order is not a thing a caller can get wrong,
    /// and so a verb added tomorrow gets both by doing nothing.
    pub(crate) async fn finish(
        &self,
        answered: &mut CallToolResponse,
        sid: Option<&str>,
        structured: bool,
    ) {
        self.add_status_bar(answered, sid).await;
        if structured {
            structure(answered);
        }
    }
}
