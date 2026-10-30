//! **The served surface, reached the way a client reaches it.**
//!
//! The occupant uses MCP. The harness seeds and grades through the room's
//! feature-gated route, whose queries and writes run inside that server.
//! Neither path writes raw rows; a cold occupant still proves retrieval.

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::{Value, json};

type Client = rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>;

/// A connection to a room.
pub struct Surface {
    client: Client,
    harness: reqwest::Client,
    harness_url: String,
    harness_token: Option<String>,
    position: tokio::sync::Mutex<Value>,
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
        Ok(Surface {
            client,
            harness: reqwest::Client::new(),
            harness_url: format!("{}/_room_harness", endpoint.trim_end_matches("/mcp")),
            harness_token: None,
            position: tokio::sync::Mutex::new(Value::Null),
        })
    }

    pub(crate) fn with_harness_token(mut self, token: String) -> Self {
        self.harness_token = Some(token);
        self
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
            // **The body rides where the agreed revision reads it**: structured
            // from `2026-07-28`, a text block below it. This client agrees to
            // whatever the SDK's newest is, so which one arrives is the SDK's
            // choice rather than this file's, and reading only one of the two
            // would end every room the day that default moves.
            Ok(result) => result
                .structured_content
                .as_ref()
                .map(ToString::to_string)
                .or_else(|| {
                    result
                        .content
                        .first()
                        .and_then(|block| block.as_text())
                        .map(|text| text.text.clone())
                })
                .unwrap_or_else(|| serde_json::to_string(&result).unwrap_or_default()),
            Err(e) => json!({"transport_error": e.to_string()}).to_string(),
        }
    }

    /// **The day the room's server says it is acting out**, read from `ping`.
    /// `None` is a server on the real clock, which is the state a room that
    /// meant to pin its day must never be in.
    pub async fn stated_day(&self) -> Option<String> {
        let answer = self.read("ping", json!({})).await;
        let body: Value = serde_json::from_str(&answer).ok()?;
        body["clock"]["stated_day"].as_str().map(str::to_string)
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

    /// An observation enters no MCP dispatch and carries no identity.
    pub async fn read(&self, verb: &str, arguments: Value) -> String {
        require_observation(
            self.harness_request(json!({"operation":"read", "verb":verb, "arguments":arguments}))
                .await
                .with_context(|| format!("the {verb} read")),
        )
        .to_string()
    }

    /// The harness holds the cursor; reading notices changes no server state.
    pub async fn changes(&self) -> Result<Value> {
        let mut position = self.position.lock().await;
        let mut answer = self
            .harness_request(json!({"operation":"changes", "position":*position}))
            .await?;
        *position = answer["position"].take();
        Ok(answer["notices"].take())
    }

    pub(crate) async fn refusal_count(&self) -> Result<u64> {
        self.harness_request(json!({"operation":"refusals"}))
            .await?["refused"]
            .as_u64()
            .context("the room did not report its grader refusal count")
    }

    async fn harness_request(&self, request: Value) -> Result<Value> {
        let mut sending = self.harness.post(&self.harness_url).json(&request);
        if let Some(token) = &self.harness_token {
            sending = sending.header("x-jojobot-room-harness", token);
        }
        let response = sending
            .send()
            .await
            .context("reaching the room's compiled harness route")?;
        let status = response.status();
        anyhow::ensure!(status.is_success(), "room harness {status}");
        response.json().await.context("the harness answers JSON")
    }
}

/// What a failed harness read panics with. `run::go` catches a panic that starts
/// with it, and lets any other panic through.
pub(crate) const OBSERVATION_FAILED: &str = "room harness observation failed";

pub(crate) fn require_observation(result: Result<Value>) -> Value {
    // A failed read is no evidence that a negative lock's object is absent, so it
    // is a signal and never a value; `run::go` catches it once.
    result.unwrap_or_else(|error| panic!("{OBSERVATION_FAILED}: {error:#}"))
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
    writes: Vec<Write>,
}

/// The names a filled board's pieces are called, each with a number after it. Characters
/// from the shows and none that a real board holds.
const FILL_WORDS: &[&str] = &[
    "nelson", "ralph", "apu", "otto", "lenny", "carl", "skinner", "patty", "selma", "krusty",
    "milhouse", "barney", "moe", "wiggum", "willie", "agnes", "edna", "martin", "jimbo", "kearney",
];

