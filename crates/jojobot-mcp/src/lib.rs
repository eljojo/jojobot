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

pub mod sid;

use jojobot_domain::mailbox::{
    self, Delivered, Delivery, Mailbox, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, guard::MailboxMatch,
};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FactStatus, Guarded, JOURNAL_TITLE, Memory, MemoryError, NewEntity, NewFact, Provenance,
    guard::{self, EntityMatch},
    search::{DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, MailCoverage, Search, SearchQuery},
    validate_edge,
};
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions,
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
- *Leave word for another session* → `list_mailboxes` to see what exists and what is waiting, `post_message` into an agreed box with a body written for a reader with none of your context. jojobot records who sent it from the `sid` you pass, so there is nothing to declare and nothing to get wrong.
- *Handle mail* → `read_mailbox`, which opens YOUR box — the `sid` you pass says which one, so there is no name to give and no way to reach into somebody else's. Reading takes delivery of every message in it; act, then `mark_processed`, ONLY after acting, with the outcome in notes. A failure is data to record, not a state to park in.
- *One message, not a whole box* → `search` for it, then `read_message` on the id the hit carries. Draining your whole box makes every message in it owed work; `read_message` takes on the one you actually meant.

When the right write is not obvious, ask the operator — an unasked write outlives the conversation that guessed it.

## The answers that are not errors

