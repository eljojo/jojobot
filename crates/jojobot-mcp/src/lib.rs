//! The MCP adapter — jojobot's single outward interface.
//!
//! This is the only crate that imports `rmcp`. It exposes a [`Jojobot`] server
//! handler; the binary mounts it on an HTTP transport. This layer only
//! translates MCP calls to domain calls and back, and holds no policy of its
//! own: the write guard and the promotion gate live in the domain, on the write
//! path, where no caller can route around them.
//!
//! **This file is the wiring.** The handler, the ports injected into it, the
//! summed router, and `ServerHandler`. Nothing else — a verb lives in its own
//! file, inside its context's directory, so adding one adds a file rather than
//! growing this one:
//!
//! * [`orientation`] — `ping`, `start_here`, and the boot beneath them.
//! * [`memory`] — the eight verbs over entities and facts.
//! * [`mailboxes`] — the six verbs over boxes and messages.
//! * [`session`] — the three verbs that keep a run's own record.
//!
//! Three things belong to no one context and sit beside them: [`caller`] (who
//! is asking, and which card their write lands in), [`beat`] (jojobot's own
//! account of what a session did), and [`answer`] (the success envelope). Plus
//! [`sid`], the handle registry.
//!
//! **Responses speak schema.org's words, with none of its machinery** — a kind
//! renders as `Person`/`CreativeWork`/`Organization`, an edge shape as
//! `memberOf`/`attendee`. Names only: no `@context`, no CURIEs, no JSON-LD. The
//! **input** grammar is untouched — ids and kind tokens stay lowercase
//! `kind:slug`, and a capitalized kind on input is still rejected.

use std::sync::Arc;

mod answer;
mod arguments;
mod beat;
mod boundary;
mod caller;
pub mod mailboxes;
pub mod memory;
pub mod orientation;
pub mod seed;
pub mod session;
pub mod sid;
mod status_bar;
pub(crate) use status_bar::OwnBox;

pub(crate) use answer::*;
pub(crate) use caller::*;

#[cfg(test)]
mod harness;
#[cfg(test)]
mod surface;

use jojobot_domain::mailbox::{
    self, Delivered, Delivery, Mailbox, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, guard::MailboxMatch,
};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, Memory, MemoryError, NewEntity, NewFact, Provenance, Standing,
    graph::Direction,
    guard::{self, EntityMatch},
    search::{Behind, Coverage, DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, Search, SearchQuery},
    types::{DeclaredType, Field, Fold, ValueType},
    validate_edge,
};
use jojobot_domain::session::{
    BEAT_CLASSES, BEAT_EXAMPLES, Beat, Board, EntryId, JournalEntry, NewEntry, NewSession, Session,
    SessionError, SessionId, SessionState, Sessions, beat_text, beats_of, sweep_and_find,
};
use jojobot_domain::text::{self, FRESH_FOCUS, Kept};
// **The args types keep their crate-root path.** They were `pub` here before
// the split and something outside may name them; where a type LIVES is this
// slice's business, where a caller finds it is not.
pub use mailboxes::{
    ListSentArgs, MarkProcessedArgs, PostMessageArgs, ReadMailboxArgs, ReadMessageArgs,
};
pub use memory::{
    AddEntityArgs, CaptureArgs, EdgeFilterArgs, ListEntitiesArgs, RecallArgs, RetractArgs,
    SearchArgs, SetCharterArgs, UpdateEntityArgs, UpdateFactArgs,
};
pub use orientation::OrientArgs;
pub use session::{AmendJournalArgs, JournalArgs, WrapSessionArgs};

use mailboxes::wire::*;
use memory::declined::*;
use memory::parse::*;
use memory::wire::*;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::*,
    service::{NotificationContext, RequestContext},
    tool_handler, tool_router,
};
use session::declined::*;
use session::wire::*;

// --- mailboxes ---------------------------------------------------------------

