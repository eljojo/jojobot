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

    /// **Refuse a rename before anything moves, when its destination mailbox
    /// name is already worn by a different box.**
    ///
    /// `repoint_owner`'s own `UPDATE` collides on the mailbox table's own
    /// primary key — the mailbox `name` — and by the time it runs the entity
    /// rename has already committed, which is what makes that collision a
    /// half-done rename rather than a clean refusal. This runs first, and this
    /// one collision is predictable: the destination name is known before
    /// anything moves, so it is checked before anything moves.
    ///
    /// `Ok(None)` when `from` owns no mailbox at all — nothing will be
    /// repointed either way — or when the destination name is free.
    async fn mailbox_rename_would_collide(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<CallToolResult>, McpError> {
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(crate::mailboxes::mailbox_error)?;
        if !boxes.iter().any(|b| &b.owner == from) {
            return Ok(None);
        }
        let new_name = to.slug();
        let Some(existing) = boxes
            .iter()
            .find(|b| b.name.as_str() == new_name && &b.owner != from)
        else {
            return Ok(None);
        };
        Ok(Some(blocked_body(
            to,
            &[],
            format!(
                "Nothing was renamed. The mailbox '{new_name}' already belongs to {}, so \
                 renaming '{from}' to '{to}' would collide with it before the entity itself \
                 moved. Choose a destination whose slug is not already a mailbox name.",
                existing.owner.as_str(),
            ),
        )))
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
        if let Some(refused) = self.mailbox_rename_would_collide(&from, &to).await? {
            return Ok(refused);
        }
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
                // **The other half of a rename**, beside the mailbox: every
                // handle THIS PROCESS already holds for `from` has to move
                // too, or it keeps attributing everything it does to the
                // identity that just stopped answering to that name.
                // Nothing on the write path knows this registry exists, so
                // it is not something `Memory::rename_entity` could have
                // done for us.
                self.registry.rebind_bot(&from, &entity.id);
                let mut body = entity_json(&entity);
                if let Some(obj) = body.as_object_mut() {
                    for (key, value) in self.repoint_mailbox(&from, &entity.id).await {
                        obj.insert(key.into(), value);
                    }
                }
                json_result(&body)
            }
            // **A parent refusal is not a near miss, and saying it is offers
            // a way forward that leads back to the same wall.** Neither of
            // these is overridable: a parent must already exist, because
            // nothing is created as a side effect of renaming something
            // else, and nothing is its own parent. The near-miss sentence
            // tells a caller to re-call with a token, and a caller who does
            // is refused again and minted another. Rule 68 is about a way
            // FORWARD. Mirrors `add_entity`'s own two arms below.
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
                        "Nothing was renamed. '{attempted}' names itself as its parent, and \
                         nothing is its own parent. No override_token lifts this. Name the \
                         entity this one sits under, or leave parent off to leave the current \
                         parent alone."
                    ),
                ))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } if attempted != to => Ok(blocked_body(
                &attempted,
                &candidates,
                format!(
                    "Nothing was renamed. '{attempted}' is the parent this call named, and it \
                     is not an entity jojobot knows. No override_token lifts this: nothing is \
                     created as a side effect of renaming something else. Create '{attempted}' \
                     with its own add_entity call first, then re-call this one — or drop the \
                     parent to leave '{from}' under its current parent."
                ),
            )),
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

    /// 🚨 **A handle minted before a rename keeps working under the NEW
    /// identity, for the rest of the process — not just on the next
    /// restart.**
    ///
    /// Nothing in the write path touches this process's own cache of who a
    /// handle answers to, so a `sid` the door handed out before the rename
    /// used to go on attributing everything it did to the bot that had just
    /// stopped answering to that name. Proven the same way the mailbox case
    /// is: by what the OLD `sid` can still reach, never by inspecting the
    /// registry directly. `read_mailbox` is the read that makes the drift
    /// visible — it resolves the caller's OWN box by `caller.bot`, so a
    /// stale identity reads as "no box" the moment the old name's box has
    /// already moved.
    #[tokio::test]
    async fn a_renamed_bots_old_sid_keeps_working_as_the_new_identity() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
            .await
            .expect("add ok");
        let writer = booted(&jojobot, "gamma").await;

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("bot:gamma", "bot:sigma", &writer)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "bot:sigma", "{renamed}");

        // Somebody else posts to the NEW handle, after the rename.
        let other = writing_as(&jojobot);
        jojobot
            .post_message(Parameters(PostMessageArgs {
                to: "sigma".into(),
                body: "mail sent after the rename".into(),
                sid: other,
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("post ok");

        // The SAME handle the door gave out before the rename reads it —
        // proof that this process now attributes it to sigma, not gamma.
        let mail = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: Some(true),
                    new_only: None,
                    sid: Some(writer),
                }))
                .await
                .expect("read_mailbox ok"),
        );
        assert_eq!(
            mail["mailbox"], "sigma",
            "the old handle must open sigma's box, not gamma's, which no longer exists: {mail}",
        );
        assert_eq!(
            mail["counts"]["new"], 1,
            "and it must be sigma's actual mail, not an empty or broken box: {mail}",
        );
    }

    /// 🚨 **A rename survives a RESTART: the renamed bot's session is still
    /// found and still resumable under its new name, on a fresh registry
    /// that never saw the rename happen.**
    ///
    /// This is the one thing `a_renamed_bots_old_sid_keeps_working_as_the_
    /// new_identity` cannot prove: that test's registry lives through the
    /// rename in memory, which is exactly what a restart throws away. Here
    /// the session store is wrapped in the real production decorator
    /// (`session::mention::Mentioning`) instead of the bare fake `handler()`
    /// uses, because the bare fake is what every other verb test wants —
    /// isolated from mention/badge resolution — and this is the one case
    /// that is ABOUT that resolution surviving the trip through a card.
    #[tokio::test]
    async fn a_renamed_bots_session_survives_a_restart() {
        use crate::harness::seed_bot;
        use crate::session::testing::journal_entry;
        use jojobot_domain::mailbox::testing::InMemoryMailboxes;
        use jojobot_domain::session::Sessions;
        use jojobot_domain::session::mention::Mentioning;
        use jojobot_domain::session::testing::InMemorySessions;

        let memory = std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        let bare_sessions = std::sync::Arc::new(InMemorySessions::new());
        let sessions: std::sync::Arc<dyn Sessions> =
            std::sync::Arc::new(Mentioning::new(bare_sessions.clone(), memory.clone()));
        seed_bot(&memory, "gamma").await;
        let registry = std::sync::Arc::new(sid::SessionRegistry::new());
        let jojobot = Jojobot::new(
            memory.clone(),
            std::sync::Arc::new(SpySearch::default()),
            std::sync::Arc::new(InMemoryMailboxes::knowing_any_owner()),
            sessions.clone(),
            std::sync::Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            registry,
        );

        let writer = booted(&jojobot, "gamma").await;
        journal_entry(&jojobot, &writer, "before the rename").await;

        jojobot
            .rename_entity(Parameters(args("bot:gamma", "bot:sigma", &writer)))
            .await
            .expect("rename ok");

        // A restart: same underlying board, a FRESH registry that never
        // observed the rename — exactly what the composition root builds
        // after a process restart, per `sid::SessionRegistry::rebuild_from`.
        // Read through `sessions` (the Mentioning-wrapped port), matching
        // `main.rs` exactly: the board a restart rebuilds from is rendered,
        // never the bare badge the row itself carries.
        let board = sessions.all_sessions().await.expect("all_sessions ok");
        let rebuilt = std::sync::Arc::new(sid::SessionRegistry::new());
        assert_eq!(rebuilt.rebuild_from(&board), 1, "one handle recovered");
        let restarted = Jojobot::new(
            memory,
            std::sync::Arc::new(SpySearch::default()),
            std::sync::Arc::new(InMemoryMailboxes::knowing_any_owner()),
            sessions,
            std::sync::Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            rebuilt,
        );

        // Booted under the NEW name, offered the OLD handle as the resume
        // answer — and it resumes, with the OLD story intact.
        let resumed = boot_answering(&restarted, "sigma", &writer).await;
        assert_eq!(
            sid_of(&resumed).as_deref(),
            Some(writer.as_str()),
            "the same handle, still addressing the same run after a restart: {resumed}",
        );
        assert_eq!(
            resumed["session"]["session"]["chronology"][0]["text"], "before the rename",
            "{resumed}",
        );
    }

    /// 🚨 **A destination mailbox name already worn by a different box
    /// refuses the WHOLE rename, before the entity itself moves — even when
    /// the destination HANDLE collides with nothing.**
    ///
    /// The destination here is `person:milhouse` — no entity answers to it,
    /// so the ordinary entity-handle guard has nothing to say and would let
    /// this rename land. It is the MAILBOX slug alone that collides:
    /// `bot:epsilon`'s box would have to become "milhouse", and `bot:milhouse`
    /// already owns a box by that name. `repoint_owner`'s own collision — on
    /// the mailbox table's primary key — lands after the entity write has
    /// already committed, which used to leave a half-done rename with only
    /// an informational note to show for it. This collision is predictable
    /// ahead of time, so it is caught ahead of time: proven by what did NOT
    /// move, not by the refusal shape alone.
    #[tokio::test]
    async fn a_mailbox_name_collision_refuses_the_whole_rename() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("bot", "epsilon", "Epsilon")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("bot", "milhouse", "Milhouse")))
            .await
            .expect("add ok");
        let sid = writing_as(&jojobot);

        // `person:milhouse` also happens to be a same-slug-other-kind near
        // miss against the existing `bot:milhouse` — but the mailbox check
        // runs BEFORE that guard is even reached, because it runs before
        // any entity write is attempted at all. So this is the mailbox
        // world's own refusal, not the entity guard's: proven below by its
        // empty `candidates` and its own wording, neither of which the
        // entity near-miss guard would produce.
        let result = jojobot
            .rename_entity(Parameters(args("bot:epsilon", "person:milhouse", &sid)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:milhouse", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
        assert!(
            body["candidates"].as_array().unwrap().is_empty(),
            "this is the mailbox world's own refusal, not another entity near-miss: {body}"
        );
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap()
                .contains("mailbox 'milhouse'"),
            "{body}"
        );

        // …and nothing moved: the source handle still answers, and neither
        // box was touched.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    sid: None,
                }))
                .await
                .expect("list ok"),
        );
        let ids: Vec<&str> = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert!(
            ids.contains(&"bot:epsilon"),
            "the source entity must not have moved: {listed}"
        );
        let boxes = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list_mailboxes ok");
        assert_eq!(
            boxes.len(),
            2,
            "no box may have been repointed or lost: {boxes:?}",
        );
        assert!(
            boxes.iter().any(|b| b.owner.as_str() == "bot:epsilon"),
            "epsilon's own box must still be its own: {boxes:?}",
        );
    }

    /// How a refusal HANDS OVER a token, which is the thing a caller can act
    /// on — as against merely naming the argument to say no token applies.
    const OFFERS_A_TOKEN: &str = "override_token: \"";

    /// **A refusal about the PARENT says what repairs it, and does not offer
    /// a token that cannot lift it.**
    ///
    /// Mirrors `add_entity`'s own
    /// `a_parent_refusal_does_not_offer_a_token_that_cannot_lift_it`: before
    /// this fix, every `Guarded::Blocked` from `rename_entity` — the
    /// destination colliding, or the parent being unusable — wore the same
    /// near-miss sentence, which tells a caller to re-call with an
    /// `override_token`. Neither parent refusal is overridable: a parent
    /// must exist, because nothing is created as a side effect of renaming
    /// something else, and nothing is its own parent. A caller following
    /// that advice is refused again and minted another token, forever — a
    /// way forward that leads back to the same wall.
    ///
    /// Paired with the positive: a parent that DOES exist still reparents,
    /// asserted in the same read.
    #[tokio::test]
    async fn a_rename_parent_refusal_does_not_offer_a_token_that_cannot_lift_it() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(add_args("thing", "tau", "Tau")))
            .await
            .expect("add ok");

        let missing = json_of(
            &jojobot
                .rename_entity(Parameters(RenameEntityArgs {
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:no-such-bike".into()),
                    recorded_at: None,
                    override_token: None,
                    sid: Some(sid.clone()),
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
        assert_eq!(
            missing["candidates"].as_array().map(Vec::len),
            Some(0),
            "no candidate resembles it: {missing}",
        );

        // Self-parent: the destination named as its own parent.
        let itself = json_of(
            &jojobot
                .rename_entity(Parameters(RenameEntityArgs {
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("work:sigma".into()),
                    recorded_at: None,
                    override_token: None,
                    sid: Some(sid.clone()),
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
        // with one; doing that is refused again, so the loop the advice
        // opened is pinned shut here rather than only described.
        let forced = json_of(
            &jojobot
                .rename_entity(Parameters(RenameEntityArgs {
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:no-such-bike".into()),
                    recorded_at: None,
                    override_token: Some(guard::override_token(
                        &EntityId::person("thing:no-such-bike"),
                        &[],
                    )),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(
            forced["status"], "blocked",
            "a token cannot create the parent it names: {forced}",
        );

        // The positive: a parent that DOES exist still reparents.
        let ok = json_of(
            &jojobot
                .rename_entity(Parameters(RenameEntityArgs {
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:kettle".into()),
                    recorded_at: None,
                    override_token: None,
                    sid: Some(sid),
                }))
                .await
                .expect("a real parent must still let the rename through"),
        );
        assert_eq!(ok["id"], "work:sigma", "{ok}");
        assert_eq!(
            ok["parent"], "thing:kettle",
            "a real parent still reparents: {ok}",
        );
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