A **blocked** result is a SUCCESS whose body says `status: "blocked"`, `wrote: false`: nothing was written, and `how_to_proceed` says what to do next. Never retry one unchanged. Four gates produce it, with different ways out: **resemblance** (creating or renaming something that looks like what exists — pick the candidate you meant, or `create_new: true` only when you can say how the two differ; an exact handle or box name is never overridable), **absence** (you named something that is not there — the subject of a capture, an edge's object, the box of a post, a handle to read, an address to edit, a message id to retire; empty `candidates` means nothing even resembles it, not that your call was malformed; for an entity, creating it and retrying is usually right — for a mailbox it usually is not), **ownership** (a mailbox has exactly one owner, and a second claim on one is refused naming the holder; `create_new` does not clear this — it answers a question about names), and **unreadable** (`mark_processed` reached an item jojobot cannot read — no retry helps, a person must repair it; treat what it carried as unhandled and say so).

A plain **error** is a malformed call — a token that is no kind, a string that is no address — or the store itself failing. **Absence is never an error here**: naming something that does not exist is an answer with candidates, not a broken server, so read `status` rather than branching on whether the call errored. And know what the guards do NOT cover: they catch resemblance, absence and ownership, never judgement — a wholly novel name sails through, and nothing will stop you standing up a box nobody drains. That call is yours, and the store keeps whatever you decide.

## Bots

An **identity** is an entity of kind `bot`: a handle like `bot:gamma`, a **charter** (its prose — what this identity is, its hard lines, where its work lives), **rules** as ordinary facts about it (so each one carries its own provenance: an inferred rule is a hypothesis, not a policy), and optionally **one owned mailbox**. If you were told which identity you are, pass that name to `start_here` — the one door — and it hands over everything here plus that identity. Nothing about a bot is built into jojobot — a bot is data somebody wrote, like every other entity.

## Sessions

A bot is a **role**; a **session is one mortal run of it** — the unit of work, not the unit of connection. It outlives a disconnect and a device hop, because what makes two connections the same session is the `sid` you carry — hold it and keep passing it, on writes and reads alike.

**Booting an identity starts or resumes its session; there is no separate verb.** `start_here` with your bot name sweeps that bot's stale sessions to `abandoned` (a day without a beat). If a resumable session remains you get the choice — what each one was working on, and whether it is still running or stopped without being wrapped up — and NO sid until you answer: choose resume and you inherit its chronology, choose new and a fresh sid is minted beside it, closing nothing. With nothing to resume the sid comes back straight away. Either way the card itself is written **lazily**, on your first real write, so a boot that does nothing leaves nothing behind.

A session has two halves that answer different questions. Its **focus** is what it is working on NOW, one line, rewritten in place. Its **chronology** is what happened: append-only, oldest first, with only the newest entry amendable.

- *Record a beat* → `journal` — **a literal journal, not a log.** What you set out to do, what you found, what you decided, what went wrong. NOT every tool call and not every file: a reader months from now wants the story, and a firehose buries it. Pass `focus` when what you are working on changes.
- *Fix the beat you just wrote* → `amend_journal`. Only the most recent one; everything older is what it was.
- *End* → `wrap_session` with the story, written for somebody with none of your context. It becomes your final entry AND one dated entry in the operator's Journal, and the session goes `wrapped` — terminal both ways.

jojobot also writes **its own beats** into your chronology: one per class of WRITE you make, its count kept current as you go. Reads are not journalled. They are marked apart (`beat` names the class) because what you said you were doing and what jojobot noticed you doing are different kinds of evidence.

### The two endings, and they are not interchangeable

**WRAP when the work is over.** Your run finished what it was for; the story is told and the card closes clean. Nothing appends to it afterwards.

**CLEAR AND RESUME when the work continues on another agent.** You are stopping, the job is not done, and somebody — a later run of you, on another device, after a context reset — picks it up. Then **journal a resume note and do NOT wrap**: the next boot of this identity is offered this session by what it says it is working on, and whoever resumes it reads your chronology. Wrapping here would tell the story of something that has not happened yet and force the next run to start from nothing.

The resume note is **the one sanctioned exception to journal leanness**. Everywhere else a beat is high-level; here, be dense and specific — where you got to, what you already ruled out, the exact next step, the thing that will bite whoever picks this up. Its only reader is somebody with your job and none of your context.

`abandoned` is neither of these, and it is **not a failure**: it means the run was never wrapped up. A session stops without telling its story — a disconnect, a closed laptop, an agent that moved on — and the next boot a day later marks it so. Its chronology survives, it is still worth reading, and **resuming it is ordinary rather than recovery**. The difference between `wrapped` and `abandoned` is whether a run ended or merely stopped.

### Your box is yours; the others are not

**You read your OWN mailbox, and the surface offers no other.** `start_here`, booted as your identity, tells you which box you own, and `read_mailbox` opens that one — there is no name to pass. This used to be a norm you could ignore: reading IS delivery, so a look moved somebody's mail out of `new` and made it yours to finish, and a message you took but cannot act on is one its real consumer never sees as fresh. It is now simply not reachable.

`list_mailboxes` reports every box on the server: that is a fact about the board and **not an invitation**. A box showing `new: 1` is not addressed to you unless it is yours. If you need something from another box, ask its owner or leave a message in it — `post_message` writes without reading, which is exactly the shape of a request.
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// The entity to read facts about — any `kind:slug` id (a bare handle is
    /// read as a person).
    pub subject: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `list_entities`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListEntitiesArgs {
    /// Narrow to one kind; omit for every entity.
    #[serde(default)]
    pub kind: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `start_here` — **the one door**, with or without an identity.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OrientArgs {
    /// Optional. The bot to boot as: its bare slug, or its full `bot:`-prefixed
    /// handle. A handle of any other kind is refused — this door boots bots.
    /// Omit it for an anonymous orientation: you get the world and the
    /// snapshot, and no sid.
    #[serde(default)]
    pub bot: Option<String>,
    /// Skip the orientation essay and return only what changes between calls —
    /// the snapshot, your identity, your session.
    #[serde(default)]
    pub brief: Option<bool>,
    /// Your answer to the resume-or-new choice a boot hands back when this bot
    /// has a session worth picking up: the `sid` of the one you are resuming,
    /// exactly as the offer spelled it, or `new` for a fresh session. Leave it
    /// off on a first boot — there is nothing to answer yet.
    #[serde(default)]
    pub resume: Option<String>,
}

/// Arguments to `set_charter`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCharterArgs {
    /// The bot whose charter this is: its bare slug, or its full handle.
    pub bot: String,
    /// The charter itself. Prose: paragraphs are fine. It **replaces** whatever
    /// charter the bot had, so send the whole thing, not an addition.
    pub prose: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `post_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PostMessageArgs {
    /// The box to leave it in. **It must already exist** — an unknown name comes
    /// back with candidates and nothing is written.
    pub mailbox: String,
    /// The message itself. Prose: paragraphs are fine.
    pub body: String,
    /// **Your session id.** Required here, because it is what jojobot records
    /// as the sender: a message from nobody is a message nobody can reply to,
    /// and identity that is merely declared is identity that can be wrong.
    pub sid: String,
    /// What this message is about, in one line — a title, not a summary.
    /// Optional, and worth giving: it is what a reader sees on the card and on
    /// a search hit before they open anything. Do NOT also repeat it as the
    /// body's first line.
    #[serde(default)]
    pub subject: Option<String>,

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
    /// Ship bodies only for messages nobody has taken yet — **the default**.
    /// Leftovers, the ones flagged `seen_before`, still come back, still
    /// counted, still owed; only their bodies are left out, and each says so.
    ///
    /// Pass `false` to get those bodies back — the read a consumer makes when
    /// it is recovering from a crash and no longer holds what it was given.
    #[serde(default)]
    pub new_only: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `list_mailboxes`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListMailboxesArgs {
    /// Your session id. The boxes your bot owns come back with their counts;
    /// every other box comes back as a name only. Omit it and you own nothing,
    /// which is right for a caller that only posts.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `list_sent`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListSentArgs {
    /// Whose outgoing mail to show, matched **exactly** against the sender
    /// recorded on each message. Omit it for your own — your `sid` says who
    /// that is, and your own mail is what this verb is for.
    #[serde(default)]
    pub sender: Option<String>,
    /// Only this box. Omit for every box you have posted into.
    #[serde(default)]
    pub mailbox: Option<String>,
    /// Ship the bodies back too. Off by default: you wrote them, so the useful
    /// answer is where they got to, not what they say.
    #[serde(default)]
    pub include_bodies: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Arguments to `read_message`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadMessageArgs {
    /// The message's id, exactly as a search hit, a delivery or `post_message`
    /// returned it.
    pub message_id: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
    /// **Your session id**, exactly as the boot door returned it. A session is
    /// bound to the bot that booted it; there is no way to write into another
    /// one.
    pub sid: String,
}

/// Arguments to `amend_journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AmendJournalArgs {
    /// What the most recent entry should say instead. It replaces that entry
    /// whole.
    pub entry: String,
    /// Your session id — the session whose newest entry to rewrite.
    pub sid: String,
}

/// Arguments to `wrap_session`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WrapSessionArgs {
    /// The story of this session, for somebody with none of your context: what
    /// it was for, what happened, what is left. It becomes the final chronology
    /// entry AND one dated entry in the operator's Journal.
    pub story: String,
    /// Your session id — the session to wrap.
    pub sid: String,
}

/// Arguments to `mark_processed`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkProcessedArgs {
    /// The message's id, exactly as `read_mailbox` returned it.
    pub message_id: String,
    /// What happened — including a failure. Optional, one plain line.
    #[serde(default)]
    pub notes: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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

/// **Who is calling**, resolved from the handle they carry.
///
/// This replaces the per-connection binding outright. The binding assumed a
/// client holds one MCP connection across a conversation; none do — claude.ai
/// and ChatGPT both open what jojobot sees as a fresh, unbound connection per
/// tool call — so an identity written on the connection was gone before the
/// next request arrived. **The handle is the only address**, it rides every
/// verb, and jojobot looks the caller up rather than remembering them.
#[derive(Debug, Clone)]
struct Caller {
    /// The handle itself, exactly as the caller passed it.
    sid: sid::Sid,
    /// The identity it belongs to. **Bound at boot and never switched**, which
    /// is what makes naming somebody else's session a refusal rather than a
    /// thing jojobot quietly honours.
    bot: EntityId,
    /// The card this run landed on, once one exists. `None` until the first
    /// real write materializes it.
    card: Option<SessionId>,
}

/// **What a boot found on the board**, after the sweep has run.
///
/// Named rather than a tuple because it grew a third thing the day an
/// `abandoned` run became something a boot could offer back, and a
/// `(Vec, Option, Vec)` at five call sites is a shape nobody can read.
struct Board {
    /// Every run still going, newest first. A bot may have several at once.
    live: Vec<Session>,
    /// **At most one** run that stopped without being wrapped up, recently
    /// enough to be worth bringing up — see
    /// [`OFFER_ABANDONED_WITHIN`](jojobot_domain::session::OFFER_ABANDONED_WITHIN).
    /// One is a memory jog; a list of them is a history nobody asked for.
    offerable: Option<Session>,
    /// The ids this boot's sweep closed.
    swept: Vec<String>,
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

    /// **The one orienting door**, with or without an identity: the world-model
    /// in prose, a live snapshot of what exists, and — when a bot is named —
    /// that identity and its session.
    ///
    /// There is deliberately no second verb for the identified case. The two
    /// used to be separate doors over this same function, which is one text and
    /// one snapshot by construction but two surfaces to keep true, and the
    /// second one drifted.
    ///
    /// The prose below is ENGINE material: it explains the method, names only
    /// roles ("the operator"), and every example identity is fictional.
    #[tool(
        description = "New here? Call this first — it is the ONE door, whether or not you have an \
                       identity. Explains what jojobot is and how its world fits together — \
                       entities, facts, provenance, edges, mailboxes — with worked examples, and \
                       returns a live snapshot of what exists right now (entities by kind, and \
                       every mailbox by name — with counts for the ones you drain and the ones \
                       nobody drains), so you start oriented instead of guessing. CALLED THIS \
                       BEFORE? Pass brief: true and you get the snapshot without the essay — the \
                       essay is the only part that does not change between calls, and calling \
                       again without brief reads it in full. NAME A BOT and the same answer also \
                       carries that identity: its charter (the orienting text — what this \
                       identity is, its hard lines, where its work lives), its rules as dated \
                       claims each carrying its own provenance (testimony is settled, inference \
                       is a hypothesis — read them that way), and the per-state counts of the \
                       mailbox it owns. THIS DOOR MINTS NOTHING: a name that is no bot comes back \
                       status: blocked, listing the bots that do exist and offering to boot as \
                       one of them, and a mailbox a bot claims but nobody has opened is reported \
                       missing rather than created. BOOTING STARTS OR RESUMES THAT BOT'S SESSION \
                       — there is no separate start verb. It first sweeps that bot's sessions \
                       that have gone a day without a beat to `abandoned` — which is the one \
                       thing a boot writes. Name no bot at all and this is an orientation \
                       preview: read-only, the world and the snapshot, no identity and no \
                       session. Pass the `sid` you were handed on EVERY call, reads included — it \
                       is how jojobot knows which bot is asking."
    )]
    async fn start_here(
        &self,
        Parameters(args): Parameters<OrientArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bot = named_bot(args.bot.as_deref())?;
        let resume = args
            .resume
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty());
        // **An answer with nobody to answer for.** `resume` responds to an
        // offer only a named boot makes, so carrying one without a bot is a
        // malformed call rather than an absence — there is no session it could
        // be about, and honouring it would mean guessing whose it was.
        if resume.is_some() && bot.is_none() {
            return Err(McpError::invalid_params(
                "resume answers the choice a boot hands back, so it needs the bot you are booting \
                 as — pass `bot` too, or drop `resume` for an anonymous orientation"
                    .to_string(),
                None,
            ));
        }
        self.orient(bot.as_ref(), args.brief.unwrap_or(false), resume)
            .await
    }

    /// Write a bot's charter — the prose layer of its own page.
    #[tool(
        description = "Write a bot's charter: the orienting text start_here hands a session that \
                       boots as this bot — what this identity is, its hard lines, where its work \
                       lives. Replaces the whole charter rather than adding to it, and returns \
                       the stored text, which is what a later boot will read back. A bot that \
                       does not exist comes back status: blocked with the nearest handles — \
                       add_entity first; nothing is created here. Rules are not written here \
                       either: a rule is a fact about the bot, so capture it."
    )]
    async fn set_charter(
        &self,
        Parameters(args): Parameters<SetCharterArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let bot = bot_id(&args.bot)?;
        let stored = match self.memory.set_prose(&bot, &args.prose).await {
            Ok(stored) => stored,
            Err(e) => return memory_declined("set_charter", e),
        };
        self.beat("set_charter", bot.as_str(), args.sid.as_deref())
            .await;
        json_result(&serde_json::json!({
            "bot": bot.as_str(),
            "charter": stored,
        }))
    }

    /// The one orientation, anonymous or identified — **the one call site is
    /// the point.** Naming a bot adds the identity half to an answer that is
    /// otherwise the same text and the same snapshot; it does not open a second
    /// way in.
    async fn orient(
        &self,
        bot: Option<&EntityId>,
        brief: bool,
        resume: Option<&str>,
    ) -> Result<CallToolResult, McpError> {
        // **The entity index is read ONCE for the whole answer.** Three parts of
        // a boot need it — the counts by kind, which boxes the caller drains,
        // and the identity itself — and each used to fetch it, which is three
        // remote round trips per boot AND three reads that can disagree with
        // one another inside a single payload.
        //
        // Best-effort per world: orientation must land even when one world is
        // down — a fresh agent on a half-configured server still gets the map.
        let index = self.memory.list_entities(None).await;
        let entities = match &index {
            Ok(entities) => {
                let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
                for e in entities {
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
        // **The snapshot is scoped the same way the listing is.** It was the
        // other place a boot met per-state counts for every box on the server,
        // and it posed the same question the own-box norm then has to answer in
        // prose: is that unread one mine? An anonymous `start_here` owns
        // nothing, which is exactly right for a caller that only posts.
        let mine = match &index {
            Ok(index) => self.ownership_of(index, bot),
            Err(_) => Ownership::unknown(),
        };
        let mailboxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => serde_json::json!({
                "available": true,
                "counts_shown_for": mine.shown_for(&boxes),
                "ownership_known": mine.known,
                "note": mine.note(),
                "boxes": boxes
                    .iter()
                    .map(|b| {
                        if mine.covers(b.name.as_str()) {
                            let mut body = mailbox_json(b);
                            if let Some(obj) = body.as_object_mut() {
                                obj.insert("yours".into(), mine.drains(b.name.as_str()).into());
                            }
                            body
                        } else {
                            serde_json::json!({
                                "name": b.name.as_str(),
                                "yours": false,
                                "counts": serde_json::Value::Null,
                                "counts_elided": true,
                                // **Quarantine is not a count, and it does not
                                // ride out with them.** It is the only place an
                                // unreadable card's existence shows, and the
                                // caller who most needs it is a SENDER — who by
                                // definition does not drain this box, and would
                                // otherwise conclude their message was never
                                // sent. What is scoped away is somebody's
                                // queue, never a fault on the board.
                                "quarantined": quarantined_json(b),
                            })
                        }
                    })
                    .collect::<Vec<_>>(),
            }),
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the mailbox world is not reachable right now — its tools will say why",
            }),
        };
        let snapshot = serde_json::json!({ "entities": entities, "mailboxes": mailboxes });
        // A memory world that is down cannot answer who anybody is; the
        // snapshot above already says so, and this stays null rather than
        // claiming the identity is missing.
        let identity = match (bot, &index) {
            (None, _) | (_, Err(_)) => serde_json::Value::Null,
            (Some(bot), Ok(index)) => match self.identity(index, bot).await? {
                Ok(identity) => identity,
                // A name that is no bot: the guards' own shape, so one
                // client-side branch handles every "jojobot declined" answer —
                // but with the door's own body, not the generic absence one.
                Err(candidates) => {
                    return Ok(booting_unknown(bot, &candidates, index));
                }
            },
        };
        // **Only after the identity resolved.** A name that is no bot boots
        // nothing, so it starts no session and sweeps nothing either — binding
        // a connection to an identity jojobot just refused would be a session
        // belonging to nobody.
        let session = match bot {
            None => serde_json::Value::Null,
            Some(bot) => match self.attach(bot, resume).await {
                Ok(session) => session,
                // A handle that addresses nothing stops the whole answer.
                // Handing back orientation around it would bury the one thing
                // the caller has to act on.
                Err(refused) => return Ok(refused),
            },
        };
        json_result(&serde_json::json!({
            "orientation": if brief { serde_json::Value::Null } else { ORIENTATION.into() },
            // **The elision is marked, and that is all it is.** The essay used
            // to arrive stamped with a version so a returning session could ask
            // whether the copy it held was current; the stamp is gone, and no
            // staleness check replaces it. What is left is the marker every
            // elision on this surface owes — less came back, and the caller is
            // told so rather than left to infer withheld from empty.
            "orientation_elided": brief,
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
        index: &[Entity],
        bot: &EntityId,
    ) -> Result<Result<serde_json::Value, Vec<EntityMatch>>, McpError> {
        let Some(entity) = index.iter().find(|e| &e.id == bot) else {
            return Ok(Err(guard::screen(bot, &[], index)));
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

    /// Start or resume this bot's session, and hand back the handle for it.
    ///
    /// **Booting an identity IS starting its session** — there is no separate
    /// verb, because there is no moment between "I am gamma" and "gamma is
    /// working" for one to sit in. The sweep runs first either way: any `active`
    /// session of THIS bot whose last beat is older than [`ABANDONED_AFTER`] is
    /// closed as `abandoned` — lazily, at boot, because there is no background
    /// runtime until M8 and a session left open forever would make "resume where
    /// you left off" resume something from last month.
    ///
    /// **Then the boot has two branches, and which one you get is not a
    /// preference.**
    ///
    /// * **Nothing survives the sweep** → the handle comes back immediately.
    ///   There is nothing to decide, and making the caller ask twice for an
    ///   address would invent the moment this verb exists to deny.
    /// * **Something does** → the CHOICE comes back and no handle: every
    ///   resumable run, each named by what it was working on, because that is
    ///   the only thing that tells two runs of one identity apart. The handle
    ///   arrives when the caller answers — `resume` with one of them, or `new`.
    ///   Attaching silently was the old behaviour and it decided for the caller;
    ///   worse, it decided for the caller who had deliberately left the run open
    ///   for somebody else.
    ///
    /// Either way the **card stays lazy**: no card until the first write, so a
    /// boot that never works leaves no trace — which is what keeps "creation is
    /// an intentional act" true for a verb whose whole job is to start
    /// something. And **nothing here auto-wraps**: choosing `new` beside a
    /// running session leaves that session running.
    ///
    /// `Err` is a handle that addresses nothing — see [`handle_declined`].
    /// A session store that is down degrades exactly as the mailbox world does:
    /// the boot still lands, and the block says jojobot does not know rather
    /// than guessing.
    async fn attach(
        &self,
        bot: &EntityId,
        resume: Option<&str>,
    ) -> Result<serde_json::Value, CallToolResult> {
        // **A boot is a read-the-board → decide → write-the-registry span like
        // the write verbs, so it takes the same gate, on the same key.** Its
        // board read is full of awaits — sweeping a stale card is one — and a
        // first write running inside them commits a card the boot then sees
        // with no handle against it yet, which is a second handle minted for a
        // run that already has one. See [`Jojobot::gate_key`] for why the key
        // is the identity: it is the only name a boot and a write share.
        //
        // Taken here rather than inside `sweep_and_find`, which runs under it —
        // the mutex is not reentrant.
        let gate = self.registry.gate(bot.as_str());
        let _serialized = gate.lock().await;
        let Board {
            live,
            offerable,
            swept,
        } = match self.sweep_and_find(bot).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(error = %e, bot = %bot, "the session world is not reachable");
                // **No handle is minted.** One handed out here would address
                // either a fresh session or one already running, and jojobot
                // cannot say which — so it hands out none and says so.
                return Ok(serde_json::json!({
                    "available": false,
                    "note": "the session world is not reachable right now, so jojobot cannot say \
                             whether you have a session in flight, and has not started one — a \
                             fresh session here could fork one that is already running. It will \
                             try again on your first write. Everything else here is unaffected; \
                             the session verbs will say why.",
                }));
            }
        };

        let mut block = match resume {
            // ── the caller answered the offer ───────────────────────────────
            Some(answer) if answer.eq_ignore_ascii_case(sid::NEW) => {
                let handle = self.mint_or_say_why(bot, None)?;
                // Bound to the identity with no session: the first write is
                // what begins the card, exactly as a first boot's is. **The run
                // that was offered is left running** — a new session never
                // closes an old one.
                self.fresh_block(handle)
            }
            Some(answer) => {
                let (handle, session) = self.resumable(bot, answer, &live).await?;
                match session {
                    Some(session) => {
                        let block = serde_json::json!({
                            "available": true,
                            "sid": handle.as_str(),
                            "resumed": true,
                            "session": session_json(&session),
                            "note": "you are resuming a session already in flight — its \
                                     chronology is above. Read it before you start: somebody \
                                     (you, before a disconnect) was part way through something.",
                        });
                        block
                    }
                    // A handle whose session was never written: theirs, still
                    // good, and still nothing behind it.
                    None => self.fresh_block(handle),
                }
            }
            // ── a first boot: the two branches ──────────────────────────────
            None if live.is_empty() && offerable.is_none() => {
                let handle = self.mint_or_say_why(bot, None)?;
                self.fresh_block(handle)
            }
            None => {
                // Every live run, then the one stop worth bringing up. The
                // abandoned one comes last because it is the weaker claim on
                // the caller's attention, not because it is worse.
                let offered: Vec<&Session> = live.iter().chain(offerable.iter()).collect();
                let mut choices = Vec::with_capacity(offered.len());
                for session in offered {
                    let handle = self.handle_for(bot, &session.id)?;
                    let mut choice = serde_json::json!({
                        "sid": handle.as_str(),
                        // **What it was working on is the whole point of the
                        // offer.** A bot may have several runs at once, and a
                        // list of opaque handles is not a choice anybody can
                        // make.
                        "working_on": session.focus,
                        // **Marked, never silently mixed in.** Not because a
                        // stop is worse — it is not a failure — but because
                        // "this one was never wrapped up" is what tells the
                        // caller which of these is still warm.
                        "state": session.state.as_token(),
                        "started_at": session.started_at.to_string(),
                        "last_beat": session.last_beat().to_string(),
                        "entry_count": session.entries.len(),
                    });
                    if session.state == SessionState::Abandoned
                        && let Some(obj) = choice.as_object_mut()
                    {
                        obj.insert(
                            "note".into(),
                            "this run stopped without being wrapped up — a disconnect, a closed \
                             laptop, an agent that moved on. Resuming it is ordinary: it reopens \
                             where it left off and its chronology continues."
                                .into(),
                        );
                    }
                    choices.push(choice);
                }
                // **Bound as it has always been, to the newest run.** This
                // round moves what the DOOR hands back; the write path still
                // resolves an unaddressed write to the live session, and
                // binding to nothing here would make the next bare write fork a
                // second card beside the one being offered.
                serde_json::json!({
                    "available": true,
                    // **No handle until the choice is answered.** Its absence
                    // is the question: there is more than one thing this boot
                    // could mean, and jojobot is not picking.
                    "sid": serde_json::Value::Null,
                    "resumed": false,
                    "session": serde_json::Value::Null,
                    "choices": choices,
                    "how_to_proceed": "This identity has work already in flight. Call start_here \
                                       again with resume: the sid of the run you are picking up — \
                                       read what it was working on above — or resume: \"new\" for \
                                       a fresh session. Nothing was closed and nothing was \
                                       written; choosing new leaves the runs above running.",
                })
            }
        };
        if let Some(obj) = block.as_object_mut() {
            obj.insert("swept".into(), swept.into());
        }
        Ok(block)
    }

    /// The block for a session with no card behind it yet — a first boot, or
    /// `new`, or a handle nothing has been written under.
    fn fresh_block(&self, handle: sid::Sid) -> serde_json::Value {
        serde_json::json!({
            "available": true,
            "sid": handle.as_str(),
            "resumed": false,
            "session": serde_json::Value::Null,
            "note": "a fresh session, and this is its sid. Nothing is written yet — the record \
                     begins on your first journal entry or the first write you make, so a boot \
                     that does nothing leaves nothing behind.",
        })
    }

    /// **What this call serializes on: the IDENTITY, not the handle.**
    ///
    /// The handle looks like the right key — it names exactly the run this call
    /// will write to, and two writes on one handle are the pair the gate was
    /// first built for. It is too narrow by one caller. A boot resolves the
    /// whole bot's board, so it can only key on the bot; a write knows its
    /// handle and nothing else; and the two are about the same run whenever
    /// that run's card does not exist yet. Keyed separately they queue apart,
    /// and the boot reads the board inside the gap between the write committing
    /// its card and the registry being told which handle it landed on — finding
    /// a live run no handle addresses, and minting a second one for it.
    ///
    /// The identity is the only name both callers hold, so it is the key. It
    /// serializes two writes on one handle exactly as before (same bot, same
    /// key) and two runs of one bot besides, which is a cost bounded by how many
    /// identities this operator has.
    ///
    /// A handle this process is not holding keys on itself, and a call carrying
    /// none keys on the empty string rather than skipping the lock, so there is
    /// exactly one code path; both are refused downstream anyway.
    fn gate_key(&self, sid: Option<&str>) -> String {
        let Some(raw) = sid.map(str::trim).filter(|s| !s.is_empty()) else {
            return String::new();
        };
        match self.registry.lookup(raw) {
            Some(held) => held.bot.as_str().to_string(),
            None => raw.to_string(),
        }
    }

    /// **Who is calling.** `None` is an anonymous caller, which is a legitimate
    /// thing to be: a reader, or a poster who has not booted.
    fn caller(&self, sid: Option<&str>) -> Result<Option<Caller>, CallToolResult> {
        let Some(raw) = sid.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        if !sid::is_readable(raw) {
            return Err(handle_declined(
                raw,
                format!(
                    "Nothing was written. '{raw}' is not a handle jojobot mints — those are {} \
                     characters of 0-9 and a-z, with i, l, o and u left out because they read as \
                     one another. jojobot will not correct one, because correcting it means \
                     guessing whose session you meant.",
                    jojobot_domain::session::SID_LEN,
                ),
            ));
        }
        let Some(held) = self.registry.lookup(raw) else {
            return Err(handle_declined(
                raw,
                format!(
                    "Nothing was written. That session is gone: '{raw}' is not a handle jojobot \
                     is holding. Call start_here with your bot name to boot again — the work on \
                     the board is untouched, and it will be offered back by what it was working \
                     on."
                ),
            ));
        };
        Ok(Some(Caller {
            sid: sid::Sid(raw.to_string()),
            bot: held.bot,
            card: held.card,
        }))
    }

    /// **A handle that is present must be good, even where carrying one is
    /// optional.**
    ///
    /// The write verbs outside the session surface take an optional `sid`:
    /// carrying none is legitimate — a reader, a poster that never booted — and
    /// costs only the automatic beat. Carrying a DEAD one is a different thing
    /// and used to cost nothing at all, because [`Jojobot::beat`] was the only
    /// place those verbs looked at the handle, and `beat` is silent by design.
    /// The refusal went out with the silence: the write landed, the caller's
    /// chronology stopped, and it found out at wrap or never.
    ///
    /// Called BEFORE the write, never after. `beat` runs once the store has
    /// already answered, and `blocked` means `wrote: false` everywhere on this
    /// surface — one handed back over a write that landed would be a worse lie
    /// than the silence it replaced.
    fn attributable(&self, sid: Option<&str>) -> Result<(), CallToolResult> {
        self.caller(sid).map(|_| ())
    }

    /// The caller, required — for the verbs that write to a session.
    fn identified(&self, sid: Option<&str>) -> Result<Caller, CallToolResult> {
        match self.caller(sid)? {
            Some(caller) => Ok(caller),
            None => Err(session_unbound()),
        }
    }

    /// Mint a handle, or turn the one failure into an answer rather than a 500.
    fn mint_or_say_why(
        &self,
        bot: &EntityId,
        card: Option<SessionId>,
    ) -> Result<sid::Sid, CallToolResult> {
        self.registry.mint(bot, card).map_err(|_| {
            handle_declined(
                "",
                "No session was started. jojobot could not mint a free session handle, which \
                 means this process is holding a great many of them. Nothing is wrong with your \
                 call and nothing on the board was touched — a restart clears the handles it is \
                 holding."
                    .to_string(),
            )
        })
    }

    /// The handle for a card that exists — the one it already has, or a new one.
    fn handle_for(&self, bot: &EntityId, card: &SessionId) -> Result<sid::Sid, CallToolResult> {
        match self.registry.addressing(card) {
            Some(handle) => Ok(handle),
            None => self.mint_or_say_why(bot, Some(card.clone())),
        }
    }

    /// Read an answer to the offer: the handle it names, and the live session it
    /// addresses if it addresses one.
    ///
    /// **Four refusals, and none of them is a correction.** A handle jojobot
    /// could not have minted, one it is not holding, one that belongs to another
    /// identity, and one whose session is closed or gone from the board. Each is
    /// blocked in its own words, because a caller's next move differs in every
    /// case — and none is repaired into a nearby handle, which would be jojobot
    /// guessing which session somebody meant.
    async fn resumable(
        &self,
        bot: &EntityId,
        answer: &str,
        live: &[Session],
    ) -> Result<(sid::Sid, Option<Session>), CallToolResult> {
        if !sid::is_readable(answer) {
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' is not a handle jojobot mints — those are \
                     {} characters of 0-9 and a-z, with i, l, o and u left out because they read \
                     as one another. jojobot will not correct one, because correcting it means \
                     guessing which session you meant. Call start_here with your bot name and no \
                     resume to see what there is.",
                    jojobot_domain::session::SID_LEN,
                ),
            ));
        }
        let Some(held) = self.registry.lookup(answer) else {
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. That session is gone: '{answer}' is not a handle \
                     this jojobot is holding — a handle whose run never wrote a card has nothing \
                     to be recovered from. The work on the board is untouched and still \
                     readable. Call start_here with your bot name again and take the offer it \
                     makes."
                ),
            ));
        };
        if held.bot != *bot {
            return Err(handle_declined(
                answer,
                format!(
                    "No session was started. The handle '{answer}' belongs to {}, and a session \
                     is bound to its identity at boot and never switches. Boot as {} to pick it \
                     up, or call start_here as '{bot}' with no resume to see what is yours.",
                    held.bot, held.bot,
                ),
            ));
        }
        let handle = sid::Sid(answer.to_string());
        let Some(card) = held.card else {
            // Minted, never written under. Still theirs, still empty.
            return Ok((handle, None));
        };
        if let Some(session) = live.iter().find(|s| s.id == card) {
            return Ok((handle, Some(session.clone())));
        }
        // **Not among the live runs, so it stopped — and stopping is not the
        // end.** Reopening is what makes "resume last session" always work, and
        // it is bounded by nothing but the state the run reached: the offer's
        // age window governs what jojobot VOLUNTEERS, never what a handle
        // someone kept can still reach.
        match self.sessions.reopen(&card).await {
            Ok(session) => Ok((handle, Some(session))),
            // The one end that is the last word. Its story is already an entry
            // in the operator's Journal, and reopening the run would make a
            // published account retroactively false.
            Err(SessionError::Closed { state, .. }) => Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' addresses a session that is {state} — its \
                     story has been told, and it went into the operator's Journal as a dated \
                     entry. Reopening it would make that account false, so this end is the last \
                     word. Its chronology stands as the record of what happened. Call start_here \
                     with your bot name and no resume to begin the next run."
                ),
            )),
            Err(SessionError::UnknownSession { .. }) => Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' is a handle jojobot is holding, but the \
                     session it addresses is not on the board any more. Nothing was changed. Call \
                     start_here with your bot name and no resume to see what is there."
                ),
            )),
            // **Degrades the way the rest of a boot degrades**: nothing was
            // changed, the caller is told plainly, and the underlying fault
            // goes to the log where an operator reads it — rather than a 500
            // that says nothing about what happened to the session.
            Err(e) => {
                tracing::warn!(error = %e, session = %card, "a session could not be reopened");
                Err(handle_declined(
                    answer,
                    format!(
                        "No session was started, and nothing was changed. '{answer}' addresses a \
                         session that stopped, and jojobot could not reopen it: the session store \
                         refused. This is not something your call can fix by being different — \
                         try again, and if it persists a person has to look at the board."
                    ),
                ))
            }
        }
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

    /// Sweep this bot's stale sessions and hand back what is live —
    /// **the half of attaching that reads and writes the store.**
    ///
    /// **One caller: the boot.** This doc used to claim two — the boot, and a
    /// first write retrying an attach the boot could not make — and to explain
    /// how the two differed in what they did with the result. There is no such
    /// write path anywhere in the crate, and no test for one; the phrase
    /// survived only here. Whether it was removed or never built, describing a
    /// caller that does not exist sent every reader looking for it.
    ///
    /// Binding is the caller's job: this returns what it found.
    ///
    /// **Every live session, not the newest one.** A bot may have several runs
    /// at once — two devices, two pieces of work — so the boot's offer needs
    /// them all.
    async fn sweep_and_find(&self, bot: &EntityId) -> Result<Board, SessionError> {
        let now = jiff::Timestamp::now();
        let existing = self.sessions.sessions_of(bot).await?;

        let mut swept = Vec::new();
        for stale in existing.iter().filter(|s| s.is_stale(now)) {
            match self
                .sessions
                .close(&stale.id, SessionState::Abandoned)
                .await
            {
                Ok(_) => swept.push(stale.id.to_string()),
                // A sweep that cannot close one session must not stop a boot:
                // the session is left active and the next boot tries again.
                Err(e) => tracing::warn!(
                    error = %e, session = %stale.id,
                    "a stale session could not be swept — left active for the next boot"
                ),
            }
        }

        // Newest first already, so the first live one is the newest.
        let live: Vec<Session> = existing
            .iter()
            .filter(|s| !s.state.is_terminal() && !s.is_stale(now))
            .cloned()
            .collect();
        // **Read AFTER the sweep, and through it.** The run this boot just
        // marked `abandoned` is the archetypal "resume last session" — it is
        // the one that stopped yesterday — so it has to be a candidate here,
        // and the list jojobot is holding still says `active` for it.
        let offerable = existing
            .into_iter()
            .map(|s| match swept.contains(&s.id.to_string()) {
                true => Session {
                    state: SessionState::Abandoned,
                    ..s
                },
                false => s,
            })
            .find(|s| s.is_offerable(now));
        Ok(Board {
            live,
            offerable,
            swept,
        })
    }

    /// **The session this call writes to, resolved from the handle it carries.**
    ///
    /// One address, and no ladder. The old resolver had three — an explicit
    /// session id, a bot name resolved against the board, and the connection's
    /// binding — because none of them worked everywhere: the binding died with
    /// the connection, and a session id could not be used before the first write
    /// had minted one. The handle has neither problem. It exists from the moment
    /// the door hands it over, it rides every call, and it names exactly one
    /// run.
    ///
    /// **The card is still lazy.** A handle with no card behind it gets one
    /// here, on the first real write and never before, so a boot that does
    /// nothing still leaves nothing behind.
    async fn session_for(
        &self,
        // **Proof the gate is held.** This reads the registry, awaits a store
        // call and writes the registry back; two calls inside that span would
        // both find no card and both begin one. Taking the guard by reference
        // makes the requirement impossible to forget rather than a comment
        // somebody has to read.
        _serialized: &tokio::sync::MutexGuard<'_, ()>,
        caller: &Caller,
        explicit_focus: Option<&str>,
        derive_from: Option<&str>,
    ) -> Result<SessionId, McpError> {
        if let Some(card) = &caller.card {
            return Ok(card.clone());
        }
        // **The focus is DERIVED, and the entry is not touched.** A first write
        // is prose — a multi-line entry, a story, a line naming code in
        // backticks — and a focus is one line of display text. Feeding the one
        // to the other applied the focus's rules to text nobody offered as a
        // focus: the write failed with `invalid entry`, naming a parameter the
        // caller never passed, and the entry it was carrying was dropped.
        let focus = match explicit_focus.map(str::trim).filter(|f| !f.is_empty()) {
            Some(theirs) => theirs.to_string(),
            None => display_line(derive_from.unwrap_or(FRESH_FOCUS)),
        };
        let begun = self
            .sessions
            .begin(NewSession {
                bot: caller.bot.clone(),
                sid: caller.sid.clone(),
                focus,
                started_at: jiff::Timestamp::now(),
            })
            .await
            .map_err(session_error)?;
        // The registry learns the card here — this is the moment one exists.
        self.registry.attach_card(&caller.sid, begun.id.clone());
        Ok(begun.id)
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
    /// this for": a caller carrying no handle (jojobot will not guess which
    /// identity made a call), a session store that refuses, and a beat that
    /// fails to
    /// write. **A beat never fails the verb it is about.** A capture that landed
    /// did land; reporting it as failed because its footnote could not be
    /// written would make the record wrong in the more damaging direction.
    ///
    /// **That first case used to be every call on a client with no session
    /// affinity, and is not any more.** The verbs jojobot beats about — captures,
    /// entity writes, mailbox writes — carried no identity of their own, so the
    /// only one available to them was the connection's, and most clients open a
    /// fresh connection per tool call: for those clients the tally simply never
    /// appeared. The `sid` rides every verb now, so a caller that keeps passing
    /// it is beaten about wherever it writes, whatever its client does with
    /// connections. What is left in the first case is a caller carrying no
    /// `sid`, which is a caller that has not asked to be recorded anywhere.
    ///
    /// **A handle that is DEAD is not one of the silent cases**, and this used
    /// to swallow it along with them — the verb wrote, the chronology stopped,
    /// and nothing said so. That refusal is made before the write now, by
    /// [`Jojobot::attributable`]. What is left here is the sliver where a
    /// handle died between that check and this call, and silence is right for
    /// it: the write has already landed.
    async fn beat(&self, class: &'static str, example: &str, sid: Option<&str>) {
        // **No caller, no beat.** jojobot does not guess which identity made a
        // call, and an anonymous one is legitimate — a reader, a poster who
        // never booted. What it is not is somebody to record work against.
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let Some((_, phrase)) = BEAT_CLASSES.iter().find(|(known, _)| *known == class) else {
            // A class with no phrase would render a beat nothing can read back,
            // so it writes none at all rather than one that breaks the tally on
            // the next reconnect.
            tracing::warn!(
                class,
                "no beat phrase for this verb class — no beat written"
            );
            return;
        };
        let gate = self.registry.gate(&self.gate_key(sid));
        let _serialized = gate.lock().await;
        // Re-read the caller inside the gate: a racing write may have
        // materialized the card since, and beginning a second one here is the
        // fork this lock exists to prevent.
        let Ok(Some(caller)) = self.caller(Some(caller.sid.as_str())) else {
            return;
        };
        let Ok(session) = self
            .session_for(&_serialized, &caller, None, Some(phrase))
            .await
        else {
            return;
        };

        // **The tally is read back off the session, never cached.** It used to
        // live on the connection, which meant it died with one — and a
        // reconnect then appended a second beat for a class that already had
        // one. The session is where it lives, so the session is what it is read
        // from.
        let held = match self.sessions.read_session(&session).await {
            Ok(read) => beats_of(&read).get(class).cloned(),
            Err(e) => {
                tracing::warn!(error = %e, class, "a session could not be read for its tally");
                return;
            }
        };
        let outcome = match held {
            Some(mut beat) => {
                beat.count += 1;
                if beat.examples.len() < BEAT_EXAMPLES
                    && !beat.examples.iter().any(|e| e == example)
                {
                    beat.examples.push(example.to_string());
                }
                let text = beat_text(phrase, &beat);
                self.sessions
                    .amend_beat(&session, &beat.entry, &text, jiff::Timestamp::now())
                    .await
                    .map(|_| ())
            }
            None => {
                let beat = Beat {
                    entry: EntryId(String::new()),
                    count: 1,
                    examples: vec![example.to_string()],
                };
                let text = beat_text(phrase, &beat);
                self.sessions
                    .append(
                        &session,
                        NewEntry::beat(class, text, jiff::Timestamp::now()),
                    )
                    .await
                    .map(|_| ())
            }
        };
        if let Err(e) = outcome {
            tracing::warn!(
                error = %e, class, session = %session,
                "a session beat could not be written — the verb it is about still succeeded"
            );
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
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
            // The tool surface is unchanged this milestone: parentage is
            // reachable only from inside, so every write through the door is
            // a root.
            parent: None,
            boot: args
                .boot
                .as_deref()
                .map_or(Default::default(), jojobot_domain::memory::Boot::from_token),
            create_new: args.create_new.unwrap_or(false),
        };
        match self.memory.add_entity(new).await.map_err(memory_error)? {
            Guarded::Written(entity) => {
                self.beat("add_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
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
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
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
        let entities = self
            .memory
            .list_entities(kind)
            .await
            .map_err(memory_error)?;
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
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
                self.beat("update_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
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
                self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
                    .await;
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let patch = FactPatch {
            content: args.content,
            details: args.details,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
            confirmed_by_user: args.confirmed_by_user.unwrap_or(false),
            edge: parse_edge(args.shape.as_deref(), args.object.as_deref())?,
        };
        let written = match self.memory.update_fact(&address, patch).await {
            Ok(written) => written,
            Err(e) => return memory_declined("update_fact", e),
        };
        match written {
            Guarded::Written(fact) => {
                self.beat(
                    "update_fact",
                    &fact.address().to_string(),
                    args.sid.as_deref(),
                )
                .await;
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let name = MailboxName(args.name.trim().to_string());
        match self
            .mailboxes
            .create_mailbox(&name, args.create_new.unwrap_or(false))
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(created) => {
                self.beat("create_mailbox", created.name.as_str(), args.sid.as_deref())
                    .await;
                json_result(&mailbox_json(&created))
            }
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::Creating,
            )),
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
                       concluding it was never sent, and say what you find. COUNTS ARE FOR YOUR \
                       OWN BOXES: the `sid` you pass says which bot is asking, and the boxes that \
                       bot owns come back with their per-state counts; every other box comes back \
                       as a NAME ONLY, marked `yours: false`. You can still see that a box \
                       EXISTS — which is what you need to post into it — but not what is waiting \
                       in somebody else's, because that is their queue to work and not yours to \
                       weigh. Call without a `sid` and you own nothing — exactly right for a \
                       caller that only posts."
    )]
    async fn list_mailboxes(
        &self,
        Parameters(args): Parameters<ListMailboxesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let named = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller.map(|c| c.bot),
            Err(refused) => return Ok(refused),
        };
        let mine = self.boxes_drained_by(named.as_ref()).await?;
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?;
        let body = serde_json::json!({
            "count": boxes.len(),
            "counts_shown_for": mine.shown_for(&boxes),
            "ownership_known": mine.known,
            "note": mine.note(),
            "mailboxes": boxes
                .iter()
                .map(|b| {
                    if mine.covers(b.name.as_str()) {
                        let mut body = mailbox_json(b);
                        if let Some(obj) = body.as_object_mut() {
                            obj.insert("yours".into(), mine.drains(b.name.as_str()).into());
                        }
                        body
                    } else {
                        // **Existence, not state.** The name is what a writer
                        // needs; the counts are what posed "is that one mine?"
                        // Quarantine stays: it is a fault on the board rather
                        // than somebody's queue, and this listing is the only
                        // place it shows.
                        serde_json::json!({
                            "name": b.name.as_str(),
                            "yours": false,
                            "counts": serde_json::Value::Null,
                            "counts_elided": true,
                            "quarantined": quarantined_json(b),
                        })
                    }
                })
                .collect::<Vec<_>>(),
        });
        json_result(&body)
    }

    /// The boxes a caller drains — the ones whose state is theirs to see.
    ///
    /// **Whose box a read opens — resolved from the handle, never named.**
    ///
    /// Reading IS delivery: a name in the caller's hand is a way to move
    /// somebody else's mail out of `new` and make it theirs-no-longer. The
    /// own-box norm was stated in the essay in the strongest words available
    /// and was still only advice for as long as the parameter sat there. The
    /// `sid` already says whose box it is, so the parameter is gone and the
    /// norm is structural.
    ///
    /// **Posting keeps its name, deliberately.** `post_message` reaches
    /// somebody else's box and writes without reading, which is exactly the
    /// shape of a request — and is the way forward this refusal points at. The
    /// asymmetry is the design.
    ///
    /// Four ways to have no box, and they are not one answer: the caller has no
    /// identity, jojobot cannot read who owns what, the bot claims nothing, or
    /// it claims a box nobody has opened. Each needs a different next move.
    async fn my_box(&self, sid: Option<&str>) -> Result<MailboxName, CallToolResult> {
        let caller = match self.caller(sid) {
            Ok(Some(caller)) => caller,
            Ok(None) => return Err(no_box_for("", NoBox::Anonymous)),
            Err(refused) => return Err(refused),
        };
        // **A world that is down is not an answer of "no".** Ownership is a
        // read of Memory, so an outage means jojobot cannot say whose box this
        // is — and opening none while implying the bot owns none would send a
        // caller off to mint a box it already has.
        let Ok(index) = self.memory.list_entities(None).await else {
            return Err(no_box_for(caller.bot.as_str(), NoBox::Unknowable));
        };
        let claim = index
            .iter()
            .find(|e| e.id == caller.bot)
            .and_then(|e| e.mailbox.clone());
        let Some(claim) = claim else {
            return Err(no_box_for(caller.bot.as_str(), NoBox::Unclaimed));
        };
        let name = MailboxName(claim.trim().to_string());
        // **Reported missing, never opened.** The same rule the boot door
        // keeps: creation is an intentional act, and `create_mailbox` is the
        // only mint. A read that minted the box it was about to drain would be
        // the side-effect creation this surface exists to forbid.
        match self.mailboxes.list_mailboxes().await {
            Err(_) => Err(no_box_for(caller.bot.as_str(), NoBox::Unknowable)),
            Ok(boxes) if boxes.iter().any(|b| b.name == name) => Ok(name),
            Ok(_) => Err(no_box_for(name.as_str(), NoBox::Unopened)),
        }
    }

    /// **Ownership is a read of Memory, never an ACL**: a bot owns a box by
    /// carrying a `mailbox:` claim on its own record, so this asks the entity
    /// index rather than anything mailbox-side. A caller that names no bot, and
    /// whose connection carries no identity either, drains nothing — which is
    /// the right answer for a pure sender, and for an anonymous `start_here`.
    async fn boxes_drained_by(&self, named: Option<&EntityId>) -> Result<Ownership, McpError> {
        // **A world that is down is not an answer of "no".** Ownership is a
        // read of Memory, so an outage means jojobot cannot say what anybody
        // drains — and rendering that as "not yours" tells every bot its own
        // queue belongs to somebody else, which is a claim nobody can act on.
        match self.memory.list_entities(None).await {
            Ok(index) => Ok(self.ownership_of(&index, named)),
            Err(_) => Ok(Ownership::unknown()),
        }
    }

    /// The same answer, off an index the caller already has — so a boot reads
    /// the entity index once for the whole payload instead of three times.
    fn ownership_of(&self, index: &[Entity], named: Option<&EntityId>) -> Ownership {
        // **Whoever the caller says they are, and nobody by default.** There is
        // no connection to fall back to any more: a caller with no handle owns
        // nothing, which is exactly right for one that only posts.
        let bot = named.cloned();
        // **An unclaimed box has no queue to protect.** The scoping shields a
        // DRAINER's workload from everybody else; a box nobody owns has no
        // drainer, and hiding its counts from every caller alive would leave a
        // box that `read_mailbox` empties freely and nothing can report on —
        // which is also what falsified the polling advice pointing at this verb.
        let claimed: Vec<String> = index.iter().filter_map(|e| e.mailbox.clone()).collect();
        let Some(bot) = bot else {
            return Ownership::known(Vec::new(), claimed);
        };
        Ownership::known(
            index
                .iter()
                .find(|e| e.id == bot)
                .and_then(|e| e.mailbox.clone())
                .into_iter()
                .collect(),
            claimed,
        )
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
                       someone has it. The sender is not yours to declare: jojobot records the \
                       bot behind the `sid` you pass, so a reply can always find you and nothing \
                       can be posted under somebody else's name. A `sid` jojobot is not holding \
                       comes back status: blocked and nothing is written. YOUR BODY IS NOT \
                       ECHOED BACK — you wrote it, and \
                       jojobot verified it by reading the stored card back, so the answer carries \
                       the id, the state and body_bytes with body_elided: true rather than the \
                       text. `list_sent` with include_bodies returns it and takes no delivery. \
                       `in_reply_to` links this message to the one it \
                       answers: optional, it must name a message that exists (a miss comes back \
                       blocked, nothing written), and it says only that the two are one exchange \
                       — it does not deliver the original, handle it, or oblige anybody."
    )]
    async fn post_message(
        &self,
        Parameters(args): Parameters<PostMessageArgs>,
    ) -> Result<CallToolResult, McpError> {
        // **The sender is derived, never declared.** It was a free-text field
        // recorded exactly as claimed, which made every "who left this?" answer
        // only as good as the caller's honesty and their memory of what they
        // called themselves last time. The handle says who is asking, so the
        // handle says who sent it.
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let new = NewMessage {
            mailbox: MailboxName(args.mailbox.trim().to_string()),
            body: args.body,
            subject: args.subject,
            sender: caller.bot.as_str().to_string(),
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
                self.beat("post_message", message.mailbox.as_str(), Some(&args.sid))
                    .await;
                json_result(&message_receipt_json(
                    &message,
                    "you wrote this body; jojobot verified it by reading the stored card back. \
                     list_sent with include_bodies: true returns it, and takes no delivery",
                ))
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
        description = "Take delivery of everything unprocessed in YOUR OWN mailbox, oldest \
                       first, moving each message from `new` to `read`. There is no peek: \
                       reading IS taking delivery. WHICH BOX IS NOT AN ARGUMENT — the `sid` you \
                       pass says which bot is asking, and a bot reads the box it owns, full \
                       stop. Reading somebody else's would move their mail out of `new` and \
                       make it no longer waiting for them; to reach another box, post_message \
                       writes into it without reading it, which is the shape of a request. No \
                       box to open comes back status: blocked, saying which kind of nothing it \
                       found — no sid, no claim, or a claim nobody has opened — and delivers \
                       nothing. Messages a previous read already handed over come back too, \
                       flagged seen_before: true — leftovers from an interrupted earlier read, \
                       not fresh mail. A message somebody else finished while this delivery was \
                       in flight is left out, so a delivery can be smaller than counts you saw a \
                       moment ago. Act on what you receive, then call \
                       mark_processed for each. Draining a whole box makes every message in it \
                       yours to finish — use read_message when you want only one. ONLY CHECKING \
                       WHETHER ANYTHING IS WAITING? Use list_mailboxes — it reads counts without \
                       taking delivery, so a poll that finds an empty box costs nothing and owes \
                       nothing. BY DEFAULT you get bodies for the messages nobody has taken yet: \
                       leftovers still come back, still counted, still flagged and still owed, \
                       but with their bodies left out (body_elided: true, plus body_bytes and the \
                       opening line) — because you were handed those bodies once already. Pass \
                       new_only: false to get them back, which is the read for a consumer \
                       recovering from a crash that no longer holds what it was given. Either \
                       way it changes what is SHIPPED, never what is owed."
    )]
    async fn read_mailbox(
        &self,
        Parameters(args): Parameters<ReadMailboxArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = match self.my_box(args.sid.as_deref()).await {
            Ok(name) => name,
            Err(refused) => return Ok(refused),
        };
        // **The safe branch is the default.** The cheap, common read is a poll
        // for news; re-shipping a body its reader already has is the expensive
        // case, and a caller that follows defaults rather than prose must land
        // on the conservative one. Nothing goes silent either way — a leftover
        // is still delivered, counted, flagged and owed.
        let new_only = args.new_only.unwrap_or(true);
        match self
            .mailboxes
            .read_mailbox(&name)
            .await
            .map_err(mailbox_error)?
        {
            mailbox::Guarded::Written(delivery) => json_result(&delivery_json(&delivery, new_only)),
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
                       nothing leaves nothing behind. PASS `sid` — the session id the boot door \
                       gave you — ON EVERY CALL; it is the only address, and it is what tells \
                       jojobot which bot is writing. A `sid` whose session is closed comes back \
                       status: blocked: a closed session takes no more entries, whichever end it \
                       reached. The two ends part company on what comes NEXT — a run that stopped \
                       without being wrapped up is offered back at your next boot, and resuming \
                       it continues this same record, while a wrapped one is the last word, its \
                       story already a dated entry in the operator's Journal, so carrying on \
                       there means a fresh session."
    )]
    async fn journal(
        &self,
        Parameters(args): Parameters<JournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let focus = args.focus.as_deref();
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        // Resolved inside the gate: a racing write may have materialized this
        // session's card since, and beginning a second one is the fork the lock
        // exists to prevent.
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let session = self
            .session_for(&_serialized, &caller, focus, Some(&args.entry))
            .await?;
        let entry = match self
            .sessions
            .append(
                &session,
                NewEntry::manual(args.entry, jiff::Timestamp::now()),
            )
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
                       too. Pass your `sid` on every call — it is the address, and it survives \
                       the fresh connection most clients open per tool call. This verb never \
                       STARTS a session: there is nothing to amend in one that does not exist \
                       yet."
    )]
    async fn amend_journal(
        &self,
        Parameters(args): Parameters<AmendJournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **No lazy begin here, deliberately.** There is nothing to amend in a
        // session that has not been written yet, and minting a card to hold a
        // correction would be a card created by the one verb whose whole job is
        // to add nothing. A handle with no card behind it is told exactly that,
        // rather than "no such session" — the handle is real, the run simply has
        // not started writing.
        let Some(session) = caller.card else {
            return Ok(session_nothing_to_amend());
        };
        // The guard exists to be held across the amend, not merely taken.
        let _ = &_serialized;
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
                       boot of the same identity sweeps it to `abandoned` after a day, its \
                       chronology stays readable, and the run itself can be picked up again — but \
                       its story was never told, and that is the difference between the two \
                       endings. Pass your `sid` on every call. When the work continues but this \
                       run has gotten long, wrapping is also how you ROTATE: wrap the story, then \
                       boot again for a fresh sid."
    )]
    async fn wrap_session(
        &self,
        Parameters(args): Parameters<WrapSessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // A run that never wrote anything can still tell its story: the card is
        // created here, exactly as a first journal entry would create it, so
        // "I booted, did the work elsewhere, and I am done" is not a dead end.
        let session = self
            .session_for(&_serialized, &caller, None, Some(&args.story))
            .await?;

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
            Ok(read) => read
                .entries
                .iter()
                .rev()
                .find(|e| !e.is_auto() && e.text == story)
                .cloned(),
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

        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
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
        // **The handle outlives the run it named, and stops addressing it.** The
        // registry keeps the mapping — re-issuing a wrapped run's handle would
        // send somebody's next call into an archive — so nothing is removed
        // here. What changes is what the store will accept: `wrapped` is the
        // last word, and every later write on this handle comes back blocked in
        // those words.
        //
        // A bot that wraps and keeps working boots again for a fresh handle,
        // which is the rotation the description names.
        json_result(&serde_json::json!({
            "session": session_json(&wrapped),
            "entry": entry_json(&entry),
            "journal": journalled,
        }))
    }

    /// What a sender has sent, and where it got to — without touching any of it.
    #[tool(
        description = "See the mail YOU have sent and where it got to — read-only, and it moves \
                       NOTHING: no state changes, nobody's delivery is taken, and the messages \
                       stay exactly as owed as they were. It answers whether something you sent \
                       arrived and whether anyone has read it — questions every other verb could \
                       only answer by taking delivery of the box you posted into. A `mailbox` \
                       that names no box comes back status: blocked with candidates, never an \
                       empty list, because an empty list would read as 'it never arrived'. Cards \
                       jojobot cannot read as messages are reported separately under \
                       `unreadable`: it cannot tell who sent them, so one of yours could be \
                       there. Newest first, each with its \
                       state (`new` = nobody has picked it up · `read` = delivered, not yet \
                       finished with · `processed` = acted on) plus notes when the consumer \
                       recorded an outcome. Bodies are left out unless you ask for them — you \
                       wrote them — so each carries body_bytes and the opening line instead, and \
                       says body_elided: true rather than leaving you to guess. OMIT `sender` for \
                       your own mail — your `sid` already says who that is. Pass one to ask after \
                       somebody else's outgoing mail: it is matched exactly against the bot \
                       handle recorded on each message (`bot:gamma`), which is allowed, because \
                       where a message got to is not private to its sender."
    )]
    async fn list_sent(
        &self,
        Parameters(args): Parameters<ListSentArgs>,
    ) -> Result<CallToolResult, McpError> {
        // **Your own mail by default.** The sender is derived from the handle
        // now, so the caller does not have to remember what they called
        // themselves — and asking after somebody else's is still allowed,
        // because where a message got to is not private to its writer.
        let caller = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let declared = args
            .sender
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let own = caller.as_ref().map(|c| c.bot.as_str().to_string());
        let Some(sender) = declared.map(str::to_string).or(own) else {
            return Ok(session_unbound());
        };
        let sender = sender.as_str();
        let only = args
            .mailbox
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty());
        let bodies = args.include_bodies.unwrap_or(false);

        // **A named box must exist, exactly as it must for every other verb
        // that names one.** Without this a typo answered `count: 0` — and this
        // verb's whole job is answering "did my report land", so a mistyped box
        // says "no" and the sender posts it again. The near-miss screen is the
        // read-side twin of "a typo must never mint a box".
        if let Some(name) = only {
            let name = MailboxName(name.to_string());
            let known = self
                .mailboxes
                .list_mailboxes()
                .await
                .map_err(mailbox_error)?;
            let names: Vec<MailboxName> = known.iter().map(|b| b.name.clone()).collect();
            if let mailbox::guard::Decision::Block(candidates) =
                mailbox::guard::decide_existing(&name, &names)
            {
                return Ok(mailbox_blocked(
                    &name,
                    &candidates,
                    BlockedBox::MustExist("list_sent"),
                ));
            }
        }

        // Built on the scan, which is the one read that moves nothing: it is
        // how the search projection is rebuilt, and its "nothing moves" is
        // pinned by the shared contract on every tier.
        let mut sent: Vec<Message> = self
            .mailboxes
            .scan_messages()
            .await
            .map_err(mailbox_error)?
            .into_iter()
            .filter(|m| m.sender.trim() == sender)
            .filter(|m| only.is_none_or(|name| m.mailbox.as_str() == name))
            .collect();
        // **The tie breaks on the id as a NUMBER.** Ids are a decimal counter,
        // so ordering them as text puts `9` after `10` — the same trap the
        // board read and the fake both avoid deliberately.
        let minted = |id: &MessageId| id.as_str().parse::<u64>().unwrap_or(u64::MAX);
        sent.sort_by(|a, b| {
            b.sent_at
                .cmp(&a.sent_at)
                .then_with(|| minted(&b.id).cmp(&minted(&a.id)))
        });

        // **A card jojobot cannot read is not a message that was never sent.**
        // The scan leaves quarantined cards out — it cannot parse them, so it
        // has nothing to return — and this verb answers "did my report land".
        // Staying silent about them means the honest answer ("something is
        // wrong with a card here") arrives as a confident "no". Their senders
        // are unreadable too, so they cannot be filtered to this caller; the
        // count is reported per box and the ids are named.
        let unreadable: Vec<serde_json::Value> = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?
            .iter()
            .filter(|b| only.is_none_or(|name| b.name.as_str() == name))
            .filter(|b| !b.quarantined.is_empty())
            .map(|b| {
                serde_json::json!({
                    "mailbox": b.name.as_str(),
                    "card_ids": b.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect();

        json_result(&serde_json::json!({
            "sender": sender,
            "mailbox": only,
            "count": sent.len(),
            "unreadable": unreadable,
            "unreadable_note": "Cards jojobot cannot read as messages are not in the list above — \
                                it cannot tell who sent them. If one of yours is missing, it may \
                                be here: a person has to repair the card on the board.",
            "messages": sent
                .iter()
                .map(|m| if bodies {
                    message_json(m)
                } else {
                    message_receipt_json(
                        m,
                        "call list_sent again with include_bodies: true — this is your own \
                         message, so reading it takes no delivery from anybody",
                    )
                })
                .collect::<Vec<_>>(),
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
                       message whose handling failed has still been handled. When a message asks \
                       nothing of you — its whole content is known to you once you have read it \
                       — READING IT IS THE ACTING, so process it with a note and move on; the \
                       order matters for work you still owe, not for work that was never owed. \
                       Write the outcome you actually have: a note \
                       longer than the card holds is CUT to fit and says so (a trailing ellipsis, \
                       and notes_truncated: true), never refused — the verb that retires a \
                       message will not fail over the length of its own record. The answer \
                       confirms the move — state, notes, id — WITHOUT echoing the message's body \
                       back at you, since the read that handed it over already gave you that; it \
                       carries body_bytes and body_elided: true instead, and read_message returns \
                       the text unchanged for a processed message. A message can be \
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
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let id = MessageId(args.message_id.trim().to_string());
        // What the caller asked to record, blank-is-absent.
        let asked = args
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        match self
            .mailboxes
            .mark_processed(&id, args.notes.as_deref())
            .await
        {
            Ok(processed) => {
                self.beat("mark_processed", processed.id.as_str(), args.sid.as_deref())
                    .await;
                let mut body = message_receipt_json(
                    &processed,
                    "you had this body from the read that handed it to you. read_message returns \
                     it, and a processed message comes back unchanged — processed is terminal",
                );
                if let Some(obj) = body.as_object_mut() {
                    // **Always present, never inferred from the ellipsis.** The
                    // record can legitimately end in one, and a reader that has
                    // to guess whether a store cut its text is a reader that
                    // will eventually guess wrong.
                    //
                    // **Only a record this call OFFERED can have been cut.**
                    // Both stores carry a pre-existing note forward when the
                    // caller supplies none, and nothing gates re-processing, so
                    // comparing unconditionally made a second call report a cut
                    // of a record it never sent — the same wrong inference,
                    // pointing the other way.
                    obj.insert(
                        "notes_truncated".into(),
                        asked
                            .is_some_and(|asked| processed.notes.as_deref() != Some(asked))
                            .into(),
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
        Hit::Entity {
            entity,
            doc_id,
            edges,
        } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("doc".into(), doc_id.clone().into());
                obj.insert("edges".into(), edges.iter().map(edge_json).collect());
            }
            body
        }
        Hit::Fact {
            fact,
            subject,
            home,
        } => {
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
        let Some(class) = entry.beat.as_deref() else {
            continue;
        };
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
        "how_to_proceed": "Nothing was written. This call carried no `sid`, and jojobot will not \
                           guess which session is writing. Call start_here with your bot name to \
                           get one, then pass it on every call — reads included. It is the only \
                           address, and it is what tells jojobot which bot is asking: most \
                           clients open a fresh connection per tool call, so nothing about who \
                           you are survives from your last one.",
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A session handle that addresses nothing.** The guards' own shape, so a
/// caller branches on `status` here exactly as everywhere else — and `wrote:
/// false` says the thing that matters most: a boot jojobot refused started no
/// session, so nothing on the board moved.
fn handle_declined(attempted: &str, how_to_proceed: String) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// An amend on a session that has not begun. Refused rather than turned into a
/// first entry.
fn session_nothing_to_amend() -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        // **True of both ways to get here.** A bot with no session at all has
        // nothing written yet; a bot whose last session was wrapped or swept
        // has a record that is closed and no longer amendable. Saying "not even
        // written to disk" was false for the second, and it sent a caller
        // looking for entries that are sitting right there, closed.
        "how_to_proceed": "Nothing was written. There is no OPEN session to amend: either this \
                           identity has not written anything yet — a session's record begins on \
                           its first beat — or its last session is closed, and closed is \
                           terminal both ways. Use journal to begin the next one; its first \
                           entry is what brings the record into being. To read a closed \
                           session's chronology, booting as this identity through start_here \
                           reports its state.",
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
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    };
    match e {
        SessionError::UnknownSession { attempted } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. No session on jojobot's board has the id '{attempted}'. \
                 Ids are minted by jojobot and handed back by start_here when you boot as your \
                 identity — use the sid it gives you rather than composing one."
            ),
        ),
        // **The two ends part company here, because the way forward does.** One
        // paragraph for both used to tell the owner of a run that merely
        // stopped that their work belonged to a new session — which is advice
        // to fork the very thing they were trying to continue.
        SessionError::Closed {
            attempted,
            state: SessionState::Abandoned,
        } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' is abandoned — it stopped without \
                 being wrapped up, so it takes no write as it stands. That is not a failure and \
                 not the end of it: resume it. Call start_here with your bot name, and either \
                 take it from the offer or pass resume with its sid — it reopens where it left \
                 off and its chronology continues."
            ),
        ),
        SessionError::Closed { attempted, state } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' is {state} — its story has been told, \
                 and it went into the operator's Journal as a dated entry. Reopening it would \
                 make that account false, so this end is the last word. Its chronology stands as \
                 the record of what happened. If there is more to say, it belongs to a new \
                 session: boot again (or rotate) and start_here mints one."
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
        SessionError::Stranded { .. } | SessionError::Store(_) | SessionError::NotConfigured(_) => {
            McpError::internal_error(e.to_string(), None)
        }
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
        return excluded(
            "you passed include_mail: false, so messages were left out of this answer.",
        );
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

/// **The boot door's own refusal: the roster, and an offer.**
///
/// It used to reuse the generic absence gate — "nothing resembles it, call
/// add_entity first" — and that answer is wrong here in two ways. Its candidate
/// list is near misses only, so a name that resembles nothing came back with an
/// empty list, which reads as a broken server rather than as "you are not one
/// of these"; and its advice sends a caller who has no identity off to make one
/// through a verb that needs a session it does not have.
///
/// So this says what a caller in that position actually needs: here is who
/// exists, boot as one of them, and create the identity you wanted from inside
/// that session. **The door itself mints nothing** — creation is an intentional
/// act, and it happens through the verb that is for it, from a session that can
/// answer for it.
fn booting_unknown(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    index: &[Entity],
) -> CallToolResult {
    let roster: Vec<&str> = index
        .iter()
        .filter(|e| e.id.as_str().starts_with("bot:"))
        .map(|e| e.id.as_str())
        .collect();
    let how_to_proceed = if roster.is_empty() {
        format!(
            "Nothing was written and no session was started. '{attempted}' is not a bot jojobot \
             knows, and there are no bots on this server at all yet. Call start_here with no bot \
             for the world and the snapshot, then add_entity with kind `bot` to create the first \
             identity — this door mints nothing."
        )
    } else {
        format!(
            "Nothing was written and no session was started. '{attempted}' is not a bot jojobot \
             knows. The identities that exist are: {}. Boot as one of these and create \
             '{attempted}' from inside that session — this door mints nothing.",
            roster.join(", "),
        )
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        // **The roster, not only the near misses.** `candidates` answers "did
        // you mean one of these"; it is empty whenever nothing resembles the
        // name, and that is exactly the caller who most needs to be told who
        // does exist.
        "bots": roster,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
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
        "quarantined": quarantined_json(mailbox),
    })
}

/// Which boxes a caller drains, and **whether jojobot could tell**.
///
/// The two are separate answers on purpose. "You drain none of these" and
/// "jojobot cannot read the store that says which you drain" produce the same
/// listing and mean opposite things, and a caller acts on both.
struct Ownership {
    /// The boxes this caller drains. Empty when they drain none — or when
    /// nothing could be read, which is why `known` exists beside it.
    mine: Vec<String>,
    /// Every box some bot has claimed. What is NOT in here is drained by
    /// nobody, and a box with no drainer has no queue to shield.
    claimed: Vec<String>,
    /// Whether the ownership read succeeded at all.
    known: bool,
}

impl Ownership {
    fn known(mine: Vec<String>, claimed: Vec<String>) -> Self {
        Ownership {
            mine,
            claimed,
            known: true,
        }
    }

    fn unknown() -> Self {
        Ownership {
            mine: Vec::new(),
            claimed: Vec::new(),
            known: false,
        }
    }

    /// Whether this caller drains this box — the ownership question.
    fn drains(&self, name: &str) -> bool {
        self.known && self.mine.iter().any(|m| m == name)
    }

    /// Whether this box's counts are this caller's to see — a **different**
    /// question from whether they drain it: one they drain, or one nobody
    /// drains, since a box with no drainer has no queue to shield. Never true
    /// when ownership could not be read; an unknown is not a yes.
    fn covers(&self, name: &str) -> bool {
        self.known && (self.drains(name) || !self.claimed.iter().any(|c| c == name))
    }

    /// Which of the boxes actually on the board this answer counted.
    ///
    /// **Derived from the listing, never from the claim.** A bot can carry a
    /// `mailbox:` claim on a box nobody ever created, and naming that in
    /// `counts_shown_for` would point a reader at a box not in the list beside
    /// it.
    fn shown_for(&self, boxes: &[Mailbox]) -> Vec<String> {
        boxes
            .iter()
            .map(|b| b.name.as_str())
            .filter(|name| self.covers(name))
            .map(str::to_string)
            .collect()
    }

    /// The clause that says what this listing's counts mean, including when it
    /// cannot say.
    fn note(&self) -> &'static str {
        if self.known {
            "Counts are shown for the boxes you drain, and for any box nobody drains. A box \
             somebody else works is listed by name only — it exists and you can post into it; \
             what is waiting in it belongs to whoever works it."
        } else {
            "OWNERSHIP IS UNKNOWN: the store that records which boxes you drain could not be \
             read, so no counts are shown for any box — including your own. This is jojobot \
             being unable to tell, NOT a statement that you drain nothing."
        }
    }
}

/// The cards on a box that jojobot cannot read as messages.
///
/// **Rendered apart from the counts, because it is scoped differently.** Counts
/// are a queue and belong to whoever drains it; an unreadable card is a fault
/// on the board that no verb can act on, and the caller who most needs to see
/// it is a sender — somebody who does not drain this box, and who would
/// otherwise read the silence as "my message was never sent".
fn quarantined_json(mailbox: &Mailbox) -> serde_json::Value {
    serde_json::json!({
        "count": mailbox.quarantined.len(),
        "card_ids": mailbox.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
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

/// A message **without its body shipped back** — the whole record otherwise,
/// plus enough of the body to recognize which message this is.
///
/// **Eliding is never silent.** `body_elided` is always present and always
/// true here, `body_bytes` is the exact size of what is stored, and
/// `how_to_read` names the verb that hands the body over. A reader that has to
/// infer from a missing key whether a body was withheld or empty is a reader
/// that will eventually infer wrong.
///
/// The write is still verified: the store's read-back invariant means a body
/// that did not survive storage is an error rather than a success with mangled
/// bytes, so what the full echo used to prove is proven server-side. What the
/// echo added was shipping a 4-8 KB report back to the one caller who wrote it.
fn message_receipt_json(message: &Message, how_to_read: &str) -> serde_json::Value {
    let mut body = message_json(message);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("body".into(), serde_json::Value::Null);
        obj.insert("body_elided".into(), true.into());
        obj.insert("body_bytes".into(), message.body.len().into());
        obj.insert(
            "body_head".into(),
            text::BODY_DIGEST.render(&message.body).into(),
        );
        obj.insert("how_to_read".into(), how_to_read.into());
    }
    body
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
///
/// **`new_only` changes what is shipped, never what is owed.** Every message
/// the delivery covers is here either way, counted and flagged the same, and
/// every one of them still has to be marked processed — the crash contract is
/// exactly as it was. What it drops is the BODIES of the leftovers, which is
/// the whole cost of polling a box that holds a message somebody is
/// deliberately keeping open: the report stays unprocessed on purpose until its
/// round closes, and every poll in between was re-shipping it in full.
///
/// The elision is announced per message rather than once for the delivery,
/// because a reader walking the list must not have to remember a flag from the
/// envelope to know what it is looking at.
fn delivery_json(delivery: &Delivery, new_only: bool) -> serde_json::Value {
    serde_json::json!({
        "mailbox": delivery.mailbox.as_str(),
        "count": delivery.messages.len(),
        "new_only": new_only,
        "messages": delivery
            .messages
            .iter()
            .map(|d| if new_only && d.seen_before {
                let mut body = message_receipt_json(
                    &d.message,
                    "an earlier read already handed you this one. read_message returns it in \
                     full, or read_mailbox without new_only",
                );
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("seen_before".into(), true.into());
                }
                body
            } else {
                delivered_json(d)
            })
            .collect::<Vec<_>>(),
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
/// Why a read had no box to open. Four states, four different next moves — one
/// generic miss would be advice that fits none of them.
enum NoBox {
    /// No handle, so no identity, so no box.
    Anonymous,
    /// A world that is down. jojobot does not know, which is not the same as
    /// "you own none" and must never be rendered as it.
    Unknowable,
    /// A bot carrying no `mailbox:` claim — an identity that cannot receive
    /// mail yet.
    Unclaimed,
    /// A claim nobody has opened. Said plainly, with the mint named; never
    /// created here.
    Unopened,
}

/// The refusal a read gets when there is no box behind its handle.
fn no_box_for(attempted: &str, why: NoBox) -> CallToolResult {
    let how_to_proceed = match why {
        NoBox::Anonymous => {
            "Nothing was delivered. This call carried no `sid`, and a read opens the box of \
             whoever is asking — so jojobot has nobody to open one for. Call start_here with \
             your bot name to get a handle, then pass it on every call. To leave mail in \
             somebody else's box you do not need one of your own: post_message writes without \
             reading."
                .to_string()
        }
        NoBox::Unknowable => {
            "Nothing was delivered, and nothing is wrong with your call. Which box you drain is \
             a read of Memory, and that world is not reachable right now — so jojobot cannot \
             say whose box this is rather than saying you have none. Try again; if it persists \
             a person has to look."
                .to_string()
        }
        NoBox::Unclaimed => format!(
            "Nothing was delivered. '{attempted}' owns no mailbox, so there is nothing for it to \
             drain — an identity that cannot receive mail yet, which is a normal thing to be. \
             Give it one with update_entity naming the box it should own, and open that box with \
             create_mailbox if nobody has yet. Posting into other boxes needs none of this: \
             post_message writes without reading."
        ),
        NoBox::Unopened => format!(
            "Nothing was delivered, and nothing was created. Your bot claims the mailbox \
             '{attempted}' and no such box exists — the claim is a name, not a box, and only \
             create_mailbox mints one, deliberately and with the near-miss screen. Open it and \
             read again."
        ),
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

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
                format!(
                    "The addresses that do exist here are: {}.",
                    nearest.join(", ")
                )
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
        MailboxError::Stranded { .. } => {
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

/// The identity a session verb was told to write as, if it was told one.
///
/// Blank is absent rather than an error: a client that sends `bot: ""` meant to
/// send nothing, and refusing the whole call over an empty string would be the
/// second-worst way to answer.
fn named_bot(name: Option<&str>) -> Result<Option<EntityId>, McpError> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some(name) => bot_id(name).map(Some),
    }
}

/// Read a bot handle off a name. A bare name is a bot here — this is the bot
/// door, so a bare slug is read with the bot kind on it — and a handle of
/// another kind is a client error rather than a silent winner: booting a person
/// as an identity would hand somebody's page back as a charter.
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
    match (
        shape.map(str::trim).filter(|s| !s.is_empty()),
        object.map(str::trim).filter(|s| !s.is_empty()),
    ) {
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
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Boot, Edge, EdgeShape, EntityKind, FactId};
    use jojobot_domain::session::Sid;
    use jojobot_domain::session::testing::InMemorySessions;

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
            Arc::new(sid::SessionRegistry::new()),
        )
    }

    /// A handler whose search port is a spy the test keeps a handle on.
    fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            spy,
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
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
            sid: None,
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
            sid: None,
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
                sid: None,
            }))
            .await
            .expect("add_entity call ok");
    }

    /// [`ensure`], attributed to a session. Beats are written for whoever the
    /// handle names, so a spec about the tally has to say who is calling.
    async fn ensure_as(jojobot: &Jojobot, sid: &str, handle: &str) {
        let id = EntityId::person(handle);
        let kind = id.kind().expect("test handles are well-formed");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(sid.to_string()),
                ..add_args(kind.as_token(), id.slug(), id.slug())
            }))
            .await
            .expect("add_entity call ok");
    }

    /// [`capture_ok`], attributed to a session — same reason.
    async fn capture_as(jojobot: &Jojobot, sid: &str, args: CaptureArgs) -> serde_json::Value {
        capture_ok(
            jojobot,
            CaptureArgs {
                sid: Some(sid.to_string()),
                ..args
            },
        )
        .await
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
        assert_eq!(
            body["wrote"], false,
            "a blocked write says so in the body: {body}"
        );
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
            sid: None,
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
            sid: None,
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
                sid: None,
            }))
            .await
            .expect("search ok");

        let query = spy.query();
        assert_eq!(query.terms(), Some("winter"));
        assert!(
            !query.include_mail,
            "the caller's exclusion must reach the port"
        );
        assert_eq!(query.kind, Some(EntityKind::Person));
        assert_eq!(query.status, Some(FactStatus::Superseded));
        assert_eq!(query.provenance, Some(Provenance::Testimony));
        assert_eq!(
            query.subject.as_ref().map(|s| s.as_str()),
            Some("person:alpha")
        );
        let edge = query
            .edge
            .expect("the edge filter must survive translation");
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
        let searching = || SearchArgs {
            query: Some("winter".into()),
            ..search_args()
        };
        let bad = [
            SearchArgs {
                kind: Some("receipt".into()),
                ..searching()
            },
            SearchArgs {
                status: Some("retired".into()),
                ..searching()
            },
            SearchArgs {
                provenance: Some("maybe".into()),
                ..searching()
            },
            // A *bare* subject is read as a person, as everywhere else — so the
            // malformed case is one that can't be an id at all.
            SearchArgs {
                subject: Some("person:a|b".into()),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: Some("knows".into()),
                    object: "place:x".into(),
                }),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "place:a|b".into(),
                }),
                ..searching()
            },
            SearchArgs {
                limit: Some(0),
                ..searching()
            },
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
        assert_eq!(
            hit["hit"], "message",
            "a caller must not have to guess from the shape"
        );
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
                body["results"]
                    .as_array()
                    .expect("results")
                    .iter()
                    .any(|h| h["hit"] == "message"),
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
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
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
        // `start_here` hands over, and the server instructions every
        // client loads before it calls anything — and fixing one leaves a
        // session reading either of the others exactly as misinformed as before.
        let instructions = handler().get_info().instructions.unwrap_or_default();
        for (surface, text) in [
            (
                "the search description",
                search.description.as_deref().unwrap_or_default(),
            ),
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
            parent: None,
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
            edge: Some(Edge::new(
                EdgeShape::Membership,
                EntityId("org:guild".into()),
            )),
        };
        let alpha = Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::Person,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "user-named".into(),
            crm: None,
            mailbox: None,
            parent: None,
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
        assert_eq!(
            results[0]["edges"][0]["type"], "memberOf",
            "where it sits in the graph"
        );
        assert_eq!(results[0]["edges"][0]["object"], "org:guild");

        assert_eq!(results[1]["hit"], "fact");
        assert_eq!(
            results[1]["address"], "person:alpha#f3",
            "a fact hit is editable"
        );
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
        assert_eq!(
            body["mailbox"], "gamma-inbox",
            "the claim reads back: {body}"
        );

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
                sid: None,
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
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    sid: None,
                }))
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
        assert_eq!(
            body["id"], "project:atlas",
            "the handle keeps its lowercase kind token"
        );
        assert_eq!(
            body["type"], "Project",
            "responses name the type, schema.org-flavored"
        );
        assert_eq!(body["crm"], "card:874");

        let listed = jojobot
            .list_entities(Parameters(ListEntitiesArgs {
                kind: Some("project".into()),
                sid: None,
            }))
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
        let captured = capture_ok(
            &jojobot,
            capture_args("place:north-trail", "swimmable in August"),
        )
        .await;
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
        assert_eq!(
            table.len(),
            EntityKind::ALL.len(),
            "every kind must be named here"
        );
        for (kind, token, name) in table {
            assert_eq!(kind.as_token(), token, "the input token stays lowercase");
            assert_eq!(type_name(kind), name);
            // The response name is a name, not a token: input grammar unchanged.
            if name != token {
                assert!(parse_kind(name).is_err(), "{name} must not parse as a kind");
            }
        }
        assert!(
            parse_kind("Person").is_err(),
            "a capitalized kind stays rejected"
        );
    }

    /// An unknown kind is a client error that names the closed set, rather than
    /// a record filed under a noun nobody chose.
    #[tokio::test]
    async fn an_unknown_kind_is_a_client_error() {
        let err = handler()
            .add_entity(Parameters(add_args(
                "receipt",
                "some-slug",
                "An unknown kind",
            )))
            .await
            .expect_err("must reject an unknown kind");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("person"),
            "the error must name the kinds: {}",
            err.message
        );
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
                sid: None,
            }))
            .await
            .expect("update ok");
        let body = json_of(&updated);
        assert_eq!(body["id"], "thing:red-bike", "the handle is immutable");
        assert_eq!(body["name"], "Red Bike (the gravel one)");
        assert_eq!(
            body["source"], "user-named",
            "an omitted field is left alone"
        );
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
            sid: None,
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
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: None,
                }))
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
                sid: None,
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
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: None,
                }))
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
            sid: None,
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
        assert!(
            cleared["alternateName"]
                .as_array()
                .expect("a list")
                .is_empty()
        );

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
                sid: None,
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
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: None,
                }))
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
        assert!(
            advice.contains("add_entity"),
            "…and still name the way through: {advice}"
        );
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
        assert!(
            body["candidates"].as_array().unwrap().is_empty(),
            "got {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("add_entity"),
            "must name the way through: {advice}"
        );
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
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
        let halves = [(Some("location"), None), (None, Some("place:north-trail"))];
        for (shape, object) in halves {
            let err = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: shape.map(str::to_string),
                    object: object.map(str::to_string),
                    ..capture_args("alpha", "half an edge")
                }))
                .await
                .expect_err("half an edge must be refused");
            assert_eq!(
                err.code,
                ErrorCode::INVALID_PARAMS,
                "for {shape:?}/{object:?}"
            );
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
        assert!(
            err.message.contains("place"),
            "must say what it wanted: {}",
            err.message
        );
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        let address = body["facts"][0]["address"]
            .as_str()
            .expect("every fact carries an address");
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
        let captured = capture_ok(
            &jojobot,
            capture_args("alpha", "a close contact of the user"),
        )
        .await;

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
        assert_eq!(
            updated["status"], "active",
            "the negative truth is the truth"
        );
        assert_eq!(updated["content"], "NOT a close contact — do not re-infer");
        assert_eq!(
            updated["address"], "person:alpha#f1",
            "the row keeps its address"
        );
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            body["facts"].as_array().unwrap().len(),
            1,
            "nothing was created"
        );
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
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
                .recall(Parameters(RecallArgs {
                    subject: "person:zenit".into(),
                    sid: None,
                }))
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
                .recall(Parameters(RecallArgs {
                    subject: "person:zenith".into(),
                    sid: None,
                }))
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
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
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
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
            Arc::new(sid::SessionRegistry::new()),
        )
    }

    /// A handle bound to this bot, minted straight from the registry — the same
    /// thing the door hands back, without the boot.
    ///
    /// Every verb is addressed by handle now, so a spec about a mailbox still
    /// needs one to call with. Booting for it would have to stand the bot up in
    /// Memory first, which moves the entity counts other specs assert on and
    /// would make these mailbox specs pay for an identity they never look at.
    fn as_bot(jojobot: &Jojobot, bot: &str) -> String {
        jojobot
            .registry
            .mint(&EntityId::new(EntityKind::Bot, bot), None)
            .expect("a free handle")
            .as_str()
            .to_string()
    }

    /// The listing as the bot that drains those boxes sees it — counts are for
    /// your own boxes now, so a test that asserts one has to say whose it is.
    async fn drains(jojobot: &Jojobot, bot: &str) -> serde_json::Value {
        let sid = as_bot(jojobot, bot);
        json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs { sid: Some(sid) }))
                .await
                .expect("list ok"),
        )
    }

    async fn make_box(jojobot: &Jojobot, name: &str) -> serde_json::Value {
        let result = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: name.into(),
                create_new: None,
                sid: None,
            }))
            .await
            .expect("create_mailbox call ok");
        let body = json_of(&result);
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    /// Post as a bot. `sender` is its bare slug now, not free text: the sender
    /// recorded on the message is the identity behind the handle, so it lands
    /// as `bot:<sender>`.
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
                sid: as_bot(jojobot, sender),
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
        let reader = as_bot(&jojobot, "gamma");
        let created = make_box(&jojobot, "inbox").await;
        assert_eq!(created["name"], "inbox");
        assert_eq!(created["counts"]["new"], 0);

        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        assert_eq!(posted["mailbox"], "inbox");
        assert_eq!(posted["sender"], "bot:epsilon");
        assert_eq!(posted["state"], "new");
        // The author's own body is not shipped back to them — see
        // `a_post_is_receipted_without_shipping_the_body_back`.
        assert!(posted["body"].is_null());
        assert_eq!(posted["body_bytes"], "the shipment landed".len());
        assert!(
            posted["sent_at"].is_string(),
            "a message says when it was sent"
        );
        let id = posted["id"]
            .as_str()
            .expect("a message carries its id")
            .to_string();

        make_bot(&jojobot, "gamma", Some("inbox")).await;
        let listed = drains(&jojobot, "gamma").await;
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["mailboxes"][0]["name"], "inbox");
        assert_eq!(listed["mailboxes"][0]["counts"]["new"], 1);

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["mailbox"], "inbox");
        assert_eq!(delivery["count"], 1);
        assert_eq!(delivery["messages"][0]["id"], id);
        assert_eq!(
            delivery["messages"][0]["state"], "read",
            "delivery moves the column"
        );
        assert_eq!(
            delivery["messages"][0]["seen_before"], false,
            "a first delivery is nobody's leftover"
        );

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
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
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            after["count"], 0,
            "a processed message is never delivered again"
        );
    }

    /// **A crashed consumer's leftovers are visible as such.** A second read
    /// hands the same message back flagged, rather than as fresh mail.
    #[tokio::test]
    async fn a_redelivered_message_says_it_was_seen_before() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "gamma", "inbox").await;
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                new_only: None,
                sid: Some(reader.clone()),
            }))
            .await
            .expect("read ok");

        let again = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
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
        let reader = owning(&jojobot, "gamma", "inbox").await;
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
            posted["body_head"], "it landed at dawn; the crates are by the north door",
            "the subject sits beside the body, never carved out of it"
        );
        let id = posted["id"].as_str().expect("an id").to_string();

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["messages"][0]["subject"], "the shipment");

        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id,
                    notes: None,
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(
            processed["subject"], "the shipment",
            "the archive keeps the title"
        );
    }

    /// **One message, taken by id.** The named message is delivered and the rest
    /// of the box is left where it was — the point of the verb: a session that
    /// wants one filed finding must not have to own everything beside it.
    #[tokio::test]
    async fn read_message_delivers_one_and_leaves_the_box_alone() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let wanted = send(&jojobot, "inbox", "epsilon", "the one worth reading").await;
        send(&jojobot, "inbox", "sigma", "the rest of the box").await;
        let id = wanted["id"].as_str().expect("an id").to_string();

        let delivered = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: id.clone(),
                    sid: None,
                }))
                .await
                .expect("read_message ok"),
        );
        assert_eq!(delivered["id"], id.as_str());
        assert_eq!(delivered["body"], "the one worth reading");
        assert_eq!(
            delivered["state"], "read",
            "taking one message moves its column"
        );
        assert_eq!(delivered["seen_before"], false);

        make_bot(&jojobot, "gamma", Some("inbox")).await;
        let listed = drains(&jojobot, "gamma").await;
        assert_eq!(listed["mailboxes"][0]["counts"]["read"], 1);
        assert_eq!(
            listed["mailboxes"][0]["counts"]["new"], 1,
            "the rest of the box was not delivered with it"
        );

        // Taken again: a leftover, not a second delivery.
        let again = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    message_id: id,
                    sid: None,
                }))
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
            .read_message(Parameters(ReadMessageArgs {
                message_id: "999999".into(),
                sid: None,
            }))
            .await
            .expect("a blocked read is a successful call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "999999");
        assert!(
            body["candidates"]
                .as_array()
                .expect("candidates key")
                .is_empty(),
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
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId(id.clone()),
            "its description no longer carries a readable machine block",
        );

        let result = jojobot
            .read_message(Parameters(ReadMessageArgs {
                message_id: id.clone(),
                sid: None,
            }))
            .await
            .expect("a quarantined card is a successful, refusing call");
        let body = blocked(&result);
        assert_eq!(body["attempted"], id.as_str());
        let reason = body["reason"]
            .as_str()
            .expect("a quarantined card says why");
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
                sid: as_bot(&jojobot, "epsilon"),
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
                sid: None,
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
                    sid: None,
                }))
                .await
                .expect("a blocked create is a successful call"),
        );
        assert_eq!(
            refused["status"], "blocked",
            "without the signal: {refused}"
        );

        let created = json_of(
            &jojobot
                .create_mailbox(Parameters(CreateMailboxArgs {
                    name: "worker-2".into(),
                    create_new: Some(true),
                    sid: None,
                }))
                .await
                .expect("create ok"),
        );
        assert_eq!(
            created["name"], "worker-2",
            "the signal creates the sibling: {created}"
        );

        let exact = json_of(
            &jojobot
                .create_mailbox(Parameters(CreateMailboxArgs {
                    name: "worker-1".into(),
                    create_new: Some(true),
                    sid: None,
                }))
                .await
                .expect("a blocked create is a successful call"),
        );
        assert_eq!(
            exact["status"], "blocked",
            "an exact name is never overridden: {exact}"
        );
    }

    /// Malformed input is a client error that says what the grammar is, rather
    /// than a store failure or a silently-normalized name.
    // TODO(dev): semantics changed, needs a decision. The second half asserted
    // that a blank `sender` is INVALID_PARAMS; there is no `sender` field any
    // more, and a blank `sid` comes back as the blocked shape (`session_unbound`),
    // which is a successful call. Ignored rather than rewritten so the decision
    // stays yours; the first half still holds and goes green with it.
    #[tokio::test]
    async fn malformed_mailbox_input_is_a_client_error() {
        let jojobot = mailbox_handler();
        let err = jojobot
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: "Inbox".into(),
                create_new: None,
                sid: None,
            }))
            .await
            .expect_err("a name outside the grammar must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        // **A post with no handle is a blocked ANSWER, not a malformed call.**
        // The caller's grammar is fine; what is missing is who they are, and
        // absence on this surface is always an answer with a way forward.
        make_box(&jojobot, "inbox").await;
        let body = blocked(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "inbox".into(),
                    sid: "  ".into(),
                    body: "the shipment landed".into(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("a message with no sender is an answer, not a protocol failure"),
        );
        assert_eq!(
            body["wrote"], false,
            "nothing is recorded from nobody: {body}"
        );
    }

    /// **A held-open message stops costing its full size on every poll — and is
    /// never hidden.** The crash contract keeps a message unprocessed until the
    /// work it asks for is done, which is correct; but every poll of that box
    /// then re-delivered the whole multi-KB body flagged `seen_before`. Over a
    /// long pickup loop that is the same message downloaded all night.
    ///
    /// A bot that exists, owns `name`, and has a handle to call with.
    async fn owning(jojobot: &Jojobot, bot: &str, name: &str) -> String {
        make_bot(jojobot, bot, Some(name)).await;
        as_bot(jojobot, bot)
    }

    /// **The box is not an argument on the read side: the `sid` says whose it
    /// is.** Reading IS delivery, so a name in the caller's hand is a way to
    /// take somebody else's mail out of `new` and make it theirs-no-longer. The
    /// own-box norm was written in the essay in the strongest words available
    /// and was still only advice, because the parameter was right there. It is
    /// structural now.
    #[tokio::test]
    async fn a_read_opens_the_callers_own_box_and_needs_no_name() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "gamma-inbox").await;
        make_box(&jojobot, "somebody-elses").await;
        let sid = owning(&jojobot, "gamma", "gamma-inbox").await;
        send(&jojobot, "gamma-inbox", "delta", "for gamma").await;
        send(&jojobot, "somebody-elses", "delta", "not for gamma").await;

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(sid),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(delivery["mailbox"], "gamma-inbox");
        assert_eq!(delivery["count"], 1);
        assert_eq!(delivery["messages"][0]["body"], "for gamma");

        // …and the other box was not touched, which is the whole point: a
        // delivery it never took is still waiting in `new` for its own drainer.
        let theirs = json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs {
                    sid: Some(owning(&jojobot, "delta", "somebody-elses").await),
                }))
                .await
                .expect("list ok"),
        );
        let mine = theirs["mailboxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "somebody-elses")
            .expect("delta's box");
        assert_eq!(
            mine["counts"]["new"], 1,
            "gamma's read must not have taken delivery of delta's mail: {mine}"
        );
    }

    /// **Three ways to have no box, three different next moves.** Folding them
    /// into one miss would be advice that fits none of them: a caller with no
    /// identity has to boot, a bot with no claim has to be given one, and a
    /// claim nobody has opened needs the box minted — deliberately, by the one
    /// verb that mints.
    #[tokio::test]
    async fn a_read_with_no_box_to_open_says_which_kind_of_nothing_it_found() {
        let jojobot = mailbox_handler();

        // 1. No handle at all.
        let anonymous = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: None,
                }))
                .await
                .expect("an answer, not a protocol failure"),
        );
        let how = anonymous["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("start_here"),
            "an anonymous caller is sent to the door that gives it an identity: {how}"
        );

        // 2. A bot that claims no box.
        let boxless = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some({
                        make_bot(&jojobot, "gamma", None).await;
                        as_bot(&jojobot, "gamma")
                    }),
                }))
                .await
                .expect("an answer"),
        );
        let how = boxless["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("update_entity") && how.contains("create_mailbox"),
            "a bot with no claim is told how to get one: {how}"
        );

        // 3. A claim nobody has opened. Reported missing, never created —
        //    the same answer the boot door gives, so the two agree.
        let missing = blocked(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(owning(&jojobot, "delta", "never-opened").await),
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(missing["attempted"], "never-opened");
        let how = missing["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("create_mailbox"),
            "a claimed box nobody opened names the verb that mints: {how}"
        );
        assert!(
            jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .is_empty(),
            "and it stayed a report: nothing was minted"
        );
    }

    /// **The safe branch is the DEFAULT, not the documented preference.** A
    /// caller that passes nothing gets the cheap, common read — news whole,
    /// leftovers named but not re-shipped — and pays for the expensive one only
    /// by asking. Prose recommending the cheap option does not help a client
    /// that follows defaults, which is most of them.
    ///
    /// **What makes that safe is that nothing goes silent**, so it is pinned
    /// here rather than left to the description: under the default, a leftover
    /// is still delivered, still counted, still flagged `seen_before`, and
    /// still owed. Only its body is withheld, and it says so.
    #[tokio::test]
    async fn a_read_that_asks_for_nothing_still_hands_over_every_leftover() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "gamma", "dev").await;
        make_box(&jojobot, "dev").await;
        let held_body = "a long hand-off that stays open until the round closes. ".repeat(40);
        let held = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "dev".into(),
                    sid: as_bot(&jojobot, "delta"),
                    body: held_body.clone(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let held_id = held["id"].as_str().expect("an id").to_string();

        // Delivered once and deliberately not processed.
        json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        send(&jojobot, "dev", "delta", "and here is the next batch").await;

        // The plain read — no argument, no opinion.
        let plain = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(plain["new_only"], true, "the safe branch is the default");
        assert_eq!(
            plain["count"], 2,
            "the leftover is still delivered: {plain}"
        );

        let leftover = plain["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("a default read still hands the leftover over");
        assert_eq!(leftover["seen_before"], true, "…still owed: {leftover}");
        assert_eq!(leftover["body_elided"], true, "…and says what it withheld");
        assert_eq!(leftover["body_bytes"], held_body.trim().len());
        assert!(leftover["body"].is_null());

        let fresh = plain["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] != held_id.as_str())
            .expect("the fresh message");
        assert_eq!(
            fresh["body"], "and here is the next batch",
            "news is what a plain read is for, so news arrives whole: {fresh}"
        );

        // And the expensive read is still there for the caller who asks.
        let whole = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: Some(false),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        let recovered = whole["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("still there");
        assert_eq!(
            recovered["body"],
            held_body.trim(),
            "new_only: false is how a crashed consumer gets the body back: {recovered}"
        );
    }

    /// `new_only` changes what is SHIPPED, never what is owed: the leftover is
    /// still in the delivery, still counted, still flagged, still to be marked
    /// processed. Only its body is left out, and it says so.
    ///
    /// What holds the invariant here is the `.find(...).expect(...)` below, not
    /// the count: `count` is `delivery.messages.len()`, so an implementation
    /// that dropped leftovers from the RENDERED list alone would still report
    /// two. The lookup is what fails.
    #[tokio::test]
    async fn new_only_elides_a_leftover_s_body_and_never_its_existence() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "gamma", "dev").await;
        make_box(&jojobot, "dev").await;
        let held_body = "a long hand-off that stays open until the round closes. ".repeat(40);
        let held = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "dev".into(),
                    sid: as_bot(&jojobot, "delta"),
                    body: held_body.clone(),
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let held_id = held["id"].as_str().expect("an id").to_string();

        // Take delivery once, and deliberately do NOT process it.
        let first = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            first["messages"][0]["body"],
            held_body.trim(),
            "the first read is whole"
        );

        // Fresh mail arrives, and the poll asks for news only.
        send(&jojobot, "dev", "delta", "and here is the next batch").await;
        let poll = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: Some(true),
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            poll["count"], 2,
            "the leftover is STILL in the delivery: {poll}"
        );
        assert_eq!(poll["new_only"], true);

        let leftover = poll["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] == held_id.as_str())
            .expect("the held message is still handed over");
        assert_eq!(
            leftover["seen_before"], true,
            "…still flagged as owed: {leftover}"
        );
        assert!(
            leftover["body"].is_null(),
            "…and its body is what was dropped"
        );
        assert_eq!(leftover["body_elided"], true);
        assert_eq!(leftover["body_bytes"], held_body.trim().len());

        let fresh = poll["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["id"] != held_id.as_str())
            .expect("the fresh message");
        assert_eq!(
            fresh["body"], "and here is the next batch",
            "news is the point of the poll, so news arrives whole: {fresh}"
        );

        // And it is still owed: processing it is still the caller's job.
        let processed = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: held_id,
                    notes: None,
                    sid: None,
                }))
                .await
                .expect("mark ok"),
        );
        assert_eq!(processed["state"], "processed");
    }

    /// **The two verbs that echo a body back echo it to the one caller who
    /// already has it.** `post_message` returned the whole stored body to its
    /// author; `mark_processed` returned the entire original message to the
    /// consumer who had just read it. On 4–8 KB reports that doubled the cost
    /// of the behaviour the crash contract asks for, which is a price that
    /// scales with thoroughness — the wrong thing to charge for.
    ///
    /// What the full echo proved is preserved: the store's read-back invariant
    /// means a body that did not survive storage is an ERROR, not a success
    /// with mangled bytes, so fidelity is proven server-side. The receipt keeps
    /// what a caller cannot derive — the id, the state, the notes, the exact
    /// stored size — and says plainly that the body was left out.
    #[tokio::test]
    async fn a_post_is_receipted_without_shipping_the_body_back() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        let long = "counted the crates and reconciled them against the manifest. ".repeat(60);

        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: long.clone(),
                    subject: Some("the crate count".into()),
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        // Everything a caller cannot derive is still here.
        assert!(posted["id"].as_str().is_some());
        assert_eq!(posted["mailbox"], "pm");
        assert_eq!(posted["state"], "new");
        assert_eq!(posted["subject"], "the crate count");
        assert!(posted["sent_at"].is_string());
        // …and the body is not, loudly.
        assert!(posted["body"].is_null());
        assert_eq!(posted["body_elided"], true);
        assert_eq!(posted["body_bytes"], long.trim().len());
        assert!(
            posted["body_head"]
                .as_str()
                .expect("a head")
                .starts_with("counted the crates")
        );
        assert!(
            posted["body_head"]
                .as_str()
                .expect("a head")
                .chars()
                .count()
                < long.chars().count() / 4,
            "the head is a head, not the body under another key"
        );
        assert!(
            posted["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("list_sent")
        );
    }

    /// The same, for the terminal verb — whose caller got the body from the
    /// read that handed it to them.
    #[tokio::test]
    async fn processing_receipts_without_shipping_the_body_back() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed at dawn").await;

        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: posted["id"].as_str().expect("an id").to_string(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(
            body["state"], "processed",
            "the proof that matters: it moved"
        );
        assert_eq!(
            body["notes"], "filed under shipments",
            "…and what was recorded"
        );
        assert!(body["body"].is_null());
        assert_eq!(body["body_elided"], true);
        assert_eq!(body["body_bytes"], "the shipment landed at dawn".len());
        assert!(
            body["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("read_message")
        );
    }

    /// **The delivery verbs still ship bodies.** The elision is for the caller
    /// who wrote or already read the text; a consumer taking delivery is being
    /// handed something they have never seen, and that is the whole verb.
    #[tokio::test]
    async fn taking_delivery_still_hands_over_the_whole_body() {
        let jojobot = mailbox_handler();
        let reader = owning(&jojobot, "gamma", "inbox").await;
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed at dawn").await;

        let delivery = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    new_only: None,
                    sid: Some(reader.clone()),
                }))
                .await
                .expect("read ok"),
        );
        assert_eq!(
            delivery["messages"][0]["body"],
            "the shipment landed at dawn"
        );
        assert!(
            delivery["messages"][0]["body_elided"].is_null(),
            "nothing was withheld"
        );
    }

    /// **Existence is public; what is waiting in somebody's queue is not.**
    /// Live report: sender bots posting into boxes they do not drain kept
    /// narrating "there is an unread message there that is not mine to pick up"
    /// — correct restraint, and attention spent on a question that should never
    /// have been posed. The affordance posed it: every box's per-state counts
    /// were shown to everybody, and the own-box norm then had to suppress in
    /// prose what the payload kept suggesting.
    ///
    /// Names stay visible, because a writer needs them — `post_message` must
    /// name an existing box, and a near-miss comes back with candidates.
    #[tokio::test]
    async fn counts_are_shown_for_the_boxes_you_drain_and_names_for_the_rest() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "dev").await;
        make_box(&jojobot, "pm").await;
        make_bot(&jojobot, "gamma", Some("dev")).await;
        // **A second bot that drains the other box** — without one, "your boxes"
        // and "every claimed box" are the same set and the scoping proves
        // nothing.
        make_bot(&jojobot, "delta", Some("pm")).await;
        send(&jojobot, "dev", "delta", "your hand-off").await;
        send(&jojobot, "pm", "sigma", "not your business").await;

        let listed = drains(&jojobot, "gamma").await;
        assert_eq!(listed["count"], 2, "every box is still LISTED: {listed}");
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!(["dev"]),
            "…and the answer says whose counts these are: {listed}"
        );

        let by_name = |name: &str| -> serde_json::Value {
            listed["mailboxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .find(|b| b["name"] == name)
                .expect("the box")
                .clone()
        };

        let mine = by_name("dev");
        assert_eq!(mine["yours"], true);
        assert_eq!(mine["counts"]["new"], 1, "my own queue, in full: {mine}");

        let theirs = by_name("pm");
        assert_eq!(
            theirs["name"], "pm",
            "it EXISTS — post_message needs the name"
        );
        assert_eq!(theirs["yours"], false);
        assert!(
            theirs["counts"].is_null(),
            "…and its queue is not mine to weigh: {theirs}"
        );
        assert_eq!(theirs["counts_elided"], true, "elided, never silently");
    }

    /// A boot sees its own box's counts in the snapshot, and names only for the
    /// rest — the same rule, in the other place a session meets this listing.
    #[tokio::test]
    async fn a_boot_snapshot_counts_only_the_bot_s_own_box() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "dev").await;
        make_box(&jojobot, "pm").await;
        make_bot(&jojobot, "gamma", Some("dev")).await;
        make_bot(&jojobot, "delta", Some("pm")).await;
        send(&jojobot, "dev", "delta", "your hand-off").await;
        send(&jojobot, "pm", "sigma", "not your business").await;

        let booted = boot(&jojobot, "gamma").await;
        let boxes = booted["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("boxes")
            .clone();
        let find = |name: &str| {
            boxes
                .iter()
                .find(|b| b["name"] == name)
                .expect("the box")
                .clone()
        };

        assert_eq!(find("dev")["counts"]["new"], 1, "my box, counted: {booted}");
        assert_eq!(find("dev")["yours"], true);
        assert!(
            find("pm")["counts"].is_null(),
            "somebody else's, name only: {booted}"
        );
        assert_eq!(find("pm")["yours"], false);

        // The bot's own box still comes back in full under `identity`, which is
        // the whole point of booting as somebody.
        assert_eq!(booted["identity"]["owned_mailbox"]["counts"]["new"], 1);
    }

    /// **A sender can see where their own mail got to, and seeing moves
    /// nothing.** Twice a session wanted to confirm a report had been *read*
    /// rather than merely delivered, and could not: the only verbs that show a
    /// message's state take delivery, and taking delivery of somebody else's box
    /// makes their mail yours to finish. So the question went unanswered because
    /// asking it cost more than the answer was worth.
    #[tokio::test]
    async fn a_sender_sees_where_their_mail_got_to_without_moving_any_of_it() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "pm", "otto", "the kiln slice is done").await;
        send(&jojobot, "inbox", "otto", "a note for somebody else").await;
        let theirs = send(&jojobot, "pm", "delta", "not yours to see").await;

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: Some("bot:otto".into()),
                    mailbox: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(sent["count"], 2, "only what this sender sent: {sent}");
        let bodies: Vec<&str> = sent["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| m["sender"].as_str().expect("a sender"))
            .collect();
        assert_eq!(bodies, vec!["bot:otto", "bot:otto"]);

        // The body is elided, and says so rather than leaving a reader to guess.
        let first = &sent["messages"][0];
        assert!(first["body"].is_null(), "{first}");
        assert_eq!(first["body_elided"], true);
        assert!(first["body_bytes"].as_u64().expect("a size") > 0);
        assert!(
            first["body_head"]
                .as_str()
                .expect("a head")
                .contains("note for somebody else")
        );
        assert!(
            first["how_to_read"]
                .as_str()
                .expect("a pointer")
                .contains("include_bodies")
        );

        // **Nothing moved — read from the STORE, not from the verb.** Asserting
        // `state == "new"` on `list_sent`'s own response lets the verb grade
        // itself: its body is built from a snapshot taken before it returns, so
        // a version that took delivery afterwards would still report `new`. The
        // counts come from the other side of the store.
        make_bot(&jojobot, "gamma", Some("pm")).await;
        let counted = drains(&jojobot, "gamma").await;
        let pm = counted["mailboxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "pm")
            .expect("the box");
        assert_eq!(
            pm["counts"]["read"], 0,
            "looking at your own outbox is not a delivery: {pm}"
        );
        assert_eq!(
            pm["counts"]["new"], 2,
            "…and everything is still waiting: {pm}"
        );
        assert!(
            !json_of(
                &jojobot
                    .read_message(Parameters(ReadMessageArgs {
                        message_id: theirs["id"].as_str().expect("an id").to_string(),
                        sid: None
                    }))
                    .await
                    .expect("read ok")
            )["seen_before"]
                .as_bool()
                .expect("a flag"),
            "somebody else's message was never taken"
        );
    }

    /// **A mistyped box is a near miss, not an empty outbox.** This verb's
    /// whole job is answering "did my report land", so answering `count: 0` for
    /// a typo says "no, it did not" — and the sender posts it again, leaving
    /// duplicate mail with the original still unprocessed. Every other verb
    /// that names a box screens it; this was the one that did not.
    #[tokio::test]
    async fn a_mistyped_box_is_blocked_with_candidates_rather_than_answering_empty() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "handoffs").await;
        send(&jojobot, "handoffs", "otto", "the kiln slice is done").await;

        let body = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: Some("bot:otto".into()),
                    mailbox: Some("handofs".into()),
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("a near miss is an answer, not an error"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_ne!(body["count"], 0, "…and never a confident zero: {body}");
        let names: Vec<&str> = body["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .map(|c| c["name"].as_str().expect("a name"))
            .collect();
        assert!(
            names.contains(&"handoffs"),
            "the box they meant is named: {body}"
        );
    }

    /// **A card jojobot cannot read is not a message that was never sent.** The
    /// scan cannot parse a quarantined card, so it leaves it out — and this
    /// verb would then answer "no, your report never landed" about a card
    /// sitting on the board with the report on it.
    #[tokio::test]
    async fn list_sent_surfaces_cards_it_cannot_read_rather_than_answering_no() {
        let boxes = Arc::new(InMemoryMailboxes::new());
        let jojobot = with_mailboxes(boxes.clone());
        make_box(&jojobot, "pm").await;
        boxes.quarantine(
            &MailboxName("pm".into()),
            &MessageId("4212".into()),
            "its description no longer carries a readable machine block",
        );

        let body = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: Some("dev (implementer)".into()),
                    mailbox: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(body["count"], 0, "nothing readable is theirs");
        assert_eq!(
            body["unreadable"][0]["mailbox"], "pm",
            "…but the unreadable card is not silence: {body}"
        );
        assert_eq!(body["unreadable"][0]["card_ids"][0], "4212");
        assert!(
            body["unreadable_note"]
                .as_str()
                .is_some_and(|n| n.contains("repair")),
            "…and it says what fixes it: {body}"
        );
    }

    /// Ids are minted as decimal counters, so ordering them as text puts `9`
    /// after `10`. Both other sort sites in this subsystem compare them as
    /// numbers on purpose; this one did not.
    #[tokio::test]
    async fn list_sent_breaks_a_tie_on_the_id_as_a_number() {
        let boxes = Arc::new(InMemoryMailboxes::new());
        let jojobot = with_mailboxes(boxes.clone());
        make_box(&jojobot, "pm").await;
        // **Seeded through the store, with ONE instant across all ten.** The
        // handler stamps `now()` per call, so posting through it never produces
        // the tie this sorts on and the tie-break would go unexercised.
        let at = jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant");
        for n in 1..=10 {
            boxes
                .post_message(NewMessage {
                    mailbox: MailboxName("pm".into()),
                    body: format!("report {n}"),
                    subject: None,
                    sender: "dev (implementer)".into(),
                    sent_at: at,
                    in_reply_to: None,
                })
                .await
                .expect("post ok");
        }

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: Some("dev (implementer)".into()),
                    mailbox: None,
                    include_bodies: None,
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        let first = sent["messages"][0]["id"].as_str().expect("an id");
        assert_eq!(first, "10", "the newest is id 10, not id 9: {sent}");
    }

    /// Asking for the bodies gets them — the elision is a default, not a rule.
    #[tokio::test]
    async fn a_sender_can_ask_for_the_bodies_of_their_own_mail() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        send(&jojobot, "pm", "otto", "the kiln slice is done").await;

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    sender: Some("bot:otto".into()),
                    mailbox: Some("pm".into()),
                    include_bodies: Some(true),
                    sid: None,
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(sent["messages"][0]["body"], "the kiln slice is done");
        assert!(
            sent["messages"][0]["body_elided"].is_null(),
            "nothing was elided to announce"
        );
    }

    /// **A reply names what it answers, and a dangling link is blocked.** The
    /// hand-off ↔ report chain was correlated by prose convention alone, which
    /// is manual archaeology the moment there is any volume. The link is
    /// optional, carries no semantics beyond itself, and — like every other
    /// reference on this surface — must name something that exists.
    #[tokio::test]
    async fn a_reply_carries_the_message_it_answers_and_a_dangling_link_is_blocked() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "pm").await;
        let original = send(&jojobot, "pm", "delta", "build the kiln slice").await;
        let original_id = original["id"].as_str().expect("an id").to_string();
        assert!(
            original["in_reply_to"].is_null(),
            "a message answering nothing says so"
        );

        let reply = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
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
                    sid: None,
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
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering something nobody said".into(),
                    subject: None,
                    in_reply_to: Some("9999".into()),
                }))
                .await
                .expect("a bad reference is an answer, not an error"),
        );
        assert_eq!(dangling["status"], "blocked", "{dangling}");
        assert_eq!(dangling["wrote"], false);

        // **A blank link is no link.** A client that sends `in_reply_to: ""`
        // meant to send nothing; refusing the whole post over an empty string
        // would be the second-worst way to answer, and the message reads back
        // as answering nothing — which is what it says.
        let unlinked = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "pm".into(),
                    sid: as_bot(&jojobot, "otto"),
                    body: "answering nothing in particular".into(),
                    subject: None,
                    in_reply_to: Some("   ".into()),
                }))
                .await
                .expect("a blank link is not a malformed call"),
        );
        assert_ne!(unlinked["status"], "blocked", "{unlinked}");
        assert!(
            unlinked["in_reply_to"].is_null(),
            "blank is absent, not empty: {unlinked}"
        );
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
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let long = "counted the crates and reconciled them against the manifest ".repeat(200);
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: id.clone(),
                    notes: Some(long.clone()),
                    sid: None,
                }))
                .await
                .expect("a long note must not fail the terminal verb"),
        );
        assert_eq!(
            body["state"], "processed",
            "the message WAS handled: {body}"
        );
        assert_eq!(
            body["notes_truncated"], true,
            "…and the cut is said out loud: {body}"
        );
        let kept = body["notes"].as_str().expect("the outcome is recorded");
        assert!(
            kept.ends_with('…'),
            "the record itself says it was cut: {kept:?}"
        );
        assert!(kept.chars().count() < long.chars().count());
    }

    /// **A caller who recorded nothing was cut off from nothing.** The flag
    /// compared the stored notes against what this call asked to store, on the
    /// premise that the store applies the same rule — but both stores carry a
    /// PRE-EXISTING note forward when the caller supplies none, and
    /// `mark_processed` has no state gate, so re-processing is reachable. The
    /// second call then saw notes it had not sent and reported a cut nobody
    /// made: the same wrong inference the flag exists to prevent, pointing the
    /// other way.
    #[tokio::test]
    async fn processing_again_without_notes_reports_no_cut() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let id = posted["id"].as_str().expect("an id").to_string();

        let processed = |notes: Option<String>| {
            let id = id.clone();
            async {
                json_of(
                    &jojobot
                        .mark_processed(Parameters(MarkProcessedArgs {
                            message_id: id,
                            notes,
                            sid: None,
                        }))
                        .await
                        .expect("mark_processed ok"),
                )
            }
        };

        let first = processed(Some("filed under shipments".into())).await;
        assert_eq!(first["notes_truncated"], false);

        // Again, recording nothing. The store keeps the earlier note.
        let again = processed(None).await;
        assert_eq!(
            again["notes"], "filed under shipments",
            "the record stands: {again}"
        );
        assert_eq!(
            again["notes_truncated"], false,
            "no record was offered, so none was cut: {again}"
        );
    }

    /// A record that fits is stored whole and reports no cut — the flag is
    /// always present, so a reader never branches on whether it is there.
    #[tokio::test]
    async fn an_outcome_record_that_fits_reports_no_cut() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let posted = send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        let body = json_of(
            &jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    message_id: posted["id"].as_str().expect("an id").to_string(),
                    notes: Some("filed under shipments".into()),
                    sid: None,
                }))
                .await
                .expect("mark_processed ok"),
        );
        assert_eq!(body["notes"], "filed under shipments");
        assert_eq!(body["notes_truncated"], false, "{body}");
    }

    /// **An id that names nothing is an answer, not a failure** — and no longer
    /// a protocol error either: naming something that does not exist is the
    /// same kind of answer whichever gate catches it, so it wears one shape.
    #[tokio::test]
    async fn processing_an_unknown_message_is_blocked_not_an_error() {
        let jojobot = mailbox_handler();
        let result = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "999999".into(),
                notes: None,
                sid: None,
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
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;
        store.quarantine(
            &MailboxName("inbox".into()),
            &MessageId("4212".into()),
            "its description no longer carries a readable machine block",
        );

        make_bot(&jojobot, "gamma", Some("inbox")).await;
        let listed = drains(&jojobot, "gamma").await;
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
                sid: None,
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
                    sid: None,
                }))
                .await
                .expect("an id nothing answers to is still an answer"),
        );
        assert!(
            unknown["reason"].is_null(),
            "there is no card to explain — that field belongs to the quarantine answer: {unknown}"
        );
        assert!(
            !unknown["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("PERSON"),
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
        // The seven mailbox verbs in it are create_mailbox, list_mailboxes,
        // list_sent, post_message, read_mailbox, read_message and
        // mark_processed; the three session verbs are journal, amend_journal
        // and wrap_session (there is deliberately no start_session — booting an
        // identity IS starting its session); the rest are Memory's.
        assert_eq!(
            names,
            [
                "add_entity",
                "amend_journal",
                "capture",
                "create_mailbox",
                "journal",
                "list_entities",
                "list_mailboxes",
                "list_sent",
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

    /// **There is exactly one orientation verb, and this is written so a second
    /// one cannot satisfy it.**
    ///
    /// "One door, never a second" was prose in the roadmap sitting beside a
    /// claim about lineage, and the only test that watched the surface pinned a
    /// LIST OF NAMES. So a second door was added, its name was added to the
    /// list, the suite stayed green, and the diff read as a deliberate act
    /// rather than as the drift it was. A list cannot express "one of these,
    /// ever" — adding to it is how you satisfy it.
    ///
    /// The property is asserted three ways in the code and once on the surface,
    /// because a second door can be built four ways: by calling `orient` again,
    /// by taking the door's arguments again, by reading the essay again, or by
    /// telling a caller to start somewhere else.
    #[test]
    fn there_is_exactly_one_orientation_verb() {
        // The tests below this line construct doors on purpose; the constraint
        // is about the shipped surface, so it reads only the code half.
        let source = include_str!("lib.rs");
        let (code, _) = source
            .split_once("#[cfg(test)]\nmod tests")
            .expect("the test module marks where the shipped code ends");

        for (what, marker, expected) in [
            ("entry points into orientation", "self.orient(", 1),
            (
                "verbs taking the door's arguments",
                "Parameters<OrientArgs>",
                1,
            ),
            // Defined once, read once. A door that reimplemented the answer
            // rather than calling `orient` would still have to reach for the
            // essay, and this is where that shows.
            ("readers of the orientation essay", "ORIENTATION", 2),
        ] {
            let found = code.matches(marker).count();
            assert_eq!(
                found, expected,
                "{found} {what} ({marker:?}) — there is one door, and a second is how this fails"
            );
        }

        // And on the surface a caller actually reads: exactly one verb claims
        // to be the one you call first. A door nobody is told to call is not a
        // door, so a second one has to say this somewhere.
        let tools = Jojobot::tool_router().list_all();
        let claiming: Vec<&str> = tools
            .iter()
            .filter(|t| {
                let description = t.description.as_deref().unwrap_or_default().to_lowercase();
                description.contains("call this first") || description.contains("call it first")
            })
            .map(|t| t.name.as_ref())
            .collect();
        assert_eq!(
            claiming,
            ["start_here"],
            "one verb tells a caller where to start, and it is the door"
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
            "start_here",
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
            description.contains("READING IT IS THE ACTING"),
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

    /// **A description may not name a parameter its verb does not take.**
    ///
    /// `bot` and `session` are both gone from these verbs' schemas — one address
    /// rides every call now, and it is the `sid`. The descriptions are the half
    /// of the surface a model actually reads, so one still saying "pass `bot`,
    /// the name you booted as" produces exactly the call the schema refuses,
    /// from a caller who has no reason to doubt the sentence.
    ///
    /// **Pinned per verb rather than swept over the whole surface**, because two
    /// verbs keep a legitimate `bot` and neither is the caller's identity:
    /// `start_here` takes the name to boot AS, and `set_charter`'s names the bot
    /// its write is ABOUT, exactly as a capture names a subject.
    #[test]
    fn the_session_verbs_are_described_by_the_one_address_they_take() {
        let tools = Jojobot::tool_router().list_all();
        for name in [
            "journal",
            "amend_journal",
            "wrap_session",
            "list_mailboxes",
            "post_message",
        ] {
            let tool = tools
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{name} is a tool"));
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(
                description.contains("`sid`"),
                "{name} must name the address it takes: {description}"
            );
            // `sender` joins the list for the same reason the other two are on
            // it: `post_message` derives it from the handle and takes no such
            // parameter, so a sentence describing one sends a caller to emit a
            // field that is silently dropped.
            for gone in ["`bot`", "`session`", "`sender`", "you booted as"] {
                assert!(
                    !description.contains(gone),
                    "{name} still describes {gone}, which is no parameter of it: {description}"
                );
            }
        }
    }

    /// **Nothing agent-facing tells a caller to declare who it is.**
    ///
    /// `sender` left `PostMessageArgs` when it became derived from the `sid`,
    /// and three texts went on describing it. `PostMessageArgs` does not deny
    /// unknown fields, so a caller following those sentences emits a `sender`
    /// that is silently dropped, then calls `list_sent` with the string it
    /// invented, gets nothing, and concludes its report never arrived — which
    /// is the exact failure `list_sent` exists to prevent.
    ///
    /// **Asserted as absence of the token, not as a list of today's
    /// sentences.** The essay and `post_message` have no honest use for the
    /// word: the caller does not supply one, so any sentence that reaches for
    /// it is describing a parameter that is not there, whatever its wording.
    /// `list_sent` is the one verb that still takes a `sender` — somebody
    /// else's, to ask after their mail — so it is the one place the token
    /// belongs.
    #[test]
    fn no_agent_facing_text_asks_a_caller_to_declare_a_sender() {
        assert!(
            !ORIENTATION.contains("`sender`"),
            "the essay still asks a caller for a sender it does not supply"
        );
        let tools = Jojobot::tool_router().list_all();
        let post = tools
            .iter()
            .find(|t| t.name == "post_message")
            .expect("post_message is a tool");
        let description = post.description.as_deref().unwrap_or_default();
        assert!(
            !description.contains("`sender`"),
            "post_message still describes a `sender` parameter it does not take: {description}"
        );
    }

    /// **The door says what to carry away from it, and how far it reaches.**
    ///
    /// A boot that hands back an address and then tells the caller to identify
    /// itself some other way has spent the answer it just gave. The reach is the
    /// part a caller cannot infer: `sid` rides the reads too — they are
    /// attributed, never journalled — and a caller who passes it only on the
    /// session verbs is anonymous for every other call it makes.
    #[test]
    fn the_boot_door_says_the_sid_rides_every_call_including_the_reads() {
        let tools = Jojobot::tool_router().list_all();
        let door = tools
            .iter()
            .find(|t| t.name == "start_here")
            .expect("start_here is a tool");
        let description = door.description.as_deref().unwrap_or_default();
        assert!(
            !description.contains("you booted as"),
            "the door must not send a caller back to naming its bot: {description}"
        );
        assert!(
            description.contains("reads included"),
            "the door must say the sid rides the reads too: {description}"
        );
    }

    /// **The essay teaches the address, and what jojobot writes down about you.**
    ///
    /// Two claims that moved with the model. What makes two connections one
    /// session is the `sid` the caller carries, not an identity the connection
    /// remembers — nothing remembers anything between calls. And jojobot's own
    /// beats follow the WRITES: every call site of [`Jojobot::beat`] is a write
    /// verb and [`BEAT_CLASSES`] holds no read, so an essay promising "one per
    /// verb class you use" tells a session to expect a tally of its reads that
    /// will never appear.
    #[test]
    fn the_orientation_teaches_the_sid_as_the_address_and_leaves_reads_untallied() {
        assert!(
            ORIENTATION.contains("`sid` you carry"),
            "the essay must name what makes two connections one session"
        );
        assert!(
            !ORIENTATION.contains("the identity that booted them"),
            "the essay still says a connection carries the identity, which nothing does"
        );
        assert!(
            ORIENTATION.contains("Reads are not journalled"),
            "the essay must say which calls jojobot beats about"
        );
        assert!(
            !ORIENTATION.contains("one per verb class you use"),
            "the essay still promises a beat per verb class, reads included"
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
                sid: None,
            }))
            .await
            .expect("entity ok");
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;

        let out = jojobot
            .start_here(Parameters(OrientArgs {
                bot: None,
                brief: None,
                resume: None,
            }))
            .await
            .expect("start_here ok");
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
            // has to say what one is made of before the door hands one over.
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
        // **Anonymous orientation drains nothing, so it sees no bot's queue** —
        // but this box is claimed by nobody, and a box with no drainer has no
        // queue to shield. That is the distinction the scoping actually draws:
        // it protects a drainer's workload, not the board's contents.
        assert_eq!(boxes[0]["name"], "inbox");
        assert_eq!(
            boxes[0]["yours"], false,
            "an anonymous caller drains nothing"
        );
        assert_eq!(
            boxes[0]["counts"]["new"], 1,
            "…and an unclaimed box is still countable: {:?}",
            boxes[0]
        );
    }

    /// The other half, and the one the scoping exists for: a box somebody else
    /// drains comes back to an anonymous caller as a name only.
    #[tokio::test]
    async fn an_anonymous_caller_sees_no_counts_for_a_box_somebody_drains() {
        let jojobot = handler();
        make_box(&jojobot, "dev").await;
        make_bot(&jojobot, "gamma", Some("dev")).await;
        send(&jojobot, "dev", "delta", "your hand-off").await;

        let listed = json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs { sid: None }))
                .await
                .expect("list ok"),
        );
        let dev = listed["mailboxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "dev")
            .expect("the box");
        assert_eq!(dev["yours"], false);
        assert!(dev["counts"].is_null(), "somebody drains this one: {dev}");
        assert_eq!(dev["counts_elided"], true, "elided, never silently");
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!([]),
            "…and the answer names what it counted: {listed}"
        );
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
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn post_message(
            &self,
            _: mailbox::NewMessage,
        ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn read_mailbox(
            &self,
            _: &mailbox::MailboxName,
        ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn read_message(
            &self,
            _: &mailbox::MessageId,
        ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
        async fn mark_processed(
            &self,
            _: &mailbox::MessageId,
            _: Option<&str>,
        ) -> Result<mailbox::Message, mailbox::MailboxError> {
            Err(mailbox::MailboxError::NotConfigured(
                "the mailbox world is down".into(),
            ))
        }
    }

    /// A handler whose mailbox world answers nothing, over a memory the caller
    /// may already have populated — a bot has to be stood up while the world is
    /// up, since a claim that cannot be screened is refused.
    /// A Memory whose ENTITY INDEX cannot be read, everything else working —
    /// the shape an Outline outage takes for the one read ownership depends on.
    struct UnindexedMemory(Arc<InMemoryMemory>);

    #[async_trait]
    impl Memory for UnindexedMemory {
        async fn list_entities(&self, _: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
            Err(MemoryError::Store("the entity index cannot be read".into()))
        }
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.0.add_entity(new).await
        }
        async fn update_entity(
            &self,
            id: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.0.update_entity(id, patch).await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.0.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.0.recall(subject).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.0.update_fact(address, patch).await
        }
        async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
            self.0.set_prose(entity, prose).await
        }
        async fn append_journal(
            &self,
            on: jiff::civil::Date,
            entry: &str,
        ) -> Result<String, MemoryError> {
            self.0.append_journal(on, entry).await
        }
        async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
            self.0.scan().await
        }
    }

    /// **A world that is down is not an answer of "no".** Ownership is a read
    /// of Memory, so an outage means jojobot cannot say what anybody drains —
    /// and rendering that as "not yours" told every bot its own queue belonged
    /// to somebody else, with a note asserting counts are shown for the boxes
    /// you drain. That is a claim nobody can act on.
    #[tokio::test]
    async fn an_unreadable_entity_index_says_ownership_is_unknown() {
        let memory = Arc::new(InMemoryMemory::new());
        let boxes = Arc::new(InMemoryMailboxes::new());
        let seeded = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_box(&seeded, "dev").await;
        make_bot(&seeded, "gamma", Some("dev")).await;
        send(&seeded, "dev", "delta", "your hand-off").await;

        let blind = Jojobot::new(
            Arc::new(UnindexedMemory(memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        let listed = drains(&blind, "gamma").await;

        assert_eq!(listed["ownership_known"], false, "{listed}");
        assert!(
            listed["note"]
                .as_str()
                .expect("a note")
                .contains("OWNERSHIP IS UNKNOWN"),
            "…and the note says so rather than asserting whose the counts are: {listed}"
        );
        assert_eq!(
            listed["mailboxes"][0]["yours"], false,
            "an unknown is not a yes — the counts still do not go out"
        );
        assert!(listed["mailboxes"][0]["counts"].is_null());
    }

    fn handler_with_mailboxes_down(memory: Arc<InMemoryMemory>) -> Jojobot {
        Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(DownMailboxes),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        )
    }

    /// **The norms a session cannot derive from the tool list are taught.**
    /// Each of these was a real session getting it wrong or having no way to
    /// know: wrapping a session whose work continues (so the next run started
    /// from nothing), treating `abandoned` as an ordinary ending, and reading a
    /// flat box listing as an invitation to survey a shared namespace.
    ///
    /// Deliberately **engine-generic**: how long a given role's session should
    /// run, or which box a particular bot drains, is that bot's charter at
    /// seeding — not prose compiled into a user-agnostic server.
    #[test]
    fn the_orientation_teaches_the_two_endings_and_the_own_box_norm() {
        // The two endings, and that they are a choice about the WORK.
        assert!(
            ORIENTATION.contains("CLEAR AND RESUME"),
            "the continuing case is named"
        );
        assert!(
            ORIENTATION.contains("do NOT wrap"),
            "…and says which verb NOT to reach for, since wrapping is the tempting default"
        );
        assert!(
            ORIENTATION.contains("resume note"),
            "…and names the thing you leave for whoever picks it up"
        );
        assert!(
            ORIENTATION.contains("exception to journal leanness"),
            "…and exempts it from the leanness rule, or the rule suppresses it"
        );
        // **`abandoned` is not a failure**, and the essay must not teach it as
        // one: it means the run was never wrapped up, and picking one back up
        // is ordinary rather than recovery. What the essay still has to draw is
        // the distinction that survives — a run that ENDED against one that
        // merely STOPPED.
        assert!(
            ORIENTATION.contains("not a failure"),
            "abandoned is a run nobody wrapped up, not a run that broke"
        );
        assert!(
            !ORIENTATION.contains("failure path"),
            "…so the old framing must be gone, not merely balanced by the new one"
        );
        assert!(
            ORIENTATION.contains("merely stopped"),
            "…and the distinction that does survive is ended against stopped"
        );

        // The own-box norm, and the affordance that tempted otherwise. It is no
        // longer a norm a caller can decline — the read side takes no box name —
        // so what the essay owes is that the reader knows which box opens.
        assert!(ORIENTATION.contains("read your OWN mailbox"));
        assert!(
            ORIENTATION.contains("no name to pass"),
            "the essay has to say the choice is gone, not merely discouraged"
        );
        assert!(
            ORIENTATION.contains("not an invitation"),
            "the flat listing is what posed the access question, so it is what gets answered"
        );
        assert!(
            ORIENTATION.contains("post_message"),
            "…and there is a sanctioned way to reach another box: write to it"
        );

        engine_generic("ORIENTATION", ORIENTATION);
    }

    /// **The engine names roles, never a particular working agreement.** A
    /// cadence ("every 20 minutes"), a named protocol ("the round is closed"),
    /// or one party's framing ("my report") is a charter's business — data in
    /// the operator's own store — and compiling it in makes a user-agnostic
    /// server carry one user's arrangements.
    ///
    /// **Asserted as a property, not an enumerated denylist.** A list of
    /// today's phrasings only fires on today's phrasings: "every 15 minutes"
    /// and "each morning" would both sail past one. This matches the SHAPE — a
    /// cadence is a count next to a unit of time — so a wording nobody
    /// anticipated is caught too.
    fn engine_generic(what: &str, prose: &str) {
        let lower = prose.to_lowercase();
        let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();

        const UNITS: [&str; 12] = [
            "minute", "minutes", "hour", "hours", "day", "days", "week", "weeks", "morning",
            "evening", "night", "nights",
        ];
        const QUANTIFIERS: [&str; 6] = ["every", "each", "per", "twice", "once", "hourly"];

        for (i, word) in words.iter().enumerate() {
            // A cadence is a quantifier reaching a time unit within a couple of
            // words: "every 20 minutes", "each morning", "twice a day".
            if !QUANTIFIERS.contains(word) {
                continue;
            }
            if *word == "hourly" {
                panic!("{what} states a cadence ('hourly') — that belongs to a bot's charter");
            }
            let mut reach = words.iter().skip(i + 1).take(3);
            if let Some(unit) = reach.find(|w| UNITS.contains(w)) {
                panic!(
                    "{what} states a cadence ('{word} … {unit}') — how often a role runs belongs \
                     to that bot's charter at seeding, not to a user-agnostic engine"
                );
            }
        }
    }

    /// The same property, over every tool description — which is where this
    /// round's working-agreement prose actually landed. The orientation essay
    /// had a gate; the descriptions had none, and they are read by exactly the
    /// same audience for exactly the same purpose.
    #[test]
    fn no_tool_description_carries_a_working_agreement() {
        for tool in Jojobot::tool_router().list_all() {
            let description = tool.description.as_deref().unwrap_or_default();
            engine_generic(&format!("{}'s description", tool.name), description);

            // Named protocols and one party's framing: a verb's contract is
            // what it does and refuses, never who is arranged to call it.
            for borrowed in ["round-closed", "the round", "my report", "hand-off ↔"] {
                assert!(
                    !description.to_lowercase().contains(borrowed),
                    "{}'s description borrows a working agreement ({borrowed:?}): a description \
                     states the contract, and an arrangement between two bots is charter material",
                    tool.name
                );
            }
        }
    }

    /// **A returning session pays for the essay once.** The orientation prose
    /// is the only part of this answer that does not change between calls, and
    /// it rode every one of them — so a client running a boot-surface token
    /// budget skipped orientation entirely rather than paying for it again,
    /// which is the opposite of what it is for. `brief` returns everything that
    /// moves, and says plainly that the essay is what it left out.
    #[tokio::test]
    async fn a_brief_orientation_keeps_the_snapshot_and_drops_only_the_essay() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        let full = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(full["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(full["orientation_elided"], false);

        let brief = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: Some(true),
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            brief["orientation"].is_null(),
            "the essay is what was dropped: {brief}"
        );
        assert_eq!(brief["orientation_elided"], true);
        assert_eq!(
            full["orientation_elided"], false,
            "…and the marker says which of the two answers this is: {full}"
        );

        // **How to get it back is on the surface a caller reads**, since the
        // payload no longer carries a nudge of its own — an elision nobody can
        // undo is an elision that costs the reader the thing it saved.
        let tools = Jojobot::tool_router().list_all();
        let door = tools
            .iter()
            .find(|t| t.name == "start_here")
            .expect("start_here is a tool");
        let description = door.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("without brief"),
            "the way back to the essay must be stated where brief is: {description}"
        );

        // Everything that changes between calls is still here.
        assert_eq!(brief["snapshot"], full["snapshot"]);
        assert_eq!(brief["snapshot"]["entities"]["available"], true);
        assert!(brief["snapshot"]["mailboxes"].is_object());
    }

    /// **The orientation stamp is gone, whole.** It was a version on the essay
    /// so a returning session could tell whether the copy it held was current —
    /// and it was rejected outright, along with every proposed way of keeping
    /// the check honest (a prose hash, a derived version, a hand-maintained
    /// one). A number a human has to remember to bump is a number that lies,
    /// and what it bought did not pay for that.
    ///
    /// **Asserted over the whole payload and the whole surface**, not over the
    /// two keys that used to carry it: the failure this guards against is the
    /// idea coming back somewhere adjacent, and a key-by-key check would miss
    /// it in a note or an arg doc. `brief` survives, as a plain caller-chosen
    /// option with nothing to compare.
    #[tokio::test]
    async fn nothing_on_the_surface_stamps_the_orientation_with_a_version() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store);
        make_bot(&jojobot, "gamma", None).await;

        let answers = [
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        bot: None,
                        brief: None,
                        resume: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        bot: None,
                        brief: Some(true),
                        resume: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            boot(&jojobot, "gamma").await,
        ];
        for body in &answers {
            assert!(
                !body.to_string().contains("orientation_version"),
                "no answer carries a version stamp: {body}"
            );
            assert!(
                body.get("how_to_read_orientation").is_none(),
                "…nor the nudge that existed only to explain one: {body}"
            );
        }

        for tool in Jojobot::tool_router().list_all() {
            let description = tool.description.as_deref().unwrap_or_default();
            let schema = serde_json::to_string(&tool.input_schema).expect("a schema");
            for surface in [description, schema.as_str()] {
                assert!(
                    !surface.contains("orientation_version"),
                    "{} still teaches a version stamp: {surface}",
                    tool.name
                );
            }
        }
    }

    /// A boot is brief the same way, and never at the cost of the things a boot
    /// exists for: the identity, its box, and its session.
    #[tokio::test]
    async fn a_brief_boot_still_hands_over_the_identity_and_the_session() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: Some(true),
                    resume: None,
                }))
                .await
                .expect("boot ok"),
        );
        assert!(booted["orientation"].is_null());
        assert_eq!(booted["orientation_elided"], true);
        assert_eq!(booted["identity"]["bot"]["id"], "bot:gamma");
        assert_eq!(booted["session"]["available"], true);
        assert_eq!(booted["session"]["resumed"], false);
    }

    /// One world being down must not take orientation with it: a fresh agent
    /// on a half-configured server still deserves the map.
    #[tokio::test]
    async fn start_here_survives_a_world_that_is_down() {
        let out = handler_with_mailboxes_down(Arc::new(InMemoryMemory::new()))
            .start_here(Parameters(OrientArgs {
                bot: None,
                brief: None,
                resume: None,
            }))
            .await
            .expect("orientation still lands");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert!(body["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(body["snapshot"]["mailboxes"]["available"], false);
    }

    // ── booting as an identity ──────────────────────────────────────────────

    /// Stand up a bot the way an operator would: an entity of kind `bot`
    /// claiming a box, its charter as prose, its rules as facts.
    async fn make_bot(jojobot: &Jojobot, slug: &str, mailbox: Option<&str>) {
        let result = jojobot
            .add_entity(Parameters(AddEntityArgs {
                mailbox: mailbox.map(str::to_string),
                ..add_args("bot", slug, slug)
            }))
            .await
            .expect("add_entity call ok");
        // **A blocked write is a SUCCESSFUL result**, so `.expect` alone let a
        // refused claim pass as a created bot — and a fixture that silently
        // created nothing makes every assertion built on it vacuous. Its
        // sibling `make_box` has always checked this.
        let body = json_of(&result);
        assert_ne!(
            body["status"], "blocked",
            "the fixture bot {slug:?} was not created: {body}"
        );
    }

    async fn boot(jojobot: &Jojobot, name: &str) -> serde_json::Value {
        json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some(name.into()),
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("the boot call is ok"),
        )
    }

    /// Answer the choice a boot handed back.
    async fn boot_answering(jojobot: &Jojobot, name: &str, answer: &str) -> serde_json::Value {
        json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some(name.into()),
                    brief: None,
                    resume: Some(answer.into()),
                }))
                .await
                .expect("the boot call is ok"),
        )
    }

    /// The handle a boot handed back, or `None` when it handed none back.
    fn sid_of(body: &serde_json::Value) -> Option<String> {
        body["session"]["sid"].as_str().map(str::to_string)
    }

    /// Boot as this bot and take the handle the door hands back.
    async fn booted(jojobot: &Jojobot, name: &str) -> String {
        sid_of(&boot(jojobot, name).await)
            .unwrap_or_else(|| panic!("{name} booted without a handle"))
    }

    /// Boot as this bot and pick up the one run it is offered — what a reconnect
    /// does, now that a boot finding work in flight hands back a choice rather
    /// than a handle.
    async fn resumed(jojobot: &Jojobot, name: &str) -> String {
        let offered = boot(jojobot, name).await;
        let choice = offered["session"]["choices"][0]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} was offered nothing to resume: {offered}"))
            .to_string();
        sid_of(&boot_answering(jojobot, name, &choice).await).expect("the resumed handle")
    }

    /// A handle addressing a card that already exists — what a restart rebuilds
    /// off the board, and the only way to name one particular run now that the
    /// handle is the address.
    fn as_run(jojobot: &Jojobot, bot: &str, card: &SessionId) -> String {
        jojobot
            .registry
            .mint(&EntityId::new(EntityKind::Bot, bot), Some(card.clone()))
            .expect("a free handle")
            .as_str()
            .to_string()
    }

    // ── the two-branch boot ─────────────────────────────────────────────────

    /// **An anonymous boot is an orientation preview: nothing usable behind
    /// it.** The world and the snapshot, no identity, and above all no handle —
    /// a caller who was handed one would reasonably believe it addressed
    /// something, and there is nothing for it to address.
    #[tokio::test]
    async fn an_anonymous_boot_hands_back_no_handle_at_all() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma", None).await;

        let body = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            body["identity"].is_null(),
            "no identity was claimed: {body}"
        );
        assert!(
            body["session"].is_null(),
            "and no session was begun: {body}"
        );
        // Asserted over the whole payload, not over the one key it would sit
        // on: a handle smuggled anywhere in this answer is a handle a caller
        // will try to use.
        assert!(
            !body.to_string().contains("\"sid\""),
            "an anonymous boot carries no handle anywhere: {body}"
        );
    }

    /// **Nothing to resume, so the handle comes back immediately.** There is no
    /// moment between "I am gamma" and "gamma is working", and a boot that made
    /// the caller ask a second time for the address would invent one.
    #[tokio::test]
    async fn a_boot_with_nothing_to_resume_hands_back_a_handle_at_once() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        let body = boot(&jojobot, "gamma").await;
        let handle = sid_of(&body).unwrap_or_else(|| panic!("a handle comes back: {body}"));
        assert!(
            sid::is_readable(&handle),
            "…and it is a readable one: {handle}"
        );
        assert_eq!(body["session"]["resumed"], false);
        assert!(
            body["session"]["choices"].is_null(),
            "there was nothing to choose: {body}"
        );

        // **The card stays lazy.** A boot that does nothing leaves nothing
        // behind, handle or no handle.
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "the handle is minted here; the card waits for the first write"
        );
    }

    /// **Something to resume, so the choice comes first and the handle waits.**
    /// Attaching silently was the old behaviour and it decided for the caller;
    /// each option is named by what it was working on, because that is the only
    /// thing that tells two runs of one identity apart.
    #[tokio::test]
    async fn a_resumable_session_comes_back_as_a_choice_and_no_handle() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        for focus in ["reading the hand-off", "chasing the flaky test"] {
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: Sid(format!("t{:03}", line!() % 1000)),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                })
                .await
                .expect("begin ok");
        }

        let body = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&body).is_none(),
            "the handle arrives with the answer, not before it: {body}"
        );

        let choices = body["session"]["choices"]
            .as_array()
            .expect("the offer is a list");
        assert_eq!(
            choices.len(),
            2,
            "a bot may have several runs at once: {body}"
        );
        let mut working_on: Vec<&str> = choices
            .iter()
            .map(|c| c["working_on"].as_str().expect("what it was working on"))
            .collect();
        working_on.sort_unstable();
        assert_eq!(
            working_on,
            ["chasing the flaky test", "reading the hand-off"]
        );
        for choice in choices {
            let handle = choice["sid"].as_str().expect("each option is addressable");
            assert!(
                sid::is_readable(handle),
                "{handle} is not a readable handle"
            );
        }
        assert!(
            body["session"]["how_to_proceed"]
                .as_str()
                .is_some_and(|h| h.contains("resume") && h.contains("new")),
            "…and the way to answer is stated: {body}"
        );
    }

    /// Answering it: resume returns that session's handle and its chronology;
    /// choosing new returns a different handle and leaves the old run alone.
    #[tokio::test]
    async fn resuming_returns_that_session_s_handle_and_new_returns_another() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let begun = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "reading the hand-off".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        let offered = boot(&jojobot, "gamma").await;
        let offer = offered["session"]["choices"][0]["sid"]
            .as_str()
            .expect("one option")
            .to_string();

        let resumed = boot_answering(&jojobot, "gamma", &offer).await;
        assert_eq!(
            sid_of(&resumed).as_deref(),
            Some(offer.as_str()),
            "{resumed}"
        );
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(
            resumed["session"]["session"]["focus"], "reading the hand-off",
            "resuming hands back the run itself, chronology and all: {resumed}"
        );

        // **The offer is stable**: the same card keeps the handle it was first
        // given, so a caller who boots twice before answering is not looking at
        // two addresses for one run.
        assert_eq!(
            boot(&jojobot, "gamma").await["session"]["choices"][0]["sid"],
            offer.as_str()
        );

        let fresh = boot_answering(&jojobot, "gamma", sid::NEW).await;
        let minted = sid_of(&fresh).unwrap_or_else(|| panic!("new mints one: {fresh}"));
        assert_ne!(
            minted, offer,
            "choosing new is a different session: {fresh}"
        );
        assert_eq!(fresh["session"]["resumed"], false);

        // **Nothing auto-wrapped.** A new session never closes an old one.
        let all = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 1, "and new stays lazy until it writes: {all:?}");
        assert_eq!(all[0].id, begun.id);
        assert_eq!(
            all[0].state,
            SessionState::Active,
            "the old run is untouched"
        );
    }

    /// **A handle survives the process that minted it, because the card holds
    /// it.** This is the restart cliff closed: the registry is rebuilt from the
    /// board before anything is served, so the handle a caller wrote down
    /// yesterday still addresses its run today.
    ///
    /// It matters beyond convenience. The sid is the address every later verb
    /// carries, so a handle that died with the process meant a deploy silently
    /// re-pointed every agent at nothing.
    #[tokio::test]
    async fn a_handle_written_on_the_card_survives_a_restart() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection_sharing(
            memory.clone(),
            store.clone(),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&jojobot, "gamma", None).await;

        let handle = sid_of(&boot(&jojobot, "gamma").await).expect("a handle");
        journal_entry(&jojobot, &handle, "read the hand-off").await;
        let card = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok")
            .into_iter()
            .next()
            .expect("a card");
        assert_eq!(
            card.sid.as_ref().map(|s| s.as_str()),
            Some(handle.as_str()),
            "the card carries the handle the door handed out: {card:?}"
        );

        // A restart: same board, an empty registry, filled from the board before
        // the first request — exactly what the composition root does.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = store.all_sessions().await.expect("board read ok");
        assert_eq!(rebuilt.rebuild_from(&board), 1, "one handle recovered");
        let restarted = connection_sharing(memory, store.clone(), rebuilt);

        let resumed = boot_answering(&restarted, "gamma", &handle).await;
        assert_eq!(
            sid_of(&resumed).as_deref(),
            Some(handle.as_str()),
            "the same handle, still addressing the same run: {resumed}"
        );
        assert_eq!(resumed["session"]["session"]["id"], card.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"],
            "read the hand-off"
        );
    }

    /// **A card is born with the handle its caller is holding**, on a client
    /// with no session affinity — which is every real client.
    ///
    /// One round ago this was the gap: the write arrived carrying a bot name and
    /// nothing else, so jojobot minted the card a handle of its own rather than
    /// guessing which of possibly several booted agents was writing, and the
    /// caller's own handle stayed card-less. The sid rides the write now, so
    /// there is nothing to guess and the two are the same handle.
    #[tokio::test]
    async fn a_card_is_born_with_the_handle_its_caller_is_holding() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;
        let door_gave = sid_of(&boot(&client.call(), "gamma").await).expect("a handle");

        json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: None,
                    sid: door_gave.clone(),
                }))
                .await
                .expect("journal ok"),
        );

        let card = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok")
            .into_iter()
            .next()
            .expect("a card");
        let stored = card.sid.as_ref().expect("a card is never born handle-less");
        assert!(sid::is_readable(stored.as_str()));
        assert_eq!(
            stored.as_str(),
            door_gave.as_str(),
            "…and it is the caller's OWN handle, because the sid rides the write: jojobot no \
             longer has to guess which of several booted agents is writing"
        );

        // The card's own handle is what survives a restart and addresses the run.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = client.sessions.all_sessions().await.expect("read ok");
        assert_eq!(rebuilt.rebuild_from(&board), 1);
        assert_eq!(
            rebuilt.lookup(stored.as_str()).expect("held").card,
            Some(card.id.clone())
        );
    }

    /// **A card written before handles were persisted carries none**, and that
    /// is not a broken card: the boot that offers it mints one on the spot. The
    /// migration is a no-op *only because* minting-on-offer already exists —
    /// stated here so nobody later "simplifies" the offer into requiring a
    /// stored handle.
    #[tokio::test]
    async fn a_card_with_no_stored_handle_is_offered_one_on_the_spot() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let jojobot = connection_sharing(
            Arc::new(InMemoryMemory::new()),
            store.clone(),
            registry.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;

        let legacy = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t900".into()),
                focus: "from before handles were stored".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        // Strip the handle, which is what an older jojobot's card looks like.
        store.forget_sid(&legacy.id);
        let board = store.all_sessions().await.expect("read ok");
        assert_eq!(
            registry.rebuild_from(&board),
            0,
            "a card with no handle contributes none"
        );

        let offered = boot(&jojobot, "gamma").await;
        let choice = &offered["session"]["choices"][0];
        let minted = choice["sid"]
            .as_str()
            .expect("a handle, minted on the spot");
        assert!(sid::is_readable(minted));
        assert_eq!(choice["working_on"], "from before handles were stored");

        let resumed = boot_answering(&jojobot, "gamma", minted).await;
        assert_eq!(resumed["session"]["session"]["id"], legacy.id.as_str());
    }

    /// **A handle that never reached a card does not survive a restart, and
    /// says so** — even though the restart rebuilt everything it could.
    ///
    /// A card is written lazily, so a boot that did no work leaves the handle
    /// with nothing behind it, and nothing behind it is nothing to rebuild
    /// FROM: `rebuild_from` reads handles off the cards on the board, and this
    /// handle is on no card. It comes back blocked, which is not a 404 from the
    /// store — the store was never asked — and above all not a silent new
    /// session, which would leave a caller writing into a run they did not mean
    /// under an id they think they know.
    ///
    /// **Age is not what blocks it.** The old name here said "a handle from
    /// before a restart", which is the opposite of the spec: a pre-restart
    /// handle whose card exists RESOLVES, and its sibling
    /// `a_handle_written_on_the_card_survives_a_restart` is what pins that. The
    /// rebuild is run here rather than skipped so the two cases are told apart
    /// by the thing that actually decides them.
    #[tokio::test]
    async fn a_handle_that_never_reached_a_card_is_blocked_after_a_rebuild() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;
        let handle = sid_of(&boot(&client.call(), "gamma").await).expect("a handle");

        // Same stores, new process: the registry is what a restart empties, and
        // filling it back from the board is what a restart then does.
        let rebuilt = Arc::new(sid::SessionRegistry::new());
        let board = client.sessions.all_sessions().await.expect("read ok");
        assert_eq!(
            rebuilt.rebuild_from(&board),
            0,
            "the boot wrote no card, so the rebuild has nothing to recover: {board:?}"
        );
        let restarted = Jojobot::new(
            client.memory.clone(),
            Arc::new(SpySearch::default()),
            client.mailboxes.clone(),
            client.sessions.clone(),
            rebuilt,
        );
        let body = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    resume: Some(handle.clone()),
                }))
                .await
                .expect("a dead handle is an answer, not a protocol failure"),
        );
        assert_eq!(body["attempted"], handle);
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("gone") && how.contains("start_here"),
            "that session is gone; boot again: {how}"
        );

        // An unreadable handle is refused too — never repaired into a near one.
        let mistyped = blocked(
            &restarted
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    resume: Some("k3fo".into()),
                }))
                .await
                .expect("an unreadable handle is an answer too"),
        );
        assert_eq!(mistyped["attempted"], "k3fo");
    }

    /// **A handle is bound to its identity at boot and never switches.** Naming
    /// somebody else's session is refused rather than quietly honoured — the
    /// whole bug class deleted instead of guarded against downstream.
    #[tokio::test]
    async fn a_handle_belonging_to_another_identity_is_refused() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store);
        make_bot(&jojobot, "gamma", None).await;
        make_bot(&jojobot, "delta", None).await;

        let gammas = sid_of(&boot(&jojobot, "gamma").await).expect("a handle");
        let body = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("delta".into()),
                    brief: None,
                    resume: Some(gammas.clone()),
                }))
                .await
                .expect("somebody else's handle is an answer, not a protocol failure"),
        );
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|h| h.contains("bot:gamma")),
            "the refusal names whose it is: {body}"
        );
    }

    /// **The handle says nothing about the work.** Two runs of one identity on
    /// the same focus get different handles, and no handle carries anything
    /// derived from what its session is doing.
    #[tokio::test]
    async fn two_sessions_on_one_focus_get_different_and_opaque_handles() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        let focus = "chasing the flaky test";
        for _ in 0..2 {
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: Sid(format!("t{:03}", line!() % 1000)),
                    focus: focus.into(),
                    started_at: jiff::Timestamp::now(),
                })
                .await
                .expect("begin ok");
        }

        let offered = boot(&jojobot, "gamma").await;
        let handles: Vec<&str> = offered["session"]["choices"]
            .as_array()
            .expect("the offer")
            .iter()
            .map(|c| c["sid"].as_str().expect("a handle"))
            .collect();
        assert_eq!(handles.len(), 2);
        assert_ne!(handles[0], handles[1], "identical work, different handles");

        for handle in &handles {
            assert!(sid::is_readable(handle));
            // Nothing of the focus survives into the handle: not a slug, not a
            // word, not even a run of three of its characters.
            let slug = focus.to_lowercase();
            for window in slug.as_bytes().windows(3) {
                let fragment = String::from_utf8(window.to_vec()).expect("ascii");
                assert!(
                    !handle.contains(&fragment),
                    "{handle} carries {fragment:?} out of the focus it is for"
                );
            }
        }
    }

    /// Close a session the way the sweep would, and put its last beat far
    /// enough back that it reads as that old.
    async fn abandoned_run(
        store: &InMemorySessions,
        bot: &str,
        focus: &str,
        hours_ago: i64,
    ) -> Session {
        let begun = store
            .begin(NewSession {
                bot: EntityId(format!("bot:{bot}")),
                sid: Sid(format!("t{:03}", hours_ago.rem_euclid(1000))),
                focus: focus.into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(hours_ago),
            })
            .await
            .expect("begin ok");
        store
            .close(&begun.id, SessionState::Abandoned)
            .await
            .expect("close ok");
        store.read_session(&begun.id).await.expect("read ok")
    }

    /// **An abandoned run is picked up, not recovered from.** It stopped without
    /// telling its story — a disconnect, a closed laptop — so the boot offers it
    /// back, resuming REOPENS it, and the record continues where it stopped
    /// instead of starting again beside it.
    ///
    /// Without this, an interrupted run could never be wrapped at all: the verb
    /// that tells the story refuses a closed session, so the story was lost by
    /// construction.
    #[tokio::test]
    async fn resuming_an_abandoned_run_reopens_it_and_continues_the_record() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let stopped = abandoned_run(&store, "gamma", "reading the hand-off", 30).await;

        let offered = boot(&jojobot, "gamma").await;
        assert!(
            sid_of(&offered).is_none(),
            "there is something to choose: {offered}"
        );
        let choice = &offered["session"]["choices"][0];
        assert_eq!(choice["working_on"], "reading the hand-off");
        assert_eq!(
            choice["state"], "abandoned",
            "**marked, never silently mixed in with the live runs**: {offered}"
        );

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], stopped.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["state"], "active",
            "resuming reopens it — it is running again: {resumed}"
        );
        let sid = sid_of(&resumed).expect("the resumed handle");

        // The proof it meant something: the write that would have been refused
        // a moment ago lands, on the same record.
        journal_entry(&jojobot, &sid, "picked it back up").await;
        let read = store.read_session(&stopped.id).await.expect("read ok");
        assert_eq!(read.state, SessionState::Active);
        assert_eq!(
            read.entries.last().expect("an entry").text,
            "picked it back up"
        );
        let all = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 1, "continued, not forked beside: {all:?}");
    }

    /// **Writing to a closed run says something different depending on which
    /// end it reached**, because the way forward is different.
    ///
    /// Both refusals used to read "closed is terminal both ways — nothing
    /// appends to it, amends it, or reopens it", which is now false for half of
    /// them: an abandoned run reopens, and telling its owner to start a new one
    /// instead sends them to fork the work they were trying to continue.
    #[tokio::test]
    async fn writing_to_a_closed_run_says_which_end_it_reached() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        let stopped = abandoned_run(&store, "gamma", "reading the hand-off", 30).await;
        let told = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .close(&told.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let advice = |session: &SessionId| {
            let jojobot = &jojobot;
            let sid = as_run(jojobot, "gamma", session);
            async move {
                let body = blocked(
                    &jojobot
                        .journal(Parameters(JournalArgs {
                            entry: "one more thing".into(),
                            focus: None,
                            sid,
                        }))
                        .await
                        .expect("a closed session is an answer, not a protocol failure"),
                );
                body["how_to_proceed"].as_str().expect("advice").to_string()
            }
        };

        let on_stopped = advice(&stopped.id).await;
        assert!(
            on_stopped.contains("resume") && on_stopped.contains("start_here"),
            "a run that stopped is picked back up, not replaced: {on_stopped}"
        );
        assert!(
            !on_stopped.contains("belongs to a new session"),
            "…and it must not send the caller off to fork the work: {on_stopped}"
        );

        let on_told = advice(&told.id).await;
        assert!(
            on_told.contains("story") && on_told.contains("Journal"),
            "a told story names the reason this end is the last word: {on_told}"
        );
        assert!(
            on_told.contains("new session"),
            "…and there the next run really is the way forward: {on_told}"
        );
    }

    /// **Bounded attention, unbounded reachability.** A run nobody has touched
    /// in months is not something to bring up — but a handle its caller still
    /// holds still addresses it, and resuming it still works.
    #[tokio::test]
    async fn an_old_abandoned_run_is_not_offered_and_is_still_resumable() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let jojobot = connection_sharing(
            Arc::new(InMemoryMemory::new()),
            store.clone(),
            registry.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;
        let ancient = abandoned_run(&store, "gamma", "something from last winter", 24 * 240).await;

        let booted = boot(&jojobot, "gamma").await;
        assert!(
            booted["session"]["choices"].is_null(),
            "nothing recent enough to offer, so the sid comes back at once: {booted}"
        );
        assert!(sid_of(&booted).is_some());

        // The caller kept the handle from when this process issued it.
        let held = registry
            .for_card(&EntityId("bot:gamma".into()), &ancient.id)
            .expect("a handle");
        let resumed = boot_answering(&jojobot, "gamma", held.as_str()).await;
        assert_eq!(
            resumed["session"]["resumed"], true,
            "age bounds what is volunteered, never what a handle reaches: {resumed}"
        );
        assert_eq!(resumed["session"]["session"]["id"], ancient.id.as_str());
        assert_eq!(resumed["session"]["session"]["state"], "active");
    }

    /// The offer reaches **at most one** abandoned run — the most recent — while
    /// every live run is offered. One is a memory jog; a list of them is a
    /// history nobody asked for.
    #[tokio::test]
    async fn the_offer_carries_every_live_run_and_only_the_newest_abandoned_one() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        abandoned_run(&store, "gamma", "the oldest stop", 100).await;
        abandoned_run(&store, "gamma", "the middle stop", 70).await;
        abandoned_run(&store, "gamma", "the newest stop", 40).await;
        store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        let offered = boot(&jojobot, "gamma").await;
        let choices = offered["session"]["choices"].as_array().expect("the offer");
        let shown: Vec<(&str, &str)> = choices
            .iter()
            .map(|c| {
                (
                    c["working_on"].as_str().expect("a focus"),
                    c["state"].as_str().expect("a state"),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [("still going", "active"), ("the newest stop", "abandoned")],
            "every live run, and only the most recent stop: {offered}"
        );
    }

    /// **A wrapped run is over, both in the offer and by handle.** Its story is
    /// already an entry in the operator's Journal, and reopening it would make a
    /// published account retroactively false.
    #[tokio::test]
    async fn a_wrapped_run_is_never_offered_and_never_reopens() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let jojobot = connection_sharing(
            Arc::new(InMemoryMemory::new()),
            store.clone(),
            registry.clone(),
        );
        make_bot(&jojobot, "gamma", None).await;

        let told = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "a finished piece of work".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(2),
            })
            .await
            .expect("begin ok");
        store
            .close(&told.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let booted = boot(&jojobot, "gamma").await;
        assert!(
            booted["session"]["choices"].is_null(),
            "a told story is not on offer: {booted}"
        );

        let held = registry
            .for_card(&EntityId("bot:gamma".into()), &told.id)
            .expect("a handle");
        let refused = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    resume: Some(held.as_str().into()),
                }))
                .await
                .expect("a wrapped run is an answer, not a protocol failure"),
        );
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("wrapped") && how.contains("story"),
            "the refusal says why this end is the last word: {how}"
        );
        assert_eq!(
            store.read_session(&told.id).await.expect("read ok").state,
            SessionState::Wrapped,
            "and nothing moved"
        );
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
        connection_sharing(memory, sessions, Arc::new(sid::SessionRegistry::new()))
    }

    /// The same, over a registry the caller keeps — what two connections of one
    /// PROCESS share, and the only way a handle outlives the connection it was
    /// handed to.
    fn connection_sharing(
        memory: Arc<InMemoryMemory>,
        sessions: Arc<InMemorySessions>,
        registry: Arc<sid::SessionRegistry>,
    ) -> Jojobot {
        Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            sessions,
            registry,
        )
    }

    /// **A client with no session affinity — a FRESH connection per tool call.**
    ///
    /// This is what production clients actually present. The service factory
    /// builds one handler per MCP session, so a client that does not hold one
    /// across a conversation gets a new handler — and a new, empty binding —
    /// for every single call. Both claude.ai and ChatGPT do exactly this:
    /// the boot succeeds, and the journal on the very next call finds nobody
    /// home.
    ///
    /// **This stays in the suite permanently.** Every other test here holds a
    /// handle across calls, which is the shape no real client has, and that is
    /// the gap this whole class of bug shipped through.
    struct NoAffinity {
        memory: Arc<InMemoryMemory>,
        sessions: Arc<InMemorySessions>,
        mailboxes: Arc<InMemoryMailboxes>,
        /// Process-wide, exactly as it is in production: the connections come
        /// and go, the handles this process issued do not.
        registry: Arc<sid::SessionRegistry>,
    }

    impl NoAffinity {
        fn new() -> Self {
            NoAffinity {
                memory: Arc::new(InMemoryMemory::new()),
                sessions: Arc::new(InMemorySessions::new()),
                mailboxes: Arc::new(InMemoryMailboxes::new()),
                registry: Arc::new(sid::SessionRegistry::new()),
            }
        }

        /// One tool call, on a connection that has never seen another.
        fn call(&self) -> Jojobot {
            Jojobot::new(
                self.memory.clone(),
                Arc::new(SpySearch::default()),
                self.mailboxes.clone(),
                self.sessions.clone(),
                // **The one thing a reconnect must NOT rebuild.** A handle is
                // an address across connections or it is nothing.
                self.registry.clone(),
            )
        }
    }

    /// **THE PRODUCTION SHAPE: identity does not survive to the next call.**
    /// Every session verb was addressed by a connection binding, and no real
    /// client holds a connection — so the boot bound an identity to something
    /// that evaporated before the next request arrived, and every write after it
    /// came back "not running as any identity".
    ///
    /// The chicken-and-egg made addressing by `session` no help either: a
    /// session materializes lazily on the first write, and the first write could
    /// never land, so no id was ever minted to name. **The `sid` has neither
    /// problem** — the door mints it before any card exists and hands it back,
    /// so a caller that keeps nothing but that string writes to the same run
    /// across as many connections as its client opens.
    #[tokio::test]
    async fn a_stateless_client_can_journal_by_carrying_its_sid() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;

        // Call 1: boot. Succeeds, as it did in production.
        let opened = boot(&client.call(), "gamma").await;
        assert_eq!(opened["session"]["available"], true);
        let sid = sid_of(&opened).expect("a handle");

        // Call 2: a different connection, as every real client presents.
        let body = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal call ok"),
        );
        assert_ne!(
            body["status"], "blocked",
            "the sid is enough, on a connection that remembers nothing: {body}"
        );

        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, minted by the first write: {live:?}"
        );
        assert_eq!(live[0].entries[0].text, "read the hand-off");

        // Call 3: another fresh connection ATTACHES to that session rather than
        // forking a second one — the whole point of resolving from the board.
        json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "picked it back up".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal call ok"),
        );
        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "still one session, not one per call: {live:?}"
        );
        assert_eq!(
            live[0].entries.len(),
            2,
            "…and it accrued: {:?}",
            live[0].entries
        );
    }

    /// The two edges of reading an identity off a parameter: blank is absent,
    /// and a handle of another kind is a client error rather than a silent
    /// winner — booting a person as an identity would hand somebody's page back
    /// as a charter.
    #[test]
    fn a_named_bot_is_absent_when_blank_and_refused_when_it_is_another_kind() {
        assert_eq!(named_bot(None).expect("ok"), None);
        assert_eq!(named_bot(Some("   ")).expect("blank is absent"), None);
        assert_eq!(
            named_bot(Some(" gamma ")).expect("ok"),
            Some(EntityId("bot:gamma".into())),
            "a bare name is a bot at this door"
        );
        assert_eq!(
            named_bot(Some("bot:gamma")).expect("ok"),
            Some(EntityId("bot:gamma".into())),
            "…and so is the qualified handle"
        );
        let wrong = named_bot(Some("person:milhouse")).expect_err("another kind is refused");
        assert_eq!(wrong.code, ErrorCode::INVALID_PARAMS);
    }

    /// **A wrapped `sid` stays closed, and the bot behind it boots its next
    /// run.** Those are different questions and the answers have to differ: a
    /// `sid` names one run, and closed is terminal both ways for that record —
    /// while the identity outlives any run of it, so booting again is ordinary
    /// rather than a way back in.
    #[tokio::test]
    async fn a_wrapped_sid_stays_closed_while_its_bot_boots_the_next_run() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;
        let first = booted(&client.call(), "gamma").await;

        client
            .call()
            .journal(Parameters(JournalArgs {
                entry: "the first run".into(),
                focus: None,
                sid: first.clone(),
            }))
            .await
            .expect("journal ok");
        let wrapped = json_of(
            &client
                .call()
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "the first run is over".into(),
                    sid: first.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        let closed = wrapped["session"]["id"]
            .as_str()
            .expect("an id")
            .to_string();

        // Naming THAT session is blocked — you meant that record.
        let named = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "one more thing".into(),
                    focus: None,
                    sid: first.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(
            named["status"], "blocked",
            "a closed session takes no more entries: {named}"
        );

        // Booting the BOT again starts its next run — the identity outlives the
        // run, and the door is where the name is given now.
        let second = booted(&client.call(), "gamma").await;
        let next = json_of(
            &client
                .call()
                .journal(Parameters(JournalArgs {
                    entry: "the second run".into(),
                    focus: None,
                    sid: second,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            next["session"],
            closed.as_str(),
            "a new run, not the closed one: {next}"
        );

        let all = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 2, "two runs of one role: {all:?}");
        assert_eq!(
            all.iter().filter(|s| !s.state.is_terminal()).count(),
            1,
            "…and exactly one of them is open"
        );
    }

    /// **Writing with another identity's `sid` does not move mine.** The
    /// connection used to carry the identity, and one `journal` addressed at
    /// another bot rebound the whole thing: every later call, and every
    /// automatic beat, attributed to delta while gamma's own beats orphaned. A
    /// `sid` cannot do that — it addresses one run and says nothing about the
    /// caller's other handles — and this pins that it stays so.
    ///
    /// This is the stateful-transport shape — stdio, where connections really
    /// do persist — so it holds one handler across calls on purpose: the shape
    /// where a leftover binding would still have somewhere to live.
    #[tokio::test]
    async fn writing_with_another_identitys_sid_leaves_mine_where_it_was() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());
        make_bot(&jojobot, "gamma", None).await;
        make_bot(&jojobot, "delta", None).await;

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_session = mine["session"].as_str().expect("a session").to_string();

        // A deliberate write into the other identity's session.
        let other = booted(&jojobot, "delta").await;
        let theirs = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "a note for delta".into(),
                    focus: None,
                    sid: other,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            theirs["session"],
            my_session.as_str(),
            "it landed in delta's session"
        );

        // …and I am still gamma.
        let after = journal_entry(&jojobot, &sid, "my second beat").await;
        assert_eq!(
            after["session"],
            my_session.as_str(),
            "the connection is still gamma's"
        );

        let gamma = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(gamma.len(), 1, "one session for gamma: {gamma:?}");
        assert_eq!(gamma[0].entries.len(), 2, "…carrying both of my beats");
        let delta = store
            .sessions_of(&EntityId("bot:delta".into()))
            .await
            .expect("list ok");
        assert_eq!(delta.len(), 1, "and one for delta");
        assert_eq!(delta[0].entries.len(), 1);
    }

    /// **Two identities alive on ONE connection each keep their own session.**
    /// There used to be a per-connection binding here, and a short-circuit that
    /// read it instead of the board; the risk it carried was a cache that
    /// answered for whichever identity spoke last. Nothing remembers anything
    /// between calls now, so the answer comes from the `sid` every time — and
    /// this holds one handler across all of it, which is the transport shape
    /// where such a cache could have existed at all.
    #[tokio::test]
    async fn two_identities_on_one_connection_each_keep_their_own_session() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = connection(memory.clone(), store.clone());
        make_bot(&jojobot, "gamma", None).await;
        make_bot(&jojobot, "delta", None).await;

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_session = mine["session"].as_str().expect("a session").to_string();

        // My own handle must land in MY session.
        let named = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "named myself".into(),
                    focus: None,
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(
            named["session"],
            my_session.as_str(),
            "my own handle lands in my own session, not another: {named}"
        );

        // And a DIFFERENT identity's handle must not be served from mine.
        let theirs = booted(&jojobot, "delta").await;
        let other = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "named somebody else".into(),
                    focus: None,
                    sid: theirs,
                }))
                .await
                .expect("journal ok"),
        );
        assert_ne!(
            other["session"],
            my_session.as_str(),
            "gamma's session must not answer for delta's handle: {other}"
        );
    }

    /// **BLOCKER: a write must not mint a session for a bot that does not
    /// exist.** The door refuses an unknown name with the roster, and its own
    /// comment says why — a session bound to an identity jojobot just refused
    /// belongs to nobody. Making the bot NAME the address opened a second door
    /// into `begin` with no such screen; making the HANDLE the address closes it
    /// for good, because a handle is not a thing a caller can compose. jojobot
    /// either issued it or it did not.
    ///
    /// What a typo costs if this ever regresses: one permanent card (there is no
    /// delete verb; the sweep only marks it `abandoned` a day later), a beat
    /// misattributed away from the caller's real session, and through
    /// `wrap_session` a dated story written into the operator's Journal under a
    /// run nobody started.
    #[tokio::test]
    async fn a_session_verb_carrying_an_unheld_handle_blocks_and_writes_nothing() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;
        // A well-formed handle jojobot never minted — the nearest thing left to
        // the typo this spec was about.
        let typo = "gamm";

        for (verb, body) in [
            (
                "journal",
                json_of(
                    &client
                        .call()
                        .journal(Parameters(JournalArgs {
                            entry: "read the hand-off".into(),
                            focus: None,
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "wrap_session",
                json_of(
                    &client
                        .call()
                        .wrap_session(Parameters(WrapSessionArgs {
                            story: "a story for nobody".into(),
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "amend_journal",
                json_of(
                    &client
                        .call()
                        .amend_journal(Parameters(AmendJournalArgs {
                            entry: "actually".into(),
                            sid: typo.into(),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
        ] {
            assert_eq!(
                body["status"], "blocked",
                "{verb} minted a session for a handle nobody was given: {body}"
            );
            assert_eq!(body["wrote"], false);
            assert_eq!(
                body["attempted"], typo,
                "{verb}: the refusal quotes it back: {body}"
            );
            // **No candidates, and that is the difference from a name.** A bot
            // name is a thing jojobot can suggest neighbours for; a handle is
            // four characters of entropy, and the nearest one is somebody
            // else's session. Guessing here would hand a caller a run that is
            // not theirs, so the way out is to boot rather than to pick.
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("start_here"),
                "{verb}: the way out is the door, not a neighbour: {how}"
            );
        }

        assert!(
            client
                .sessions
                .sessions_of(&EntityId("bot:gamm".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "no card was written for an identity nobody created"
        );
        // …and the Journal was not told a story by a bot that does not exist.
        let journal: String = client
            .memory
            .scan()
            .await
            .expect("scan ok")
            .into_iter()
            .map(|d| d.prose)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !journal.contains("a story for nobody"),
            "the Journal is untouched: {journal}"
        );
    }

    /// The other two session verbs take the same one address — a stateless
    /// client has to be able to amend and to wrap, not only to journal.
    #[tokio::test]
    async fn a_stateless_client_can_amend_and_wrap_by_carrying_its_sid() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma", None).await;
        let sid = booted(&client.call(), "gamma").await;

        // Amending before anything exists is still refused, not a begin.
        let nothing = json_of(
            &client
                .call()
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "actually".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(
            nothing["status"], "blocked",
            "nothing to amend, and nothing begun: {nothing}"
        );
        assert!(
            client
                .sessions
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "an amend never mints a card"
        );

        client
            .call()
            .journal(Parameters(JournalArgs {
                entry: "read the hand-off".into(),
                focus: None,
                sid: sid.clone(),
            }))
            .await
            .expect("journal ok");

        let amended = json_of(
            &client
                .call()
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off, and scoped it".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("call ok"),
        );
        assert_ne!(amended["status"], "blocked", "{amended}");

        let wrapped = json_of(
            &client
                .call()
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the thing and told the story".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");

        let live = client
            .sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session across five connections: {live:?}"
        );
        assert_eq!(
            live[0].entries[0].text, "read the hand-off, and scoped it",
            "the amend landed in place: {:?}",
            live[0].entries
        );
    }

    /// **A boot that fails leaves a session already in flight alone.** A typo in
    /// a bot name must not disturb the handle its caller is already writing
    /// under — that would turn one mistyped call into lost work on the next
    /// write, and a boot has no business reaching a run it did not name.
    #[tokio::test]
    async fn a_failed_boot_leaves_a_live_sid_writing_where_it_was() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_id = mine["session"].as_str().expect("a session id").to_string();

        // A name that is no bot.
        let missed = boot(&jojobot, "nobody-by-that-name").await;
        assert_eq!(missed["status"], "blocked", "the boot missed: {missed}");

        // …and the next write is still mine.
        let after = journal_entry(&jojobot, &sid, "my second beat").await;
        assert_eq!(
            after["session"],
            my_id.as_str(),
            "the handle still addresses the same run after the miss"
        );
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "and no second card was minted: {live:?}");
        assert_eq!(live[0].entries.len(), 2);
    }

    async fn journal_entry(jojobot: &Jojobot, sid: &str, entry: &str) -> serde_json::Value {
        let result = jojobot
            .journal(Parameters(JournalArgs {
                entry: entry.into(),
                focus: None,
                sid: sid.into(),
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
            assert_eq!(
                display_line(input),
                expected,
                "the stored focus changed for {input:?}"
            );
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
        assert!(
            booted["session"]["session"].is_null(),
            "…and no card was written"
        );
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a boot that never works must leave no card at all"
        );

        // The first beat is what brings it into being.
        let sid = sid_of(&booted).expect("a handle");
        let journalled = journal_entry(&jojobot, &sid, "read the hand-off").await;
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
            let sid = booted(&jojobot, "gamma").await;

            let body = json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: entry.into(),
                        focus: None,
                        sid,
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
        let sid = booted(&jojobot, "gamma").await;

        let story = "read the hand-off and found nothing to do.\n\nWrapping without a beat: the \
                     `dev` box was empty and there was no slice to build.";
        let body = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    sid,
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
        let sid = booted(&jojobot, "gamma").await;

        let err = jojobot
            .journal(Parameters(JournalArgs {
                entry: "read the hand-off".into(),
                focus: Some("two\nlines".into()),
                sid,
            }))
            .await
            .expect_err("a focus that is not one line must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// **A reconnect is OFFERED the work in flight.** A session is the unit of
    /// work, not of connection, so a second boot of the same identity finds the
    /// live run and hands back its chronology rather than forking a new one —
    /// which is the whole reason a device hop is survivable.
    ///
    /// It is offered rather than attached: the run comes back as a choice named
    /// by what it was working on, and resuming it is the caller's answer. The
    /// difference matters most for the case the offer exists for — a run left
    /// open on purpose, for somebody who has not arrived yet.
    #[tokio::test]
    async fn booting_again_is_offered_the_session_in_flight() {
        let store = Arc::new(InMemorySessions::new());
        let memory = Arc::new(InMemoryMemory::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let first = connection_sharing(memory.clone(), store.clone(), registry.clone());
        make_bot(&first, "gamma", None).await;
        let sid = booted(&first, "gamma").await;
        let started = journal_entry(&first, &sid, "read the hand-off").await;

        // A different connection over the same worlds, exactly as a reconnect
        // builds one — a fresh binding, so anything it knows it read.
        let second = connection_sharing(memory, store.clone(), registry);
        let offered = boot(&second, "gamma").await;
        assert!(
            sid_of(&offered).is_none(),
            "the choice comes first: {offered}"
        );
        let choice = &offered["session"]["choices"][0];
        assert_eq!(choice["working_on"], "read the hand-off");

        let resumed = boot_answering(
            &second,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], started["session"]);
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"], "read the hand-off",
            "the work in flight comes back with it: {resumed}"
        );

        // …and writing on the new connection continues the same session.
        let again = sid_of(&resumed).expect("the resumed handle");
        journal_entry(&second, &again, "picked it back up").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one session, not two: {live:?}");
        assert_eq!(live[0].entries.len(), 2);
    }

    /// **The sweep, and what it is measured from.** A session that has gone a
    /// day without a beat is closed as `abandoned` at the next boot of its bot —
    /// never deleted, never wrapped, because its story was never told.
    ///
    /// **And the same boot offers it straight back**, which is not a
    /// contradiction: sweeping records that the run stopped, and the offer is
    /// how "resume last session" reaches it. A run that stopped yesterday is the
    /// archetypal thing a returning agent means, so closing it and then hiding
    /// it would make the sweep a way of losing work rather than of marking it.
    #[tokio::test]
    async fn a_stale_session_is_swept_to_abandoned_at_the_next_boot() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        // Begun two days ago and never touched since.
        let stale = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
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
        assert_eq!(
            booted["session"]["resumed"], false,
            "sweeping resumes nothing by itself — the caller still chooses"
        );

        let read = store.read_session(&stale.id).await.expect("read ok");
        assert_eq!(read.state, mailbox_state_abandoned(), "closed, not deleted");
        assert_eq!(
            read.focus, "something from the day before yesterday",
            "…and its record is untouched"
        );

        // **The run this very boot swept is the one it offers back.** It
        // stopped the day before yesterday, which is exactly the run a
        // returning agent means by "resume last session".
        let choice = &booted["session"]["choices"][0];
        assert_eq!(
            choice["state"], "abandoned",
            "offered, and marked: {booted}"
        );
        assert_eq!(
            choice["working_on"],
            "something from the day before yesterday"
        );

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            choice["sid"].as_str().expect("an addressable option"),
        )
        .await;
        assert_eq!(resumed["session"]["session"]["id"], stale.id.as_str());
        assert_eq!(
            resumed["session"]["session"]["state"], "active",
            "…and taking the offer reopens it: {resumed}"
        );
    }

    /// A session that is merely quiet — an hour, not a day — is still yours, and
    /// being offered it back is the point.
    #[tokio::test]
    async fn a_recent_session_is_offered_back_rather_than_swept() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let recent = store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: Sid("t001".into()),
                focus: "still going".into(),
                started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(1),
            })
            .await
            .expect("begin ok");

        let booted = boot(&jojobot, "gamma").await;
        assert_eq!(booted["session"]["choices"][0]["working_on"], "still going");
        assert_eq!(booted["session"]["swept"], serde_json::json!([]));

        let resumed = boot_answering(
            &jojobot,
            "gamma",
            booted["session"]["choices"][0]["sid"]
                .as_str()
                .expect("an option"),
        )
        .await;
        assert_eq!(resumed["session"]["resumed"], true);
        assert_eq!(resumed["session"]["session"]["id"], recent.id.as_str());
    }

    /// **The whole arc through the surface:** boot, journal with a focus, amend
    /// the beat, wrap. The focus is current truth and the chronology is history,
    /// and the wrap writes the story to both the session and the Journal.
    #[tokio::test]
    async fn the_session_arc_through_the_handler() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off and scoped the slice".into(),
                    focus: Some("building the session context".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        assert_eq!(first["focus"], "building the session context");
        assert!(
            first["entry"]["beat"].is_null(),
            "a session's own entry is not a beat"
        );

        let amended = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "read the hand-off and scoped the slice properly".into(),
                    sid: sid.clone(),
                }))
                .await
                .expect("amend ok"),
        );
        assert_eq!(amended["entry"]["id"], first["entry"]["id"], "in place");

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "built the session context; the sweep is lazy until M8".into(),
                    sid: sid.clone(),
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
        let sid = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;
        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done".into(),
                sid: sid.clone(),
            }))
            .await
            .expect("wrap ok");

        let refused = |body: serde_json::Value, verb: &str| {
            assert_eq!(body["status"], "blocked", "{verb} must be blocked: {body}");
            assert_eq!(body["wrote"], false);
            let how = body["how_to_proceed"].as_str().expect("advice");
            // **Why this end is the last word, not merely that it is.** A
            // wrapped run's story is already a dated entry in the operator's
            // Journal, and that published account is what reopening would
            // falsify — which is also what makes this refusal different from
            // the one an abandoned run gets.
            assert!(
                how.contains("story has been told") && how.contains("Journal"),
                "{verb} has to say why: {how}"
            );
        };
        refused(
            json_of(
                &jojobot
                    .journal(Parameters(JournalArgs {
                        entry: "one more thing".into(),
                        focus: None,
                        sid: sid.clone(),
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
                        sid: sid.clone(),
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
                        sid: sid.clone(),
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
                    sid: String::new(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        // **The remedy must be one that works on the caller's next call.** It
        // used to say "call boot_bot" — a verb that bound a connection most clients
        // do not keep, so the very next call landed back here. `bot` is the
        // address that survives, and this is the message that has to say so.
        assert!(
            how.contains("`sid`"),
            "the way out names the address: {how}"
        );
    }

    /// **Amending a session that has not begun is refused, not turned into a
    /// first entry.** A correction that silently became an append leaves a
    /// chronology saying something nobody meant.
    #[tokio::test]
    async fn amending_before_the_first_entry_is_blocked_and_writes_nothing() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "there is nothing to correct".into(),
                    sid,
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
        let sid = booted(&jojobot, "gamma").await;

        ensure_as(&jojobot, &sid, "alpha").await;
        ensure_as(&jojobot, &sid, "milhouse").await;
        capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        capture_as(&jojobot, &sid, capture_args("milhouse", "plays chess")).await;
        journal_entry(&jojobot, &sid, "captured a couple of things").await;

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
            beats
                .iter()
                .filter(|(class, _)| *class == "capture")
                .count(),
            1,
            "one beat for the class, however many captures: {entries:?}"
        );
        let (_, tally) = beats
            .iter()
            .find(|(class, _)| *class == "capture")
            .expect("a capture beat");
        assert!(
            tally.contains("(2)"),
            "…with its count kept current: {tally}"
        );
        assert!(
            tally.contains("person:alpha"),
            "…and what it touched: {tally}"
        );
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
        let sid = booted(&first, "gamma").await;
        ensure_as(&first, &sid, "alpha").await;
        capture_as(&first, &sid, capture_args("alpha", "plays go")).await;

        // A reconnect, then another capture. The handle is the address, so
        // resuming the run in flight is what the reconnect answers with.
        let second = connection(memory, store.clone());
        let again = resumed(&second, "gamma").await;
        ensure_as(&second, &again, "milhouse").await;
        capture_as(&second, &again, capture_args("milhouse", "plays chess")).await;

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
        let sid = booted(&first, "gamma").await;
        ensure_as(&first, &sid, "alpha").await;
        capture_as(&first, &sid, capture_args("alpha", "plays go")).await;

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
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
        let again = resumed(&second, "gamma").await;
        ensure_as(&second, &again, "milhouse").await;
        capture_as(&second, &again, capture_args("milhouse", "plays chess")).await;

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
            let sid = booted(&jojobot, bot).await;
            jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    sid,
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
        let sid = booted(&jojobot, "gamma").await;
        let started = journal_entry(&jojobot, &sid, "read the hand-off").await;
        let session = started["session"]
            .as_str()
            .expect("a session id")
            .to_string();

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
                sid,
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
    ///
    /// The cards are begun straight on the board and addressed by a handle
    /// minted for each — the `sid` is the only address a verb takes now, so
    /// "name that particular run" means "hold a handle to it".
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
                        sid: Sid(format!("r{n:03}")),
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
            let sid = as_run(&jojobot, "gamma", session);
            jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: story.into(),
                    sid,
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

    /// **Wrapping one session leaves every other one running.** A wrap reaches
    /// exactly the run its handle addresses: the session it closes, the story it
    /// tells, and nothing else. It used to clear the connection's binding
    /// regardless of which session it had been pointed at, orphaning the live
    /// one, losing its tally, and making the next write mint a second card for a
    /// session that was already running.
    ///
    /// **The binding is gone, so that mechanism cannot recur** — what is pinned
    /// here is the invariant it broke, now that a handle is the only address:
    /// closing somebody else's run leaves this one's card, tally and chronology
    /// exactly where they were.
    #[tokio::test]
    async fn wrapping_another_session_leaves_this_one_running() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma", None).await;

        // Somebody else's session, on the same board.
        let theirs = store
            .begin(NewSession {
                bot: EntityId("bot:delta".into()),
                sid: Sid("d001".into()),
                focus: "their run".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");
        store
            .append(
                &theirs.id,
                NewEntry::manual("their beat", jiff::Timestamp::now()),
            )
            .await
            .expect("append ok");

        let sid = booted(&jojobot, "gamma").await;
        let mine = journal_entry(&jojobot, &sid, "my first beat").await;
        let my_id = mine["session"].as_str().expect("a session id").to_string();

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "wrapping theirs".into(),
                sid: as_run(&jojobot, "delta", &theirs.id),
            }))
            .await
            .expect("wrap ok");

        // My next beat continues MY session rather than minting a second card.
        journal_entry(&jojobot, &sid, "my second beat").await;
        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(live.len(), 1, "one card for this run, not two: {live:?}");
        assert_eq!(live[0].id.as_str(), my_id);
        assert_eq!(
            live[0].entries.len(),
            2,
            "…and it kept accruing: {:?}",
            live[0].entries
        );
    }

    /// **amend_journal triages the same way the other two do.** A caller with no
    /// identity is told to boot — not told there is nothing to amend, which is a
    /// different fact about a different thing.
    #[tokio::test]
    async fn amending_without_a_boot_says_to_boot_rather_than_no_entries() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "actually, it was the other thing".into(),
                    // No boot, so no handle to carry. `sid` is a required
                    // parameter now, so "never booted" reaches the verb as an
                    // empty one rather than as an absent field.
                    sid: String::new(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        // **The remedy has to be one that works.** This is the message a
        // stateless caller sees, and identity survives nothing but the handle —
        // so the advice has to name the handle and the door that mints it,
        // rather than pointing back into the loop this refusal exists to close.
        assert!(
            how.contains("`sid`"),
            "the way out names the parameter: {how}"
        );
        assert!(
            how.contains("start_here"),
            "…and the door that hands one over: {how}"
        );
        assert!(
            !how.contains("no entries"),
            "…and it does not answer about a session nobody looked for: {how}"
        );
    }

    /// **A handle jojobot is not holding is refused by the write verbs, not
    /// quietly ignored.**
    ///
    /// These seven verbs take an optional `sid`, and `beat` was the only place
    /// any of them looked at it. `beat` is silent by design — three cases where
    /// there is nobody to record for — and it swallowed the refusal along with
    /// them, because it read `caller()` as "some caller or none" when that
    /// method distinguishes THREE answers: nobody (fine), a handle that is not
    /// a handle, and a handle whose session is gone.
    ///
    /// So a caller holding a dead sid wrote successfully, its chronology
    /// silently stopped, and it found out at wrap or never — which is the
    /// failure mode a handle exists to prevent, arriving in the one shape
    /// nothing reports.
    ///
    /// **Refused BEFORE the write, not propagated out of `beat`**, which runs
    /// after it: `blocked` means `wrote: false` everywhere on this surface, and
    /// one handed back over a write that already landed would be a worse lie
    /// than the silence.
    #[tokio::test]
    async fn a_dead_sid_is_refused_by_the_write_verbs_rather_than_swallowed() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        ensure(&jojobot, "alpha").await;
        make_bot(&jojobot, "gamma", None).await;
        make_box(&jojobot, "somewhere").await;
        let posted = json_of(
            &jojobot
                .post_message(Parameters(PostMessageArgs {
                    mailbox: "somewhere".into(),
                    body: "something to retire later".into(),
                    sid: booted(&jojobot, "gamma").await,
                    subject: None,
                    in_reply_to: None,
                }))
                .await
                .expect("post ok"),
        );
        let message = posted["id"].as_str().expect("an id").to_string();

        // Well-formed and never minted: the shape a handle takes after a
        // restart, or after the run it named was swept.
        let dead = "2gf7".to_string();
        assert!(sid::is_readable(&dead));

        let refusals: Vec<(&str, serde_json::Value)> = vec![
            (
                "capture",
                blocked(
                    &jojobot
                        .capture(Parameters(CaptureArgs {
                            sid: Some(dead.clone()),
                            ..capture_args("alpha", "plays go")
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "add_entity",
                blocked(
                    &jojobot
                        .add_entity(Parameters(AddEntityArgs {
                            kind: "person".into(),
                            handle: "milhouse".into(),
                            name: "Milhouse".into(),
                            aliases: None,
                            source: "test-fixture".into(),
                            crm: None,
                            boot: None,
                            mailbox: None,
                            create_new: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "update_entity",
                blocked(
                    &jojobot
                        .update_entity(Parameters(UpdateEntityArgs {
                            handle: "person:alpha".into(),
                            name: Some("Alpha".into()),
                            aliases: None,
                            source: None,
                            crm: None,
                            mailbox: None,
                            create_new: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "update_fact",
                blocked(
                    &jojobot
                        .update_fact(Parameters(UpdateFactArgs {
                            sid: Some(dead.clone()),
                            ..update_args("person:alpha#1")
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "set_charter",
                blocked(
                    &jojobot
                        .set_charter(Parameters(SetCharterArgs {
                            bot: "gamma".into(),
                            prose: "a charter nobody asked for".into(),
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "create_mailbox",
                blocked(
                    &jojobot
                        .create_mailbox(Parameters(CreateMailboxArgs {
                            name: "nowhere".into(),
                            create_new: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
            (
                "mark_processed",
                blocked(
                    &jojobot
                        .mark_processed(Parameters(MarkProcessedArgs {
                            message_id: message.clone(),
                            notes: None,
                            sid: Some(dead.clone()),
                        }))
                        .await
                        .expect("call ok"),
                ),
            ),
        ];
        for (verb, body) in &refusals {
            assert_eq!(
                body["attempted"], dead,
                "{verb} must name the handle it refused: {body}"
            );
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("gone") && how.contains("start_here"),
                "{verb} must say the session is gone and where to get another: {how}"
            );
        }

        // …and every one of them wrote nothing, which is what `wrote: false`
        // above is claiming.
        assert!(
            !jojobot
                .memory
                .list_entities(None)
                .await
                .expect("list ok")
                .iter()
                .any(|e| e.id.as_str() == "person:milhouse"),
            "add_entity wrote an entity behind a refusal"
        );
        assert!(
            jojobot
                .memory
                .recall(&EntityId("person:alpha".into()))
                .await
                .expect("recall ok")
                .is_empty(),
            "capture wrote a fact behind a refusal"
        );
        assert!(
            !jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .iter()
                .any(|b| b.name.as_str() == "nowhere"),
            "create_mailbox opened a box behind a refusal"
        );
    }

    /// **A call carrying no identity auto-journals nothing** — not even when
    /// there is exactly one live session it could obviously have meant.
    ///
    /// jojobot does not guess which session made a call. The temptation is the
    /// single-candidate case: one bot, one run in flight, an unaddressed write
    /// arriving — and resolving it "helpfully" attributes somebody's work to a
    /// session they did not name.
    ///
    /// **The fixture is the assertion here.** This booted nothing and wrote
    /// nothing before, so the board was empty and there was nothing for a
    /// guessing implementation to guess FROM: restoring the deleted
    /// fall-back-to-the-one-live-run resolver left the suite green, which is a
    /// test that cannot fail. There is a card on the board now, warm and
    /// unambiguous, and the anonymous write must still leave it alone.
    #[tokio::test]
    async fn a_call_carrying_no_sid_writes_no_beats() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        ensure(&jojobot, "alpha").await;
        make_bot(&jojobot, "gamma", None).await;

        // One live run, with a card already materialized: the single obvious
        // candidate an unaddressed write would land in if anything resolved it.
        let sid = booted(&jojobot, "gamma").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("alpha", "plays go")
            },
        )
        .await;
        let chronology = || async {
            let runs = store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok");
            assert_eq!(runs.len(), 1, "one run in flight: {runs:?}");
            runs[0]
                .entries
                .iter()
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
        };
        let before = chronology().await;
        assert_eq!(
            before.len(),
            1,
            "…carrying the beat for the write that named it: {before:?}"
        );

        // **Two anonymous writes, of two classes, because they fail
        // differently.** A guessing resolver reached by a class the session
        // already has AMENDS that beat in place — the entry count never moves,
        // so counting entries alone is a test that still cannot fail. A class it
        // does not have opens a new one.
        capture_ok(&jojobot, capture_args("alpha", "plays go on tuesdays")).await;
        jojobot
            .create_mailbox(Parameters(CreateMailboxArgs {
                name: "nobodys-box".into(),
                create_new: None,
                sid: None,
            }))
            .await
            .expect("create ok");

        let after = chronology().await;
        assert_eq!(
            after, before,
            "an anonymous write lands in nobody's chronology, however obvious the candidate — \
             neither as a new beat nor as a count moving on one that is already there"
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
            self.refuse
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Sessions for RefusingClose {
        async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
            self.inner.sessions_of(bot).await
        }
        async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
            self.inner.all_sessions().await
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
        async fn amend_last(
            &self,
            id: &SessionId,
            text: &str,
        ) -> Result<JournalEntry, SessionError> {
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
        async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.inner.reopen(id).await
        }
    }

    /// A handler over a store whose close refuses, and the handle a boot as
    /// `gamma` hands back — the fixture both wrap-retry specs start from.
    async fn refusing_close() -> (Jojobot, Arc<RefusingClose>, Arc<InMemoryMemory>, String) {
        let store = Arc::new(RefusingClose::new());
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            store.clone(),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;
        (jojobot, store, memory, sid)
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
        let (jojobot, store, memory, sid) = refusing_close().await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                sid: sid.clone(),
            }))
        };
        assert!(
            wrap().await.is_err(),
            "the close refused, so the wrap failed"
        );

        // The retry, with the close working this time.
        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store
            .inner
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
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

    /// **A retry finishes what the first attempt started, wherever the story now
    /// sits.** The chronology half of the guard looked only at the newest entry,
    /// so anything written between the failed close and the retry — a journal
    /// entry saying the wrap failed, which is the natural thing to write — pushed
    /// the story off the tail and the retry told it a second time.
    #[tokio::test]
    async fn a_wrap_retried_after_an_intervening_entry_tells_the_story_once() {
        let (jojobot, store, memory, sid) = refusing_close().await;
        journal_entry(&jojobot, &sid, "read the hand-off").await;

        let story = "built the thing; the close is what failed";
        let wrap = || {
            jojobot.wrap_session(Parameters(WrapSessionArgs {
                story: story.into(),
                sid: sid.clone(),
            }))
        };
        assert!(
            wrap().await.is_err(),
            "the close refused, so the wrap failed"
        );

        // The natural next beat: saying so. It is now the tail, not the story.
        journal_entry(&jojobot, &sid, "the wrap failed at the close — retrying").await;

        store.allow_close();
        let second = json_of(&wrap().await.expect("the retry must land"));
        assert_eq!(second["session"]["state"], "wrapped");

        let live = store
            .inner
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
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
        async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
            self.0.all_sessions().await
        }
        async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.read_session(id).await
        }
        /// **Yields on both sides of the write, because reality does.** A real
        /// `begin` is a round trip: the card exists on the board the moment the
        /// server commits it, and the caller learns its id only when the
        /// response comes back. A double that suspends only on the way in never
        /// makes the board observable without its registry entry, which is the
        /// one interleaving worth being hostile about here.
        async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
            self.pause().await;
            let begun = self.0.begin(new).await;
            self.pause().await;
            begun
        }
        async fn append(
            &self,
            id: &SessionId,
            entry: NewEntry,
        ) -> Result<JournalEntry, SessionError> {
            self.pause().await;
            self.0.append(id, entry).await
        }
        async fn amend_last(
            &self,
            id: &SessionId,
            text: &str,
        ) -> Result<JournalEntry, SessionError> {
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
        async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
            self.pause().await;
            self.0.reopen(id).await
        }
    }

    /// A handler whose session store yields at every call — see [`Yielding`].
    fn racing(store: Arc<InMemorySessions>) -> Jojobot {
        Jojobot::new(
            Arc::new(InMemoryMemory::new()),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(Yielding(store)),
            Arc::new(sid::SessionRegistry::new()),
        )
    }

    /// **Two tool calls in flight on one handle must not fork the session.**
    /// rmcp runs one task per request, and the card behind a handle is read,
    /// awaited across, and written back — so without a gate both calls see "no
    /// card yet" and both materialize one, and two same-class verbs both append
    /// a beat.
    #[tokio::test]
    async fn concurrent_first_writes_materialize_exactly_one_card() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;

        let one = jojobot.journal(Parameters(JournalArgs {
            entry: "first".into(),
            focus: None,
            sid: sid.clone(),
        }));
        let two = jojobot.journal(Parameters(JournalArgs {
            entry: "second".into(),
            focus: None,
            sid: sid.clone(),
        }));
        let (a, b) = tokio::join!(one, two);
        a.expect("journal ok");
        b.expect("journal ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, not one per racing call: {live:?}"
        );
        assert_eq!(live[0].entries.len(), 2, "…carrying both entries");
    }

    /// The same race, one class down: two concurrent captures must leave one
    /// beat, not two.
    ///
    /// **Counting the class on one card is not enough to see the failure.** A
    /// beat mints the card it writes to, so an ungated race forks — and each of
    /// the two cards then carries exactly one beat of the class, which reads as
    /// a pass on whichever card is looked at. The card count is what turns the
    /// fork into a failure, so it is asserted first.
    #[tokio::test]
    async fn concurrent_same_class_verbs_leave_exactly_one_beat() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = racing(store.clone());
        make_bot(&jojobot, "gamma", None).await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let (a, b) = tokio::join!(
            jojobot.capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("alpha", "plays go")
            })),
            jojobot.capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("milhouse", "plays chess")
            })),
        );
        a.expect("capture ok");
        b.expect("capture ok");

        let live = store
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("list ok");
        assert_eq!(
            live.len(),
            1,
            "one session, not one per racing capture: {live:?}"
        );
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

    /// **Two writers on one identity, on two connections, must not fork it.**
    ///
    /// The gate that stops this was a mutex on the HANDLER, and the transport
    /// builds one handler per connection — so it excluded nothing between calls,
    /// which on a client with no session affinity means it excluded nothing at
    /// all. Both callers read a card that did not exist, both began one, and the
    /// loser's chronology was orphaned on a card nothing would ever address
    /// again: a session whose story is never told, by construction.
    ///
    /// It was masked while addressing was by bot name and the board was
    /// re-resolved every call. Nothing masks it now that the `sid` names one
    /// specific session, so the lock lives on the one structure that is already
    /// process-wide and already keyed by the thing being serialized — the
    /// registry, which two connections of one process share and a handler does
    /// not.
    ///
    /// **Both orders, because only one of them forks.** `tokio::join!` rotates
    /// which future it polls first.
    #[tokio::test]
    async fn two_connections_writing_as_one_bot_do_not_fork_the_card() {
        for first_wins in [true, false] {
            let client = NoAffinity::new();
            make_bot(&client.call(), "gamma", None).await;
            // The handle outlives the connection that was handed it — that is
            // the whole point of it — so one boot serves both writers below.
            let sid = booted(&client.call(), "gamma").await;

            // **A store that yields between its steps.** The in-memory fake
            // never suspends inside the read-then-begin span, so two futures on
            // one runtime finish it one after the other and the race cannot
            // happen — a green test proving nothing. This is the same double the
            // single-connection race test uses.
            let racing_ports = |ports: &NoAffinity| {
                Jojobot::new(
                    ports.memory.clone(),
                    Arc::new(SpySearch::default()),
                    ports.mailboxes.clone(),
                    Arc::new(Yielding(ports.sessions.clone())),
                    ports.registry.clone(),
                )
            };
            let write = |entry: &'static str| {
                let jojobot = racing_ports(&client);
                let sid = sid.clone();
                async move {
                    jojobot
                        .journal(Parameters(JournalArgs {
                            entry: entry.into(),
                            focus: None,
                            sid,
                        }))
                        .await
                        .expect("journal ok")
                }
            };

            // Two connections, as two agents booted as one identity present —
            // or as one assistant turn issuing parallel tool calls.
            let (a, b) = (write("the first beat"), write("the second beat"));
            if first_wins {
                let (x, y) = tokio::join!(a, b);
                json_of(&x);
                json_of(&y);
            } else {
                let (y, x) = tokio::join!(b, a);
                json_of(&x);
                json_of(&y);
            }

            let live: Vec<Session> = client
                .sessions
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .into_iter()
                .filter(|s| !s.state.is_terminal())
                .collect();
            assert_eq!(
                live.len(),
                1,
                "first_wins={first_wins}: one card, not one per connection: {live:?}"
            );
            assert_eq!(
                live[0].entries.len(),
                2,
                "first_wins={first_wins}: …and neither beat was orphaned: {:?}",
                live[0].entries
            );
        }
    }

    /// **A boot writes nothing a concurrent first write can lose.** A boot reads
    /// the board, sweeps what is stale and answers; a write on a handle already
    /// held reads that handle's card and begins one if there is none. The two
    /// overlap: sweeping a stale card is an await sitting inside the boot's
    /// board read, and that is exactly when the racing write gets to run.
    ///
    /// The old name promised a race this can no longer run. It forked because
    /// the boot wrote a connection binding at the end of that span, clearing the
    /// session the write had just materialized and rolling the tally back to
    /// what the stale read saw; the next write then minted a second card for a
    /// session already running. **The binding is gone** — the boot writes no
    /// identity anywhere a write reads from, so there is nothing left for it to
    /// clobber. What is pinned here is that: whatever the interleaving, the
    /// handle keeps addressing one card and the next write keeps accruing to it.
    /// The remaining overlap between the two — a boot reading the board inside
    /// the gap a first write leaves — is a different defect with its own test
    /// below.
    ///
    /// **Both orders, because only one of them forked.** `tokio::join!` rotates
    /// which future it polls first, so a single ordering proves whichever
    /// interleaving it happened to produce; the invariant is that neither
    /// produces two cards.
    #[tokio::test]
    async fn a_racing_boot_writes_nothing_the_first_write_can_lose() {
        for boot_first in [true, false] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = racing(store.clone());
            make_bot(&jojobot, "gamma", None).await;
            let sid = booted(&jojobot, "gamma").await;

            // Something for the racing boot to sweep. Closing it is an await
            // inside the boot's board read — the gap the racing write slips
            // through.
            store
                .begin(NewSession {
                    bot: EntityId("bot:gamma".into()),
                    sid: Sid(format!("t{:03}", line!() % 1000)),
                    focus: "from the day before yesterday".into(),
                    started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(48),
                })
                .await
                .expect("begin ok");

            let booting = jojobot.start_here(Parameters(OrientArgs {
                bot: Some("gamma".into()),
                brief: None,
                resume: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                entry: "the first beat".into(),
                focus: None,
                sid: sid.clone(),
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
            journal_entry(&jojobot, &sid, "the second beat").await;

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

    /// **One run answers to one handle, even when a boot reads the board in the
    /// middle of the write that creates it.**
    ///
    /// A first write begins the card and then tells the registry which handle it
    /// landed on, and those two are not one step: the card is on the board the
    /// moment the store commits it, and the registry learns of it only when the
    /// write's own future is polled again. A boot reading the board inside that
    /// gap finds a live run no handle addresses and mints a second one for it —
    /// so the offer names an address the run's own writer has never heard of,
    /// and one session answers to two names. That is the fork the per-run gate
    /// exists to prevent, one layer up.
    ///
    /// **The gate has to be keyed on the identity rather than the handle**,
    /// because that is the only key the two callers share: the boot knows the
    /// bot, the write knows its sid, and they are talking about the same run.
    /// Keying the boot on the bot and the write on its handle put them in
    /// different queues, which is a lock that excludes the pair it was for.
    ///
    /// **Both orders, and only one of them forks.** Polled boot-first, the board
    /// read lands before the card exists and the boot legitimately hands back a
    /// fresh handle with nothing behind it; polled write-first, the boot reads
    /// inside the gap. `tokio::join!` rotates which future it polls first, so a
    /// single ordering proves only whichever it happened to produce.
    #[tokio::test]
    async fn a_boot_reading_the_board_mid_write_offers_the_handle_the_run_has() {
        for boot_first in [true, false] {
            let store = Arc::new(InMemorySessions::new());
            let jojobot = racing(store.clone());
            make_bot(&jojobot, "gamma", None).await;
            let sid = booted(&jojobot, "gamma").await;

            let booting = jojobot.start_here(Parameters(OrientArgs {
                bot: Some("gamma".into()),
                brief: None,
                resume: None,
            }));
            let writing = jojobot.journal(Parameters(JournalArgs {
                entry: "the first beat, which is what mints the card".into(),
                focus: None,
                sid: sid.clone(),
            }));
            let booted_answer = if boot_first {
                let (b, w) = tokio::join!(booting, writing);
                w.expect("journal ok");
                json_of(&b.expect("boot ok"))
            } else {
                let (w, b) = tokio::join!(writing, booting);
                w.expect("journal ok");
                json_of(&b.expect("boot ok"))
            };

            // A boot that saw the card offers it back. Whether it saw one is the
            // interleaving's business; what it may never do is offer it under a
            // handle minted beside the one its writer is already using.
            if let Some(choices) = booted_answer["session"]["choices"].as_array() {
                for choice in choices {
                    assert_eq!(
                        choice["sid"].as_str(),
                        Some(sid.as_str()),
                        "boot_first={boot_first}: the offer minted a second handle for a run that \
                         already has one: {choice}"
                    );
                }
            }
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
    async fn booting_lands_a_session_knowing_which_identity_it_is() {
        let jojobot = handler();
        make_bot(&jojobot, "otto", Some("otto-inbox")).await;
        make_box(&jojobot, "otto-inbox").await;
        send(&jojobot, "otto-inbox", "epsilon", "the shipment landed").await;

        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "otto".into(),
                prose: "Keeps the schedule.\n\nHard line: never writes to the ledger.".into(),
                sid: None,
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
        assert!(
            body["orientation"]
                .as_str()
                .is_some_and(|o| o.contains("provenance"))
        );
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["bot"], 1);

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:otto");
        assert_eq!(me["bot"]["type"], "SoftwareApplication");
        assert!(
            me["charter"]
                .as_str()
                .is_some_and(|c| c.contains("never writes to the ledger")),
            "the charter is the orienting text, and it arrives: {me}"
        );

        let rules = me["rules"].as_array().expect("rules are a list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["content"], "answers before noon");
        assert_eq!(
            rules[0]["provenance"], "testimony",
            "a rule arrives with its provenance showing, or it reads as settled when it is a guess"
        );
        assert!(
            rules[0]["address"].as_str().is_some(),
            "and with the address that edits it"
        );

        let owned = &me["owned_mailbox"];
        assert_eq!(owned["name"], "otto-inbox");
        assert_eq!(
            owned["counts"]["new"], 1,
            "the state of its own box: {owned}"
        );
        assert_eq!(owned["exists"], true, "the box is there and says so");
    }

    /// **Booting creates nothing.** A bot whose declared box nobody has opened
    /// yet still boots — with the box reported plainly as not there, and the
    /// deliberate act named. Creation is an intentional act: `create_mailbox`
    /// is the only mint, and it is the only thing that runs the full name
    /// screen, so a door that minted on the side would be a door that minted
    /// near-duplicates nobody was ever shown.
    #[tokio::test]
    async fn booting_reports_a_missing_box_and_opens_nothing() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma", Some("sigma-inbox")).await;

        let body = boot(&jojobot, "sigma").await;
        let owned = &body["identity"]["owned_mailbox"];
        assert_eq!(owned["name"], "sigma-inbox");
        assert_eq!(
            owned["available"], true,
            "the world is up; the box is not there"
        );
        assert_eq!(owned["exists"], false, "said plainly: {owned}");
        assert!(
            owned["counts"].is_null(),
            "there are no counts for a box that is not there"
        );
        assert!(
            owned["how_to_proceed"]
                .as_str()
                .is_some_and(|a| a.contains("create_mailbox")),
            "the way forward is the deliberate verb: {owned}"
        );

        // **Nothing was created**, by this call or any number of them.
        for _ in 0..2 {
            boot(&jojobot, "sigma").await;
        }
        let listed = json_of(
            &jojobot
                .list_mailboxes(Parameters(ListMailboxesArgs { sid: None }))
                .await
                .expect("list ok"),
        );
        assert_eq!(
            listed["count"], 0,
            "booting must not put a box on the board: {listed}"
        );
    }

    /// …and once someone opens it deliberately, the same boot reports it live.
    #[tokio::test]
    async fn booting_reports_the_box_once_it_has_been_opened_deliberately() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma", Some("sigma-inbox")).await;
        assert_eq!(
            boot(&jojobot, "sigma").await["identity"]["owned_mailbox"]["exists"],
            false
        );

        make_box(&jojobot, "sigma-inbox").await;
        send(&jojobot, "sigma-inbox", "epsilon", "the shipment landed").await;

        let owned = boot(&jojobot, "sigma").await["identity"]["owned_mailbox"].clone();
        assert_eq!(owned["available"], true);
        assert_eq!(owned["exists"], true);
        assert_eq!(owned["counts"]["new"], 1, "got {owned}");
        assert!(
            owned["how_to_proceed"].is_null(),
            "nothing to advise: {owned}"
        );
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
        assert_eq!(
            body["attempted"], "gamma-inbo",
            "the suspicious thing is the box name"
        );
        assert_eq!(body["candidates"][0]["name"], "gamma-inbox");
        assert_eq!(body["candidates"][0]["reason"], "near");

        // Nothing was written — not the claim, and not the entity carrying it.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        assert_eq!(
            listed["count"], 0,
            "a blocked claim writes no entity: {listed}"
        );
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
        assert_eq!(
            sibling["mailbox"], "gamma-inbo",
            "the signal clears it: {sibling}"
        );

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
                sid: None,
            }))
            .await
            .expect("a near-miss claim is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "gamma-inbo");
        assert_eq!(body["candidates"][0]["name"], "gamma-inbox");

        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    sid: None,
                }))
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
            json_of(
                &jojobot
                    .list_mailboxes(Parameters(ListMailboxesArgs { sid: None }))
                    .await
                    .expect("list ok")
            )["mailboxes"]
                .as_array()
                .expect("boxes")
                .is_empty(),
            "a bot with no claim must not cause a box to appear"
        );
    }

    /// A name that is no bot comes back in the guards' own shape — nothing was
    /// written, here is what jojobot suspects you meant — rather than a fresh
    /// identity conjured out of a typo.
    ///
    /// **And with the roster, not only the near misses.** `candidates` answers
    /// "did you mean one of these", so it is EMPTY whenever the name resembles
    /// nothing — and an empty list reads as a broken server to the one caller
    /// who most needs telling who does exist. The way out is an offer: boot as
    /// somebody real and create the identity you wanted from in there.
    #[tokio::test]
    async fn booting_an_unknown_bot_answers_with_the_roster_and_an_offer() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;
        make_bot(&jojobot, "delta", None).await;
        ensure(&jojobot, "alpha").await;

        // A near miss: the candidates are the guards' own answer, and they stay.
        let near = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamm".into()),
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(near["attempted"], "bot:gamm");
        assert_eq!(near["candidates"][0]["handle"], "bot:gamma");

        // A name resembling nothing: the candidate list is empty, and the
        // answer still has to be useful.
        let stranger = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("nobody".into()),
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert!(
            stranger["candidates"]
                .as_array()
                .expect("a list")
                .is_empty(),
            "nothing resembles this name, which is exactly the case: {stranger}"
        );

        for body in [&near, &stranger] {
            let roster: Vec<&str> = body["bots"]
                .as_array()
                .expect("the roster is a list")
                .iter()
                .map(|b| b.as_str().expect("a handle"))
                .collect();
            assert_eq!(roster, ["bot:gamma", "bot:delta"], "who exists: {body}");
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("bot:gamma"),
                "the roster is in the words too: {how}"
            );
            assert!(
                how.contains("Boot as one of these") && how.contains("from inside that session"),
                "the offer is the way out: {how}"
            );
            assert!(
                how.contains("mints nothing"),
                "…and the door says what it will not do: {how}"
            );
        }

        // **Nothing was written.** Not the identity, not a session, not a box.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        assert_eq!(
            listed["count"], 2,
            "a refused boot mints no identity: {listed}"
        );
    }

    /// The empty board says something different, because "boot as one of these"
    /// is no offer when there is nobody to boot as.
    #[tokio::test]
    async fn booting_into_an_empty_roster_says_so_rather_than_offering_nobody() {
        let jojobot = handler();
        let body = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("gamma".into()),
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert!(body["bots"].as_array().expect("a list").is_empty());
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("no bots on this server") && how.contains("add_entity"),
            "with nobody to boot as, the way out is the verb that creates one: {how}"
        );
    }

    /// This door boots bots. A bare name is read as one, and a handle of another
    /// kind is the caller's mistake — booting a person as an identity would hand
    /// back somebody's page as a charter.
    #[tokio::test]
    async fn the_door_reads_a_bare_name_as_a_bot_and_refuses_another_kind() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        assert_eq!(
            boot(&jojobot, "bot:gamma").await["identity"]["bot"]["id"],
            "bot:gamma",
            "a fully qualified bot handle is the same door"
        );

        let err = jojobot
            .start_here(Parameters(OrientArgs {
                bot: Some("person:milhouse".into()),
                brief: None,
                resume: None,
            }))
            .await
            .expect_err("another kind must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("bot"),
            "the error says what this door takes: {}",
            err.message
        );
    }

    /// **Both halves of the door make the same promise, so both keep it.** `orient` says
    /// orientation lands even when a world is down — and `start_here` did,
    /// while the identified half hard-errored the moment a bot owned a box, which made
    /// every box-owning identity unbootable over an outage in the *other*
    /// world. The charter and the rules are in Memory and were right there.
    ///
    /// Now the mailbox half degrades on its own, the same way the snapshot's
    /// does: the boot lands, the identity is whole, and the one thing jojobot
    /// cannot answer says so instead of guessing.
    #[tokio::test]
    async fn a_boot_survives_a_world_that_is_down_exactly_as_an_anonymous_one_does() {
        // Stood up while both worlds are up — a claim that cannot be screened
        // is refused, so this bot could not have been created below.
        let memory = Arc::new(InMemoryMemory::new());
        let healthy = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::new()),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&healthy, "gamma", Some("gamma-inbox")).await;
        healthy
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: None,
            }))
            .await
            .expect("set_charter ok");

        let jojobot = handler_with_mailboxes_down(memory);
        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "a boot must still land: {body}");

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:gamma");
        assert_eq!(
            me["charter"], "Holds the plan.",
            "the half that is up arrives whole"
        );

        let owned = &me["owned_mailbox"];
        assert_eq!(
            owned["name"], "gamma-inbox",
            "the claim is Memory's and is still known"
        );
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
        assert!(
            !listed(&before),
            "the snapshot must agree it is absent: {before}"
        );

        make_box(&jojobot, "sigma-inbox").await;

        let after = boot(&jojobot, "sigma").await;
        assert_eq!(after["identity"]["owned_mailbox"]["exists"], true);
        assert!(listed(&after), "…and agree it is there: {after}");
    }

    /// **One orientation, one door.** Naming a bot is `start_here` plus an
    /// identity — not a second world-model to drift out of step with the first.
    #[tokio::test]
    async fn a_named_boot_and_an_anonymous_one_hand_over_the_same_world() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let identified = boot(&jojobot, "gamma").await;
        assert_eq!(
            anonymous["orientation"], identified["orientation"],
            "the world-model is one text, or the two doors teach different jojobots"
        );
        assert_eq!(
            anonymous["snapshot"]["entities"], identified["snapshot"]["entities"],
            "what exists is one answer, whoever asks"
        );
        // **The mailbox half is deliberately NOT equal once a bot drains a
        // box** — that is the whole point of scoping counts to the caller — so
        // the shared invariant is the set of boxes, not their contents. The
        // fixture used to give gamma no mailbox, which made a stale assertion
        // of full equality pass for a reason that had nothing to do with the
        // invariant it claimed.
        let names = |body: &serde_json::Value| -> Vec<String> {
            body["snapshot"]["mailboxes"]["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .map(|b| b["name"].as_str().expect("a name").to_string())
                .collect()
        };
        assert_eq!(
            names(&anonymous),
            names(&identified),
            "both doors see the same board; they differ only in whose queue is theirs to read"
        );
        assert!(
            anonymous["identity"].is_null(),
            "an anonymous session claims no identity"
        );
    }

    /// …and the difference the previous test carves out, asserted directly: the
    /// booted door counts the box its identity drains, the anonymous one does
    /// not.
    #[tokio::test]
    async fn the_two_doors_differ_only_in_whose_queue_is_theirs_to_read() {
        let jojobot = handler();
        make_box(&jojobot, "dev").await;
        make_bot(&jojobot, "gamma", Some("dev")).await;
        send(&jojobot, "dev", "delta", "your hand-off").await;

        let counts_for = |body: &serde_json::Value| -> serde_json::Value {
            body["snapshot"]["mailboxes"]["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .find(|b| b["name"] == "dev")
                .expect("the box")
                .clone()
        };

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(counts_for(&anonymous)["counts"].is_null(), "{anonymous}");

        let identified = boot(&jojobot, "gamma").await;
        assert_eq!(counts_for(&identified)["counts"]["new"], 1, "{identified}");
        assert_eq!(counts_for(&identified)["yours"], true);
    }

    /// `set_charter` writes the orienting prose and reads it back — and it is
    /// the same text a boot hands over, so what an operator writes is what a
    /// session is told.
    #[tokio::test]
    async fn set_charter_writes_the_prose_that_a_boot_reads_back() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma", None).await;

        let written = json_of(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "gamma".into(),
                    prose: "  Holds the plan. Does not implement.  ".into(),
                    sid: None,
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
                    sid: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "bot:nobody");
        assert!(
            missed["how_to_proceed"]
                .as_str()
                .is_some_and(|a| a.contains("add_entity")),
            "the way out names the verb that opens it: {missed}"
        );
    }
}
