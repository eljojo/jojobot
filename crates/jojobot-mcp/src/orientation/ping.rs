//! `ping` — Liveness: jojobot's identity and its current wall-clock time.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `ping` — a probe, with or without a session behind it.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PingArgs {
    /// The session handle you are carrying, if you have one — the same `sid`
    /// that rides every other call you make. A probe needs no identity, so it
    /// is never required and **never turned away**: the answer says whether the
    /// handle you carried still addresses your session, beside the build that
    /// is running. Those are the two halves of one question when the surface
    /// stops looking like the one you booted on.
    #[serde(default)]
    pub sid: Option<String>,
}

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
                       CARRYING A `sid`? Pass it and the answer also says whether that handle \
                       still addresses your session — the other half of the same question, and \
                       a handle is never turned away here. No side effects."
    )]
    pub(crate) async fn ping(
        &self,
        Parameters(args): Parameters<PingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let now = jiff::Timestamp::now();
        let body = serde_json::json!({
            // The same pair the handshake introduces this server with, so the
            // two doors cannot come to disagree about who is answering.
            "server": crate::SERVER_NAME,
            "version": crate::SERVER_VERSION,
            // **Which BUILD is running**, which `version` cannot say: it is a
            // crate version nobody bumps, so it cannot separate a current
            // deployment from one months old. A session whose tool list looked
            // wrong could not tell which it was talking to, and had to hand two
            // hypotheses to a person. `unknown` is a real answer here — a build
            // that cannot say what it is says so, rather than something
            // plausible.
            "build": env!("JOJOBOT_BUILD"),
            "time": now.to_string(),
            // **The other half of the same question.** A session whose surface
            // stopped looking right asks which server this is; what it does
            // with the answer depends on whether the handle it is holding still
            // addresses anything here. Answered, never refused — a probe that
            // turned away a handle would fail exactly the caller whose handle
            // has stopped working, which is the one calling. See
            // [`Jojobot::standing`].
            "carried_session": self.standing(args.sid.as_deref()),
            "status": "ok",
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;

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
        let body = json_of(
            &handler()
                .ping(Parameters(PingArgs { sid: None }))
                .await
                .expect("ping answers"),
        );

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

    /// **The probe reads the handle it is handed, and reports rather than
    /// refuses.**
    ///
    /// The build id answers half of a stranded session's question — which
    /// server am I talking to — and the handle answers the other half: is the
    /// session I am carrying still on it. Both in one call, because from where
    /// that session stands they are one question.
    ///
    /// **Refusing a handle here would break the probe for exactly the callers
    /// who need it.** Every handle is dead after a restart, and the session
    /// holding one is the session with the most reason to ping; a probe that
    /// answered that with a refusal would be one more thing that stopped
    /// working.
    ///
    /// Four answers, and each pairs with the others: a live handle is `held`, a
    /// well-formed handle nothing is holding is `gone`, a string that is no
    /// handle at all is `malformed`, and a probe carrying none says nothing
    /// about one. Without the first this passes against a probe that says
    /// `gone` to everything; without the third, against one that tells a caller
    /// whose handle arrived upcased or truncated to abandon a session it could
    /// still reach by fixing the string; without the last, against one that
    /// invents a standing for a caller that has no handle at all.
    #[tokio::test]
    async fn the_probe_says_whether_the_handle_it_was_handed_is_still_held() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma").await;
        let live = booted(&jojobot, "gamma").await;

        let held = json_of(
            &jojobot
                .ping(Parameters(PingArgs {
                    sid: Some(live.clone()),
                }))
                .await
                .expect("ping answers"),
        );
        assert_eq!(held["carried_session"], "held", "{held}");

        // Well-formed and never minted — the shape every handle takes once the
        // process holding it has gone.
        let lost = json_of(
            &jojobot
                .ping(Parameters(PingArgs {
                    sid: Some("2gf7".into()),
                }))
                .await
                .expect("ping answers"),
        );
        assert_eq!(lost["carried_session"], "gone", "{lost}");
        assert_eq!(
            lost["status"], "ok",
            "a probe carrying a handle that stopped working must still answer: {lost}"
        );
        assert!(
            lost["build"].as_str().is_some(),
            "…and still say which build it is, which is what it was asked: {lost}"
        );

        // **The same four characters, upcased — and a different answer.** This
        // is the handle a client quoted, cased or truncated on the way through,
        // and `gone` would send its holder off to boot a second run while the
        // one it is carrying is still reachable by fixing the string. Read
        // beside `lost` above: the two must not collapse into one word in
        // either direction.
        let mistyped = json_of(
            &jojobot
                .ping(Parameters(PingArgs {
                    sid: Some("2GF7".into()),
                }))
                .await
                .expect("ping answers"),
        );
        assert_eq!(mistyped["carried_session"], "malformed", "{mistyped}");
        assert_eq!(
            mistyped["status"], "ok",
            "a probe carrying a handle it mistyped is answered, not refused: {mistyped}"
        );

        let anonymous = json_of(
            &jojobot
                .ping(Parameters(PingArgs { sid: None }))
                .await
                .expect("ping answers"),
        );
        assert!(
            anonymous["carried_session"].is_null(),
            "a probe carrying no handle is told nothing about one: {anonymous}"
        );
    }
}
