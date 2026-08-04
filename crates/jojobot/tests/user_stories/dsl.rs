//! The words a story is written in.
//!
//! Every call goes over the wire to a served jojobot, and every session is its
//! own client — a story's later sessions must work for a client that was not
//! there for the earlier ones.
//!
//! A write that comes back `blocked` fails the test. In a story a refused write
//! means the use case is not reachable, which is the thing being measured.

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::testing::InMemorySessions;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

type Client = RunningService<rmcp::RoleClient, ClientInfo>;

pub struct Story {
    addr: SocketAddr,
    ct: CancellationToken,
    bot: String,
}

impl Story {
    /// Serve a fresh jojobot and stand the bot up, the way an operator would.
    /// `bot` carries its kind prefix, for the same reason `add` does.
    pub async fn begin(bot: &str) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let indexed =
            Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens"));
        let indexed_for_seed = indexed.clone();
        // **Mail goes through the search index, exactly as the binary wires
        // it.** Both worlds sit behind one `search`, so a fixture holding the
        // raw store would serve a jojobot whose mail no story could see —
        // poorer than the deployment it stands for, and silently so.
        let boxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> = Arc::new(IndexedMailboxes::new(
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            indexed.index(),
        ));
        let boxes_for_seed = boxes.clone();
        let state = AppState {
            resource: format!("http://{addr}/mcp"),
            issuer: None,
            validator: None,
            metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
            memory: indexed.clone(),
            search: indexed,
            mailboxes: boxes,
            sessions: Arc::new(InMemorySessions::new()),
            registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        };
        let ct = CancellationToken::new();
        let app = build_app(state, ct.child_token());
        let shutdown = ct.clone();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await
                .unwrap();
        });

        // **A fresh jojobot arrives with its default identity**, exactly as a
        // real one does — seeded before anything serves. Every story runs
        // against a server that has one, because every real server does.
        let seed_memory: Arc<dyn jojobot_domain::memory::Memory> = indexed_for_seed;
        jojobot_mcp::seed::ensure_default_identity(&seed_memory, &boxes_for_seed).await;

        let story = Self {
            addr,
            ct,
            bot: bot.to_string(),
        };
        // The bot exists before anything boots as it; its box opens with it.
        //
        // **Created BY the default identity**, because a memory write carries
        // one. This is the real bootstrap and not a fixture trick: every
        // jojobot arrives with `assistant`, so the first act on a fresh server
        // is somebody booting as it and standing up whoever else is needed.
        let client = story.connect().await;
        let booted = call(
            &client,
            "start_here",
            json!({"bot": jojobot_mcp::seed::DEFAULT_BOT, "brief": true}),
        )
        .await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the default identity must boot: {booted}"))
            .to_string();
        let made = call(
            &client,
            "add_entity",
            json!({
                "kind": "bot", "handle": bot, "name": bot,
                "source": "user-named", "sid": sid,
            }),
        )
        .await;
        assert_ne!(made["status"], "blocked", "the story's bot: {made}");
        client.cancel().await.unwrap();
        story
    }

    async fn connect(&self) -> Client {
        let transport =
            StreamableHttpClientTransport::from_uri(format!("http://{}/mcp", self.addr));
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("user-story", "0.0.1"),
        )
        .serve(transport)
        .await
        .unwrap()
    }

    /// A new session, on its own connection.
    pub async fn session(&self) -> Session {
        let client = self.connect().await;
        let booted = call(
            &client,
            "start_here",
            json!({"bot": self.bot, "brief": true}),
        )
        .await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        Session { client, sid }
    }

    /// A new session's own boot, whole — the essay included. `.session()`
    /// takes `brief` on every other story, because they act after booting;
    /// this exists for a story whose whole point is what a full boot itself
    /// hands over, before anything else is asked of it.
    pub async fn full_boot(&self) -> (Value, Session) {
        let client = self.connect().await;
        let booted = call(&client, "start_here", json!({"bot": self.bot})).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        (booted, Session { client, sid })
    }

    /// A session of ANOTHER bot on this same server — the other side of a
    /// dispatch. Its own connection, which never met the first bot's.
    pub async fn as_bot(&self, bot: &str) -> Session {
        let bot = bot.strip_prefix("bot:").unwrap_or(bot);
        let client = self.connect().await;
        let booted = call(&client, "start_here", json!({"bot": bot, "brief": true})).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        Session { client, sid }
    }

    /// Fetch a shipped procedure by name, through the same door a boot uses.
    /// No bot and no session: reading a skill starts nothing.
    pub async fn skill(&self, name: &str) -> Value {
        let client = self.connect().await;
        let body = call(&client, "start_here", json!({"skill": name})).await;
        client.cancel().await.unwrap();
        body
    }

    pub async fn finish(self) {
        self.ct.cancel();
    }
}

