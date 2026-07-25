//! The MCP adapter — jojobot's single outward interface.
//!
//! This is the only crate that imports `rmcp`. It exposes a [`Jojobot`] server
//! handler; the binary mounts it on an HTTP transport. Alongside the skeleton's
//! `ping` it carries the six Memory verbs — `add_entity`, `capture`,
//! `update_fact`, `update_entity`, `recall`, `list_entities` — mapped onto the
//! [`Memory`](jojobot_domain::memory::Memory) port. The port's adapter (real
//! Outline in production, a fake in tests) is injected; this layer only
//! translates MCP calls to domain calls and back, and holds no policy of its
//! own: the write guard and the promotion gate live in the domain, on the write
//! path, where no caller can route around them.
//!
//! TODO: Memory M1 landed. `search`, structured edges, and the Attention verbs
//! arrive here later, one bounded context at a time.

use std::sync::Arc;

use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, FactStatus, Guarded,
    Memory, MemoryError, NewEntity, NewFact, Provenance, guard::EntityMatch,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};

/// Arguments to `add_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddEntityArgs {
    /// One of `person`, `project`, `place`, `event`, `work`, `thing`, `org`,
    /// `topic`.
    pub kind: String,
    /// The slug half of the handle (`[a-z0-9-]+`), or a full `kind:slug` id
    /// whose kind must match `kind`. The handle is permanent in this milestone.
    pub handle: String,
    /// Display name, as a human would write it.
    pub name: String,
    /// Where this entity came from — **never invented**: the user named it, or
    /// a real source produced it (e.g. `user-named`, `crm-card`, `calendar`).
    pub source: String,
    /// The kanban token this entity mirrors, as `card:N`.
    #[serde(default)]
    pub crm: Option<String>,
    /// `always` to read this entity's doc every boot; defaults to `on-demand`.
    #[serde(default)]
    pub boot: Option<String>,
    /// Set only after a previous call came back with candidates and you judged
    /// them a different entity. It never overrides an exact handle collision.
    #[serde(default)]
    pub create_new: Option<bool>,
}

/// Arguments to `capture`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — any `kind:slug` id (a bare handle is read
    /// as a person).
    pub subject: String,
    /// The crisp claim to remember (one line; no raw newline).
    pub content: String,
    /// Nuance, the why, merge notes — the description under the claim.
    #[serde(default)]
    pub details: Option<String>,
    /// `testimony` (the user said it) or `inference` (derived). Defaults to
    /// `inference`: anything not tied to the user's words is a hypothesis.
    #[serde(default)]
    pub provenance: Option<String>,
    /// The fact's freshness date, `YYYY-MM-DD`. Defaults to today (UTC).
    #[serde(default)]
    pub date: Option<String>,
    /// Set only after a previous call reported candidates for a subject that
    /// doesn't exist yet, and you judged them different.
    #[serde(default)]
    pub create_new: Option<bool>,
}

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// The entity to read facts about — any `kind:slug` id (a bare handle is
    /// read as a person).
    pub subject: String,
}

/// Arguments to `list_entities`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListEntitiesArgs {
    /// Narrow to one kind; omit for every entity.
    #[serde(default)]
    pub kind: Option<String>,
}

/// Arguments to `update_fact`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateFactArgs {
    /// The fact's global address, `kind:slug#local-id` — exactly as `recall`
    /// returned it.
    pub address: String,
    /// Replacement claim.
    #[serde(default)]
    pub content: Option<String>,
    /// Replacement details; pass an empty string to clear them.
    #[serde(default)]
    pub details: Option<String>,
    /// `active`, `superseded`, or `negated`. Negating keeps the fact and its id
    /// — rephrase `content` as the thing NOT to infer.
    #[serde(default)]
    pub status: Option<String>,
    /// `testimony` or `inference`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// Required to promote a claim from inference to testimony: set it only
    /// when the user has actually confirmed the claim.
    #[serde(default)]
    pub confirmed_by_user: Option<bool>,
}

