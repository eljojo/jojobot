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

use jojobot_domain::mailbox::{
    self, Delivered, Delivery, Mailbox, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, guard::MailboxMatch,
};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, Memory, MemoryError, NewEntity, NewFact, Provenance,
    guard::{self, EntityMatch},
    search::{DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, Search, SearchQuery},
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
    /// The other names this one answers to — nickname, short form, initials.
    /// Screened and searched exactly as `name` is, so a nickname the user
    /// actually says is both recognized and findable. No commas.
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
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
    /// as a person). **It must already exist**: a subject jojobot doesn't know
    /// comes back with candidates and nothing is written. Create it with
    /// `add_entity` first if it is genuinely new.
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
    /// The entity the edge points at, as `kind:slug`. **It must already exist**,
    /// exactly as `subject` must — an edge into a node nobody else references is
    /// how a cross-entity question quietly starts coming back empty.
    #[serde(default)]
    pub object: Option<String>,
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
    /// The entity the edge points at, as `kind:slug`. **It must already exist** —
    /// `add_entity` first if it is genuinely new.
    #[serde(default)]
    pub object: Option<String>,
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
    /// The whole alias set, replaced. Omit to leave it alone; pass `[]` to clear
    /// it. No commas.
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    /// New source.
    #[serde(default)]
    pub source: Option<String>,
    /// New kanban token, as `card:N`.
    #[serde(default)]
    pub crm: Option<String>,
    /// Set only after a previous call reported candidates for a name or alias
    /// you are claiming here, and you judged them a different entity. Any change
    /// to what this entity is CALLED is screened exactly as a creation is.
    #[serde(default)]
    pub create_new: Option<bool>,
}

// --- mailboxes ---------------------------------------------------------------

/// Arguments to `create_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateMailboxArgs {
    /// The box's name: `[a-z0-9-]+`, starting and ending alphanumeric. One
    /// spelling per box, so two callers cannot create `Inbox` and `inbox`.
    pub name: String,
}

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// The box to leave it in. **It must already exist** — an unknown name comes
    /// back with candidates and nothing is written.
    pub mailbox: String,
    /// The message itself. Prose: paragraphs are fine.
    pub body: String,
    /// Who is sending, as you declare it. Recorded as claimed — jojobot does not
    /// resolve or verify identity yet.
    pub sender: String,
}

/// Arguments to `read_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMailboxArgs {
    /// The box to read.
    pub mailbox: String,
}

/// Arguments to `mark_processed`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkProcessedArgs {
    /// The message's id, exactly as `read_mailbox` returned it.
    pub message_id: String,
    /// What happened — including a failure. Optional, one plain line.
    #[serde(default)]
    pub notes: Option<String>,
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
    /// The Mailboxes port — a **separate bounded context**, with its own store
    /// (Vikunja) and its own vocabulary. It shares nothing with Memory but this
    /// handler.
    mailboxes: Arc<dyn Mailboxes>,
}

