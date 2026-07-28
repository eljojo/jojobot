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

pub mod mailboxes;
pub mod memory;
pub mod sid;

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
    FactStatus, Guarded, Memory, MemoryError, NewEntity, NewFact, Provenance,
    guard::{self, EntityMatch},
    search::{DEFAULT_LIMIT, EdgeFilter, EntityRef, Hit, MailCoverage, Search, SearchQuery},
    validate_edge,
};
use jojobot_domain::session::{
    BEAT_CLASSES, BEAT_EXAMPLES, Beat, Board, EntryId, JournalEntry, NewEntry, NewSession, Session,
    SessionError, SessionId, SessionState, Sessions, beat_text, beats_of, sweep_and_find,
};
use jojobot_domain::text::{self, FRESH_FOCUS};
// **The args types keep their crate-root path.** They were `pub` here before
// the split and something outside may name them; where a type LIVES is this
// slice's business, where a caller finds it is not.
pub use mailboxes::{
    ListMailboxesArgs, ListSentArgs, MarkProcessedArgs, PostMessageArgs, ReadMailboxArgs,
    ReadMessageArgs,
};
pub use memory::{
    AddEntityArgs, CaptureArgs, EdgeFilterArgs, ListEntitiesArgs, RecallArgs, SearchArgs,
    SetCharterArgs, UpdateEntityArgs, UpdateFactArgs,
};

use mailboxes::wire::*;
use memory::declined::*;
use memory::parse::*;
use memory::wire::*;
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
- *No mailbox fits what you want to leave* → **there is no verb that opens one.** A box is not a thing you make: it belongs to a bot, is named for it, and comes into being with it — so the only way a new box appears is that a new identity does, and standing up somebody's identity to file a note is not a move you make on your own. Use an existing, agreed box, or say plainly there is nowhere fitting and let the operator decide.
- *"Which people are in Shelbyville?"* → `search` with kind `person` and edge `{shape: location, object: place:shelbyville}` — an edge walk, not a text match.
- *"That was wrong"* → `recall` the subject, then `update_fact` rewrites the claim in place to state what is true NOW — including negative truth ("NOT allergic — confirmed by the operator"). The record is current truth, never a correction trail. *"That changed"* is a different move: the old claim was true in its day — mark it `superseded` and `capture` the new one.
- *Leave word for another session* → `list_mailboxes` to see what exists and what is waiting, `post_message` into an agreed box with a body written for a reader with none of your context. jojobot records who sent it from the `sid` you pass, so there is nothing to declare and nothing to get wrong.
- *Handle mail* → `read_mailbox`, which opens YOUR box — the `sid` you pass says which one, so there is no name to give and no way to reach into somebody else's. Reading takes delivery of every message in it; act, then `mark_processed`, ONLY after acting, with the outcome in notes. A failure is data to record, not a state to park in.
- *One message, not a whole box* → `search` for it, then `read_message` on the id the hit carries. Draining your whole box makes every message in it owed work; `read_message` takes on the one you actually meant.

When the right write is not obvious, ask the operator — an unasked write outlives the conversation that guessed it.

## The answers that are not errors