/// Arguments to `update_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityArgs {
    /// The entity's handle. Not editable — renaming a handle is a separate
    /// operation that does not exist yet.
    pub handle: String,
    /// New display name.
    #[serde(default)]
    pub name: Option<String>,
    /// New source.
    #[serde(default)]
    pub source: Option<String>,
    /// New kanban token, as `card:N`.
    #[serde(default)]
    pub crm: Option<String>,
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

    /// Create an entity of any kind. Screened by the write guard, so a handle
    /// or name that looks like one jojobot already knows comes back as
    /// candidates instead of a second record.
    #[tool(
        description = "Create an entity (person/project/place/event/work/thing/org/topic). \
                       If it looks like one that already exists, returns candidates to \
                       confirm instead of writing."
    )]
    async fn add_entity(
        &self,
        Parameters(args): Parameters<AddEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = entity_id(&args.kind, &args.handle)?;
        let new = NewEntity {
            id,
            name: args.name,
            source: args.source,
            crm: args.crm,
            boot: args
                .boot
                .as_deref()
                .map_or(Default::default(), jojobot_domain::memory::Boot::from_token),
            create_new: args.create_new.unwrap_or(false),
        };
        match self.memory.add_entity(new).await.map_err(memory_error)? {
            Guarded::Written(entity) => json_result(&entity_json(&entity)),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, "add_entity")),
        }
    }

    /// Every entity jojobot knows, optionally narrowed to one kind.
    #[tool(description = "List the entities jojobot knows, optionally filtered by kind.")]
    async fn list_entities(
        &self,
        Parameters(args): Parameters<ListEntitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let kind = args.kind.as_deref().map(parse_kind).transpose()?;
        let entities = self.memory.list_entities(kind).await.map_err(memory_error)?;
        let body = serde_json::json!({
            "count": entities.len(),
            "entities": entities.iter().map(entity_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }

    /// Edit an entity's metadata in place. The handle itself never changes.
    #[tool(
        description = "Edit an entity's metadata (name/source/crm) in place. The handle is \
                       immutable. An unknown handle errors with near misses — it never creates."
    )]
    async fn update_entity(
        &self,
        Parameters(args): Parameters<UpdateEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let handle = EntityId::person(&args.handle);
        let patch = EntityPatch {
            name: args.name,
            source: args.source,
            crm: args.crm,
        };
        let entity = self
            .memory
            .update_entity(&handle, patch)
            .await
            .map_err(memory_error)?;
        json_result(&entity_json(&entity))
    }

    /// Remember a fact about an entity. Returns the stored fact including the
    /// address a later `update_fact` can edit it through.
    #[tool(
        description = "Capture a fact about an entity of any kind. Returns the stored fact and \
                       its address. A subject that doesn't exist yet is screened by the write \
                       guard first."
    )]
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
            details: args.details,
            provenance,
            status: Default::default(),
            date,
            create_new: args.create_new.unwrap_or(false),
        };
        match self.memory.capture(new).await.map_err(memory_error)? {
            Guarded::Written(fact) => json_result(&fact_json(&fact)),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, "capture")),
        }
    }

    /// Read back every fact about an entity, each with its address.
    #[tool(
        description = "Recall all facts about an entity. Every fact carries its address — pass \
                       that address to update_fact to edit it."
    )]
    async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        let subject = EntityId::person(&args.subject);
        let facts = self.memory.recall(&subject).await.map_err(memory_error)?;
        let body = serde_json::json!({
            "subject": subject.as_str(),
            "facts": facts.iter().map(fact_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }

    /// Edit one addressed fact in place — fix the source, never an addendum.
    #[tool(
        description = "Edit an addressed fact in place (content/details/status/provenance). \
                       Negating is a status flip that keeps the fact. Promoting inference → \
                       testimony requires confirmed_by_user. An unknown address errors with \
                       the addresses that do exist — it never creates."
    )]
    async fn update_fact(
        &self,
        Parameters(args): Parameters<UpdateFactArgs>,
    ) -> Result<CallToolResult, McpError> {
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let patch = FactPatch {
            content: args.content,
            details: args.details,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args.provenance.as_deref().map(parse_one_provenance).transpose()?,
            confirmed_by_user: args.confirmed_by_user.unwrap_or(false),
        };
        let fact = self
            .memory
            .update_fact(&address, patch)
            .await
            .map_err(memory_error)?;
        json_result(&fact_json(&fact))
    }
}

/// A fact on the wire: its fields plus the **address** — the handle a caller
/// needs to edit it. Reads return it with every fact precisely so that update is
/// usable without a second lookup.
fn fact_json(fact: &Fact) -> serde_json::Value {
    let mut body = serde_json::to_value(fact).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("address".into(), fact.address().to_string().into());
    }
    body
}

/// An entity on the wire.
fn entity_json(entity: &Entity) -> serde_json::Value {
    serde_json::to_value(entity).unwrap_or(serde_json::Value::Null)
}