/// **The identity that occupies every room.** A seed never furnishes it, by any
/// verb: a run must meet the shipped identity, and a seed that stood it up or
/// gave it sessions would be coaching it.
pub const OCCUPANT: &str = "assistant";

/// **One piece of furniture, and who writes it.**
#[derive(Debug, Clone, serde::Serialize)]
enum Write {
    /// A fixture write attributed to the scenario's author.
    AsTheOccupant(String, Value),
    /// A second bot and its charter, before any occupant boots.
    HouseBot {
        handle: String,
        name: String,
        charter: String,
    },
    /// One completed sitting the scenario gives that bot as history.
    HouseSitting {
        bot: String,
        day: String,
        focus: String,
        entry: String,
    },
}

impl Seed {
    pub fn new() -> Seed {
        Seed::default()
    }

    /// **How many things this seed furnishes.** A room read out of a document
    /// has to be able to say that its lines reached the builder, and a count is
    /// the whole of that claim.
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// Put a thing in the world. A `bot` is refused: standing up an identity is
    /// the shipped behaviour under test, not a fixture.
    pub fn entity(mut self, kind: &str, handle: &str, name: &str) -> Result<Seed> {
        anyhow::ensure!(
            kind != "bot",
            "a seed may furnish the room and may not create the identity that occupies it — \
             the shipped `assistant` is what a run must meet",
        );
        self.writes.push(Write::AsTheOccupant(
            "add_entity".to_string(),
            json!({"kind": kind, "handle": handle, "name": name, "source": "the room"}),
        ));
        Ok(self)
    }

    /// Put a thing in the world **under another one**. A kind that requires a
    /// parent — a recurring loop, which is somebody's job rather than a thing
    /// on its own — cannot be furnished any other way.
    pub fn child(mut self, parent: &str, kind: &str, handle: &str, name: &str) -> Result<Seed> {
        anyhow::ensure!(
            kind != "bot",
            "a seed may furnish the room and may not create the identity that occupies it — \
             the shipped `assistant` is what a run must meet",
        );
        self.writes.push(Write::AsTheOccupant(
            "add_entity".to_string(),
            json!({
                "kind": kind, "handle": handle, "name": name,
                "source": "the room", "parent": parent,
            }),
        ));
        Ok(self)
    }

    /// **Declare that the entity or child just added is meant to resemble
    /// another thing already in the room.**
    ///
    /// A resemblance refusal is the guard doing its job: two same-kind names
    /// a couple of edits apart is exactly what it exists to catch, and
    /// overriding it is a JUDGEMENT that the two are different things. A real
    /// caller makes that judgement by hand; a room's world block is a
    /// document, and nobody is standing there at furnish time to make it. So
    /// [`Seed::furnish`] only retries a resemblance refusal past the guard
    /// for a pair THIS DOCUMENT DECLARED — everything else it catches, a
    /// typo included, still refuses.
    ///
    /// Chain it onto the entity or child it describes:
    /// `.entity("thing", "kettle", "Kettle")?.resembling("thing:kettl")`.
    /// Calling it with nothing added yet does nothing — there is no write to
    /// attach the declaration to.
    pub fn resembling(mut self, other: &str) -> Seed {
        if let Some(Write::AsTheOccupant(_, arguments)) = self.writes.last_mut() {
            arguments["declared_near"] = json!(other);
        }
        self
    }

