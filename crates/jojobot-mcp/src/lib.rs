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
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions,
};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, JOURNAL_TITLE, Memory, MemoryError, NewEntity, NewFact, Provenance,
    guard::{self, EntityMatch},
    search::{DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, MailCoverage, Search, SearchQuery},
    validate_edge,
};
use jojobot_domain::text::{self, FRESH_FOCUS};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};

/// What `start_here` hands a fresh agent. Engine prose: the method, in role
/// language only — no operator specifics, fictional example identities.
const ORIENTATION: &str = r#"# jojobot — start here

jojobot is a personal-assistant server: the durable memory and message rail behind an assistant serving one person, the operator. You are one of possibly many AI sessions connected to it — jojobot itself never thinks; it stores, guards, and serves. What you write here outlives this conversation and will be read back as truth by sessions that cannot ask you what you meant. The rules below exist for them.

## The two worlds

**MEMORY** is a typed graph of the operator's life. An **entity** is a noun — person · project · place · event · work · thing · org · topic — with a permanent handle like `person:milhouse`. A **fact** is one dated claim about an entity, addressed `person:milhouse#3`, carrying a **provenance**: `testimony` (the operator said or confirmed it) or `inference` (an AI derived it). Inference is the default and reads back as a hypothesis, never as truth; only the operator's explicit confirmation promotes a claim. A fact may draw one typed **edge** at another entity — `location` · `membership` · `attendance` · `about` — and edges are what make cross-entity questions answerable. **`search` is the front door** to all of it — and to the messages in mailboxes too: one ranked list, one call.

**MAILBOXES** are the async rail between sessions: named boxes where one session leaves a message another will find. A message is `new` → `read` → `processed`. Reading IS taking delivery (no peek); anything read but not yet processed comes back on the next read, flagged — so crashed work resurfaces on its own. `processed` means acted-on, and it is a terminal archive: nothing here is ever deleted. **A box is infrastructure, not data**: a permanent label in the operator's own task system, worth having only because some specific party is committed to draining it. A message is addressed to a box, never to you — there is no recipient field, and no box is "yours" unless you were told it is. **Messages are searchable**: `search` finds them beside the memory hits, in every state, `processed` archives included — so a finding somebody filed for another session is reachable by anyone who asks the right question, without knowing where to look. A hit says which box and which state; `read_message` takes that one message without making the rest of the box yours.

## Working here, by example

- *"Remember that Milhouse is allergic to shellfish"* → `search` for milhouse to find the handle → `capture` subject `person:milhouse`, content the claim, provenance `testimony` (the operator's own words back it) or `inference` (you concluded it). The gate is on promotion, not assertion — a first capture declares its own provenance on honour, so declare `testimony` only for the operator's words, and capture what a later session would need: a passing mention is not a fact.
- *A person, place, org or event the operator named that jojobot doesn't know* → `add_entity`, then the write: two deliberate steps, nothing created as a side effect. This is the normal, welcome move — the graph is meant to grow with the operator's life.
- *No mailbox fits what you want to leave* → almost never `create_mailbox`. A new box is a message posted where nobody is listening, plus a permanent label. Use an existing, agreed box, or say plainly there is nowhere fitting and let the operator decide — mint one only when the operator or a standing arrangement asked for that box by name.
- *"Which people are in Shelbyville?"* → `search` with kind `person` and edge `{shape: location, object: place:shelbyville}` — an edge walk, not a text match.
- *"That was wrong"* → `recall` the subject, then `update_fact` rewrites the claim in place to state what is true NOW — including negative truth ("NOT allergic — confirmed by the operator"). The record is current truth, never a correction trail. *"That changed"* is a different move: the old claim was true in its day — mark it `superseded` and `capture` the new one.
- *Leave word for another session* → `list_mailboxes` to see what exists and what is waiting, `post_message` into an agreed box with a body written for a reader with none of your context, and your `sender` naming a role that still exists next week, not this session's id.
- *Handle mail* → `read_mailbox` on the box you were told to drain — reading takes delivery of every message in it, and they are not yours just because you can read them — act, then `mark_processed`, ONLY after acting, with the outcome in notes. A failure is data to record, not a state to park in.
- *One message, not a whole box* → `search` for it, then `read_message` on the id the hit carries. Draining a box you were not told to drain makes every message in it owed work you never agreed to.

When the right write is not obvious, ask the operator — an unasked write outlives the conversation that guessed it.

## The answers that are not errors

A **blocked** result is a SUCCESS whose body says `status: "blocked"`, `wrote: false`: nothing was written, and `how_to_proceed` says what to do next. Never retry one unchanged. Four gates produce it, with different ways out: **resemblance** (creating or renaming something that looks like what exists — pick the candidate you meant, or `create_new: true` only when you can say how the two differ; an exact handle or box name is never overridable), **absence** (you named something that is not there — the subject of a capture, an edge's object, the box of a post, a handle to read, an address to edit, a message id to retire; empty `candidates` means nothing even resembles it, not that your call was malformed; for an entity, creating it and retrying is usually right — for a mailbox it usually is not), **ownership** (a mailbox has exactly one owner, and a second claim on one is refused naming the holder; `create_new` does not clear this — it answers a question about names), and **unreadable** (`mark_processed` reached an item jojobot cannot read — no retry helps, a person must repair it; treat what it carried as unhandled and say so).

A plain **error** is a malformed call — a token that is no kind, a string that is no address — or the store itself failing. **Absence is never an error here**: naming something that does not exist is an answer with candidates, not a broken server, so read `status` rather than branching on whether the call errored. And know what the guards do NOT cover: they catch resemblance, absence and ownership, never judgement — a wholly novel name sails through, and nothing will stop you standing up a box nobody drains. That call is yours, and the store keeps whatever you decide.

## Bots

An **identity** is an entity of kind `bot`: a handle like `bot:gamma`, a **charter** (its prose — what this identity is, its hard lines, where its work lives), **rules** as ordinary facts about it (so each one carries its own provenance: an inferred rule is a hypothesis, not a policy), and optionally **one owned mailbox**. If you were told which identity you are, `boot_bot` is your first call instead of this one: it hands over everything here plus that identity. Nothing about a bot is built into jojobot — a bot is data somebody wrote, like every other entity.

## Sessions

A bot is a **role**; a **session is one mortal run of it** — the unit of work, not the unit of connection. It outlives a disconnect and a device hop, because what makes two connections the same session is the identity that booted them.

**Booting an identity starts or resumes its session; there is no separate verb.** `boot_bot` sweeps that bot's stale sessions to `abandoned` (a day without a beat), resumes the live one if there is one — read its chronology before you start, somebody was part way through something — and otherwise begins a fresh one **lazily**: no card exists until your first write, so a boot that does nothing leaves nothing behind.

A session has two halves that answer different questions. Its **focus** is what it is working on NOW, one line, rewritten in place. Its **chronology** is what happened: append-only, oldest first, with only the newest entry amendable.

- *Record a beat* → `journal` — **a literal journal, not a log.** What you set out to do, what you found, what you decided, what went wrong. NOT every tool call and not every file: a reader months from now wants the story, and a firehose buries it. Pass `focus` when what you are working on changes.
- *Fix the beat you just wrote* → `amend_journal`. Only the most recent one; everything older is what it was.
- *End* → `wrap_session` with the story, written for somebody with none of your context. It becomes your final entry AND one dated entry in the operator's Journal, and the session goes `wrapped` — terminal both ways.

jojobot also writes **its own beats** into your chronology: one per verb class you use, its count kept current as you go. They are marked apart (`beat` names the class) because what you said you were doing and what jojobot noticed you doing are different kinds of evidence.
"#;

/// Arguments to `add_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddEntityArgs {
    /// One of `person`, `project`, `place`, `event`, `work`, `thing`, `org`,
    /// `topic`.
    pub kind: String,
    /// The slug half of the handle (`[a-z0-9-]+`), or a full `kind:slug` id
    /// whose kind must match `kind`. The handle is permanent — choose one that
    /// will still be right in a year.
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
    /// Optional cross-link to this entity's card in the user's task system,
    /// written `card:N`.
    #[serde(default)]
    pub crm: Option<String>,
    /// The mailbox this entity owns — the box whose mail is its mail. **One box
    /// has one owner**: claiming a box another entity already owns comes back
    /// blocked naming that owner, and `create_new` does not override it. The box
    /// need not exist yet.
    #[serde(default)]
    pub mailbox: Option<String>,
    /// `always` marks this entity as part of the core an assistant loads at
    /// the start of every session; the default `on-demand` is fetched when the
    /// conversation reaches for it. Only the exact token `always` counts.
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
    /// The crisp claim to remember — single line, no line breaks.
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
    /// Whether messages left in mailboxes are searched too. **Defaults to
    /// true** — a report filed for another session is exactly the context you
    /// would not know to go looking for. Pass `false` to keep session traffic
    /// out of a question about the operator's life.
    #[serde(default)]
    pub include_mail: Option<bool>,
    /// How many results; defaults to 20. There is no pagination — a second page
    /// is a better query.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Arguments to `update_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityArgs {
    /// The entity's handle. Not editable — renaming a handle is a separate
    /// operation.
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
    /// New cross-link to the entity's card in the user's task system, `card:N`.
    #[serde(default)]
    pub crm: Option<String>,
    /// The mailbox this entity owns. **One box has one owner**: claiming one
    /// another entity already owns comes back blocked naming that owner, and
    /// `create_new` does not override it.
    #[serde(default)]
    pub mailbox: Option<String>,
    /// Set only after a previous call reported candidates for a name or alias
    /// you are claiming here, and you judged them a different entity. Any change
    /// to what this entity is CALLED is screened exactly as a creation is.
    #[serde(default)]
    pub create_new: Option<bool>,
}

/// Arguments to `boot_bot`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BootBotArgs {
    /// The bot to start as: its bare slug, or its full `bot:`-prefixed handle.
    /// A handle of any other kind is refused — this door boots bots.
    pub name: String,
}

/// Arguments to `set_charter`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCharterArgs {
    /// The bot whose charter this is: its bare slug, or its full handle.
    pub bot: String,
    /// The charter itself. Prose: paragraphs are fine. It **replaces** whatever
    /// charter the bot had, so send the whole thing, not an addition.
    pub prose: String,
}

// --- mailboxes ---------------------------------------------------------------

/// Arguments to `create_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateMailboxArgs {
    /// The box's name: `[a-z0-9-]+`, starting and ending alphanumeric. One
    /// spelling per box, so two callers cannot create `Inbox` and `inbox`.
    pub name: String,
    /// Set only after a previous call reported candidates for this name and
    /// you judged the resemblance deliberate — sibling boxes like `worker-2`
    /// beside `worker-1`. Overrides the similarity screen. An exact name is
    /// never overridden: that box already exists.
    #[serde(default)]
    pub create_new: Option<bool>,
}

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// The box to leave it in. **It must already exist** — an unknown name comes
    /// back with candidates and nothing is written.
    pub mailbox: String,
    /// The message itself. Prose: paragraphs are fine.
    pub body: String,
    /// What this message is about, in one line — a title, not a summary.
    /// Optional, and worth giving: it is what a reader sees on the card and on
    /// a search hit before they open anything. Do NOT also repeat it as the
    /// body's first line.
    #[serde(default)]
    pub subject: Option<String>,
    /// Who is sending, as you declare it. Recorded as claimed — jojobot does not
    /// resolve or verify identity — name yourself specifically enough that a
    /// reply can find you.
    pub sender: String,
    /// The id of the message this one answers, when it answers one. Optional.
    /// It must name a message that exists — a miss comes back blocked and
    /// nothing is written — and it links the two without saying anything about
    /// either: it does not deliver, handle, or oblige.
    #[serde(default)]
    pub in_reply_to: Option<String>,
}

/// Arguments to `read_mailbox`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMailboxArgs {
    /// The box to read.
    pub mailbox: String,
}

/// Arguments to `read_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMessageArgs {
    /// The message's id, exactly as a search hit, a delivery or `post_message`
    /// returned it.
    pub message_id: String,
}

// --- sessions ----------------------------------------------------------------

/// Arguments to `journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct JournalArgs {
    /// One high-level beat: what you set out to do, what you found, what you
    /// decided, what went wrong. Prose — paragraphs are fine.
    pub entry: String,
    /// What you are working on NOW, in one line. Optional, and it **replaces**
    /// the session's current focus rather than adding to it.
    #[serde(default)]
    pub focus: Option<String>,
    /// The session to write to. Omit it: the connection is bound to the session
    /// `boot_bot` started or resumed. Only name one to write to a different
    /// session than the one you are in.
    #[serde(default)]
    pub session: Option<String>,
}

/// Arguments to `amend_journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AmendJournalArgs {
    /// What the most recent entry should say instead. It replaces that entry
    /// whole.
    pub entry: String,
    /// The session whose newest entry to rewrite; omit for the one you are in.
    #[serde(default)]
    pub session: Option<String>,
}

/// Arguments to `wrap_session`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WrapSessionArgs {
    /// The story of this session, for somebody with none of your context: what
    /// it was for, what happened, what is left. It becomes the final chronology
    /// entry AND one dated entry in the operator's Journal.
    pub story: String,
    /// The session to wrap; omit for the one you are in.
    #[serde(default)]
    pub session: Option<String>,
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
    /// The Sessions port — a third context, on its own board.
    sessions: Arc<dyn Sessions>,
    /// **Which identity this connection is running as, and which session.**
    ///
    /// In-process and per connection: the transport builds one handler per MCP
    /// session, so this map is born empty on every connect and evaporates on
    /// restart. That is fine, and it is why the truth lives on the card: the
    /// next `boot_bot` re-attaches to the same session by reading the board.
    /// Shared across clones through the `Arc`, because the router clones the
    /// handler per call and a binding held by value would vanish between verbs.
    bound: Arc<std::sync::RwLock<Option<Bound>>>,
    /// **Serializes the read-await-write span over `bound`.** rmcp runs one task
    /// per request, so two tool calls in flight on one connection would
    /// otherwise both read "no session yet" and both materialize a card.
    session_gate: Arc<tokio::sync::Mutex<()>>,
}

/// What a booted connection knows about itself.
#[derive(Debug, Clone)]
struct Bound {
    /// The identity this connection booted as.
    bot: EntityId,
    /// The session it is working in — `None` until the card materializes on the
    /// first write. **A boot that never works leaves no card**, so a session
    /// with nothing to say is a session that never existed.
    session: Option<SessionId>,
    /// **Whether jojobot actually read the board for this bot.** False when the
    /// session world was down at boot: the connection knows its identity but not
    /// whether a session is in flight, so the first write retries the attach
    /// rather than beginning one that would fork a running session.
    attached: bool,
    /// One entry per verb class jojobot has already written a beat for, so the
    /// second call of a class corrects the first beat instead of adding one.
    beats: std::collections::HashMap<&'static str, Beat>,
}

/// A running tally of one verb class, as one chronology entry.
#[derive(Debug, Clone)]
struct Beat {
    /// The entry the tally lives in.
    entry: EntryId,
    /// How many calls of this class this session has made.
    count: usize,
    /// The first few things it named, so the beat says what it touched and not
    /// only how often. Capped — a beat is a beat, not a log.
    examples: Vec<String>,
}

/// How many examples a beat carries before it stops naming them.
const BEAT_EXAMPLES: usize = 5;