pub struct Session {
    client: Client,
    sid: String,
}

impl Session {
    async fn write(&self, what: &str, tool: &str, mut args: Value) -> Value {
        args["sid"] = self.sid.clone().into();
        let body = call(&self.client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{what} was refused, so this part of the story is not reachable: {body}"
        );
        body
    }

    async fn read(&self, what: String, tool: &str, mut args: Value) -> Answer {
        args["sid"] = self.sid.clone().into();
        let body = call(&self.client, tool, args).await;
        Answer {
            what,
            body: body.to_string(),
        }
    }

    /// **The tripwire for a gap the SURFACE has, rather than a record.**
    ///
    /// A gap marker says a capability is missing. Where the missing thing
    /// would be a verb or an argument, the evidence is the served surface
    /// itself: this reads the verb list a client is given and asserts the verb
    /// is not on it. The positive half is that the list arrived at all and
    /// carries the verbs the story just used — without it the assertion passes
    /// identically on an empty answer.
    ///
    /// It goes red on the day the verb ships, which is the point.
    pub async fn has_no_verb(&self, verb: &str, alongside: &[&str]) {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let names: Vec<&str> = listed.tools.iter().map(|t| t.name.as_ref()).collect();
        for known in alongside {
            assert!(
                names.contains(known),
                "the verb list must carry {known:?} — without it this proves nothing: {names:?}"
            );
        }
        assert!(
            !names.contains(&verb),
            "jojobot now serves {verb:?} — the gap is closed, so flip this assertion: {names:?}"
        );
    }

