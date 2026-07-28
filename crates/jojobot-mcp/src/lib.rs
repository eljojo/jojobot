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
pub mod orientation;
pub mod session;
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
pub use orientation::OrientArgs;
pub use session::{AmendJournalArgs, JournalArgs, WrapSessionArgs};

use mailboxes::wire::*;
use memory::declined::*;
use memory::parse::*;
use memory::wire::*;
use rmcp::{
    ErrorData as McpError, ServerHandler, handler::server::router::tool::ToolRouter, model::*,
    tool_handler, tool_router,
};
use session::declined::*;
use session::wire::*;

// --- mailboxes ---------------------------------------------------------------

// --- sessions ----------------------------------------------------------------

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
    use crate::session::testing::*;
    use rmcp::handler::server::wrapper::Parameters;

    use super::*;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::testing::InMemorySessions;

    // --- search: the front door -----------------------------------------------

    // --- the entity verbs -----------------------------------------------------

    // --- the write guard, through the MCP boundary ----------------------------

    // --- structured edges at capture ------------------------------------------

    // --- addresses and update -------------------------------------------------

    // --- mailboxes ------------------------------------------------------------

    // ── start_here ──────────────────────────────────────────────────────────

    // ── a bot and its box are one act ───────────────────────────────────────

    // ── booting as an identity ──────────────────────────────────────────────

    // ── the two-branch boot ─────────────────────────────────────────────────

    // ── sessions ────────────────────────────────────────────────────────────

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
                    sid: fixture_sid(line!()),
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
}