#[tool_router]
impl Jojobot {
    pub fn new(
        memory: Arc<dyn Memory>,
        search: Arc<dyn Search>,
        mailboxes: Arc<dyn Mailboxes>,
        sessions: Arc<dyn Sessions>,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            memory,
            search,
            mailboxes,
            sessions,
            bound: Arc::new(std::sync::RwLock::new(None)),
            session_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Liveness probe: returns jojobot's identity and its current wall-clock
    /// time. Proves an MCP client can reach the server and get a real response.
    #[tool(
        description = "Check that jojobot is reachable: returns its identity, version and \
                       current time. No side effects."
    )]
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

    /// The orientation call: the world-model in prose, then a live snapshot of
    /// what exists. The anonymous ancestor of a later per-identity boot.
    ///
    /// The prose below is ENGINE material: it explains the method, names only
    /// roles ("the operator"), and every example identity is fictional.
    #[tool(
        description = "New here? Call this first. Explains what jojobot is and how its world \
                       fits together — entities, facts, provenance, edges, mailboxes — with \
                       worked examples, and returns a live snapshot of what exists right now \
                       (entities by kind, every mailbox with its counts), so you start \
                       oriented instead of guessing. Read-only, no side effects."
    )]
    async fn start_here(&self) -> Result<CallToolResult, McpError> {
        self.orient(None).await
    }

    /// Orientation with an identity attached: the same world-model and the same
    /// live snapshot `start_here` hands an anonymous session, plus which bot
    /// this session is — its charter, its rules, and the state of its own box.
    #[tool(
        description = "Start a session AS a named bot. Hands over what start_here does — how \
                       jojobot's world fits together, and a snapshot of what exists — plus the \
                       identity itself: the bot's charter (the orienting text: what this identity \
                       is, its hard lines, where its work lives), its rules as dated claims each \
                       carrying its own provenance (testimony is settled, inference is a \
                       hypothesis — read them that way), and the per-state counts of the mailbox \
                       it owns. Call it first when you have been told which identity you are; \
                       call start_here when you have not. A name that is no bot comes back \
                       status: blocked with candidates and boots nothing. A mailbox the bot \
                       claims but nobody has opened is reported as missing, never created. \
                       BOOTING ALSO STARTS OR RESUMES THIS BOT'S SESSION — there is no separate \
                       start verb. It first sweeps that bot's sessions that have gone a day \
                       without a beat to `abandoned`; then, if a live session remains, this \
                       ATTACHES to it and returns it with its chronology (`session.resumed: \
                       true`) — read that before you start, it is work already in flight, yours \
                       from before a disconnect. Otherwise a fresh session begins lazily: no \
                       card is written until your first journal entry or first write, so a boot \
                       that does nothing leaves no trace. The connection is bound to that \
                       identity and session afterwards, so journal/amend_journal/wrap_session \
                       need no id."
    )]
    async fn boot_bot(
        &self,
        Parameters(args): Parameters<BootBotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bot = bot_id(&args.name)?;
        self.orient(Some(&bot)).await
    }

    /// Write a bot's charter — the prose layer of its own page.
    #[tool(
        description = "Write a bot's charter: the orienting text boot_bot hands a session that \
                       starts as this bot — what this identity is, its hard lines, where its work \
                       lives. Replaces the whole charter rather than adding to it, and returns \
                       the stored text, which is what a later boot_bot will read back. A bot that \
                       does not exist comes back status: blocked with the nearest handles — \
                       add_entity first; nothing is created here. Rules are not written here \
                       either: a rule is a fact about the bot, so capture it."
    )]
    async fn set_charter(
        &self,
        Parameters(args): Parameters<SetCharterArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bot = bot_id(&args.bot)?;
        let stored = match self.memory.set_prose(&bot, &args.prose).await {
            Ok(stored) => stored,
            Err(e) => return memory_declined("set_charter", e),
        };
        self.beat("set_charter", bot.as_str()).await;
        json_result(&serde_json::json!({
            "bot": bot.as_str(),
            "charter": stored,
        }))
    }

    /// The one orientation, anonymous or identified. `start_here` and
    /// `boot_bot` are this function with and without a bot — deliberately, so
    /// the two doors can never come to teach two different jojobots.
    async fn orient(&self, bot: Option<&EntityId>) -> Result<CallToolResult, McpError> {
        // Best-effort per world: orientation must land even when one world is
        // down — a fresh agent on a half-configured server still gets the map.
        let entities = match self.memory.list_entities(None).await {
            Ok(entities) => {
                let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
                for e in &entities {
                    let kind = e.id.as_str().split(':').next().unwrap_or("unknown");
                    *by_kind.entry(kind).or_default() += 1;
                }
                serde_json::json!({
                    "available": true,
                    "count": entities.len(),
                    "by_kind": by_kind,
                })
            }
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the memory world is not reachable right now — its tools will say why",
            }),
        };
        let mailboxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => serde_json::json!({
                "available": true,
                "boxes": boxes.iter().map(mailbox_json).collect::<Vec<_>>(),
            }),
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the mailbox world is not reachable right now — its tools will say why",
            }),
        };
        let snapshot = serde_json::json!({ "entities": entities, "mailboxes": mailboxes });
        let identity = match bot {
            None => serde_json::Value::Null,
            Some(bot) => match self.identity(bot).await? {
                Ok(identity) => identity,
                // A name that is no bot: the guards' own shape, so one
                // client-side branch handles every "jojobot declined" answer.
                Err(candidates) => {
                    return Ok(blocked_result(
                        bot,
                        &candidates,
                        Blocked::MustExist("boot_bot"),
                        None,
                    ));
                }
            },
        };
        // **Only after the identity resolved.** A name that is no bot boots
        // nothing, so it starts no session and sweeps nothing either — binding
        // a connection to an identity jojobot just refused would be a session
        // belonging to nobody.
        let session = match bot {
            None => serde_json::Value::Null,
            Some(bot) => self.attach(bot).await,
        };
        json_result(&serde_json::json!({
            "orientation": ORIENTATION,
            "snapshot": snapshot,
            "identity": identity,
            "session": session,
        }))
    }

    /// Who this session is: the bot's record, the charter its prose carries,
    /// the rules its facts carry, and the live state of the box it owns.
    /// `Err(candidates)` is the guards' answer for a name that is no bot.
    async fn identity(
        &self,
        bot: &EntityId,
    ) -> Result<Result<serde_json::Value, Vec<EntityMatch>>, McpError> {
        let index = self.memory.list_entities(None).await.map_err(memory_error)?;
        let Some(entity) = index.iter().find(|e| &e.id == bot) else {
            return Ok(Err(guard::screen(bot, &[], &index)));
        };

        // The charter is the doc's prose; a bot nobody has written one for has
        // none, and null says so rather than an empty string pretending to be
        // an answer.
        let charter = self
            .memory
            .scan_entity(bot)
            .await
            .map_err(memory_error)?
            .map(|doc| doc.prose)
            .filter(|p| !p.trim().is_empty());
        let rules = self.memory.recall(bot).await.map_err(memory_error)?;

        Ok(Ok(serde_json::json!({
            "bot": entity_json(entity),
            "charter": charter,
            "rules": rules.iter().map(fact_json).collect::<Vec<_>>(),
            "owned_mailbox": match entity.mailbox.as_deref() {
                None => serde_json::Value::Null,
                Some(name) => self.owned_mailbox(name).await?,
            },
        })))
    }

    /// The live state of the box a bot owns — **reported, never opened.**
    ///
    /// Booting used to mint a declared box that was missing. It doesn't now:
    /// creation is an intentional act, and `create_mailbox` is both the only
    /// mint and the only place the full name screen runs. A door that opened a
    /// box on the side was a door that opened near-duplicates nobody was ever
    /// shown — and there is no verb that deletes one.
    ///
    /// So a missing box is *said*, plainly, with the deliberate verb named. A
    /// bot whose box nobody has opened still boots: it is an identity that
    /// cannot receive mail yet, and the honest thing is to tell it so.
    async fn owned_mailbox(&self, name: &str) -> Result<serde_json::Value, McpError> {
        let name = MailboxName(name.trim().to_string());
        // The mailbox half degrades on its own, exactly as the snapshot's does.
        // Hard-erroring here made every box-owning identity unbootable over an
        // outage in the *other* world — while its charter and its rules, the
        // things a session most needs, were sitting right there in Memory.
        let boxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes,
            Err(_) => {
                return Ok(serde_json::json!({
                    "name": name.as_str(),
                    "available": false,
                    // Not false: jojobot does not know whether it exists, and
                    // saying it does not would be a guess a session would act on.
                    "exists": serde_json::Value::Null,
                    "note": "the mailbox world is not reachable right now, so jojobot cannot say \
                             whether this box exists or what is waiting in it — its tools will \
                             say why",
                }));
            }
        };

        let Some(mailbox) = boxes.into_iter().find(|b| b.name == name) else {
            return Ok(serde_json::json!({
                "name": name.as_str(),
                "available": true,
                "exists": false,
                "counts": serde_json::Value::Null,
                "how_to_proceed": format!(
                    "This bot owns '{name}', but no such mailbox exists yet, so nothing can be \
                     left for it and nothing is waiting. Booting does not open one — creating a \
                     box is a deliberate act, because a near-duplicate box is a channel nobody \
                     drains and there is no verb that removes one. Call create_mailbox '{name}' \
                     if that is the box that was meant; if it looks like a typo of a box that \
                     already exists, the claim on this bot is what needs correcting instead."
                ),
            }));
        };
        // The three answers wear one shape — `available` then `exists`, always
        // both present — so a session reads them in one pass instead of
        // branching on which keys came back.
        let mut body = mailbox_json(&mailbox);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("available".into(), true.into());
            obj.insert("exists".into(), true.into());
        }
        Ok(body)
    }

    // ── sessions ────────────────────────────────────────────────────────────

    /// Start or resume this bot's session, and bind the connection to it.
    ///
    /// **Booting an identity IS starting its session** — there is no separate
    /// verb, because there is no moment between "I am gamma" and "gamma is
    /// working" for one to sit in. Three things happen, in this order:
    ///
    /// 1. **the sweep.** Any `active` session of THIS bot whose last beat is
    ///    older than [`ABANDONED_AFTER`] is closed as `abandoned` — lazily, at
    ///    boot, because there is no background runtime until M8 and a session
    ///    left open forever would make "resume where you left off" resume
    ///    something from last month.
    /// 2. **attach**, if a live session survives the sweep. That is what makes a
    ///    reconnect or a device hop resume work in flight rather than fork it.
    /// 3. **bind without a card otherwise.** A fresh session begins lazily: no
    ///    card until the first write. A boot that never works leaves no trace,
    ///    which is what keeps "creation is an intentional act" true for a verb
    ///    whose whole job is to start something.
    ///
    /// Returns the session block for the boot payload, or the reason there
    /// isn't one. A session store that is down degrades exactly as the mailbox
    /// world does: the boot still lands, and the block says jojobot does not
    /// know rather than guessing.
    async fn attach(&self, bot: &EntityId) -> serde_json::Value {
        // **A boot is a bind-read → act → bind-write span like the others, so it
        // takes the same gate.** Without it a boot racing a first write on this
        // connection read the board before the card existed and wrote its
        // binding after — clearing the session the write had just materialized
        // and rolling the tally back to what the stale read saw. The next write
        // then minted a second card for a session already running. The gap is
        // real rather than theoretical: sweeping a stale card is an await
        // sitting inside that very span.
        //
        // Taken here rather than in `sweep_and_find`, which the first-write
        // retry calls with the gate already held — the mutex is not reentrant.
        let _serialized = self.session_gate.lock().await;
        let (live, swept) = match self.sweep_and_find(bot).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(error = %e, bot = %bot, "the session world is not reachable");
                // Bound, but NOT attached: jojobot has not read the board, so it
                // does not know whether this bot has a session in flight. The
                // first write retries the attach rather than beginning one, or a
                // boot during an outage would fork a session that is running.
                *self.bound.write().expect("binding poisoned") = Some(Bound {
                    bot: bot.clone(),
                    session: None,
                    attached: false,
                    beats: Default::default(),
                });
                return serde_json::json!({
                    "available": false,
                    "note": "the session world is not reachable right now, so jojobot cannot say \
                             whether you have a session in flight, and has not started one — a \
                             fresh session here could fork one that is already running. It will \
                             try again on your first write. Everything else here is unaffected; \
                             the session verbs will say why.",
                });
            }
        };

        let mut block = match &live {
            Some(session) => serde_json::json!({
                "available": true,
                "resumed": true,
                "session": session_json(session),
                "note": "you are resuming a session already in flight — its chronology is above. \
                         Read it before you start: somebody (you, before a disconnect) was part \
                         way through something.",
            }),
            None => serde_json::json!({
                "available": true,
                "resumed": false,
                "session": serde_json::Value::Null,
                "note": "a fresh session. Nothing is written yet — the record begins on your \
                         first journal entry or the first write you make, so a boot that does \
                         nothing leaves nothing behind.",
            }),
        };
        if let Some(obj) = block.as_object_mut() {
            obj.insert("swept".into(), swept.into());
        }
        block
    }

    /// Whether **this session** has already told its story to the Journal — the
    /// other half of making a retry finish rather than repeat.
    ///
    /// **Scoped by session, and the mark is a LINE rather than a substring.**
    /// The Journal is one page holding every entry of every session there has
    /// ever been, so asking whether the story appears anywhere on it answers yes
    /// for work a different session did last month — and the wrap then reports
    /// success having written nothing, which is a dropped story: the very
    /// failure the guard trades a duplicate to avoid. A session tells its story
    /// at most once, because wrapping closes it for good, so its own mark is the
    /// whole question.
    ///
    /// Asking it of a whole line is what keeps the answer about this session:
    /// the mark is written on its own line, and a page can perfectly well carry
    /// the same characters inside somebody else's sentence — an entry quoting a
    /// mark, the operator's own handwriting — which a substring match reads as
    /// this session's entry. A line that has been joined to its story by hand
    /// stops matching and the retry writes a duplicate, which is the direction
    /// this whole guard is willing to fail in.
    ///
    /// Reads the Journal through the ordinary scan, because that is the only
    /// read there is: the Journal is nobody's entity, so there is no handle to
    /// fetch it by. A scan that fails answers "not there" and the wrap writes
    /// the entry — a duplicate line in the Journal is a cost worth paying to
    /// avoid dropping the story of a session that is about to close for good.
    async fn journal_holds(&self, mark: &str) -> bool {
        self.memory.scan().await.is_ok_and(|docs| {
            docs.iter().any(|doc| {
                doc.title.trim() == JOURNAL_TITLE
                    && doc.prose.lines().any(|line| line.trim() == mark)
            })
        })
    }

    /// Sweep this bot's stale sessions and hand back the live one, if any —
    /// **the half of attaching that reads and writes the board**, shared by the
    /// boot and by the first write that retries an attach a boot could not make.
    ///
    /// Binding is the caller's job: this returns what it found, and the two call
    /// sites differ in what they do with it.
    async fn sweep_and_find(
        &self,
        bot: &EntityId,
    ) -> Result<(Option<Session>, Vec<String>), SessionError> {
        let now = jiff::Timestamp::now();
        let existing = self.sessions.sessions_of(bot).await?;

        let mut swept = Vec::new();
        for stale in existing.iter().filter(|s| s.is_stale(now)) {
            match self.sessions.close(&stale.id, SessionState::Abandoned).await {
                Ok(_) => swept.push(stale.id.to_string()),
                // A sweep that cannot close one session must not stop a boot:
                // the session is left active and the next boot tries again.
                Err(e) => tracing::warn!(
                    error = %e, session = %stale.id,
                    "a stale session could not be swept — left active for the next boot"
                ),
            }
        }

        // Newest first already, so the first live one is the one to resume.
        let live = existing
            .into_iter()
            .find(|s| !s.state.is_terminal() && !s.is_stale(now));
        *self.bound.write().expect("binding poisoned") = Some(Bound {
            bot: bot.clone(),
            session: live.as_ref().map(|s| s.id.clone()),
            attached: true,
            // **Read back off the resumed session, not started empty.** The
            // tally belongs to the session; a connection is only holding it. An
            // empty map here made the first verb of each class after every
            // reconnect append a second beat for that class — and a reconnect is
            // the ordinary case, so the duplicate was the ordinary shape.
            beats: live.as_ref().map(beats_of).unwrap_or_default(),
        });
        Ok((live, swept))
    }
    /// The session a verb should write to: the one named outright, else the one
    /// this connection is bound to — **beginning it lazily** if the connection
    /// booted but has not written anything yet.
    ///
    /// An explicit id wins over the binding, always: a caller that names a
    /// session means that session, and silently writing somewhere else would be
    /// the worst kind of helpfulness.
    async fn working_session_locked(
        &self,
        // **Proof the session gate is held.** This function reads the binding,
        // awaits a store call, and writes the binding back; rmcp runs one task
        // per request, so two in-flight calls on one connection would otherwise
        // both see "no session yet" and both materialize a card — two cards for
        // one session, and a duplicate beat besides. Taking the guard by
        // reference makes the requirement impossible to forget rather than a
        // comment somebody has to read.
        _serialized: &tokio::sync::MutexGuard<'_, ()>,
        explicit: Option<&str>,
        explicit_focus: Option<&str>,
        derive_from: Option<&str>,
    ) -> Result<Result<SessionId, CallToolResult>, McpError> {
        if let Some(id) = explicit.map(str::trim).filter(|i| !i.is_empty()) {
            return Ok(Ok(SessionId(id.to_string())));
        }
        let bound = self.bound.read().expect("binding poisoned").clone();
        let Some(bound) = bound else {
            return Ok(Err(session_unbound()));
        };
        if let Some(id) = bound.session {
            return Ok(Ok(id));
        }
        // **A boot that could not read the board has not attached to anything**,
        // so beginning a session here would fork one that is already running.
        // The attach is retried at the first write instead, which is the next
        // moment the store might be up.
        //
        // **A retry that failed is not a retry that found nothing.** Falling
        // through to the begin below on an error made the outage fork the record
        // deterministically — the same wrong answer every time, not a race — for
        // the one bot whose session is in flight and unreadable. Only a retry
        // that SUCCEEDED and found nothing active reaches the begin.
        if !bound.attached {
            match self.sweep_and_find(&bound.bot).await {
                Ok((Some(live), _)) => return Ok(Ok(live.id)),
                Ok((None, _)) => {}
                Err(e) => return Err(session_error(e)),
            }
        }

        // The card materializes here, on the first write and never before.
        //
        // **The focus is DERIVED, and the entry is not touched.** A first write
        // is prose — a multi-line entry, a story, a line naming code in
        // backticks — and a focus is one line of display text. Feeding the one
        // to the other applied the focus's rules to text nobody offered as a
        // focus: the write failed with `invalid entry`, naming a parameter the
        // caller never passed, and the entry it was carrying was dropped.
        let focus = match explicit_focus.map(str::trim).filter(|f| !f.is_empty()) {
            // A focus the caller passed is theirs, and stays theirs — held to
            // the focus rules, refused in their own words if it breaks them.
            Some(theirs) => theirs.to_string(),
            None => display_line(derive_from.unwrap_or(FRESH_FOCUS)),
        };
        let begun = self
            .sessions
            .begin(NewSession {
                bot: bound.bot.clone(),
                focus,
                started_at: jiff::Timestamp::now(),
            })
            .await
            .map_err(session_error)?;
        if let Some(held) = self.bound.write().expect("binding poisoned").as_mut() {
            held.session = Some(begun.id.clone());
        }
        Ok(Ok(begun.id))
    }

    /// Record one coarse beat for a verb class — **at most one per class per
    /// session**, its count and examples corrected in place as the class repeats.
    ///
    /// One case leaves two lines of a class, and does so deliberately: a beat
    /// somebody rewrote by hand no longer parses as a tally, so [`beats_of`]
    /// does not find it and the class opens a fresh one beside it. Their words
    /// stay theirs — overwriting what a person wrote on the card to keep a count
    /// tidy is the worse trade.
    ///
    /// Silent by design in three cases, all of them "there is nobody to record
    /// this for": an unbound connection (jojobot will not guess which identity
    /// made a call), a session store that refuses, and a beat that fails to
    /// write. **A beat never fails the verb it is about.** A capture that landed
    /// did land; reporting it as failed because its footnote could not be
    /// written would make the record wrong in the more damaging direction.
    async fn beat(&self, class: &'static str, example: &str) {
        if self.bound.read().expect("binding poisoned").is_none() {
            return;
        }
        let Some((_, phrase)) = BEAT_CLASSES.iter().find(|(known, _)| *known == class) else {
            // A class with no phrase would render a beat nothing can read back,
            // so it writes none at all rather than one that breaks the tally on
            // the next reconnect.
            tracing::warn!(class, "no beat phrase for this verb class — no beat written");
            return;
        };
        let _serialized = self.session_gate.lock().await;
        let session = match self.working_session_locked(&_serialized, None, None, Some(phrase)).await
        {
            Ok(Ok(session)) => session,
            _ => return,
        };

        let held = self
            .bound
            .read()
            .expect("binding poisoned")
            .as_ref()
            .and_then(|b| b.beats.get(class).cloned());
        let outcome = match held {
            Some(mut beat) => {
                beat.count += 1;
                if beat.examples.len() < BEAT_EXAMPLES && !beat.examples.iter().any(|e| e == example)
                {
                    beat.examples.push(example.to_string());
                }
                let text = beat_text(phrase, &beat);
                let written = self
                    .sessions
                    .amend_beat(&session, &beat.entry, &text, jiff::Timestamp::now())
                    .await;
                written.map(|_| (class, beat))
            }
            None => {
                let beat = Beat {
                    entry: EntryId(String::new()),
                    count: 1,
                    examples: vec![example.to_string()],
                };
                let text = beat_text(phrase, &beat);
                self.sessions
                    .append(&session, NewEntry::beat(class, text, jiff::Timestamp::now()))
                    .await
                    .map(|entry| (class, Beat { entry: entry.id, ..beat }))
            }
        };
        match outcome {
            Ok((class, beat)) => {
                if let Some(held) = self.bound.write().expect("binding poisoned").as_mut() {
                    held.beats.insert(class, beat);
                }
            }
            Err(e) => tracing::warn!(
                error = %e, class, session = %session,
                "a session beat could not be written — the verb it is about still succeeded"
            ),
        }
    }

    /// Screen a mailbox claim against the boxes that exist, returning the
    /// refusal when it is a near miss of one.
    ///
    /// **This is the only invariant on this surface that needs both worlds at
    /// once**, and it is why it sits here rather than on a store's write path
    /// with every other gate: Memory cannot see mailboxes, and Mailboxes is
    /// deliberately ignorant of who might own one. The *decision* is still the
    /// domain's pure function — this only fetches the two halves and puts them
    /// together.
    ///
    /// A world that is down fails the write rather than waving it through: a
    /// claim nobody could screen is exactly the near-duplicate this gate exists
    /// to catch, and an entity is writable without one.
    async fn screen_claim(
        &self,
        claimed: &str,
        create_new: bool,
    ) -> Result<Option<CallToolResult>, McpError> {
        let name = MailboxName(claimed.trim().to_string());
        let existing: Vec<MailboxName> = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!(
                        "the claim on mailbox '{name}' could not be checked against the boxes \
                         that exist, so it was not written ({e}). Retry, or write the entity \
                         without a mailbox and claim it once the mailbox world is reachable."
                    ),
                    None,
                )
            })?
            .into_iter()
            .map(|b| b.name)
            .collect();

        let mailbox::guard::Decision::Block(candidates) =
            mailbox::guard::decide_claim(&name, &existing, create_new)
        else {
            return Ok(None);
        };
        Ok(Some(mailbox_blocked_body(
            name.as_str(),
            Some(&candidates),
            format!(
                "Nothing was written. '{name}' is a near miss of a mailbox that already exists, \
                 and a claim on the wrong name is an identity whose mail arrives somewhere it \
                 will never look. If one of the boxes above is the one meant, claim that name \
                 instead. If this really is a separate box — a sibling like worker-2 beside \
                 worker-1 — re-call with create_new: true, and open it with create_mailbox."
            ),
        )))
    }

    /// Create an entity of any kind. Screened by the write guard, so a handle
    /// or name that looks like one jojobot already knows comes back as
    /// candidates instead of a second record.
    #[tool(
        description = "Bring a new entity into existence (person/project/place/event/work/\
                       thing/org/topic) — the required first step before any other write may \
                       name it. Returns the stored entity. If its handle or any of its names \
                       resembles something jojobot already knows, NOTHING is written: the \
                       result says status: blocked with candidates and how_to_proceed. Use the \
                       candidate you meant, or re-call with create_new: true if this genuinely \
                       is a different thing sharing a name. An exact handle collision can never \
                       be forced — a handle has exactly one owner."
    )]
    async fn add_entity(
        &self,
        Parameters(args): Parameters<AddEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = entity_id(&args.kind, &args.handle)?;
        let claimed = args.mailbox.clone();
        // Screened before anything is written, so a blocked claim costs the
        // entity too — the claim was part of what the caller asked for.
        if let Some(name) = claimed.as_deref()
            && let Some(refused) = self
                .screen_claim(name, args.create_new.unwrap_or(false))
                .await?
        {
            return Ok(refused);
        }
        let new = NewEntity {
            id,
            name: args.name,
            aliases: args.aliases.unwrap_or_default(),
            source: args.source,
            crm: args.crm,
            mailbox: args.mailbox,
            boot: args
                .boot
                .as_deref()
                .map_or(Default::default(), jojobot_domain::memory::Boot::from_token),
            create_new: args.create_new.unwrap_or(false),
        };
        match self.memory.add_entity(new).await.map_err(memory_error)? {
            Guarded::Written(entity) => {
                self.beat("add_entity", entity.id.as_str()).await;
                json_result(&entity_json(&entity))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::Creating,
                claimed.as_deref(),
            )),
        }
    }

    /// The front door: one ranked list over entities, facts and prose.
    #[tool(
        description = "The front door — use it first, and any time you do not already hold the \
                       exact handle or address. One ranked list over entities, facts, free \
                       prose AND the messages in mailboxes at once. `query` is free text (ALL \
                       words must match) and is optional when a filter narrows it: kind · status \
                       (default active; superseded is excluded unless named) · provenance · \
                       subject · edge {shape, object} · include_mail; a call with neither query \
                       nor filter is refused. kind + edge answers a cross-entity question in one \
                       call (\"which people are in X\") by walking typed edges — prose that \
                       merely mentions X is not an answer. No hit comes back bare: a fact \
                       carries its whole row, its address (feed that to update_fact), and who it \
                       is `about` and where it is `home`d (a null name there means the handle \
                       names nothing — a real defect worth reporting); an entity or prose hit \
                       carries that entity's names and the edges its facts draw; a message hit \
                       carries its box, its state (new/read/processed — an archived report is \
                       findable, and the state is how you tell it from live work), its sender \
                       and the id read_message takes, plus a snippet rather than the whole body. \
                       Mail is searched by default — pass include_mail: false to leave session \
                       traffic out, and note that a `kind` filter also leaves it out, since a \
                       message belongs to no entity and so has no kind to match. ALWAYS read the \
                       `mail` field of the answer, in BOTH directions: searched: false means no \
                       message was searched at all, which is not the same as nothing matching; \
                       and searched: true can still be partial after a degraded start, where the \
                       hits are real but anything older than this server's start is missing. \
                       Whenever `mail` carries a `note`, that note says which case you are in — \
                       read it before concluding a message does not exist. No pagination — raise \
                       `limit` or ask a better question."
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
            include_mail: args.include_mail.unwrap_or(true),
            limit: args.limit.map_or(DEFAULT_LIMIT, |l| l as usize),
        };
        // Checked here as well as in the index: a malformed query is the caller's
        // mistake, and it should read as one no matter which adapter is behind us.
        query.validate().map_err(memory_error)?;
        let hits = self.search.search(&query).map_err(memory_error)?;
        let body = serde_json::json!({
            "count": hits.len(),
            "mail": mail_coverage(&query, self.search.mail_coverage()),
            "results": hits.iter().map(hit_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }

    /// Every entity jojobot knows, optionally narrowed to one kind.
    #[tool(
        description = "List the entities jojobot knows, optionally narrowed to one kind — the \
                       inventory. Use it to orient, or as the cheap existence check before a \
                       write that must name an entity; use search when you are looking for \
                       something. Metadata only — no facts, no ordering guarantee."
    )]
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
        description = "Edit what an entity is called, where it came from, or which mailbox it \
                       owns (name/aliases/source/crm/mailbox), in place. The handle never \
                       changes — there is no rename. Any change to what it is CALLED — name or \
                       aliases — faces the same check a creation does, because an alias is a \
                       name: it can come back status: blocked with candidates, and create_new: \
                       true is how you confirm a genuinely shared name. Claiming a mailbox \
                       another entity owns is also blocked, and create_new does NOT clear that \
                       one — a box has exactly one owner. Passing `aliases` REPLACES the whole \
                       set ([] clears it); source and crm edits are never questioned. A handle \
                       that names nothing comes back blocked with the nearest handles — it \
                       never creates."
    )]
    async fn update_entity(
        &self,
        Parameters(args): Parameters<UpdateEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let handle = EntityId::person(&args.handle);
        let claimed = args.mailbox.clone();
        // A claim moved onto an entity later is screened exactly as one written
        // at creation — otherwise the gate is a two-step walk around.
        if let Some(name) = claimed.as_deref()
            && let Some(refused) = self
                .screen_claim(name, args.create_new.unwrap_or(false))
                .await?
        {
            return Ok(refused);
        }
        let patch = EntityPatch {
            name: args.name,
            aliases: args.aliases,
            source: args.source,
            crm: args.crm,
            mailbox: args.mailbox,
            create_new: args.create_new.unwrap_or(false),
        };
        let written = match self.memory.update_entity(&handle, patch).await {
            Ok(written) => written,
            Err(e) => return memory_declined("update_entity", e),
        };
        match written {
            Guarded::Written(entity) => {
                self.beat("update_entity", entity.id.as_str()).await;
                json_result(&entity_json(&entity))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::Relabelling,
                claimed.as_deref(),
            )),
        }
    }

    /// Remember a fact about an entity. Returns the stored fact including the
    /// address a later `update_fact` can edit it through.
    #[tool(
        description = "Remember one fact about an entity: the claim, when it became true, and \
                       whether it is testimony or inference (default inference — a hypothesis, \
                       not a finding). It may also draw one typed edge at another entity. \
                       Returns the stored fact with the address you later edit it through. \
                       Every entity it names — the subject, and an edge's object — must \
                       ALREADY EXIST: one jojobot doesn't know comes back status: blocked with \
                       candidates and nothing is written. A genuinely new entity is two \
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
            Guarded::Written(fact) => {
                self.beat("capture", fact.subject.as_str()).await;
                json_result(&fact_json(&fact))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("capture"),
                None,
            )),
        }
    }

    /// Read back every fact about an entity, each with its address.
    #[tool(
        description = "Read every fact about one entity, each with the address that makes it \
                       editable through update_fact. Use it when you already hold the handle \
                       and want the whole picture; use search when you don't. Unlike search, \
                       this returns claims of every status, superseded included. An entity that \
                       exists with nothing recorded comes back as an empty list; a handle that \
                       names nothing comes back status: blocked with the nearest handles, never \
                       as an empty list. A fact recorded under this entity that claims to be \
                       about someone else comes back too — that mismatch is worth surfacing, and \
                       the address is how it gets repaired."
    )]
    async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        let subject = EntityId::person(&args.subject);
        let facts = match self.memory.recall(&subject).await {
            Ok(facts) => facts,
            Err(e) => return memory_declined("recall", e),
        };
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
                       confirmed_by_user. An address that names no fact comes back status: \
                       blocked with the addresses that do exist — it never creates."
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
        let written = match self.memory.update_fact(&address, patch).await {
            Ok(written) => written,
            Err(e) => return memory_declined("update_fact", e),
        };
        match written {
            Guarded::Written(fact) => {
                self.beat("update_fact", &fact.address().to_string()).await;
                json_result(&fact_json(&fact))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("update_fact"),
                None,
            )),
        }
    }

    /// Create a mailbox. Screened against the boxes that exist, so a near miss
    /// comes back as candidates instead of a second box nobody meant.
    #[tool(
        description = "Create a mailbox. The name is [a-z0-9-]+ and has exactly one spelling. \
                       If it looks like a box that already exists, returns candidates to confirm \
                       instead of creating one — a typo that mints a box is a message posted \
                       where nobody is listening. If the resemblance is deliberate (sibling \
                       boxes like worker-2 beside worker-1), re-call with create_new: true; an \
                       exact name is never overridden, because that box already exists."
    )]
    async fn create_mailbox(
        &self,
        Parameters(args): Parameters<CreateMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = MailboxName(args.name.trim().to_string());
        match self
            .mailboxes
            .create_mailbox(&name, args.create_new.unwrap_or(false))
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(created) => {
                self.beat("create_mailbox", created.name.as_str()).await;
                json_result(&mailbox_json(&created))
            }
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(&attempted, &candidates, BlockedBox::Creating)),
        }
    }

    /// Every mailbox, with what is new, seen, and handled in each.
    #[tool(
        description = "Every mailbox and what is waiting in it: new (left, never delivered) · \
                       read (delivered, nobody has finished with it) · processed (acted on — \
                       terminal, an archive; nothing is ever deleted). Each box also reports \
                       any items that could NOT be read as messages: they are counted nowhere, \
                       delivered nowhere, and cannot be processed, so this is the only place \
                       their existence shows — their ids are listed, and repairing one takes a \
                       person. If a message somebody expected is missing, look here before \
                       concluding it was never sent, and say what you find."
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
        description = "Leave a message for someone who is not in this conversation. The box \
                       must ALREADY EXIST — an unknown name comes back status: blocked with \
                       candidates and nothing is written; call create_mailbox first if it is \
                       genuinely new. Returns the stored message, including the id that \
                       read_message and mark_processed later target. Give it a `subject`: one \
                       line saying what the message is about, which is what a reader sees on the \
                       card and on a search hit before opening anything — put it there rather \
                       than on the body's first line. The `state` you get back is the state as \
                       it stands — it can already say `read` if a person picked the message up \
                       in between, and that is success, not a problem: the message exists and \
                       someone has it. `sender` is recorded exactly as you declare it — \
                       identity is not verified, so name yourself specifically enough that a \
                       reply can find you. `in_reply_to` links this message to the one it \
                       answers: optional, it must name a message that exists (a miss comes back \
                       blocked, nothing written), and it says only that the two are one exchange \
                       — it does not deliver the original, handle it, or oblige anybody."
    )]
    async fn post_message(
        &self,
        Parameters(args): Parameters<PostMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let new = NewMessage {
            mailbox: MailboxName(args.mailbox.trim().to_string()),
            body: args.body,
            subject: args.subject,
            sender: args.sender,
            // Stamped here, at the edge, for the same reason `capture` stamps a
            // date here: the domain stays clock-free, and a caller does not get
            // to backdate a message it is posting now.
            sent_at: jiff::Timestamp::now(),
            in_reply_to: args
                .in_reply_to
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| MessageId(id.to_string())),
        };
        // Declined rather than errored: a reply naming a message jojobot does
        // not hold is a bad reference, and every other bad reference on this
        // surface comes back as the blocked shape.
        let posted = match self.mailboxes.post_message(new).await {
            Ok(posted) => posted,
            Err(e) => return mailbox_declined(e),
        };
        match posted {
            mailbox::Guarded::Written(message) => {
                self.beat("post_message", message.mailbox.as_str()).await;
                json_result(&message_json(&message))
            }
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
        description = "Take delivery of everything unprocessed in a mailbox, oldest first, \
                       moving each message from `new` to `read`. There is no peek: reading IS \
                       taking delivery. Messages a previous read already handed over come back \
                       too, flagged seen_before: true — leftovers from an interrupted earlier \
                       read, not fresh mail. A message somebody else finished while this \
                       delivery was in flight is left out, so a delivery can be smaller than \
                       counts you saw a moment ago. An unknown box comes back status: blocked \
                       with candidates and delivers nothing. Act on what you receive, then call \
                       mark_processed for each. Draining a whole box makes every message in it \
                       yours to finish — use read_message when you want only one. ONLY CHECKING \
                       WHETHER ANYTHING IS WAITING? Use list_mailboxes — it reads counts without \
                       taking delivery, so a poll that finds an empty box costs nothing and owes \
                       nothing."
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

    /// Record one beat in this session's chronology, and optionally move what
    /// it says it is working on.
    #[tool(
        description = "Record ONE beat in your session's chronology — a literal journal, not a \
                       log. High-level: what you set out to do, what you found, what you \
                       decided, what went wrong. Not every tool call, not every file: a reader \
                       months from now wants the story, and a firehose buries it. `focus` \
                       rewrites what your session says it is working on RIGHT NOW, in place — \
                       the chronology is history, the focus is the present, and they answer \
                       different questions. The first journal entry (or the first write of any \
                       kind) is what brings your session card into being, so a boot that does \
                       nothing leaves nothing behind. Bound to the identity you booted as, so \
                       `session` is only needed to write to a different one. A session that is \
                       already wrapped or abandoned comes back status: blocked — closed is \
                       terminal both ways, and what is left to say belongs to a new session."
    )]
    async fn journal(
        &self,
        Parameters(args): Parameters<JournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let focus = args.focus.as_deref();
        let _serialized = self.session_gate.lock().await;
        let session = match self
            .working_session_locked(
                &_serialized,
                args.session.as_deref(),
                focus,
                Some(&args.entry),
            )
            .await?
        {
            Ok(session) => session,
            Err(refused) => return Ok(refused),
        };
        let entry = match self
            .sessions
            .append(&session, NewEntry::manual(args.entry, jiff::Timestamp::now()))
            .await
        {
            Ok(entry) => entry,
            Err(e) => return session_declined(e),
        };
        // The focus moves only once the beat is recorded: a session whose focus
        // says it is doing something its chronology never mentions is a record
        // that disagrees with itself.
        let moved = match focus {
            None => None,
            Some(focus) => match self.sessions.set_focus(&session, focus).await {
                Ok(session) => Some(session),
                Err(e) => return session_declined(e),
            },
        };
        json_result(&serde_json::json!({
            "session": session.as_str(),
            "entry": entry_json(&entry),
            "focus": moved.map(|s| s.focus),
        }))
    }

    /// Rewrite the newest entry in place.
    #[tool(
        description = "Rewrite your session's MOST RECENT chronology entry, in place — for a \
                       beat you got wrong or want to finish saying. Only the most recent one: \
                       everything older is append-only, because a journal that can be rewritten \
                       further back is not evidence of anything. A session with no entries yet \
                       comes back status: blocked rather than quietly writing your text as a \
                       first entry — an amend that silently became an append leaves a chronology \
                       saying something you did not mean. A closed session comes back blocked \
                       too."
    )]
    async fn amend_journal(
        &self,
        Parameters(args): Parameters<AmendJournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let _serialized = self.session_gate.lock().await;
        // No lazy begin: there is nothing to amend in a session that does not
        // exist yet, and creating one to hold a correction would be a card
        // minted by a verb whose whole job is to not add anything.
        //
        // **But the triage is the same triage.** A connection that never booted
        // gets told to boot, not told there is nothing to amend — those are
        // different facts — and a connection whose boot could not read the board
        // retries the attach here rather than answering "no entries" about a
        // session it never looked for. Unknown is not false.
        let session = match args.session.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => SessionId(id.to_string()),
            None => {
                let bound = self.bound.read().expect("binding poisoned").clone();
                let Some(bound) = bound else {
                    return Ok(session_unbound());
                };
                match bound.session {
                    Some(id) => id,
                    None if !bound.attached => {
                        match self.sweep_and_find(&bound.bot).await {
                            Ok((Some(live), _)) => live.id,
                            // Still unreadable, or genuinely nothing in flight.
                            Ok((None, _)) => return Ok(session_nothing_to_amend()),
                            Err(e) => return Err(session_error(e)),
                        }
                    }
                    None => return Ok(session_nothing_to_amend()),
                }
            }
        };
        match self.sessions.amend_last(&session, &args.entry).await {
            Ok(entry) => json_result(&serde_json::json!({
                "session": session.as_str(),
                "entry": entry_json(&entry),
            })),
            Err(e) => session_declined(e),
        }
    }

    /// End the session, telling its story into the Journal.
    #[tool(
        description = "End your session and tell its story. Three things happen together: the \
                       story is recorded in your chronology, it is written through to the \
                       operator's Journal as one dated entry carrying your session id on its own \
                       line (`[session <id>]`, so a person reading that page can see which run \
                       wrote it), and the session moves to `wrapped` — terminal both ways, so \
                       nothing appends to it or reopens it afterwards, and a later \
                       journal/amend_journal/wrap_session on that id comes back status: blocked. \
                       A wrap you have to retry finishes what the first attempt started rather \
                       than repeating it, so the story is told once in each place — which means \
                       it is your chronology's newest entry only when nothing was written \
                       between the attempts. Write the story for somebody with \
                       none of your context: what this run was for, what actually happened, what \
                       is left. A session that stops without wrapping is not lost — the next \
                       boot of the same identity sweeps it to `abandoned` after a day, and its \
                       chronology stays readable — but its story was never told, and that is the \
                       difference between the two endings."
    )]
    async fn wrap_session(
        &self,
        Parameters(args): Parameters<WrapSessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let _serialized = self.session_gate.lock().await;
        let session = match self
            .working_session_locked(&_serialized, args.session.as_deref(), None, Some(&args.story))
            .await?
        {
            Ok(session) => session,
            Err(refused) => return Ok(refused),
        };

        // **A retry must not tell the story twice.** The order below is the
        // right one — the story reaches the session's own record before
        // anything else, so a failure anywhere after it leaves the story safe
        // and the session open — but the step most likely to fail transiently is
        // the LAST one, the close. After that failure the story is already in
        // both places and the only move left is to wrap again, which without
        // this would append it to both a second time. So each write is guarded
        // by whether its own half is already done, and a retry finishes what the
        // first attempt started rather than repeating it.
        let story = jojobot_domain::session::normalize_entry(&args.story);
        // **Anywhere in the chronology, not just at its tail.** The retry is the
        // move left after a failed close, and the natural thing to write between
        // the two is a beat saying the wrap failed — which made the story no
        // longer the newest entry, and the retry told it again.
        let already = match self.sessions.read_session(&session).await {
            Ok(read) => read.entries.iter().rev().find(|e| !e.is_auto() && e.text == story).cloned(),
            // Not fatal: an unreadable session fails the append below, in that
            // verb's own words rather than this guard's.
            Err(_) => None,
        };
        let entry = match already {
            Some(told) => told,
            None => match self
                .sessions
                .append(&session, NewEntry::manual(&story, jiff::Timestamp::now()))
                .await
            {
                Ok(entry) => entry,
                Err(e) => return session_declined(e),
            },
        };

        let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
        // The entry carries the session's mark, which is what a retry looks for.
        // It is also the one thing a reader of the Journal cannot recover
        // otherwise: which run of which bot wrote this.
        let told = format!("{story}\n\n{}", journal_mark(&session));
        let journalled = match self.journal_holds(&journal_mark(&session)).await {
            // Already on the page — reported as the entry rather than the dated
            // block a fresh append reads back, because the date it first landed
            // under belongs to that attempt and this one cannot know it.
            true => told,
            false => self
                .memory
                .append_journal(today, &told)
                .await
                .map_err(memory_error)?,
        };

        let wrapped = match self.sessions.close(&session, SessionState::Wrapped).await {
            Ok(wrapped) => wrapped,
            Err(e) => return session_declined(e),
        };
        if let Some(held) = self.bound.write().expect("binding poisoned").as_mut()
            // **Only when it is this connection's own session.** Wrapping
            // somebody else's by id used to clear the binding anyway, orphaning
            // the live session, losing its tally, and making the next write mint
            // a second card for a session that was already running.
            && held.session.as_ref() == Some(&session)
        {
            // The connection keeps its identity and loses its session: a bot
            // that wraps and keeps working starts a new one, rather than
            // writing into an archive.
            held.session = None;
            held.beats.clear();
        }
        json_result(&serde_json::json!({
            "session": session_json(&wrapped),
            "entry": entry_json(&entry),
            "journal": journalled,
        }))
    }

    /// Take delivery of one message by id, leaving the rest of its box alone.
    #[tool(
        description = "Take delivery of ONE message by id — the selective half of read_mailbox, \
                       for when you want a single message (the one a search hit named) and have \
                       no business owning the rest of the box. That one moves `new` to `read`; \
                       nothing else in the box is touched. Same envelope a delivery hands over, \
                       seen_before and all: true means somebody had already taken this message, \
                       so it is a leftover rather than fresh mail. A `processed` message comes \
                       back unchanged and flagged — processed is a terminal archive, and reading \
                       one is reading history, not taking it on. Taking delivery is NOT handling: \
                       call mark_processed once you have acted, and only then. Two refusals wear \
                       the status: blocked shape — an id that names nothing at all, and an id \
                       naming an item jojobot cannot read, which comes with a `reason` and needs \
                       a person, not a retry."
    )]
    async fn read_message(
        &self,
        Parameters(args): Parameters<ReadMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = MessageId(args.message_id.trim().to_string());
        match self.mailboxes.read_message(&id).await {
            Ok(delivered) => json_result(&delivered_json(&delivered)),
            Err(e) => mailbox_declined(e),
        }
    }

    /// Retire a message once it has actually been acted on.
    #[tool(
        description = "Retire a message once it has been handled — terminal, an archive, never \
                       a deletion — optionally recording the outcome in `notes`. \
                       THE CRASH CONTRACT: call this ONLY AFTER you have acted on the message. \
                       Mark first and then fail, and the message is gone from every future \
                       delivery with nobody the wiser; act first and crash before marking, and \
                       the next read_mailbox hands it back as a leftover — recoverable. A \
                       FAILURE IS DATA, NOT A STATE: record it in notes (and reply with a new \
                       message if someone needs to know) — there is no failed status, because a \
                       message whose handling failed has still been handled. For a pure \
                       acknowledgement — an ack, a heads-up, a round-closed note, anything whose \
                       whole content is now known to you — READING IT IS THE ACTING, so process \
                       it with a note and move on; the order matters for work you still owe, not \
                       for work that was never owed. Write the outcome you actually have: a note \
                       longer than the card holds is CUT to fit and says so (a trailing ellipsis, \
                       and notes_truncated: true), never refused — the verb that retires a \
                       message will not fail over the length of its own record. A message can be \
                       processed straight from `new`, no delivery first. Two refusals wear the \
                       same status: blocked shape and mean different things: an id that names \
                       nothing at all (use one read_mailbox or post_message handed you), and an \
                       id naming an item jojobot cannot read, which comes with a `reason` — \
                       retrying that one will not help, a person has to repair it, and until \
                       then treat whatever it carried as unhandled and say so."
    )]
    async fn mark_processed(
        &self,
        Parameters(args): Parameters<MarkProcessedArgs>,
    ) -> Result<CallToolResult, McpError> {
        let id = MessageId(args.message_id.trim().to_string());
        // What the caller asked to record, blank-is-absent — the store applies
        // the same rule, so anything else coming back means it made a cut.
        let asked = args.notes.as_deref().map(str::trim).filter(|n| !n.is_empty());
        match self.mailboxes.mark_processed(&id, args.notes.as_deref()).await {
            Ok(processed) => {
                self.beat("mark_processed", processed.id.as_str()).await;
                let mut body = message_json(&processed);
                if let Some(obj) = body.as_object_mut() {
                    // **Always present, never inferred from the ellipsis.** The
                    // record can legitimately end in one, and a reader that has
                    // to guess whether a store cut its text is a reader that
                    // will eventually guess wrong.
                    obj.insert(
                        "notes_truncated".into(),
                        (processed.notes.as_deref() != asked).into(),
                    );
                }
                json_result(&body)
            }
            // Both misses here are answers, not failures: an id that names
            // nothing, and an id naming a card jojobot cannot read. They stay
            // different answers — one is repairable by a better id, the other
            // only by a person on the board — in the guards' one shape.
            Err(e) => mailbox_declined(e),
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
        // A mail hit is unmistakably mail: the whole envelope, so a reader can
        // tell live work from an archived report without a second call, and the
        // id that takes delivery of the rest. `body` is deliberately absent —
        // what is here is the snippet, and read_message is how the message is
        // taken whole.
        Hit::Message { message, snippet } => serde_json::json!({
            "hit": "message",
            "id": message.id.as_str(),
            "mailbox": message.mailbox.as_str(),
            "state": message.state.as_token(),
            "sender": message.sender,
            "subject": message.subject,
            "sent_at": message.sent_at.to_string(),
            "notes": message.notes,
            "snippet": snippet,
        }),
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

/// Prose reduced to one line a display field can carry.
///
/// **A cut, never a refusal.** This is what a focus is derived from when the
/// caller offered none, and the text it is derived from is the record: an
/// entry, a story. Refusing prose because a *display* field cannot hold it
/// would throw away the thing worth keeping to protect the thing that is only a
/// glance — which is exactly what it did.
///
/// The rules — one line, no backtick or control character, cut on a word
/// boundary with an ellipsis inside the cap — are [`text::FOCUS_LINE`]. Each is
/// a rule of the *field* rather than a judgement about the text, which is why
/// they are declared there beside the other fields' and pinned by a golden.
fn display_line(prose: &str) -> String {
    text::FOCUS_LINE.render(prose)
}

/// The mark a session's Journal entry carries, so a wrap that has to be
/// retried can find **its own** entry on a page holding everybody's.
///
/// Written on its own line, which is how [`Jojobot::journal_holds`] reads it:
/// matching the whole line is what tells `[session 1]` from `[session 12]` and
/// what keeps the same characters inside somebody's sentence from counting.
/// **The brackets are for the person reading the page** — they mark the line as
/// jojobot's rather than part of the story — and they are belt to the line's
/// braces, not the thing holding the ids apart.
///
/// Session ids are minted by the one store, so the id alone says which run of
/// which bot without naming the bot twice.
fn journal_mark(session: &SessionId) -> String {
    format!("[session {session}]")
}

/// A running tally, as one line of chronology.
///
/// **One shape, always, including at a count of one** — because this line is
/// where the tally LIVES. The handler's copy is per connection and a session
/// outlives connections, so a resumed session's counts are read back out of the
/// entries by [`parse_beat`], and a rendering that dropped the count for the
/// first occurrence would make the two disagree the moment somebody reconnects.
fn beat_text(phrase: &str, beat: &Beat) -> String {
    let mut named = beat.examples.join(", ");
    // Said out loud when the examples stop naming everything, so the line does
    // not read as a complete list that happens to be short.
    if beat.examples.len() < beat.count {
        named.push_str(", …");
    }
    format!("{phrase}: {named} ({})", beat.count)
}

/// Read a tally back out of the line it was rendered as — the inverse of
/// [`beat_text`], and the reason a resumed session keeps counting rather than
/// starting over.
///
/// `None` for a line this did not write: a beat whose text a person edited by
/// hand is left exactly as they left it, and the class starts a fresh tally
/// rather than jojobot rewriting their words into its own format.
fn parse_beat(phrase: &str, entry: &JournalEntry) -> Option<Beat> {
    let rest = entry.text.strip_prefix(phrase)?.strip_prefix(": ")?;
    let (named, count) = rest.rsplit_once(" (")?;
    let count: usize = count.strip_suffix(')')?.parse().ok()?;
    let examples: Vec<String> = named
        .trim_end_matches(", …")
        .split(", ")
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect();
    Some(Beat {
        entry: entry.id.clone(),
        count,
        examples,
    })
}

/// The tally this session already has, read off its chronology — what makes the
/// one-beat-per-class rule belong to the SESSION rather than to whichever
/// connection happens to be holding it.
fn beats_of(session: &Session) -> std::collections::HashMap<&'static str, Beat> {
    let mut found = std::collections::HashMap::new();
    for entry in &session.entries {
        let Some(class) = entry.beat.as_deref() else { continue };
        let Some((class, phrase)) = BEAT_CLASSES.iter().find(|(known, _)| *known == class) else {
            continue;
        };
        if let Some(beat) = parse_beat(phrase, entry) {
            found.insert(*class, beat);
        }
    }
    found
}

/// Every verb class jojobot beats, and the phrase its tally is written with.
///
/// **One table, because the phrase is half the parse.** A beat is rendered from
/// it and read back through it, so a class whose phrase lived only at its call
/// site would render fine and come back unparseable on the next reconnect.
const BEAT_CLASSES: &[(&str, &str)] = &[
    ("add_entity", "brought entities into being"),
    ("update_entity", "edited entities"),
    ("capture", "captured facts about"),
    ("update_fact", "edited facts"),
    ("set_charter", "wrote charters for"),
    ("create_mailbox", "opened mailboxes"),
    ("post_message", "posted to mailboxes"),
    ("mark_processed", "retired messages"),
];

/// One session on the wire — the record, its chronology, and where it sits.
fn session_json(session: &Session) -> serde_json::Value {
    serde_json::json!({
        "id": session.id.as_str(),
        "bot": session.bot.as_str(),
        "focus": session.focus,
        "started_at": session.started_at.to_string(),
        "state": session.state.as_token(),
        "entry_count": session.entries.len(),
        "chronology": session.entries.iter().map(entry_json).collect::<Vec<_>>(),
    })
}

/// One chronology entry. `beat` names the verb class for an entry **jojobot**
/// wrote and is null for one the session wrote — a reader weighing a chronology
/// has to tell an account of intent from a tally of calls.
fn entry_json(entry: &JournalEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id.as_str(),
        "at": entry.at.to_string(),
        "text": entry.text,
        "beat": entry.beat,
    })
}