#[tool_router]
impl Jojobot {
    pub fn new(
        memory: Arc<dyn Memory>,
        search: Arc<dyn Search>,
        mailboxes: Arc<dyn Mailboxes>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
            mailboxes,
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
            aliases: args.aliases.unwrap_or_default(),
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
            } => Ok(blocked_result(&attempted, &candidates, Blocked::Creating)),
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
                       No hit comes back bare: a fact also names who it is `about` and whose \
                       page it is `home`d on (handle, type, name and alternateName — a null name \
                       means that handle resolves to no entity), and an entity or prose hit \
                       carries that entity's names and the `edges` its facts draw. \
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
    /// any change to what it is CALLED — name or aliases — is screened by the
    /// write guard just as a creation is.
    #[tool(
        description = "Edit an entity's metadata (name/aliases/source/crm) in place. The handle \
                       is immutable. Any change to what it is CALLED — its name or its aliases \
                       — is screened by the write guard, so it can come back status: blocked \
                       with candidates to confirm; aliases are names, so claiming one another \
                       entity already answers to is the same collision a rename is. Passing \
                       `aliases` REPLACES the whole set ([] clears it). source/crm edits are \
                       never screened. An unknown handle errors with near misses — it never \
                       creates."
    )]
    async fn update_entity(
        &self,
        Parameters(args): Parameters<UpdateEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let handle = EntityId::person(&args.handle);
        let patch = EntityPatch {
            name: args.name,
            aliases: args.aliases,
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
            } => Ok(blocked_result(&attempted, &candidates, Blocked::Relabelling)),
        }
    }

    /// Remember a fact about an entity. Returns the stored fact including the
    /// address a later `update_fact` can edit it through.
    #[tool(
        description = "Capture a fact about an entity of any kind. Returns the stored fact and \
                       its address. Every entity it names — the subject, and an edge's object — \
                       must ALREADY EXIST: one jojobot doesn't know comes back status: blocked \
                       with candidates and nothing is written. A genuinely new entity is two \
                       deliberate steps — add_entity, then capture."
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
        };
        match self.memory.capture(new).await.map_err(memory_error)? {
            Guarded::Written(fact) => json_result(&fact_json(&fact)),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, Blocked::MustExist("capture"))),
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
            } => Ok(blocked_result(&attempted, &candidates, Blocked::MustExist("update_fact"))),
        }
    }

    /// Create a mailbox. Screened against the boxes that exist, so a near miss
    /// comes back as candidates instead of a second box nobody meant.
    #[tool(
        description = "Create a mailbox. The name is [a-z0-9-]+ and has exactly one spelling. \
                       If it looks like a box that already exists, returns candidates to confirm \
                       instead of creating one — a typo that mints a box is a message posted \
                       where nobody is listening."
    )]
    async fn create_mailbox(
        &self,
        Parameters(args): Parameters<CreateMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = MailboxName(args.name.trim().to_string());
        match self
            .mailboxes
            .create_mailbox(&name)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(created) => json_result(&mailbox_json(&created)),
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(&attempted, &candidates, BlockedBox::Creating)),
        }
    }

    /// Every mailbox, with what is new, seen, and handled in each.
    #[tool(
        description = "List every mailbox with its per-state counts: new (left, never delivered) \
                       · read (delivered, not yet acted on) · processed (handled — terminal, and \
                       an archive, so nothing is ever deleted). Each box also reports its \
                       quarantined cards — cards wearing the box's label that could not be read \
                       as messages (e.g. hand-edited past parsing); they are surfaced here and \
                       delivered nowhere."
    )]
    async fn list_mailboxes(&self) -> Result<CallToolResult, McpError> {
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?;
        let body = serde_json::json!({
            "count": boxes.len(),
            "mailboxes": boxes.iter().map(mailbox_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }

    /// Leave a message in a box.
    #[tool(
        description = "Leave a message in a mailbox. It lands in `new`. The mailbox must ALREADY \
                       EXIST: an unknown name comes back status: blocked with candidates and \
                       nothing is written — call create_mailbox first if the box is genuinely \
                       new. `sender` is recorded as you declare it."
    )]
    async fn post_message(
        &self,
        Parameters(args): Parameters<PostMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let new = NewMessage {
            mailbox: MailboxName(args.mailbox.trim().to_string()),
            body: args.body,
            sender: args.sender,
            // Stamped here, at the edge, for the same reason `capture` stamps a
            // date here: the domain stays clock-free, and a caller does not get
            // to backdate a message it is posting now.
            sent_at: jiff::Timestamp::now(),
        };
        match self
            .mailboxes
            .post_message(new)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(message) => json_result(&message_json(&message)),
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::MustExist("post_message"),
            )),
        }
    }

    /// Take delivery of everything unprocessed in a box.
    #[tool(
        description = "Take delivery of every unprocessed message in a mailbox, oldest first, and \
                       move each from `new` to `read`. There is no peek: reading IS taking \
                       delivery. Messages a previous read already handed over come back too, \
                       flagged seen_before: true — those are a crashed consumer's leftovers, not \
                       fresh mail. Act on what you receive, then call mark_processed."
    )]
    async fn read_mailbox(
        &self,
        Parameters(args): Parameters<ReadMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = MailboxName(args.mailbox.trim().to_string());
        match self
            .mailboxes
            .read_mailbox(&name)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(delivery) => json_result(&delivery_json(&delivery)),
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::MustExist("read_mailbox"),
            )),
        }
    }

    /// Retire a message once it has actually been acted on.
    #[tool(
        description = "Move a message to `processed` — terminal, an archive, never a deletion — \
                       and optionally record the outcome in `notes`. \
                       THE CRASH CONTRACT: call this ONLY AFTER you have acted on the message. \
                       If you mark first and then fail, the message is gone from every future \
                       delivery and nobody will ever know it was dropped; if you act first and \
                       crash before marking, the next read_mailbox hands it back flagged as a \
                       leftover, which is recoverable. A FAILURE IS DATA, NOT A STATE: record it \
                       in notes (and reply with a new message if someone needs to know) — there \
                       is no failed column, because a message whose handling failed has still \
                       been handled."
    )]
    async fn mark_processed(
        &self,
        Parameters(args): Parameters<MarkProcessedArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = MessageId(args.message_id.trim().to_string());
        let processed = self
            .mailboxes
            .mark_processed(&id, args.notes.as_deref())
            .await
            .map_err(mailbox_error)?;
        json_result(&message_json(&processed))
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
///
/// **And every hit arrives with its surroundings.** A fact adds `about` and
/// `home` — its subject and its home doc's entity, resolved to every name they
/// answer to — and an
/// entity or a prose doc adds `edges`, where it sits in the graph. The
/// enrichment is strictly additive: `subject` is still the same handle string
/// here as in `recall`, so one record has one spelling across every verb.
fn hit_json(hit: &Hit) -> serde_json::Value {
    match hit {
        Hit::Entity { entity, doc_id, edges } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("doc".into(), doc_id.clone().into());
                obj.insert("edges".into(), edges.iter().map(edge_json).collect());
            }
            body
        }
        Hit::Fact { fact, subject, home } => {
            let mut body = fact_json(fact);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "fact".into());
                obj.insert("about".into(), entity_ref_json(subject));
                obj.insert("home".into(), entity_ref_json(home));
            }
            body
        }
        Hit::Prose {
            doc_id,
            title,
            entity,
            edges,
            snippet,
        } => serde_json::json!({
            "hit": "prose",
            "doc": doc_id,
            "title": title,
            "entity": entity.as_ref().map(entity_json),
            "edges": edges.iter().map(edge_json).collect::<Vec<_>>(),
            "snippet": snippet,
        }),
    }
}