    /// **Fill a board with generated pieces**, `count` children of `parent`, all of one
    /// `kind`, from one seeded rule.
    ///
    /// Each key in `draws` names how that key is drawn for every piece: an array of
    /// values is a choice from it, `"a..b"` is a whole number from a to b and
    /// `{"words": 250}` is a filler note of that many names. The same
    /// `seed` and the same `draws` give the same pieces on every run, so a room that is
    /// too big to write by hand is still a room whose every run meets the same world.
    /// A piece is one `add_entity` with its keys set in the call, so a board of hundreds
    /// is hundreds of calls and not twice that.
    ///
    /// Handles are a name from a fixed list and a number, and no piece is ever a bot.
    pub fn fill(
        mut self,
        parent: &str,
        kind: &str,
        count: usize,
        seed: u64,
        draws: &serde_json::Map<String, Value>,
    ) -> Result<Seed> {
        anyhow::ensure!(
            kind != "bot",
            "a seed may furnish the room and may not create the identity that occupies it",
        );
        anyhow::ensure!(count > 0, "a fill of no pieces furnishes nothing");
        let mut state = seed;
        let mut draw = move || {
            // splitmix64: one small generator, so the pieces depend on nothing but the seed.
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let mut last: Option<String> = None;
        for n in 1..=count {
            let mut sets = serde_json::Map::new();
            for (key, how) in draws {
                let value = match how {
                    Value::Array(choices) if !choices.is_empty() => {
                        let at = (draw() % choices.len() as u64) as usize;
                        choices[at]
                            .as_str()
                            .with_context(|| format!("{key}: a choice is a string"))?
                            .to_string()
                    }
                    Value::String(range) if range.contains("..") => {
                        let (from, to) = range.split_once("..").expect("checked");
                        let (from, to): (u64, u64) = (
                            from.trim()
                                .parse()
                                .with_context(|| format!("{key}: {range:?}"))?,
                            to.trim()
                                .parse()
                                .with_context(|| format!("{key}: {range:?}"))?,
                        );
                        anyhow::ensure!(from <= to, "{key}: {range:?} runs backwards");
                        (from + draw() % (to - from + 1)).to_string()
                    }
                    // `{"words": 250}` is that many names from the list, a filler note of the
                    // size a board's pieces carry when somebody has written about them.
                    Value::Object(spec) if spec.get("words").is_some_and(Value::is_u64) => {
                        let words = spec["words"].as_u64().expect("checked");
                        (0..words)
                            .map(|_| FILL_WORDS[(draw() % FILL_WORDS.len() as u64) as usize])
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    _ => anyhow::bail!(
                        "{key}: a draw is an array of strings to choose from, a range `a..b` \
                         or `{{\"words\": 250}}`"
                    ),
                };
                sets.insert(key.clone(), Value::String(value));
            }
            let word = FILL_WORDS[(draw() % FILL_WORDS.len() as u64) as usize];
            let slug = format!("{word}-{n:04}");
            let mut piece = json!({
                "kind": kind, "handle": slug,
                "name": format!("{} {n:04}", word[..1].to_uppercase() + &word[1..]),
                "source": "the room", "parent": parent, "sets": sets,
            });
            // **A filled board is a run of siblings that resemble one another by
            // construction**, a name and a number each, and the room declares it: the
            // pieces after the first are furnished past the resemblance screen, as a
            // declared pair is. Nothing else in a board is.
            if let Some(previous) = &last {
                piece["declared_near"] = json!(previous);
            }
            last = Some(format!("{kind}:{slug}"));
            self.writes
                .push(Write::AsTheOccupant("add_entity".to_string(), piece));
        }
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
        self.writes.push(Write::AsTheOccupant(
            "capture".to_string(),
            json!({"subject": subject, "content": content, "provenance": provenance}),
        ));
        Ok(self)
    }

    /// Record something about a thing **as values under keys**, which is what
    /// a question later asks about.
    ///
    /// A room that has accumulated is furnished with records rather than with
    /// sentences alone: the year's distance, the day a job was done, the day a
    /// cover runs out. It is the same refusal as [`Seed::fact`] — a record on
    /// an identity is coaching — and the same one call, so the keys a room
    /// already uses are keys the occupant can find and reuse.
    pub fn record(mut self, subject: &str, content: &str, fields: Value) -> Result<Seed> {
        anyhow::ensure!(
            !subject.starts_with("bot:"),
            "a seed may not write a record onto an identity — a rule written for a run measures \
             itself",
        );
        self.writes.push(Write::AsTheOccupant(
            "capture".to_string(),
            json!({
                "subject": subject, "content": content,
                "provenance": "testimony", "sets": fields,
            }),
        ));
        Ok(self)
    }

    /// **Declare a type: a name and the keys a thing of it carries, each with what
    /// it holds.** A comparison, an ordering or a `contains` asked of a key needs
    /// the key declared a number, a date or a list, and nothing else in the room
    /// declares the keys a room invents. Written as the occupant, through the same
    /// verb a session uses, so the declaration is the one the surface serves.
    ///
    /// Each key says what it holds (`number`, `date`, `text`, `list:text`, …), and
    /// a key with none is refused here rather than declared as text, which
    /// licenses no comparison.
    pub fn declare_type(mut self, name: &str, keys: &[(String, String)]) -> Result<Seed> {
        anyhow::ensure!(!name.is_empty(), "a type has a name");
        anyhow::ensure!(
            !keys.is_empty(),
            "the type {name:?} names no key, so nobody could query by it",
        );
        let fields: Vec<Value> = keys
            .iter()
            .map(|(key, holds)| json!({"key": key, "holds": holds}))
            .collect();
        self.writes.push(Write::AsTheOccupant(
            "declare_type".to_string(),
            json!({"name": name, "fields": fields}),
        ));
        Ok(self)
    }

    /// Leave a message waiting in a bot's box, from the shipped identity.
    ///
    /// **The one piece of furniture that speaks.** Everything else a seed
    /// writes is inert until a model goes looking; a message is prose that
    /// lands in front of it, and a room whose goal arrives by mail puts the
    /// whole task here.
    ///
    /// **So the line is what it says, not that it says anything.** A body may
    /// carry the WORK — what happened, what the operator wants — because that
    /// is what a session is for. It may not carry the METHOD: no verb, no
    /// argument, no hint about where anything is stored, and no word that a
    /// test is running. A seed that named a verb would make its own run pass,
    /// and the transcript would read like a product that teaches itself.
    pub fn message(mut self, to: &str, subject: &str, body: &str) -> Seed {
        self.writes.push(Write::AsTheOccupant(
            "post_message".to_string(),
            json!({"to": to, "subject": subject, "body": body}),
        ));
        self
    }

    /// **Put a SECOND bot in the world: its handle, its name and a one-line
    /// charter.** The occupant stands it up and charters it, the way an
    /// operator's assistant stands a colleague up, so a room can hold a house
    /// bot a session meets rather than makes.
    ///
    /// **The occupant itself is refused**, by this verb as by every other: the
    /// shipped identity is what a run must meet, and a charter or a beat written
    /// onto it for a run would measure itself. The refusal comes from the
    /// builder, before anything is written.
    pub fn house_bot(mut self, handle: &str, name: &str, charter: &str) -> Result<Seed> {
        let slug = house_bot_slug(handle)?;
        self.writes.push(Write::HouseBot {
            handle: slug,
            name: name.to_string(),
            charter: charter.to_string(),
        });
        Ok(self)
    }

    /// **One sitting a house bot already had**, on a day it states: a fresh
    /// session as that bot, one beat with a focus, wrapped.
    ///
    /// **The bot must have been furnished earlier in this same seed**, so a
    /// sitting is never written as a bot nobody stood up, and the occupant is
    /// refused as for [`Seed::house_bot`].
    pub fn house_sitting(mut self, bot: &str, day: &str, focus: &str, entry: &str) -> Result<Seed> {
        let slug = house_bot_slug(bot)?;
        anyhow::ensure!(
            self.writes
                .iter()
                .any(|write| matches!(write, Write::HouseBot { handle, .. } if *handle == slug)),
            "bot:{slug} was not furnished earlier in this document, so a sitting of it would be \
             written as nobody — furnish the bot first",
        );
        self.writes.push(Write::HouseSitting {
            bot: slug,
            day: day.to_string(),
            focus: focus.to_string(),
            entry: entry.to_string(),
        });
        Ok(self)
    }

    /// Furniture is scenario data, authored without opening a harness run.
    pub async fn furnish(&self, room: &Surface) -> Result<()> {
        room.harness_request(json!({
            "operation": "seed", "author": "bot:assistant", "writes": self.writes,
        }))
        .await?;
        Ok(())
    }
}

/// **The slug of a house bot's handle**, refusing the occupant. Takes
/// `bot:lisa` or `lisa`; any other kind is no bot.
fn house_bot_slug(handle: &str) -> Result<String> {
    let slug = match handle.split_once(':') {
        Some(("bot", slug)) => slug,
        Some((kind, _)) => anyhow::bail!("{handle:?} is a {kind}, not a bot"),
        None => handle,
    };
    anyhow::ensure!(!slug.is_empty(), "a bot handle names a bot");
    anyhow::ensure!(
        slug != OCCUPANT,
        "a seed may furnish a second bot and never the occupant — the shipped `{OCCUPANT}` is \
         what a run must meet, and a charter or a sitting written onto it would be coaching",
    );
    Ok(slug.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_harness_observation_cannot_grade_an_empty_room() {
        for answer in [json!({"objects":[]}), json!({"status":"blocked"})] {
            assert_eq!(require_observation(Ok(answer.clone())), answer);
        }
        let failed = std::panic::catch_unwind(|| {
            require_observation(Err(anyhow::anyhow!(reqwest::StatusCode::NOT_FOUND)))
        });
        assert!(
            failed.is_err(),
            "a 404 must abort, never look like room state"
        );
    }
}
