//! `add_entity` — Bring a new entity into existence — and, for a bot, the box that comes with it.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;
use crate::teaching::{
    DUPLICATE_REPAIR_DOMAIN, DUPLICATE_REPAIR_TEACHING, PROJECTS_SKILL_DOMAIN,
    PROJECTS_SKILL_TEACHING, RHYTHM_HISTORY_DOMAIN, RHYTHM_HISTORY_TEACHING,
};

/// Arguments to `add_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddEntityArgs {
    /// One of `person`, `project`, `place`, `event`, `work`, `thing`, `org`,
    /// `topic`, `bot`, `pet`, `rhythm`, `promise`, `machine`, `view`, `thread`.
    ///
    /// **A pet is a `pet` and not a `thing`.** `thing` is a named possession —
    /// a bike, a hand tool — and a companion animal is not one.
    ///
    /// **A computer is a `machine` and not a `thing` either**, and the kind
    /// names the object rather than a role: a guest running on another machine
    /// is a `machine`, and so is the one it runs on.
    ///
    /// **A `rhythm` is a recurring loop** — a thing that comes round on a
    /// cadence — and, with `promise`, one of the two kinds that REQUIRE a
    /// `parent`: the parent says whose job the loop is. Two loops on one object
    /// are two rhythms, which is why they are entities rather than a label on
    /// the object.
    ///
    /// **A `promise` is something somebody has to do by a day** — a borrowed
    /// thing that has to go back. It is its own entity for the same reason: it
    /// keeps its own events, and two can exist about one thing. Its `parent`
    /// says whose job it is to keep.
    ///
    /// **A `work` is a piece of a `project`'s work, filed under it**, and it
    /// carries the keys that say where it stands: `status`, `owner`,
    /// `waiting_on` and `depends_on`. A `project` may list its own statuses under
    /// `columns`. The projects skill, fetched through `start_here`, says how
    /// to write and read them.
    ///
    /// **`bot` is an ordinary kind here**, and creating one is what this verb
    /// is for: nothing about an identity is compiled in, so every bot beyond
    /// the one a fresh instance ships with is made through this call — and the
    /// mailbox it owns opens in the same act.
    ///
    /// **A `thread` is an ongoing line in somebody's life, not a `project`
    /// and not a `topic`.** A project moves toward completion and ends when
    /// it is DONE; a thread ends when it stops being TRUE. A topic is the glue noun
    /// for a world-fact belonging to no person, place or project; a thread
    /// belongs to somebody's life and is never the anchor of last resort.
    pub(crate) kind: String,
    /// The slug half of the handle (`[a-z0-9-]+`), or a full `kind:slug` id
    /// whose kind must match `kind`. The handle is the name this entity is
    /// addressed by, and the only name a caller ever sends: every fact, message
    /// and journal beat that refers to it carries this string. Choose it with
    /// care.
    pub(crate) handle: String,
    /// Display name, as a human would write it.
    pub(crate) name: String,
    /// The other names this one answers to — nickname, short form, initials.
    /// Screened and searched exactly as `name` is, so a nickname the user
    /// actually says is both recognized and findable. No commas.
    #[serde(default)]
    pub(crate) aliases: Option<Vec<String>>,
    /// Where this entity came from — **never invented**: the user named it, or
    /// a real source produced it (e.g. `user-named`, `crm-card`, `calendar`).
    pub(crate) source: String,
    /// Optional cross-link to this entity in the task layer, in whatever form
    /// that layer addresses things. One reference, no space and no comma.
    #[serde(default)]
    pub(crate) crm: Option<String>,
    /// `always` marks this entity as part of the core an assistant loads at
    /// the start of every session; the default `on-demand` is fetched when the
    /// conversation reaches for it. Only the exact token `always` counts.
    #[serde(default)]
    pub(crate) boot: Option<String>,
    /// **The entity this one sits under**, as `kind:slug`. Optional — most
    /// entities are roots — and **it must already exist**, exactly as every
    /// other handle a write names must: a parent jojobot does not know comes
    /// back blocked with candidates, and nothing is written. Nothing may be its
    /// own parent.
    ///
    /// **Fixed here and nowhere else.** There is no reparenting verb: where a
    /// thing sits is decided when it is created, so choose it as deliberately
    /// as the handle.
    ///
    /// A `rhythm` requires one — it is a loop ON something, and the parent is
    /// what says on what.
    #[serde(default)]
    pub(crate) parent: Option<String>,
    /// **What this call sets on the new thing**, written as its first claim in
    /// the same act, so a piece of work is made and says where it stands in one
    /// call. The claim says the thing's name and carries these keys, exactly as
    /// the keys of a `capture` would, and it faces every guard a capture does: a
    /// key the kind declares is held to what it declares, and a key that holds a
    /// handle must name a thing that exists.
    ///
    /// **Whole or not at all.** A key that is refused refuses the whole call and
    /// nothing is created, so there is never a half-made thing. Leave it off and
    /// the call creates the thing alone, as before.
    ///
    /// The claim is `inference`, as a capture's is when it says nothing. To put
    /// the operator's own word behind a key, write it with `capture`.
    #[serde(default)]
    pub(crate) sets: Option<std::collections::BTreeMap<String, String>>,
    /// The token a previous call's refusal handed you, sent back after you read
    /// its candidates and judged them a different entity. It lifts only the
    /// refusal that minted it — a token you made up, or one from another
    /// refusal, lifts nothing — and it never overrides an exact handle
    /// collision.
    #[serde(default)]
    pub(crate) override_token: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// **A deliberate, visible act**: this verb's own refusal is allowed to
/// promise an override because this impl says its `Args` really carries
/// one. See [`AcceptsOverride`].
impl AcceptsOverride for AddEntityArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
}

impl Jojobot {
    /// The box that comes with a bot, opened in the same act that creates it.
    ///
    /// **Not a side effect, and not a mint.** [`Rule 18`] forbids bringing a
    /// thing into being as a consequence of doing something else — and this is
    /// not that, because the box is not a second thing. An identity that cannot
    /// be written to is not an identity; the box is part of what a bot IS, the
    /// way its handle is. What the rule forbids is a box appearing behind a
    /// caller's back, and there is no back to appear behind here: the caller
    /// asked for a bot, and this is what a bot is made of.
    ///
    /// The name is the handle, so nothing is chosen and nothing can drift. The
    /// screen that would have guarded a box name has already run on the handle,
    /// one layer up, against the entity roster — which is the same screen doing
    /// the same job once instead of twice.
    ///
    /// [`Rule 18`]: creation is an intentional act.
    async fn open_box_with(&self, entity: &Entity) -> Vec<(&'static str, serde_json::Value)> {
        if entity.id.kind() != Some(EntityKind::BOT) {
            return Vec::new();
        }
        let name = MailboxName(entity.id.slug().to_string());
        // No token, and none is needed: the box name IS the owner's handle, so
        // the mailbox guard waives its SIMILARITY screen on that ground alone,
        // and never an exact collision. A near-miss box name is a near-miss bot
        // handle, and that was already screened against the roster before this
        // bot existed — re-screening the same string against a different list
        // would block `bot:gamma` for a box called `gamma-2` that the operator
        // deliberately named.
        match self.mailboxes.create_mailbox(&name, &entity.id, None).await {
            Ok(mailbox::Guarded::Written(opened)) => {
                vec![("mailbox", opened.name.as_str().into())]
            }
            // **The identity is incomplete, and it says so rather than reading
            // as whole.** The bot is written and there is no verb that deletes
            // it, so this cannot be rolled back into a clean refusal — the
            // honest answer is the write that happened plus the part that did
            // not.
            other => vec![
                ("mailbox", serde_json::Value::Null),
                (
                    "mailbox_note",
                    format!(
                        "THIS IDENTITY IS INCOMPLETE: the bot exists, but its box '{}' could not \
                     be opened, so it cannot receive mail and nothing can be posted to it. \
                     Tell the operator — repairing it takes a person.{}",
                        name.as_str(),
                        match &other {
                            Err(err) => format!(" The mailbox world said: {err}"),
                            _ => String::new(),
                        }
                    )
                    .into(),
                ),
            ],
        }
    }
}