A **blocked** result is a SUCCESS whose body says `status: "blocked"`, `wrote: false`: nothing was written, and `how_to_proceed` says what to do next. Never retry one unchanged. Four gates produce it, with different ways out: **resemblance** (creating or renaming something that looks like what exists — pick the candidate you meant, or `create_new: true` only when you can say how the two differ; an exact handle or box name is never overridable), **absence** (you named something that is not there — the subject of a capture, an edge's object, the box of a post, a handle to read, an address to edit, a message id to retire; empty `candidates` means nothing even resembles it, not that your call was malformed; for an entity, creating it and retrying is usually right — for a mailbox it usually is not), **ownership** (a mailbox has exactly one owner, and a second claim on one is refused naming the holder; `create_new` does not clear this — it answers a question about names), and **unreadable** (`mark_processed` reached an item jojobot cannot read — no retry helps, a person must repair it; treat what it carried as unhandled and say so).

A plain **error** is a malformed call — a token that is no kind, a string that is no address — or the store itself failing. **Absence is never an error here**: naming something that does not exist is an answer with candidates, not a broken server, so read `status` rather than branching on whether the call errored. And know what the guards do NOT cover: they catch resemblance, absence and ownership, never judgement — a wholly novel name sails through, and nothing will stop you standing up an entity nobody needed. That call is yours, and the store keeps whatever you decide.

## Bots

An **identity** is an entity of kind `bot`: a handle like `bot:gamma`, a **charter** (its prose — what this identity is, its hard lines, where its work lives), **rules** as ordinary facts about it (so each one carries its own provenance: an inferred rule is a hypothesis, not a policy), and **one owned mailbox**, named for it and opened with it — not optional, and not separately created: an identity that cannot be written to is not one. If you were told which identity you are, pass that name to `start_here` — the one door — and it hands over everything here plus that identity. Nothing about a bot is built into jojobot — a bot is data somebody wrote, like every other entity.

## Sessions

A bot is a **role**; a **session is one mortal run of it** — the unit of work, not the unit of connection. It outlives a disconnect and a device hop, because what makes two connections the same session is the `sid` you carry — hold it and keep passing it, on writes and reads alike.

**Booting an identity starts or resumes its session; there is no separate verb.** `start_here` with your bot name sweeps that bot's stale sessions to `abandoned` (a day without a beat). If a resumable session remains you get the choice — what each one was working on, and whether it is still running or stopped without being wrapped up — and NO sid until you answer: choose resume and you inherit its chronology, choose new and a fresh sid is minted beside it, closing nothing. With nothing to resume the sid comes back straight away. Either way the card itself is written **lazily**, on your first real write, so a boot that does nothing leaves nothing behind.

A session has two halves that answer different questions. Its **focus** is what it is working on NOW, one line, rewritten in place. Its **chronology** is what happened: append-only, oldest first, with only the newest entry amendable.

- *Record a beat* → `journal` — **a literal journal, not a log.** What you set out to do, what you found, what you decided, what went wrong. NOT every tool call and not every file: a reader months from now wants the story, and a firehose buries it. Pass `focus` when what you are working on changes.
- *Fix the beat you just wrote* → `amend_journal`. Only the most recent one; everything older is what it was.
- *End* → `wrap_session` with the story, written for somebody with none of your context. It becomes your final entry and the session goes `wrapped` — terminal both ways. It is published NOWHERE: your chronology is the record, and it is the only one.

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

// --- mailboxes ---------------------------------------------------------------

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
    /// it was for, what happened, what is left. It becomes the final entry in
    /// this session's own chronology, and goes nowhere else.
    pub story: String,
    /// Your session id — the session to wrap.
    pub sid: String,
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

/// **The verbs still living in this file.** Every context that has moved out
/// carries its own router; this one shrinks as they go, and the sum below is
/// what a client actually sees.
#[tool_router(router = core_router, vis = "pub(crate)")]
impl Jojobot {
    /// The whole surface: this file's verbs, plus every context's.
    ///
    /// **Summed, never scanned.** A verb reaches a client by its context naming
    /// it and its context being named here — the same deliberate friction
    /// `the_tool_surface_is_exactly_this_list` puts in front of a new tool.
    pub fn tool_router() -> ToolRouter<Self> {
        Self::core_router() + mailboxes::router() + memory::router()
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
                       every mailbox by name — with counts for the ones you drain), so you start \
                       oriented instead of guessing. CALLED THIS \
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
            // **The prose is unchanged; the CHANNEL is the fix.** A thrown error
            // is not a value a caller can branch on, so advice naming two ways
            // forward arrived somewhere nothing reads structurally. Blocked is
            // the shape every other misuse here wears.
            return Ok(misused(
                "resume answers the choice a boot hands back, so it needs the bot you are booting \
                 as — pass `bot` too, or drop `resume` for an anonymous orientation"
                    .to_string(),
            ));
        }
        self.orient(bot.as_ref(), args.brief.unwrap_or(false), resume)
            .await
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
        let listed = self.mailboxes.list_mailboxes().await;
        let mailboxes = match listed {
            Ok(boxes) => {
                let mine = self.ownership_of(&boxes, bot);
                serde_json::json!({
                "available": true,
                "counts_shown_for": mine.shown_for(&boxes),
                "note": mine.note(),
                "boxes": boxes
                    .iter()
                    .map(|b| {
                        if mine.drains(b.name.as_str()) {
                            let mut body = mailbox_json(b);
                            if let Some(obj) = body.as_object_mut() {
                                obj.insert("yours".into(), true.into());
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
                })
            }
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
            "owned_mailbox": self.owned_mailbox(&entity.id).await?,
        })))
    }

    /// The live state of the box a bot owns — **and the one place jojobot heals
    /// one that is missing.**
    ///
    /// A box opens with its bot, so a bot whose box is absent is damage: an
    /// `add_entity` that wrote the identity and then failed to open the box, or
    /// a record predating the rule. The operator's ruling is that jojobot fixes
    /// it rather than filing it — *"the system should auto heal next time when
    /// it notices it's not there but it should. and notify the agent that the
    /// message/box wasn't created."*
    ///
    /// **This is not rule 18 being bent.** The intentional act already happened:
    /// somebody stood up the bot, and a bot's box is part of what a bot IS
    /// rather than a second thing they forgot to ask for. Healing completes an
    /// interrupted act; it mints nothing nobody asked for. The repair is only
    /// legitimate because it needs no judgement — the owner is in hand and the
    /// name is derived from it, so there is exactly one correct box.
    ///
    /// **Boot is the only place that heals**, and that is a deliberate limit.
    /// `list_mailboxes` and the count scoping are pure reads of the board, and
    /// healing there would make every read a potential write. `post_message`
    /// names somebody *else's* box: writing another identity's infrastructure is
    /// not this caller's act, and the owner heals it the moment it boots — which
    /// is the next time anyone would drain it anyway. A message is not more
    /// delivered for a box existing that nobody has booted to read.
    async fn owned_mailbox(&self, bot: &EntityId) -> Result<serde_json::Value, McpError> {
        // The mailbox half degrades on its own, exactly as the snapshot's does.
        // Hard-erroring here made every box-owning identity unbootable over an
        // outage in the *other* world — while its charter and its rules, the
        // things a session most needs, were sitting right there in Memory.
        let boxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes,
            Err(_) => {
                // Not `null`, which is the answer for a bot that owns no box:
                // jojobot does not know whether it owns one, and saying it does
                // not would be a guess a session would act on.
                return Ok(serde_json::json!({
                    "available": false,
                    "note": "the mailbox world is not reachable right now, so jojobot cannot say \
                             whether you own a box or what is waiting in it — its tools will \
                             say why",
                }));
            }
        };

        // **A lookup by owner, not a claim read.** A box is created for
        // somebody, so ownership is stated once on the box itself — there is no
        // second field on the bot's record to keep in step with it.
        //
        // Which also deletes a state: there used to be a "declared but never
        // opened" answer, for a claim naming a box nobody had created. A claim
        // can no longer outlive the thing it claims, so a box this bot owns is
        // a box that exists, and the branch reporting otherwise is gone rather
        // than left unreachable.
        // Which also takes `exists` with it: it told a claim's box apart from a
        // box, and a box is the only thing left to report, so it now says `true`
        // wherever it appears at all. `available` is the one question a reader
        // still has to branch on.
        let Some(mailbox) = boxes.into_iter().find(|b| &b.owner == bot) else {
            return Ok(self.heal_missing_box(bot).await);
        };
        let mut body = mailbox_json(&mailbox);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("available".into(), true.into());
        }
        Ok(body)
    }

    /// Open the box this bot should have had, and **say that it was missing.**
    ///
    /// The notification is half the ruling, not a courtesy. A silent repair is
    /// the thing this codebase forbids everywhere else — eliding is never
    /// silent — and a session that cannot tell "your box is here" from "your box
    /// was gone and is here now" cannot report the damage to anybody who could
    /// ask why it happened.
    ///
    /// **One attempt, never a loop.** If the repair does not land, that is said
    /// too, and the boot still returns whole: the charter and the rules are in
    /// the other world entirely and are what a session most needs.
    async fn heal_missing_box(&self, bot: &EntityId) -> serde_json::Value {
        let name = MailboxName(bot.slug().to_string());
        match self.mailboxes.create_mailbox(&name, bot, true).await {
            Ok(mailbox::Guarded::Written(opened)) => {
                let mut body = mailbox_json(&opened);
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("available".into(), true.into());
                    obj.insert("healed".into(), true.into());
                    obj.insert(
                        "note".into(),
                        format!(
                            "YOUR BOX '{name}' was missing and jojobot has just opened it. A box \
                             opens with its bot, so its absence means that creation was \
                             interrupted — the identity existed with no way to receive mail. It \
                             is repaired and this boot is otherwise normal, but anything posted \
                             to you before now was refused as an unknown box and was never \
                             stored: tell the operator, since only they can say what was lost."
                        )
                        .into(),
                    );
                }
                body
            }
            // **The world answered and the box still is not there**, which is a
            // different thing from the world being unreachable — so `available`
            // stays true and `name` is null rather than claiming a box.
            other => serde_json::json!({
                "available": true,
                "name": serde_json::Value::Null,
                "healed": false,
                "note": format!(
                    "YOUR BOX '{name}' is missing and jojobot could not open it. You have an \
                     identity with no way to receive mail, and this is damage rather than a \
                     setup step: a box opens with its bot. Nothing you post is affected — \
                     post_message needs no box of your own. Tell the operator.{}",
                    match &other {
                        Err(err) => format!(" The mailbox world said: {err}"),
                        _ => String::new(),
                    }
                ),
            }),
        }
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
        // The clock is read HERE and handed down: the sweep is domain policy
        // and the domain is clock-free, so the instant it decides against is
        // stamped at the edge exactly as a capture's date is.
        let swept_at = jiff::Timestamp::now();
        let Board {
            live,
            offerable,
            swept,
            unswept,
        } = match sweep_and_find(self.sessions.as_ref(), bot, swept_at).await {
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
        // **The domain named them; the log is this layer's.** A stale session
        // the store refused to close is left active for the next boot, and the
        // boot itself carries on — but it must not go quiet, because nothing
        // else on the surface will ever mention it.
        for (session, e) in &unswept {
            tracing::warn!(
                error = %e, %session,
                "a stale session could not be swept — left active for the next boot"
            );
        }

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
            // The one end that is the last word. Wrapping publishes nothing
            // now, so the reason is no longer a published account going stale —
            // it is that a run which told its story is over, and its chronology
            // stands as the record of what happened.
            Err(SessionError::Closed { state, .. }) => Err(handle_declined(
                answer,
                format!(
                    "No session was started. '{answer}' addresses a session that is {state} — its \
                     story has been told, so this end is the last word. Its chronology stands as \
                     the record of what happened. Call start_here with your bot name and no \
                     resume to begin the next run."
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
                       it continues this same record, while a wrapped one is the last word — its \
                       story is told and nothing appends to it, so carrying on means a fresh \
                       session."
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
        description = "End your session and tell its story. Two things happen together: the \
                       story is recorded in your chronology as its final entry, and the session \
                       moves to `wrapped` — terminal both ways, so \
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
        // **The current unpublished beat is flushed INTO the story, as ONE
        // entry.** A session's focus is truth about the run, rewritten in place,
        // and becomes chronology only once something has happened (rule 81).
        // Wrapping is the last thing that happens, so the focus that never
        // became a beat becomes one here.
        //
        // One entry, not two beside each other: the focus and the story are the
        // same moment, and a chronology ending on two records of it leaves a
        // reader unable to tell which is the account. Chronological inside —
        // the focus is what the run was doing, the story is what became of it.
        //
        // Read before the guard below, so the retry looks for the composed text
        // rather than the story alone. A retry that searched for half of what it
        // wrote would tell it twice.
        let focus = match self.sessions.read_session(&session).await {
            Ok(read) => jojobot_domain::session::normalize_entry(&read.focus),
            // Unreadable is not "no focus", but the append below fails in that
            // verb's own words; guessing an empty one here only risks losing a
            // line, never duplicating the story.
            Err(_) => String::new(),
        };
        // **A focus DERIVED from this same story is not an unpublished beat.** A
        // wrap that is the session's first write creates the card with a focus
        // made out of the story itself (`display_line`), so folding it back in
        // would tell the story twice inside one entry — and it would not compare
        // equal, because the derived form is flattened to one display line.
        // Compared through the same derivation, which is the only form the two
        // can meet in.
        let story = if focus.is_empty() || display_line(&story) == focus {
            story
        } else {
            format!("{focus}\n\n{story}")
        };
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
        }))
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

/// **A call whose arguments are each fine and wrong together.** Not a malformed
/// call — every token parsed — so it is not a protocol error: it is a caller
/// mistake, and those are answers here.
///
/// **No `attempted` and no `candidates`, deliberately.** There is nothing that
/// was nearly right to name and nothing that nearly matched; what a caller needs
/// is the other call to make. [`session_unbound`] is the precedent — the shape
/// has always carried a candidate-free refusal, so this fits it rather than
/// stretching it into something that reads like a near miss.
fn misused(how_to_proceed: String) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": how_to_proceed,
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
                 so this end is the last word. Its chronology stands as the record of what \
                 happened. If there is more to say, it belongs to a new session: boot again (or \
                 rotate) and start_here mints one."
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

// --- mailboxes on the wire ---------------------------------------------------

/// Render a JSON body as a successful tool result.
fn json_result(body: &serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]))
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
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;

    use super::*;
    use async_trait::async_trait;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::memory::EntityKind;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::Sid;
    use jojobot_domain::session::testing::InMemorySessions;

    // --- search: the front door -----------------------------------------------

    // --- the entity verbs -----------------------------------------------------

    // --- the write guard, through the MCP boundary ----------------------------

    // --- structured edges at capture ------------------------------------------

    // --- addresses and update -------------------------------------------------

    // --- mailboxes ------------------------------------------------------------

    /// A boot sees its own box's counts in the snapshot, and names only for the
    /// rest — the same rule, in the other place a session meets this listing.
    #[tokio::test]
    async fn a_boot_snapshot_counts_only_the_bot_s_own_box() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        send(&jojobot, "delta", "sigma", "not your business").await;

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

        assert_eq!(
            find("gamma")["counts"]["new"],
            1,
            "my box, counted: {booted}"
        );
        assert_eq!(find("gamma")["yours"], true);
        assert!(
            find("delta")["counts"].is_null(),
            "somebody else's, name only: {booted}"
        );
        assert_eq!(find("delta")["yours"], false);

        // The bot's own box still comes back in full under `identity`, which is
        // the whole point of booting as somebody.
        assert_eq!(booted["identity"]["owned_mailbox"]["counts"]["new"], 1);
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
        // Two entities, and the second one is the point: the box below belongs
        // to a bot, because there is no other kind of box.
        assert_eq!(body["snapshot"]["entities"]["count"], 2);
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["person"], 1);
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["bot"], 1);
        let boxes = body["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("mailboxes listed");
        // **Anonymous orientation drains nothing, so it sees no queue.** There
        // is no longer a second case: every box has a drainer, so a box is
        // either yours or somebody's, and this caller is nobody.
        assert_eq!(boxes[0]["name"], "inbox");
        assert_eq!(
            boxes[0]["yours"], false,
            "an anonymous caller drains nothing"
        );
        assert!(
            boxes[0]["counts"].is_null(),
            "…and somebody else's queue is not its to weigh: {:?}",
            boxes[0]
        );
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
        async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
            self.0.scan().await
        }
    }

    /// **The two worlds came apart, and this is the test that says so.**
    ///
    /// It used to assert the opposite, and correctly: ownership was a `mailbox:`
    /// claim on the bot's entity record, so an unreadable entity index meant
    /// jojobot could not say what anybody drained, and the listing said
    /// OWNERSHIP IS UNKNOWN rather than telling every bot its own queue was
    /// somebody else's.
    ///
    /// Ownership is stated on the box now. The entity index is no longer on
    /// that path at all, so an outage in it takes the charter, the rules and
    /// the roster with it — and leaves the mail scoping standing. Inverted
    /// deliberately: the old assertion passing would mean the claim field was
    /// still being read.
    #[tokio::test]
    async fn an_unreadable_entity_index_no_longer_hides_who_drains_what() {
        let memory = Arc::new(InMemoryMemory::new());
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let seeded = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_box(&seeded, "dev").await;
        send(&seeded, "dev", "delta", "your hand-off").await;

        let blind = Jojobot::new(
            Arc::new(UnindexedMemory(memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        let listed = drains(&blind, "dev").await;

        assert_eq!(listed["mailboxes"][0]["yours"], true, "{listed}");
        assert_eq!(
            listed["mailboxes"][0]["counts"]["new"], 1,
            "the mail world knows whose box this is without asking Memory: {listed}"
        );
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!(["dev"]),
            "{listed}"
        );
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

        crate::surface::engine_generic("ORIENTATION", ORIENTATION);
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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;

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

    /// **A misuse is an ANSWER, not a thrown error.** `resume` responds to an
    /// offer only a named boot makes, so carrying one without a `bot` is a
    /// caller mistake — and every other caller mistake on this surface comes
    /// back as a blocked result a client can branch on. A thrown error is not a
    /// value: the model on the other end gets a failure where it should get a
    /// next move, and the prose telling it what to do instead is stranded in a
    /// channel nothing reads structurally.
    ///
    /// **The prose was already right and is kept verbatim** — it names both ways
    /// forward. Only the channel changes.
    #[tokio::test]
    async fn resume_without_a_bot_is_a_blocked_answer_rather_than_a_thrown_error() {
        let jojobot = handler();
        let out = jojobot
            .start_here(Parameters(OrientArgs {
                bot: None,
                brief: None,
                resume: Some("new".into()),
            }))
            .await
            .expect("a misuse is an answer, not a protocol failure");
        let body = blocked(&out);
        assert_eq!(body["wrote"], false, "nothing was started: {body}");

        // Both ways forward survive the move, because that is the whole value of
        // the answer over the error.
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("bot") && how.contains("resume"),
            "the advice names both moves: {how}"
        );

        // …and no session was minted behind the refusal.
        assert!(
            body["sid"].is_null(),
            "a refused boot hands back no handle: {body}"
        );
    }

    /// **jojobot heals the box it notices is missing, and says so out loud.**
    ///
    /// The operator's ruling: *"the system should auto heal next time when it
    /// notices it's not there but it should. and notify the agent that the
    /// message/box wasn't created."* Two halves, and the second is the one that
    /// is easy to drop — a silent repair is the class this codebase forbids
    /// everywhere else, because a caller who has to infer "this was fixed" from
    /// the absence of a complaint will eventually infer wrong.
    #[tokio::test]
    async fn booting_a_bot_whose_box_is_missing_opens_it_and_says_so() {
        let jojobot = handler();
        broken_bot(&jojobot, "gamma").await;

        let owned = boot(&jojobot, "gamma").await["identity"]["owned_mailbox"].clone();
        assert_eq!(owned["name"], "gamma", "the box is there now: {owned}");
        assert_eq!(owned["available"], true);
        assert_eq!(
            owned["healed"], true,
            "…and the repair is on the record, not silent: {owned}"
        );
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("was missing"),
            "the note says what was wrong, not just that something happened: {note}"
        );

        // It is a real box on the board, not a rendering.
        let boxes = jojobot.mailboxes.list_mailboxes().await.expect("list ok");
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        assert_eq!(boxes[0].owner, EntityId::new(EntityKind::Bot, "gamma"));

        // …and the second boot is quiet, because there is nothing left to fix.
        let again = boot(&jojobot, "gamma").await["identity"]["owned_mailbox"].clone();
        assert!(
            again["healed"].is_null(),
            "a heal is news exactly once: {again}"
        );
    }

    /// **The heal never conjures a box for a bot that does not exist.** An
    /// unknown name is answered with the roster, and nothing is written — the
    /// heal must not turn a typo'd boot into a new identity's infrastructure.
    #[tokio::test]
    async fn healing_opens_nothing_for_a_bot_that_does_not_exist() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamm").await;
        assert_eq!(
            body["status"], "blocked",
            "an unknown bot is refused: {body}"
        );
        let names: Vec<String> = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list ok")
            .iter()
            .map(|b| b.name.as_str().to_string())
            .collect();
        assert_eq!(
            names,
            ["gamma"],
            "no box was conjured for a name nobody owns"
        );
    }

    /// **The heal never conjures a box for something that is not a bot.** A
    /// person is not an addressee, and the door that heals only ever holds a
    /// bot — this pins that rather than trusting it.
    #[tokio::test]
    async fn healing_opens_nothing_for_an_entity_that_is_not_a_bot() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "milhouse", "Milhouse")))
            .await
            .expect("add_entity call ok");

        // The one door refuses a non-bot handle outright, so the heal is never
        // reached with one…
        let err = jojobot
            .start_here(Parameters(OrientArgs {
                bot: Some("person:milhouse".into()),
                brief: None,
                resume: None,
            }))
            .await
            .expect_err("this door boots bots");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        // …and nothing was opened on the way to finding that out.
        assert!(
            jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .is_empty(),
            "a person is not an addressee and never gets a box"
        );
    }

    /// **A heal that cannot land is reported honestly, and is not retried into a
    /// loop.** The boot still lands: the charter and the rules are the things a
    /// session most needs and they are in the other world entirely.
    #[tokio::test]
    async fn a_heal_that_fails_is_reported_rather_than_spun_on() {
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(UnopenableMailboxes(InMemoryMailboxes::knowing_any_owner())),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        broken_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "the boot still lands: {body}");
        assert_eq!(body["identity"]["bot"]["id"], "bot:gamma");

        let owned = &body["identity"]["owned_mailbox"];
        assert_eq!(
            owned["healed"], false,
            "the repair was attempted and did not land, and says so: {owned}"
        );
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("could not"),
            "an honest failure, not a silent absence: {note}"
        );
    }

    /// **Wrapping flushes the current unpublished beat INTO the story, as one
    /// entry.** His ruling, and the "one entry" half is the part a reasonable
    /// implementation gets wrong: *"it should be both but it should be one
    /// entry."*
    ///
    /// A session's `focus` is current truth, rewritten in place, and it becomes
    /// chronology only once something has happened (rule 81). Wrapping IS
    /// something happening — it is the last thing that happens — so the focus
    /// that never became a beat becomes one. Writing it as a SECOND entry beside
    /// the story would leave the chronology ending on two records of one moment,
    /// and a reader unable to tell which was the account.
    ///
    /// Ordering is chronological: the focus was what the run was doing, the
    /// story is the account of it, so the focus comes first inside the entry.
    #[tokio::test]
    async fn wrapping_flushes_the_unpublished_focus_into_the_story_as_one_entry() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let started = json_of(
            &jojobot
                .journal(Parameters(JournalArgs {
                    entry: "read the hand-off".into(),
                    focus: Some("cutting the codec seam".into()),
                    sid: sid.clone(),
                }))
                .await
                .expect("journal ok"),
        );
        let session = SessionId(
            started["session"]
                .as_str()
                .expect("a session id")
                .to_string(),
        );

        jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "the seam is cut and the suite is green".into(),
                sid,
            }))
            .await
            .expect("wrap ok");

        let read = store.read_session(&session).await.expect("read ok");
        let told: Vec<&str> = read
            .entries
            .iter()
            .filter(|e| !e.is_auto())
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            told.len(),
            2,
            "the beat, then ONE closing entry — never two for one moment: {told:?}"
        );
        let last = told[1];
        assert!(
            last.contains("cutting the codec seam"),
            "the unpublished focus was flushed: {last:?}"
        );
        assert!(
            last.contains("the seam is cut and the suite is green"),
            "…into the story, not beside it: {last:?}"
        );
        assert!(
            last.find("cutting the codec seam") < last.find("the seam is cut"),
            "…and in the order they happened: {last:?}"
        );
    }

    /// A run that set no focus wraps on the story alone — nothing empty is
    /// folded in, and no blank line is left where a flush would have been.
    #[tokio::test]
    async fn wrapping_with_no_unpublished_focus_tells_the_story_alone() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let wrapped = json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "booted, found nothing to do".into(),
                    sid,
                }))
                .await
                .expect("wrap ok"),
        );
        assert_eq!(wrapped["entry"]["text"], "booted, found nothing to do");
    }

    // ── a bot and its box are one act ───────────────────────────────────────

    // ── booting as an identity ──────────────────────────────────────────────

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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&client.call(), "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&client.call(), "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
            on_told.contains("story has been told"),
            "a told story names the reason this end is the last word: {on_told}"
        );
        assert!(
            !on_told.contains("Journal"),
            "…and never a shared Journal, which no longer exists: {on_told}"
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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

    /// **A wrapped run is over, both in the offer and by handle.** It told its
    /// story and ended; reopening it would reopen something that said it was
    /// finished.
    #[tokio::test]
    async fn a_wrapped_run_is_never_offered_and_never_reopens() {
        let store = Arc::new(InMemorySessions::new());
        let registry = Arc::new(sid::SessionRegistry::new());
        let jojobot = connection_sharing(
            Arc::new(InMemoryMemory::new()),
            store.clone(),
            registry.clone(),
        );
        make_bot(&jojobot, "gamma").await;

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
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
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
                mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
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
        make_bot(&client.call(), "gamma").await;

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

    /// **A wrapped `sid` stays closed, and the bot behind it boots its next
    /// run.** Those are different questions and the answers have to differ: a
    /// `sid` names one run, and closed is terminal both ways for that record —
    /// while the identity outlives any run of it, so booting again is ordinary
    /// rather than a way back in.
    #[tokio::test]
    async fn a_wrapped_sid_stays_closed_while_its_bot_boots_the_next_run() {
        let client = NoAffinity::new();
        make_bot(&client.call(), "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

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
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;

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
        make_bot(&client.call(), "gamma").await;
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
        make_bot(&client.call(), "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
            make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&first, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
        // **Wrapping publishes NOWHERE.** It told the story into a shared
        // Journal document, and his ruling deletes that: the journal goes dark
        // until events land, and a wrap is the session's own record closing.
        assert!(
            wrapped.get("journal").is_none(),
            "a wrap publishes nowhere, so it reports no publication: {wrapped}"
        );
        assert!(
            !jojobot
                .memory
                .scan()
                .await
                .expect("scan ok")
                .iter()
                .any(|doc| doc.title.trim() == "Journal"),
            "…and no shared Journal document was brought into being"
        );

        let read = store
            .read_session(&SessionId(
                first["session"].as_str().expect("a session id").to_string(),
            ))
            .await
            .expect("read ok");
        let texts: Vec<&str> = read.entries.iter().map(|e| e.text.as_str()).collect();
        // The closing entry carries the unpublished focus folded into the
        // story — one entry for one moment, which is his ruling.
        assert_eq!(
            texts,
            vec![
                "read the hand-off and scoped the slice properly",
                "building the session context\n\nbuilt the session context; the sweep is lazy until M8",
            ],
            "two entries: the amended one, and the story with the flushed focus"
        );
    }

    /// **Wrapped is terminal both ways, through the surface.** Every session
    /// verb on a closed id comes back blocked, in the guards' one shape.
    #[tokio::test]
    async fn a_wrapped_session_refuses_every_further_write() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
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
            // **Why this end is the last word, not merely that it is.** The
            // reason used to be a published account that reopening would
            // falsify; wrapping publishes nothing now, and the reason survives
            // it: a run that told its story has ended, which is what makes this
            // refusal different from the one an abandoned run gets.
            assert!(
                how.contains("story has been told"),
                "{verb} has to say why: {how}"
            );
            assert!(
                !how.contains("Journal"),
                "{verb} must not cite a Journal that is gone: {how}"
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&first, "gamma").await;
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
        make_bot(&first, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;

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
        make_bot(&jojobot, "gamma").await;
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
                        // **A bot, not a person**, so the box assertion below
                        // has something to bite on: `add_entity` is the verb
                        // that opens boxes now, and a person would leave the
                        // mail world untouched however broken the refusal was.
                        .add_entity(Parameters(AddEntityArgs {
                            kind: "bot".into(),
                            handle: "delta".into(),
                            name: "Delta".into(),
                            aliases: None,
                            source: "test-fixture".into(),
                            crm: None,
                            boot: None,
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
                .any(|e| e.id.as_str() == "bot:delta"),
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
        // **`add_entity` is the box-opening verb now**, so its refusal is the one
        // that must leave the mail world untouched: a bot refused for a dead
        // handle must not leave a box behind either.
        assert!(
            !jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .iter()
                .any(|b| b.name.as_str() == "delta"),
            "add_entity opened a box behind a refusal"
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
        make_bot(&jojobot, "gamma").await;

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
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "person:alpha".into(),
                name: Some("Alpha, renamed by nobody".into()),
                aliases: None,
                source: None,
                crm: None,
                create_new: None,
                sid: None,
            }))
            .await
            .expect("update ok");

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
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            store.clone(),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&jojobot, "gamma").await;
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
        let (jojobot, store, _memory, sid) = refusing_close().await;
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
        // **Counted as occurrences, not as whole entries.** A wrap folds the
        // session's unpublished focus into the story, so the closing entry is
        // the story plus that line — the guard is about telling the story once,
        // not about the entry equalling it.
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.text.contains(story))
                .count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
        );
    }

    /// **A retry finishes what the first attempt started, wherever the story now
    /// sits.** The chronology half of the guard looked only at the newest entry,
    /// so anything written between the failed close and the retry — a journal
    /// entry saying the wrap failed, which is the natural thing to write — pushed
    /// the story off the tail and the retry told it a second time.
    #[tokio::test]
    async fn a_wrap_retried_after_an_intervening_entry_tells_the_story_once() {
        let (jojobot, store, _memory, sid) = refusing_close().await;
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
        // **Counted as occurrences, not as whole entries.** A wrap folds the
        // session's unpublished focus into the story, so the closing entry is
        // the story plus that line — the guard is about telling the story once,
        // not about the entry equalling it.
        assert_eq!(
            live[0]
                .entries
                .iter()
                .filter(|e| e.text.contains(story))
                .count(),
            1,
            "the story is told once in the chronology: {:?}",
            live[0].entries
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
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
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
        make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "gamma").await;
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
            make_bot(&client.call(), "gamma").await;
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
            make_bot(&jojobot, "gamma").await;
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
            make_bot(&jojobot, "gamma").await;
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
        make_bot(&jojobot, "otto").await;
        send(&jojobot, "otto", "epsilon", "the shipment landed").await;

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
        assert_eq!(owned["name"], "otto", "a bot's box is named for it");
        assert_eq!(
            owned["counts"]["new"], 1,
            "the state of its own box: {owned}"
        );
        assert_eq!(owned["available"], true, "the box is there and says so");
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
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
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
        make_bot(&jojobot, "gamma").await;

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
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&healthy, "gamma").await;
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

        // **WHICH WORLD KNOWS THIS HAS FLIPPED.** Ownership used to be a claim
        // on the bot's own record, so a mailbox outage left the *name* known
        // and only its contents unknown. Ownership is stated on the box now, so
        // an unreachable mailbox world means jojobot cannot say which box is
        // yours, or whether you have one — and it says exactly that instead of
        // naming a box it cannot see.
        let owned = &me["owned_mailbox"];
        assert_eq!(owned["available"], false, "got {owned}");
        assert!(
            owned["name"].is_null(),
            "a box it cannot read is not a box it can name: {owned}"
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
    /// believe. Both halves are reads of the same world now; this holds them to
    /// agreeing.
    ///
    /// **The "before" half of this test is gone with the state it described.**
    /// It used to boot a bot whose box nobody had opened and assert both halves
    /// called it absent. There is no such bot: a box opens with its owner, so
    /// the disagreement now reachable is the opposite one — an identity naming
    /// a box the snapshot beside it does not list.
    #[tokio::test]
    async fn a_boot_never_disagrees_with_its_own_snapshot_about_a_box() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma").await;
        make_bot(&jojobot, "delta").await;

        let booted = boot(&jojobot, "sigma").await;
        let named = booted["identity"]["owned_mailbox"]["name"]
            .as_str()
            .expect("the identity names its box")
            .to_string();
        let on_the_board: Vec<&str> = booted["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .map(|b| b["name"].as_str().expect("a name"))
            .collect();
        assert!(
            on_the_board.contains(&named.as_str()),
            "the identity named {named:?}, and the snapshot beside it does not list it: {booted}"
        );
        // …and the other bot's box is on the same board, so this is not passing
        // because the board holds exactly one thing.
        assert!(on_the_board.contains(&"delta"), "{booted}");
    }

    /// **One orientation, one door.** Naming a bot is `start_here` plus an
    /// identity — not a second world-model to drift out of step with the first.
    #[tokio::test]
    async fn a_named_boot_and_an_anonymous_one_hand_over_the_same_world() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

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

        let identified = boot(&jojobot, "dev").await;
        assert_eq!(counts_for(&identified)["counts"]["new"], 1, "{identified}");
        assert_eq!(counts_for(&identified)["yours"], true);
    }
}