/// The write guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant. Flagged as an error result so it can't read as a
/// completed write, with the candidates in the body so the caller can decide —
/// jojobot detects, the AI decides.
fn blocked_result(attempted: &EntityId, candidates: &[EntityMatch], verb: &str) -> CallToolResult {
    let body = serde_json::json!({
        "status": "needs_confirmation",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates,
        "how_to_proceed": format!(
            "Nothing was written. Either use an existing handle above, or re-call {verb} with \
             create_new: true if this is genuinely a different entity. An exact handle \
             collision can't be forced — pick a more qualified slug instead."
        ),
    });
    CallToolResult::error(vec![ContentBlock::text(body.to_string())])
}

/// Render a JSON body as a successful tool result.
fn json_result(body: &serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]))
}

/// Parse a kind token; the closed set is named in the error so a caller can fix
/// the call without guessing.
fn parse_kind(raw: &str) -> Result<EntityKind, McpError> {
    EntityKind::from_token(raw.trim()).ok_or_else(|| {
        let kinds: Vec<&str> = EntityKind::ALL.iter().map(|k| k.as_token()).collect();
        McpError::invalid_params(
            format!("kind must be one of {}, got '{raw}'", kinds.join(", ")),
            None,
        )
    })
}

/// Build an entity id from a `kind` argument and a handle that may be a bare
/// slug or a fully qualified id. A qualified handle that disagrees with `kind`
/// is a client error rather than a silent winner.
fn entity_id(kind: &str, handle: &str) -> Result<EntityId, McpError> {
    let kind = parse_kind(kind)?;
    match handle.trim().split_once(':') {
        None => Ok(EntityId::new(kind, handle)),
        Some((k, slug)) if EntityKind::from_token(k) == Some(kind) => Ok(EntityId::new(kind, slug)),
        Some((k, _)) => Err(McpError::invalid_params(
            format!("handle '{handle}' says kind '{k}' but kind is '{kind}'"),
            None,
        )),
    }
}

/// Parse a lifecycle status; unknown values are a client error, never a silent
/// fallback to active — that would turn a failed negation into a live claim.
fn parse_status(raw: &str) -> Result<FactStatus, McpError> {
    match raw.trim() {
        "active" => Ok(FactStatus::Active),
        "superseded" => Ok(FactStatus::Superseded),
        "negated" => Ok(FactStatus::Negated),
        other => Err(McpError::invalid_params(
            format!("status must be 'active', 'superseded', or 'negated', got '{other}'"),
            None,
        )),
    }
}

