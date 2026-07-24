//! The MCP adapter — jojobot's single outward interface.
//!
//! This is the only crate that imports `rmcp`. It exposes a [`Jojobot`] server
//! handler; the binary mounts it on an HTTP transport. Alongside the skeleton's
//! `ping`, it carries the first Memory verbs — `capture` and `recall` — mapped
//! onto the [`Memory`](jojobot_domain::memory::Memory) port. Facts are about
//! **entities** (people), passed as a subject id; there is no privileged owner.
//! The port's adapter (real Outline in production, a fake in tests) is injected;
//! this layer only translates MCP calls to domain calls and back.
//!
//! TODO: Memory `capture`/`recall` landed. Attention verbs and the rest of the
//! Memory surface arrive here later, one bounded context at a time.

use std::sync::Arc;

use jojobot_domain::memory::{EntityId, Memory, MemoryError, NewFact, Provenance};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};

/// Arguments to `capture`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — a person id like `person:jose` (a bare
    /// handle like `jose` is prefixed to `person:`).
    pub subject: String,
    /// The crisp claim to remember (one line; no raw newline).
    pub content: String,
    /// `testimony` (the user said it) or `inference` (derived). Defaults to
    /// `inference`: anything not tied to the user's words is a hypothesis.
    #[serde(default)]
    pub provenance: Option<String>,
    /// The fact's freshness date, `YYYY-MM-DD`. Defaults to today (UTC).
    #[serde(default)]
    pub date: Option<String>,
}

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// The entity to read facts about — a person id like `person:jose`.
    pub subject: String,
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

    /// Remember a fact about an entity. Returns the stored fact including the id
    /// its home assigned — which a later `recall` is guaranteed to return.
    #[tool(description = "Capture a fact about an entity (a person id). Returns the stored fact.")]
    async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        let subject = EntityId::person(&args.subject);
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let date = parse_date(args.date.as_deref())?;

        let new = NewFact {
            subject,
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

    /// Read back every fact about an entity.
    #[tool(description = "Recall all facts about an entity (a person id).")]
    async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        let subject = EntityId::person(&args.subject);
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
                 `capture` (remember a fact about an entity), `recall` (read facts about an \
                 entity). More domain verbs arrive later."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::Fact;
    use jojobot_domain::memory::testing::InMemoryMemory;

    fn handler() -> Jojobot {
        Jojobot::new(Arc::new(InMemoryMemory::new()))
    }

    /// Pull the single text block out of a tool result.
    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .first()
            .and_then(|b| b.as_text())
            .map(|t| t.text.clone())
            .expect("tool result should carry a text block")
    }

    fn capture_args(subject: &str, content: &str) -> CaptureArgs {
        CaptureArgs {
            subject: subject.into(),
            content: content.into(),
            provenance: None,
            date: None,
        }
    }

    /// The end-to-end MCP path: capture through the handler, then recall through
    /// the handler, and the fact comes back.
    #[tokio::test]
    async fn capture_then_recall_through_the_handler() {
        let jojobot = handler();
        let captured: Fact = serde_json::from_str(&text_of(
            &jojobot
                .capture(Parameters(capture_args("jose", "drinks oat milk")))
                .await
                .expect("capture ok"),
        ))
        .unwrap();
        assert_eq!(captured.subject.as_str(), "person:jose");

        let recalled = jojobot
            .recall(Parameters(RecallArgs { subject: "jose".into() }))
            .await
            .expect("recall ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&recalled)).unwrap();
        assert_eq!(body["subject"], "person:jose");
        let facts: Vec<Fact> = serde_json::from_value(body["facts"].clone()).unwrap();
        assert!(
            facts.iter().any(|f| f.id == captured.id && f.content == "drinks oat milk"),
            "recall must return the captured fact: {facts:?}"
        );
    }

    /// Omitting `provenance` defaults to inference (a hypothesis until confirmed).
    #[tokio::test]
    async fn provenance_defaults_to_inference() {
        let jojobot = handler();
        let captured: Fact = serde_json::from_str(&text_of(
            &jojobot
                .capture(Parameters(capture_args("jose", "maybe a morning person")))
                .await
                .expect("capture ok"),
        ))
        .unwrap();
        assert_eq!(captured.provenance, Provenance::Inference);
    }

    /// Omitting `date` defaults to today in UTC.
    #[tokio::test]
    async fn date_defaults_to_today_utc() {
        let jojobot = handler();
        let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
        let captured: Fact = serde_json::from_str(&text_of(
            &jojobot
                .capture(Parameters(capture_args("jose", "dated today")))
                .await
                .expect("capture ok"),
        ))
        .unwrap();
        assert_eq!(captured.date, today);
    }

    /// An explicit testimony provenance is honoured.
    #[tokio::test]
    async fn explicit_testimony_is_honoured() {
        let jojobot = handler();
        let captured: Fact = serde_json::from_str(&text_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    subject: "jose".into(),
                    content: "born in Chile".into(),
                    provenance: Some("testimony".into()),
                    date: Some("2026-01-01".into()),
                }))
                .await
                .expect("capture ok"),
        ))
        .unwrap();
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.date, jiff::civil::date(2026, 1, 1));
    }

    #[tokio::test]
    async fn unknown_provenance_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                subject: "jose".into(),
                content: "x".into(),
                provenance: Some("maybe".into()),
                date: None,
            }))
            .await
            .expect_err("must reject unknown provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn malformed_date_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                subject: "jose".into(),
                content: "x".into(),
                provenance: None,
                date: Some("not-a-date".into()),
            }))
            .await
            .expect_err("must reject a malformed date");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn empty_content_is_a_client_error() {
        let err = handler()
            .capture(Parameters(capture_args("jose", "   ")))
            .await
            .expect_err("must reject empty content");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }
}
