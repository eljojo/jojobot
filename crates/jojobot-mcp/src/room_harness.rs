//! Account-free room queries. This module is absent from production builds.

use crate::*;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

mod seed;
pub use seed::SeedWrite;

/// A grader holds the same ports as the occupant, but never enters dispatch.
/// It creates no caller and never finishes an answer into a session's ledgers.
pub struct Harness {
    handler: Jojobot,
}

impl Harness {
    pub async fn changes(&self, position: Value) -> Result<Value, String> {
        self.handler.harness_changes(position).await
    }

    pub fn new(mut handler: Jojobot) -> Self {
        handler.grading = true;
        Self { handler }
    }

    pub async fn read(&self, verb: &str, arguments: Value) -> Result<Value, String> {
        if arguments.get("sid").is_some() {
            return Err("a grader has no session".into());
        }
        let result = match verb {
            "ping" => self.handler.ping(Parameters(decode(arguments)?)).await,
            "search" => self.handler.search(Parameters(decode(arguments)?)).await,
            "recall" => self.handler.recall(Parameters(decode(arguments)?)).await,
            "list_entities" => {
                self.handler
                    .list_entities(Parameters(decode(arguments)?))
                    .await
            }
            "start_here" => return self.orientation(arguments).await,
            _ => return Err(format!("{verb} is not a room observation")),
        }
        .map_err(|error| error.to_string())?;
        body(result)
    }

    async fn orientation(&self, arguments: Value) -> Result<Value, String> {
        let mut view = body(
            self.handler
                .start_here(Parameters(decode(serde_json::json!({"brief":true}))?))
                .await
                .map_err(|error| error.to_string())?,
        )?;
        let Some(bot) = arguments["bot"].as_str() else {
            return Ok(view);
        };
        let bot = crate::memory::parse::bot_id(bot).map_err(|error| error.to_string())?;
        let entities = self
            .handler
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .map_err(|error| error.to_string())?;
        let entity = entities
            .iter()
            .find(|entity| entity.id == bot)
            .ok_or("the observed bot does not exist")?;
        let doc = self
            .handler
            .memory
            .scan_entity(&bot)
            .await
            .map_err(|error| error.to_string())?;
        let facts = self
            .handler
            .memory
            .recall(&bot)
            .await
            .map_err(|error| error.to_string())?;
        let in_force = jojobot_domain::memory::rules_in_force(&facts);
        let fields = self
            .handler
            .memory
            .fields(&bot)
            .await
            .map_err(|error| error.to_string())?;
        let day = self.handler.clock.today_in(&jiff::tz::TimeZone::UTC);
        let owned_mailbox = self
            .handler
            .owned_mailbox_read(entity)
            .await
            .map_err(|error| error.to_string())?;
        view["identity"] = self
            .handler
            .identity_block(crate::orientation::identity::IdentityParts {
                entity,
                charter: doc
                    .map(|doc| doc.prose)
                    .filter(|text| !text.trim().is_empty()),
                charter_elided: false,
                in_force: &in_force,
                seat_count: jojobot_domain::memory::rule_seats_of(&fields),
                as_of: day,
                owned_mailbox,
            })
            .await;
        let choices: Vec<_> = self
            .handler
            .sessions
            .sessions_of(&bot)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|run| run.is_offerable(self.handler.clock.now(), Some(day)))
            .map(|run| serde_json::json!({"id":run.id.as_str(),"working_on":run.focus}))
            .collect();
        view["session"] = serde_json::json!({"choices":choices});
        Ok(view)
    }
}

fn decode<T: serde::de::DeserializeOwned>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| error.to_string())
}

fn body(result: rmcp::model::CallToolResult) -> Result<Value, String> {
    if let Some(value) = result.structured_content {
        return Ok(value);
    }
    let text = result.content.iter().find_map(|block| block.as_text());
    serde_json::from_str(&text.ok_or("the query returned no JSON")?.text)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