// --- sessions ----------------------------------------------------------------

#[derive(Clone)]
pub struct Jojobot {
    /// The verb table this handler dispatches through, and the source the
    /// argument gate reads a verb's published arguments from.
    tool_router: ToolRouter<Jojobot>,
    /// The Memory port. Injected: real Outline in production, a fake in tests.
    memory: Arc<dyn Memory>,
    /// The retrieval port — the search projection over the same store. Injected
    /// separately because it is a different port, not a second store: in
    /// production both are the one indexed adapter.
    search: Arc<dyn Search>,
    /// The Mailboxes port — a **separate bounded context**, with its own store
    /// and its own vocabulary. It shares nothing with Memory but this
    /// handler.
    mailboxes: Arc<dyn Mailboxes>,
    /// The Sessions port — a third context, on its own board.
    sessions: Arc<dyn Sessions>,
    /// **Every session handle this PROCESS has issued** — see [`sid`].
    ///
    /// Shared across connections rather than born with each one, which is what
    /// makes a `sid` an address: the transport builds a handler per MCP session
    /// and most clients open a fresh one per tool call, so a registry living
    /// here alone would forget each handle the moment it handed it out.
    registry: Arc<sid::SessionRegistry>,
}

/// **The verbs still living in this file.** Every context that has moved out
/// carries its own router; this one shrinks as they go, and the sum below is
/// what a client actually sees.
#[tool_router(router = core_router, vis = "pub(crate)")]
impl Jojobot {
    /// **The newest protocol revision jojobot serves in full**, and the cap on
    /// what a handshake will agree to. It is not the newest revision the SDK
    /// can name: this SDK answers a `2026-07-28` client without the fields
    /// that revision requires on a list result.
    const NEWEST_SERVED: ProtocolVersion = ProtocolVersion::V_2025_11_25;

    /// The whole surface: this file's verbs, plus every context's.
    ///
    /// **Summed, never scanned.** A verb reaches a client by its context naming
    /// it and its context being named here — the same deliberate friction
    /// `the_tool_surface_is_exactly_this_list` puts in front of a new tool.
    pub fn tool_router() -> ToolRouter<Self> {
        Self::core_router()
            + mailboxes::router()
            + memory::router()
            + orientation::router()
            + session::router()
    }

    pub fn new(
        memory: Arc<dyn Memory>,
        search: Arc<dyn Search>,
        mailboxes: Arc<dyn Mailboxes>,
        sessions: Arc<dyn Sessions>,
        registry: Arc<sid::SessionRegistry>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
            mailboxes,
            sessions,
            registry,
        }
    }

    // ── sessions ────────────────────────────────────────────────────────────
}

// --- mailboxes on the wire ---------------------------------------------------