/// A session verb reached on a connection that never booted. Not an error: the
/// caller did nothing malformed, they just have no identity yet.
fn session_unbound() -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": "Nothing was written. This connection is not running as any identity, \
                           so there is no session to write to and jojobot will not guess which \
                           one you meant. Call boot_bot with the identity you were told you are \
                           — that starts or resumes its session — or name an existing `session` \
                           outright if you are writing to somebody else's.",
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// An amend on a session that has not begun. Refused rather than turned into a
/// first entry.
fn session_nothing_to_amend() -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": "Nothing was written. Your session has no entries yet — it has not \
                           even been written to disk, because a session's record begins on its \
                           first beat. There is nothing to amend, so use journal to record the \
                           first one.",
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// The session context's half of "a miss is an answer, not a failure": an id
/// that names nothing, a session that is closed, and an amend with nothing to
/// amend all come back in the guards' one shape.
fn session_declined(e: SessionError) -> Result<CallToolResult, McpError> {
    let blocked = |attempted: &str, how: String| {
        let body = serde_json::json!({
            "status": "blocked",
            "attempted": attempted,
            "wrote": false,
            "how_to_proceed": how,
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(body.to_string())]))
    };
    match e {
        SessionError::UnknownSession { attempted } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. No session on jojobot's board has the id '{attempted}'. \
                 Ids are minted by jojobot and handed back by boot_bot — boot as the identity \
                 you are and use the session it gives you, rather than composing an id."
            ),
        ),
        SessionError::Closed { attempted, state } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' is {state} — closed, and closed is \
                 terminal both ways. Its chronology stands as the record of what happened; \
                 nothing appends to it, amends it, or reopens it. If there is more to say, it \
                 belongs to a new session: boot_bot starts one."
            ),
        ),
        SessionError::NoEntries { attempted } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' has no entries yet, so there is no \
                 most-recent one to amend — journal it instead."
            ),
        ),
        SessionError::NotABeat { attempted, session } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Entry '{attempted}' on session '{session}' is one the \
                 session recorded itself, and those are append-only wherever they sit. Only the \
                 most recent entry can be amended, through amend_journal."
            ),
        ),
        other => Err(session_error(other)),
    }
}

