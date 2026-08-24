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
mod clock;
pub mod mailboxes;
pub mod memory;
pub mod orientation;
pub mod views;

/// **Everything this build supplies, in one place.**
///
/// A capability ships data by adding an entry here — the charter's text, the
/// views, and whatever comes next. **One door**, so a reader can see the whole
/// of what the software puts into an instance without hunting for it, and so
/// adding a capability is adding data rather than wiring.
pub fn provisions() -> jojobot_domain::memory::owned::Provisions {
    let mut supplied = orientation::charter::provisions();
    supplied.extend(views::provisions());
    supplied
}
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
pub use answer::Receipts;
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
    /// **The carriers the owed read asks when a thing falls due.**
    ///
    /// Held here rather than built inside the read, for one reason: a read that
    /// makes its own list can only ever be asked about the carriers this build
    /// ships, so nothing can show that a carrier it has never seen appears in
    /// its answer untouched. **That demonstration is the only difference
    /// between a general read and one that looks general.**
    ///
    /// It is a field and not a registry: the narrowest thing that lets a caller
    /// hand the read a carrier is a list it was given, and a plug-in mechanism
    /// would be machinery guarding a case nobody has.
    carriers: Arc<Vec<Box<dyn jojobot_domain::attention::Carrier>>>,
    /// **Which of the two computed lines a write's receipt carries.**
    ///
    /// A field rather than a constant because the two ship behind their own
    /// switches: the run that measures whether either changes how an agent
    /// writes needs one of them held still.
    receipts: answer::Receipts,
    /// **The clock this server runs on.** The real one unless an operator
    /// stated a day for the whole run — see [`jojobot_domain::clock::Clock`].
    ///
    /// A field for the same reason `receipts` is one: the handler is built per
    /// connection, so a decision read once at startup has to reach every
    /// handler the factory makes after it.
    clock: jojobot_domain::clock::Clock,
}

/// **The verbs still living in this file.** Every context that has moved out
/// carries its own router; this one shrinks as they go, and the sum below is
/// what a client actually sees.
#[tool_router(router = core_router, vis = "pub(crate)")]
impl Jojobot {
    /// **The newest protocol revision jojobot serves in full**, and the cap on
    /// what a handshake will agree to. It is the newest revision the SDK can
    /// name, because the SDK now emits the fields `2026-07-28` requires on a
    /// list result. It is a separate constant from the SDK's own newest so
    /// that what jojobot agrees to speak stays a decision this code makes.
    const NEWEST_SERVED: ProtocolVersion = ProtocolVersion::V_2026_07_28;

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
        Self::carrying(
            memory,
            search,
            mailboxes,
            sessions,
            registry,
            jojobot_domain::attention::shipped(),
        )
    }

    /// The same handler, told which carriers answer for a due moment.
    ///
    /// [`Jojobot::new`] is this with the ones the software ships, which is what
    /// production wants. A caller passing its own is how a carrier the read has
    /// never seen gets in front of it.
    pub fn carrying(
        memory: Arc<dyn Memory>,
        search: Arc<dyn Search>,
        mailboxes: Arc<dyn Mailboxes>,
        sessions: Arc<dyn Sessions>,
        registry: Arc<sid::SessionRegistry>,
        carriers: Vec<Box<dyn jojobot_domain::attention::Carrier>>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
            mailboxes,
            sessions,
            registry,
            carriers: Arc::new(carriers),
            receipts: answer::Receipts::default(),
            clock: jojobot_domain::clock::Clock::default(),
        }
    }

    /// The same handler, running on a stated day rather than the real clock.
    ///
    /// [`Jojobot::new`] is this on the real clock, which is what every instance
    /// gets unless an operator states a day.
    #[must_use]
    pub fn on_clock(self, clock: jojobot_domain::clock::Clock) -> Self {
        Self { clock, ..self }
    }

    /// The same handler, told which computed lines its write receipts carry.
    ///
    /// [`Jojobot::new`] is this with both of them, which is what a caller with
    /// no opinion should get. An operator turns one off to measure the other.
    #[must_use]
    pub fn receipting(self, receipts: answer::Receipts) -> Self {
        Self { receipts, ..self }
    }

    /// What this handler asks when it needs to know if something is owed.
    pub(crate) fn carriers(&self) -> Vec<&dyn jojobot_domain::attention::Carrier> {
        self.carriers
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect()
    }

    // ── sessions ────────────────────────────────────────────────────────────
}