/// **What a client is told the moment it connects** — the two worlds, in
/// miniature. Named rather than written inline so a test can read it: it
/// enumerates the entity kinds, and prose that lists a closed set goes stale
/// silently.
pub(crate) const INSTRUCTIONS: &str = "jojobot — a personal-assistant server. Two worlds live here.\
                 \n\n**MEMORY.** What jojobot knows is **entities** — a person, project, place, \
                 event, work, thing, org, topic, bot or pet, each with a typed handle, `kind:slug` \
                 — the name it is addressed by, and the only name a caller ever sends — and **facts** about them: single dated claims, each carrying an \
                 **address** (`kind:slug#local-id`) it can be edited through and a \
                 **provenance** — `testimony` (the user said or confirmed it) or `inference` \
                 (you derived it). **Inference is the default and reads back as a hypothesis, \
                 never as truth**; only the user's explicit confirmation promotes a claim. A \
                 fact may also draw one typed **edge** at another entity — `location` · \
                 `membership` · `attendance` · `about` · `connection` (a link is there and how \
                 it relates was not recorded) — and edges are what make cross-entity \
                 questions (\"which people are in X\") answerable without reading everything. \
                 **Start with `search`**: one ranked list over entities, facts and free prose, \
                 every hit arriving with its surroundings — and over mailbox messages too when \
                 you pass `include_mail: true`.\
                 \n\n**A record carries FIELDS** — a flat bag of key/value pairs beside the \
                 claim, stored and never interpreted. Nothing has to be declared first, and a \
                 key you invent is kept as you wrote it. **A thing's fields are every write on \
                 it, folded, the newest write of each key winning** — a write that takes a key \
                 off takes it off the thing — and **carrying keys is what makes a thing a \
                 type**: declare a type to say which keys it names, and a thing holding \
                 all of them fits it. Declaring admits nothing — a thing is found by the keys it \
                 carries whether or not anybody declared the type. `answers_type` selects things \
                 carrying SOME of a type's keys and says which each lacks; `fits_type` keeps \
                 only the ones with no gaps.\
                 \n\n**jojobot hands back the small answer and keeps the large one reachable.** \
                 A write returns a receipt, not the thing you wrote; a body is not echoed to its \
                 author; a delivery leaves out what it handed you once; prose is off by default \
                 on a read. Context is the scarce thing, and **eliding is never silent** — the \
                 answer says what was left out and which call returns it.\
                 \n\n**MAILBOXES.** A place to leave a message for someone who is not in this \
                 conversation. A mailbox is a named box (`[a-z0-9-]+`); a message in one is \
                 `new` → `read` → `processed`. **Read is not processed, and processed is not \
                 deleted**: reading takes delivery, processing means you acted, and `processed` \
                 is a terminal archive. **Messages are searchable, on request**: `search` with \
                 `include_mail: true` returns them beside \
                 the memory hits, in every state including the processed archive, each hit \
                 carrying its box, its state, its sender and the id `read_message` takes — so a \
                 message left for one session is findable by any of them. `read_message` takes \
                 delivery of that one message; `read_mailbox` takes the whole box, and everything \
                 in it becomes yours to finish.\
                 \n\n**Three rules of engagement.** 1. **Everything a write NAMES must already \
                 exist.** jojobot never brings an entity or a box into being as a side effect — \
                 not a capture's subject, not an edge's object, not the box you post into. \
                 Something genuinely new is two deliberate steps: create it, then write. \
                 2. **Confirm, don't guess.** A creation, or a change to what something is \
                 CALLED, that resembles something jojobot already knows comes back as a \
                 SUCCESSFUL result whose body says `status: blocked`, `wrote: false`, with \
                 `candidates` and `how_to_proceed` — nothing was written; use the candidate you \
                 meant, or re-call with the `override_token` that refusal carries if it truly is \
                 a different thing sharing a name — the token lifts the one refusal that minted \
                 it and no other. **Naming something that does not exist is blocked too**, with \
                 whatever is nearby — never a plain error, so branch on `status`, not on whether \
                 the call errored. A plain error is a malformed call, or the store failing. \
                 Nothing on this surface deletes anything. 3. **Mark a message processed only \
                 AFTER acting on it**: \
                 mark first and then fail, and it is gone from every future delivery with \
                 nobody the wiser; act first and crash, and the next read hands it back, \
                 flagged `seen_before` — recoverable.\
                 \n\nResponses name types the schema.org way (`Person`, `CreativeWork`, \
                 `memberOf`); input stays lowercase (`person`, `membership`, `kind:slug`).";