    /// **The tripwire for a write jojobot refuses today.**
    ///
    /// Returns the refusal so a story can say what it named. Unlike every
    /// other write here, a `blocked` answer is the expected one — this is the
    /// one place a story asserts a use case is NOT reachable, and it fails on
    /// the day the write starts landing.
    pub async fn refused(&self, tool: &str, mut args: Value) -> Answer {
        args["sid"] = self.sid.clone().into();
        // **Both refusal shapes count, and they are different answers.** A
        // domain refusal comes back as a `blocked` body with a way forward; an
        // argument jojobot's schema does not admit is a client error and never
        // reaches the domain at all. A tripwire that accepted only one would
        // pass the day a refusal moved from one shape to the other.
        let body = match self
            .client
            .call_tool(
                CallToolRequestParams::new(tool.to_string())
                    .with_arguments(args.as_object().expect("arguments are an object").clone()),
            )
            .await
        {
            Err(e) => json!({"status": "blocked", "client_error": e.to_string()}),
            Ok(result) => {
                let text = result
                    .content
                    .first()
                    .and_then(|b| b.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
            }
        };
        assert_eq!(
            body["status"], "blocked",
            "{tool} was accepted — the gap is closed, so flip this assertion: {body}"
        );
        Answer {
            what: format!("the refusal from {tool}"),
            body: body.to_string(),
        }
    }

    /// Takes the whole `kind:slug` handle, deliberately: **the fixture-roster
    /// gate scans source text for handle-shaped literals**, so a story that
    /// passed its kind and slug separately would create entities the one guard
    /// against life specifics cannot see.
    pub async fn add(&self, handle: &str, name: &str) {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        self.write(
            &format!("adding {handle}"),
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named"}),
        )
        .await;
    }

    /// Something the person said. Testimony. Hands back the fact's address,
    /// which is what `correct` later edits it through — and what rule 15 calls
    /// the receipt.
    pub async fn fact(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("a fact about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "testimony"}),
            )
            .await;
        address_of(&body)
    }

    /// A fact carrying a date. **Which date it is, is the whole problem** — the
    /// one field means when a claim became known, and a caller almost always
    /// holds a date for when the thing HAPPENED. Rule 101; the answer is
    /// `happened_at` and `recorded_at` as separate columns, not yet built.
    pub async fn fact_on(&self, subject: &str, content: &str, date: &str) {
        self.write(
            &format!("a dated fact about {subject}"),
            "capture",
            json!({
                "subject": subject, "content": content,
                "provenance": "testimony", "date": date,
            }),
        )
        .await;
    }

    /// Something worked out rather than heard. Inference, and it reads back as one.
    pub async fn guess(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("an inference about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "inference"}),
            )
            .await;
        address_of(&body)
    }

    /// **A claim the operator made and is not sure of.**
    ///
    /// Both halves are recorded, because they are two questions. `provenance`
    /// says WHO BACKS IT, and the answer is `testimony`: the operator said it.
    /// `standing` says HOW SURE, and the answer is `open`: they said they were
    /// not sure. It is the one pairing no default can produce.
    pub async fn hedged(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("a hedged claim about {subject}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "standing": "open",
                }),
            )
            .await;
        address_of(&body)
    }

    /// The operator confirms a claim that was standing as a hypothesis, and it
    /// becomes settled. Promotion is gated on exactly this — `confirmed_by_user`
    /// is refused unless it is true, so nothing can promote itself.
    pub async fn confirm(&self, address: &str) {
        self.write(
            &format!("confirming {address}"),
            "update_fact",
            // **Naming `standing` is what routes this through the settling
            // gate.** A restatement of the provenance would close the hedge
            // through the ungated default instead, and never reach the gate.
            json!({
                "address": address,
                "standing": "settled",
                "confirmed_by_user": true,
            }),
        )
        .await;
    }

    /// Something worked out from ANOTHER CLAIM, not from an entity — the
    /// fact-to-fact link. `source` is the parent claim's own address.
    pub async fn guess_from(&self, subject: &str, content: &str, source: &str) -> String {
        let body = self
            .write(
                &format!("an inference about {subject}, sourced from {source}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "inference", "derived_from": source,
                }),
            )
            .await;
        address_of(&body)
    }

    /// Move a claim past: it was true in its day and is not current truth any
    /// more, so it stays on the record and stops coming back as current.
    /// Different from a correction, which rewrites a claim that was never
    /// right, and from a retraction, which is for chronology.
    pub async fn supersede(&self, address: &str) {
        self.write(
            &format!("superseding {address}"),
            "update_fact",
            json!({"address": address, "status": "superseded"}),
        )
        .await;
    }

    /// Rewrite a claim that turned out to be wrong, in place — rule 58, which
    /// says a refutation FIXES THE SOURCE rather than adding a contradiction
    /// beside it. Anything less is two claims and a reader left to adjudicate.
    pub async fn correct(&self, address: &str, content: &str) {
        self.write(
            &format!("correcting {address}"),
            "update_fact",
            json!({"address": address, "content": content}),
        )
        .await;
    }

    pub async fn fact_about(
        &self,
        subject: &str,
        content: &str,
        shape: &str,
        object: &str,
    ) -> String {
        let body = self
            .write(
                &format!("a fact linking {subject} to {object}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "shape": shape, "object": object,
                }),
            )
            .await;
        address_of(&body)
    }

    /// Rewrite a claim in place AND re-point its edge — for the case where
    /// what changed is not just the wording but which thing the claim now
    /// traces to, so the edge does not go on naming what used to be true.
    pub async fn correct_with_source(&self, address: &str, content: &str, object: &str) {
        self.write(
            &format!("correcting {address} and its source"),
            "update_fact",
            json!({
                "address": address, "content": content,
                "shape": "about", "object": object,
            }),
        )
        .await;
    }

    pub async fn recall(&self, subject: &str) -> Answer {
        self.read(
            format!("recall of {subject}"),
            "recall",
            json!({"subject": subject}),
        )
        .await
    }

    pub async fn find(&self, query: &str) -> Answer {
        self.read(
            format!("search for {query:?}"),
            "search",
            json!({"query": query}),
        )
        .await
    }

    /// Every claim nobody vouched for, across the whole store — the filter
    /// asks about provenance rather than about words.
    pub async fn unbacked(&self) -> Answer {
        self.read(
            "everything nobody vouched for".to_string(),
            "search",
            json!({"provenance": "inference"}),
        )
        .await
    }

    /// Walk an edge backwards: every entity of `kind` whose fact draws a
    /// `shape` edge at `object`. The cross-entity question, in one call.
    pub async fn through(&self, shape: &str, object: &str, kind: &str) -> Answer {
        self.read(
            format!("{kind}s linked to {object} by {shape}"),
            "search",
            json!({"kind": kind, "edge": {"shape": shape, "object": object}}),
        )
        .await
    }

    /// The same walk with no kind at all — everything pointing at `object`,
    /// whatever sort of thing it is.
    ///
    /// **`kind` is optional on the wire and the DSL made it look required.**
    /// A story written through `through` had to name a kind per call, so
    /// "what rests on this" read as a question the surface could not answer.
    pub async fn through_any(&self, shape: &str, object: &str) -> Answer {
        self.read(
            format!("everything linked to {object} by {shape}"),
            "search",
            json!({"edge": {"shape": shape, "object": object}}),
        )
        .await
    }

    /// Take an event back. One way, chronology only: a fact is corrected
    /// instead, because the negative truth is still the truth.
    pub async fn retract(&self, address: &str, reason: &str) {
        self.write(
            &format!("retracting {address}"),
            "retract",
            json!({"address": address, "reason": reason}),
        )
        .await;
    }

    pub async fn list(&self, kind: &str) -> Answer {
        self.read(
            format!("listing of {kind}"),
            "list_entities",
            json!({"kind": kind}),
        )
        .await
    }

    pub async fn journal(&self, entry: &str) {
        self.write("a journal entry", "journal", json!({"entry": entry}))
            .await;
    }

    /// A record of something that HAPPENED, rather than something that is
    /// true now. `kind` is free text and jojobot interprets none of it, so two
    /// sessions recording the same class of thing need not agree on the word.
    pub async fn event(&self, subject: &str, content: &str, kind: &str) -> String {
        self.event_with(subject, content, kind, json!({}), &[])
            .await
    }

    /// The same, carrying an event's typed fields and the entities it touches.
    ///
    /// **The plain helper sent neither, and that is why several stories say a
    /// number or a link has nowhere to go but prose.** `capture` takes
    /// `metadata` and `refs`; the DSL dropped them, so a story written through
    /// the DSL could not reach a capability the surface already has, and the
    /// marker recording that read as a gap in jojobot rather than a gap in the
    /// fixture.
    pub async fn event_with(
        &self,
        subject: &str,
        content: &str,
        kind: &str,
        metadata: Value,
        refs: &[&str],
    ) -> String {
        let body = self
            .write(
                &format!("an event about {subject}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "event_type": kind,
                    "metadata": metadata, "refs": refs,
                }),
            )
            .await;
        address_of(&body)
    }

    /// Leave work in another bot's box. The shape of a request: it writes
    /// without reading, so it takes no delivery and obliges nobody.
    pub async fn post(&self, mailbox: &str, subject: &str, body: &str) -> String {
        let sent = self
            .write(
                &format!("posting to {mailbox}"),
                "post_message",
                json!({"mailbox": mailbox, "subject": subject, "body": body}),
            )
            .await;
        sent["id"]
            .as_str()
            .expect("a post hands back its id")
            .to_string()
    }

    /// Take delivery of everything in YOUR OWN box. There is no box argument —
    /// the sid says who is asking, and a bot drains the box it owns.
    pub async fn drain(&self) -> Answer {
        self.read("my mailbox".to_string(), "read_mailbox", json!({}))
            .await
    }

    /// Retire a message once it has been acted on, with the outcome.
    pub async fn processed(&self, id: &str, notes: &str) {
        self.write(
            &format!("processing {id}"),
            "mark_processed",
            json!({"message_id": id, "notes": notes}),
        )
        .await;
    }

    /// Ends the session and the connection with it.
    pub async fn wrap(self, story: &str) {
        self.write(
            "wrapping the session",
            "wrap_session",
            json!({"story": story}),
        )
        .await;
        self.client.cancel().await.unwrap();
    }
}

