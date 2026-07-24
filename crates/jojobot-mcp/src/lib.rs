//! The MCP adapter — jojobot's single outward interface.
//!
//! This is the only crate that imports `rmcp`. It exposes a [`Jojobot`] server
//! handler; the binary mounts it on an HTTP transport. Alongside the skeleton's
//! `ping`, it now carries the first Memory verbs — `capture` and `recall` —
//! mapped onto the [`Memory`](jojobot_domain::memory::Memory) port. The port's
//! adapter (real Outline in production, a fake in tests) is injected; this layer
//! only translates MCP calls to domain calls and back.
//!
//! TODO: Memory `capture`/`recall` landed. Attention verbs and the rest of the
//! Memory surface arrive here later, one bounded context at a time.

use std::sync::Arc;

use jojobot_domain::memory::{
    EntityId, Memory, MemoryError, NewFact, Provenance,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};

/// Arguments to `capture`. This slice's subject is always the `self` entity, so
/// there's no subject field — general entities (and the sourcing guardrail that
/// keeps junk ones out) are slice two. The domain port stays subject-general;
/// only the exposed verb is self-scoped.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The crisp claim to remember about the user (one line; no raw newline).
    pub content: String,
    /// `testimony` (the user said it) or `inference` (derived). Defaults to
    /// `inference`: anything not tied to the user's words is a hypothesis.
    #[serde(default)]
    pub provenance: Option<String>,
    /// The fact's freshness date, `YYYY-MM-DD`. Defaults to today (UTC).
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Clone)]
pub struct Jojobot {
    // Consumed by the `#[tool_handler]` macro's generated routing; rustc's
    // dead-code pass can't see through the macro, hence the allow.
    #[allow(dead_code)]
    tool_router: ToolRouter<Jojobot>,
    /// The Memory port. Injected: real Outline in production, a fake in tests.
    memory: Arc<dyn Memory>,
}

#[tool_router]
impl Jojobot {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
        }
    }

    /// Liveness probe: returns jojobot's identity and its current wall-clock
    /// time. Proves an MCP client can reach the server and get a real response.
    #[tool(description = "Ping jojobot — returns server identity and current time")]
    async fn ping(&self) -> Result<CallToolResult, McpError> {
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

    /// Remember a fact about the user. Returns the stored fact including the id
    /// its home assigned — which a later `recall` is guaranteed to return.
    #[tool(description = "Capture a fact about the user. Returns the stored fact.")]
    async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let date = parse_date(args.date.as_deref())?;

        let new = NewFact {
            subject: EntityId::self_(),
            content: args.content,
            provenance,
            status: Default::default(),
            date,
        };
        let stored = self.memory.capture(new).await.map_err(memory_error)?;
        let body = serde_json::to_string(&stored)
            .map_err(|e| McpError::internal_error(format!("serializing fact: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(body)]))
    }

    /// Read back every fact about the user.
    #[tool(description = "Recall all facts about the user.")]
    async fn recall(&self) -> Result<CallToolResult, McpError> {
        let subject = EntityId::self_();
        let facts = self.memory.recall(&subject).await.map_err(memory_error)?;
        let body = serde_json::json!({
            "subject": subject.as_str(),
            "facts": facts,
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    }
}

/// Parse the provenance argument; unknown values are a client error.
fn parse_provenance(raw: Option<&str>) -> Result<Provenance, McpError> {
    match raw.map(str::trim) {
        None | Some("") | Some("inference") => Ok(Provenance::Inference),
        Some("testimony") => Ok(Provenance::Testimony),
        Some(other) => Err(McpError::invalid_params(
            format!("provenance must be 'testimony' or 'inference', got '{other}'"),
            None,
        )),
    }
}

/// Parse the date argument, or default to today in UTC. The UTC default keeps
/// the domain clock-free while giving `capture` a sensible freshness stamp.
fn parse_date(raw: Option<&str>) -> Result<jiff::civil::Date, McpError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()),
        Some(s) => s.parse().map_err(|e| {
            McpError::invalid_params(format!("date must be YYYY-MM-DD, got '{s}': {e}"), None)
        }),
    }
}

/// Map a domain [`MemoryError`] to an MCP error, splitting client mistakes
/// (invalid params) from server-side failures.
fn memory_error(e: MemoryError) -> McpError {
    match e {
        MemoryError::InvalidFact(msg) => McpError::invalid_params(msg, None),
        MemoryError::NotConfigured(msg) => {
            McpError::internal_error(format!("memory not configured: {msg}"), None)
        }
        MemoryError::Store(msg) => McpError::internal_error(msg, None),
    }
}

#[tool_handler]
impl ServerHandler for Jojobot {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "jojobot — a personal-assistant server. Tools: `ping` (connectivity), \
                 `capture` (remember a fact), `recall` (read facts about an entity). \
                 More domain verbs arrive later."
                    .to_string(),
            )
    }
}