/// Map a [`SessionError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split the other two contexts make.
fn session_error(e: SessionError) -> McpError {
    match e {
        SessionError::InvalidId(_) | SessionError::InvalidEntry(_) => {
            McpError::invalid_params(e.to_string(), None)
        }
        // Reached only if a verb surfaces one without going through
        // `session_declined` — kept as a client error rather than a 500 for the
        // same reason the other contexts keep theirs.
        SessionError::UnknownSession { .. }
        | SessionError::Closed { .. }
        | SessionError::NoEntries { .. }
        | SessionError::NotABeat { .. } => McpError::invalid_params(e.to_string(), None),
        SessionError::ForeignProject(_)
        | SessionError::Stranded { .. }
        | SessionError::Store(_)
        | SessionError::NotConfigured(_) => McpError::internal_error(e.to_string(), None),
    }
}

/// **Whether this answer covered mail, and why not when it didn't.**
///
/// One shape, always present, so a caller reads it in one pass instead of
/// branching on which keys came back — the same deal `owned_mailbox` makes.
///
/// It exists because silence is a lie here. A search is a read of an in-process
/// index: if the mailbox world was unreachable when that index was built, mail
/// is simply not in it, and an answer that comes back without mail hits and
/// without a word reads as "no message says that". That is a different claim
/// from "jojobot has read no messages", and it is the one a caller acts on.
fn mail_coverage(query: &SearchQuery, coverage: MailCoverage) -> serde_json::Value {
    let excluded = |note: &str| serde_json::json!({ "searched": false, "note": note });
    if !query.include_mail {
        return excluded("you passed include_mail: false, so messages were left out of this answer.");
    }
    if query.is_fact_scoped() {
        return excluded(
            "this query filters on a property only a fact has (status, provenance, subject or \
             edge), so it is a question about facts — messages, entities and prose are all out \
             of it.",
        );
    }
    // **A `kind` filter excludes mail, silently and structurally.** A message
    // belongs to no entity, so it has no kind to match — the filter drops it
    // exactly as it drops prose in a doc that is nobody's. Saying `searched:
    // true` here was the field's one wrong answer, and a field a caller is told
    // to trust has to be right in every case rather than in most of them.
    if query.kind.is_some() {
        return excluded(
            "this query narrows to one entity kind, and a message belongs to no entity, so \
             mail was left out of it. Drop `kind` to search messages too.",
        );
    }
    match coverage {
        MailCoverage::Unread => excluded(
            "jojobot has not been able to read the mailbox board, so NO message is searchable \
             right now — this is not 'nothing matched'. The memory half of this answer is \
             complete. list_mailboxes will say what is wrong.",
        ),
        // Searched, and said so — hits are real. But the board read failed, so
        // only what this server has handled since is in there, and a caller
        // hunting an older message has to be told rather than shown an empty
        // list. Reporting this as `searched: false` was an answer that carried
        // message hits and denied having searched any.
        MailCoverage::Partial => serde_json::json!({
            "searched": true,
            "note": "PARTIAL: jojobot could not read the mailbox board at startup, so only \
                     messages it has handled since are searchable. Any hit here is real, but an \
                     older message may be missing — this is not a complete answer over mail. \
                     list_mailboxes will say what is wrong.",
        }),
        MailCoverage::Loaded => serde_json::json!({ "searched": true }),
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
        // The box whose mail is this entity's. Null for everything that owns
        // none, which is nearly everything.
        "mailbox": entity.mailbox,
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
        // schema.org has no bot; `SoftwareApplication` is its nearest word for
        // a non-human actor, and it is the one a model already knows.
        EntityKind::Bot => "SoftwareApplication",
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
    claimed: Option<&str>,
) -> CallToolResult {
    let exact = candidates
        .iter()
        .any(|c| c.reason == guard::MatchReason::ExactHandle);
    // A claimed box is its own gate whichever verb carried the claim, and the
    // advice for the verb would be actively wrong here — it would send the
    // caller back with an override that cannot clear this.
    let claimants: Vec<&EntityMatch> = candidates
        .iter()
        .filter(|c| c.reason == guard::MatchReason::MailboxClaimed)
        .collect();
    if let Some(owner) = claimants.first() {
        let held: Vec<&str> = claimants.iter().map(|c| c.handle.as_str()).collect();
        let name = claimed.unwrap_or("that mailbox");
        return blocked_body(
            attempted,
            candidates,
            format!(
                "Nothing was written. The mailbox '{name}' is already owned by {} — a box has \
                 exactly one owner, and there is no override for this: a second owner means each \
                 one's mark_processed is the other's message gone from every future delivery. \
                 Either '{name}' IS {}'s box (leave the claim off '{attempted}' and let it stay \
                 where it is), or '{attempted}' needs a box of its own under a different name.",
                held.join(", "),
                owner.handle,
            ),
        );
    }
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
    blocked_body(attempted, candidates, how_to_proceed)
}

/// The blocked envelope itself, once — so every gate's advice arrives in one
/// shape and a client branches on `status`, never on which gate fired.
fn blocked_body(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    how_to_proceed: String,
) -> CallToolResult {
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
        // Null for every message posted before there was a field for one, and
        // for every one posted without it since. Absent-as-null rather than an
        // omitted key: a reader must not have to branch on whether it is there.
        "subject": message.subject,
        "body": message.body,
        "state": message.state.as_token(),
        "notes": message.notes,
        // Null for a message that answers nothing, which is most of them. A
        // link, never a status: it says these two are one exchange and nothing
        // about whether either has been handled.
        "in_reply_to": message.in_reply_to.as_ref().map(|id| id.as_str()),
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
             that already exists. If one of the boxes above is the one you meant, use its name. \
             If the resemblance is deliberate — sibling boxes like worker-2 beside worker-1 — \
             re-call create_mailbox with create_new: true to override the similarity screen. \
             An exact match cannot be overridden: that box already exists.",
        ),
        BlockedBox::MustExist(verb) if candidates.is_empty() => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows, and nothing \
             resembles it. {verb} cannot create one — and a new box is rarely the answer: a \
             mailbox is a channel someone must be draining, so use list_mailboxes to pick an \
             existing box, or tell the operator there is nowhere fitting to put this. Only \
             create_mailbox '{attempted}' if the operator or a standing arrangement asked for \
             that box by name.",
        ),
        BlockedBox::MustExist(_) => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows. If one of the \
             names above is what you meant, use that — it is almost certainly a typo. \
             Otherwise: a new box is rarely the answer (a mailbox is a channel someone must be \
             draining), so prefer an existing box or ask the operator; create_mailbox only if \
             this box was asked for by name.",
        ),
    };
    mailbox_blocked_body(attempted.as_str(), Some(candidates), how_to_proceed)
}

/// The mailbox blocked envelope itself, once. `None` candidates is a refusal
/// with nothing to suggest — an id nothing answers to — and the key is still
/// present and empty, because a client that branches on its shape must not have
/// to branch on whether it is there.
fn mailbox_blocked_body(
    attempted: &str,
    candidates: Option<&[MailboxMatch]>,
    how_to_proceed: String,
) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "candidates": candidates
            .unwrap_or_default()
            .iter()
            .map(mailbox_candidate_json)
            .collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A quarantined card, answered in the guards' own shape.** The id is real —
/// jojobot is looking straight at the card — but it cannot be read as a
/// message, so no verb will act on it until a person repairs it. A successful result carrying a
/// structured refusal, exactly like a blocked write: same `status` / `wrote` /
/// `how_to_proceed` keys, so one client-side branch handles every "jojobot
/// declined, here is what to do" answer in this context.
fn mailbox_quarantined(attempted: &str, reason: &str) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "reason": format!("card {attempted} is quarantined: {reason}"),
        "how_to_proceed": format!(
            "Nothing was written, and retrying will not help — this is not a missing message. \
             Card {attempted} is on a jojobot mailbox board, but jojobot cannot read it as a \
             message, so no verb will act on it. A PERSON has to open that card in the task board \
             and put back whichever of the three things above is missing: its mailbox label, its \
             machine block, or a place in one of the funnel's columns. **All three, not one** — a \
             card moved into a funnel column while still missing its label reads as somebody \
             else's note and goes invisible to jojobot entirely, which is worse than where it is \
             now. Until then, treat the message it was carrying as unhandled and say so."
        ),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A miss and a block speak one shape.** An id, handle or address that names
/// nothing is not a malformed call and not a server failure: it is jojobot
/// declining because what the caller named is not there — the same answer the
/// resemblance and existence gates give — so it comes back as a *successful*
/// result whose body says `status: blocked`, `wrote: false`, with whatever is
/// nearby and what to do next.
///
/// Two shapes for one idea meant a client had to branch twice to learn the same
/// thing, and the error half read as a broken server: clients that retry on
/// error retry it, and clients that unwrap on error handle it exactly wrong.
///
/// Everything that is genuinely a caller mistake (a malformed address, an
/// unknown kind token) or genuinely a failure (the store is down) stays an
/// error. `Ok` here is the refusal; `Err` is still an error.
fn memory_declined(verb: &'static str, e: MemoryError) -> Result<CallToolResult, McpError> {
    match e {
        MemoryError::UnknownEntity { attempted, nearest } => Ok(blocked_result(
            &EntityId(attempted),
            &nearest,
            Blocked::MustExist(verb),
            None,
        )),
        // A fact miss has no entity candidates — its near misses are the live
        // addresses in the same doc, which is what makes it repairable.
        MemoryError::UnknownFact { attempted, nearest } => {
            let live = if nearest.is_empty() {
                "That entity holds no facts at all yet, so there is nothing here to edit — \
                 capture one first."
                    .to_string()
            } else {
                format!("The addresses that do exist here are: {}.", nearest.join(", "))
            };
            Ok(blocked_body(
                &EntityId(attempted.clone()),
                &[],
                format!(
                    "Nothing was written. '{attempted}' addresses no fact jojobot holds, and this \
                     verb never creates one. {live} Recall the entity if none of them is what you \
                     meant — every fact comes back carrying the address that edits it."
                ),
            ))
        }
        other => Err(memory_error(other)),
    }
}

/// The mailbox half of [`memory_declined`]: an id that names nothing, and the
/// quarantined card that names something jojobot cannot read. Different answers
/// — one is repairable by a better id, the other only by a person on the board
/// — in one shape.
fn mailbox_declined(e: MailboxError) -> Result<CallToolResult, McpError> {
    match e {
        MailboxError::UnknownMessage { attempted } => Ok(mailbox_blocked_body(
            &attempted,
            None,
            format!(
                "Nothing was written. No message jojobot holds has the id '{attempted}', in any \
                 mailbox. Ids are minted by jojobot and handed back by search, read_mailbox and \
                 post_message — use an id from one of those rather than composing one."
            ),
        )),
        MailboxError::Quarantined { attempted, reason } => {
            Ok(mailbox_quarantined(&attempted, &reason))
        }
        other => Err(mailbox_error(other)),
    }
}

