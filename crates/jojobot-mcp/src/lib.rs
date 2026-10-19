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
//! * [`session`] — the four verbs that keep and read a run's own record.
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
mod calls;
mod changed;
mod clock;
pub mod mailboxes;
pub mod memory;
pub mod orientation;
mod request;
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

/// **Handles an earlier build supplied as records and this one does not.** A
/// boot records what its build supplies, so the next build finds a dropped
/// handle there and gives it a row (rule 508). A store that skips the builds in
/// between never recorded it, so the release that stops supplying a handle adds
/// it here. No release has dropped one yet.
pub fn retired_handles() -> Vec<jojobot_domain::memory::EntityId> {
    Vec::new()
}

mod held;
pub mod seed;
mod seen;
pub mod session;
pub mod sid;
mod status_bar;
mod teaching;
mod windows;
pub(crate) use status_bar::OwnBox;

pub(crate) use answer::*;
pub(crate) use caller::*;

#[cfg(test)]
mod harness;
#[cfg(test)]
mod read_refusals;
#[cfg(test)]
mod snapshot;
#[cfg(test)]
mod surface;
#[cfg(test)]
mod switch;

use jojobot_domain::mailbox::{
    self, Delivered, Delivery, Mailbox, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, guard::MailboxMatch,
};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, Landed, Memory, MemoryError, NewEntity, NewFact, Provenance, Standing,
    graph::Direction,
    guard::{self, EntityMatch},
    search::{
        Behind, Coverage, DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, RankClock, Search, SearchQuery,
    },
    types::{DeclaredType, Displaced, Field, Fold, ValueType},
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
pub use session::{AmendJournalArgs, JournalArgs, ListRunsArgs, WrapSessionArgs};

use mailboxes::wire::*;
use memory::declined::*;
use memory::parse::*;
use memory::wire::*;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::*,
    service::{NotificationContext, RequestContext, SubscriptionContext},
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
    /// The Memory port. Injected: the real adapter in production, a fake in
    /// tests.
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
    /// The Teachings port — a fourth context: whether a session's handle has
    /// already been taught how a domain behaves. See
    /// [`jojobot_domain::teaching`].
    teachings: Arc<dyn jojobot_domain::teaching::Teachings>,
    /// **The ledger of request ids that writes have spent.** `None` is an
    /// instance that keeps none, and a call that sends a `request_id` to it is
    /// refused before it writes. See [`jojobot_domain::request`].
    requests: Option<Arc<dyn jojobot_domain::request::Requests>>,
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
    /// **The clock this server runs on.** The real one unless an operator
    /// stated a day for the whole run — see [`jojobot_domain::clock::Clock`].
    ///
    /// A field because the handler is built per connection, so a decision read
    /// once at startup has to reach every handler the factory makes after it.
    clock: jojobot_domain::clock::Clock,
    /// **How long a run must have gone without a write before its name can
    /// take it.** A full lease unless a test shortens it, because a case about
    /// resuming a run it wrote a moment ago would otherwise wait forty-five
    /// minutes. See [`Jojobot::quiet_after`].
    quiet_after: jiff::SignedDuration,
    /// **The skills this handler lists and offers.** The ones the build ships; a
    /// test hands it others so a scope can be watched on the boot and the offer
    /// of an act, which no shipped skill uses.
    skills: &'static [orientation::skills::Skill],
    /// **How long one `subscriptions/listen` is held before jojobot ends it.**
    /// `None` is the served default, [`LISTEN_BUDGET`] with a jitter drawn per
    /// listen; a test gives one fixed, short budget with [`Jojobot::listening_for`].
    listen_budget: Option<std::time::Duration>,
}

/// **How long a held `subscriptions/listen` lasts before jojobot ends it
/// itself.** A tunnel in front of jojobot cuts a stream that stays open about
/// four minutes, and a cut stream fails the client's call and logs an ERROR at
/// the transport. Ending the listen first, with the final result the protocol
/// defines for a graceful close, keeps both from happening. Three minutes is
/// well inside that cut.
const LISTEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(180);