#[tool_handler]
impl ServerHandler for Jojobot {
    /// **Every call passes the argument gate before anything runs.**
    ///
    /// `#[tool_handler]` writes this method only when the impl does not, so
    /// this is the generated dispatch with one check in front of it: an
    /// argument the named verb does not implement comes back as a blocked
    /// answer and never reaches the verb — see [`crate::arguments`]. Putting it
    /// here rather than in each verb is what makes it true of every verb,
    /// including the next one somebody writes.
    ///
    /// The router is the one built with this handler rather than a fresh one
    /// per call, because the check and the dispatch have to agree about which
    /// verbs exist.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        // Read before dispatch, because dispatch consumes the request. Every
        // verb on this surface publishes `sid`, so this reaches the caller
        // without any verb having to hand it over — and it is read ahead of the
        // gate, because the gate's refusal is an answer and carries the block
        // like every other one.
        let sid = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("sid"))
            .and_then(|sid| sid.as_str())
            .map(str::to_string);
        if let Some(refused) = self.unimplemented_arguments(&request) {
            let mut answered: CallToolResponse = refused.into();
            self.add_status_bar(&mut answered, sid.as_deref()).await;
            return Ok(answered);
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let mut answered = self.tool_router.call(call).await?;
        self.add_status_bar(&mut answered, sid.as_deref()).await;
        Ok(answered)
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(INSTRUCTIONS.to_string())
    }

    /// **Tell a client, the moment it connects, that the list it is about to
    /// cache is not a constant.**
    ///
    /// jojobot's tool list does not move at runtime — it moves when a new build
    /// is deployed, which is invisible from inside a client that registered
    /// against the old one. Two sessions against one deployment once held
    /// different lists: the older one had seven memory verbs and no way to boot
    /// as its bot or read its own box, and the message waiting for it went
    /// unread while the work it named looked untouched.
    ///
    /// Initialize is the one moment jojobot knows a client is holding a list
    /// AND can still reach it, so that is when the notification goes out. It is
    /// unconditional on purpose: the server cannot know what any particular
    /// client cached, and re-listing is cheap where being stranded is silent.
    /// The capability in [`Jojobot::get_info`] is the other half — without it
    /// **jojobot agrees to speak only a revision it serves in full, and says
    /// so rather than agreeing to something else.**
    ///
    /// A handshake is a promise about the shape of everything after it: the
    /// client reads every later answer against the revision the two of them
    /// agreed. The `2026-07-28` revision makes `ttlMs` and `cacheScope`
    /// mandatory on a tool list (SEP-2549) and this SDK emits neither, so a
    /// client that agreed it discards the WHOLE list and reports the server
    /// connected while holding no verbs at all. That looks nothing like an
    /// outage from the inside and matches no runbook.
    ///
    /// **The answer is a refusal naming what jojobot does serve, not a quieter
    /// version number.** Answering with an older revision was tried and it does
    /// not work at this SDK: the transport picks its session lifecycle from the
    /// version the CLIENT asked for, never from the one that was agreed, so a
    /// downgraded client is served statelessly, gets no session, and its next
    /// call is refused as an unexpected message. A refusal carries the
    /// supported list, which is what a client needs to open again.
    ///
    /// **Raise the cap when the SDK serves the newer revision, never before.**
    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        if request.protocol_version.as_str() > Self::NEWEST_SERVED.as_str() {
            return std::future::ready(Err(McpError::unsupported_protocol_version(
                request.protocol_version,
                &self.supported_protocol_versions(),
            )));
        }
        context.peer.set_peer_info(request.clone());
        let mut info = self.get_info();
        info.protocol_version = request.protocol_version;
        std::future::ready(Ok(info))
    }

    /// The same rule at the other door: a request that declares its revision
    /// inline, with no handshake behind it, is held to the same list.
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Owned(
            ProtocolVersion::KNOWN_VERSIONS
                .iter()
                .filter(|version| version.as_str() <= Self::NEWEST_SERVED.as_str())
                .cloned()
                .collect(),
        )
    }

    /// this notification is one the client never agreed to receive.
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        if let Err(e) = context.peer.notify_tool_list_changed().await {
            // A client that has already gone is the ordinary case here, not a
            // fault: this fires on a connection that may be one request long.
            tracing::debug!("could not tell a client the tool list can move: {e}");
        }
    }
}
