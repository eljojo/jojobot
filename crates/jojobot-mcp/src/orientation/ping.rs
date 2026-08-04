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
        description = "Check that jojobot is reachable: returns its identity, the BUILD that \
                       is running, and the current time. Use `build` to tell one deployment \
                       from another — `version` is a crate version and does not move. If the \
                       verbs you can see look wrong for what you were told this server does, \
                       this is the call that says which server you are actually talking to. \
                       No side effects."
    )]
    pub(crate) async fn ping(&self) -> Result<CallToolResult, McpError> {
        let now = jiff::Timestamp::now();
        let body = serde_json::json!({
            "server": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            // **Which BUILD is running**, which `version` cannot say: it is a
            // crate version nobody bumps, so it cannot separate a current
            // deployment from one months old. A session whose tool list looked
            // wrong could not tell which it was talking to, and had to hand two
            // hypotheses to a person. `unknown` is a real answer here — a build
            // that cannot say what it is says so, rather than something
            // plausible.
            "build": env!("JOJOBOT_BUILD"),
            "time": now.to_string(),
            "status": "ok",
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    }
}

#[cfg(test)]
mod tests {
    use crate::harness::*;

    /// **`ping` says which BUILD is running, not which crate version.**
    ///
    /// Paid for in production: a stranded session produced two hypotheses about
    /// why its tool list was short and correctly said it could not choose
    /// between them from where it stood. `version` answers `0.1.0` — a crate
    /// version nobody bumps — so it cannot separate a current deployment from
    /// one months old, and a caller reading it learns nothing it did not
    /// already assume.
    ///
    /// The build id is asserted to be present and to be something other than
    /// the crate version. It is deliberately NOT asserted to be any particular
    /// value: what it says depends on how the binary was built, and a test that
    /// pinned one spelling would fail on the build that matters.
    #[tokio::test]
    async fn ping_identifies_the_running_build_and_not_just_the_crate_version() {
        let body = json_of(&handler().ping().await.expect("ping answers"));

        // **The extraction is the check.** `build.rs` filters an empty value
        // and falls through to git and then to `unknown`, so an emptiness
        // assertion here has no reachable input; and a crate-version
        // comparison only fails if somebody exports the crate version by hand.
        // Both were ceremony beside this line, which fails whenever the field
        // is absent or is not a string — the two ways this can actually break.
        body["build"]
            .as_str()
            .unwrap_or_else(|| panic!("ping must identify the running build: {body}"));

        // Paired with the positive: the fields a caller already relies on are
        // still there, so this is an addition rather than a reshuffle.
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
}
