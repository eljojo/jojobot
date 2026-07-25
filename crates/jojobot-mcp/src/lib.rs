//! The MCP adapter — jojobot's single outward interface.
//!
//! This is the only crate that imports `rmcp`. It exposes a [`Jojobot`] server
//! handler; the binary mounts it on an HTTP transport. Alongside the skeleton's
//! `ping` it carries the six Memory verbs — `add_entity`, `capture`,
//! `update_fact`, `update_entity`, `recall`, `list_entities` — mapped onto the
//! [`Memory`](jojobot_domain::memory::Memory) port, and **`search`**, the front
//! door, on the [`Search`](jojobot_domain::memory::search::Search) port. Both
//! adapters (real Outline behind the index in production, a fake in tests) are
//! injected; this layer only
//! translates MCP calls to domain calls and back, and holds no policy of its
//! own: the write guard and the promotion gate live in the domain, on the write
//! path, where no caller can route around them.
//!
//! **Responses speak schema.org's words, with none of its machinery** — a kind
//! renders as `Person`/`CreativeWork`/`Organization`, an edge shape as
//! `memberOf`/`attendee`. Names only: no `@context`, no CURIEs, no JSON-LD. The
//! **input** grammar is untouched — ids and kind tokens stay lowercase
//! `kind:slug`, and a capitalized kind on input is still rejected.
//!
//! TODO: Memory M1 landed; M2 adds structured edges at capture. The Attention
//! verbs arrive here later, one bounded context at a time.

use std::sync::Arc;

use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, Memory, MemoryError, NewEntity, NewFact, Provenance, guard::EntityMatch,
    search::{DEFAULT_LIMIT, EdgeFilter, Hit, Search, SearchQuery},
    validate_edge,
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
    /// The shape of the edge this fact draws: `location` (object is a place) ·
    /// `membership` (an org) · `attendance` (an event) · `about` (any kind).
    /// Requires `object`; neither works alone.
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. Screened by the write guard
    /// exactly as `subject` is — a typo comes back as candidates, never a new node.
    #[serde(default)]
    pub object: Option<String>,
    /// Set only after a previous call reported candidates for a subject or an
    /// edge object that doesn't exist yet, and you judged them different.
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
    /// `active` or `superseded`. **A refutation is not a status** — to record
    /// that something is not so, rewrite `content` to state the negative truth;
    /// it stays `active`, because that IS the current truth.
    #[serde(default)]
    pub status: Option<String>,
    /// `testimony` or `inference`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// Required to promote a claim from inference to testimony: set it only
    /// when the user has actually confirmed the claim.
    #[serde(default)]
    pub confirmed_by_user: Option<bool>,
    /// The shape of an edge to attach: `location` · `membership` · `attendance` ·
    /// `about`. Requires `object`; neither works alone.
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. Screened by the write guard.
    #[serde(default)]
    pub object: Option<String>,
    /// Set only after a previous call reported candidates for the edge's object
    /// and you judged them a different entity.
    #[serde(default)]
    pub create_new: Option<bool>,
}

/// The `edge` filter of a `search` — a shape and the entity it points at.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeFilterArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about`).
    /// Omit for **any** edge pointing at `object` — "what's connected to X".
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge must point at, as `kind:slug`.
    pub object: String,
}

/// Arguments to `search`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Free text over entity handles/names, fact claims and details, and the
    /// prose of documents. **All words must match.** Optional when at least one
    /// filter below is given.
    #[serde(default)]
    pub query: Option<String>,
    /// Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
    /// or the owner of the doc a prose match sits in.
    #[serde(default)]
    pub kind: Option<String>,
    /// `active` (the default) or `superseded`. A superseded fact is **excluded
    /// unless asked for by name** — a claim already moved past must not come
    /// back as current truth.
    #[serde(default)]
    pub status: Option<String>,
    /// `testimony` or `inference`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// Facts about this entity, as `kind:slug`.
    #[serde(default)]
    pub subject: Option<String>,
    /// Facts drawing a matching edge. With `kind`, this is how a cross-entity
    /// question ("which people are in X") is answered in one call.
    #[serde(default)]
    pub edge: Option<EdgeFilterArgs>,
    /// How many results; defaults to 20. There is no pagination — a second page
    /// is a better query.
    #[serde(default)]
    pub limit: Option<u32>,
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
    /// Set only after a previous call reported candidates for the new name and
    /// you judged them a different entity. A rename is screened exactly as a
    /// creation is.
    #[serde(default)]
    pub create_new: Option<bool>,
}

