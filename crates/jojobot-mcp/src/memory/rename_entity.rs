//! `rename_entity` — Move a handle: a new slug, a new kind, a new parent, or any two together.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `rename_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameEntityArgs {
    /// The entity's current handle — what it answers to now, or what it
    /// used to answer to before an earlier move already changed it. A stale
    /// handle still resolves here, and through a mention, an edge, a
    /// reference-typed field value, a claim's home, a journal beat and a
    /// mailbox message — all of them stay current after a move. A stale
    /// `handle` given here is refused with the name it moved to, rather
    /// than a bare miss.
    pub(crate) handle: String,
    /// The destination, as `kind:slug`. Always fully qualified — unlike a
    /// capture's subject, a bare slug is never read as a kind by default
    /// here, because guessing one would be guessing whether this call is a
    /// reslug or a retype. Changing the kind IS a retype; changing the slug
    /// IS a reslug; the id is one string, so either, or both together, is
    /// this same call.
    pub(crate) to: String,
    /// Reparent onto this handle. Omit to leave the current parent alone.
    /// **There is no way to clear a parent through this verb.**
    #[serde(default)]
    pub(crate) parent: Option<String>,
    /// The day this rename was made, `YYYY-MM-DD`. Defaults to today in the
    /// caller's own day.
    #[serde(default)]
    pub(crate) recorded_at: Option<String>,
    /// The token a previous call's refusal handed you, sent back after you
    /// read its candidates for the DESTINATION handle and judged them a
    /// different entity. It lifts only the refusal that minted it, and
    /// never an exact handle collision — a handle has exactly one owner.
    #[serde(default)]
    pub(crate) override_token: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

