//! Scenario furniture uses the production write ports without a setup run.

use super::*;
use crate::memory::parse::{entity_id, parse_boot, parse_provenance};
use jojobot_domain::attention;
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
            match write {
                SeedWrite::AsTheOccupant(verb, arguments) => {
                    self.write(&author, &verb, arguments).await?
                }
                SeedWrite::HouseBot {
                    handle,
                    name,
                    charter,
                } => {
                    self.write(
                        &author,
                        "add_entity",
                        serde_json::json!({
                            "kind":"bot", "handle":handle, "name":name, "source":"the room"
                        }),
                    )
                    .await?;
                    let bot = crate::memory::parse::bot_id(&handle).map_err(err)?;
                    self.handler
                        .memory
                        .set_prose(&bot, &charter, None)
                        .await
                        .map_err(err)?;
                }
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
                }
            }
        }
        Ok(serde_json::json!({"seeded":count}))
    }

    async fn write(&self, author: &EntityId, verb: &str, args: Value) -> Result<(), String> {
        match verb {
            "add_entity" => {
                let id = entity_id(text(&args, "kind")?, text(&args, "handle")?).map_err(err)?;
                let mut new =
                    NewEntity::new(id.clone(), text(&args, "name")?, text(&args, "source")?);
                new.parent = args["parent"].as_str().map(EntityId::person);
                new.boot = parse_boot(args["boot"].as_str()).map_err(err)?;
                new.override_token = args["override_token"].as_str().map(str::to_string);
                new.override_reason = args["override_reason"].as_str().map(str::to_string);
                let first = match args.get("sets") {
                    Some(sets) => Some(
                        self.prepare(self.fact(author, &id, &new.name, sets)?)
                            .await
                            .0,
                    ),
                    None => None,
                };
                let written = self.create(new.clone(), first.clone()).await?;
                let entity = match written {
                    Guarded::Blocked {
                        attempted,
                        candidates,
                    } if args.get("declared_near").is_some() && attempted == new.id => {
                        new.override_token = Some(guard::override_token(&attempted, &candidates));
                        written_value(self.create(new, first).await?)?
                    }
                    other => written_value(other)?,
                };
                if entity.id.kind() == Some(EntityKind::BOT) {
                    let name = MailboxName(
                        entity
                            .id
                            .as_str()
                            .strip_prefix("bot:")
                            .ok_or("bot handle")?
                            .to_string(),
                    );
                    mailbox_written(
                        self.handler
                            .mailboxes
                            .create_mailbox(&name, &entity.id, None)
                            .await
                            .map_err(err)?,
                    )?;
                }
            }
            "capture" => {
                let subject = EntityId::person(text(&args, "subject")?);
                let mut fact =
                    self.fact(author, &subject, text(&args, "content")?, &args["sets"])?;
                fact.provenance = parse_provenance(args["provenance"].as_str()).map_err(err)?;
                let (fact, clears_due) = self.prepare(fact).await;
                written_value(self.handler.memory.capture(fact).await.map_err(err)?)?;
                if clears_due {
                    self.handler
                        .clear_stored_due_moment(&subject, author, None)
                        .await
                        .map_err(|error| {
                            format!("the seeded due moment could not be cleared: {error:?}")
                        })?;
                }
            }
            "declare_type" => {
                let mut fields = Vec::new();
                for field in args["fields"]
                    .as_array()
                    .ok_or("type fields are an array")?
                {
                    let key = text(field, "key")?;
                    let holds = text(field, "holds")?;
                    fields.push(
                        Field::of_token(key, holds)
                            .ok_or_else(|| format!("unknown field type {holds}"))?,
                    );
                }
                self.handler
                    .memory
                    .declare_type(DeclaredType::new(text(&args, "name")?, fields), None)
                    .await
                    .map_err(err)?;
            }
            "post_message" => {
                let to = text(&args, "to")?;
                let bot = crate::memory::parse::bot_id(to).map_err(err)?;
                let board = self.handler.mailboxes.list_mailboxes().await.map_err(err)?;
                let matches: Vec<_> = board
                    .iter()
                    .filter(|box_| box_.owner == bot || box_.name.as_str() == to)
                    .collect();
                if matches.len() != 1 {
                    return Err(format!("{to} does not name exactly one mailbox"));
                }
                let message = NewMessage {
                    mailbox: matches[0].name.clone(),
                    body: text(&args, "body")?.into(),
                    subject: args["subject"].as_str().map(str::to_string),
                    sender: author.as_str().into(),
                    sent_at: self.handler.clock.now(),
                    in_reply_to: None,
                    supersedes: None,
                    sender_mail_waiting_at_send: None,
                    posted_by_session: None,
                };
                mailbox_written(
                    self.handler
                        .mailboxes
                        .post_message(message)
                        .await
                        .map_err(err)?,
                )?;
            }
            _ => return Err(format!("{verb} is not room furniture")),
        }
        Ok(())
    }

    fn fact(
        &self,
        author: &EntityId,
        subject: &EntityId,
        content: &str,
        sets: &Value,
    ) -> Result<NewFact, String> {
        let mut fact = NewFact::about(
            subject.clone(),
            content,
            self.handler.clock.today_in(&jiff::tz::TimeZone::UTC),
        );
        if !sets.is_null() {
            fact.fields = decode(sets.clone())?;
        }
        fact.sets = fact.fields.keys().cloned().collect();
        fact.written_by_bot = Some(author.clone());
        Ok(fact)
    }

    /// Seeded settings carry the same arithmetic as a capture, with no caller.
    async fn prepare(&self, mut fact: NewFact) -> (NewFact, bool) {
        let unread = crate::memory::instance_unread::InstanceUnread::default();
        let (moved, derived) = self
            .handler
            .moved_due_moment(&fact.subject, &fact.fields, &[], fact.recorded_at, &unread)
            .await;
        if let attention::DueMove::Set(day) = moved {
            fact.fields
                .insert(attention::DUE_ON.to_string(), day.to_string());
            fact.derived.insert(attention::DUE_ON.to_string());
            if derived {
                fact.provenance = Provenance::Inference;
            }
        }
        (fact, matches!(moved, attention::DueMove::Cleared))
    }

    async fn create(
        &self,
        new: NewEntity,
        first: Option<NewFact>,
    ) -> Result<Guarded<Entity>, String> {
        match first {
            Some(first) => Ok(
                match self
                    .handler
                    .memory
                    .add_entity_with_first_claim(new, first)
                    .await
                    .map_err(err)?
                {
                    Guarded::Written((entity, _)) => Guarded::Written(entity),
                    Guarded::Blocked {
                        attempted,
                        candidates,
                    } => Guarded::Blocked {
                        attempted,
                        candidates,
                    },
                },
            ),
            None => self.handler.memory.add_entity(new).await.map_err(err),
        }
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("{key} is a string"))
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn written_value<T>(value: Guarded<T>) -> Result<T, String> {
    match value {
        Guarded::Written(value) => Ok(value),
        Guarded::Blocked {
            attempted,
            candidates,
        } => Err(format!("room seed refused {attempted}: {candidates:?}")),
    }
}

fn mailbox_written<T>(value: mailbox::Guarded<T>) -> Result<T, String> {
    match value {
        mailbox::Guarded::Written(value) => Ok(value),
        mailbox::Guarded::Blocked {
            attempted,
            candidates,
        } => Err(format!(
            "room seed refused mailbox {attempted}: {candidates:?}"
        )),
        mailbox::Guarded::UnknownOwner {
            attempted,
            candidates,
        } => Err(format!(
            "room seed refused owner {attempted}: {candidates:?}"
        )),
    }
}