/// Create an entity of any kind. Screened by the write guard, so a handle
/// or name that looks like one jojobot already knows comes back as
/// candidates instead of a second record.
#[tool_router(router = add_entity_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Bring a new entity into existence — the required first step before any \
                       other write may name it. The kinds are the `kind` argument's, which is \
                       the one place they are listed; creating a bot is creating an identity, \
                       and it opens the mailbox that bot owns in the same act. Send `sets` \
                       to set keys on the new thing in the same call: they are written as \
                       its first claim, under every guard a capture faces, and a key that is \
                       refused refuses the whole call and creates nothing. The \
                       claim is inference: a field that needs the operator's own word \
                       (testimony), or one you read in a system of record (observation), is \
                       written with capture. Returns the stored entity; with sets it also returns \
                       first_claim, the claim written with it. If its handle or any of its names \
                       resembles something jojobot already knows, NOTHING is written: the \
                       result says status: blocked with candidates and how_to_proceed. Use the \
                       candidate you meant, or re-call with the override_token that refusal \
                       carries if this genuinely is a different thing sharing a name — a token \
                       lifts the one refusal that minted it and no other. An exact handle \
                       collision can never be forced — a handle has exactly one owner."
    )]
    pub(crate) async fn add_entity(
        &self,
        Parameters(args): Parameters<AddEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let id = entity_id(&args.kind, &args.handle)?;
        // Kept for the refusal below: which handle the guard turned back is
        // what says whether it was this entity or the one it named as parent.
        let creating = id.clone();
        // **A new bot grows every other bot's snapshot**, which is a write that
        // is never refused: a colleague is not a mistake. What it can do is
        // take a bot's boot over the ceiling, so each bot's floor is read now
        // and again once the bot is there.
        let floors_before = match creating.kind() == Some(EntityKind::BOT) {
            true => Some(self.boot_floors_of_bots().await),
            false => None,
        };
        // Taken before `args`' fields are moved into `new` below — this
        // owns its own copy, so it survives the moves that follow.
        let token_slot = TokenSlot::from(&args);
        // **What the first claim says**, when there is one: the thing's own
        // name, as it will be stored. A claim needs words and the caller sent
        // fields, so the name is what a fields-only claim about a new thing says.
        let claim_words = args.name.trim().to_string();
        let new = NewEntity {
            id,
            name: args.name,
            aliases: args.aliases.unwrap_or_default(),
            source: args.source,
            crm: args.crm,
            // **Where a thing sits is the caller's to say.** A tree the domain
            // holds and the door does not carry makes every write through this
            // verb a root, so a kind that requires a parent has no reachable
            // way to be created at all.
            parent: args.parent.as_deref().map(EntityId::person),
            boot: parse_boot(args.boot.as_deref())?,
            override_token: args.override_token.clone(),
        };
        // Routed through the declined path rather than straight to the mapper,
        // for the reason capture is: an entity the validators refuse is a
        // caller mistake and comes back as an answer (rule 68).
        // **The first fields, when the caller sent any, are written as the thing's
        // first claim in the same act**, through the store's own combined write:
        // the store runs the guards of a capture inside the transaction that
        // creates the thing, and a refusal takes the creation back with it.
        let first_fields = args.sets.clone().filter(|sets| !sets.is_empty());
        let mut fold_behind = None;
        let (added, first_claim) = match first_fields {
            None => match self.memory.add_entity(new).await {
                Ok(added) => (added, None),
                Err(e) => return memory_declined("add_entity", e),
            },
            Some(mut fields) => {
                // **The checks a capture makes before it writes**, on what the
                // caller sent: a role's own two fields are the boot door's, and
                // the stored due moment is jojobot's own.
                if let Some(refused) = jojobot_domain::memory::refuses_role_fields(fields.keys()) {
                    return memory_declined("add_entity", refused);
                }
                // **A guarded key is licensed here as on a capture**, through
                // the same call: the store's combined write takes no caller, so
                // this is the only place a creation's `sets` is judged.
                if let Some(refused) = self
                    .refuses_an_unlicensed_key("add_entity", &creating, false, &caller.bot, &fields)
                    .await?
                {
                    return Ok(refused);
                }
                if let Some(refused) = self
                    .refuses_a_hand_written_due_moment(&creating, &fields, &[])
                    .await
                {
                    return Ok(refused);
                }
                // **Worked out here too**: a loop made with its cadence is whole
                // from the moment it is made, with the day it falls due beside
                // the keys that set it.
                let (due_on, _) = self.moved_due_moment(&creating, &fields, &[]).await;
                if let jojobot_domain::attention::DueMove::Set(due_on) = due_on {
                    fields.insert(
                        jojobot_domain::attention::DUE_ON.to_string(),
                        due_on.to_string(),
                    );
                }
                let recorded_at = self.dated(None, args.sid.as_deref()).await?;
                let first = jojobot_domain::memory::NewFact {
                    fields,
                    // **The session that made it, as a capture records it**: a
                    // claim's first write names the run that wrote it, and a
                    // later rewrite of the claim is judged against that run.
                    session: Some(caller.sid.as_str().to_string()),
                    ..jojobot_domain::memory::NewFact::about(
                        creating.clone(),
                        claim_words,
                        recorded_at,
                    )
                };
                // **A star or a seat count that would take a bot's boot over its
                // ceiling is refused before anything lands**, as on a capture.
                //
                // **The bot does not exist yet, so it is measured as it will
                // exist** once the creation commits: the same check a capture
                // makes, with the entity the boot would name.
                let creating = Entity {
                    id: new.id.clone(),
                    merged_into: None,
                    kind: new.id.kind().unwrap_or(EntityKind::THING),
                    name: new.name.clone(),
                    aliases: new.aliases.clone(),
                    source: new.source.clone(),
                    crm: new.crm.clone(),
                    parent: new.parent.clone(),
                    boot: new.boot,
                    badge: None,
                    archived: None,
                };
                if let Some(refused) = self
                    .refuses_a_boot_floor_for_creation(&first, &creating, &caller.bot)
                    .await
                {
                    return memory_declined("add_entity", refused);
                }
                match self.memory.add_entity_with_first_claim(new, first).await {
                    Ok(Guarded::Written((entity, fact))) => (Guarded::Written(entity), Some(fact)),
                    Ok(Guarded::Blocked {
                        attempted,
                        candidates,
                    }) => (
                        Guarded::Blocked {
                            attempted,
                            candidates,
                        },
                        None,
                    ),
                    // **A write that landed is never reported as failed**
                    // (rule 130), the same as on a capture.
                    Err(MemoryError::FoldBehind {
                        landed: Landed::Creation(created),
                        behind,
                        ..
                    }) => {
                        let (entity, fact) = *created;
                        fold_behind = Some(behind);
                        (Guarded::Written(entity), Some(fact))
                    }
                    Err(e) => return memory_declined("add_entity", e),
                }
            }
        };
        match added {
            Guarded::Written(entity) => {
                self.beat("add_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
                let mut body = entity_json(&entity);
                // **The first claim's receipt**, so the claim has an address a
                // later edit goes through, and the answer says the claim exists.
                if let (Some(fact), Some(obj)) = (&first_claim, body.as_object_mut()) {
                    obj.insert(
                        "first_claim".into(),
                        fact_receipt_json(fact, self.dated(None, args.sid.as_deref()).await?),
                    );
                }
                if let Some(behind) = fold_behind {
                    crate::answer::note_fold_behind(&mut body, behind);
                }
                if let Some(obj) = body.as_object_mut() {
                    for (key, value) in self.open_box_with(&entity).await {
                        obj.insert(key.into(), value);
                    }
                }
                if let Some(before) = floors_before {
                    let budget = jojobot_domain::text::BOOT_ANSWER.budget;
                    let pushed: Vec<serde_json::Value> = self
                        .boot_floors_of_bots()
                        .await
                        .into_iter()
                        .filter(|(bot, now)| {
                            *now > budget
                                && before
                                    .iter()
                                    .any(|(then, was)| then == bot && *was <= budget)
                        })
                        .map(|(bot, now)| {
                            serde_json::json!({
                                "bot": bot.as_str(),
                                "floor": now,
                                "over": now - budget,
                            })
                        })
                        .collect();
                    if !pushed.is_empty()
                        && let Some(obj) = body.as_object_mut()
                    {
                        obj.insert("pushes_over".into(), pushed.into());
                        obj.insert(
                            "pushes_over_note".into(),
                            "this bot is created and stays. Each bot named carries more than a \
                             boot may now, so its next write that grows its charter, a starred \
                             rule or its seats is refused until its text is cut. Nothing was \
                             trimmed."
                                .into(),
                        );
                    }
                }
                // **The parent, not the handle.** This verb composes the
                // handle out of `kind` and the slug half, which is what
                // those arguments say they are. The parent is read as a
                // person when it names no kind, exactly as a capture's
                // subject is, and that is the substitution worth naming.
                crate::answer::note_delta(
                    &mut body,
                    crate::answer::Difference::between(
                        "parent",
                        args.parent.as_deref(),
                        entity.parent.as_ref().map(EntityId::as_str).unwrap_or(""),
                    )
                    .into_iter()
                    .collect(),
                );
                // **The teaching that lands before the mistake, not after
                // it.** `capture` cannot be the one to teach this — a rhythm
                // must exist before anything can be captured on it, and the
                // caller who reaches for a hand-typed schedule does so on
                // the very first capture that follows. This is that
                // capture's only chance to have already been told.
                if entity.id.kind() == Some(EntityKind::RHYTHM)
                    && self
                        .first_contact(RHYTHM_HISTORY_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, RHYTHM_HISTORY_TEACHING);
                }
                // **Shares its domain with `capture`'s line about a project**,
                // so whichever write about a project comes first is the one
                // that names the skill, and never both.
                if crate::teaching::is_project_work(entity.id.kind())
                    && self
                        .first_contact(PROJECTS_SKILL_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, PROJECTS_SKILL_TEACHING);
                }
                // **Checked before the gate, never after** — `first_contact`
                // has a side effect, and an ordinary creation must not spend
                // the one teaching a forced one is owed.
                if args.override_token.is_some()
                    && self
                        .first_contact(DUPLICATE_REPAIR_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, DUPLICATE_REPAIR_TEACHING);
                }
                // **A creation that sits beside a thing this session was shown
                // is asked about it, once.** Only a creation that landed gets
                // here, so a guard refusal never doubles up with this.
                if let Some(sid) = args.sid.as_deref()
                    && let Some(noticed) = self.registry.consult_creation(
                        sid,
                        entity.id.as_str(),
                        entity.id.kind().map_or("", |kind| kind.as_token()),
                        &entity.name,
                        &entity.aliases,
                        entity.parent.as_ref().map(|parent| parent.as_str()),
                    )
                {
                    crate::answer::note_teaching(&mut body, &noticed.question(entity.id.as_str()));
                }
                json_result(&body)
            }
            // **A parent refusal is not a near miss, and saying it is offers a
            // way forward that leads back to the same wall.** Neither of these
            // is overridable: a parent must already exist, because nothing is
            // created as a side effect of creating something else, and nothing
            // is its own parent. The near-miss sentence tells a caller to
            // re-call with a token, and a caller who does is refused again and
            // minted another. Rule 68 is about a way FORWARD.
            Guarded::Blocked {
                attempted,
                candidates,
            } if candidates
                .iter()
                .any(|c| c.reason == guard::MatchReason::SelfParent) =>
            {
                Ok(blocked_body(
                    &attempted,
                    &candidates,
                    format!(
                        "Nothing was written. '{attempted}' names itself as its parent, and \
                         nothing is its own parent. No override_token lifts this. Name the \
                         entity this one sits under, or leave parent off and create it as a \
                         root."
                    ),
                ))
            }
            // **A handle a field of the first claim named** is not the parent, and
            // saying it is sends a caller to drop a parent they never sent.
            Guarded::Blocked {
                attempted,
                candidates,
            } if attempted != creating && args.parent.as_deref() != Some(attempted.as_str()) => {
                Ok(blocked_body(
                    &attempted,
                    &candidates,
                    format!(
                        "Nothing was written. A field of the first claim names '{attempted}', \
                         and it is not an entity jojobot knows. No override_token lifts this: \
                         nothing is created as a side effect of creating something else. Create \
                         '{attempted}' with its own add_entity call first, then re-call this \
                         one — or leave that field off. '{creating}' was not created."
                    ),
                ))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } if attempted != creating => Ok(blocked_body(
                &attempted,
                &candidates,
                format!(
                    "Nothing was written. '{attempted}' is the parent this call named, and it \
                     is not an entity jojobot knows. No override_token lifts this: nothing is \
                     created as a side effect of creating something else. Create '{attempted}' \
                     with its own add_entity call first, then re-call this one — or drop the \
                     parent to create '{creating}' as a root."
                ),
            )),
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::Creating(token_slot),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

    /// **The description of what a creation returns names the first claim.**
    /// With `sets` the verb writes a first claim in the same act and returns it
    /// beside the entity, so a sentence that says only "the stored entity" leaves
    /// out half of the answer. The needle is the key the answer carries, found in
    /// the sentence that says what is returned.
    #[test]
    fn the_description_of_what_is_returned_names_the_first_claim() {
        let tools = Jojobot::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "add_entity")
            .expect("add_entity is served");
        let description = tool.description.as_deref().unwrap_or_default();
        let at = description
            .find("Returns the stored entity")
            .expect("the description says what is returned");
        let sentence = description[at..].split(". ").next().expect("a sentence");
        assert!(
            sentence.contains("first_claim") && sentence.contains("sets"),
            "what is returned does not name the first claim and what makes it: {sentence}"
        );
    }

    /// **The `kind` description names the keys a `work` or `project` carries**,
    /// beside what it says of `promise` and `rhythm`, so a session choosing a
    /// kind learns the keys before it writes. Read off the published schema and
    /// the engine's declaration, never a second copy of either.
    #[test]
    fn the_kind_description_names_the_keys_a_work_or_project_carries() {
        use jojobot_domain::memory::kinds;
        let schema = serde_json::to_value(schemars::schema_for!(AddEntityArgs))
            .expect("the schema serialises");
        let kind = schema["properties"]["kind"]["description"]
            .as_str()
            .expect("kind carries a description");
        let declared: Vec<String> = kinds::keys_of("work").into_iter().map(|f| f.key).collect();
        for key in ["status", "owner", "depends_on", "waiting_on"] {
            assert!(
                declared.iter().any(|d| d == key),
                "the engine no longer declares `{key}` on a piece of work"
            );
            assert!(
                kind.contains(&format!("`{key}`")),
                "the kind description does not name the work key `{key}`"
            );
        }
        assert!(
            kind.contains(&format!("`{}`", kinds::COLUMNS)),
            "the kind description does not name the key a project lists its columns under"
        );
    }

    /// **A `boot` token jojobot does not know is a client error, never a silent
    /// default.**
    ///
    /// This field spent a release wearing the DELETED `mailbox` parameter's
    /// description — "the mailbox this entity owns… the box need not exist
    /// yet" — so an agent reading the schema would pass a box name here
    /// intending to claim one. `Boot::from_token` maps anything but the exact
    /// `always` to `on-demand`, so the value vanished: no error, no blocked
    /// answer, the wrong boot tier written, and a caller believing it had
    /// claimed a box.
    ///
    /// A token that is no boot tier is a malformed call, exactly as a token
    /// that is no kind or no status is — the line the orientation essay draws
    /// between an error and a blocked answer.
    #[tokio::test]
    async fn an_unknown_boot_token_is_a_client_error_rather_than_a_silent_default() {
        let jojobot = handler();
        let err = jojobot
            .add_entity(Parameters(AddEntityArgs {
                boot: Some("gamma-inbox".into()),
                ..add_args("bot", "gamma", "Gamma")
            }))
            .await
            .expect_err("a token that is no boot tier is a malformed call");
        assert!(
            err.message.contains("boot") && err.message.contains("always"),
            "the error names the field and the tokens it takes: {}",
            err.message
        );

        // The two it does take still work, and `always` is not swallowed.
        // Distinct handles AND distinct names: two entities called "Alpha"
        // trip the name screen, which would answer blocked and prove nothing.
        for (handle, name, token) in [
            ("alpha-one", "Alpha One", "always"),
            ("alpha-two", "Alpha Two", "on-demand"),
        ] {
            let body = json_of(
                &jojobot
                    .add_entity(Parameters(AddEntityArgs {
                        boot: Some(token.into()),
                        ..add_args("person", handle, name)
                    }))
                    .await
                    .expect("a known token is accepted"),
            );
            assert_eq!(body["boot"], token, "{token} round-trips");
        }
    }

    use crate::memory::testing::*;

    /// 🚨 **A bare PARENT is read as a person, and the receipt says so.**
    ///
    /// ⚠️ **The parent, not the handle.** This verb takes `kind` and the slug
    /// half separately and composes them, which is what the argument says it
    /// does — announcing that on every call would be the noise a reader learns
    /// to skip. The parent is different: it is read as a person when it names
    /// no kind, exactly as `capture`'s subject is, and `capture` announces it.
    ///
    /// **Paired with a parent that needed no reading**, and with no reason
    /// given, for the reasons the sibling case on `update_entity` gives.
    #[tokio::test]
    async fn a_bare_parent_is_reported_as_read_and_a_qualified_one_is_not() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:alpha").await;

        let bare = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("alpha".into()),
                    sid: Some(sid.clone()),
                    ..add_args("thing", "red-kite", "Red Kite")
                }))
                .await
                .expect("add_entity ok"),
        );
        assert_eq!(bare["parent"], "person:alpha", "{bare}");
        assert_eq!(bare["delta"][0]["field"], "parent", "{bare}");
        assert_eq!(bare["delta"][0]["sent"], "alpha", "{bare}");
        assert_eq!(bare["delta"][0]["stored"], "person:alpha", "{bare}");
        assert_eq!(
            bare["delta"][0]["because"],
            serde_json::Value::Null,
            "a difference with nothing to explain carries no explanation: {bare}",
        );

        let qualified = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("person:alpha".into()),
                    sid: Some(sid.clone()),
                    ..add_args("thing", "blue-kite", "Blue Kite")
                }))
                .await
                .expect("add_entity ok"),
        );
        assert_eq!(qualified["parent"], "person:alpha", "{qualified}");
        assert!(
            qualified.get("delta").is_none(),
            "a parent that needed no reading carries no delta: {qualified}",
        );

        // The positive that stops both halves passing on a verb that stopped
        // reporting parents at all: a call naming none says so and carries no
        // delta either.
        let rootless = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid),
                    ..add_args("thing", "green-kite", "Green Kite")
                }))
                .await
                .expect("add_entity ok"),
        );
        assert_eq!(rootless["parent"], serde_json::Value::Null, "{rootless}");
        assert!(rootless.get("delta").is_none(), "{rootless}");
    }

    /// `add_entity` creates any kind, and `list_entities` reads it back — the
    /// two halves of the entity surface, through the MCP path.
    #[tokio::test]
    async fn add_entity_then_list_entities_through_the_handler() {
        let jojobot = handler();
        let added = jojobot
            .add_entity(Parameters(AddEntityArgs {
                crm: Some("card:874".into()),
                ..add_args("project", "atlas", "Atlas")
            }))
            .await
            .expect("add ok");
        let body = json_of(&added);
        assert_eq!(
            body["id"], "project:atlas",
            "the handle keeps its lowercase kind token"
        );
        assert_eq!(
            body["type"], "Project",
            "responses name the type, schema.org-flavored"
        );
        assert_eq!(body["crm"], "card:874");

        let listed = jojobot
            .list_entities(Parameters(ListEntitiesArgs {
                kind: Some("project".into()),
                parent: None,
                sid: None,
            }))
            .await
            .expect("list ok");
        let body = json_of(&listed);
        assert_eq!(body["entities"][0]["id"], "project:atlas");
        assert_eq!(body["count"], 1);
    }

    /// **A caller can put an entity under another one, and a rhythm needs it.**
    ///
    /// The tree shipped in the domain and stopped at the door: every write
    /// through this verb was a root, so nothing a caller could send made a
    /// child. A kind that requires a parent then had no reachable way to be
    /// created at all, and every unit test below the door would still be green.
    ///
    /// Both halves. The refusal alone passes on a build where no rhythm can
    /// ever be written, and the child alone passes on a build where the
    /// requirement does nothing.
    #[tokio::test]
    async fn a_caller_can_name_a_parent_and_a_rhythm_is_refused_without_one() {
        let jojobot = handler();
        ensure(&jojobot, "thing:kettle").await;

        let refused = json_of(
            &jojobot
                .add_entity(Parameters(add_args("rhythm", "descale", "Descale")))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked");
        assert_eq!(refused["wrote"], false);
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .expect("a blocked answer says how to proceed")
                .contains("parent"),
            "the way forward names the argument that repairs it: {refused}",
        );

        let added = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("thing:kettle".into()),
                    ..add_args("rhythm", "descale", "Descale")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(added["id"], "rhythm:descale");
        assert_eq!(
            added["parent"], "thing:kettle",
            "the parent survives the door and comes back on the entity: {added}",
        );
    }

    /// How a refusal HANDS OVER a token, which is the thing a caller can act
    /// on — as against merely naming the argument to say no token applies.
    const OFFERS_A_TOKEN: &str = "override_token: \"";

    /// **A refusal about the PARENT says what repairs it, and does not offer a
    /// token that cannot lift it.**
    ///
    /// Both parent refusals wore the near-miss sentence, which tells a caller
    /// to re-call with an `override_token` when the handle is genuinely a
    /// different thing sharing a name. Neither refusal is overridable: a parent
    /// must exist, because nothing is created as a side effect of creating
    /// something else, and nothing is its own parent ever. A caller following
    /// that advice is refused again, is minted another token, and can repeat it
    /// for ever — a way forward that leads back to the same wall is rule 68
    /// failing while appearing to hold.
    ///
    /// Paired with the positive: the child's OWN handle resembling something
    /// that exists is the near miss, and that one is overridable and still
    /// says so.
    #[tokio::test]
    async fn a_parent_refusal_does_not_offer_a_token_that_cannot_lift_it() {
        let jojobot = handler();
        ensure(&jojobot, "thing:kettle").await;

        let missing = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("thing:no-such-bike".into()),
                    ..add_args("thing", "tau", "Tau")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(missing["status"], "blocked");
        assert_eq!(missing["attempted"], "thing:no-such-bike");
        let how = missing["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            !how.contains(OFFERS_A_TOKEN),
            "no token lifts a parent that does not exist: {how}",
        );
        assert!(
            how.contains("add_entity"),
            "and the repair is to create it first: {how}",
        );
        // Nothing resembles it, which is what makes the token minted below the
        // one this refusal would have carried rather than some other token.
        assert_eq!(
            missing["candidates"].as_array().map(Vec::len),
            Some(0),
            "no candidate resembles it: {missing}",
        );

        let itself = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("thing:sigma".into()),
                    ..add_args("thing", "sigma", "Sigma")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(itself["status"], "blocked");
        let how = itself["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            !how.contains(OFFERS_A_TOKEN),
            "nothing is its own parent, and no token changes that: {how}",
        );

        // **And a token does not lift it, which is what the old advice sent a
        // caller to find out.** The near-miss sentence told them to re-call
        // with one; doing that is refused again, so the loop the advice opened
        // is pinned shut here rather than only described.
        let forced = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("thing:no-such-bike".into()),
                    override_token: Some(guard::override_token(
                        &EntityId::person("thing:no-such-bike"),
                        &[],
                    )),
                    ..add_args("thing", "tau", "Tau")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(
            forced["status"], "blocked",
            "a token cannot create the parent it names: {forced}",
        );

        // The positive: a near miss on the child's own handle is the refusal
        // that IS overridable, and it still offers the token. Without this the
        // two above pass on a build that never offers one at all.
        let near_miss = json_of(
            &jojobot
                .add_entity(Parameters(add_args("thing", "kettl", "Kettl")))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(near_miss["status"], "blocked");
        let offered = near_miss["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            offered.contains(OFFERS_A_TOKEN),
            "a near miss on the handle being created is lifted by a token: {near_miss}",
        );

        // **The control the two refusals above rest on: a token works.**
        // Without it, "blocked with a token" proves nothing — the same result
        // comes back on a build where no token lifts anything at all.
        let token = offered
            .split_once(OFFERS_A_TOKEN)
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the advice hands over the token it mints")
            .0
            .to_string();
        let forced = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    override_token: Some(token),
                    ..add_args("thing", "kettl", "Kettl")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(
            forced["id"], "thing:kettl",
            "the token this refusal minted lifts this refusal: {forced}",
        );
    }

    /// An unknown kind is a client error that names the closed set, rather than
    /// a record filed under a noun nobody chose.
    #[tokio::test]
    async fn an_unknown_kind_is_a_client_error() {
        let err = handler()
            .add_entity(Parameters(add_args(
                "receipt",
                "some-slug",
                "An unknown kind",
            )))
            .await
            .expect_err("must reject an unknown kind");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("person"),
            "the error must name the kinds: {}",
            err.message
        );
    }

    /// A guarded write comes back as a **successful** result whose body says
    /// nothing was written. "Needs confirmation" is an answer — the guard did its
    /// job and is handing the decision over — not an exception; delivering it as
    /// a protocol error made a working feature look like a broken server, and
    /// clients that retry or unwrap on error handle it exactly wrong.
    #[tokio::test]
    async fn a_blocked_add_returns_the_candidates_in_a_successful_result() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("first add ok");

        let result = jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha Two")))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:alpha");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        assert_eq!(body["candidates"][0]["reason"], "exact-handle");
        assert_eq!(body["candidates"][0]["source"], "user-named");

        // And nothing was written.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    parent: None,
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["entities"][0]["name"], "Alpha");
    }

    /// **A box is what having an identity MEANS, not a thing you go and make.**
    ///
    /// The operator's ruling, and they gave it twice — once as "an unowned mailbox
    /// should not be creatable at all", and then, when the answer to that was a
    /// mint verb that takes an owner, again and harder: *"it makes no sense for
    /// us to be able to create mailboxes because there's nothing to attach them
    /// to… there should be no ownerless mailboxes and no bot without a
    /// mailbox."* Both directions of one invariant, and a separate verb can only
    /// break them: every call of it is a chance to open a box for nobody, and
    /// every bot stood up without calling it is an identity that cannot receive
    /// mail.
    ///
    /// So there is no mint. A bot's box opens with the bot, in the same act.
    #[tokio::test]
    async fn standing_up_a_bot_opens_its_box_in_the_same_act() {
        let jojobot = handler();
        let created = json_of(
            &jojobot
                .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
                .await
                .expect("add_entity call ok"),
        );
        assert_ne!(
            created["status"], "blocked",
            "the bot is created: {created}"
        );

        let boxes = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list_mailboxes ok");
        let opened = boxes
            .iter()
            .find(|b| b.name.as_str() == "gamma")
            .unwrap_or_else(|| panic!("the bot's box was never opened: {boxes:?}"));
        assert_eq!(
            opened.owner,
            EntityId("bot:gamma".into()),
            "…and it is the bot's own, by construction"
        );
    }

    /// **The name is DERIVED, never stored.** Every bot on the live server
    /// already had a box named for its handle — five for five — so the
    /// `mailbox:` field was a second copy of the handle in every real case, and
    /// a second copy is the thing this codebase names as a disease everywhere
    /// else. Derived, the two cannot drift, and there is no string left for a
    /// caller to pass, mistype, or point at somebody else's box.
    #[tokio::test]
    async fn the_box_is_named_for_its_bot_and_the_answer_says_so() {
        let jojobot = handler();
        let created = json_of(
            &jojobot
                .add_entity(Parameters(add_args("bot", "sigma", "Sigma")))
                .await
                .expect("add_entity call ok"),
        );
        assert_eq!(
            created["mailbox"], "sigma",
            "the creating call says which box it opened: {created}"
        );

        // …and the boot door agrees with it, because both read the same world.
        let booted = boot(&jojobot, "sigma").await;
        assert_eq!(booted["identity"]["owned_mailbox"]["name"], "sigma");
    }

    /// A bot is the only kind that gets one: a person is not an addressee.
    #[tokio::test]
    async fn standing_up_anything_but_a_bot_opens_no_box() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "milhouse", "Milhouse")))
            .await
            .expect("add_entity call ok");
        assert!(
            jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list_mailboxes ok")
                .is_empty(),
            "only an identity that can be written to gets a box"
        );
    }

    /// **The token as a caller gets it: out of the refusal's own advice.**
    /// Sixteen hex digits is its shape, so this survives the advice being
    /// reworded.
    fn token_in(advice: &str) -> String {
        let chars: Vec<char> = advice.chars().collect();
        chars
            .windows(16)
            .find(|w| w.iter().all(char::is_ascii_hexdigit))
            .map(|w| w.iter().collect())
            .unwrap_or_else(|| panic!("the refusal must carry the token it minted: {advice}"))
    }

    /// Create `handle` named `name` past the near-miss refusal, the way a
    /// caller does: refused first, then the same call with the token it was
    /// handed. Returns the receipt of the creation that landed.
    async fn forced_past_a_near_miss(
        jojobot: &Jojobot,
        sid: &str,
        handle: &str,
        name: &str,
    ) -> serde_json::Value {
        let refused = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.into()),
                    ..add_args("place", handle, name)
                }))
                .await
                .expect("add_entity ok"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        let token = token_in(refused["how_to_proceed"].as_str().expect("advice"));
        json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    override_token: Some(token),
                    sid: Some(sid.into()),
                    ..add_args("place", handle, name)
                }))
                .await
                .expect("add_entity ok"),
        )
    }

    /// Whether a receipt carries a teaching that names `needle`.
    fn teaches(receipt: &serde_json::Value, needle: &str) -> bool {
        receipt["teaching"].as_array().is_some_and(|all| {
            all.iter()
                .any(|t| t.as_str().is_some_and(|t| t.contains(needle)))
        })
    }

    /// **A creation forced past a near-miss says what repairs a duplicate,
    /// once per session, and only that creation says it.**
    ///
    /// The moment a deliberate near-duplicate is made is the one moment the
    /// repair can be named before it is needed. Three claims in one run, so
    /// each is the positive the others rest on: an ordinary creation says
    /// nothing and spends nothing; the first forced creation teaches
    /// `merge_entities` and the `archive_entity` limit; the second forced
    /// creation in the same session teaches nothing.
    #[tokio::test]
    async fn a_creation_forced_past_a_near_miss_teaches_the_duplicate_repair_once() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let plain = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    ..add_args("place", "leftorium", "Leftorium")
                }))
                .await
                .expect("add_entity ok"),
        );
        assert!(
            !teaches(&plain, "merge_entities"),
            "an ordinary creation made a duplicate-repair teaching: {plain}",
        );

        let first = forced_past_a_near_miss(&jojobot, &sid, "wharf-road", "Leftorium").await;
        assert_eq!(
            first["id"], "place:wharf-road",
            "the forced creation landed: {first}"
        );
        assert!(
            teaches(&first, "merge_entities") && teaches(&first, "archive_entity"),
            "the first forced creation names the repair and the archive limit: {first}",
        );

        let second = forced_past_a_near_miss(&jojobot, &sid, "wonder-wharf", "Leftorium").await;
        assert_eq!(second["id"], "place:wonder-wharf", "{second}");
        assert!(
            !teaches(&second, "merge_entities"),
            "the teaching fired a second time in one session: {second}",
        );
    }

    /// **The duplicate-repair teaching says what a recall of the merged-away
    /// handle returns now: the `merged` status naming the survivor in
    /// `merged_into`.** A recall of that handle holds nothing and answers as
    /// nothing, so a teaching that says the handle "forwards" sends a caller to
    /// expect the survivor's claims where there are none.
    #[tokio::test]
    async fn the_duplicate_repair_teaching_says_the_merged_handle_names_its_survivor() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        created(&jojobot, &sid, "place", "leftorium", "Leftorium").await;
        let first = forced_past_a_near_miss(&jojobot, &sid, "wharf-road", "Leftorium").await;
        assert!(
            teaches(&first, "merged_into"),
            "the teaching does not say the merged-away handle names its survivor: {first}",
        );
        assert!(
            !teaches(&first, "forwards"),
            "the teaching still says the merged-away handle forwards: {first}",
        );
    }

    /// Read a kind's things through the verb a caller uses, which is what shows
    /// them to the session.
    async fn browsed(jojobot: &Jojobot, sid: &str, kind: &str) {
        jojobot
            .list_entities(Parameters(crate::memory::list_entities::ListEntitiesArgs {
                kind: Some(kind.into()),
                parent: None,
                sid: Some(sid.into()),
            }))
            .await
            .expect("list_entities ok");
    }

    /// Create a place named `name` under `handle`, as the session `sid`.
    async fn created(
        jojobot: &Jojobot,
        sid: &str,
        kind: &str,
        handle: &str,
        name: &str,
    ) -> serde_json::Value {
        json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.into()),
                    ..add_args(kind, handle, name)
                }))
                .await
                .expect("add_entity ok"),
        )
    }

    /// **A creation beside a thing this session was just shown is asked about
    /// it, once.**
    ///
    /// The guard compares a creation with the store and cannot see that the
    /// thing sits beside one a read returned a few calls ago. The question names
    /// the thing shown, the fold that would repair it, and what archive would
    /// leave behind. Every other claim rides in the same run, so each is the
    /// positive another rests on: nothing shown is asked nothing; a different
    /// kind is asked nothing; the same thing is not asked about twice.
    #[tokio::test]
    async fn a_creation_beside_a_thing_the_session_was_shown_is_asked_about_it_once() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let seeded = created(&jojobot, &sid, "place", "wonder-wharf", "Wonder Wharf").await;
        assert!(!teaches(&seeded, "same thing"), "{seeded}");

        // Nothing has been shown to this session yet, so a creation with a
        // shared word is asked nothing.
        let unshown = created(&jojobot, &sid, "place", "leftorium", "Wonder Leftorium").await;
        assert!(!teaches(&unshown, "place:wonder-wharf"), "{unshown}");

        browsed(&jojobot, &sid, "place").await;

        // A different kind sharing the word is asked nothing.
        let other_kind = created(&jojobot, &sid, "thing", "kettle", "Wharf Kettle").await;
        assert!(!teaches(&other_kind, "place:wonder-wharf"), "{other_kind}");

        // The same kind, a shared word, and the thing was shown: asked.
        let asked = created(&jojobot, &sid, "place", "wharf-road", "Wharf Road").await;
        assert!(
            teaches(&asked, "place:wonder-wharf")
                && teaches(&asked, "merge_entities")
                && teaches(&asked, "archive_entity"),
            "the creation beside a thing it was shown is not asked about it: {asked}",
        );

        // Asked about once: a second creation beside the same thing is not.
        let again = created(&jojobot, &sid, "place", "wharf-lane", "Wharf Lane").await;
        assert!(
            !teaches(&again, "place:wonder-wharf"),
            "the same thing was asked about twice: {again}",
        );
    }

    /// Create a thing under `parent`, as the session `sid`.
    async fn created_under(
        jojobot: &Jojobot,
        sid: &str,
        (kind, handle, name): (&str, &str, &str),
        parent: Option<&str>,
    ) -> serde_json::Value {
        json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.into()),
                    parent: parent.map(str::to_string),
                    ..add_args(kind, handle, name)
                }))
                .await
                .expect("add_entity ok"),
        )
    }

    /// A session that created a project and one thing under it, then read the
    /// things back, which is what shows the child to the session.
    async fn shown_a_child_of_atlas() -> (Jojobot, String) {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        created(&jojobot, &sid, "project", "atlas", "Atlas").await;
        created_under(
            &jojobot,
            &sid,
            ("work", "atlas-x-donut-stand", "X Donut Stand"),
            Some("project:atlas"),
        )
        .await;
        browsed(&jojobot, &sid, "work").await;
        (jojobot, sid)
    }

    /// **A creation beside a sibling is not asked about the parent's own
    /// word, and is still asked about any other word they share.** Each branch
    /// runs on a session of its own, because a thing is asked about once and the
    /// branches would spend it for one another. The case travels the verb a
    /// caller uses, so a parent that is read and never handed to the question
    /// leaves the first branch asked and fails it.
    #[tokio::test]
    async fn a_creation_beside_a_sibling_is_not_asked_about_the_parents_own_word() {
        // The only word they share is the project's: not asked.
        let (jojobot, sid) = shown_a_child_of_atlas().await;
        let quiet = created_under(
            &jojobot,
            &sid,
            ("work", "atlas-kwik-e-mart", "Kwik E Mart"),
            Some("project:atlas"),
        )
        .await;
        assert_eq!(quiet["id"], "work:atlas-kwik-e-mart", "it landed: {quiet}");
        assert!(
            !teaches(&quiet, "work:atlas-x-donut-stand"),
            "the parent's own word was asked about: {quiet}",
        );

        // A sibling sharing a word the project does not give: asked.
        let (jojobot, sid) = shown_a_child_of_atlas().await;
        let asked = created_under(
            &jojobot,
            &sid,
            ("work", "atlas-donut-stand-two", "Donut Stand Two"),
            Some("project:atlas"),
        )
        .await;
        assert!(
            teaches(&asked, "work:atlas-x-donut-stand"),
            "a sibling sharing a word the parent does not give was not asked: {asked}",
        );

        // The same project word with no parent to share it: asked.
        let (jojobot, sid) = shown_a_child_of_atlas().await;
        let rootless = created_under(
            &jojobot,
            &sid,
            ("work", "atlas-kwik-e-mart", "Kwik E Mart"),
            None,
        )
        .await;
        assert!(
            teaches(&rootless, "work:atlas-x-donut-stand"),
            "a creation under no parent sharing the word was not asked: {rootless}",
        );
    }

    /// **The session that was not shown the thing is not asked about it.**
    ///
    /// The ledger is per session. Another session reading the same places leaves
    /// this one with nothing to be asked.
    #[tokio::test]
    async fn another_sessions_reading_is_not_this_sessions_question() {
        let jojobot = handler();
        let reader = writing_as(&jojobot);
        let writer = jojobot
            .registry
            .mint_with(&EntityId("bot:otto".into()), None, || "tst2".to_string())
            .expect("a second session")
            .to_string();
        created(&jojobot, &reader, "place", "wonder-wharf", "Wonder Wharf").await;
        browsed(&jojobot, &reader, "place").await;

        // The positive the negative rests on: the session that read them is asked.
        let asked_of_the_reader =
            created(&jojobot, &reader, "place", "wharf-road", "Wharf Road").await;
        assert!(
            teaches(&asked_of_the_reader, "place:wonder-wharf"),
            "the session that read the places was not asked: {asked_of_the_reader}",
        );
        let asked_of_the_writer =
            created(&jojobot, &writer, "place", "wharf-lane", "Wharf Lane").await;
        assert!(
            !teaches(&asked_of_the_writer, "place:wonder-wharf"),
            "a thing this session never saw was asked about: {asked_of_the_writer}",
        );
    }

    /// Fold `duplicate` into `survivor`, as the session `sid`.
    async fn merged(jojobot: &Jojobot, sid: &str, duplicate: &str, survivor: &str) {
        let landed = json_of(
            &jojobot
                .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
                    duplicate: duplicate.into(),
                    survivor: survivor.into(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(sid.into()),
                }))
                .await
                .expect("merge ok"),
        );
        assert_eq!(landed["merged"], duplicate, "the merge landed: {landed}");
    }

    /// **A handle that was merged away is never the thing a creation is asked
    /// about, whether the session was shown it before the merge or reads it
    /// after.** Asking about it suggests a merge that answers `AlreadyMerged`.
    ///
    /// Two sessions, because the two ways the ledger can be handed a folded
    /// handle are different: the one that merged held it as a live thing, and
    /// the one that reads the husk afterwards is shown it as a status. Each
    /// is paired with an unmerged thing shown the same way, which is still
    /// asked about, so an empty ledger cannot pass for a correct one.
    #[tokio::test]
    async fn a_merged_away_handle_is_not_the_thing_a_creation_is_asked_about() {
        let jojobot = handler();
        let merger = writing_as(&jojobot);
        let reader = jojobot
            .registry
            .mint_with(&EntityId("bot:otto".into()), None, || "tst2".to_string())
            .expect("a second session")
            .to_string();
        created(&jojobot, &merger, "thing", "teapot", "Mill Race Weir").await;
        created(&jojobot, &merger, "thing", "piano", "Grist Works").await;
        created(&jojobot, &merger, "thing", "canoe", "Mill Pond").await;

        // The merging session was shown all three, then folds the first.
        browsed(&jojobot, &merger, "thing").await;
        merged(&jojobot, &merger, "thing:teapot", "thing:piano").await;
        let asked = created(&jojobot, &merger, "thing", "battery", "Mill Race Road").await;
        assert!(
            teaches(&asked, "thing:canoe"),
            "the unmerged thing it was shown is no longer asked about: {asked}",
        );
        assert!(
            !teaches(&asked, "thing:teapot"),
            "the session that merged was asked about the handle it merged away: {asked}",
        );

        // The reading session lists the things, then reads the folded handle's
        // husk, which makes the husk the most recently shown.
        browsed(&jojobot, &reader, "thing").await;
        jojobot
            .recall(Parameters(crate::memory::recall::RecallArgs {
                sid: Some(reader.clone()),
                ..crate::memory::testing::recall_args("thing:teapot")
            }))
            .await
            .expect("recall ok");
        let asked = created(&jojobot, &reader, "thing", "backup-drive", "Mill Weir Pond").await;
        assert!(
            teaches(&asked, "thing:canoe"),
            "the unmerged thing the reader was shown is not asked about: {asked}",
        );
        assert!(
            !teaches(&asked, "thing:teapot"),
            "the session that read the husk was asked about the merged-away handle: {asked}",
        );
    }

    /// **A read's answer is the same whether or not the session is being
    /// remembered.** The ledger changes no read: the same listing, asked with a
    /// session and without one, comes back byte for byte alike.
    #[tokio::test]
    async fn remembering_what_a_read_showed_changes_nothing_the_read_says() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        created(&jojobot, &sid, "place", "wonder-wharf", "Wonder Wharf").await;
        let ask = |sid: Option<String>| {
            let jojobot = jojobot.clone();
            async move {
                json_of(
                    &jojobot
                        .list_entities(Parameters(crate::memory::list_entities::ListEntitiesArgs {
                            kind: Some("place".into()),
                            parent: None,
                            sid,
                        }))
                        .await
                        .expect("list_entities ok"),
                )
            }
        };
        let anonymous = ask(None).await;
        let remembered = ask(Some(sid)).await;
        assert_eq!(anonymous["count"], 1, "{anonymous}");
        assert_eq!(
            anonymous["entities"], remembered["entities"],
            "remembering what was shown changed what the read said",
        );
        assert!(
            remembered.get("teaching").is_none(),
            "a read carries a note: {remembered}",
        );
    }
    /// 🚨 **A guarded key in a creation's `sets` is judged as a capture judges
    /// it.** `reports_to` is written only by whom its declaration licenses, and
    /// a creation that carried it in `sets` once skipped the check, so any
    /// caller could place a new bot under any manager. A caller who is neither
    /// the manager named nor above it is refused and nothing is created; the
    /// manager itself creates the same bot and the chart reads back.
    #[tokio::test]
    async fn a_creation_naming_a_manager_is_licensed_as_a_capture_is() {
        let jojobot = handler();
        ensure(&jojobot, "bot:omega").await;
        let placing = |sid: String| AddEntityArgs {
            sid: Some(sid),
            sets: Some(
                [("reports_to".to_string(), "bot:omega".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..add_args("bot", "sigma", "Sigma")
        };

        // `TEST_SID` is bot:otto, who is neither bot:omega nor above it.
        let refused = blocked(
            &jojobot
                .add_entity(Parameters(placing(TEST_SID.into())))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .is_some_and(|how| how.contains("bot:omega")),
            "the refusal names the bot that may place it: {refused}"
        );
        let after = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    parent: None,
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        assert!(
            !after.to_string().contains("bot:sigma"),
            "a refused creation leaves nothing behind: {after}"
        );

        // The positive: the manager named creates the same bot, and it lands.
        let omega = as_bot(&jojobot, "omega");
        let landed = json_of(
            &jojobot
                .add_entity(Parameters(placing(omega)))
                .await
                .expect("add ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        let held = fields_of(&jojobot, "bot:sigma").await;
        assert_eq!(held["reports_to"], "bot:omega", "{held}");
    }
    /// 🚨 **A creation at a handle a renamed thing used to wear is judged as the
    /// NEW thing it makes.** A rename frees the old handle, and a new bot may
    /// take it. The licence check resolves a subject to the handle it answers to
    /// now so a renamed bot cannot raise its own ceiling by its former name, but
    /// a thing being created has no former handle: resolving it found the bot
    /// that used to wear the name, and judged the new bot as that one. Here the
    /// renamed bot creates a new bot at its old handle and sets that new bot's
    /// ceiling, which a different identity may do. Paired with the same write
    /// naming the renamed bot itself, which is refused.
    #[tokio::test]
    async fn a_creation_at_a_handle_a_renamed_thing_used_to_wear_is_judged_as_the_new_thing() {
        let jojobot = handler();
        // Named apart from its slug, so the ordinary same-name screen has
        // nothing to say about the new bot that takes the freed handle.
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(TEST_SID.into()),
                ..add_args("bot", "omega", "The Original")
            }))
            .await
            .expect("add ok");
        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                    handle: "bot:omega".into(),
                    to: "bot:delta".into(),
                    parent: None,
                    recorded_at: None,
                    override_token: None,
                    sid: Some(TEST_SID.into()),
                }))
                .await
                .expect("rename ok"),
        );
        assert_ne!(renamed["status"], "blocked", "{renamed}");
        let delta = as_bot(&jojobot, "delta");
        let ceiling = || {
            Some(
                [("thought_capacity".to_string(), "5".to_string())]
                    .into_iter()
                    .collect(),
            )
        };

        // The renamed bot sets a ceiling on a NEW bot at its old handle.
        let created = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(delta.clone()),
                    sets: ceiling(),
                    ..add_args("bot", "omega", "The Spare")
                }))
                .await
                .expect("add ok"),
        );
        assert_ne!(created["status"], "blocked", "{created}");
        assert_eq!(created["id"], "bot:omega", "{created}");

        // The pair: the same bot setting its OWN ceiling is still refused.
        let own = blocked(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(delta),
                    fields: ceiling(),
                    ..capture_args("bot:delta", "raising my own ceiling")
                }))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(own["wrote"], false, "{own}");
    }
}