impl Jojobot {
    /// **The mail world's half of a rename**, called after the entity
    /// world's own write has already landed.
    ///
    /// Not gated on the entity's new kind: what decides whether there is
    /// anything to do is whether the OLD handle owned a box, which
    /// `repoint_owner` itself answers — a bot renamed, reslugged or even
    /// retyped away from `bot` all reach here the same way, and only the
    /// first two have a box to find.
    async fn repoint_mailbox(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Vec<(&'static str, serde_json::Value)> {
        match self.mailboxes.repoint_owner(from, to).await {
            Ok(Some(mailbox)) => vec![("mailbox", mailbox.name.as_str().into())],
            Ok(None) => Vec::new(),
            // **Named rather than silent, for the reason `open_box_with`'s
            // own failure branch is.** The entity moved and there is no verb
            // that moves it back, so this cannot be rolled into a clean
            // refusal — the honest answer is the write that happened plus
            // the part that did not.
            Err(err) => vec![(
                "mailbox_note",
                format!(
                    "THIS RENAME IS INCOMPLETE: the entity moved to '{to}', but its mailbox \
                     could not be repointed, so mail addressed to it may not be found on the \
                     next boot. Tell the operator — repairing it takes a person. The mailbox \
                     world said: {err}"
                )
                .into(),
            )],
        }
    }
}

#[tool_router(router = rename_entity_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Move a handle — a new slug, a new kind (a retype), a new parent, or any \
                       two together in one call, since the handle is one string and a retype is \
                       a rename. What follows automatically, with nothing rewritten anywhere: a \
                       mention written as @kind:slug, an edge, a reference-typed field value, \
                       every claim's own home, a journal beat and a mailbox message all keep \
                       resolving under the new handle — the path renders on the way out, and \
                       text stored before any of that was true was migrated once, so it is not \
                       an exception. Every entity naming this one as its parent is repointed in \
                       the same write, and a bot's mailbox follows to its new name. What does \
                       NOT follow, ever: a handle written as free prose that never used \
                       @kind:slug — nothing marked it as a link, so nothing resolves it. And a \
                       thing renamed and later folded into another resolves one hop short of \
                       the survivor, through the folded thing, rather than composing the two. \
                       `to` is always `kind:slug`, \
                       fully qualified — a bare slug is refused rather than guessed at, because \
                       guessing one would be guessing whether this call is a reslug or a retype. \
                       `parent` reparents when given and leaves the current parent alone when \
                       omitted; there is no way to clear a parent through this verb. The \
                       destination faces the same near-miss guard a creation does: a collision \
                       with something that resembles it comes back status: blocked with \
                       candidates, liftable with override_token unless the collision is an exact \
                       handle, which can never be forced. A `handle` that names nothing jojobot \
                       ever held is blocked with the nearest handles; one that named something \
                       BEFORE an earlier rename moved it is told what it is called now."
    )]
    pub(crate) async fn rename_entity(
        &self,
        Parameters(args): Parameters<RenameEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let from = EntityId::person(&args.handle);
        // **Always fully qualified — no bare-slug convenience here.** A
        // capture's subject reads a bare slug as `person:` because that is
        // the ordinary case; a rename's destination has no ordinary case, so
        // guessing a kind would be guessing the caller's intent about the
        // one thing this call exists to let them state.
        let to = EntityId(args.to.trim().to_string());
        let parent = args.parent.as_deref().map(EntityId::person);
        let date = self.dated(args.recorded_at.as_deref(), args.sid.as_deref())?;
        let renamed = match self
            .memory
            .rename_entity(&from, &to, parent, date, args.override_token.as_deref())
            .await
        {
            Ok(renamed) => renamed,
            Err(e) => return memory_declined("rename_entity", e),
        };
        match renamed {
            Guarded::Written(entity) => {
                self.beat("rename_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
                let mut body = entity_json(&entity);
                if let Some(obj) = body.as_object_mut() {
                    for (key, value) in self.repoint_mailbox(&from, &entity.id).await {
                        obj.insert(key.into(), value);
                    }
                }
                json_result(&body)
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(&attempted, &candidates, Blocked::Renaming)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    fn args(handle: &str, to: &str, sid: &str) -> RenameEntityArgs {
        RenameEntityArgs {
            handle: handle.into(),
            to: to.into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            sid: Some(sid.into()),
        }
    }

    /// 🚨 **The mailbox case is its own, never riding a general assertion: a
    /// renamed bot finds its EXISTING box, through the served surface.**
    ///
    /// Not a repeat of the domain-level proof — that one never touches
    /// mailboxes at all, since `Memory::rename_entity` does not own them.
    /// This is the cross-context bridge, the same shape `add_entity`'s own
    /// box-opening is, and it has its own failure mode: a boot that finds no
    /// box owned by the new handle heals a second, EMPTY one beside the
    /// first, silently splitting a bot's mail across two boxes. Proven by
    /// what survives, not by a name matching: a message posted before the
    /// rename is still there, under the box the rename repointed, after it.
    #[tokio::test]
    async fn a_renamed_bots_mailbox_follows_to_its_new_handle() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
            .await
            .expect("add ok");
        let writer = booted(&jojobot, "gamma").await;
        jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "gamma".into(),
                body: "mail that has to survive the rename".into(),
                sid: writer.clone(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("post ok");

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("bot:gamma", "bot:sigma", &writer)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "bot:sigma", "{renamed}");
        assert_eq!(
            renamed["mailbox"], "sigma",
            "the rename's own answer says which box it repointed: {renamed}",
        );

        // **Exactly one box — not the old one plus a healed second.**
        let boxes = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list_mailboxes ok");
        assert_eq!(
            boxes.len(),
            1,
            "a repointed box must not leave the old one behind or open a second: {boxes:?}",
        );
        assert_eq!(boxes[0].name.as_str(), "sigma");
        assert_eq!(boxes[0].owner, EntityId("bot:sigma".into()));

        // **The boot door agrees**, reading the same world `owned_mailbox`
        // does — a renamed bot's next boot must not go through
        // `heal_missing_box` and open a fresh, empty one.
        let booted_body = boot(&jojobot, "sigma").await;
        assert_eq!(
            booted_body["identity"]["owned_mailbox"]["name"], "sigma",
            "{booted_body}",
        );

        // **And what survives is the actual mail, not just the box's
        // name.** Read as sigma now — the same identity, the handle it
        // answers to today.
        let reader = booted(&jojobot, "sigma").await;
        let mail = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: Some(reader),
                }))
                .await
                .expect("read_mailbox ok"),
        );
        assert_eq!(
            mail["counts"]["new"], 1,
            "the message posted before the rename did not survive it: {mail}",
        );
    }

    /// A rename onto a name the index already holds comes back blocked, the
    /// same shape a creation's or a relabel's collision does — the guard
    /// cannot be side-stepped by renaming onto a contested handle.
    #[tokio::test]
    async fn a_rename_onto_an_existing_handle_is_blocked() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let result = jojobot
            .rename_entity(Parameters(args("person:zenith", "person:alpha", &sid)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:alpha");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        assert_eq!(body["candidates"][0]["reason"], "exact-handle");

        // …and nothing moved.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        let names: Vec<&str> = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Alpha", "Zenith"]);
    }

    /// `rename_entity` moves the handle and leaves the metadata alone.
    #[tokio::test]
    async fn rename_entity_moves_the_handle_through_the_handler() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("thing:red-bike", "work:red-bike", &sid)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "work:red-bike", "{renamed}");
        assert_eq!(renamed["name"], "Red Bike", "the metadata rides along");
    }

    /// A `handle` naming nothing jojobot ever held is a client error naming
    /// near misses, exactly as the other verbs answer one — never a create.
    #[tokio::test]
    async fn rename_entity_unknown_handle_is_a_client_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let err = jojobot
            .rename_entity(Parameters(args("thing:red-bikee", "work:red-bike", &sid)))
            .await
            .expect("an unknown handle is an answer, not a protocol failure");
        let body = blocked(&err);
        assert_eq!(body["attempted"], "thing:red-bikee");
        assert_eq!(
            body["candidates"][0]["handle"], "thing:red-bike",
            "must name the near miss: {body}"
        );
    }
}
