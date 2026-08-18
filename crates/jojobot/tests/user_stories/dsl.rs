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
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, IndexedSessions, Retrieval};
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

/// The subject the operator's browser logs in as. One reader, because the
/// listing is his own window and nobody else's.
const READER: &str = "sub-the-operator";

/// The client id the listing registers with its issuer.
const UI_CLIENT: &str = "jojobot-ui";

/// **A store that can stop being readable, the way a real one does.**
///
/// The service behind memory is a process on a network: it is up when jojobot
/// boots or it is not, and it can go away afterwards. Every other story runs
/// against a store that is always there, so the degraded half of the surface —
/// what a search says about its own coverage — has never been reached by one.
///
/// It stages the WORLD, not the answer. Only `scan` fails, which is the read
/// that fills the index; reads and writes still land, exactly as they would
/// against a store whose listing call is failing. Nothing here touches the
/// index or the coverage it reports: the story asks jojobot through the served
/// surface and jojobot works out what to say.
pub struct Blindable {
    inner: Arc<InMemoryMemory>,
    blind: std::sync::atomic::AtomicBool,
}

impl Blindable {
    fn new() -> Self {
        Blindable {
            inner: Arc::new(InMemoryMemory::new()),
            blind: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The store stops answering the read that fills the index.
    pub fn goes_away(&self) {
        self.blind.store(true, std::sync::atomic::Ordering::Release);
    }

    /// …and comes back.
    pub fn comes_back(&self) {
        self.blind
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

#[async_trait::async_trait]
impl jojobot_domain::memory::Memory for Blindable {
    async fn scan(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::search::DocScan>, jojobot_domain::memory::MemoryError>
    {
        if self.blind.load(std::sync::atomic::Ordering::Acquire) {
            return Err(jojobot_domain::memory::MemoryError::Store(
                "the store cannot be read".into(),
            ));
        }
        self.inner.scan().await
    }

    async fn add_entity(
        &self,
        new: jojobot_domain::memory::NewEntity,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.add_entity(new).await
    }

    async fn list_entities(
        &self,
        kind: Option<jojobot_domain::memory::EntityKind>,
    ) -> Result<Vec<jojobot_domain::memory::Entity>, jojobot_domain::memory::MemoryError> {
        self.inner.list_entities(kind).await
    }

    async fn declare_type(
        &self,
        declared: jojobot_domain::memory::types::DeclaredType,
    ) -> Result<jojobot_domain::memory::types::DeclaredType, jojobot_domain::memory::MemoryError>
    {
        self.inner.declare_type(declared).await
    }

    async fn declared_types(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::types::DeclaredType>, jojobot_domain::memory::MemoryError>
    {
        self.inner.declared_types().await
    }

    async fn update_entity(
        &self,
        handle: &jojobot_domain::memory::EntityId,
        patch: jojobot_domain::memory::EntityPatch,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.update_entity(handle, patch).await
    }

    async fn capture(
        &self,
        fact: jojobot_domain::memory::NewFact,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.capture(fact).await
    }

    async fn recall(
        &self,
        subject: &jojobot_domain::memory::EntityId,
    ) -> Result<Vec<jojobot_domain::memory::Fact>, jojobot_domain::memory::MemoryError> {
        self.inner.recall(subject).await
    }

    async fn history(
        &self,
        entity: &jojobot_domain::memory::EntityId,
        key: &str,
    ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, jojobot_domain::memory::MemoryError> {
        self.inner.history(entity, key).await
    }

    async fn update_fact(
        &self,
        address: &jojobot_domain::memory::FactAddress,
        patch: jojobot_domain::memory::FactPatch,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.update_fact(address, patch).await
    }

    async fn retract(
        &self,
        address: &jojobot_domain::memory::FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
    ) -> Result<jojobot_domain::memory::Retraction, jojobot_domain::memory::MemoryError> {
        self.inner.retract(address, reason, date).await
    }

    async fn set_prose(
        &self,
        entity: &jojobot_domain::memory::EntityId,
        prose: &str,
    ) -> Result<String, jojobot_domain::memory::MemoryError> {
        self.inner.set_prose(entity, prose).await
    }
}

impl Story {
    /// Serve a fresh jojobot whose memory store can be taken away — the same
    /// server every other story gets, wired over a [`Blindable`]. Hands back
    /// the store beside the story, because taking it away is the story's move.
    pub async fn begin_over_a_store_that_can_go_away(bot: &str) -> (Self, Arc<Blindable>) {
        let store = Arc::new(Blindable::new());
        let story = Self::serve(bot, store.clone()).await;
        (story, store)
    }

    /// Serve a fresh jojobot and stand the bot up, the way an operator would.
    /// `bot` carries its kind prefix, for the same reason `add` does.
    pub async fn begin(bot: &str) -> Self {
        Self::serve(bot, Arc::new(InMemoryMemory::new())).await
    }

    async fn serve(bot: &str, store: Arc<dyn jojobot_domain::memory::Memory>) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let indexed = Arc::new(IndexedMemory::new(store).expect("index opens"));
        let indexed_for_seed = indexed.clone();
        // **Mail goes through the search index, exactly as the binary wires
        // it.** Both worlds sit behind one `search`, so a fixture holding the
        // raw store would serve a jojobot whose mail no story could see —
        // poorer than the deployment it stands for, and silently so.
        let mail = Arc::new(IndexedMailboxes::new(
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            indexed.index(),
        ));
        // **The retrieval port over ALL THREE halves, exactly as the binary
        // wires it.** A port over fewer answers without ever refreshing the
        // rest, which is a poorer jojobot than the deployment this stands for —
        // and a story would report the fixture's limits as the software's.
        let runs = Arc::new(InMemorySessions::new());
        let indexed_runs = Arc::new(IndexedSessions::new(runs.clone(), indexed.index()));
        let search = Arc::new(Retrieval::new(
            indexed.index(),
            vec![indexed.clone(), mail.clone(), indexed_runs],
        ));
        let boxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> = mail;
        let boxes_for_seed = boxes.clone();
        // **The operator's own window is served too, over the same state.**
        // A story that could not open a page could not tell whether what it
        // wrote is visible to the one reader who does not speak MCP — and every
        // story before this one ran against a server with no page at all, so
        // nothing it renders was ever exercised by a use case.
        let idp = crate::support::TestIdp::new();
        let (_, endpoints) = crate::support::spawn_idp(idp.token_for(READER, UI_CLIENT)).await;
        let ui = jojobot::ui::Ui::new(
            &jojobot::config::UiConfig {
                client_id: UI_CLIENT.to_string(),
                base_url: format!("http://{addr}"),
            },
            endpoints,
            idp.validator_for(UI_CLIENT, &[READER]),
            reqwest::Client::new(),
        );
        let state = AppState {
            resource: format!("http://{addr}/mcp"),
            issuer: Some(crate::support::ISS.to_string()),
            validator: None,
            metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
            memory: indexed.clone(),
            search,
            mailboxes: boxes,
            sessions: runs,
            registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
            ui: Some(Arc::new(ui)),
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

    /// **Open the operator's own page for a handle, as his browser does.**
    ///
    /// The whole login runs — the gate turns the browser away, the issuer
    /// vouches for him, the callback opens a session — because a page reached
    /// any other way is not the page he reads. It takes no `sid` and starts no
    /// run: looking through the window is not a session, and nothing on this
    /// path may change what a bot sees.
    pub async fn page(&self, handle: &str) -> Answer {
        let path = format!("/{handle}/");
        let client = crate::support::browser();
        let cookie = crate::support::log_in(&client, self.addr, &path).await;
        let body = client
            .get(format!("http://{}{path}", self.addr))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("the listing answers")
            .text()
            .await
            .expect("the page is text");
        Answer {
            what: format!("the page for {handle}"),
            body,
        }
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

    /// **Any verb, before there is a session to make it from.**
    ///
    /// [`Session::call`] rides a `sid`; this is the same escape hatch one step
    /// earlier, for the calls a caller makes before it holds an identity — the
    /// door itself, and a liveness probe. Nothing is injected into `args`,
    /// because the point of these calls is what they do without a session.
    ///
    /// Hands back the session when the answer carried a `sid`, and none when
    /// it did not. **That is a real distinction and not a convenience**: a boot
    /// with a run worth picking up answers with the choice and no handle, so a
    /// story asserting the offer has to be able to see the absence.
    pub async fn call(&self, tool: &str, args: Value) -> (Answer, Option<Session>) {
        let client = self.connect().await;
        let body = call(&client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{tool} was refused, so this part of the story is not reachable: {body}"
        );
        let answer = Answer {
            what: format!("the answer from {tool}"),
            body: body.to_string(),
        };
        match body["session"]["sid"].as_str() {
            Some(sid) => {
                let sid = sid.to_string();
                (answer, Some(Session { client, sid }))
            }
            None => {
                client.cancel().await.unwrap();
                (answer, None)
            }
        }
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
    /// **The handle this session carries — unless the story named one.**
    ///
    /// The default does not move: every story here leaves `sid` out and gets
    /// this session's, and a harness that made them pass one would be a
    /// rewrite of all of them for nothing.
    ///
    /// What it must ALSO do is send the handle a story picked, because a
    /// caller sending a handle jojobot no longer holds is behaviour with a
    /// guard behind it, and a harness that overwrites the argument puts that
    /// use case out of a story's reach — rule 152, arriving through an
    /// argument rather than through a verb. **One rule, stated once: if the
    /// story put `sid` in the arguments, it stands exactly as written**, `null`
    /// included, which is how a story sends no handle at all. A named method
    /// per verb would satisfy 152's letter and rebuild the thing it exists to
    /// catch.
    fn riding(&self, args: &mut Value) {
        if args.get("sid").is_none() {
            args["sid"] = self.sid.clone().into();
        }
    }

    async fn write(&self, what: &str, tool: &str, mut args: Value) -> Value {
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{what} was refused, so this part of the story is not reachable: {body}"
        );
        body
    }

    async fn read(&self, what: String, tool: &str, mut args: Value) -> Answer {
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        Answer {
            what,
            body: body.to_string(),
        }
    }

    /// **Any verb, any arguments — the success-path twin of [`Session::refused`].**
    ///
    /// A story reaches the whole served surface through this one method, so a
    /// capability can be exercised the day it ships rather than the day
    /// somebody writes an adapter for it here. The named methods below are for
    /// the moves stories make constantly and earn their place by being used
    /// everywhere; **a verb does not get one just for existing** — that is the
    /// habit that let six verbs and fourteen arguments go unexercised while
    /// every one of them was reachable over the wire.
    ///
    /// The `sid` rides along as it does on every other call, and a story that
    /// names one sends that one — see [`riding`]. A `blocked`
    /// answer fails the beat, for the reason this file's header gives: in a
    /// story a refusal means the use case is not reachable, and [`refused`] is
    /// where that is the expected answer.
    ///
    /// [`refused`]: Session::refused
    /// [`riding`]: Session::riding
    pub async fn call(&self, tool: &str, args: Value) -> Answer {
        let mut args = args;
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{tool} was refused, so this part of the story is not reachable: {body}"
        );
        Answer {
            what: format!("the answer from {tool}"),
            body: body.to_string(),
        }
    }

    /// This run's own handle — what a later boot offers back when this one
    /// stops without wrapping, and what a story compares against to say the
    /// run was resumed rather than replaced.
    pub fn sid(&self) -> &str {
        &self.sid
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

    /// **The tripwire for a gap whose capability will arrive as an ARGUMENT.**
    ///
    /// [`has_no_verb`] cannot hold one of these. A verb has to earn its place
    /// against widening one that exists, so most of what is missing ships as a
    /// parameter on a read that is already served — and a check reading the
    /// verb list is green the day before that lands and green the day after.
    ///
    /// This reads the arguments the verb publishes to every client, at every
    /// depth, and asserts that none of them is named for the capability. The
    /// needle is matched as a SUBSTRING of each published name, because the
    /// spelling is settled when the thing is built: a check pinned to one
    /// guess is the test that cannot fail, one guess later. The positive half
    /// is the verb's own schema arriving and carrying the arguments the
    /// missing one would sit beside.
    ///
    /// It goes red on the day the argument ships, which is the point.
    pub async fn has_no_argument(&self, verb: &str, named_for: &str, alongside: &[&str]) {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let tool = listed
            .tools
            .iter()
            .find(|t| t.name == verb)
            .unwrap_or_else(|| {
                panic!("the verb list must carry {verb:?} for this to prove anything")
            });
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let mut published = Vec::new();
        argument_names(&schema, &mut published);
        for known in alongside {
            assert!(
                published.iter().any(|name| name == known),
                "{verb} must publish {known:?} — without it this proves nothing: {published:?}"
            );
        }
        assert!(
            !published.iter().any(|name| name.contains(named_for)),
            "{verb} now takes an argument named for {named_for:?} — the gap is closed, \
             so flip this assertion: {published:?}"
        );
    }

    /// **The tripwire for a write jojobot refuses today.**
    ///
    /// Returns the refusal so a story can say what it named. Unlike every
    /// other write here, a `blocked` answer is the expected one — this is the
    /// one place a story asserts a use case is NOT reachable, and it fails on
    /// the day the write starts landing.
    pub async fn refused(&self, tool: &str, mut args: Value) -> Answer {
        self.riding(&mut args);
        // **Both refusal shapes count, and they are different answers.** A
        // `blocked` body with a way forward is the answer jojobot writes
        // itself — either a domain refusal, or the argument gate turning back
        // a top-level argument the verb does not implement before dispatch,
        // with nothing written. A VALUE the schema cannot deserialize at all —
        // an unknown `kind`, an unknown `shape` — still fails in the client and
        // never reaches the domain. A tripwire that accepted only one would
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
    /// Rewrite the record's own fields: set the keys named, take away the keys
    /// listed. Every other key on the record is left where it is.
    pub async fn correct_fields(&self, address: &str, set: Value, clear: &[&str]) {
        self.write(
            &format!("correcting the fields of {address}"),
            "update_fact",
            json!({
                "address": address,
                "fields": set, "clear_fields": clear,
            }),
        )
        .await;
    }

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

    /// **A thing and its claims** — the records read, asked for as one.
    ///
    /// `facts` is sent rather than left to the default, because every one of
    /// this helper's call sites asserts something inside a record: the content,
    /// the provenance, the standing, an address, an edge. A story that wants
    /// the thing WITHOUT its claims asks for that shape through [`shape`],
    /// which is where a beat says what it wants; this one is named for the
    /// half it carries, and sending the argument is what keeps it true when
    /// the default moves.
    ///
    /// [`shape`]: Session::shape
    pub async fn recall(&self, subject: &str) -> Answer {
        self.read(
            format!("recall of {subject}"),
            "recall",
            json!({"subject": subject, "facts": true}),
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

    /// The same front door, asking for mail as well. **Mail is opt-in**: the
    /// bare search above answers about the operator's life, and this is how a
    /// session reaches what another session filed.
    pub async fn find_including_mail(&self, query: &str) -> Answer {
        self.read(
            format!("search for {query:?}, mail included"),
            "search",
            json!({"query": query, "include_mail": true}),
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

    /// **The graph query.** `recall` takes the shape of the answer — which
    /// objects, what of each, which edges to walk — so a story writes the
    /// question's own shape rather than reaching for a verb per question.
    ///
    /// `what` is the question in the operator's words, so a failure names the
    /// thing that could not be asked rather than a bag of arguments.
    pub async fn shape(&self, what: &str, args: Value) -> Answer {
        self.read(what.to_string(), "recall", args).await
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

    /// Take a record back. One way: what was said stands and is marked
    /// withdrawn, where a correction rewrites a claim to the current truth.
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
    /// true now. **Nothing on the record says which it is**: the two are one
    /// class, and what a record is gets read off the fields it carries.
    pub async fn event(&self, subject: &str, content: &str) -> String {
        self.event_with(subject, content, json!({}), &[]).await
    }

    /// The same, carrying the record's fields and the entities it touches.
    ///
    /// **The plain helper sends neither, and that is why several stories say a
    /// number or a link has nowhere to go but prose.** `capture` takes
    /// `fields` and `refs`; the DSL dropped them, so a story written through
    /// the DSL could not reach a capability the surface already has, and the
    /// marker recording that read as a gap in jojobot rather than a gap in the
    /// fixture.
    pub async fn event_with(
        &self,
        subject: &str,
        content: &str,
        fields: Value,
        refs: &[&str],
    ) -> String {
        let body = self
            .write(
                &format!("a record about {subject}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony",
                    "fields": fields, "refs": refs,
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
                json!({"to": mailbox, "subject": subject, "body": body}),
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

    /// **One named table on a page**, so an assertion says which part of it it
    /// is about.
    ///
    /// A page repeats a value in more than one place on purpose — a thing's
    /// folded fields and the record each one came from — so a substring over
    /// the whole page cannot tell the two apart, and a beat written that way
    /// passes on a build where the part it names is missing.
    pub fn section(&self, id: &str) -> Answer {
        let anchor = format!("id=\"{id}\"");
        let start = self
            .body
            .find(&anchor)
            .unwrap_or_else(|| panic!("no {id} section in the {}: {}", self.what, self.body));
        let rest = &self.body[start..];
        let body = match rest.find("</table>") {
            Some(end) => &rest[..end],
            None => rest,
        };
        Answer {
            what: format!("{} section of the {}", id, self.what),
            body: body.to_string(),
        }
    }

    /// **What ONE claim says, picked by its address.** `says` is a substring
    /// over the whole answer, so on a subject holding two facts it proves only
    /// that a token appears SOMEWHERE — which is how a story about two claims
    /// that must differ can pass while they are identical, and how a dropped
    /// argument can hide behind a neighbour supplying the same word.
    pub fn claim(&self, address: &str) -> Answer {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        // **Wherever the answer keeps its claims.** `recall` answers with
        // objects, each carrying its own and each carrying the objects it
        // reached; other answers carry a flat list. A claim is picked by its
        // ADDRESS, which is unique across every shape, so gathering from all
        // of them is not a guess about which verb replied — and reaching into
        // the walk is the point, because what a nested object carries is
        // exactly what a story about a walk is asserting on.
        fn carried(body: &Value, into: &mut Vec<Value>) {
            into.extend(body["facts"].as_array().into_iter().flatten().cloned());
            for object in body["objects"]
                .as_array()
                .into_iter()
                .chain(body["connected"].as_array())
                .flatten()
            {
                carried(object, into);
            }
        }
        let mut found = Vec::new();
        carried(&body, &mut found);
        let fact = found
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

    /// One top-level field of the answer, as a string — the id a later beat
    /// addresses this thing through. Panics rather than returning an empty
    /// string when the field is missing, so a beat built on it cannot go on
    /// asserting against nothing.
    pub fn field(&self, name: &str) -> String {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        body[name]
            .as_str()
            .unwrap_or_else(|| panic!("the {} carries no {name}: {}", self.what, self.body))
            .to_string()
    }

    /// A value the answer's advice tells the caller to send back, read out
    /// from `key: "…"`.
    ///
    /// **The override token rides inside the advice rather than in a field of
    /// its own**, so a story that wants to send one back has to read it out of
    /// there. This keys on the ARGUMENT NAME the advice says to send it under —
    /// something a rename would be a real change to — and not on the sentence
    /// around it, which is ours to improve.
    pub fn advised(&self, key: &str) -> String {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        let advice = body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("the {} carries no advice: {}", self.what, self.body));
        let marker = format!("{key}: \"");
        let from = advice
            .find(&marker)
            .unwrap_or_else(|| panic!("the {} names no {key}: {advice}", self.what))
            + marker.len();
        let rest = &advice[from..];
        let to = rest
            .find('"')
            .unwrap_or_else(|| panic!("the {key} in the {} is unterminated: {advice}", self.what));
        rest[..to].to_string()
    }

    /// The answer as it came back, for a beat that has to reason about the
    /// ORDER of what is in it rather than only about what is there.
    pub fn raw(&self) -> &str {
        &self.body
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

/// **Every argument name one verb publishes, at every depth.**
///
/// The keys of each `properties` map anywhere in the schema document, the
/// definitions an argument of its own shape is emitted through included — so a
/// name nested inside a filter or a walk is read like a top-level one. Nothing
/// here enumerates the levels a schema may have: it takes the whole document
/// the client was served and reads every level it finds.
fn argument_names(node: &Value, found: &mut Vec<String>) {
    if let Some(properties) = node.get("properties").and_then(|p| p.as_object()) {
        found.extend(properties.keys().cloned());
    }
    match node {
        Value::Object(map) => map.values().for_each(|child| argument_names(child, found)),
        Value::Array(items) => items.iter().for_each(|child| argument_names(child, found)),
        _ => {}
    }
}

/// The address a write handed back, which is how a claim is edited later.
fn address_of(body: &Value) -> String {
    body["address"]
        .as_str()
        .or_else(|| body["fact"]["address"].as_str())
        .unwrap_or_else(|| panic!("a captured fact must hand back its address: {body}"))
        .to_string()
}