#[derive(Clone)]
pub struct Jojobot {
    // Consumed by the `#[tool_handler]` macro's generated routing; rustc's
    // dead-code pass can't see through the macro, hence the allow.
    #[allow(dead_code)]
    tool_router: ToolRouter<Jojobot>,
    /// The Memory port. Injected: real Outline in production, a fake in tests.
    memory: Arc<dyn Memory>,
    /// The retrieval port — the search projection over the same store. Injected
    /// separately because it is a different port, not a second store: in
    /// production both are the one indexed adapter.
    search: Arc<dyn Search>,
}

#[tool_router]
impl Jojobot {
    pub fn new(memory: Arc<dyn Memory>, search: Arc<dyn Search>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
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

    /// The front door: one ranked list over entities, facts and prose.
    #[tool(
        description = "Search everything jojobot knows — entities, facts, and the prose of \
                       documents — as ONE ranked list of typed hits. `query` is free text (all \
                       words must match) and is optional when a filter narrows it: kind · \
                       status (default active; superseded is excluded unless named) · provenance · \
                       subject · edge {shape, object}. kind + edge answers a cross-entity \
                       question in one call (\"which people are in X\"): it walks the typed \
                       edges, so a doc that merely mentions X is not an answer. Every fact hit \
                       carries its whole row and its address, so you can edit what you find. \
                       No pagination — raise `limit` or ask a better question."
    )]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let edge = args
            .edge
            .as_ref()
            .map(|e| -> Result<EdgeFilter, McpError> {
                Ok(EdgeFilter {
                    shape: e.shape.as_deref().map(parse_shape).transpose()?,
                    object: EntityId(e.object.trim().to_string()),
                })
            })
            .transpose()?;
        let query = SearchQuery {
            text: args.query,
            kind: args.kind.as_deref().map(parse_kind).transpose()?,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args.provenance.as_deref().map(parse_one_provenance).transpose()?,
            subject: args.subject.as_deref().map(EntityId::person),
            edge,
            limit: args.limit.map_or(DEFAULT_LIMIT, |l| l as usize),
        };
        // Checked here as well as in the index: a malformed query is the caller's
        // mistake, and it should read as one no matter which adapter is behind us.
        query.validate().map_err(memory_error)?;
        let hits = self.search.search(&query).map_err(memory_error)?;
        let body = serde_json::json!({
            "count": hits.len(),
            "results": hits.iter().map(hit_json).collect::<Vec<_>>(),
        });
        json_result(&body)
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

    /// Edit an entity's metadata in place. The handle itself never changes, and
    /// a name change is screened by the write guard just as a creation is.
    #[tool(
        description = "Edit an entity's metadata (name/source/crm) in place. The handle is \
                       immutable. A name change is screened by the write guard, so it can come \
                       back with candidates to confirm. An unknown handle errors with near \
                       misses — it never creates."
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
            create_new: args.create_new.unwrap_or(false),
        };
        match self
            .memory
            .update_entity(&handle, patch)
            .await
            .map_err(memory_error)?
        {
            Guarded::Written(entity) => json_result(&entity_json(&entity)),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, "update_entity")),
        }
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
        let edge = parse_edge(args.shape.as_deref(), args.object.as_deref())?;

        let new = NewFact {
            subject,
            content: args.content,
            details: args.details,
            provenance,
            status: Default::default(),
            date,
            edge,
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
                       To record that something is NOT so, rewrite content to state the \
                       negative truth — that is an ordinary edit and the fact stays active; \
                       there is no negated status. Promoting inference → testimony requires \
                       confirmed_by_user. An unknown address errors with the addresses that do \
                       exist — it never creates."
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
            edge: parse_edge(args.shape.as_deref(), args.object.as_deref())?,
            create_new: args.create_new.unwrap_or(false),
        };
        match self
            .memory
            .update_fact(&address, patch)
            .await
            .map_err(memory_error)?
        {
            Guarded::Written(fact) => json_result(&fact_json(&fact)),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, "update_fact")),
        }
    }
}

/// A fact on the wire: the whole row plus the **address** — the handle a caller
/// needs to edit it. Reads return it with every fact precisely so that update is
/// usable without a second lookup.
///
/// Rendered by hand rather than derived, so `capture`, `recall`, `update_fact`
/// and `search` cannot drift into three spellings of one record — and so the
/// response vocabulary (schema.org names, § Vocabulary) lives in exactly one
/// place. **Input grammar is unaffected:** ids and kind tokens stay lowercase
/// `kind:slug` on the way in.
fn fact_json(fact: &Fact) -> serde_json::Value {
    serde_json::json!({
        "address": fact.address().to_string(),
        "subject": fact.subject.as_str(),
        "content": fact.content,
        "details": fact.details,
        "provenance": fact.provenance.as_token(),
        "status": fact.status.as_token(),
        "date": fact.date.to_string(),
        "edge": fact.edge.as_ref().map(edge_json),
    })
}