/// **How far the drawn budget may sit either side of [`LISTEN_BUDGET`]**, in
/// percent. Clients that reconnect together after a restart would otherwise
/// all end their listens on the same second.
const LISTEN_JITTER_PERCENT: u32 = 20;

/// **`budget` moved by a share of itself between minus and plus
/// [`LISTEN_JITTER_PERCENT`]**, where `unit` runs from 0 (the low end) to 1 (the
/// high end).
fn jittered(budget: std::time::Duration, unit: f64) -> std::time::Duration {
    let spread = f64::from(LISTEN_JITTER_PERCENT) / 100.0;
    budget.mul_f64(1.0 - spread + 2.0 * spread * unit.clamp(0.0, 1.0))
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
        Self::stating_the_windows(Self::publishing_what_falls_due(
            Self::core_router()
                + mailboxes::router()
                + memory::router()
                + orientation::router()
                + session::router(),
        ))
    }

    /// **Every description on the surface, with the windows it states written
    /// from their constants.** A description carries `{{lease}}`, `{{offer}}` or
    /// `{{code}}` where a number goes (see [`windows`]); this puts the number in,
    /// in the tool's own description and in every argument's, at any depth.
    fn stating_the_windows(mut router: ToolRouter<Self>) -> ToolRouter<Self> {
        fn fill_descriptions(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, inner) in map.iter_mut() {
                        match (key.as_str(), &mut *inner) {
                            ("description", serde_json::Value::String(text)) => {
                                *text = windows::fill(text);
                            }
                            _ => fill_descriptions(inner),
                        }
                    }
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(fill_descriptions),
                _ => {}
            }
        }
        for route in router.map.values_mut() {
            if let Some(text) = route.attr.description.as_mut() {
                *text = windows::fill(text).into();
            }
            let schema = Arc::make_mut(&mut route.attr.input_schema);
            for inner in schema.values_mut() {
                fill_descriptions(inner);
            }
        }
        router
    }

    /// **The two write verbs' `sets` description, ended with the keys that make
    /// a thing fall due, and their `fields` description, ended with the keys that
    /// only label a claim.** Every other line a model reads at the write says a
    /// key it invents is kept, and nothing there said which keys are acted on, or
    /// where. A date sent under `fields` stays on the claim and moves nothing, so
    /// the falls-due paragraph belongs to the bag that acts. The text is built
    /// from the shipped carriers, so it cannot name a key no carrier reads or miss
    /// one that does.
    fn publishing_what_falls_due(mut router: ToolRouter<Self>) -> ToolRouter<Self> {
        let carriers = jojobot_domain::attention::shipped();
        let carriers: Vec<&dyn jojobot_domain::attention::Carrier> =
            carriers.iter().map(std::convert::AsRef::as_ref).collect();
        let labels = seed::describing_keys();
        let on_sets = memory::due::what_makes_a_thing_fall_due(&carriers);
        let on_fields = memory::due::what_describes_its_claim(&labels);
        for verb in ["capture", "update_fact"] {
            let Some(route) = router.map.get_mut(verb) else {
                continue;
            };
            let schema = Arc::make_mut(&mut route.attr.input_schema);
            for (property, text) in [("sets", &on_sets), ("fields", &on_fields)] {
                if text.is_empty() {
                    continue;
                }
                let Some(described) = schema
                    .get_mut("properties")
                    .and_then(|properties| properties.get_mut(property))
                    .and_then(|argument| argument.get_mut("description"))
                else {
                    continue;
                };
                if let Some(existing) = described.as_str() {
                    *described = serde_json::Value::String(format!("{existing}\n\n{text}"));
                }
            }
        }
        router
    }

    pub fn new(
        memory: Arc<dyn Memory>,
        search: Arc<dyn Search>,
        mailboxes: Arc<dyn Mailboxes>,
        sessions: Arc<dyn Sessions>,
        teachings: Arc<dyn jojobot_domain::teaching::Teachings>,
        registry: Arc<sid::SessionRegistry>,
    ) -> Self {
        Self::carrying(
            memory,
            search,
            mailboxes,
            sessions,
            teachings,
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
        teachings: Arc<dyn jojobot_domain::teaching::Teachings>,
        registry: Arc<sid::SessionRegistry>,
        carriers: Vec<Box<dyn jojobot_domain::attention::Carrier>>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
            mailboxes,
            sessions,
            teachings,
            requests: None,
            registry,
            carriers: Arc::new(carriers),
            clock: jojobot_domain::clock::Clock::default(),
            // **A lease in every build that serves, and nothing in this crate's
            // own unit cases**, which resume a run they wrote a moment ago and
            // would otherwise each wait forty-five minutes. The cases that hold
            // the rule itself set the lease back with [`Jojobot::quiet_after`].
            quiet_after: if cfg!(test) {
                jiff::SignedDuration::ZERO
            } else {
                jojobot_domain::session::LEASE_FRESHNESS
            },
            skills: orientation::skills::SKILLS,
            listen_budget: None,
        }
    }

    /// **The same handler, ending every listen after `budget` exactly**, with no
    /// jitter. **For tests**: the binary serves [`LISTEN_BUDGET`] with its jitter.
    #[must_use]
    pub fn listening_for(self, budget: std::time::Duration) -> Self {
        Self {
            listen_budget: Some(budget),
            ..self
        }
    }

    /// The budget for one listen: the fixed one a test set, else
    /// [`LISTEN_BUDGET`] moved by a fresh draw.
    fn budget_for_one_listen(&self) -> std::time::Duration {
        self.listen_budget.unwrap_or_else(|| {
            let mut bytes = [0u8; 4];
            getrandom::fill(&mut bytes).expect("the OS entropy source is readable");
            jittered(
                LISTEN_BUDGET,
                f64::from(u32::from_le_bytes(bytes)) / f64::from(u32::MAX),
            )
        })
    }

    /// **The same handler over other skills**, so a skill scoped to one bot can
    /// be listed and offered by the code a shipped skill goes through. **For
    /// tests**: the binary serves [`orientation::skills::SKILLS`] and nothing
    /// calls this.
    #[must_use]
    pub fn with_skills(self, skills: &'static [orientation::skills::Skill]) -> Self {
        Self { skills, ..self }
    }

    /// **The same handler, keeping request ids in `requests`.** The store that
    /// stores the writes spends into the same ledger, in the write's own step.
    #[must_use]
    pub fn with_requests(self, requests: Arc<dyn jojobot_domain::request::Requests>) -> Self {
        Self {
            requests: Some(requests),
            ..self
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

    /// **The same handler, with a different quiet period** for resuming a run
    /// by its name. The default is the lease and nothing but a test changes it.
    #[must_use]
    pub fn quiet_after(self, quiet_after: jiff::SignedDuration) -> Self {
        Self {
            quiet_after,
            ..self
        }
    }

    /// [`Jojobot::quiet_after`] when one is given, and the handler as it is when
    /// none is, so a composition root passes what its configuration holds.
    #[must_use]
    pub fn quiet_after_unless_default(self, quiet_after: Option<jiff::SignedDuration>) -> Self {
        match quiet_after {
            Some(quiet_after) => self.quiet_after(quiet_after),
            None => self,
        }
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

/// **What the server tells a client at connection**: the written instructions,
/// then the keys that make a thing fall due, built from the shipped carriers.
pub(crate) fn instructions() -> String {
    let carriers = jojobot_domain::attention::shipped();
    let carriers: Vec<&dyn jojobot_domain::attention::Carrier> =
        carriers.iter().map(std::convert::AsRef::as_ref).collect();
    let falls_due = memory::due::what_makes_a_thing_fall_due(&carriers);
    if falls_due.is_empty() {
        return INSTRUCTIONS.to_string();
    }
    format!("{INSTRUCTIONS}\n\n**WHAT FALLS DUE.** {falls_due}")
}

pub(crate) const INSTRUCTIONS: &str = "jojobot — a personal-assistant server with two worlds.\
                 \n\n**MEMORY.** **Entities** have typed handles, `kind:slug`, the name \
                 each is addressed by. **Facts** are dated claims about them, each \
                 with an **address** (`kind:slug#local-id`) and a **provenance** \
                 saying who backs it; a claim nobody confirmed is a hypothesis \
                 until the operator confirms it. **jojobot is where durable things are \
                 kept.** A thing's **fields** are its `sets` writes, folded, the \
                 newest write of each key winning (a handle, or a comma list of \
                 handles, is a link and must exist; a counter key sums). **Start \
                 with `search`**, which reaches mail with `include_mail: true`.\
                 \n\n**MAILBOXES.** A message left for someone not here moves new, \
                 read, processed. `read_mailbox` takes the whole box, all of it \
                 yours to finish; `read_message` takes one. `search` finds mail in \
                 every state except the operator's box, which no bot reads.\
                 \n\n**Three rules of engagement.** 1. Everything a write NAMES must \
                 already exist; something new is two steps, create then write. 2. A \
                 name resembling something known comes back as a SUCCESSFUL result \
                 with `status: blocked`, `wrote: false`, `candidates` and \
                 `how_to_proceed`: use the candidate, or re-call with the \
                 `override_token` that refusal carries. Branch on `status`; a plain \
                 error is a malformed call. 3. Mark a message processed only AFTER \
                 acting on it.\
                 \n\nA write returns a receipt, and eliding is never silent: the answer \
                 says what it left out and which call returns it. `start_here` has \
                 the vocabulary.";

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
        // Read before dispatch consumes the request. **The verb is counted under
        // its own name or under none**: a name the router does not serve is
        // text a caller chose, and a counter keyed by it would grow with it.
        let verb = self.counted_verb(&request.name);
        let sid = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("sid"))
            .and_then(|sid| sid.as_str())
            .map(str::to_string);
        // **Who is calling is resolved before the verb runs**, because a verb may
        // void the handle it was called with.
        let caller = self.caller(sid.as_deref()).ok().flatten();
        // **The boot is the one verb that names a bot**, and a boot that was
        // offered a choice names no run to count it under.
        let named_bot = (verb == "start_here")
            .then(|| {
                request
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("bot"))
                    .and_then(|bot| bot.as_str())
            })
            .flatten()
            .and_then(|bot| crate::memory::parse::named_bot(Some(bot)).ok().flatten());
        let (result, counted) = self.serve_call(request, context).await;
        if counted {
            self.count_call(verb, caller, named_bot, &result).await;
        }
        result
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
        .with_instructions(instructions())
    }

    /// **`subscriptions/listen` is served for the one category jojobot
    /// advertises: a changed tool list.** The newest revision jojobot agrees to
    /// defines this method, and a client on it opens one per category it wants
    /// to hear about. Answering method-not-found to it was a wrong answer, and
    /// the SDK logs every such answer at WARN, on every connection.
    ///
    /// The SDK acknowledges the filter and then holds the request open until
    /// the client cancels, the connection ends or `listen` ends it
    /// at its budget. Nothing is sent on it: the
    /// tool list moves only when a new build is deployed, and that ends the
    /// connection. A client on an older revision still gets method-not-found,
    /// because there the method does not exist.
    fn accepted_subscription_filter(
        &self,
        _requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let mut accepted = SubscriptionFilter::default();
        accepted.tools_list_changed = Some(true);
        Some(accepted)
    }

    /// **Hold the listen until the client cancels it or its budget ends, and
    /// then end it ourselves.** Returning `Ok(())` sends the final result for a
    /// listen, which the client reads as a graceful close, so nothing between the
    /// client and jojobot has to cut the stream and no call fails on a cut. A
    /// client that reconnects gets a new listen. The budget is
    /// [`Jojobot::listening_for`] in a test and [`LISTEN_BUDGET`] with jitter
    /// when served.
    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let budget = self.budget_for_one_listen();
        tokio::select! {
            () = context.cancelled() => {}
            () = tokio::time::sleep(budget) => {}
        }
        Ok(())
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

