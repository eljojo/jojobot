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
use jojobot_adapters::search::IndexedMemory;
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
        let state = AppState {
            resource: format!("http://{addr}/mcp"),
            issuer: None,
            validator: None,
            metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
            memory: indexed.clone(),
            search: indexed,
            mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
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

        let story = Self {
            addr,
            ct,
            bot: bot.to_string(),
        };
        // The bot exists before anything boots as it; its box opens with it.
        let client = story.connect().await;
        let made = call(
            &client,
            "add_entity",
            json!({"kind": "bot", "handle": bot, "name": bot, "source": "user-named"}),
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

    /// Takes the whole `kind:slug` handle, deliberately — **split into two
    /// arguments it writes no handle-shaped literal, and the fixture-roster gate
    /// scans for exactly that.** Every entity a story created would have been
    /// invisible to the one guard that keeps life specifics out of the files most
    /// likely to carry them.
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
