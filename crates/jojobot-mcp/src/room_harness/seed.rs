//! Scenario furniture uses the same decisions and writes as the MCP handlers.

use super::*;
use jojobot_domain::session::{NewEntry, NewSession, SessionState};

#[derive(serde::Deserialize)]
pub enum SeedWrite {
    AsTheOccupant(String, Value),
    HouseBot {
        handle: String,
        name: String,
        charter: String,
    },
    HouseSitting {
        bot: String,
        day: String,
        focus: String,
        entry: String,
    },
}

impl Harness {
    pub async fn seed(&self, author: &str, writes: Vec<SeedWrite>) -> Result<Value, String> {
        let author = crate::memory::parse::bot_id(author).map_err(err)?;
        if !self
            .handler
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .map_err(err)?
            .iter()
            .any(|bot| bot.id == author)
        {
            return Err(format!("the scenario's author {author} does not exist"));
        }
        let count = writes.len();
        for write in writes {
            let (verb, args, charter) = match write {
                SeedWrite::AsTheOccupant(verb, args) => (verb, args, None),
                SeedWrite::HouseBot {
                    handle,
                    name,
                    charter,
                } => (
                    "add_entity".into(),
                    serde_json::json!({"kind":"bot", "handle":handle, "name":name, "source":"the room"}),
                    Some((handle, charter)),
                ),
                SeedWrite::HouseSitting {
                    bot,
                    day,
                    focus,
                    entry,
                } => {
                    let bot = crate::memory::parse::bot_id(&bot).map_err(err)?;
                    let day: jiff::civil::Date = day.parse().map_err(err)?;
                    let at = jojobot_domain::clock::Clock::stating(day).now();
                    // This run is scenario history, never a harness login.
                    let run = self
                        .handler
                        .sessions
                        .begin(NewSession {
                            bot,
                            sid: sid::Sid(sid::draw()),
                            id: None,
                            key_hash: None,
                            focus: focus.clone(),
                            started_at: at,
                            timezone: None,
                            started_on: Some(day),
                        })
                        .await
                        .map_err(err)?;
                    self.handler
                        .sessions
                        .append(
                            &run.id,
                            NewEntry::manual(entry, at, Some(day)).closing(Some(focus)),
                        )
                        .await
                        .map_err(err)?;
                    self.handler
                        .sessions
                        .close(&run.id, SessionState::Wrapped)
                        .await
                        .map_err(err)?;
                    continue;
                }
            };
            let day = self.handler.clock.today_in(&jiff::tz::TimeZone::UTC);
            match verb.as_str() {
                "add_entity" => {
                    let near = args["declared_near"].as_str().map(str::to_string);
                    let added = accepted(
                        self.handler
                            .add_entity_write(&author, decode(args)?, day, None, near.as_deref())
                            .await
                            .map_err(err)?,
                    )?;
                    if !added.complete() {
                        return Err(
                            "the bot was created but its mailbox could not be opened".into()
                        );
                    }
                }
                "capture" => {
                    accepted(
                        self.handler
                            .capture_write(
                                &author,
                                decode(args)?,
                                day,
                                None,
                                &crate::memory::capture::Mode::default(),
                                None,
                            )
                            .await
                            .map_err(err)?,
                    )?;
                }
                "declare_type" => {
                    accepted(
                        self.handler
                            .declare_type_write(Some(&author), decode(args)?, None)
                            .await
                            .map_err(err)?,
                    )?;
                }
                "post_message" => {
                    let args = decode(args)?;
                    let recipient =
                        accepted(self.handler.message_recipient(&args).await.map_err(err)?)?;
                    let destination = accepted(
                        self.handler
                            .message_destination(&recipient)
                            .await
                            .map_err(err)?,
                    )?;
                    accepted(
                        self.handler
                            .post_message_write(
                                &author,
                                args,
                                &recipient,
                                destination,
                                crate::mailboxes::post_message::write::MessageAudit::default(),
                            )
                            .await
                            .map_err(err)?,
                    )?;
                }
                _ => return Err(format!("{verb} is not room furniture")),
            }
            if let Some((handle, charter)) = charter {
                let bot = crate::memory::parse::bot_id(&handle).map_err(err)?;
                accepted(
                    self.handler
                        .set_charter_write(&author, &bot, &charter, None)
                        .await
                        .map_err(err)?,
                )?;
            }
        }
        Ok(serde_json::json!({"seeded":count}))
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn accepted<T>(outcome: Result<T, rmcp::model::CallToolResult>) -> Result<T, String> {
    match outcome {
        Ok(written) => Ok(written),
        Err(refused) => Err(body(refused)?.to_string()),
    }
}