/// Map a domain [`MailboxError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split [`memory_error`] makes.
fn mailbox_error(e: MailboxError) -> McpError {
    match e {
        MailboxError::InvalidName(_)
        | MailboxError::InvalidMessageId(_)
        | MailboxError::InvalidMessage(_)
        | MailboxError::UnknownMessage { .. }
        // Reached only if a verb other than mark_processed ever surfaces one;
        // that verb renders it as a structured result instead.
        | MailboxError::Quarantined { .. } => McpError::invalid_params(e.to_string(), None),
        // Neither of these is a caller mistake, and neither is something a
        // caller can fix by calling differently: jojobot found a card on its
        // own board that belongs to another project and refused, or a write
        // failed and could not be undone, leaving a card mid-verb. Both are
        // integrity conditions on the server side that need a person.
        MailboxError::ForeignProject(_) | MailboxError::Stranded { .. } => {
            McpError::internal_error(e.to_string(), None)
        }
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

/// Read a bot handle off a name. A bare name is a bot here — this is the bot
/// door, so a bare slug is read with the bot kind on it — and a handle of
/// another kind is a client
/// error rather than a silent winner: booting a person as an identity would
/// hand somebody's page back as a charter.
fn bot_id(name: &str) -> Result<EntityId, McpError> {
    let name = name.trim();
    match name.split_once(':') {
        None => Ok(EntityId::new(EntityKind::Bot, name)),
        Some(("bot", slug)) => Ok(EntityId::new(EntityKind::Bot, slug)),
        Some((kind, _)) => Err(McpError::invalid_params(
            format!(
                "'{name}' is a {kind}, and this verb takes a bot — pass a bare name, or a handle \
                 with the bot kind on it"
            ),
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
                "jojobot — a personal-assistant server. Two worlds live here.\
                 \n\n**MEMORY.** What jojobot knows is **entities** — a person, project, place, \
                 event, work, thing, org or topic, each with a permanent typed handle, \
                 `kind:slug` — and **facts** about them: single dated claims, each carrying an \
                 **address** (`kind:slug#local-id`) it can be edited through and a \
                 **provenance** — `testimony` (the user said or confirmed it) or `inference` \
                 (you derived it). **Inference is the default and reads back as a hypothesis, \
                 never as truth**; only the user's explicit confirmation promotes a claim. A \
                 fact may also draw one typed **edge** at another entity — `location` · \
                 `membership` · `attendance` · `about` — and edges are what make cross-entity \
                 questions (\"which people are in X\") answerable without reading everything. \
                 **Start with `search`**: one ranked list over entities, facts, free prose and \
                 mailbox messages at once, every hit arriving with its surroundings.\
                 \n\n**MAILBOXES.** A place to leave a message for someone who is not in this \
                 conversation. A mailbox is a named box (`[a-z0-9-]+`); a message in one is \
                 `new` → `read` → `processed`. **Read is not processed, and processed is not \
                 deleted**: reading takes delivery, processing means you acted, and `processed` \
                 is a terminal archive. **Messages are searchable**: `search` returns them beside \
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
                 meant, or re-call with `create_new: true` if it truly is a different thing \
                 sharing a name. **Naming something that does not exist is blocked too**, with \
                 whatever is nearby — never a plain error, so branch on `status`, not on whether \
                 the call errored. A plain error is a malformed call, or the store failing. \
                 Nothing on this surface deletes anything. 3. **Mark a message processed only \
                 AFTER acting on it**: \
                 mark first and then fail, and it is gone from every future delivery with \
                 nobody the wiser; act first and crash, and the next read hands it back, \
                 flagged `seen_before` — recoverable.\
                 \n\nResponses name types the schema.org way (`Person`, `CreativeWork`, \
                 `memberOf`); input stays lowercase (`person`, `membership`, `kind:slug`)."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use async_trait::async_trait;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Boot, Edge, EdgeShape, EntityKind, FactId};

    /// A [`Search`] double: it records the query it was handed and answers with
    /// canned hits. On this path the MCP layer's whole job is translating
    /// arguments into a query and hits into JSON, and that is exactly what this
    /// pins — the ranking and matching are the index's tests, not these.
    struct SpySearch {
        seen: Mutex<Option<SearchQuery>>,
        hits: Mutex<Vec<Hit>>,
        /// How much of the mail board this double claims to hold. Default
        /// loaded: an index that has read the board is the ordinary case, and
        /// the degraded ones are worth writing down at a call site.
        coverage: MailCoverage,
    }

    impl Default for SpySearch {
        fn default() -> Self {
            SpySearch {
                seen: Mutex::new(None),
                hits: Mutex::new(Vec::new()),
                coverage: MailCoverage::Loaded,
            }
        }
    }

    impl SpySearch {
        fn answering(hits: Vec<Hit>) -> Self {
            SpySearch {
                hits: Mutex::new(hits),
                ..Default::default()
            }
        }

        /// A search port at a given mail coverage — the states a degraded index
        /// reports.
        fn covering(coverage: MailCoverage, hits: Vec<Hit>) -> Self {
            SpySearch {
                hits: Mutex::new(hits),
                coverage,
                ..Default::default()
            }
        }

        /// A search port whose mailbox world was never readable — the state an
        /// index is in when the boot scan of the board failed and nothing has
        /// indexed a message since.
        fn with_no_mail_indexed() -> Self {
            Self::covering(MailCoverage::Unread, Vec::new())
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

        fn mail_coverage(&self) -> MailCoverage {
            self.coverage
        }
    }

    fn handler() -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
        )
    }

    /// A handler whose search port is a spy the test keeps a handle on.
    fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            spy,
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
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
                mailbox: None,
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
            mailbox: None,
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
            include_mail: None,
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
                include_mail: Some(false),
                limit: Some(5),
            }))
            .await
            .expect("search ok");

        let query = spy.query();
        assert_eq!(query.terms(), Some("winter"));
        assert!(!query.include_mail, "the caller's exclusion must reach the port");
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

    /// **Mail comes back in the one list, and unmistakably as mail.** A message
    /// hit says which box, which state, who sent it, and the id `read_message`
    /// takes — without those it is an anonymous paragraph and a reader cannot
    /// tell a live task from an archived report. The body is a snippet: taking
    /// the whole message is `read_message`'s job, and that is a deliberate act.
    #[tokio::test]
    async fn a_message_hit_arrives_with_its_whole_envelope() {
        let spy = Arc::new(SpySearch::answering(vec![Hit::Message {
            message: Message {
                id: MessageId("42".into()),
                mailbox: MailboxName("pm".into()),
                body: "The kiln rebuild landed; the damper is still hand-cut.".into(),
                subject: Some("the kiln slice".into()),
                sender: "dev (implementer)".into(),
                sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                state: mailbox::MessageState::Processed,
                notes: Some("filed".into()),
                in_reply_to: None,
            },
            snippet: "…the damper is still hand-cut…".into(),
        }]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        let hit = &body["results"][0];
        assert_eq!(hit["hit"], "message", "a caller must not have to guess from the shape");
        assert_eq!(hit["id"], "42", "the id read_message takes");
        assert_eq!(hit["mailbox"], "pm");
        assert_eq!(hit["state"], "processed", "an archive reads as one");
        assert_eq!(hit["sender"], "dev (implementer)");
        assert_eq!(hit["subject"], "the kiln slice");
        assert_eq!(hit["notes"], "filed");
        assert!(hit["sent_at"].is_string());
        assert_eq!(hit["snippet"], "…the damper is still hand-cut…");
        assert!(
            hit["body"].is_null(),
            "the whole body is read_message's to hand over, not a hit's: {hit}"
        );
        assert_eq!(body["mail"]["searched"], true);
    }

    /// **A search that could not see mail says so.** Coming back without mail
    /// hits and without a word reads as "no message says that", which is a
    /// different claim from "jojobot has read no messages" — and it is the one a
    /// caller acts on. The memory half is unaffected: degrade, don't error.
    #[tokio::test]
    async fn a_search_says_when_no_message_was_searched_at_all() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::with_no_mail_indexed()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
                .await
                .expect("a down mailbox world must not break search"),
        );
        assert_eq!(body["mail"]["searched"], false);
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("not 'nothing matched'"),
            "the note has to draw the distinction it exists for: {note}"
        );

        // The caller's own exclusion is a different absence, and says so.
        let excluded = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: Some(false),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(excluded["mail"]["searched"], false);
        assert!(
            excluded["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("include_mail"),
            "an exclusion the caller asked for must not read as an outage: {excluded}"
        );

        // …and so is a query that is about facts to begin with.
        let fact_scoped = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    provenance: Some("testimony".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(fact_scoped["mail"]["searched"], false);
        assert!(
            fact_scoped["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("only a fact has"),
            "got {fact_scoped}"
        );
    }

    /// **THE INVARIANT: no answer both returns a message hit and claims no
    /// message was searched.** After a failed boot board read, every verb still
    /// indexes the messages it touches and search still returns them — while the
    /// coverage flag stayed false for the life of the process. One answer said
    /// both things at once, and a caller reading the field it is told to trust
    /// would discard a hit that is real.
    ///
    /// The fix is a third state, not a flipped flag: hits are real, but the
    /// board was never read, so anything older than this process is missing —
    /// which a caller hunting an old message has to be told rather than shown an
    /// empty list.
    #[tokio::test]
    async fn an_answer_carrying_a_message_never_claims_no_mail_was_searched() {
        let hit = || {
            vec![Hit::Message {
                message: Message {
                    id: MessageId("42".into()),
                    mailbox: MailboxName("pm".into()),
                    body: "the damper is still hand-cut".into(),
                    subject: None,
                    sender: "dev".into(),
                    sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                    state: mailbox::MessageState::New,
                    notes: None,
                    in_reply_to: None,
                },
                snippet: "…the damper…".into(),
            }]
        };

        for coverage in [MailCoverage::Partial, MailCoverage::Loaded] {
            let body = json_of(
                &handler_with(Arc::new(SpySearch::covering(coverage, hit())))
                    .search(Parameters(SearchArgs {
                        query: Some("damper".into()),
                        ..search_args()
                    }))
                    .await
                    .expect("search ok"),
            );
            assert!(
                body["results"].as_array().expect("results").iter().any(|h| h["hit"] == "message"),
                "the double answered with a message: {body}"
            );
            assert_eq!(
                body["mail"]["searched"], true,
                "an answer carrying a message hit cannot claim no message was searched \
                 ({coverage:?}): {body}"
            );
        }

        // …and the degraded one still says it is degraded, or the caller reads a
        // partial answer over mail as a complete one.
        let partial = json_of(
            &handler_with(Arc::new(SpySearch::covering(MailCoverage::Partial, hit())))
                .search(Parameters(SearchArgs { query: Some("damper".into()), ..search_args() }))
                .await
                .expect("search ok"),
        );
        assert!(
            partial["mail"]["note"]
                .as_str()
                .expect("a partial answer says it is partial")
                .contains("PARTIAL"),
            "got {partial}"
        );
    }

    /// **A `kind` filter excludes every message, and the answer has to say so.**
    /// The exclusion is structural and silent — a message doc carries no `kind`
    /// field, so the filter's own MUST clause drops it, exactly as it drops
    /// prose in nobody's doc. The coverage block knew three reasons and not this
    /// one, so `kind`-filtered answers claimed `searched: true` while the tool
    /// description tells a caller to trust that field. A field worth reading is
    /// a field that has to be right in every case, not in most of them.
    #[tokio::test]
    async fn a_kind_filter_reports_that_mail_was_left_out() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    kind: Some("person".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            body["mail"]["searched"], false,
            "a kind filter leaves no message in the answer, so it cannot claim it searched them"
        );
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("kind"),
            "…and it says which filter did it, since the caller can drop that one: {note}"
        );

        // The tool description makes the same promise, so it names this case too.
        let tools = Jojobot::tool_router().list_all();
        let description = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool")
            .description
            .as_deref()
            .unwrap_or_default();
        assert!(
            description.contains("kind") && description.contains("mail"),
            "the description tells a caller kind and mail interact: {description}"
        );
    }

    /// **The one claim `search`'s description is not allowed to keep making.**
    /// It used to disclose that mail was unreachable from here; that is now
    /// false, and a description that says so sends a caller to a second verb
    /// that does not exist. Pinned rather than fixed once, because the sentence
    /// is exactly the kind that survives a rewrite by being plausible.
    #[test]
    fn the_search_description_no_longer_says_mail_is_unsearchable() {
        let tools = Jojobot::tool_router().list_all();
        let search = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool");

        // **All three surfaces, not the one that was noticed.** The claim was
        // written down in three places — the tool description, the orientation
        // `start_here`/`boot_bot` hand over, and the server instructions every
        // client loads before it calls anything — and fixing one leaves a
        // session reading either of the others exactly as misinformed as before.
        let instructions = handler().get_info().instructions.unwrap_or_default();
        for (surface, text) in [
            ("the search description", search.description.as_deref().unwrap_or_default()),
            ("the orientation", ORIENTATION),
            ("the server instructions", instructions.as_str()),
        ] {
            for stale in [
                "Messages and mailboxes are not searchable",
                "not searchable here",
                "sees memory only",
                "never messages",
            ] {
                assert!(
                    !text.contains(stale),
                    "{surface} still claims mail is out of reach ({stale:?})"
                );
            }
            assert!(
                text.contains("searchable") || text.contains("include_mail"),
                "{surface} has to say that mail IS reachable — silence reads as the old claim"
            );
        }
        assert!(
            search
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("include_mail"),
            "…and the description has to name the parameter that takes mail back out"
        );
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
            mailbox: None,
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
            mailbox: None,
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

    // --- an identity and the box it owns --------------------------------------

    /// **A bot claims its box through ordinary plumbing.** No special write
    /// verb: `add_entity` carries the claim, the entity comes back wearing it,
    /// and a second identity reaching for the same box is refused with advice
    /// that does NOT send it back with `create_new` — that signal answers a
    /// question about names, and there is no honest answer of that shape to
    /// "someone else already owns this".
    #[tokio::test]
    async fn a_bot_owns_a_mailbox_and_a_rival_claim_is_refused_without_an_override() {
        let jojobot = handler();
        let owner = jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: Some("gamma-inbox".into()),
                ..add_args("bot", "gamma", "Gamma")
            }))
            .await
            .expect("add ok");
        let body = json_of(&owner);
        assert_eq!(body["id"], "bot:gamma");
        assert_eq!(body["type"], "SoftwareApplication");
        assert_eq!(body["mailbox"], "gamma-inbox", "the claim reads back: {body}");

        let result = jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: Some("gamma-inbox".into()),
                create_new: Some(true),
                ..add_args("bot", "delta", "Delta")
            }))
            .await
            .expect("a claimed box is an answer, not a protocol failure");
        let refused = blocked(&result);
        assert_eq!(refused["candidates"][0]["handle"], "bot:gamma");
        assert_eq!(refused["candidates"][0]["reason"], "mailbox-claimed");
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("gamma-inbox") && advice.contains("bot:gamma"),
            "the advice names the box and who holds it: {advice}"
        );
        assert!(
            !advice.contains("create_new"),
            "an override that cannot clear this gate must not be offered: {advice}"
        );
    }

    /// **The two-step walk around, over the wire.** A rival blocked from
    /// claiming a box at creation must not be able to arrive bare and move the
    /// claim on afterwards — and the refusal has to reach the caller as the
    /// blocked envelope naming the owner, not as some other shape. The store
    /// side of this was implemented and tested by nothing: a verifier deleted
    /// the check from both stores and every test stayed green.
    #[tokio::test]
    async fn a_rival_cannot_take_an_owned_box_by_updating_onto_it() {
        let jojobot = handler();
        make_box(&jojobot, "gamma-inbox").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: Some("gamma-inbox".into()),
                ..add_args("bot", "gamma", "Gamma")
            }))
            .await
            .expect("add ok");
        make_bot(&jojobot, "delta", None).await;

        let result = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "bot:delta".into(),
                name: None,
                aliases: None,
                source: None,
                crm: None,
                mailbox: Some("gamma-inbox".into()),
                // The signal that clears a shared name must not clear this.
                create_new: Some(true),
            }))
            .await
            .expect("a claimed box is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "bot:delta");
        assert_eq!(body["candidates"][0]["handle"], "bot:gamma");
        assert_eq!(body["candidates"][0]["reason"], "mailbox-claimed");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("gamma-inbox") && advice.contains("bot:gamma"),
            "the advice names the box and who holds it: {advice}"
        );
        assert!(
            !advice.contains("create_new"),
            "an override that cannot clear this gate must not be offered: {advice}"
        );

        // Nothing moved: the rival is still bare, the owner still owns.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("bot".into()) }))
                .await
                .expect("list ok"),
        );
        let of = |handle: &str| {
            listed["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .find(|e| e["id"] == handle)
                .expect("both bots are listed")
                .clone()
        };
        assert!(of("bot:delta")["mailbox"].is_null(), "got {listed}");
        assert_eq!(of("bot:gamma")["mailbox"], "gamma-inbox");
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
            (EntityKind::Bot, "bot", "SoftwareApplication"),
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
                mailbox: None,
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
            mailbox: None,
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
                mailbox: None,
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
            mailbox: None,
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
                mailbox: None,
                create_new: None,
            }))
            .await
            .expect("an unknown handle is an answer, not a protocol failure");
        let body = blocked(&err);
        assert_eq!(body["attempted"], "thing:red-bikee");
        assert_eq!(
            body["candidates"][0]["handle"], "thing:red-bike",
            "must name the near miss: {body}"
        );
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

    /// **A malformed address and a missed one are different answers**, and
    /// never a new fact. Malformed is the caller writing something that is not
    /// an address at all — a protocol error. Missed is a well-formed address
    /// naming nothing, which is the same "you named what does not exist" every
    /// gate answers, so it wears the blocked shape and carries the addresses
    /// that do exist.
    #[tokio::test]
    async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "the only fact here")).await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                content: Some("nope".into()),
                ..update_args("not-an-address")
            }))
            .await
            .expect_err("a string that is no address is a malformed call");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let missed = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("nope".into()),
                    ..update_args("person:alpha#f99")
                }))
                .await
                .expect("an address that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:alpha#f99");
        let advice = missed["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("person:alpha#f1"),
            "the addresses that DO exist are what makes this repairable: {advice}"
        );
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

        let missed = blocked(
            &jojobot
                .recall(Parameters(RecallArgs { subject: "person:zenit".into() }))
                .await
                .expect("a handle that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:zenit");
        assert_eq!(
            missed["candidates"][0]["handle"], "person:zenith",
            "the near candidate surfaces: {missed}"
        );

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
        with_mailboxes(Arc::new(InMemoryMailboxes::new()))
    }

    /// A handler over a mailbox store the test still holds a typed handle to —
    /// for the states only the store can put itself into.
    fn with_mailboxes(mailboxes: Arc<InMemoryMailboxes>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            mailboxes,
            Arc::new(InMemorySessions::new()),
        )
    }

    async fn make_box(jojobot: &Jojobot, name: &str) -> serde_json::Value {
        let result = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: name.into(),
                create_new: None,
            }))
            .await
            .expect("create_mailbox call ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    async fn send(jojobot: &Jojobot, mailbox: &str, sender: &str, body: &str) -> serde_json::Value {
        send_titled(jojobot, mailbox, sender, None, body).await
    }

    async fn send_titled(
        jojobot: &Jojobot,
        mailbox: &str,
        sender: &str,
        subject: Option<&str>,
        body: &str,
    ) -> serde_json::Value {
        let result = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: mailbox.into(),
                sender: sender.into(),
                subject: subject.map(str::to_string),
                body: body.into(),
                in_reply_to: None,
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
        assert!(
            processed["subject"].is_null(),
            "a message posted without a subject has none, on every verb that renders it"
        );

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

    /// **A subject travels the whole surface.** It goes in on the post and comes
    /// back on the post, the delivery and the archive — a title only the poster
    /// ever sees is not a title.
    #[tokio::test]
    async fn a_subject_is_carried_by_every_verb_that_renders_a_message() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send_titled(
            &jojobot,
            "inbox",
            "alpha",
            Some("the shipment"),
            "it landed at dawn; the crates are by the north door",
        )
        .await;
        assert_eq!(posted["subject"], "the shipment");
        assert_eq!(
            posted["body"], "it landed at dawn; the crates are by the north door",
            "the subject sits beside the body, never carved out of it"
        );
        let id = posted["id"].as_str().expect("an id").to_string();

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs { mailbox: "inbox".into() }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["messages"][0]["subject"], "the shipment");

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs { message_id: id, notes: None }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(processed["subject"], "the shipment", "the archive keeps the title");
    }

    /// **One message, taken by id.** The named message is delivered and the rest
    /// of the box is left where it was — the point of the verb: a session that
    /// wants one filed finding must not have to own everything beside it.
    #[tokio::test]
    async fn read_message_delivers_one_and_leaves_the_box_alone() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let wanted = send(&jojobot, "inbox", "alpha", "the one worth reading").await;
        send(&jojobot, "inbox", "milhouse", "the rest of the box").await;
        let id = wanted["id"].as_str().expect("an id").to_string();

        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs { message_id: id.clone() }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(delivered["id"], id.as_str());
        assert_eq!(delivered["body"], "the one worth reading");
        assert_eq!(delivered["state"], "read", "taking one message moves its column");
        assert_eq!(delivered["seen_before"], false);

        let listed = json_of(&jojobot.list_mailboxes().await.expect("list ok"));
        assert_eq!(listed["mailboxes"][0]["counts"]["read"], 1);
        assert_eq!(
            listed["mailboxes"][0]["counts"]["new"], 1,
            "the rest of the box was not delivered with it"
        );

        // Taken again: a leftover, not a second delivery.
        let again = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs { message_id: id }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(again["seen_before"], true);
    }

    /// **An id that names nothing is blocked, not an error** — the same answer
    /// `mark_processed` gives, so one client branch handles both.
    #[tokio::test]
    async fn reading_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs { message_id: "999999".into() }))
            .await
            .expect("a blocked read is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "999999");
        assert!(
            body["candidates"].as_array().expect("candidates key").is_empty(),
            "nothing resembles a message id: {body}"
        );
    }

    /// A quarantined card addressed by `read_message` gets the quarantine's own
    /// words, not "no such message" — the distinction `mark_processed` draws,
    /// drawn by every verb that addresses a card by id.
    #[tokio::test]
    async fn reading_a_quarantined_card_is_blocked_with_its_own_words() {
        let store = Arc::new(InMemoryMailboxes::new());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId(id.clone()),
            "its description no longer carries a readable machine block",
        );

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs { message_id: id.clone() }))
            .await
            .expect("a quarantined card is a successful, refusing call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], id.as_str());
        let reason = body["reason"].as_str().expect("a quarantined card says why");
        assert!(reason.contains("machine block"), "got {reason}");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("PERSON"),
            "retrying does not help — a person must repair it: {advice}"
        );
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
                subject: None,
                in_reply_to: None,
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

    /// Creating a box that looks like one already there is blocked too — and
    /// its advice names the way out: `create_new`, for the case where the
    /// resemblance is deliberate.
    #[tokio::test]
    async fn creating_a_near_miss_box_is_blocked_with_the_create_new_escape_named() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;

        let result = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: "inbx".into(),
                create_new: None,
            }))
            .await
            .expect("a blocked create is a successful call");
        let body = blocked(&result);
        assert_eq!(body["candidates"][0]["name"], "inbox");
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("create_new"),
            "the way out of this gate is the parameter that opens it: {advice}"
        );
    }

    /// **The operator's escape hatch works end to end.** A sibling box blocked
    /// as a near miss is created on the second, confirmed call — and an exact
    /// name stays blocked however hard the caller confirms.
    #[tokio::test]
    async fn a_deliberate_sibling_box_is_created_with_create_new() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "worker-1").await;

        let refused = json_of(
            &jojobot
                .create_mailbox(Parameters(CreateMailboxArgs {
                    name: "worker-2".into(),
                    create_new: None,
                }))
                .await
                .expect("a blocked create is a successful call"),
        );
        assert_eq!(refused["status"], "blocked", "without the signal: {refused}");

        let created = json_of(
            &jojobot
                .create_mailbox(Parameters(CreateMailboxArgs {
                    name: "worker-2".into(),
                    create_new: Some(true),
                }))
                .await
                .expect("create ok"),
        );
        assert_eq!(created["name"], "worker-2", "the signal creates the sibling: {created}");

        let exact = json_of(
            &jojobot
                .create_mailbox(Parameters(CreateMailboxArgs {
                    name: "worker-1".into(),
                    create_new: Some(true),
                }))
                .await
                .expect("a blocked create is a successful call"),
        );
        assert_eq!(
            exact["status"], "blocked",
            "an exact name is never overridden: {exact}"
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
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: "Inbox".into(),
                create_new: None,
            }))
            .await
            .expect_err("a name outside the grammar must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        make_box(&jojobot, "inbox").await;
        let err = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "inbox".into(),
                sender: "  ".into(),
                body: "the shipment landed".into(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect_err("a message with no sender has no provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// An id nothing answers to is **blocked**, carrying the id that missed —
    /// never a silent success, which would look exactly like a handled message,
    /// **A reply names what it answers, and a dangling link is blocked.** The
    /// hand-off ↔ report chain was correlated by prose convention alone, which
    /// is manual archaeology the moment there is any volume. The link is
    /// optional, carries no semantics beyond itself, and — like every other
    /// reference on this surface — must name something that exists.
    #[tokio::test]
    async fn a_reply_carries_the_message_it_answers_and_a_dangling_link_is_blocked() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        let original = send(&jojobot, "pm", "coordinator (pm)", "build the kiln slice").await;
        let original_id = original["id"].as_str().expect("an id").to_string();
        assert!(original["in_reply_to"].is_null(), "a message answering nothing says so");

        let reply = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "pm".into(),
                    sender: "dev (implementer)".into(),
                    body: "the kiln slice is done".into(),
                    subject: None,
                    in_reply_to: Some(original_id.clone()),
                }))
                .await
                .expect("post ok"),
        );
        assert_eq!(reply["in_reply_to"], original_id.as_str());

        // …and it rides on every verb that renders a message.
        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: reply["id"].as_str().expect("an id").to_string(),
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(delivered["in_reply_to"], original_id.as_str());

        // A link to nothing is the blocked shape, never a protocol error and
        // never a stored message.
        let dangling = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "pm".into(),
                    sender: "dev (implementer)".into(),
                    body: "answering something nobody said".into(),
                    subject: None,
                    in_reply_to: Some("9999".into()),
                }))
                .await
                .expect("a bad reference is an answer, not an error"),
        );
        assert_eq!(dangling["status"], "blocked", "{dangling}");
        assert_eq!(dangling["wrote"], false);
    }

    /// **A long outcome record is cut, and the caller is told it was cut.** The
    /// crash contract asks for an account of what happened; refusing the whole
    /// call over its length left the message unprocessed and cost exactly the
    /// record the cap was policing — which is what it did to a caller in
    /// production. Cutting silently would be the other half of the same
    /// mistake: notes that stop mid-sentence read as a consumer who trailed
    /// off, not a store that ran out of room.
    #[tokio::test]
    async fn a_long_outcome_record_is_cut_and_says_so_rather_than_failing() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let long = "counted the crates and reconciled them against the manifest ".repeat(200);
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some(long.clone()),
                }))
                .await
                .expect("a long note must not fail the terminal verb"),
        );
        assert_eq!(body["state"], "processed", "the message WAS handled: {body}");
        assert_eq!(body["notes_truncated"], true, "…and the cut is said out loud: {body}");
        let kept = body["notes"].as_str().expect("the outcome is recorded");
        assert!(kept.ends_with('…'), "the record itself says it was cut: {kept:?}");
        assert!(kept.chars().count() < long.chars().count());
    }

    /// A record that fits is stored whole and reports no cut — the flag is
    /// always present, so a reader never branches on whether it is there.
    #[tokio::test]
    async fn an_outcome_record_that_fits_reports_no_cut() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: posted["id"].as_str().expect("an id").to_string(),
                    notes: Some("filed under shipments".into()),
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(body["notes"], "filed under shipments");
        assert_eq!(body["notes_truncated"], false, "{body}");
    }

    /// and no longer a protocol error either: naming something that does not
    /// exist is the same kind of answer whichever gate catches it, so it wears
    /// one shape.
    #[tokio::test]
    async fn processing_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "999999".into(),
                notes: None,
            }))
            .await
            .expect("an id that names nothing is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "999999");
        assert!(
            body["candidates"].as_array().is_some_and(|c| c.is_empty()),
            "nothing resembles a message id: {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("read_mailbox"),
            "the way out is a delivery that hands back real ids: {advice}"
        );
    }

    /// **A quarantined card is visible on the wire, and it is not a count of
    /// zero.** A card jojobot cannot read is invisible to every other verb —
    /// not counted, not delivered, not processable — so this field is the only
    /// place a caller learns it exists at all. Rendering it wrong reads as an
    /// empty, healthy box.
    #[tokio::test]
    async fn a_quarantined_card_is_rendered_with_its_count_and_its_ids() {
        let store = Arc::new(InMemoryMailboxes::new());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "alpha", "the shipment landed").await;
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its description no longer carries a readable machine block",
        );

        let listed = json_of(&jojobot.list_mailboxes().await.expect("list ok"));
        let inbox = &listed["mailboxes"][0];
        assert_eq!(inbox["quarantined"]["count"], 1, "got {listed}");
        assert_eq!(inbox["quarantined"]["card_ids"][0], "4212");
        assert_eq!(
            inbox["counts"]["total"], 1,
            "a quarantined card is not a message and is never counted as one: {listed}"
        );
    }

    /// **`mark_processed` on a quarantined id says so.** Answering "no message
    /// with that id" — for an id `list_mailboxes` published one call ago — is a
    /// false statement about jojobot's own output, and it sends the caller
    /// hunting for a lost message instead of at the card sitting on the board.
    /// The answer takes the blocked shape the guards use, so one client-side
    /// branch handles every "declined, here is what to do" in this context.
    #[tokio::test]
    async fn processing_a_quarantined_card_is_blocked_with_its_own_words() {
        let store = Arc::new(InMemoryMailboxes::new());
        let jojobot = with_mailboxes(store.clone());
        make_box(&jojobot, "inbox").await;
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its description no longer carries a readable machine block",
        );

        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "4212".into(),
                notes: Some("filed".into()),
            }))
            .await
            .expect("a quarantined card is a structured answer, not a protocol error");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "4212");
        assert_eq!(body["wrote"], false);
        let reason = body["reason"].as_str().expect("a reason");
        assert!(
            reason.contains("machine block"),
            "the answer says why this card cannot be read: {reason}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("4212") && advice.contains("PERSON"),
            "…and that the way out is a human on the board, not a retry: {advice}"
        );

        // Both wear the blocked shape now — but they are still different
        // answers, and the difference is the one that matters: a quarantined
        // card is a real card no retry can reach, while an unknown id names
        // nothing at all.
        let unknown = blocked(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: "999999".into(),
                    notes: None,
                }))
                .await
                .expect("an id nothing answers to is still an answer"),
        );
        assert!(
            unknown["reason"].is_null(),
            "there is no card to explain — that field belongs to the quarantine answer: {unknown}"
        );
        assert!(
            !unknown["how_to_proceed"].as_str().expect("advice").contains("PERSON"),
            "and its way out is not a human on the board: {unknown}"
        );
    }

    /// **The whole tool surface, named.** Production jojobot never deletes
    /// anything: the standing rule is structural at the store (the Mailboxes
    /// port has no delete operation at all), and this pins the other end — that
    /// nothing at all reaches a client except these.
    ///
    /// **The exact list, not a filter and a list of forbidden words.** A
    /// name-shape filter only sees the tools it thought to look for, and a
    /// denylist only catches the wordings somebody guessed: `retire_message`,
    /// `archive_box`, `clear_mailbox` all sail past both while doing the thing
    /// the rule exists to forbid. Adding a tool here is a line in this list and
    /// a reviewer reading it — which is the whole point.
    #[test]
    fn the_tool_surface_is_exactly_this_list() {
        let tools = Jojobot::tool_router().list_all();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort_unstable();

        // Sorted, so the list is stable and a diff to it is legible — which
        // means it is NOT grouped by context, and any comment here claiming
        // otherwise would be describing a different list than the one below.
        // The six mailbox verbs in it are create_mailbox, list_mailboxes,
        // post_message, read_mailbox, read_message and mark_processed; the
        // three session verbs are journal, amend_journal and wrap_session
        // (there is deliberately no start_session — booting an identity IS
        // starting its session); the rest are Memory's.
        assert_eq!(
            names,
            [
                "add_entity",
                "amend_journal",
                "boot_bot",
                "capture",
                "create_mailbox",
                "journal",
                "list_entities",
                "list_mailboxes",
                "mark_processed",
                "ping",
                "post_message",
                "read_mailbox",
                "read_message",
                "recall",
                "search",
                "set_charter",
                "start_here",
                "update_entity",
                "update_fact",
                "wrap_session",
            ],
            "the tool surface changed — if that was deliberate, say so here"
        );
    }

    /// **Every verb whose miss is blocked says so where a caller reads it.**
    ///
    /// A description that promises an error for a miss is worse than one that
    /// says nothing: a client written against it branches on the wrong thing
    /// and handles the answer exactly wrong. The unification rider fixed four
    /// of these descriptions and missed `set_charter`, which went on promising
    /// "an error naming the nearest handles" while the code returned blocked —
    /// so the whole class is pinned here rather than one more instance of it.
    #[test]
    fn the_verbs_whose_misses_are_blocked_all_say_so() {
        let tools = Jojobot::tool_router().list_all();
        for name in [
            "recall",
            "update_entity",
            "update_fact",
            "mark_processed",
            "journal",
            "amend_journal",
            "wrap_session",
            "read_message",
            "set_charter",
            "boot_bot",
        ] {
            let tool = tools
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{name} is a tool"));
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(
                description.contains("blocked"),
                "{name} must tell a caller its miss is a blocked result: {description}"
            );
            assert!(
                !description.contains("is an error"),
                "{name} still promises an error for a miss it no longer errors on: {description}"
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
        // **…and it must not read as forbidding the ack.** "Act first" made a
        // real session hesitate over pure acknowledgements, where reading IS
        // the acting. The rule and its one boundary case travel together.
        assert!(
            description.contains("acknowledgement"),
            "the crash contract must say where reading is itself the acting: {description}"
        );
    }

    /// **Polling is a read, and the surface has to say which verb reads.** A
    /// session whose standing loop was "check the box; if empty do nothing" paid
    /// ~14 state-changing deliveries of an empty box, because the only verb that
    /// visibly answers "is there anything waiting" is the one that takes
    /// delivery. `list_mailboxes` was the answer the whole time and nothing
    /// pointed at it from the place the caller was standing.
    #[test]
    fn the_read_mailbox_description_points_at_the_read_only_way_to_poll() {
        let tools = Jojobot::tool_router().list_all();
        let read = tools
            .iter()
            .find(|t| t.name == "read_mailbox")
            .expect("read_mailbox is a tool");
        let description = read.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("list_mailboxes"),
            "the cheaper verb must be named where the expensive one is read: {description}"
        );
    }

    // ── start_here ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn start_here_lands_a_fresh_agent_with_the_world_and_a_snapshot() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                kind: "person".into(),
                handle: "milhouse".into(),
                name: "Milhouse".into(),
                aliases: None,
                source: "user-named".into(),
                crm: None,
                mailbox: None,
                boot: None,
                create_new: None,
            }))
            .await
            .expect("entity ok");
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "alpha", "the shipment landed").await;

        let out = jojobot.start_here().await.expect("start_here ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        let orientation = body["orientation"].as_str().expect("orientation prose");
        // The orientation must teach the load-bearing vocabulary, not assume it.
        for taught in [
            "entity",
            "fact",
            "testimony",
            "inference",
            "edge",
            "mailbox",
            "processed",
            "search",
            "blocked",
            // The norms the box-minting review added (2026-07-26): a mailbox is
            // a channel someone drains, never minted mid-errand; changed claims
            // supersede rather than overwrite; ambiguity goes to the operator.
            "drain",
            "superseded",
            "ask the operator",
            // M4: an identity is a thing a session can be, and the orientation
            // has to say what one is made of before boot_bot hands one over.
            "bot",
            "charter",
        ] {
            assert!(
                orientation.contains(taught),
                "the orientation never teaches `{taught}`"
            );
        }
        assert_eq!(body["snapshot"]["entities"]["count"], 1);
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["person"], 1);
        let boxes = body["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("mailboxes listed");
        assert_eq!(boxes[0]["name"], "inbox");
        assert_eq!(boxes[0]["counts"]["new"], 1);
    }

    /// A mailbox world that answers nothing. Shared by both orientation doors:
    /// they make the same promise, so they are held to it by the same double.
    struct DownMailboxes;

    #[async_trait]
    impl mailbox::Mailboxes for DownMailboxes {
            async fn create_mailbox(
                &self,
                _: &mailbox::MailboxName,
                _: bool,
            ) -> Result<mailbox::Guarded<mailbox::Mailbox>, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn list_mailboxes(
                &self,
            ) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn post_message(
                &self,
                _: mailbox::NewMessage,
            ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn read_mailbox(
                &self,
                _: &mailbox::MailboxName,
            ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn read_message(
                &self,
                _: &mailbox::MessageId,
            ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
                Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
            }
            async fn mark_processed(
                &self,
                _: &mailbox::MessageId,
                _: Option<&str>,
        ) -> Result<mailbox::Message, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured("the mailbox world is down".into()))
        }
    }

    /// A handler whose mailbox world answers nothing, over a memory the caller
    /// may already have populated — a bot has to be stood up while the world is
    /// up, since a claim that cannot be screened is refused.
    fn handler_with_mailboxes_down(memory: Arc<InMemoryMemory>) -> Jojobot {
        Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(DownMailboxes),
            Arc::new(InMemorySessions::new()),
        )
    }

    /// One world being down must not take orientation with it: a fresh agent
    /// on a half-configured server still deserves the map.
    #[tokio::test]
    async fn start_here_survives_a_world_that_is_down() {
        let out = handler_with_mailboxes_down(Arc::new(InMemoryMemory::new()))
            .start_here()
            .await
            .expect("orientation still lands");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert!(body["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(body["snapshot"]["mailboxes"]["available"], false);
    }

    // ── boot_bot ────────────────────────────────────────────────────────────

    /// Stand up a bot the way an operator would: an entity of kind `bot`
    /// claiming a box, its charter as prose, its rules as facts.
    async fn make_bot(jojobot: &Jojobot, slug: &str, mailbox: Option<&str>) {
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: mailbox.map(str::to_string),
                ..add_args("bot", slug, slug)
            }))
            .await
            .expect("add_entity ok");
    }

    async fn boot(jojobot: &Jojobot, name: &str) -> serde_json::Value {
        json_of(
            &jojobot
                .boot_bot(Parameters(BootBotArgs { name: name.into() }))
                .await
                .expect("boot_bot call ok"),
        )
    }

    // ── sessions ────────────────────────────────────────────────────────────

    /// A handler over a session store the test still holds a typed handle to.
    fn with_sessions(sessions: Arc<InMemorySessions>) -> Jojobot {
        connection(Arc::new(InMemoryMemory::new()), sessions)
    }

    /// A second connection to the same worlds — what a reconnect or a device hop
    /// builds. The binding is per handler, so this is the only way to test that
    /// resuming reads the board rather than remembering anything.
    fn connection(memory: Arc<InMemoryMemory>, sessions: Arc<InMemorySessions>) -> Jojobot {
        Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            sessions,
        )
    }

    async fn journal_entry(jojobot: &Jojobot, entry: &str) -> serde_json::Value {
        let result = jojobot
            .journal(Parameters(JournalArgs {
                entry: entry.into(),
                focus: None,
                session: None,
            }))
            .await
            .expect("journal call ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    /// **The golden: every byte the derived focus has ever been given.** A focus
    /// is stored in a session card's description on a live board, so this
    /// strategy's output is the product. Recorded literally so the shared text
    /// engine underneath can only pass by producing the same bytes.
    ///
    /// This is the one strategy that strips: a focus rides above a fenced
    /// machine block, so a backtick in it can close the fence, and it has an
    /// empty fallback because a card with a blank description says nothing.
    #[test]
    fn the_focus_line_golden() {
        let w200 = "w".repeat(200);
        let w199 = "w".repeat(199);
        let words = format!("{} tail", "word ".repeat(45));
        let x400 = "x".repeat(400);
        let cases: [(&str, String); 9] = [
            ("short one", "short one".into()),
            (
                "read the hand-off\n\nthen scoped the slice",
                "read the hand-off then scoped the slice".into(),
            ),
            (
                "started on `working_session`, which was the wrong shape",
                "started on working_session, which was the wrong shape".into(),
            ),
            (&w200, w200.clone()),
            (&w199, w199.clone()),
            (&words, format!("{}word…", "word ".repeat(39))),
            (&x400, format!("{}…", "x".repeat(199))),
            ("   ", "working".into()),
            ("bell\u{7}char", "bellchar".into()),
        ];
        for (input, expected) in cases {
            assert_eq!(display_line(input), expected, "the stored focus changed for {input:?}");
        }
    }

    /// **A boot that does nothing leaves nothing behind.** The card materializes
    /// on the first write and never before, which is what keeps "creation is an
    /// intentional act" true for the one verb whose job is to start something.
    #[tokio::test]
    async fn booting_writes_no_session_card_until_the_first_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["available"], true);
        assert_eq!(booted["session"]["resumed"], false, "nothing was in flight");
        assert!(booted["session"]["session"].is_null(), "…and no card was written");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a boot that never works must leave no card at all"
        );

        // The first beat is what brings it into being.
        let journalled = journal_entry(&jojobot, "read the hand-off").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "the first entry materializes the card");
        assert_eq!(journalled["session"], live[0].id.as_str());
        assert_eq!(live[0].entries.len(), 1);
        assert_eq!(live[0].entries[0].text, "read the hand-off");
        assert_eq!(
            live[0].focus, "read the hand-off",
            "with nothing else to go on, what it first recorded is what it is doing"
        );
    }

    /// **THE BLOCKER: a first write is prose, and prose is not a focus.** The
    /// card materializes with a focus derived from the entry, so the focus's
    /// rules — one line, 200 characters, no backtick — were being applied to
    /// text nobody offered as a focus. A multi-line entry, a long story, or a
    /// one-liner naming code in backticks failed with `invalid entry` naming a
    /// `focus` parameter the caller never passed; the entry was dropped and no
    /// card appeared at all.
    ///
    /// The entry reaches the chronology **whole**. The focus is a glance, so it
    /// is derived: flattened, cut, and stripped of what a one-line display field
    /// cannot carry.
    #[tokio::test]
    async fn a_first_entry_is_prose_and_still_lands_whole() {
        let backticked = "started on `working_session`, which was the wrong shape";
        let long = "x".repeat(400);
        let cut = format!("{}…", "x".repeat(199));
        // The derived focus in full, not just its shape — a flatten that joined
        // with nothing would glue the words either side of a paragraph break
        // into one, and every rule-shaped assertion (no newline, no backtick,
        // within the cap) still holds of the glued line.
        let cases: [(&str, &str, &str); 3] = [
            (
                "multi-line",
                "read the hand-off\n\nthen scoped the slice",
                "read the hand-off then scoped the slice",
            ),
            (
                "backticked",
                backticked,
                "started on working_session, which was the wrong shape",
            ),
            ("over-long", &long, &cut),
        ];
        for (shape, entry, focus) in cases {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = with_sessions(store.clone());
            make_bot(&jojobot, "gamma", None).await;
            boot(&jojobot, "gamma").await;

            let body = json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: entry.into(),
                        focus: None,
                        session: None,
                    }))
                    .await
                    .unwrap_or_else(|e| panic!("a {shape} first entry must not error: {e:?}")),
            );
            assert_ne!(body["status"], "blocked", "{shape}: {body}");

            let live = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            assert_eq!(live.len(), 1, "{shape}: the card must materialize");
            assert_eq!(
                live[0].entries[0].text,
                jojobot_domain::session::normalize_entry(entry),
                "{shape}: the entry reaches the chronology whole"
            );
            assert_eq!(
                live[0].focus, focus,
                "{shape}: the derived focus is display text, word for word"
            );
            assert!(
                live[0].focus.chars().count() <= 200,
                "{shape}: …and it is cut to fit: {:?}",
                live[0].focus
            );
        }
    }

    /// **A wrap as a first write is the same bug, and it is always prose.** A
    /// story written for somebody with none of your context is never one short
    /// line, so this path was broken for every caller who wrapped without
    /// journalling first.
    #[tokio::test]
    async fn a_wrap_can_be_a_first_write_and_the_story_is_prose() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        let story = "read the hand-off and found nothing to do.\n\nWrapping without a beat: the \
                     `dev` box was empty and there was no slice to build.";
        let body = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    session: None,
                }))
                .await
                .expect("a wrap as a first write must not error"),
        );
        assert_eq!(body["session"]["state"], "wrapped");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1);
        assert_eq!(
            live[0].entries[0].text,
            jojobot_domain::session::normalize_entry(story),
            "the story is the record — it must not be cut to fit a display field"
        );
    }

    /// A focus the caller passed IS validated as a focus — the rules were never
    /// wrong, only misapplied. Its refusal names the parameter they actually
    /// sent.
    #[tokio::test]
    async fn an_explicit_focus_is_still_held_to_the_focus_rules() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        let err = jojobot
            .journal(Parameters(JournalArgs {
                entry: "read the hand-off".into(),
                focus: Some("two\nlines".into()),
                session: None,
            }))
            .await
            .expect_err("a focus that is not one line must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// **A reconnect resumes the work in flight.** A session is the unit of
    /// work, not of connection, so a second boot of the same identity attaches
    /// to the live session and hands back its chronology rather than forking a
    /// new one — which is the whole reason a device hop is survivable.
    #[tokio::test]
    async fn booting_again_resumes_the_session_in_flight() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let first = connection(memory.clone(), store.clone());
        make_bot(&first, "gamma", None).await;
        boot(&first, "gamma").await;
        let started = journal_entry(&first, "read the hand-off").await;

        // A different connection over the same worlds, exactly as a reconnect
        // builds one — a fresh binding, so anything it knows it read.
        let second = connection(memory, store.clone());
        let resumed = boot(&second, "gamma").await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], started["session"]);
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"], "read the hand-off",
            "the work in flight comes back with it: {resumed}"
        );

        // …and writing on the new connection continues the same session.
        journal_entry(&second, "picked it back up").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one session, not two: {live:?}");
        assert_eq!(live[0].entries.len(), 2);
    }

    /// **The sweep, and what it is measured from.** A session that has gone a
    /// day without a beat is closed as `abandoned` at the next boot of its bot —
    /// never deleted, never wrapped, because its story was never told. A fresh
    /// session begins beside it rather than resuming it.
    #[tokio::test]
    async fn a_stale_session_is_swept_to_abandoned_at_the_next_boot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        // Begun two days ago and never touched since.
        let stale = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                focus: "something from the day before yesterday".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(
            booted["session"]["swept"],
            serde_json::json!([stale.id.as_str()]),
            "the boot says what it closed: {booted}"
        );
        assert_eq!(booted["session"]["resumed"], false, "a swept session is not resumed");

        let read = store.read_session(&stale.id).await.expect("read ok");
        assert_eq!(read.state, mailbox_state_abandoned(), "closed, not deleted");
        assert_eq!(
            read.focus, "something from the day before yesterday",
            "…and its record is untouched"
        );
    }

    /// A session that is merely quiet — an hour, not a day — is still yours, and
    /// resuming it is the point.
    #[tokio::test]
    async fn a_recent_session_is_resumed_rather_than_swept() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let recent = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1),
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["resumed"], true);
        assert_eq!(booted["session"]["session"]["id"], recent.id.as_str());
        assert_eq!(booted["session"]["swept"], serde_json::json!([]));
    }

    /// **The whole arc through the surface:** boot, journal with a focus, amend
    /// the beat, wrap. The focus is current truth and the chronology is history,
    /// and the wrap writes the story to both the session and the Journal.
    #[tokio::test]
    async fn the_session_arc_through_the_handler() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: Some("building the session context".into()),
                    session: None,
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(first["focus"], "building the session context");
        assert!(first["entry"]["beat"].is_null(), "a session's own entry is not a beat");

        let amended = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off and scoped the slice properly".into(),
                    session: None,
                }))
                .await
                .expect("amend ok"),
        );
        assert_eq!(amended["entry"]["id"], first["entry"]["id"], "in place");

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the session context; the sweep is lazy until M8".into(),
                    session: None,
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["session"]["state"], "wrapped");
        assert!(
            wrapped["journal"]
                .as_str()
                .expect("the Journal entry as stored")
                .contains("built the session context"),
            "the story goes through to the operator's Journal: {wrapped}"
        );

        let read = store
            .read_session(&SessionId(
                first["session"].as_str().expect("a session id").to_string(),
            ))
            .await
            .expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "read the hand-off and scoped the slice properly",
                "built the session context; the sweep is lazy until M8",
            ],
            "two entries: the amended one and the story"
        );
    }

    /// **Wrapped is terminal both ways, through the surface.** Every session
    /// verb on a closed id comes back blocked, in the guards' one shape.
    #[tokio::test]
    async fn a_wrapped_session_refuses_every_further_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;
        let started = journal_entry(&jojobot, "read the hand-off").await;
        let id = started["session"].as_str().expect("a session id").to_string();
        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done".into(),
                session: None,
            }))
            .await
            .expect("wrap ok");

        let refused = |body: serde_json::Value, verb: &str| {
            assert_eq!(body["status"], "blocked", "{verb} must be blocked: {body}");
            assert_eq!(body["wrote"], false);
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("terminal both ways"),
                "{verb} has to say why: {how}"
            );
        };
        refused(
            json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: "one more thing".into(),
                        focus: None,
                        session: Some(id.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "journal",
        );
        refused(
            json_of(
                &jojobot
                    .amend_journal(Parameters(AmendJournalArgs {
                        entry: "actually".into(),
                        session: Some(id.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "amend_journal",
        );
        refused(
            json_of(
                &jojobot
                    .wrap_session(Parameters(WrapSessionArgs {
                        story: "done again".into(),
                        session: Some(id.clone()),
                    }))
                    .await
                    .expect("call ok"),
            ),
            "wrap_session",
        );
    }

    /// A session verb on a connection that never booted is blocked with the way
    /// forward — jojobot will not guess which identity made the call.
    #[tokio::test]
    async fn a_session_verb_without_a_boot_is_blocked_with_the_way_forward() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "who am i".into(),
                    focus: None,
                    session: None,
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(how.contains("boot_bot"), "the way out names the verb: {how}");
    }

    /// **Amending a session that has not begun is refused, not turned into a
    /// first entry.** A correction that silently became an append leaves a
    /// chronology saying something nobody meant.
    #[tokio::test]
    async fn amending_before_the_first_entry_is_blocked_and_writes_nothing() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "there is nothing to correct".into(),
                    session: None,
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "…and it did not mint a session to hold the correction"
        );
    }

    /// **One beat per verb class, its count kept current.** jojobot's own
    /// footnotes are a tally, not a log: the second capture corrects the first
    /// beat rather than adding one, and they stay marked apart from what the
    /// session said about itself.
    #[tokio::test]
    async fn jojobot_writes_one_beat_per_verb_class_and_keeps_its_count() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;
        capture_ok(&jojobot, capture_args("alpha", "plays go")).await;
        capture_ok(&jojobot, capture_args("milhouse", "plays chess")).await;
        journal_entry(&jojobot, "captured a couple of things").await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let entries = &live[0].entries;
        let beats: Vec<(&str, &str)> = entries
            .iter()
            .filter_map(|e| e.beat.as_deref().map(|b| (b, e.text.as_str())))
            .collect();
        assert_eq!(
            beats.iter().filter(|(class, _)| *class == "capture").count(),
            1,
            "one beat for the class, however many captures: {entries:?}"
        );
        let (_, tally) = beats
            .iter()
            .find(|(class, _)| *class == "capture")
            .expect("a capture beat");
        assert!(tally.contains("(2)"), "…with its count kept current: {tally}");
        assert!(tally.contains("person:alpha"), "…and what it touched: {tally}");
        assert!(tally.contains("person:milhouse"), "…both of them: {tally}");

        // The classes stay apart, and so do jojobot's words and the session's.
        assert!(
            beats.iter().any(|(class, _)| *class == "add_entity"),
            "a different verb class is a different beat: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| !e.is_auto() && e.text == "captured a couple of things"),
            "the session's own entry is not a beat: {entries:?}"
        );
    }

    /// **The tally belongs to the session, not to the connection.** Resuming
    /// rebuilt an empty beat map, so the first verb of each class after every
    /// reconnect appended a SECOND beat for that class — and a reconnect is the
    /// headline case this milestone exists for, so the duplicate would have been
    /// the normal shape rather than the rare one.
    ///
    /// The chronology already says which class each beat is about, so the tally
    /// is re-derivable: attaching reads it back off the entries.
    #[tokio::test]
    async fn the_beat_tally_survives_a_reconnect() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let first = connection(memory.clone(), store.clone());
        make_bot(&first, "gamma", None).await;
        boot(&first, "gamma").await;
        ensure(&first, "alpha").await;
        capture_ok(&first, capture_args("alpha", "plays go")).await;

        // A reconnect, then another capture.
        let second = connection(memory, store.clone());
        boot(&second, "gamma").await;
        ensure(&second, "milhouse").await;
        capture_ok(&second, capture_args("milhouse", "plays chess")).await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        let captures: Vec<&str> = live[0]
            .entries
            .iter()
            .filter(|e| e.beat.as_deref() == Some("capture"))
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            captures.len(),
            1,
            "one beat for the class across both connections: {:?}",
            live[0].entries
        );
        assert!(
            captures[0].contains("(2)"),
            "…and the count carried across the reconnect: {}",
            captures[0]
        );
        assert!(
            captures[0].contains("person:alpha") && captures[0].contains("person:milhouse"),
            "…along with what it touched on both sides: {}",
            captures[0]
        );
    }

    /// **A beat line a person rewrote stays theirs, and the class starts over
    /// beside it.** jojobot reads its tally back out of the line it rendered, so
    /// a hand-edited one no longer parses — and the deliberate answer is to
    /// leave their words alone and open a fresh tally rather than overwrite what
    /// somebody wrote on the card. The cost is the one case where a session
    /// carries two beat lines of one class, which is why the rule is "at most
    /// one per class that jojobot itself is still keeping".
    #[tokio::test]
    async fn a_hand_edited_beat_is_left_alone_and_the_class_starts_a_fresh_tally() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let first = connection(memory.clone(), store.clone());
        make_bot(&first, "gamma", None).await;
        boot(&first, "gamma").await;
        ensure(&first, "alpha").await;
        capture_ok(&first, capture_args("alpha", "plays go")).await;

        let live = store.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        let beat = live[0]
            .entries
            .iter()
            .find(|e| e.beat.as_deref() == Some("capture"))
            .expect("a capture beat")
            .clone();

        // Somebody edits that comment on the board, in their own words.
        let theirs = "I checked these myself — they are right";
        store
            .amend_beat(&live[0].id, &beat.id, theirs, jiff::Timestamp::now())
            .await
            .expect("amend ok");

        // A reconnect: the tally is re-read off the chronology, and this line no
        // longer says anything jojobot can count.
        let second = connection(memory, store.clone());
        boot(&second, "gamma").await;
        ensure(&second, "milhouse").await;
        capture_ok(&second, capture_args("milhouse", "plays chess")).await;

        let live = store.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        let captures: Vec<&str> = live[0]
            .entries
            .iter()
            .filter(|e| e.beat.as_deref() == Some("capture"))
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            captures,
            vec![theirs, "captured facts about: person:milhouse (1)"],
            "their line untouched, and a fresh tally beside it: {:?}",
            live[0].entries
        );
    }

    /// Every doc's prose on one string — how a test reads the operator's
    /// Journal, which is a page rather than an entity and so has no handle to
    /// fetch it by.
    async fn journal_prose(memory: &InMemoryMemory) -> String {
        memory
            .scan()
            .await
            .expect("scan ok")
            .into_iter()
            .map(|d| d.prose)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A board that cannot be listed until it can — an outage that ends, which
    /// is the whole reason a boot binds without attaching instead of giving up.
    struct BoardComesBack {
        inner: Arc<InMemorySessions>,
        down: std::sync::atomic::AtomicBool,
    }

    impl BoardComesBack {
        fn new(inner: Arc<InMemorySessions>) -> Self {
            BoardComesBack {
                inner,
                down: std::sync::atomic::AtomicBool::new(true),
            }
        }
        fn comes_back(&self) {
            self.down.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Sessions for BoardComesBack {
        async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
            if self.down.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(SessionError::Store("the board cannot be listed".into()));
            }
            self.inner.sessions_of(bot).await
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.inner.read_session(id).await
        }
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.inner.begin(new).await
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.inner.append(id, entry).await
        }
        async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
            self.inner.amend_last(id, text).await
        }
        async fn amend_beat(
            &self,
            id: &SessionId,
            entry: &EntryId,
            text: &str,
            at: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            self.inner.amend_beat(id, entry, text, at).await
        }
        async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
            self.inner.set_focus(id, focus).await
        }
        async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
            self.inner.close(id, to).await
        }
    }

    /// The bot, its session already in flight, and a connection that booted
    /// while the board was unreadable — the state the whole `attached` flag
    /// exists to represent.
    async fn booted_blind(store: &Arc<BoardComesBack>) -> (Jojobot, Session) {
        let live = store
            .inner
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                focus: "in flight, and unreadable".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .inner
            .append(&live.id, NewEntry::manual("what it was doing", jiff::Timestamp::now()))
            .await
            .expect("append ok");

        let jojobot = Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            store.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;
        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(
            booted["session"]["available"], false,
            "the board was unreadable, so nothing was attached to: {booted}"
        );
        (jojobot, live)
    }

    /// **The first write RETRIES the attach.** A boot that could not read the
    /// board binds without attaching precisely so the next write can look again
    /// — and when the board is back, that write joins the session already in
    /// flight instead of beginning a second one beside it.
    #[tokio::test]
    async fn a_first_write_after_a_blind_boot_attaches_rather_than_beginning() {
        let store = Arc::new(BoardComesBack::new(Arc::new(InMemorySessions::new())));
        let (jojobot, live) = booted_blind(&store).await;

        store.comes_back();
        let journalled = journal_entry(&jojobot, "picked it back up").await;
        assert_eq!(
            journalled["session"], live.id.as_str(),
            "the write joined the session in flight: {journalled}"
        );

        let all = store.inner.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        assert_eq!(all.len(), 1, "no second card beside it: {all:?}");
        assert_eq!(all[0].entries.len(), 2, "…and it kept accruing: {:?}", all[0].entries);
    }

    /// **amend_journal triages the same way the other two do.** A connection
    /// that never booted is told to boot — not told there is nothing to amend,
    /// which is a different fact about a different thing.
    #[tokio::test]
    async fn amending_without_a_boot_says_to_boot_rather_than_no_entries() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "actually, it was the other thing".into(),
                    session: None,
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(how.contains("boot_bot"), "the way out names the verb: {how}");
        assert!(
            !how.contains("no entries"),
            "…and it does not answer about a session nobody looked for: {how}"
        );
    }

    /// …and a connection whose boot could not read the board retries the attach
    /// here too, rather than answering "no entries" about a session it never
    /// looked for. Unknown is not false.
    #[tokio::test]
    async fn amending_after_a_blind_boot_retries_the_attach() {
        let store = Arc::new(BoardComesBack::new(Arc::new(InMemorySessions::new())));
        let (jojobot, live) = booted_blind(&store).await;

        store.comes_back();
        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "what it was doing, said better".into(),
                    session: None,
                }))
                .await
                .expect("call ok"),
        );
        assert_ne!(body["status"], "blocked", "the session was there to be found: {body}");
        assert_eq!(body["session"], live.id.as_str());

        let read = store.inner.read_session(&live.id).await.expect("read ok");
        assert_eq!(read.entries.len(), 1, "amended in place, not appended");
        assert_eq!(read.entries[0].text, "what it was doing, said better");
    }

    /// A board that cannot be listed, though everything else on it works — the
    /// shape one endpoint being down actually takes, and the one that matters
    /// here: a boot cannot see what is in flight, but `begin` would happily mint
    /// a card beside it.
    struct UnlistableBoard(Arc<InMemorySessions>);

    #[async_trait]
    impl Sessions for UnlistableBoard {
        async fn sessions_of(&self, _: &EntityId) -> Result<Vec<Session>, SessionError> {
            Err(SessionError::Store("the board cannot be listed".into()))
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.0.read_session(id).await
        }
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.0.begin(new).await
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.0.append(id, entry).await
        }
        async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
            self.0.amend_last(id, text).await
        }
        async fn amend_beat(
            &self,
            id: &SessionId,
            entry: &EntryId,
            text: &str,
            at: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            self.0.amend_beat(id, entry, text, at).await
        }
        async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
            self.0.set_focus(id, focus).await
        }
        async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
            self.0.close(id, to).await
        }
    }

    /// **A retry that FAILED is not a retry that found nothing.** The boot binds
    /// without attaching when it cannot read the board, so the first write tries
    /// again — and when that try errored too, the code fell straight through to
    /// beginning a session. Deterministically, not as a race: the outage forks
    /// the record every time, minting a second card for a bot whose real session
    /// is in flight and unreadable. Unknown is not "nothing there".
    #[tokio::test]
    async fn a_failed_attach_retry_answers_the_outage_rather_than_beginning() {
        let inner = Arc::new(InMemorySessions::new());
        // The session actually running — exactly what the boot cannot see.
        let live = inner
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                focus: "in flight, and unreadable".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        let jojobot = Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(UnlistableBoard(inner.clone())),
        );
        make_bot(&jojobot, "gamma", None).await;
        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(
            booted["session"]["available"], false,
            "the board was unreadable, so the boot attached to nothing: {booted}"
        );

        // The first write retries the attach. The board is still down.
        let err = jojobot
            .journal(Parameters(JournalArgs {
                entry: "the first beat of the reconnect".into(),
                focus: None,
                session: None,
            }))
            .await
            .expect_err("a write against an unreadable board must say so");
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);

        let all = inner.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        assert_eq!(all.len(), 1, "no second card was minted beside the live one: {all:?}");
        assert_eq!(all[0].id, live.id);
    }

    /// **The Journal guard is scoped to the SESSION, never to the page.** It
    /// asked whether the whole Journal — every dated entry, every bot, every
    /// session there has ever been — contained the story as a substring, and
    /// skipped the write when it did. So a session whose story matched anything
    /// already written had that story silently dropped while its wrap reported
    /// success: the ordinary repeat loop, the short story, the second run of the
    /// same work. That is the exact failure the guard trades a duplicate to
    /// avoid, arriving through the guard itself.
    #[tokio::test]
    async fn two_sessions_telling_the_same_story_both_reach_the_journal() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let story = "read the hand-off, found nothing to do, wrapped";

        for bot in ["gamma", "delta"] {
            let jojobot = connection(memory.clone(), store.clone());
            make_bot(&jojobot, bot, None).await;
            boot(&jojobot, bot).await;
            jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    session: None,
                }))
                .await
                .expect("wrap ok");
        }

        let journal = journal_prose(&memory).await;
        assert_eq!(
            journal.matches(story).count(),
            2,
            "both sessions told their story, so both entries belong on the page: {journal}"
        );
    }

    /// **The mark is a LINE of the page, never a substring of it.** The guard
    /// answers one question — has THIS session told its story — and a page that
    /// happens to carry the literal mark inside somebody else's sentence
    /// answered yes to it: an entry that quotes one, the operator's own
    /// handwriting. The wrap then wrote nothing and reported `wrapped`, which is
    /// the silent drop the scoping exists to kill, arriving through the scoping.
    #[tokio::test]
    async fn a_mark_inside_foreign_prose_is_not_this_session_s_entry() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;
        let started = journal_entry(&jojobot, "read the hand-off").await;
        let session = started["session"].as_str().expect("a session id").to_string();

        // An entry already on the page that mentions this session's mark in
        // passing — its own line, so nothing but a substring match sees it.
        memory
            .append_journal(
                jiff::civil::date(2026, 7, 26),
                &format!("picked up where [session {session}] left off, and stopped there"),
            )
            .await
            .expect("append_journal ok");

        let story = "built the thing, then told the story";
        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                session: None,
            }))
            .await
            .expect("wrap ok");

        let journal = journal_prose(&memory).await;
        assert!(
            journal.contains(story),
            "this session had told nobody anything, so its story belongs on the page: {journal}"
        );
    }

    /// **Two sessions whose ids share a prefix are two sessions.** `[session 1]`
    /// and `[session 12]` are one bracket apart, and the shorter one wrapping
    /// second must not read the longer one's entry as its own.
    #[tokio::test]
    async fn a_session_whose_id_prefixes_another_still_tells_its_story() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());

        // Ids are minted in sequence, so twelve of them yield a pair where one
        // is a prefix of the other.
        let mut ids = Vec::new();
        for n in 0..12 {
            ids.push(
                store
                    .begin(NewSession {
                        bot: EntityId("bot:gamma".into()),
                        focus: format!("run {n}"),
                        started_at: jiff::Timestamp::now(),
                    })
                    .await
                    .expect("begin ok")
                    .id,
            );
        }
        assert_eq!(
            (ids[0].as_str(), ids[11].as_str()),
            ("1", "12"),
            "the fixture needs a prefix pair: {ids:?}"
        );

        let wrap = async |session: &SessionId, story: &str| {
            jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    session: Some(session.to_string()),
                }))
                .await
                .expect("wrap ok");
        };
        wrap(&ids[11], "the longer id's story").await;
        wrap(&ids[0], "the shorter id's story").await;

        let journal = journal_prose(&memory).await;
        assert!(
            journal.contains("the longer id's story") && journal.contains("the shorter id's story"),
            "both sessions told their own story: {journal}"
        );
    }

    /// **A retry finishes what the first attempt started, wherever the story now
    /// sits.** The chronology half of the guard looked only at the newest entry,
    /// so anything written between the failed close and the retry — a journal
    /// entry saying the wrap failed, which is the natural thing to write — pushed
    /// the story off the tail and the retry told it a second time.
    #[tokio::test]
    async fn a_wrap_retried_after_an_intervening_entry_tells_the_story_once() {
        let store = Arc::new(RefusingClose::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            store.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;
        journal_entry(&jojobot, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                session: None,
            }))
        };
        assert!(wrap().await.is_err(), "the close refused, so the wrap failed");

        // The natural next beat: saying so. It is now the tail, not the story.
        journal_entry(&jojobot, "the wrap failed at the close — retrying").await;

        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store.inner.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        assert_eq!(
            live[0].entries.iter().filter(|e| e.text == story).count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
        );
        let journal = journal_prose(&memory).await;
        assert_eq!(
            journal.matches(story).count(),
            1,
            "…and once in the operator's Journal: {journal}"
        );
    }

    /// **A retried wrap finishes what the first one started.** The close is the
    /// step most likely to fail transiently, and by then the story is already in
    /// the chronology AND the operator's Journal — so the only move left, wrap
    /// again, told the story twice in both places.
    ///
    /// The ordering is deliberately unchanged: the story reaches the session's
    /// own record first, so a failure after it loses nothing. What changed is
    /// that each write asks whether its own half is already done.
    #[tokio::test]
    async fn a_wrap_retried_after_a_failed_close_tells_the_story_once() {
        let store = Arc::new(RefusingClose::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            store.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;
        journal_entry(&jojobot, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                session: None,
            }))
        };
        assert!(wrap().await.is_err(), "the close refused, so the wrap failed");

        // The retry, with the close working this time.
        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store.inner.sessions_of(&EntityId("bot:gamma".into())).await.expect("list ok");
        assert_eq!(
            live[0].entries.iter().filter(|e| e.text == story).count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
        );
        let journal: String = memory
            .scan()
            .await
            .expect("scan ok")
            .into_iter()
            .map(|d| d.prose)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            journal.matches(story).count(),
            1,
            "…and once in the operator's Journal: {journal}"
        );
    }

    /// A session store whose `close` refuses until it is told not to — the
    /// transient failure a wrap is most likely to meet, and the only one that
    /// leaves both writes already done.
    struct RefusingClose {
        inner: InMemorySessions,
        refuse: std::sync::atomic::AtomicBool,
    }

    impl RefusingClose {
        fn new() -> Self {
            RefusingClose {
                inner: InMemorySessions::new(),
                refuse: std::sync::atomic::AtomicBool::new(true),
            }
        }
        fn allow_close(&self) {
            self.refuse.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Sessions for RefusingClose {
        async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
            self.inner.sessions_of(bot).await
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.inner.read_session(id).await
        }
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.inner.begin(new).await
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.inner.append(id, entry).await
        }
        async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
            self.inner.amend_last(id, text).await
        }
        async fn amend_beat(
            &self,
            id: &SessionId,
            entry: &EntryId,
            text: &str,
            at: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            self.inner.amend_beat(id, entry, text, at).await
        }
        async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
            self.inner.set_focus(id, focus).await
        }
        async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
            if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(SessionError::Store("the close failed in flight".into()));
            }
            self.inner.close(id, to).await
        }
    }

    /// **Wrapping somebody else's session by id leaves your own alone.** It used
    /// to clear the binding regardless, orphaning the live session, losing its
    /// tally, and making the next write mint a second card for a session that
    /// was already running.
    #[tokio::test]
    async fn wrapping_another_session_by_id_leaves_this_connections_own_intact() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        // Somebody else's session, on the same board.
        let theirs = store
            .begin(NewSession {
                bot: EntityId("bot:delta".into()),
                focus: "their run".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .append(&theirs.id, NewEntry::manual("their beat", jiff::Timestamp::now()))
            .await
            .expect("append ok");

        boot(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, "my first beat").await;
        let my_id = mine["session"].as_str().expect("a session id").to_string();

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "wrapping theirs".into(),
                session: Some(theirs.id.to_string()),
            }))
            .await
            .expect("wrap ok");

        // My next beat continues MY session rather than minting a second card.
        journal_entry(&jojobot, "my second beat").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one card for this connection, not two: {live:?}");
        assert_eq!(live[0].id.as_str(), my_id);
        assert_eq!(live[0].entries.len(), 2, "…and it kept accruing: {:?}", live[0].entries);
    }

    /// A session store that hands the runtime a chance to run the other task at
    /// every call — what an HTTP round trip does, and what the in-memory fake
    /// never does on its own.
    ///
    /// **Without this the concurrency cases below prove nothing**: a fake that
    /// never yields runs one whole verb before the other starts, so the two
    /// futures never interleave and the race under test cannot happen.
    struct Yielding(Arc<InMemorySessions>);

    impl Yielding {
        async fn pause(&self) {
            tokio::task::yield_now().await;
        }
    }

    #[async_trait]
    impl Sessions for Yielding {
        async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
            self.pause().await;
            self.0.sessions_of(bot).await
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.read_session(id).await
        }
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.begin(new).await
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.pause().await;
            self.0.append(id, entry).await
        }
        async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
            self.pause().await;
            self.0.amend_last(id, text).await
        }
        async fn amend_beat(
            &self,
            id: &SessionId,
            entry: &EntryId,
            text: &str,
            at: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            self.pause().await;
            self.0.amend_beat(id, entry, text, at).await
        }
        async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.set_focus(id, focus).await
        }
        async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.close(id, to).await
        }
    }

    /// A handler whose session store yields at every call — see [`Yielding`].
    fn racing(store: Arc<InMemorySessions>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(Yielding(store)),
        )
    }

    /// **Two tool calls in flight on one connection must not fork the session.**
    /// rmcp runs one task per request, and the binding is read, awaited across,
    /// and written back — so without a gate both calls see "no session yet" and
    /// both materialize a card, and two same-class verbs both append a beat.
    #[tokio::test]
    async fn concurrent_first_writes_materialize_exactly_one_card() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;

        let one = jojobot.journal(Parameters(JournalArgs {
            entry: "first".into(),
            focus: None,
            session: None,
        }));
        let two = jojobot.journal(Parameters(JournalArgs {
            entry: "second".into(),
            focus: None,
            session: None,
        }));
        let (a, b) = tokio::join!(one, two);
        a.expect("journal ok");
        b.expect("journal ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one session, not one per racing call: {live:?}");
        assert_eq!(live[0].entries.len(), 2, "…carrying both entries");
    }

    /// The same race, one class down: two concurrent captures must leave one
    /// beat, not two.
    #[tokio::test]
    async fn concurrent_same_class_verbs_leave_exactly_one_beat() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        boot(&jojobot, "gamma").await;
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let (a, b) = tokio::join!(
            jojobot.capture(Parameters(capture_args("alpha", "plays go"))),
            jojobot.capture(Parameters(capture_args("milhouse", "plays chess"))),
        );
        a.expect("capture ok");
        b.expect("capture ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.beat.as_deref() == Some("capture"))
                .count(),
            1,
            "one beat for the class, whatever raced: {:?}",
            live[0].entries
        );
    }

    /// **A boot is a bind-read → act → bind-write span too, and it was ungated.**
    /// Every other such span takes `session_gate`; attaching did not, so a boot
    /// racing a first write on the same connection read the board before the
    /// card existed and wrote its binding after — clearing the session the write
    /// had just materialized, and the tally with it. The next write then minted
    /// a second card for a session already running.
    ///
    /// The window is the sweep: closing a stale card is an await between the
    /// board read and the binding write, which is exactly when the racing write
    /// gets to run.
    /// **Both orders, because only one of them forked.** `tokio::join!` rotates
    /// which future it polls first, so a single ordering proves whichever
    /// interleaving it happened to produce; the invariant is that neither
    /// produces two cards.
    #[tokio::test]
    async fn a_boot_racing_a_first_write_does_not_fork_the_card() {
        for boot_first in [true, false] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = racing(store.clone());
            make_bot(&jojobot, "gamma", None).await;
            boot(&jojobot, "gamma").await;

            // Something for the racing boot to sweep. Closing it is an await
            // between the board read and the binding write — the gap the racing
            // write slips through.
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    focus: "from the day before yesterday".into(),
                    started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                })
                .await
                .expect("begin ok");

            let booting = jojobot.boot_bot(Parameters(BootBotArgs { name: "gamma".into() }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                entry: "the first beat".into(),
                focus: None,
                session: None,
            }));
            if boot_first {
                let (b, w) = tokio::join!(booting, writing);
                b.expect("boot ok");
                w.expect("journal ok");
            } else {
                let (w, b) = tokio::join!(writing, booting);
                b.expect("boot ok");
                w.expect("journal ok");
            }

            // The next write must continue that session rather than mint a second.
            journal_entry(&jojobot, "the second beat").await;

            let live: Vec<Session> = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .into_iter()
                .filter(|s| !s.state.is_terminal())
                .collect();
            assert_eq!(
                live.len(),
                1,
                "boot_first={boot_first}: one card, not one per racing boot: {live:?}"
            );
            assert_eq!(
                live[0].entries.len(),
                2,
                "boot_first={boot_first}: …and it kept accruing: {:?}",
                live[0].entries
            );
        }
    }

    /// **An unbound connection auto-journals nothing.** jojobot does not guess
    /// which identity made a call, so a verb on a connection that never booted
    /// writes no beat and mints no session.
    #[tokio::test]
    async fn an_unbound_connection_writes_no_beats() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        ensure(&jojobot, "alpha").await;
        capture_ok(&jojobot, capture_args("alpha", "plays go")).await;

        for bot in ["bot:gamma", "bot:delta"] {
            assert!(
                store
                    .sessions_of(&EntityId(bot.into()))
                    .await
                    .expect("list ok")
                    .is_empty(),
                "nothing may be recorded against an identity nobody claimed"
            );
        }
    }

    /// **A session world that is down must not stop a boot.** The identity, the
    /// charter, the rules and the snapshot are all in other stores; refusing to
    /// boot over the session half would take an identity offline for a reason
    /// that has nothing to do with who it is.
    #[tokio::test]
    async fn boot_bot_survives_a_session_world_that_is_down() {
        let memory = Arc::new(InMemoryMemory::new());
        let healthy = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
        );
        make_bot(&healthy, "gamma", None).await;

        let down = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(DownSessions),
        );
        let booted = boot(&down, "gamma").await;
        assert_eq!(booted["identity"]["bot"]["id"], "bot:gamma", "the boot still lands");
        assert_eq!(booted["session"]["available"], false);
        assert!(
            booted["session"]["note"]
                .as_str()
                .expect("a note")
                .contains("not reachable"),
            "…and says it does not know rather than guessing: {booted}"
        );
    }

    /// A session store that answers nothing.
    struct DownSessions;

    #[async_trait]
    impl Sessions for DownSessions {
        async fn sessions_of(&self, _: &EntityId) -> Result<Vec<Session>, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn read_session(&self, _: &SessionId) -> Result<Session, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn begin(&self, _: NewSession) -> Result<Session, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn append(
            &self,
            _: &SessionId,
            _: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn amend_last(&self, _: &SessionId, _: &str) -> Result<JournalEntry, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn amend_beat(
            &self,
            _: &SessionId,
            _: &EntryId,
            _: &str,
            _: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn set_focus(&self, _: &SessionId, _: &str) -> Result<Session, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
        async fn close(&self, _: &SessionId, _: SessionState) -> Result<Session, SessionError> {
            Err(SessionError::NotConfigured("the session world is down".into()))
        }
    }

    /// `SessionState::Abandoned`, spelled once so the assertion above reads.
    fn mailbox_state_abandoned() -> SessionState {
        SessionState::Abandoned
    }

    /// **The acceptance case: "start jojobot as the PM" and the session knows
    /// who it is.** One call answers all of it — the world (the same orientation
    /// an anonymous session gets), what exists, and *which identity this is*:
    /// the charter, the rules with their provenance showing, and the state of
    /// the box whose mail is this bot's.
    #[tokio::test]
    async fn boot_bot_lands_a_session_knowing_which_identity_it_is() {
        let jojobot = handler();
        make_bot(&jojobot, "otto", Some("otto-inbox")).await;
        make_box(&jojobot, "otto-inbox").await;
        send(&jojobot, "otto-inbox", "alpha", "the shipment landed").await;

        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "otto".into(),
                prose: "Keeps the schedule.\n\nHard line: never writes to the ledger.".into(),
            }))
            .await
            .expect("set_charter ok");
        jojobot
            .capture(Parameters(CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("bot:otto", "answers before noon")
            }))
            .await
            .expect("capture ok");

        let body = boot(&jojobot, "otto").await;
        assert_ne!(body["status"], "blocked", "a bot that exists boots: {body}");

        // The world, and what is in it — everything start_here hands over.
        assert!(body["orientation"].as_str().is_some_and(|o| o.contains("provenance")));
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["bot"], 1);

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:otto");
        assert_eq!(me["bot"]["type"], "SoftwareApplication");
        assert!(
            me["charter"].as_str().is_some_and(|c| c.contains("never writes to the ledger")),
            "the charter is the orienting text, and it arrives: {me}"
        );

        let rules = me["rules"].as_array().expect("rules are a list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["content"], "answers before noon");
        assert_eq!(
            rules[0]["provenance"], "testimony",
            "a rule arrives with its provenance showing, or it reads as settled when it is a guess"
        );
        assert!(rules[0]["address"].as_str().is_some(), "and with the address that edits it");

        let owned = &me["owned_mailbox"];
        assert_eq!(owned["name"], "otto-inbox");
        assert_eq!(owned["counts"]["new"], 1, "the state of its own box: {owned}");
        assert_eq!(owned["exists"], true, "the box is there and says so");
    }

    /// **Booting creates nothing.** A bot whose declared box nobody has opened
    /// yet still boots — with the box reported plainly as not there, and the
    /// deliberate act named. Creation is an intentional act: `create_mailbox`
    /// is the only mint, and it is the only thing that runs the full name
    /// screen, so a door that minted on the side would be a door that minted
    /// near-duplicates nobody was ever shown.
    #[tokio::test]
    async fn boot_bot_reports_a_missing_box_and_opens_nothing() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma", Some("sigma-inbox")).await;

        let body = boot(&jojobot, "sigma").await;
        let owned = &body["identity"]["owned_mailbox"];
        assert_eq!(owned["name"], "sigma-inbox");
        assert_eq!(owned["available"], true, "the world is up; the box is not there");
        assert_eq!(owned["exists"], false, "said plainly: {owned}");
        assert!(owned["counts"].is_null(), "there are no counts for a box that is not there");
        assert!(
            owned["how_to_proceed"].as_str().is_some_and(|a| a.contains("create_mailbox")),
            "the way forward is the deliberate verb: {owned}"
        );

        // **Nothing was created**, by this call or any number of them.
        for _ in 0..2 {
            boot(&jojobot, "sigma").await;
        }
        let listed = json_of(&jojobot.list_mailboxes().await.expect("list ok"));
        assert_eq!(
            listed["count"], 0,
            "booting must not put a box on the board: {listed}"
        );
    }

    /// …and once someone opens it deliberately, the same boot reports it live.
    #[tokio::test]
    async fn boot_bot_reports_the_box_once_it_has_been_opened_deliberately() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma", Some("sigma-inbox")).await;
        assert_eq!(boot(&jojobot, "sigma").await["identity"]["owned_mailbox"]["exists"], false);

        make_box(&jojobot, "sigma-inbox").await;
        send(&jojobot, "sigma-inbox", "alpha", "the shipment landed").await;

        let owned = boot(&jojobot, "sigma").await["identity"]["owned_mailbox"].clone();
        assert_eq!(owned["available"], true);
        assert_eq!(owned["exists"], true);
        assert_eq!(owned["counts"]["new"], 1, "got {owned}");
        assert!(owned["how_to_proceed"].is_null(), "nothing to advise: {owned}");
    }

    /// **A claim is screened against the boxes that exist.** The review's hole:
    /// `dev2` claimed beside an existing `dev` met no screen anywhere in its
    /// life, and the box then got minted on the side. Now the claim itself is
    /// the gate — blocked, naming what it resembles, before anything is written.
    #[tokio::test]
    async fn a_claim_that_near_misses_an_existing_box_is_blocked() {
        let jojobot = handler();
        make_box(&jojobot, "gamma-inbox").await;

        let result = jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: Some("gamma-inbo".into()),
                ..add_args("bot", "gamma", "Gamma")
            }))
            .await
            .expect("a near-miss claim is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "gamma-inbo", "the suspicious thing is the box name");
        assert_eq!(body["candidates"][0]["name"], "gamma-inbox");
        assert_eq!(body["candidates"][0]["reason"], "near");

        // Nothing was written — not the claim, and not the entity carrying it.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("bot".into()) }))
                .await
                .expect("list ok"),
        );
        assert_eq!(listed["count"], 0, "a blocked claim writes no entity: {listed}");
    }

    /// The same signal a deliberate sibling box is created with clears it — and
    /// claiming the box that actually exists was never suspicious at all.
    #[tokio::test]
    async fn a_deliberate_sibling_claim_and_an_exact_one_both_go_through() {
        let jojobot = handler();
        make_box(&jojobot, "gamma-inbox").await;

        let sibling = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    mailbox: Some("gamma-inbo".into()),
                    create_new: Some(true),
                    ..add_args("bot", "gamma", "Gamma")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(sibling["mailbox"], "gamma-inbo", "the signal clears it: {sibling}");

        let exact = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    mailbox: Some("gamma-inbox".into()),
                    ..add_args("bot", "delta", "Delta")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(
            exact["mailbox"], "gamma-inbox",
            "claiming the box that exists is the ordinary case: {exact}"
        );
    }

    /// A claim moved onto an entity later is screened exactly as one written at
    /// creation — the two-step route round every gate, closed here too.
    #[tokio::test]
    async fn a_claim_added_by_update_is_screened_the_same_way() {
        let jojobot = handler();
        make_box(&jojobot, "gamma-inbox").await;
        make_bot(&jojobot, "gamma", None).await;

        let result = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "bot:gamma".into(),
                name: None,
                aliases: None,
                source: None,
                crm: None,
                mailbox: Some("gamma-inbo".into()),
                create_new: None,
            }))
            .await
            .expect("a near-miss claim is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "gamma-inbo");
        assert_eq!(body["candidates"][0]["name"], "gamma-inbox");

        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs { kind: Some("bot".into()) }))
                .await
                .expect("list ok"),
        );
        assert!(
            listed["entities"][0]["mailbox"].is_null(),
            "a blocked claim leaves the entity as it was: {listed}"
        );
    }

    /// A bot that owns no box boots perfectly well — ownership is optional, and
    /// nothing is invented to fill the hole.
    #[tokio::test]
    async fn a_bot_that_owns_no_box_still_boots() {
        let jojobot = handler();
        make_bot(&jojobot, "epsilon", None).await;

        let body = boot(&jojobot, "epsilon").await;
        assert_eq!(body["identity"]["bot"]["id"], "bot:epsilon");
        assert!(body["identity"]["owned_mailbox"].is_null(), "got {body}");
        assert!(
            json_of(&jojobot.list_mailboxes().await.expect("list ok"))["mailboxes"]
                .as_array()
                .expect("boxes")
                .is_empty(),
            "a bot with no claim must not cause a box to appear"
        );
    }

    /// A name that is no bot comes back in the guards' own shape — nothing was
    /// written, here is what jojobot suspects you meant — rather than a fresh
    /// identity conjured out of a typo.
    #[tokio::test]
    async fn booting_an_unknown_bot_is_blocked_with_candidates() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        let body = blocked(
            &jojobot
                .boot_bot(Parameters(BootBotArgs { name: "gamm".into() }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(body["attempted"], "bot:gamm");
        assert_eq!(body["candidates"][0]["handle"], "bot:gamma");
        assert!(
            body["how_to_proceed"].as_str().is_some_and(|a| a.contains("add_entity")),
            "the way out names the verb that opens it: {body}"
        );
    }

    /// This door boots bots. A bare name is read as one, and a handle of another
    /// kind is the caller's mistake — booting a person as an identity would hand
    /// back somebody's page as a charter.
    #[tokio::test]
    async fn boot_bot_reads_a_bare_name_as_a_bot_and_refuses_another_kind() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        assert_eq!(
            boot(&jojobot, "bot:gamma").await["identity"]["bot"]["id"],
            "bot:gamma",
            "a fully qualified bot handle is the same door"
        );

        let err = jojobot
            .boot_bot(Parameters(BootBotArgs { name: "person:milhouse".into() }))
            .await
            .expect_err("another kind must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("bot"), "the error says what this door takes: {}", err.message);
    }

    /// **Both doors make the same promise, so both keep it.** `orient` says
    /// orientation lands even when a world is down — and `start_here` did,
    /// while `boot_bot` hard-errored the moment a bot owned a box, which made
    /// every box-owning identity unbootable over an outage in the *other*
    /// world. The charter and the rules are in Memory and were right there.
    ///
    /// Now the mailbox half degrades on its own, the same way the snapshot's
    /// does: the boot lands, the identity is whole, and the one thing jojobot
    /// cannot answer says so instead of guessing.
    #[tokio::test]
    async fn boot_bot_survives_a_world_that_is_down_exactly_as_start_here_does() {
        // Stood up while both worlds are up — a claim that cannot be screened
        // is refused, so this bot could not have been created below.
        let memory = Arc::new(InMemoryMemory::new());
        let healthy = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
        );
        make_bot(&healthy, "gamma", Some("gamma-inbox")).await;
        healthy
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
            }))
            .await
            .expect("set_charter ok");

        let jojobot = handler_with_mailboxes_down(memory);
        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "a boot must still land: {body}");

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:gamma");
        assert_eq!(me["charter"], "Holds the plan.", "the half that is up arrives whole");

        let owned = &me["owned_mailbox"];
        assert_eq!(owned["name"], "gamma-inbox", "the claim is Memory's and is still known");
        assert_eq!(owned["available"], false, "got {owned}");
        assert!(
            owned["exists"].is_null(),
            "whether the box is there is unknown, and null says so rather than guessing: {owned}"
        );
        assert!(owned["note"].as_str().is_some_and(|n| !n.is_empty()));

        // …and the snapshot degrades beside it, exactly as it does anonymously.
        assert_eq!(body["snapshot"]["mailboxes"]["available"], false);
    }

    /// **One response never contradicts itself about which boxes exist.**
    ///
    /// It could before: booting minted the declared box *between* taking the
    /// snapshot and reporting the identity, so a single payload said in one
    /// half that no such box was on the board and in the other that it was
    /// there with counts — and a session had no way to tell which half to
    /// believe. Nothing is minted mid-orient now, so both halves are reads of
    /// the same world; this holds them to agreeing, in both directions.
    #[tokio::test]
    async fn a_boot_never_disagrees_with_its_own_snapshot_about_a_box() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma", Some("sigma-inbox")).await;

        let listed = |body: &serde_json::Value| -> bool {
            body["snapshot"]["mailboxes"]["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .any(|b| b["name"] == "sigma-inbox")
        };

        let before = boot(&jojobot, "sigma").await;
        assert_eq!(before["identity"]["owned_mailbox"]["exists"], false);
        assert!(!listed(&before), "the snapshot must agree it is absent: {before}");

        make_box(&jojobot, "sigma-inbox").await;

        let after = boot(&jojobot, "sigma").await;
        assert_eq!(after["identity"]["owned_mailbox"]["exists"], true);
        assert!(listed(&after), "…and agree it is there: {after}");
    }

    /// **One orientation, two doors.** `boot_bot` is `start_here` plus an
    /// identity — not a second world-model to drift out of step with the first.
    #[tokio::test]
    async fn boot_bot_and_start_here_hand_over_the_same_world() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        let anonymous = json_of(&jojobot.start_here().await.expect("start_here ok"));
        let identified = boot(&jojobot, "gamma").await;
        assert_eq!(
            anonymous["orientation"], identified["orientation"],
            "the world-model is one text, or the two doors teach different jojobots"
        );
        assert_eq!(anonymous["snapshot"], identified["snapshot"]);
        assert!(anonymous["identity"].is_null(), "an anonymous session claims no identity");
    }

    /// `set_charter` writes the orienting prose and reads it back — and it is
    /// the same text `boot_bot` hands over, so what an operator writes is what a
    /// session is told.
    #[tokio::test]
    async fn set_charter_writes_the_prose_that_boot_bot_reads_back() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        let written = json_of(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "gamma".into(),
                    prose: "  Holds the plan. Does not implement.  ".into(),
                }))
                .await
                .expect("set_charter ok"),
        );
        assert_eq!(written["bot"], "bot:gamma");
        assert_eq!(
            written["charter"], "Holds the plan. Does not implement.",
            "the verb returns what a read will return: {written}"
        );
        assert_eq!(
            boot(&jojobot, "gamma").await["identity"]["charter"],
            "Holds the plan. Does not implement."
        );

        // A charter for a bot that does not exist misses — it never creates one,
        // and the miss wears the same blocked shape every other absence does.
        let missed = blocked(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "nobody".into(),
                    prose: "a charter for nobody".into(),
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "bot:nobody");
        assert!(
            missed["how_to_proceed"].as_str().is_some_and(|a| a.contains("add_entity")),
            "the way out names the verb that opens it: {missed}"
        );
    }
}