impl Jojobot {
    /// **Everything a call does between its arrival and its answer**, and
    /// whether the answer is one to count: a spent request id answers from the
    /// ledger, and the exchange of an old run name for a key is no verb running.
    /// See [`ServerHandler::call_tool`], which counts what this says to.
    async fn serve_call(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> (Result<CallToolResponse, McpError>, bool) {
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
        // **The one call that exchanges an old name for a key**, ahead of every
        // verb: it is the same for all of them, and the answer is the key.
        if let Some(raw) = sid.as_deref()
            && let Some(exchanged) = self.exchange_legacy_name(raw).await
        {
            let mut answered: CallToolResponse = exchanged.into();
            self.finish(&mut answered, None, structured).await;
            return (Ok(answered), false);
        }
        // **A live secret written into text is refused before any verb reads
        // it**, so no verb has to remember to, including the next one somebody
        // writes. The arguments are read once, here.
        if let Some(refused) = self.refuse_a_live_secret(request.arguments.as_ref()).await {
            let mut answered: CallToolResponse = refused.into();
            self.finish(&mut answered, sid.as_deref(), structured).await;
            return (Ok(answered), true);
        }
        if let Some(refused) = self
            .unimplemented_arguments(&request)
            .or_else(|| self.missing_arguments(&request))
        {
            let mut answered: CallToolResponse = refused.into();
            self.finish(&mut answered, sid.as_deref(), structured).await;
            return (Ok(answered), true);
        }
        // **A call that carries a `request_id` is looked up before the verb runs**,
        // ahead of its beat, its teaching and its accounting. A spent id answers
        // from the ledger and runs none of them.
        let scope = match self.begin_request(&request, sid.as_deref()).await {
            request::Begun::Absent => None,
            request::Begun::Refused(refused) => {
                let mut answered: CallToolResponse = refused.into();
                self.finish(&mut answered, sid.as_deref(), structured).await;
                return (Ok(answered), true);
            }
            request::Begun::Repeat(receipt) => {
                let mut answered: CallToolResponse = receipt.into();
                self.finish_replay(&mut answered, structured);
                return (Ok(answered), false);
            }
            request::Begun::Fresh(scope) => Some(scope),
        };
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        // A protocol error from the verb leaves through the `?` below and never
        // reaches `end_request`; see its note for why that is accepted.
        let ran = match &scope {
            Some(scope) => {
                jojobot_domain::request::within(scope.clone(), self.tool_router.call(call)).await
            }
            None => self.tool_router.call(call).await,
        };
        let mut answered = match ran {
            Ok(answered) => answered,
            Err(error) => return (Err(error), true),
        };
        self.finish_unshaped(&mut answered, sid.as_deref()).await;
        // **What a spent id stores is the answer as the caller was told it**, so a
        // repeat gives back the same words. Taken before the answer is shaped for
        // the wire, which a repeat does again on its way out.
        if let Some(scope) = &scope
            && let Some(receipt) = self.end_request(scope, &answered).await
        {
            // Another call spent the id while this one ran, and its receipt stands.
            let mut answered: CallToolResponse = receipt.into();
            self.finish_replay(&mut answered, structured);
            return (Ok(answered), false);
        }
        self.shape(&mut answered, structured);
        (Ok(answered), true)
    }
}

#[cfg(test)]
mod listen_budget_tests {
    use super::*;
    use std::time::Duration;

    /// **A served listen ends within twenty percent either side of three
    /// minutes, and two draws are not all the same.** The ends of the draw's
    /// range are the bounds, and a mid-range draw is the budget itself.
    #[test]
    fn the_jitter_keeps_a_listen_within_twenty_percent_of_its_budget() {
        assert_eq!(jittered(LISTEN_BUDGET, 0.0), Duration::from_secs(144));
        assert_eq!(jittered(LISTEN_BUDGET, 0.5), LISTEN_BUDGET);
        assert_eq!(jittered(LISTEN_BUDGET, 1.0), Duration::from_secs(216));
        assert_eq!(jittered(LISTEN_BUDGET, 7.0), Duration::from_secs(216));
        assert_eq!(jittered(LISTEN_BUDGET, -3.0), Duration::from_secs(144));
    }
}