// --- mailboxes on the wire ---------------------------------------------------

/// **What a client is told the moment it connects** — the two worlds, in
/// miniature. Named rather than written inline so a test can read it: it
/// enumerates the entity kinds, and prose that lists a closed set goes stale
/// silently.
/// **Who is answering, and which build of it** — read where jojobot is
/// compiled rather than where the library is.
///
/// The SDK ships `Implementation::from_build_env`, and it does what it says in
/// the crate it was compiled in: it is a function inside the library, so its
/// `env!` reads the LIBRARY's crate name and version. A server built with it
/// introduces itself to every client as the library — the first thing a client
/// learns about jojobot, and true of the machinery rather than of jojobot
/// (rule 53).
///
/// **One pair of constants because two doors answer this question.** The
/// handshake says who is answering and `ping` says which build answered, and a
/// server with two answers about its own identity is worse than one wrong
/// answer: whichever a person quotes in an incident is the one they act on.
///
/// **The name is the product's, written down, and the package name is not it.**
/// That the surface lives in a crate of its own is jojobot's own arrangement,
/// and a client has no business learning it from a handshake — the same rule
/// that keeps the library's name off the wire keeps the crate layout off it
/// (rule 53).
pub(crate) const SERVER_NAME: &str = "jojobot";

/// **The build's version, and it is read rather than written.** A literal here
/// would pass every check the name does and go stale the first time nobody
/// remembered it: a version is a fact about the binary, so it comes from the
/// build.
pub(crate) const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const INSTRUCTIONS: &str = "jojobot — a personal-assistant server. Two worlds live here.\
                 \n\n**MEMORY.** What jojobot knows is **entities** — a person, project, place, \
                 event, work, thing, org, topic, bot, pet, rhythm, machine or view, each with a typed \
                 handle, \
                 `kind:slug` \
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
                 \n\n**A key holds one value, so WHO was there is an edge rather than a key.** \
                 A trip records where and when as keys, and each person who came is a record on \
                 THAT PERSON carrying an `attendance` edge at the trip — many people per trip, \
                 where a key would keep only the last one. One edge answers both questions, \
                 because a walk carries its own direction: who came on this trip, and which \
                 trips this person was on.\
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
        // Read before dispatch for the same reason `sid` is: dispatch takes the
        // context. Where the body rides is the agreed revision's to decide, and
        // deciding it here rather than in each verb is what makes it true of
        // every verb, including the next one somebody writes.
        let structured = context
            .protocol_version()
            .is_some_and(|revision| revision >= ProtocolVersion::V_2026_07_28);
        if let Some(refused) = self.unimplemented_arguments(&request) {
            let mut answered: CallToolResponse = refused.into();
            self.finish(&mut answered, sid.as_deref(), structured).await;
            return Ok(answered);
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let mut answered = self.tool_router.call(call).await?;
        self.finish(&mut answered, sid.as_deref(), structured).await;
        Ok(answered)
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info({
            // **Built from the library's own value and then corrected**, since
            // the struct is non-exhaustive: the fields jojobot has an opinion
            // about are the two it answers with, and anything the library adds
            // later arrives with the library's default rather than a stale copy
            // of one.
            let mut me = Implementation::from_build_env();
            me.name = SERVER_NAME.to_string();
            me.version = SERVER_VERSION.to_string();
            me
        })
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
    /// agreed. A revision jojobot agrees to but does not serve in full costs
    /// the client the whole answer, not one field of it: a tool list missing a
    /// field its revision makes mandatory is discarded entire, and the client
    /// reports the server connected while holding no verbs at all. That looks
    /// nothing like an outage from the inside and matches no runbook. So the
    /// cap is what jojobot serves, never what it can name.
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
    /// What the current cap requires of a served answer is pinned by
    /// `the_newest_revision_is_served_whole_and_not_merely_agreed`.
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