/// A handle the reader can act on **and** understand: the id, the kind, and the
/// display name when the store knows one.
///
/// `name` is null for a handle that resolves to nothing — the orphan case. It is
/// left null rather than filled with the handle: an unresolvable subject is a
/// real condition, and hiding it behind a plausible string is how it went
/// unnoticed for a milestone.
fn entity_ref_json(reference: &EntityRef) -> serde_json::Value {
    serde_json::json!({
        "id": reference.id.as_str(),
        "type": reference.kind.map(type_name),
        "name": reference.name,
        // Same key an entity hit uses for the same idea — the asker who typed a
        // nickname has to see it here, or the hit answers a question they did
        // not ask under a name they do not recognize.
        "alternateName": reference.aliases,
    })
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
        // schema.org's word for the same idea, and SKOS's split: one preferred
        // label, any number of alternate ones.
        "alternateName": entity.aliases,
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

/// Which gate stopped a write — because the way out of each one is different,
/// and one copy-pasted paragraph telling a rename to "pick a more qualified
/// slug" is worse than no advice at all.
enum Blocked {
    /// A creation: the handle is being minted here, so an exact collision is
    /// unforgivable and `create_new` covers only a shared *name*.
    Creating,
    /// A relabel — a change to a name or an alias. No handle is moving, so
    /// nothing here is unforgivable.
    Relabelling,
    /// A write that only **names** an entity (a capture's subject, an edge's
    /// object). It cannot create one, so `create_new` does not exist on it.
    MustExist(&'static str),
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
fn blocked_result(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    gate: Blocked,
) -> CallToolResult {
    let exact = candidates
        .iter()
        .any(|c| c.reason == guard::MatchReason::ExactHandle);
    let how_to_proceed = match gate {
        Blocked::Creating if exact => format!(
            "Nothing was written. The handle '{attempted}' is already taken, and that cannot be \
             forced — a handle has exactly one owner. Either this IS the entity above (use its \
             handle and carry on), or it is a different one and needs a more qualified slug.",
        ),
        Blocked::Creating => format!(
            "Nothing was written. If '{attempted}' IS one of the entities above, use that handle \
             instead. If it is genuinely a different one that happens to share a name, re-call \
             add_entity with create_new: true — display names are not unique and never have to \
             be; the handle is what has to be.",
        ),
        // Says "name" rather than "rename": this gate fires on an alias write
        // too, and telling a caller nothing was renamed when they renamed
        // nothing sends them looking for a rename they never made.
        Blocked::Relabelling => format!(
            "Nothing was written, and the handle '{attempted}' is unaffected either way — this \
             only moves the names it answers to. Either pick a name or alias that isn't already \
             worn, or re-call update_entity with create_new: true if this entity really does \
             share a name with one above: names are not unique, handles are.",
        ),
        // The candidate list is often empty here — this gate fires on any
        // unrecognized handle, not only a near miss — so the advice must not
        // point at "the handles above" when there are none.
        Blocked::MustExist(verb) if candidates.is_empty() => format!(
            "Nothing was written. '{attempted}' is not an entity jojobot knows, and nothing \
             resembles it. {verb} cannot create an entity: call add_entity to create \
             '{attempted}' first, then re-call {verb}.",
        ),
        Blocked::MustExist(verb) => format!(
            "Nothing was written. '{attempted}' is not an entity jojobot knows. If one of the \
             handles above is what you meant, use that. Otherwise {verb} cannot create it for \
             you — call add_entity to create '{attempted}' first, then re-call {verb}.",
        ),
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

// --- mailboxes on the wire ---------------------------------------------------

/// A mailbox on the wire: its name, what is in it per state, and what is in it
/// that could not be read — a caller must see "N unreadable" rather than
/// nothing, because a quarantined card is invisible to every other verb.
fn mailbox_json(mailbox: &Mailbox) -> serde_json::Value {
    serde_json::json!({
        "name": mailbox.name.as_str(),
        "counts": {
            "new": mailbox.counts.new,
            "read": mailbox.counts.read,
            "processed": mailbox.counts.processed,
            "total": mailbox.counts.total(),
        },
        "quarantined": {
            "count": mailbox.quarantined.len(),
            "card_ids": mailbox.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        },
    })
}

/// A message on the wire. Rendered by hand rather than derived, so
/// `post_message`, `read_mailbox` and `mark_processed` cannot drift into three
/// spellings of one record — the same rule the fact renderer follows.
fn message_json(message: &Message) -> serde_json::Value {
    serde_json::json!({
        "id": message.id.as_str(),
        "mailbox": message.mailbox.as_str(),
        "sender": message.sender,
        "sent_at": message.sent_at.to_string(),
        "body": message.body,
        "state": message.state.as_token(),
        "notes": message.notes,
    })
}

/// One delivered message: the whole record, plus whether a previous read had
/// already handed it over.
fn delivered_json(delivered: &Delivered) -> serde_json::Value {
    let mut body = message_json(&delivered.message);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("seen_before".into(), delivered.seen_before.into());
    }
    body
}

/// A whole delivery.
fn delivery_json(delivery: &Delivery) -> serde_json::Value {
    serde_json::json!({
        "mailbox": delivery.mailbox.as_str(),
        "count": delivery.messages.len(),
        "messages": delivery.messages.iter().map(delivered_json).collect::<Vec<_>>(),
    })
}

/// One of the mailbox guard's candidates on the wire.
fn mailbox_candidate_json(candidate: &MailboxMatch) -> serde_json::Value {
    serde_json::json!({
        "name": candidate.name.as_str(),
        "reason": match candidate.reason {
            mailbox::guard::MatchReason::Exact => "exact",
            mailbox::guard::MatchReason::Near => "near",
            mailbox::guard::MatchReason::Contains => "contains",
        },
    })
}

/// Which mailbox gate stopped a write — because the way out of each is
/// different, and one copy-pasted paragraph telling a create to "call
/// create_mailbox" is advice that goes in a circle.
enum BlockedBox {
    /// A creation: the name is being minted here.
    Creating,
    /// A write that only **names** a box. It cannot create one.
    MustExist(&'static str),
}

/// The mailbox guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant. A successful result carrying a structured payload, not a
/// protocol error — the same shape the Memory verbs use, so one client-side
/// branch handles both contexts.
fn mailbox_blocked(
    attempted: &MailboxName,
    candidates: &[MailboxMatch],
    gate: BlockedBox,
) -> CallToolResult {
    let how_to_proceed = match gate {
        BlockedBox::Creating => format!(
            "Nothing was created. '{attempted}' is the same as, or a near miss of, a mailbox \
             that already exists. If one of the boxes above is the one you meant, use its name; \
             otherwise pick a name that is not confusable with it. Two boxes with names a typo \
             apart cannot both be right: the whole value of a box is that a sender can name it \
             without checking.",
        ),
        BlockedBox::MustExist(verb) if candidates.is_empty() => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows, and nothing \
             resembles it. {verb} cannot create one: call create_mailbox to create '{attempted}' \
             first, then re-call {verb}.",
        ),
        BlockedBox::MustExist(verb) => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows. If one of the \
             names above is what you meant, use that — it is almost certainly a typo. Otherwise \
             {verb} cannot create the box for you: call create_mailbox first, then re-call \
             {verb}.",
        ),
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(mailbox_candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// Map a domain [`MailboxError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split [`memory_error`] makes.
fn mailbox_error(e: MailboxError) -> McpError {
    match e {
        MailboxError::InvalidName(_)
        | MailboxError::InvalidMessageId(_)
        | MailboxError::InvalidMessage(_)
        | MailboxError::UnknownMessage { .. } => McpError::invalid_params(e.to_string(), None),
        // Not a caller mistake and not something a caller can fix by calling
        // differently: jojobot found a card on its own board that belongs to
        // another project, and refused. That is an integrity condition on the
        // server side.
        MailboxError::ForeignProject(_) => McpError::internal_error(e.to_string(), None),
        MailboxError::NotConfigured(msg) => {
            McpError::internal_error(format!("mailboxes not configured: {msg}"), None)
        }
        MailboxError::Store(msg) => McpError::internal_error(msg, None),
    }
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
                 at once, so you do not have to know where something was filed — and every hit \
                 arrives with its surroundings, so a result is readable without a second call. \
                 Tools: `search` \
                 · `ping` (connectivity) · `add_entity` · `capture` (remember a fact) · `recall` \
                 (every fact about one entity) · `update_fact` (edit in place; to refute a \
                 claim, rewrite its content to state the negative truth — there is no negated \
                 status) · `update_entity` (metadata only) · `list_entities`. A fact may \
                 also draw one typed **edge** — pass `shape` (location · membership · \
                 attendance · about) with `object`, the entity it points at; that is what \
                 makes cross-entity questions answerable. Three rules to expect: **every entity \
                 a write NAMES must already exist** — a capture's subject and an edge's object \
                 are never created for you, so a new one is add_entity and then the write, two \
                 deliberate steps; a **creation or rename** that looks like an entity jojobot \
                 already knows comes back with candidates and writes nothing until you confirm \
                 or pass create_new; and promoting a claim from inference to testimony needs the \
                 user's explicit confirmation. A guarded write is a SUCCESSFUL result whose body \
                 says `status: blocked`, `wrote: false` — read how_to_proceed, do not retry it \
                 unchanged. Responses name types the schema.org way \
                 (`Person`, `CreativeWork`, `memberOf`); input stays lowercase `kind:slug`. \
                 \n\nSeparately, jojobot carries **mailboxes** — a place to leave a message for \
                 a consumer that is not in this conversation. A mailbox is a named box \
                 (`[a-z0-9-]+`) and a message moves through three states, which are columns: \
                 `new` → `read` → `processed`, with `processed` terminal and an archive — \
                 nothing is ever deleted. Tools: `create_mailbox` · `list_mailboxes` (per-state \
                 counts) · `post_message` · `read_mailbox` · `mark_processed`. The same \
                 must-already-exist rule applies: posting into a box jojobot doesn't know comes \
                 back blocked with candidates, because a typo that mints a box is a message \
                 posted where nobody is listening. `read_mailbox` IS taking delivery — there is \
                 no peek — and it re-delivers anything a previous read handed over, flagged \
                 `seen_before`. **Mark a message processed only AFTER acting on it**: marking \
                 first and then failing drops it from every future delivery with nobody the \
                 wiser. \
                 More domain verbs arrive later."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
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
            Arc::new(InMemoryMailboxes::new()),
        )
    }

    /// A handler whose search port is a spy the test keeps a handle on.
    fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            spy,
            Arc::new(InMemoryMailboxes::new()),
        )
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
        }
    }

    /// The JSON body of a tool result.
    fn json_of(result: &CallToolResult) -> serde_json::Value {
        serde_json::from_str(&text_of(result)).expect("tool results carry a JSON body")
    }

    /// Make sure a handle names an entity, so the write guard's **existence
    /// gate** is not what a spec about something else trips over. Idempotent —
    /// an add that comes back blocked means it is already there.
    async fn ensure(jojobot: &Jojobot, handle: &str) {
        let id = EntityId::person(handle);
        let kind = id.kind().expect("test handles are well-formed");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                kind: kind.as_token().into(),
                handle: id.slug().into(),
                name: id.slug().into(),
                aliases: None,
                source: "test-fixture".into(),
                crm: None,
                boot: None,
                create_new: None,
            }))
            .await
            .expect("add_entity call ok");
    }

    /// Capture through the handler, expecting the guard to wave it through —
    /// provisioning the subject and any edge object first, because every write
    /// that names an entity now requires one that exists.
    async fn capture_ok(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
        ensure(jojobot, &args.subject).await;
        if let Some(object) = args.object.as_deref() {
            ensure(jojobot, object).await;
        }
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
            aliases: None,
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
    /// that quietly became `active` would answer a question about superseded
    /// rows with the live ones and look like a straight answer.
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

    /// **One list, every hit typed — and none of them bare.** An entity, a fact
    /// and a prose match come back together, each saying what it is, carrying
    /// what makes it actionable, *and* carrying its surroundings: the fact names
    /// the entities it is about and sits on, the entity and the prose doc carry
    /// the edges that place them in the graph.
    #[tokio::test]
    async fn search_renders_a_mixed_list_of_typed_hits() {
        let entity = Entity {
            id: EntityId::new(EntityKind::Work, "first-mix"),
            kind: EntityKind::Work,
            name: "First Mix".into(),
            aliases: vec!["The First One".into()],
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
        let alpha = Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::Person,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "user-named".into(),
            crm: None,
            boot: Boot::OnDemand,
        };
        let guild = Edge::new(EdgeShape::Membership, EntityId("org:guild".into()));
        let spy = Arc::new(SpySearch::answering(vec![
            Hit::Entity {
                entity,
                doc_id: "doc-9".into(),
                edges: vec![guild.clone()],
            },
            Hit::Fact {
                fact,
                subject: EntityRef::resolved(&alpha),
                home: EntityRef::resolved(&alpha),
            },
            Hit::Prose {
                doc_id: "doc-1".into(),
                title: "Alpha".into(),
                entity: Some(alpha.clone()),
                edges: vec![guild],
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
        assert_eq!(results[0]["edges"][0]["type"], "memberOf", "where it sits in the graph");
        assert_eq!(results[0]["edges"][0]["object"], "org:guild");

        assert_eq!(results[1]["hit"], "fact");
        assert_eq!(results[1]["address"], "person:alpha#f3", "a fact hit is editable");
        assert_eq!(
            results[1]["subject"], "person:alpha",
            "the row keeps one spelling across capture, recall and search"
        );
        assert_eq!(results[1]["content"], "spending the winter away");
        assert_eq!(results[1]["details"], "said so in June");
        assert_eq!(results[1]["provenance"], "testimony");
        assert_eq!(results[1]["status"], "active");
        assert_eq!(results[1]["date"], "2026-07-01");
        assert_eq!(results[1]["edge"]["type"], "memberOf");
        assert_eq!(results[1]["edge"]["object"], "org:guild");
        // …and the surroundings, resolved: who this is about, and whose page it
        // sits on. A handle alone costs the reader a call to find out.
        assert_eq!(results[1]["about"]["id"], "person:alpha");
        assert_eq!(results[1]["about"]["type"], "Person");
        assert_eq!(results[1]["about"]["name"], "Alpha");
        assert_eq!(results[1]["home"]["id"], "person:alpha");
        assert_eq!(results[1]["home"]["name"], "Alpha");
        // …under the same key an entity hit uses, so one shape means one thing.
        assert_eq!(
            results[1]["about"]["alternateName"][0], "Al",
            "a search on the nickname has to show the linkage on the hit itself"
        );
        assert_eq!(results[1]["home"]["alternateName"][0], "Al");

        assert_eq!(results[2]["hit"], "prose");
        assert_eq!(results[2]["doc"], "doc-1");
        assert_eq!(results[2]["title"], "Alpha");
        assert_eq!(results[2]["entity"]["id"], "person:alpha");
        assert_eq!(results[2]["entity"]["name"], "Alpha");
        assert_eq!(
            results[2]["entity"]["alternateName"][0], "Al",
            "the names it answers to come with it"
        );
        assert_eq!(results[2]["edges"][0]["object"], "org:guild");
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
                aliases: None,
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
            aliases: None,
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

    /// **The guard's last door, through the real handler.** A patch carrying
    /// only aliases renames nothing, so nothing used to screen it — and the
    /// advice it gets back must not describe a rename the caller never made.
    #[tokio::test]
    async fn an_alias_onto_a_taken_name_is_blocked_and_says_so_in_its_own_words() {
        let jojobot = handler();
        for (handle, name) in [("homer-simpson", "Homer Simpson"), ("zenith", "Zenith")] {
            jojobot
                .add_entity(Parameters(add_args("person", handle, name)))
                .await
                .expect("add ok");
        }

        let result = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "person:zenith".into(),
                name: None,
                aliases: Some(vec!["Homer Simpson".into()]),
                source: None,
                crm: None,
                create_new: None,
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:homer-simpson");
        let advice = body["how_to_proceed"].as_str().expect("advice is a string");
        assert!(
            advice.contains("alias"),
            "the advice must name the thing that was actually refused: {advice}"
        );
        assert!(
            !advice.contains("renamed"),
            "nothing was renamed — telling them so sends them hunting for a rename: {advice}"
        );

        // …and the alias did not land.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("person".into()) }))
                .await
                .expect("list ok"),
        );
        let zenith = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "person:zenith")
            .expect("zenith is still there");
        assert_eq!(
            zenith["alternateName"].as_array().map(Vec::len),
            Some(0),
            "a blocked alias write lands nothing: {zenith}"
        );
    }

    /// **Alternate names go in and come back**, under schema.org's word for
    /// them. `update_entity` replaces the set whole — including with nothing,
    /// because "it has none" is a thing a caller must be able to say.
    #[tokio::test]
    async fn an_entity_carries_its_alternate_names_through_the_handler() {
        let jojobot = handler();
        let added = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    aliases: Some(vec!["Cosme Fulanito".into(), "H.".into()]),
                    ..add_args("person", "homer-simpson", "Homer Simpson")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(added["alternateName"][0], "Cosme Fulanito");
        assert_eq!(added["alternateName"][1], "H.");

        let patch = |aliases: Vec<String>| UpdateEntityArgs {
            handle: "person:homer-simpson".into(),
            name: None,
            aliases: Some(aliases),
            source: None,
            crm: None,
            create_new: None,
        };

        let replaced = json_of(
            &jojobot
                .update_entity(Parameters(patch(vec!["Cosme Fulanito".into()])))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            replaced["alternateName"].as_array().expect("a list").len(),
            1,
            "the set is replaced, not appended to: {replaced}"
        );

        let cleared = json_of(
            &jojobot
                .update_entity(Parameters(patch(Vec::new())))
                .await
                .expect("update ok"),
        );
        assert!(cleared["alternateName"].as_array().expect("a list").is_empty());

        // An alias carrying the separator is a client error, not a silent split.
        let err = jojobot
            .add_entity(Parameters(AddEntityArgs {
                aliases: Some(vec!["one, two".into()]),
                ..add_args("person", "comma-carrier", "Comma Carrier")
            }))
            .await
            .expect_err("a comma in an alias must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
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
                aliases: None,
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

    /// **Capture's subject must exist**, near miss or complete stranger, and the
    /// way through is `add_entity` — never a flag. The advice in the payload has
    /// to say that, because the AI reading it is the only thing that acts on it:
    /// telling it to pass a `create_new` that no longer exists on this verb
    /// would send it round a loop it can't get out of.
    #[tokio::test]
    async fn a_blocked_capture_says_to_add_the_entity_first() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let near = jojobot
            .capture(Parameters(capture_args("zenit", "should not land")))
            .await
            .expect("call ok");
        let body = blocked(&near);
        assert_eq!(body["candidates"][0]["handle"], "person:zenith");
        // The near-miss branch has its own copy, and it has to earn its keep: the
        // candidate list is the whole reason this case differs from a stranger,
        // so the advice must point at it rather than repeat the stranger's text.
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("above"),
            "with candidates in hand, the advice must point at them: {advice}"
        );
        assert!(advice.contains("add_entity"), "…and still name the way through: {advice}");
        assert!(
            !advice.contains("nothing resembles it"),
            "something does resemble it — that is what the candidates are: {advice}"
        );
        assert!(
            !advice.contains("create_new"),
            "capture has no create_new, near miss or not: {advice}"
        );

        // A handle nothing resembles blocks too, with nothing to suggest.
        let stranger = jojobot
            .capture(Parameters(capture_args("work:first-mix", "32 tracks")))
            .await
            .expect("call ok");
        let body = blocked(&stranger);
        assert_eq!(body["attempted"], "work:first-mix");
        assert!(body["candidates"].as_array().unwrap().is_empty(), "got {body}");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(advice.contains("add_entity"), "must name the way through: {advice}");
        assert!(
            !advice.contains("create_new: true"),
            "capture has no create_new; advising it sends the caller round a loop \
             with no exit: {advice}"
        );
        assert!(
            !advice.contains("above"),
            "there are no candidates above to point at: {advice}"
        );

        // Two deliberate steps, and it lands.
        jojobot
            .add_entity(Parameters(add_args("work", "first-mix", "First Mix")))
            .await
            .expect("add ok");
        let landed = capture_ok(&jojobot, capture_args("work:first-mix", "32 tracks")).await;
        assert_eq!(landed["subject"], "work:first-mix");
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
        // The subject faces the gate too, and the guard reports the first handle
        // it stops — this spec is about the object.
        ensure(&jojobot, "alpha").await;

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
        ensure(&jojobot, "event:winter-fest").await;

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

    /// **An unknown handle is a miss at the wire too.** The production smoke
    /// test asked for a nonexistent person and was told "reads fine, no facts"
    /// — the same answer an empty page gives, so a caller can never repair a
    /// bad handle. The miss now comes back as an error naming the handle and
    /// its near candidates, while an empty-but-real entity still reads fine.
    #[tokio::test]
    async fn recall_of_an_unknown_entity_is_a_miss_with_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let err = jojobot
            .recall(Parameters(RecallArgs { subject: "person:zenit".into() }))
            .await
            .expect_err("an unknown entity must not read as an empty page");
        let msg = format!("{err:?}");
        assert!(msg.contains("no entity"), "the miss names itself: {msg}");
        assert!(msg.contains("zenith"), "the near candidate surfaces: {msg}");

        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "person:zenith".into() }))
                .await
                .expect("an existing entity's empty page still reads"),
        );
        assert_eq!(body["facts"].as_array().expect("a list").len(), 0);
    }

    /// **`recall` shows the edges too.** Search grew a neighborhood; a recall
    /// that answered with the same rows stripped of their edges would make the
    /// graph a thing you can only see by searching for it, and reading an
    /// entity's own page is the commonest way anyone looks.
    #[tokio::test]
    async fn recall_returns_the_edge_a_fact_draws() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("org", "guild", "The Guild")))
            .await
            .expect("add_entity ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:guild".into()),
                ..capture_args("alpha", "joined in the spring")
            },
        )
        .await;

        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "alpha".into() }))
                .await
                .expect("recall ok"),
        );
        let edged = body["facts"]
            .as_array()
            .expect("recall returns a list")
            .iter()
            .find(|f| f["content"] == "joined in the spring")
            .unwrap_or_else(|| panic!("the captured fact must come back: {body}"));
        assert_eq!(edged["edge"]["type"], "memberOf", "got {edged}");
        assert_eq!(edged["edge"]["object"], "org:guild");
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

    // --- mailboxes ------------------------------------------------------------

    fn mailbox_handler() -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
        )
    }

    async fn make_box(jojobot: &Jojobot, name: &str) -> serde_json::Value {
        let result = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs { name: name.into() }))
            .await
            .expect("create_mailbox call ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    async fn send(jojobot: &Jojobot, mailbox: &str, sender: &str, body: &str) -> serde_json::Value {
        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: mailbox.into(),
                sender: sender.into(),
                body: body.into(),
            }))
            .await
            .expect("post_message call ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    /// The whole arc through the MCP surface: make a box, leave a message, see
    /// it as new, take delivery, mark it handled.
    #[tokio::test]
    async fn the_mailbox_arc_through_the_handler() {
        let jojobot = mailbox_handler();
        let created = make_box(&jojobot, "inbox").await;
        assert_eq!(created["name"], "inbox");
        assert_eq!(created["counts"]["new"], 0);

        let posted = send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        assert_eq!(posted["mailbox"], "inbox");
        assert_eq!(posted["sender"], "alpha");
        assert_eq!(posted["body"], "the shipment landed");
        assert_eq!(posted["state"], "new");
        assert!(posted["sent_at"].is_string(), "a message says when it was sent");
        let id = posted["id"].as_str().expect("a message carries its id").to_string();

        let listed = json_of(&jojobot.list_mailboxes().await.expect("list ok"));
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["mailboxes"][0]["name"], "inbox");
        assert_eq!(listed["mailboxes"][0]["counts"]["new"], 1);

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbox".into() }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["mailbox"], "inbox");
        assert_eq!(delivery["count"], 1);
        assert_eq!(delivery["messages"][0]["id"], id);
        assert_eq!(delivery["messages"][0]["state"], "read", "delivery moves the column");
        assert_eq!(
            delivery["messages"][0]["seen_before"], false,
            "a first delivery is nobody's leftover"
        );

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some("filed under shipments".into()),
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(processed["state"], "processed");
        assert_eq!(processed["notes"], "filed under shipments");

        let after = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbox".into() }))
                .await
                .expect("read ok"),
        );
        assert_eq!(after["count"], 0, "a processed message is never delivered again");
    }

    /// **A crashed consumer's leftovers are visible as such.** A second read
    /// hands the same message back flagged, rather than as fresh mail.
    #[tokio::test]
    async fn a_redelivered_message_says_it_was_seen_before() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        jojobot
            .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbox".into() }))
            .await
            .expect("read ok");

        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbox".into() }))
                .await
                .expect("read ok"),
        );
        assert_eq!(again["count"], 1);
        assert_eq!(again["messages"][0]["seen_before"], true);
    }

    /// **Blocked is a result, not a protocol error** — the same shape the Memory
    /// verbs use, so one client-side branch handles both contexts.
    #[tokio::test]
    async fn posting_into_an_unknown_box_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "inbx".into(),
                sender: "alpha".into(),
                body: "the shipment landed".into(),
            }))
            .await
            .expect("a blocked post is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "inbx");
        assert_eq!(body["candidates"][0]["name"], "inbox");
        assert_eq!(body["candidates"][0]["reason"], "near");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("create_mailbox"),
            "the way out of this gate is naming the verb that opens it: {advice}"
        );
    }

    /// Creating a box that looks like one already there is blocked too — and its
    /// advice is a different one, because the way out is different.
    #[tokio::test]
    async fn creating_a_near_miss_box_is_blocked_with_its_own_advice() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs { name: "inbx".into() }))
            .await
            .expect("a blocked create is a successful call");
        let body = blocked(&result);
        assert_eq!(body["candidates"][0]["name"], "inbox");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            !advice.contains("create_mailbox"),
            "telling a create to call create is advice that goes in a circle: {advice}"
        );
    }

    /// Reading a box jojobot doesn't know is blocked — never an empty delivery,
    /// which would read as "your box is empty" for a name that does not exist.
    #[tokio::test]
    async fn reading_an_unknown_box_is_blocked_rather_than_empty() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let result = jojobot
            .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbx".into() }))
            .await
            .expect("a blocked read is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "inbx");
    }

    /// Malformed input is a client error that says what the grammar is, rather
    /// than a store failure or a silently-normalized name.
    #[tokio::test]
    async fn malformed_mailbox_input_is_a_client_error() {
        let jojobot = mailbox_handler();
        let err = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs { name: "Inbox".into() }))
            .await
            .expect_err("a name outside the grammar must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        make_box(&jojobot, "inbox").await;
        let err = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "inbox".into(),
                sender: "  ".into(),
                body: "the shipment landed".into(),
            }))
            .await
            .expect_err("a message with no sender has no provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// An id nothing answers to is a client error carrying the id that missed —
    /// never a silent success, which would look exactly like a handled message.
    #[tokio::test]
    async fn processing_an_unknown_message_is_a_client_error() {
        let jojobot = mailbox_handler();
        let err = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "999999".into(),
                notes: None,
            }))
            .await
            .expect_err("an unknown id must not report success");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("999999"), "got {}", err.message);
    }

    /// **The mailbox surface is five verbs, and none of them deletes.**
    /// Production jojobot never deletes anything — the standing rule is
    /// structural (the store's port has no delete operation at all), and this
    /// pins the other end: no delete-shaped capability is ever exposed over
    /// MCP, for mailboxes or anything else.
    #[test]
    fn the_mailbox_surface_is_exactly_five_verbs_and_none_deletes() {
        let tools = Jojobot::tool_router().list_all();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        let mailbox_verbs = [
            "create_mailbox",
            "list_mailboxes",
            "post_message",
            "read_mailbox",
            "mark_processed",
        ];
        let mailbox_shaped: std::collections::BTreeSet<&str> = names
            .iter()
            .copied()
            .filter(|n| {
                n.contains("mailbox") || n.contains("message") || n.contains("processed")
            })
            .collect();
        assert_eq!(
            mailbox_shaped,
            mailbox_verbs.iter().copied().collect(),
            "the mailbox surface is exactly the five verbs, no more, no fewer"
        );

        for shape in ["delete", "remove", "destroy", "purge", "erase", "drop"] {
            assert!(
                names.iter().all(|n| !n.contains(shape)),
                "no tool may be {shape}-shaped: {names:?}"
            );
        }
    }

    /// **The crash contract is in the tool description, not only in the docs.**
    /// A consumer that marks first and then fails drops the message silently;
    /// the model reading this surface has to be told which order is safe.
    #[test]
    fn the_mark_processed_description_states_the_crash_contract() {
        let tools = Jojobot::tool_router().list_all();
        let mark = tools
            .iter()
            .find(|t| t.name == "mark_processed")
            .expect("mark_processed is a tool");
        let description = mark.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("ONLY AFTER"),
            "the crash contract must be stated where a consumer reads it: {description}"
        );
    }
}
