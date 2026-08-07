//! `add_entity` — Bring a new entity into existence — and, for a bot, the box that comes with it.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `add_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddEntityArgs {
    /// One of `person`, `project`, `place`, `event`, `work`, `thing`, `org`,
    /// `topic`.
    pub kind: String,
    /// The slug half of the handle (`[a-z0-9-]+`), or a full `kind:slug` id
    /// whose kind must match `kind`. The handle is permanent — choose one that
    /// will still be right in a year.
    pub handle: String,
    /// Display name, as a human would write it.
    pub name: String,
    /// The other names this one answers to — nickname, short form, initials.
    /// Screened and searched exactly as `name` is, so a nickname the user
    /// actually says is both recognized and findable. No commas.
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    /// Where this entity came from — **never invented**: the user named it, or
    /// a real source produced it (e.g. `user-named`, `crm-card`, `calendar`).
    pub source: String,
    /// Optional cross-link to this entity in the task layer, in whatever form
    /// that layer addresses things. One reference, no space and no comma.
    #[serde(default)]
    pub crm: Option<String>,
    /// `always` marks this entity as part of the core an assistant loads at
    /// the start of every session; the default `on-demand` is fetched when the
    /// conversation reaches for it. Only the exact token `always` counts.
    #[serde(default)]
    pub boot: Option<String>,
    /// The token a previous call's refusal handed you, sent back after you read
    /// its candidates and judged them a different entity. It lifts only the
    /// refusal that minted it — a token you made up, or one from another
    /// refusal, lifts nothing — and it never overrides an exact handle
    /// collision.
    #[serde(default)]
    pub override_token: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
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
        if entity.id.kind() != Some(EntityKind::Bot) {
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
        description = "Bring a new entity into existence (person/project/place/event/work/\
                       thing/org/topic) — the required first step before any other write may \
                       name it. Returns the stored entity. If its handle or any of its names \
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
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let id = entity_id(&args.kind, &args.handle)?;
        let new = NewEntity {
            id,
            name: args.name,
            aliases: args.aliases.unwrap_or_default(),
            source: args.source,
            crm: args.crm,
            // The tool surface is unchanged this milestone: parentage is
            // reachable only from inside, so every write through the door is
            // a root.
            parent: None,
            boot: parse_boot(args.boot.as_deref())?,
            override_token: args.override_token.clone(),
        };
        // Routed through the declined path rather than straight to the mapper,
        // for the reason capture is: an entity the validators refuse is a
        // caller mistake and comes back as an answer (rule 68).
        let added = match self.memory.add_entity(new).await {
            Ok(added) => added,
            Err(e) => return memory_declined("add_entity", e),
        };
        match added {
            Guarded::Written(entity) => {
                self.beat("add_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
                let mut body = entity_json(&entity);
                if let Some(obj) = body.as_object_mut() {
                    for (key, value) in self.open_box_with(&entity).await {
                        obj.insert(key.into(), value);
                    }
                }
                json_result(&body)
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, Blocked::Creating)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

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
                sid: None,
            }))
            .await
            .expect("list ok");
        let body = json_of(&listed);
        assert_eq!(body["entities"][0]["id"], "project:atlas");
        assert_eq!(body["count"], 1);
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
            EntityId::new(EntityKind::Bot, "gamma"),
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
}