/// Parse an explicit provenance value (no default — the caller named one).
fn parse_one_provenance(raw: &str) -> Result<Provenance, McpError> {
    match raw.trim() {
        "testimony" => Ok(Provenance::Testimony),
        "inference" => Ok(Provenance::Inference),
        other => Err(McpError::invalid_params(
            format!("provenance must be 'testimony' or 'inference', got '{other}'"),
            None,
        )),
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
        // Everything the caller can fix by calling differently is invalid_params
        // — including the misses, whose messages carry the near candidates.
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::UnknownFact { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::UnconfirmedPromotion => McpError::invalid_params(e.to_string(), None),
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
                "jojobot — a personal-assistant server. Everything it knows is an **entity** \
                 (person/project/place/event/work/thing/org/topic, handled `kind:slug`) or a \
                 **fact** about one (addressed `kind:slug#local-id`). Tools: `ping` \
                 (connectivity) · `add_entity` · `capture` (remember a fact) · `recall` (read \
                 facts, each with its address) · `update_fact` (edit in place; negating is a \
                 status flip) · `update_entity` (metadata only) · `list_entities`. Two rules \
                 to expect: a write that looks like an entity jojobot already knows comes \
                 back with candidates and writes nothing until you confirm or pass \
                 create_new; and promoting a claim from inference to testimony needs the \
                 user's explicit confirmation. More domain verbs arrive later."
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
            details: None,
            provenance: None,
            date: None,
            create_new: None,
        }
    }

    /// The JSON body of a tool result.
    fn json_of(result: &CallToolResult) -> serde_json::Value {
        serde_json::from_str(&text_of(result)).expect("tool results carry a JSON body")
    }

    /// Capture through the handler, expecting the guard to wave it through.
    async fn capture_ok(jojobot: &Jojobot, args: CaptureArgs) -> Fact {
        let result = jojobot.capture(Parameters(args)).await.expect("capture ok");
        assert_ne!(result.is_error, Some(true), "the guard blocked: {}", text_of(&result));
        serde_json::from_str(&text_of(&result)).expect("a captured fact")
    }

    fn add_args(kind: &str, handle: &str, name: &str) -> AddEntityArgs {
        AddEntityArgs {
            kind: kind.into(),
            handle: handle.into(),
            name: name.into(),
            source: "user-named".into(),
            crm: None,
            boot: None,
            create_new: None,
        }
    }

    // --- the entity verbs -----------------------------------------------------

    /// `add_entity` creates any kind, and `list_entities` reads it back — the
    /// two halves of the entity surface, through the MCP path.
    #[tokio::test]
    async fn add_entity_then_list_entities_through_the_handler() {
        let jojobot = handler();
        let added = jojobot
            .add_entity(Parameters(AddEntityArgs {
                crm: Some("card:874".into()),
                ..add_args("project", "atlas", "Atlas")
            }))
            .await
            .expect("add ok");
        let body = json_of(&added);
        assert_eq!(body["id"], "project:atlas");
        assert_eq!(body["kind"], "project");
        assert_eq!(body["crm"], "card:874");

        let listed = jojobot
            .list_entities(Parameters(ListEntitiesArgs { kind: Some("project".into()) }))
            .await
            .expect("list ok");
        let body = json_of(&listed);
        assert_eq!(body["entities"][0]["id"], "project:atlas");
        assert_eq!(body["count"], 1);
    }

    /// A subject of any kind captures — facts are no longer people-only.
    #[tokio::test]
    async fn a_fact_can_be_about_any_kind() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("place:north-trail", "swimmable in August")).await;
        assert_eq!(captured.subject.as_str(), "place:north-trail");
    }

    /// An unknown kind is a client error that names the closed set, rather than
    /// a record filed under a noun nobody chose.
    #[tokio::test]
    async fn an_unknown_kind_is_a_client_error() {
        let err = handler()
            .add_entity(Parameters(add_args("receipt", "some-slug", "An unknown kind")))
            .await
            .expect_err("must reject an unknown kind");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("person"), "the error must name the kinds: {}", err.message);
    }

    /// `update_entity` edits metadata and leaves the handle alone.
    #[tokio::test]
    async fn update_entity_edits_metadata() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let updated = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "thing:red-bike".into(),
                name: Some("Red Bike (the gravel one)".into()),
                source: None,
                crm: Some("card:551".into()),
            }))
            .await
            .expect("update ok");
        let body = json_of(&updated);
        assert_eq!(body["id"], "thing:red-bike", "the handle is immutable");
        assert_eq!(body["name"], "Red Bike (the gravel one)");
        assert_eq!(body["source"], "user-named", "an omitted field is left alone");
    }

    /// Updating an entity that isn't there is a client error naming near misses
    /// — it never creates one.
    #[tokio::test]
    async fn update_entity_unknown_handle_is_a_client_error() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let err = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "thing:red-bikee".into(),
                name: Some("nope".into()),
                source: None,
                crm: None,
            }))
            .await
            .expect_err("unknown handle must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("thing:red-bike"), "must name the near miss: {}", err.message);
    }

    // --- the write guard, through the MCP boundary ----------------------------

    /// A guarded write comes back as an **error-flagged** result carrying the
    /// candidates — never as a quiet success the caller could mistake for a
    /// completed write.
    #[tokio::test]
    async fn a_blocked_add_returns_the_candidates_as_an_error_result() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("first add ok");

        let blocked = jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha Two")))
            .await
            .expect("the call succeeds; the guard answers in the body");
        assert_eq!(blocked.is_error, Some(true), "a blocked write must not read as success");
        let body = json_of(&blocked);
        assert_eq!(body["status"], "needs_confirmation");
        assert_eq!(body["attempted"], "person:alpha");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        assert_eq!(body["candidates"][0]["reason"], "exact-handle");
        assert_eq!(body["candidates"][0]["source"], "user-named");

        // And nothing was written.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("person".into()) }))
                .await
                .expect("list ok"),
        );
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["entities"][0]["name"], "Alpha");
    }

    /// The same on the capture path — and `create_new: true` is what lets a
    /// genuinely different entity through.
    #[tokio::test]
    async fn a_blocked_capture_reports_then_accepts_create_new() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let blocked = jojobot
            .capture(Parameters(capture_args("zenit", "should not land")))
            .await
            .expect("call ok");
        assert_eq!(blocked.is_error, Some(true));
        assert_eq!(json_of(&blocked)["candidates"][0]["handle"], "person:zenith");

        let forced = capture_ok(
            &jojobot,
            CaptureArgs { create_new: Some(true), ..capture_args("zenit", "now it lands") },
        )
        .await;
        assert_eq!(forced.subject.as_str(), "person:zenit");
    }

    // --- addresses and update -------------------------------------------------

    /// Every recalled fact carries its address, and that address is what
    /// `update_fact` takes — the pairing that makes editing possible.
    #[tokio::test]
    async fn recall_returns_addresses_that_update_fact_accepts() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        let address = body["facts"][0]["address"].as_str().expect("every fact carries an address");
        assert_eq!(address, "person:alpha#f1");

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    address: address.into(),
                    content: Some("works at the new place".into()),
                    details: Some("changed jobs in July".into()),
                    status: None,
                    provenance: None,
                    confirmed_by_user: None,
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["content"], "works at the new place");
        assert_eq!(updated["details"], "changed jobs in July");
        assert_eq!(updated["address"], "person:alpha#f1");
    }

    /// Negating is a status flip through the MCP path too.
    #[tokio::test]
    async fn update_fact_can_negate() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a close contact of the user")).await;
        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    address: captured.address().to_string(),
                    content: Some("NOT a close contact — do not re-infer".into()),
                    details: None,
                    status: Some("negated".into()),
                    provenance: None,
                    confirmed_by_user: None,
                }))
                .await
                .expect("negate ok"),
        );
        assert_eq!(updated["status"], "negated");
        assert_eq!(updated["id"], "f1", "a negated fact keeps its id");
    }

    /// Promotion to testimony needs the explicit confirmation flag.
    #[tokio::test]
    async fn promoting_to_testimony_requires_the_confirmation_flag() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "prefers mornings")).await;
        let promote = |confirmed: Option<bool>| UpdateFactArgs {
            address: captured.address().to_string(),
            content: None,
            details: None,
            status: None,
            provenance: Some("testimony".into()),
            confirmed_by_user: confirmed,
        };

        let err = jojobot
            .update_fact(Parameters(promote(None)))
            .await
            .expect_err("an unconfirmed promotion must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let ok = json_of(
            &jojobot
                .update_fact(Parameters(promote(Some(true))))
                .await
                .expect("a confirmed promotion is allowed"),
        );
        assert_eq!(ok["provenance"], "testimony");
    }

    /// A malformed or unknown address is a client error — never a new fact.
    #[tokio::test]
    async fn a_bad_address_is_a_client_error() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "the only fact here")).await;
        for address in ["not-an-address", "person:alpha#f99"] {
            let err = jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    address: address.into(),
                    content: Some("nope".into()),
                    details: None,
                    status: None,
                    provenance: None,
                    confirmed_by_user: None,
                }))
                .await
                .expect_err("must reject {address}");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {address}");
        }
        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(body["facts"].as_array().unwrap().len(), 1, "nothing was created");
    }

    /// An unknown status token is a client error, not a silently-active fact.
    #[tokio::test]
    async fn an_unknown_status_is_a_client_error() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;
        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                address: captured.address().to_string(),
                content: None,
                details: None,
                status: Some("retired".into()),
                provenance: None,
                confirmed_by_user: None,
            }))
            .await
            .expect_err("must reject an unknown status");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// The end-to-end MCP path: capture through the handler, then recall through
    /// the handler, and the fact comes back.
    #[tokio::test]
    async fn capture_then_recall_through_the_handler() {
        let jojobot = handler();
        let captured: Fact = serde_json::from_str(&text_of(
            &jojobot
                .capture(Parameters(capture_args("alpha", "drinks oat milk")))
                .await
                .expect("capture ok"),
        ))
        .unwrap();
        assert_eq!(captured.subject.as_str(), "person:alpha");

        let recalled = jojobot
            .recall(Parameters(RecallArgs { subject: "alpha".into() }))
            .await
            .expect("recall ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&recalled)).unwrap();
        assert_eq!(body["subject"], "person:alpha");
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
                .capture(Parameters(capture_args("alpha", "maybe a morning person")))
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
                .capture(Parameters(capture_args("alpha", "dated today")))
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
                    provenance: Some("testimony".into()),
                    date: Some("2026-01-01".into()),
                    ..capture_args("alpha", "speaks two languages")
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
                provenance: Some("maybe".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject unknown provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn malformed_date_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                date: Some("not-a-date".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject a malformed date");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn empty_content_is_a_client_error() {
        let err = handler()
            .capture(Parameters(capture_args("alpha", "   ")))
            .await
            .expect_err("must reject empty content");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }
}
