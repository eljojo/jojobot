//! **The served surface, reached the way a client reaches it.**
//!
//! Everything a run does to a room — seeding it, driving a model through it,
//! reading it back to assert — goes through this one door, over MCP. There is
//! no path here that writes a row, and that is not an accident: a starting
//! state the software could not have produced proves nothing, and an assertion
//! read out of the database proves nothing about whether a later session could
//! have retrieved it.

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::{Value, json};

type Client = rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>;

/// A connection to a room.
pub struct Surface {
    client: Client,
}

impl Surface {
    /// Connect to a room that is already answering.
    pub async fn connect(endpoint: &str) -> Result<Surface> {
        let transport = StreamableHttpClientTransport::from_uri(endpoint.to_string());
        let client = ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("jojobot-exercise", env!("CARGO_PKG_VERSION")),
        )
        .serve(transport)
        .await
        .with_context(|| format!("connecting to the room at {endpoint}"))?;
        Ok(Surface { client })
    }

    /// **Call a verb and hand back whatever the room said**, refusals included.
    ///
    /// A blocked answer is not an error here and must never be turned into one:
    /// what this tier is asking is whether a session can act on what a refusal
    /// tells it, and a harness that swallowed the refusal would be answering a
    /// different question. A transport failure is different — it is the room
    /// breaking rather than answering — and comes back as text saying so, so
    /// one failed call does not end a run that has other phases to do.
    pub async fn call(&self, verb: &str, arguments: Value) -> String {
        let mut request = CallToolRequestParams::new(verb.to_string());
        if let Some(object) = arguments.as_object() {
            request = request.with_arguments(object.clone());
        }
        match self.client.call_tool(request).await {
            Ok(result) => result
                .content
                .first()
                .and_then(|block| block.as_text())
                .map(|text| text.text.clone())
                .unwrap_or_else(|| serde_json::to_string(&result).unwrap_or_default()),
            Err(e) => json!({"transport_error": e.to_string()}).to_string(),
        }
    }

    /// The same call, for a caller that must not carry on when it fails — the
    /// seed, whose whole job is to leave a known state behind.
    pub async fn must(&self, verb: &str, arguments: Value) -> Result<Value> {
        let text = self.call(verb, arguments.clone()).await;
        let body: Value = serde_json::from_str(&text)
            .with_context(|| format!("{verb} answered something that is not JSON: {text}"))?;
        anyhow::ensure!(
            body["status"] != "blocked" && body["transport_error"].is_null(),
            "{verb} was refused, so the room did not reach its starting state: {text}",
        );
        Ok(body)
    }

    /// **Every verb this room serves, as the model is given them.** Read off
    /// the room rather than written down here: what is under test includes the
    /// descriptions, so a driver holding its own copy would be testing itself.
    pub async fn tools_for_the_model(&self) -> Result<Vec<Value>> {
        let listed = self
            .client
            .list_tools(Default::default())
            .await
            .context("reading the room's verb list")?;
        Ok(listed
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description.as_deref().unwrap_or_default(),
                    "input_schema": tool.input_schema,
                })
            })
            .collect())
    }

    pub async fn finish(self) {
        let _ = self.client.cancel().await;
    }
}

/// **What a room is furnished with before a run starts.**
///
/// The seed writes the WORLD — the entities and facts a playbook needs to
/// stand on — and it writes nothing else. It cannot install a charter, cannot
/// write a rule onto a bot and cannot stand an identity up, and that is
/// enforced here rather than remembered: the claim this tier tests is that a
/// session boots, learns who it is and gets somewhere **from the defaults
/// alone**. A harness that coached the occupant would make its own runs pass
/// and the tier would prove nothing, invisibly.
///
/// The shipped `assistant` is already there — a fresh instance arrives holding
/// it, before anything serves — so the room the model meets is the instance as
/// it ships.
#[derive(Debug, Clone, Default)]
pub struct Seed {
    writes: Vec<(String, Value)>,
}

impl Seed {
    pub fn new() -> Seed {
        Seed::default()
    }

    /// Put a thing in the world. A `bot` is refused: standing up an identity is
    /// the shipped behaviour under test, not a fixture.
    pub fn entity(mut self, kind: &str, handle: &str, name: &str) -> Result<Seed> {
        anyhow::ensure!(
            kind != "bot",
            "a seed may furnish the room and may not create the identity that occupies it — \
             the shipped `assistant` is what a run must meet",
        );
        self.writes.push((
            "add_entity".to_string(),
            json!({"kind": kind, "handle": handle, "name": name, "source": "the room"}),
        ));
        Ok(self)
    }

    /// Record something about a thing in the world. A claim about a bot is
    /// refused for the same reason: a rule on an identity is coaching.
    pub fn fact(mut self, subject: &str, content: &str, provenance: &str) -> Result<Seed> {
        anyhow::ensure!(
            !subject.starts_with("bot:"),
            "a seed may not write a rule onto an identity — a charter or rule written for a run \
             measures itself",
        );
        self.writes.push((
            "capture".to_string(),
            json!({"subject": subject, "content": content, "provenance": provenance}),
        ));
        Ok(self)
    }

    /// Leave a message waiting in a bot's box, from the shipped identity.
    ///
    /// **Furniture, and it has to read as furniture.** Everything else a seed
    /// writes is inert to a model until it goes looking; a message is prose
    /// that lands in front of it. So the body says what is in the world and
    /// never what to do about it — a seed that told the occupant anything would
    /// make its own run pass and prove nothing.
    pub fn message(mut self, to: &str, subject: &str, body: &str) -> Seed {
        self.writes.push((
            "post_message".to_string(),
            json!({"to": to, "subject": subject, "body": body}),
        ));
        self
    }

    /// **Furnish the room, through the verbs, as the shipped identity.**
    ///
    /// It boots `assistant` to get a handle, because every write names a
    /// session — which is the same door a run's model goes through, and the
    /// same door an operator's first session goes through.
    pub async fn furnish(&self, room: &Surface) -> Result<()> {
        let booted = room
            .must("start_here", json!({"bot": "assistant", "brief": true}))
            .await?;
        let sid = booted["session"]["sid"]
            .as_str()
            .context("the shipped identity booted without handing back a handle")?
            .to_string();
        for (verb, arguments) in &self.writes {
            let mut arguments = arguments.clone();
            arguments["sid"] = json!(sid);
            room.must(verb, arguments).await?;
        }
        Ok(())
    }
}
