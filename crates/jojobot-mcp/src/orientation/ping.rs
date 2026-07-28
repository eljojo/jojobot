//! `ping` — Liveness: jojobot's identity and its current wall-clock time.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Liveness probe: returns jojobot's identity and its current wall-clock
/// time. Proves an MCP client can reach the server and get a real response.
#[tool_router(router = ping_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Check that jojobot is reachable: returns its identity, version and \
                       current time. No side effects."
    )]
    pub(crate) async fn ping(&self) -> Result<CallToolResult, McpError> {
        let now = jiff::Timestamp::now();
        let body = serde_json::json!({
            "server": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "time": now.to_string(),
            "status": "ok",
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    }
}