/// One search result on the wire. **Every hit says what it is** (`hit`), so a
/// caller reads a mixed list without guessing from its shape — and each kind of
/// hit carries what makes it actionable: an entity its handle, a fact its whole
/// row and address, prose the doc to open and the text around the match.
fn hit_json(hit: &Hit) -> serde_json::Value {
    match hit {
        Hit::Entity { entity, doc_id } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("doc".into(), doc_id.clone().into());
            }
            body
        }
        Hit::Fact { fact } => {
            let mut body = fact_json(fact);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "fact".into());
            }
            body
        }
        Hit::Prose {
            doc_id,
            title,
            entity,
            snippet,
        } => serde_json::json!({
            "hit": "prose",
            "doc": doc_id,
            "title": title,
            "entity": entity.as_ref().map(|e| e.as_str()),
            "snippet": snippet,
        }),
    }
}

/// An edge on the wire. `type` carries schema.org's word for the shape —
/// `memberOf`, `attendee` — where the input token is `membership`, `attendance`.
fn edge_json(edge: &Edge) -> serde_json::Value {
    serde_json::json!({
        "type": edge.shape.as_name(),
        "object": edge.object.as_str(),
    })
}

/// An entity on the wire. `type` is the schema.org-flavored **name** for its
/// kind (`Person`, `CreativeWork`, `Organization`); the lowercase kind token
/// stays the input grammar and the handle's prefix.
fn entity_json(entity: &Entity) -> serde_json::Value {
    serde_json::json!({
        "id": entity.id.as_str(),
        "type": type_name(entity.kind),
        "name": entity.name,
        "source": entity.source,
        "crm": entity.crm,
        "boot": entity.boot.as_token(),
    })
}

/// One of the guard's candidates on the wire.
fn candidate_json(candidate: &EntityMatch) -> serde_json::Value {
    serde_json::json!({
        "handle": candidate.handle.as_str(),
        "type": type_name(candidate.kind),
        "name": candidate.name,
        "source": candidate.source,
        "reason": candidate.reason,
    })
}

/// The schema.org-flavored type name for an entity kind — **names only**, no
/// `@context`, no CURIEs, no JSON-LD: the recognition benefit is the word, which
/// models know from pretraining, not the machinery.
///
/// `Project` is jojobot's own personal-goal sense (trips, big rocks, builds),
/// deliberately NOT schema.org's Organization-subtype meaning.
fn type_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Person => "Person",
        EntityKind::Place => "Place",
        EntityKind::Event => "Event",
        EntityKind::Work => "CreativeWork",
        EntityKind::Thing => "Product",
        EntityKind::Org => "Organization",
        EntityKind::Topic => "Topic",
        EntityKind::Project => "Project",
    }
}

/// The write guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant.
///
/// A **successful** result carrying a structured payload, not a protocol error.
/// The guard doing its job is an answer the caller has to act on — jojobot
/// detects, the AI decides — and dressing it as an exception made a working
/// feature read like a broken server: clients that retry on error retry it, and
/// clients that unwrap on error handle it exactly wrong. `status` and `wrote`
/// are what stop it reading as a completed write.
fn blocked_result(attempted: &EntityId, candidates: &[EntityMatch], verb: &str) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": format!(
            "Nothing was written. Either use an existing handle above, or re-call {verb} with \
             create_new: true if this is genuinely a different entity. An exact handle \
             collision can't be forced — pick a more qualified slug instead."
        ),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
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

/// Parse an edge-shape token; the closed set is named in the error. Strict about
/// case and spelling: the **response** names (`memberOf`, `attendee`) are not
/// input, and the input grammar stays lowercase.
fn parse_shape(raw: &str) -> Result<EdgeShape, McpError> {
    EdgeShape::from_token(raw).ok_or_else(|| {
        let shapes: Vec<&str> = EdgeShape::ALL.iter().map(|s| s.as_token()).collect();
        McpError::invalid_params(
            format!("shape must be one of {}, got '{raw}'", shapes.join(", ")),
            None,
        )
    })
}