/// What a read came back with, and what the story expects to find in it.
pub struct Answer {
    what: String,
    body: String,
}

impl Answer {
    /// The assertion that can actually fail on a correction: the old wording is
    /// GONE, not merely outnumbered.
    pub fn never_says(&self, needle: &str) -> &Self {
        assert!(
            !self.body.contains(needle),
            "the {} still carries {needle:?}, so nothing was corrected: {}",
            self.what,
            self.body
        );
        self
    }

    /// **What ONE claim says, picked by its address.** `says` is a substring
    /// over the whole answer, so on a subject holding two facts it proves only
    /// that a token appears SOMEWHERE — which is how a story about two claims
    /// that must differ can pass while they are identical, and how a dropped
    /// argument can hide behind a neighbour supplying the same word.
    pub fn claim(&self, address: &str) -> Answer {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        let fact = body["facts"]
            .as_array()
            .unwrap_or_else(|| panic!("the {} carries no facts: {}", self.what, self.body))
            .iter()
            .find(|f| f["address"] == address)
            .unwrap_or_else(|| {
                panic!(
                    "the {} holds no claim at {address}: {}",
                    self.what, self.body
                )
            })
            .clone();
        Answer {
            what: format!("claim {address}"),
            body: fact.to_string(),
        }
    }

    pub fn says(&self, needle: &str) -> &Self {
        assert!(
            self.body.contains(needle),
            "the {} should mention {needle:?}: {}",
            self.what,
            self.body
        );
        self
    }
}

async fn call(client: &Client, tool: &str, args: Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_string())
                .with_arguments(args.as_object().expect("arguments are an object").clone()),
        )
        .await
        .unwrap_or_else(|e| panic!("{tool} call failed: {e}"));
    let text = result
        .content
        .first()
        .and_then(|b| b.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_else(|| panic!("{tool} returned no text block"));
    serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
}

/// The address a write handed back, which is how a claim is edited later.
fn address_of(body: &Value) -> String {
    body["address"]
        .as_str()
        .or_else(|| body["fact"]["address"].as_str())
        .unwrap_or_else(|| panic!("a captured fact must hand back its address: {body}"))
        .to_string()
}