/// Parse the `shape`/`object` pair into an edge. **Half an edge is an error, not
/// a shrug:** a shape with no object has nothing to point at, and an object with
/// no shape has no meaning — either way the caller meant an edge and did not get
/// one, which is exactly the silence ask-across dies of.
fn parse_edge(shape: Option<&str>, object: Option<&str>) -> Result<Option<Edge>, McpError> {
    match (shape.map(str::trim).filter(|s| !s.is_empty()), object.map(str::trim).filter(|s| !s.is_empty())) {
        (None, None) => Ok(None),
        (Some(shape), Some(object)) => {
            let shape = parse_shape(shape)?;
            let edge = Edge::new(shape, EntityId(object.to_string()));
            // Grammar and the shape's kind rule, checked here so the caller hears
            // it as a client error rather than a store failure.
            validate_edge(&edge).map_err(memory_error)?;
            Ok(Some(edge))
        }
        (Some(_), None) => Err(McpError::invalid_params(
            "shape needs an object: an edge is a shape AND the entity it points at".to_string(),
            None,
        )),
        (None, Some(_)) => Err(McpError::invalid_params(
            "object needs a shape: one of location, membership, attendance, about".to_string(),
            None,
        )),
    }
}

/// Parse a lifecycle status; unknown values are a client error, never a silent
/// fallback to active — a mistyped status that quietly became `active` would
/// hide the state the caller was reaching for.
///
/// **`negated` is refused by name.** The reader still maps a legacy `negated`
/// cell to superseded (rows carrying it are on disk), but the input grammar
/// does not: a caller reaching for it is reaching for behaviour that is gone,
/// and silently aliasing it to superseded would file a refutation where nobody
/// would look for it. The error says what to do instead.
fn parse_status(raw: &str) -> Result<FactStatus, McpError> {
    match raw.trim() {
        "active" => Ok(FactStatus::Active),
        "superseded" => Ok(FactStatus::Superseded),
        "negated" => Err(McpError::invalid_params(
            "there is no 'negated' status: to record that something is NOT so, rewrite the \
             fact's content to state the negative truth — it stays 'active', because that is \
             the current truth. Use 'superseded' only for a claim a later fact replaced."
                .to_string(),
            None,
        )),
        other => Err(McpError::invalid_params(
            format!("status must be 'active' or 'superseded', got '{other}'"),
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
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
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
                 **fact** about one (addressed `kind:slug#local-id`). **Start with `search`** — \
                 it is the front door: one ranked list over entities, facts and document prose \
                 at once, so you do not have to know where something was filed. Tools: `search` \
                 · `ping` (connectivity) · `add_entity` · `capture` (remember a fact) · `recall` \
                 (every fact about one entity) · `update_fact` (edit in place; to refute a \
                 claim, rewrite its content to state the negative truth — there is no negated \
                 status) · `update_entity` (metadata only) · `list_entities`. A fact may \
                 also draw one typed **edge** — pass `shape` (location · membership · \
                 attendance · about) with `object`, the entity it points at; that is what \
                 makes cross-entity questions answerable. Two rules to expect: a write that \
                 looks like an entity jojobot already knows (a subject, a name, or an edge's \
                 object) comes back with candidates and writes nothing until you confirm or \
                 pass create_new; and promoting a claim from inference to testimony needs the \
                 user's explicit confirmation. Responses name types the schema.org way \
                 (`Person`, `CreativeWork`, `memberOf`); input stays lowercase `kind:slug`. \
                 More domain verbs arrive later."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Boot, Edge, EdgeShape, EntityKind, FactId};

    /// A [`Search`] double: it records the query it was handed and answers with
    /// canned hits. On this path the MCP layer's whole job is translating
    /// arguments into a query and hits into JSON, and that is exactly what this
    /// pins — the ranking and matching are the index's tests, not these.
    #[derive(Default)]
    struct SpySearch {
        seen: Mutex<Option<SearchQuery>>,
        hits: Mutex<Vec<Hit>>,
    }

    impl SpySearch {
        fn answering(hits: Vec<Hit>) -> Self {
            SpySearch {
                seen: Mutex::new(None),
                hits: Mutex::new(hits),
            }
        }

        fn query(&self) -> SearchQuery {
            self.seen
                .lock()
                .unwrap()
                .clone()
                .expect("search must have reached the port")
        }
    }

    impl Search for SpySearch {
        fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
            *self.seen.lock().unwrap() = Some(query.clone());
            Ok(self.hits.lock().unwrap().clone())
        }
    }

    fn handler() -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
        )
    }

    /// A handler whose search port is a spy the test keeps a handle on.
    fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
        Jojobot::new(Arc::new(InMemoryMemory::new()), spy)
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
            shape: None,
            object: None,
            create_new: None,
        }
    }

    fn update_args(address: &str) -> UpdateFactArgs {
        UpdateFactArgs {
            address: address.into(),
            content: None,
            details: None,
            status: None,
            provenance: None,
            confirmed_by_user: None,
            shape: None,
            object: None,
            create_new: None,
        }
    }

    /// The JSON body of a tool result.
    fn json_of(result: &CallToolResult) -> serde_json::Value {
        serde_json::from_str(&text_of(result)).expect("tool results carry a JSON body")
    }

    /// Capture through the handler, expecting the guard to wave it through.
    async fn capture_ok(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
        let result = jojobot.capture(Parameters(args)).await.expect("capture ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    /// A tool result the guard blocked: a **successful** call whose body says
    /// nothing was written. Returns the body.
    fn blocked(result: &CallToolResult) -> serde_json::Value {
        assert_ne!(
            result.is_error,
            Some(true),
            "'needs confirmation' is an answer, not a protocol failure: {}",
            text_of(result)
        );
        let body = json_of(result);
        assert_eq!(body["status"], "blocked", "got {body}");
        assert_eq!(body["wrote"], false, "a blocked write says so in the body: {body}");
        body
    }

    /// The `address` field of a rendered fact — every read carries one.
    fn address_of(fact: &serde_json::Value) -> String {
        fact["address"]
            .as_str()
            .expect("every fact on the wire carries its address")
            .to_string()
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

    fn search_args() -> SearchArgs {
        SearchArgs {
            query: None,
            kind: None,
            status: None,
            provenance: None,
            subject: None,
            edge: None,
            limit: None,
        }
    }

    // --- search: the front door -----------------------------------------------

    /// Every argument reaches the port as the typed query it means — including the
    /// edge filter, which is the whole point of the verb.
    #[tokio::test]
    async fn search_translates_every_argument_into_the_query() {
        let spy = Arc::new(SpySearch::default());
        let jojobot = handler_with(spy.clone());
        jojobot
            .search(Parameters(SearchArgs {
                query: Some("winter".into()),
                kind: Some("person".into()),
                status: Some("superseded".into()),
                provenance: Some("testimony".into()),
                subject: Some("person:alpha".into()),
                edge: Some(EdgeFilterArgs {
                    shape: Some("location".into()),
                    object: "place:shelbyville".into(),
                }),
                limit: Some(5),
            }))
            .await
            .expect("search ok");

        let query = spy.query();
        assert_eq!(query.terms(), Some("winter"));
        assert_eq!(query.kind, Some(EntityKind::Person));
        assert_eq!(query.status, Some(FactStatus::Superseded));
        assert_eq!(query.provenance, Some(Provenance::Testimony));
        assert_eq!(query.subject.as_ref().map(|s| s.as_str()), Some("person:alpha"));
        let edge = query.edge.expect("the edge filter must survive translation");
        assert_eq!(edge.shape, Some(EdgeShape::Location));
        assert_eq!(edge.object.as_str(), "place:shelbyville");
        assert_eq!(query.limit, 5);
    }

    /// An edge filter with no shape means any edge pointing at the object, and the
    /// limit defaults to twenty.
    #[tokio::test]
    async fn a_shapeless_edge_filter_and_the_default_limit_reach_the_port() {
        let spy = Arc::new(SpySearch::default());
        handler_with(spy.clone())
            .search(Parameters(SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "event:winter-fest".into(),
                }),
                ..search_args()
            }))
            .await
            .expect("search ok");
        let query = spy.query();
        assert_eq!(query.edge.as_ref().map(|e| e.shape), Some(None));
        assert_eq!(query.limit, DEFAULT_LIMIT);
    }

    /// Neither text nor a filter is a request for everything, which is not a
    /// search — and it is the caller's mistake, whatever adapter is behind us.
    #[tokio::test]
    async fn search_with_neither_text_nor_a_filter_is_a_client_error() {
        let err = handler()
            .search(Parameters(search_args()))
            .await
            .expect_err("an unbounded search must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// Bad tokens are client errors, not silent fallbacks: a mistyped `status`
    /// that quietly became `active` would hide the anti-fact list.
    ///
    /// **Every case carries query text**, so the refusal can only be the bad
    /// token. Without it, an implementation that dropped the filter entirely
    /// would still error — as an unbounded search — and this would pass green
    /// over a `search` that ignored its filters.
    #[tokio::test]
    async fn malformed_search_filters_are_client_errors() {
        let jojobot = handler();
        let searching = || SearchArgs { query: Some("winter".into()), ..search_args() };
        let bad = [
            SearchArgs { kind: Some("receipt".into()), ..searching() },
            SearchArgs { status: Some("retired".into()), ..searching() },
            SearchArgs { provenance: Some("maybe".into()), ..searching() },
            // A *bare* subject is read as a person, as everywhere else — so the
            // malformed case is one that can't be an id at all.
            SearchArgs { subject: Some("person:a|b".into()), ..searching() },
            SearchArgs {
                edge: Some(EdgeFilterArgs { shape: Some("knows".into()), object: "place:x".into() }),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs { shape: None, object: "place:a|b".into() }),
                ..searching()
            },
            SearchArgs { limit: Some(0), ..searching() },
        ];
        for args in bad {
            let err = jojobot
                .search(Parameters(args))
                .await
                .expect_err("a malformed filter must be refused");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        }
    }

    /// **One list, every hit typed.** An entity, a fact and a prose match come
    /// back together, each saying what it is and carrying what makes it
    /// actionable — the entity its handle, the fact its whole row and address,
    /// the prose the doc to open and the text around the match.
    #[tokio::test]
    async fn search_renders_a_mixed_list_of_typed_hits() {
        let entity = Entity {
            id: EntityId::new(EntityKind::Work, "first-mix"),
            kind: EntityKind::Work,
            name: "First Mix".into(),
            source: "user-named".into(),
            crm: None,
            boot: Boot::OnDemand,
        };
        let fact = Fact {
            id: FactId("f3".into()),
            home: EntityId::person("alpha"),
            subject: EntityId::person("alpha"),
            content: "spending the winter away".into(),
            details: Some("said so in June".into()),
            provenance: Provenance::Testimony,
            status: FactStatus::Active,
            date: jiff::civil::date(2026, 7, 1),
            edge: Some(Edge::new(EdgeShape::Membership, EntityId("org:guild".into()))),
        };
        let spy = Arc::new(SpySearch::answering(vec![
            Hit::Entity { entity, doc_id: "doc-9".into() },
            Hit::Fact { fact },
            Hit::Prose {
                doc_id: "doc-1".into(),
                title: "Alpha".into(),
                entity: Some(EntityId::person("alpha")),
                snippet: "…allergic to penicillin…".into(),
            },
        ]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("winter".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(body["count"], 3);
        let results = body["results"].as_array().expect("a list of results");

        assert_eq!(results[0]["hit"], "entity");
        assert_eq!(results[0]["id"], "work:first-mix");
        assert_eq!(results[0]["type"], "CreativeWork", "the schema.org name");
        assert_eq!(results[0]["doc"], "doc-9");

        assert_eq!(results[1]["hit"], "fact");
        assert_eq!(results[1]["address"], "person:alpha#f3", "a fact hit is editable");
        assert_eq!(results[1]["subject"], "person:alpha");
        assert_eq!(results[1]["content"], "spending the winter away");
        assert_eq!(results[1]["details"], "said so in June");
        assert_eq!(results[1]["provenance"], "testimony");
        assert_eq!(results[1]["status"], "active");
        assert_eq!(results[1]["date"], "2026-07-01");
        assert_eq!(results[1]["edge"]["type"], "memberOf");
        assert_eq!(results[1]["edge"]["object"], "org:guild");

        assert_eq!(results[2]["hit"], "prose");
        assert_eq!(results[2]["doc"], "doc-1");
        assert_eq!(results[2]["title"], "Alpha");
        assert_eq!(results[2]["entity"], "person:alpha");
        assert_eq!(results[2]["snippet"], "…allergic to penicillin…");
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
        assert_eq!(body["id"], "project:atlas", "the handle keeps its lowercase kind token");
        assert_eq!(body["type"], "Project", "responses name the type, schema.org-flavored");
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
        assert_eq!(captured["subject"], "place:north-trail");
    }

    /// **The response vocabulary, whole.** Every kind renders its schema.org
    /// name — and the table is walked from `EntityKind::ALL`, so a ninth kind
    /// cannot arrive without someone deciding what it is called on the wire.
    ///
    /// The other half is the input grammar, which is **unchanged**: the names
    /// are output only, and a capitalized kind is still not a kind token.
    #[test]
    fn every_kind_renders_its_schema_org_name_and_none_is_an_input_token() {
        let table = [
            (EntityKind::Person, "person", "Person"),
            (EntityKind::Place, "place", "Place"),
            (EntityKind::Event, "event", "Event"),
            (EntityKind::Work, "work", "CreativeWork"),
            (EntityKind::Thing, "thing", "Product"),
            (EntityKind::Org, "org", "Organization"),
            (EntityKind::Topic, "topic", "Topic"),
            (EntityKind::Project, "project", "Project"),
        ];
        assert_eq!(table.len(), EntityKind::ALL.len(), "every kind must be named here");
        for (kind, token, name) in table {
            assert_eq!(kind.as_token(), token, "the input token stays lowercase");
            assert_eq!(type_name(kind), name);
            // The response name is a name, not a token: input grammar unchanged.
            if name != token {
                assert!(parse_kind(name).is_err(), "{name} must not parse as a kind");
            }
        }
        assert!(parse_kind("Person").is_err(), "a capitalized kind stays rejected");
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
                create_new: None,
            }))
            .await
            .expect("update ok");
        let body = json_of(&updated);
        assert_eq!(body["id"], "thing:red-bike", "the handle is immutable");
        assert_eq!(body["name"], "Red Bike (the gravel one)");
        assert_eq!(body["source"], "user-named", "an omitted field is left alone");
    }

    /// A rename onto a name the index already holds comes back as the same
    /// error-flagged candidates response a blocked creation does — the guard
    /// cannot be side-stepped by creating under a throwaway name and renaming.
    #[tokio::test]
    async fn a_rename_onto_an_existing_name_is_blocked() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let rename = |create_new: Option<bool>| UpdateEntityArgs {
            handle: "person:zenith".into(),
            name: Some("Alpha".into()),
            source: None,
            crm: None,
            create_new,
        };

        let result = jojobot
            .update_entity(Parameters(rename(None)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");

        // …and the name did not move.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("person".into()) }))
                .await
                .expect("list ok"),
        );
        let names: Vec<&str> = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Alpha", "Zenith"]);

        let forced = json_of(
            &jojobot
                .update_entity(Parameters(rename(Some(true))))
                .await
                .expect("confirmed rename ok"),
        );
        assert_ne!(forced["status"], "blocked");
        assert_eq!(forced["name"], "Alpha");
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
                create_new: None,
            }))
            .await
            .expect_err("unknown handle must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("thing:red-bike"), "must name the near miss: {}", err.message);
    }

    // --- the write guard, through the MCP boundary ----------------------------

    /// A guarded write comes back as a **successful** result whose body says
    /// nothing was written. "Needs confirmation" is an answer — the guard did its
    /// job and is handing the decision over — not an exception; delivering it as
    /// a protocol error made a working feature look like a broken server, and
    /// clients that retry or unwrap on error handle it exactly wrong.
    #[tokio::test]
    async fn a_blocked_add_returns_the_candidates_in_a_successful_result() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("first add ok");

        let result = jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha Two")))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
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

        let result = jojobot
            .capture(Parameters(capture_args("zenit", "should not land")))
            .await
            .expect("call ok");
        assert_eq!(blocked(&result)["candidates"][0]["handle"], "person:zenith");

        let forced = capture_ok(
            &jojobot,
            CaptureArgs { create_new: Some(true), ..capture_args("zenit", "now it lands") },
        )
        .await;
        assert_eq!(forced["subject"], "person:zenit");
    }

    // --- structured edges at capture ------------------------------------------

    /// `capture` draws a typed edge, and the edge comes back on every read of the
    /// fact — rendered with schema.org's word for the shape (`memberOf`), while
    /// the input token stays the lowercase `membership`.
    #[tokio::test]
    async fn capture_draws_an_edge_and_renders_its_schema_org_name() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:north-trail-club".into()),
                ..capture_args("alpha", "rides with the club")
            },
        )
        .await;
        assert_eq!(captured["edge"]["type"], "memberOf");
        assert_eq!(captured["edge"]["object"], "org:north-trail-club");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(recalled["facts"][0]["edge"]["type"], "memberOf");
    }

    /// Half an edge is a client error: a shape with nothing to point at, or an
    /// object with no shape, means the caller asked for an edge and would have
    /// got silence.
    #[tokio::test]
    async fn half_an_edge_is_a_client_error() {
        let jojobot = handler();
        let halves = [
            (Some("location"), None),
            (None, Some("place:north-trail")),
        ];
        for (shape, object) in halves {
            let err = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: shape.map(str::to_string),
                    object: object.map(str::to_string),
                    ..capture_args("alpha", "half an edge")
                }))
                .await
                .expect_err("half an edge must be refused");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {shape:?}/{object:?}");
        }
    }

    /// The shape set is closed, and the response spellings are not input tokens —
    /// the input grammar stays lowercase.
    #[tokio::test]
    async fn an_unknown_shape_is_a_client_error() {
        let jojobot = handler();
        for shape in ["knows", "memberOf", "Location", "attendee"] {
            let err = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: Some(shape.into()),
                    object: Some("place:north-trail".into()),
                    ..capture_args("alpha", "an unknown shape")
                }))
                .await
                .expect_err("must reject shape {shape}");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {shape}");
            assert!(
                err.message.contains("location"),
                "the error must name the closed set: {}",
                err.message
            );
        }
    }

    /// A shape's object must be the kind it requires — a `location` pointing at a
    /// person is a mis-drawn edge, and the caller hears about it.
    #[tokio::test]
    async fn a_wrong_kind_edge_object_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("person:beta".into()),
                ..capture_args("alpha", "in the wrong kind of place")
            }))
            .await
            .expect_err("a wrong-kind object must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("place"), "must say what it wanted: {}", err.message);
    }

    /// A typo'd edge object comes back as the guard's candidates — the same
    /// error-flagged response a blocked subject gets, and nothing is written.
    #[tokio::test]
    async fn a_blocked_edge_object_returns_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("place", "riverbend", "Riverbend")))
            .await
            .expect("add ok");

        let result = jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("place:riverbnd".into()),
                ..capture_args("alpha", "should not land")
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "place:riverbnd");
        assert_eq!(body["candidates"][0]["handle"], "place:riverbend");
        assert_eq!(body["candidates"][0]["type"], "Place");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["facts"].as_array().unwrap().is_empty(),
            "a blocked edge object must write no fact: {recalled}"
        );
    }

    /// `update_fact` attaches an edge to a fact that didn't have one.
    #[tokio::test]
    async fn update_fact_attaches_an_edge() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "was at the festival")).await;
        assert!(captured["edge"].is_null());

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    shape: Some("attendance".into()),
                    object: Some("event:winter-fest".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["edge"]["type"], "attendee");
        assert_eq!(updated["edge"]["object"], "event:winter-fest");
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
                    content: Some("works at the new place".into()),
                    details: Some("changed jobs in July".into()),
                    ..update_args(address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["content"], "works at the new place");
        assert_eq!(updated["details"], "changed jobs in July");
        assert_eq!(updated["address"], "person:alpha#f1");
    }

    /// **A refutation is a content edit, and `negated` is refused by name.** The
    /// rewritten row stays `active` and keeps its address — the negative truth is
    /// the current truth, so it has to be what a plain read returns. Asking for
    /// the retired status is a client error that says what to do instead, rather
    /// than an alias that would file the correction where nobody looks.
    #[tokio::test]
    async fn a_refutation_is_a_content_edit_and_negated_is_refused() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a close contact of the user")).await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("negated".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect_err("the retired status must be refused, not aliased");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("rewrite"),
            "the error must say what to do instead: {}",
            err.message
        );

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("NOT a close contact — do not re-infer".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("the refutation is an ordinary edit"),
        );
        assert_eq!(updated["status"], "active", "the negative truth is the truth");
        assert_eq!(updated["content"], "NOT a close contact — do not re-infer");
        assert_eq!(updated["address"], "person:alpha#f1", "the row keeps its address");
    }

    /// Promotion to testimony needs the explicit confirmation flag.
    #[tokio::test]
    async fn promoting_to_testimony_requires_the_confirmation_flag() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "prefers mornings")).await;
        let promote = |confirmed: Option<bool>| UpdateFactArgs {
            provenance: Some("testimony".into()),
            confirmed_by_user: confirmed,
            ..update_args(&address_of(&captured))
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
                    content: Some("nope".into()),
                    ..update_args(address)
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
                status: Some("retired".into()),
                ..update_args(&address_of(&captured))
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
        let captured = capture_ok(&jojobot, capture_args("alpha", "drinks oat milk")).await;
        assert_eq!(captured["subject"], "person:alpha");

        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(body["subject"], "person:alpha");
        let facts = body["facts"].as_array().expect("recall returns a list");
        assert!(
            facts.iter().any(|f| {
                f["address"] == captured["address"] && f["content"] == "drinks oat milk"
            }),
            "recall must return the captured fact: {body}"
        );
    }

    /// Omitting `provenance` defaults to inference (a hypothesis until confirmed).
    #[tokio::test]
    async fn provenance_defaults_to_inference() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "maybe a morning person")).await;
        assert_eq!(captured["provenance"], "inference");
    }

    /// Omitting `date` defaults to today in UTC.
    #[tokio::test]
    async fn date_defaults_to_today_utc() {
        let jojobot = handler();
        let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
        let captured = capture_ok(&jojobot, capture_args("alpha", "dated today")).await;
        assert_eq!(captured["date"], today.to_string());
    }

    /// An explicit testimony provenance is honoured.
    #[tokio::test]
    async fn explicit_testimony_is_honoured() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                date: Some("2026-01-01".into()),
                ..capture_args("alpha", "speaks two languages")
            },
        )
        .await;
        assert_eq!(captured["provenance"], "testimony");
        assert_eq!(captured["date"], "2026-01-01");
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
