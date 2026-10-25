//! `rename_entity` — Move a handle: a new slug, a new kind, a new parent, or any two together.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

mod args;
mod resume;
pub use args::RenameEntityArgs;

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
    ) -> (
        Vec<(&'static str, serde_json::Value)>,
        Option<crate::stopped::Stopped>,
    ) {
        use jojobot_domain::request::{Part, part};
        match part("box", None, self.mailboxes.repoint_owner(from, to)).await {
            Part::Ran(Ok(Some(mailbox))) => (vec![("mailbox", mailbox.name.as_str().into())], None),
            Part::Ran(Ok(None)) => (Vec::new(), None),
            // An earlier call moved the box, and said where in `landed`.
            Part::Recorded(recorded) => {
                let named = serde_json::from_str::<serde_json::Value>(&recorded.landed)
                    .ok()
                    .and_then(|landed| landed.get("mailbox")?.as_str().map(str::to_string));
                (
                    named
                        .map(|name| vec![("mailbox", name.into())])
                        .unwrap_or_default(),
                    None,
                )
            }
            // **Named rather than silent, for the reason `open_box_with`'s
            // own failure branch is.** The entity moved and there is no verb
            // that moves it back, so this cannot be rolled into a clean
            // refusal — the honest answer is the write that happened plus
            // the part that did not.
            Part::Ran(Err(err)) => (
                vec![(
                    "mailbox_note",
                    format!(
                        "THIS RENAME IS INCOMPLETE: the entity moved to '{to}', but its mailbox \
                         could not be repointed, so mail addressed to it may not be found on \
                         the next boot. Tell the operator — repairing it takes a person. The \
                         mailbox world said: {err}"
                    )
                    .into(),
                )],
                Some(crate::stopped::Stopped::of_mailbox(&err)),
            ),
        }
    }

    /// **Refuse a retype that would move the instance's operator out of the
    /// person kind, whether or not their box exists yet.** The operator is a
    /// person, and their box opens at the first post, so a retype before it
    /// leaves no box for the check below to find. The operator would be a bot,
    /// and a later post to that handle would land in an ordinary bot's box,
    /// readable by booting as the bot.
    ///
    /// The record's operator is compared as the handle `from` answers to now, so
    /// a former handle of the operator is caught too. A record that cannot be
    /// read refuses the rename with retry, because it cannot tell whether `from`
    /// is the operator. A rename that stays inside the person kind is not this
    /// check's business.
    async fn rename_would_unseat_the_operator(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Option<CallToolResult> {
        let person = Some(EntityKind::PERSON);
        if to.kind() == person {
            return None;
        }
        let operator = match self.instance_operator().await {
            Ok(operator) => operator?,
            Err(e) => {
                tracing::warn!(error = %e, "the instance's record could not be read for a rename");
                return Some(blocked_body(
                    "RenameWouldUnseatTheOperator",
                    to,
                    &[],
                    crate::answer::WayForward::memory_store_failure(format!(
                        "Nothing was renamed. Whether '{from}' is the operator is {}.",
                        crate::orientation::instance_zone::operator_unreadable(),
                    )),
                ));
            }
        };
        let now = self.current_handle(from).await.ok()?;
        (now == operator).then(|| {
            blocked_body(
                "RenameWouldUnseatTheOperator",
                to,
                &[],
                format!(
                    "Nothing was renamed. '{from}' is the instance's operator, and the operator \
                     is a person, so moving it out of the person kind would leave the instance \
                     with no operator and a later post to it in an ordinary bot's box. Rename \
                     it within the person kind, or name another person as operator first."
                ),
            )
        })
    }

    /// **Refuse a retype that would change whether a box's owner is a person,
    /// before anything moves.** A box is private because a person owns it, and a
    /// retype repoints the box with its owner. The operator renamed to a bot
    /// would hand their box to a bot, readable by booting as it; a booted bot
    /// renamed to a person would rebind its handles to the person, and the
    /// person's mail would reach the old session. Either way, a rename that keeps
    /// the owner a person, or keeps it something else, is untouched.
    ///
    /// `Ok(None)` when `from` owns no box, or when the move keeps its
    /// person-ness. The box is found by owner on the board, so the question is
    /// asked of the mail world and not of a guess about the kind.
    async fn mailbox_rename_would_change_who_may_read(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<CallToolResult>, McpError> {
        let person = Some(EntityKind::PERSON);
        if (from.kind() == person) == (to.kind() == person) {
            return Ok(None);
        }
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(crate::mailboxes::mailbox_error)?;
        if !boxes.iter().any(|b| &b.owner == from) {
            return Ok(None);
        }
        Ok(Some(blocked_body(
            "MailboxRenameWouldChangeWhoMayRead",
            to,
            &[],
            format!(
                "Nothing was renamed. '{from}' owns a mailbox, and a mailbox is private exactly \
                 when a person owns it, so moving it into or out of the person kind would change \
                 who may read that mail. Keep the kind: rename it within its kind, or leave it \
                 as it is."
            ),
        )))
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
        let new_name = mailbox::MailboxName::named_for(to);
        let new_name = new_name.as_str();
        let Some(existing) = boxes
            .iter()
            .find(|b| b.name.as_str() == new_name && &b.owner != from)
        else {
            return Ok(None);
        };
        Ok(Some(blocked_body(
            "MailboxRenameWouldCollide",
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
        description = "Move a handle — a new slug, a new kind (a retype), a new parent, or \
                       any two together in one call, since the handle is one string and a \
                       retype is a rename. What follows automatically, with nothing rewritten \
                       anywhere: a mention written as @kind:slug, an edge, a reference-typed \
                       field value, every claim's own home, a journal beat and a mailbox \
                       message all keep resolving under the new handle. Every entity naming \
                       this one as its parent is repointed in the same write, and a box \
                       follows its owner to the owner's new name. What does NOT follow: a \
                       handle written as free prose that never used @kind:slug, and a thing \
                       renamed and later folded into another resolves one hop short of the \
                       survivor. A rename that moves an owner across the person line (a bot \
                       into a person, a person into a bot, or the operator out of the person \
                       kind) is refused, because a person's box is private. A role object \
                       (kind role) is renamed, retyped or reparented only by the bot it \
                       belongs to or a bot heading the chart; anyone else is refused and told \
                       who may. The display name stays as it was: update_entity's name \
                       changes it. `to` is always `kind:slug`, fully qualified: a bare slug is \
                       refused rather than guessed at. The destination faces the same \
                       near-miss guard a creation does: a collision with something that \
                       resembles it comes back status: blocked with candidates, liftable with \
                       override_token unless the collision is an exact handle, which can \
                       never be forced; a collision on the name alone also needs \
                       override_reason, shown beside the name. A `handle` that names nothing jojobot ever held is \
                       blocked with the nearest handles; one that named something BEFORE an \
                       earlier rename moved it is told what it is called now. The receipt's \
                       `changed` lists changes, or is `nothing` or `not_read`."
    )]
    pub(crate) async fn rename_entity(
        &self,
        Parameters(args): Parameters<RenameEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let resumed = jojobot_domain::request::skip("rename");
        let from = match resumed.as_ref() {
            Some(recorded) => match resume::source(&args.handle, recorded) {
                Ok(from) => from,
                Err(refused) => return Ok(refused),
            },
            None => match self.row_handle(&EntityId::person(&args.handle)).await {
                Ok(from) => from,
                Err(e) => return memory_declined("rename_entity", e),
            },
        };
        // **A call that finds its rename already written under this id is
        // finishing it.** The checks below judge the old handle and the old box,
        // which the first call moved, so they would refuse the finish; they
        // ran, and passed, before the rename landed.
        if resumed.is_some() {
            let to = EntityId(args.to.trim().to_string());
            let date = self
                .dated(args.recorded_at.as_deref(), args.sid.as_deref())
                .await?;
            let Some(entity) = self
                .memory
                .list_entities(None)
                .await
                .ok()
                .and_then(|all| all.into_iter().find(|entity| entity.id == to))
            else {
                return Ok(crate::caller::handle_declined(
                    "rename_entity",
                    crate::answer::WayForward::person(
                        "The rename was written under this request_id, but the thing cannot \
                         be read under its new name, so the rest cannot be finished.",
                    ),
                ));
            };
            return self
                .renamed(&args, &caller, &from, date, entity, Err(()))
                .await;
        }
        // **A voucher is issued, never moved.** Judged on the handle's kind.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&from) {
            return memory_declined("rename_entity", refused);
        }
        // **Any change to a role object hands its name to somebody else**: a new
        // parent moves the role to another bot, and a new slug or a new kind
        // frees the name for a claim. So it is judged as archiving it is
        // whatever the call changes.
        let act = if args.parent.is_some() {
            "move it to another bot"
        } else {
            "rename or retype it"
        };
        if let Some(refused) = self
            .refuse_a_stranger_the_role_object(&caller.bot, &from, act)
            .await
        {
            return Ok(refused);
        }
        // **Always fully qualified — no bare-slug convenience here.** A
        // capture's subject reads a bare slug as `person:` because that is
        // the ordinary case; a rename's destination has no ordinary case, so
        // guessing a kind would be guessing the caller's intent about the
        // one thing this call exists to let them state.
        let to = EntityId(args.to.trim().to_string());
        // **Nor is one made by a rename**: a thing retyped to a voucher's kind
        // would be a grant written by hand.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&to) {
            return memory_declined("rename_entity", refused);
        }
        if let Some(refused) = self.rename_would_unseat_the_operator(&from, &to).await {
            return Ok(refused);
        }
        if let Some(refused) = self
            .mailbox_rename_would_change_who_may_read(&from, &to)
            .await?
        {
            return Ok(refused);
        }
        if let Some(refused) = self.mailbox_rename_would_collide(&from, &to).await? {
            return Ok(refused);
        }
        // **A rename that would leave the bot's boot over the ceiling is refused
        // before anything moves**: its handle is in the boot several times.
        let floor_reads = super::reads_left::ReadsLeft::default();
        if let Some(refused) = self
            .refuses_a_boot_floor_for_rename(&from, &to, &floor_reads)
            .await
        {
            return memory_declined("rename_entity", refused);
        }
        let parent = args.parent.as_deref().map(EntityId::person);
        let date = self
            .dated(args.recorded_at.as_deref(), args.sid.as_deref())
            .await?;
        // **The thing as it stands under its old handle**, for the receipt's
        // account of what the write changed.
        let before = self.as_it_stands(&from).await;
        // **The rename is the first of up to three parts**, each written once
        // under a `request_id`: the entity, the reason it was let through, and
        // the box that follows it.
        let renamed = match jojobot_domain::request::part(
            "rename",
            Some(from.to_string()),
            self.memory.rename_entity(
                &from,
                &to,
                parent,
                date,
                jojobot_domain::memory::Override::new(
                    args.override_token.as_deref(),
                    args.override_reason.as_deref(),
                ),
                Some(caller.sid.as_str()),
            ),
        )
        .await
        {
            jojobot_domain::request::Part::Ran(Ok(renamed)) => renamed,
            jojobot_domain::request::Part::Ran(Err(e)) => {
                return memory_declined("rename_entity", e);
            }
            // The checks above ran with `rename` unwritten, so a call that gets
            // here wrote it in this call.
            jojobot_domain::request::Part::Recorded(_) => {
                return Ok(crate::caller::handle_declined(
                    "rename_entity",
                    crate::answer::WayForward::retry(
                        "The rename was written under this request_id while this call ran. \
                         Send the same call again.",
                    ),
                ));
            }
        };
        match renamed {
            Guarded::Written(entity) => {
                self.renamed(&args, &caller, &from, date, entity, before)
                    .await
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
                    "RenameEntity",
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
                "RenameEntity",
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
            } => {
                // **Each candidate carries what tells it apart**, when anyone said.
                let candidates = self.told_apart(candidates).await;
                Ok(blocked_result(
                    &attempted,
                    &candidates,
                    Blocked::Renaming(TokenSlot::from(&args)),
                ))
            }
        }
    }
}

impl Jojobot {
    /// **The parts of a rename that follow the entity**, written once under a
    /// `request_id`: the reason the destination was let through, and the box
    /// that follows its owner. `before` is the thing as it stood under its old
    /// handle, or an error when this call is finishing a rename an earlier call
    /// wrote and so cannot say what it changed.
    async fn renamed(
        &self,
        args: &RenameEntityArgs,
        caller: &crate::caller::Caller,
        from: &EntityId,
        date: jiff::civil::Date,
        entity: Entity,
        before: Result<Option<jojobot_domain::memory::search::DocScan>, ()>,
    ) -> Result<CallToolResult, McpError> {
        self.beat("rename_entity", entity.id.as_str(), args.sid.as_deref())
            .await;
        // **The other half of a rename**, beside the mailbox: every
        // handle THIS PROCESS already holds for `from` has to move
        // too, or it keeps attributing everything it does to the
        // identity that just stopped answering to that name.
        // Nothing on the write path knows this registry exists, so
        // it is not something `Memory::rename_entity` could have
        // done for us.
        self.registry.rebind_bot(from, &entity.id);
        let mut body = entity_json(&entity);
        // **An override's reason is a key on the renamed thing.** The rename
        // has landed and is never reported as failed, so a key that could
        // not be written is said in the answer instead.
        if let Some(reason) = kept_override_reason(
            args.override_token.as_deref(),
            args.override_reason.as_deref(),
        ) {
            let mut claim = jojobot_domain::memory::NewFact::about(
                entity.id.clone(),
                entity.name.clone(),
                date,
            );
            claim.session = Some(caller.sid.as_str().to_string());
            claim.sets.insert(guard::TOLD_APART_BY.to_string());
            // **The record of an act jojobot did takes no slot**, so a thread that
            // holds its number is renamed and the reason is kept.
            claim.fields.insert(
                jojobot_domain::memory::capacity::ENGINE_RECORD.to_string(),
                "rename".to_string(),
            );
            // **Added beside the reason the thing already holds**, never written
            // over it. A read of the held reasons that fails is this part failing:
            // taken as "none held", it would write the new reason over them.
            let (memory, id) = (&self.memory, entity.id.clone());
            let written = async move {
                let held = memory.fields(&id).await?;
                claim.fields.insert(
                    guard::TOLD_APART_BY.to_string(),
                    guard::add_told_apart_by(
                        held.get(guard::TOLD_APART_BY).map(String::as_str),
                        reason,
                    ),
                );
                memory.capture(claim).await
            };
            // `FoldBehind` is a write that landed with only the fold's
            // re-read failed (rule 130), so the reason is kept.
            use jojobot_domain::request::{Part, part};
            let kept: Result<(), crate::stopped::Stopped> =
                match part("reason", None, written).await {
                    Part::Ran(Ok(Guarded::Written(_)))
                    | Part::Ran(Err(MemoryError::FoldBehind {
                        landed: Landed::Fact(_),
                        ..
                    }))
                    | Part::Recorded(_) => Ok(()),
                    // A claim the guard blocked wrote nothing, and the same call meets
                    // the same block.
                    Part::Ran(Ok(_)) => Err(crate::stopped::Stopped::Refused(
                        "the claim that keeps the reason was blocked".into(),
                    )),
                    Part::Ran(Err(e)) => Err(crate::stopped::Stopped::of_memory(&e)),
                };
            // **With an id the call is not finished until the reason is kept**:
            // the same call sent again writes it and nothing else, unless the step
            // was refused, which the same call meets again.
            if let Err(cause) = &kept
                && jojobot_domain::request::carries_an_id()
            {
                return Ok(crate::caller::handle_declined(
                    "rename_entity",
                    cause.unfinished(
                        "The rename was written, and the reason it was let through was not.",
                        "Send the same call again with the same request_id and it writes the \
                         reason and the rest, and renames nothing twice.",
                    ),
                ));
            }
            if kept.is_err()
                && let Some(obj) = body.as_object_mut()
            {
                obj.insert(
                    "reason_not_kept".into(),
                    "the rename landed and the reason could not be kept: set the \
                     told_apart_by key on the renamed entity"
                        .into(),
                );
            }
        }
        let (box_answer, box_stopped) = self.repoint_mailbox(from, &entity.id).await;
        if let Some(cause) = &box_stopped
            && jojobot_domain::request::carries_an_id()
        {
            return Ok(crate::caller::handle_declined(
                "rename_entity",
                cause.unfinished(
                    "The rename was written, and the box that follows it was not moved.",
                    "Send the same call again with the same request_id and it moves the box \
                     and renames nothing twice.",
                ),
            ));
        }
        jojobot_domain::request::finished();
        if let Some(obj) = body.as_object_mut() {
            for (key, value) in box_answer {
                obj.insert(key.into(), value);
            }
            // **A rename moves the handle and never the display name**,
            // so a reslug leaves the old words on the thing. Said only
            // when the name does not already spell the new slug: then
            // nothing was left behind.
            if from.slug() != entity.id.slug()
                && jojobot_domain::memory::guard::slugify(&entity.name) != entity.id.slug()
            {
                obj.insert(
                    "name_note".into(),
                    format!(
                        "The slug changed and the display name did not: it still reads \
                         '{}'. update_entity's `name` changes it.",
                        entity.name
                    )
                    .into(),
                );
            }
        }
        crate::request::note_resumed(&mut body);
        crate::changed::note_changed(&mut body, self.changed_on(&before, &entity.id).await);
        crate::memory::wire::elide_aliases_past_the_ceiling(&mut body);
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    fn args(handle: &str, to: &str, sid: &str) -> RenameEntityArgs {
        RenameEntityArgs {
            request_id: None,
            handle: handle.into(),
            to: to.into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            override_reason: None,
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
                request_id: None,
                to: "gamma".into(),
                body: "mail that has to survive the rename".into(),
                sid: writer.clone(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
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
                    parent: None,
                    offset: None,
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

    /// **The positive `a_rename_onto_an_existing_handle_is_blocked` needs**:
    /// a near miss on the DESTINATION, not an exact collision, is the
    /// refusal that IS overridable, and its own token really lifts it.
    /// Without this, `Blocked::Renaming`'s promising arm was proven to
    /// refuse correctly but never proven to keep the promise it makes —
    /// only its exact-collision sibling and its two unoverridable parent
    /// refusals had a round trip.
    #[tokio::test]
    async fn a_rename_near_miss_on_the_destination_is_lifted_by_its_own_token() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "beta", "Beta")))
            .await
            .expect("add ok");

        let rename = |token: Option<String>| RenameEntityArgs {
            override_token: token,
            ..args("person:beta", "person:alphaa", &sid)
        };

        let result = jojobot
            .rename_entity(Parameters(rename(None)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:alphaa");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        assert_eq!(body["candidates"][0]["reason"], "near-slug");
        let how = body["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            how.contains(OFFERS_A_TOKEN),
            "a near miss on the destination is lifted by a token: {body}",
        );
        let token = how
            .split_once(OFFERS_A_TOKEN)
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the advice hands over the token it mints")
            .0
            .to_string();

        // A token nobody minted must not move it — without this half, the
        // assertion below holds identically on a build that accepts any
        // string.
        let invented = json_of(
            &jojobot
                .rename_entity(Parameters(rename(Some("0000000000000000".into()))))
                .await
                .expect("the call succeeds; the guard answers in the body"),
        );
        assert_eq!(
            invented["status"], "blocked",
            "a token nobody minted moves nothing: {invented}"
        );

        let forced = json_of(
            &jojobot
                .rename_entity(Parameters(rename(Some(token))))
                .await
                .expect("confirmed rename ok"),
        );
        assert_ne!(forced["status"], "blocked");
        assert_eq!(forced["id"], "person:alphaa");
    }

    /// **A reason that landed is never reported as lost** (rule 130): the
    /// `told_apart_by` capture can land while only the fold behind it fails to
    /// refresh, and that is `MemoryError::FoldBehind`, not a failed write.
    /// Paired in one read with the key being on the renamed thing, so the
    /// absence of `reason_not_kept` cannot be a reason that never went in.
    #[tokio::test]
    async fn a_rename_past_a_name_only_near_miss_keeps_its_reason_when_the_fold_is_behind() {
        let jojobot = Jojobot::new(
            Arc::new(FoldBehindMemory(Arc::new(InMemoryMemory::booted()))),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("person", "person:ralph", "Moes")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args(
                "place",
                "place:leftorium",
                "Leftorium",
            )))
            .await
            .expect("add ok");

        let rename = |token: Option<String>, reason: Option<&str>| RenameEntityArgs {
            override_token: token,
            override_reason: reason.map(str::to_string),
            ..args("place:leftorium", "place:moes", &sid)
        };
        let refused = json_of(
            &jojobot
                .rename_entity(Parameters(rename(None, None)))
                .await
                .expect("the call succeeds; the guard answers in the body"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(
            refused["candidates"][0]["reason"], "same-name-other-kind",
            "{refused}"
        );
        let token = refused["how_to_proceed"]
            .as_str()
            .and_then(|how| how.split_once("override_token: \""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the advice hands over the token it mints")
            .0
            .to_string();

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(rename(
                    Some(token),
                    Some("a place, not a person"),
                )))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "place:moes", "{renamed}");
        assert_eq!(
            fields_of(&jojobot, "place:moes").await["told_apart_by"],
            "a place, not a person",
            "the reason landed on the renamed thing: {renamed}"
        );
        assert!(
            renamed.get("reason_not_kept").is_none(),
            "a reason that landed is not reported as lost: {renamed}"
        );
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

    /// **A reslug leaves the display name as it was, and the receipt says so.**
    /// The note names the verb that changes it. The negative half rides in the
    /// next two cases: a retype that keeps the slug, and a reslug onto the
    /// slug the name already spells, both say nothing.
    #[tokio::test]
    async fn a_reslug_that_leaves_the_name_behind_says_how_to_change_it() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("thing:red-bike", "thing:road-bike", &sid)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "thing:road-bike", "{renamed}");
        assert_eq!(renamed["name"], "Red Bike", "the name stays: {renamed}");
        let note = renamed["name_note"]
            .as_str()
            .unwrap_or_else(|| panic!("a reslug that left the name says so: {renamed}"));
        assert!(note.contains("update_entity"), "{renamed}");
    }

    /// The negative half of the case above: a rename that changes no slug, and
    /// a reslug onto the slug the display name already spells, leave the name
    /// where the caller wants it, so the receipt carries no note.
    #[tokio::test]
    async fn a_rename_that_matches_the_name_or_keeps_the_slug_says_nothing_of_it() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "fat-bike", "Gravel Bike")))
            .await
            .expect("add ok");

        let reslugged = json_of(
            &jojobot
                .rename_entity(Parameters(args(
                    "thing:fat-bike",
                    "thing:gravel-bike",
                    &sid,
                )))
                .await
                .expect("rename ok"),
        );
        assert_eq!(reslugged["id"], "thing:gravel-bike", "{reslugged}");
        assert!(reslugged.get("name_note").is_none(), "{reslugged}");

        // The name here does NOT spell the slug, so only the unchanged slug
        // keeps the note away.
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Crimson Cycle")))
            .await
            .expect("add ok");
        let retyped = json_of(
            &jojobot
                .rename_entity(Parameters(args("thing:red-bike", "work:red-bike", &sid)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(retyped["id"], "work:red-bike", "{retyped}");
        assert_eq!(retyped["name"], "Crimson Cycle", "{retyped}");
        assert!(retyped.get("name_note").is_none(), "{retyped}");
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
                request_id: None,
                to: "sigma".into(),
                body: "mail sent after the rename".into(),
                sid: other,
                subject: None,
                in_reply_to: None,
                supersedes: None,
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
        assert_eq!(
            rebuilt.rebuild_from(&board).recovered,
            1,
            "one handle recovered"
        );
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
            run_of(&resumed),
            Some(run_name(&jojobot, &writer)),
            "the same run, still addressed by the same name after a restart: {resumed}",
        );
        assert_ne!(
            sid_of(&resumed).as_deref(),
            Some(writer.as_str()),
            "a resume hands the run a new key: {resumed}",
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

        // `bot:milhouse` is also an exact entity collision — but the mailbox
        // check runs BEFORE that guard is even reached, because it runs before
        // any entity write is attempted at all. So this is the mailbox
        // world's own refusal, not the entity guard's: proven below by its
        // empty `candidates` and its own wording, neither of which the
        // entity guard would produce.
        let result = jojobot
            .rename_entity(Parameters(args("bot:epsilon", "bot:milhouse", &sid)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "bot:milhouse", "{body}");
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
                    parent: None,
                    offset: None,
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
                    request_id: None,
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:no-such-bike".into()),
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
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
                    request_id: None,
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("work:sigma".into()),
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
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
                    request_id: None,
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:no-such-bike".into()),
                    recorded_at: None,
                    override_token: Some(guard::override_token(
                        &EntityId::person("thing:no-such-bike"),
                        &[],
                    )),
                    override_reason: None,
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
                    request_id: None,
                    handle: "thing:tau".into(),
                    to: "work:sigma".into(),
                    parent: Some("thing:kettle".into()),
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
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

    /// **A stale handle refuses as an answer, never a protocol error** — the
    /// verb's own argument documentation already promises this ("given here, a
    /// stale handle is refused with the name it moved to, rather than a bare
    /// miss"), and until this fix `memory_declined` let
    /// `MemoryError::HandleMoved` fall through to a plain `McpError`, making
    /// the promise false for the one case it names.
    #[tokio::test]
    async fn a_stale_handle_rename_is_blocked_rather_than_a_client_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        jojobot
            .rename_entity(Parameters(args("thing:red-bike", "work:red-bike", &sid)))
            .await
            .expect("rename ok");

        // "thing:red-bike" is now stale: it answers to "work:red-bike".
        let result = jojobot
            .rename_entity(Parameters(args("thing:red-bike", "work:sigma", &sid)))
            .await
            .expect("a stale handle is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "thing:red-bike");
        assert_eq!(body["wrote"], false);
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap()
                .contains("work:red-bike"),
            "the way forward names what it is called now: {body}",
        );
    }

    /// **A rename to the handle it already wears refuses as an answer**, not
    /// a protocol error — `MemoryError::NothingToRename` fell through to
    /// `memory_error` exactly as `HandleMoved` and `SuppliedHandle` did.
    #[tokio::test]
    async fn renaming_to_the_same_handle_is_blocked_rather_than_a_client_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");

        let result = jojobot
            .rename_entity(Parameters(args("thing:red-bike", "thing:red-bike", &sid)))
            .await
            .expect("renaming to itself is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "thing:red-bike");
        assert_eq!(body["wrote"], false);
    }

    /// **A build-supplied handle's rename refuses as an answer**, not a
    /// protocol error. Built on a bare store that knows what the build
    /// supplies directly (the shape `Memory::rename_entity`'s own
    /// `MemoryError::SuppliedHandle` names), rather than `handler_shipped()`
    /// — that handler wraps `Provisioned`, whose `rename_entity` answers a
    /// supplied handle with `Guarded::Blocked` before the store underneath is
    /// ever asked, so it cannot exercise this error at all.
    #[tokio::test]
    async fn a_supplied_handles_rename_is_blocked_rather_than_a_client_error() {
        use jojobot_domain::mailbox::testing::InMemoryMailboxes;
        use jojobot_domain::memory::testing::InMemoryMemory;
        use jojobot_domain::session::testing::InMemorySessions;
        use jojobot_domain::teaching::testing::InMemoryTeachings;

        let jojobot = Jojobot::new(
            std::sync::Arc::new(InMemoryMemory::booted().knowing(crate::provisions())),
            std::sync::Arc::new(SpySearch::default()),
            std::sync::Arc::new(InMemoryMailboxes::knowing_any_owner()),
            std::sync::Arc::new(InMemorySessions::new()),
            std::sync::Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        let sid = writing_as(&jojobot);

        let result = jojobot
            .rename_entity(Parameters(args(
                "view:loops",
                "view:contract-supplied-rename-target",
                &sid,
            )))
            .await
            .expect("a build-supplied handle is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "view:loops");
        assert_eq!(body["wrote"], false);
    }

    /// **A build-supplied record's rename, through the shipped stack, is
    /// refused as the build's own record** — the refusal `archive_entity` and
    /// `merge` give — and never as a parent that is unknown.
    ///
    /// The shipped stack matters: `Provisioned` answers a supplied handle
    /// before the store underneath is asked, and the sentence it used to
    /// borrow was the one for a parent the call named. Told apart by the
    /// refusal's structure: that sentence rides a candidate that is the
    /// handle itself, while the build's-own-record refusal names no
    /// candidate.
    #[tokio::test]
    async fn a_supplied_records_rename_through_the_shipped_stack_is_the_supplied_refusal() {
        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);

        let result = jojobot
            .rename_entity(Parameters(args(
                "view:loops",
                "view:contract-supplied-rename-target",
                &sid,
            )))
            .await
            .expect("a supplied record's rename is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "view:loops", "{body}");
        assert_eq!(
            body["candidates"],
            serde_json::json!([]),
            "no candidate: the handle is the build's own record, not a near miss or a parent: {body}",
        );
    }

    /// **The paired positive: a stored view still renames.** The supplied
    /// refusal is for the build's own record, not for the kind.
    #[tokio::test]
    async fn a_stored_view_still_renames_through_the_shipped_stack() {
        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("view", "my-bikes", "My Bikes")))
            .await
            .expect("add ok");

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("view:my-bikes", "view:my-pets", &sid)))
                .await
                .expect("rename ok"),
        );
        assert_eq!(renamed["id"], "view:my-pets", "{renamed}");
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

    /// 🚨 **A renamed bot's SENT mail still counts as theirs.**
    ///
    /// `list_sent` answers "did my report land" by matching the sender
    /// column exactly against the caller's CURRENT handle. Without mailbox
    /// `Mentioning` resolving `sender` the way `session.bot` already is, a
    /// bot that posts, is renamed, and then asks what it sent is told
    /// none — a confident zero standing in for mail that is still there.
    ///
    /// PAIRED: a bot that genuinely sent nothing is still told zero, so the
    /// fix cannot be told from deleting the filter.
    #[tokio::test]
    async fn a_renamed_bots_sent_mail_still_counts_as_theirs() {
        use crate::mailboxes::list_sent::ListSentArgs;
        use crate::mailboxes::post_message::PostMessageArgs;
        use jojobot_domain::mailbox::Mailboxes;
        use jojobot_domain::mailbox::mention::Mentioning as MailboxMentioning;
        use jojobot_domain::mailbox::testing::InMemoryMailboxes;

        let memory = std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        let bare_mail = std::sync::Arc::new(InMemoryMailboxes::knowing_any_owner());
        let mailboxes: std::sync::Arc<dyn Mailboxes> =
            std::sync::Arc::new(MailboxMentioning::new(bare_mail.clone(), memory.clone()));
        let jojobot = Jojobot::new(
            memory.clone(),
            std::sync::Arc::new(SpySearch::default()),
            mailboxes,
            std::sync::Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            std::sync::Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );

        jojobot
            .add_entity(Parameters(add_args("bot", "otto", "Otto")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
            .await
            .expect("add ok");
        let writer = booted(&jojobot, "gamma").await;

        for body in ["first report", "second report"] {
            jojobot
                .post_message(Parameters(PostMessageArgs {
                    request_id: None,
                    to: "otto".into(),
                    body: body.into(),
                    sid: writer.clone(),
                    subject: None,
                    in_reply_to: None,
                    supersedes: None,
                }))
                .await
                .expect("post ok");
        }

        jojobot
            .rename_entity(Parameters(args("bot:gamma", "bot:sigma", &writer)))
            .await
            .expect("rename ok");

        let sent = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    offset: None,
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(writer.clone()),
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(
            sent["count"], 2,
            "both messages sent before the rename are still theirs: {sent}"
        );
        let senders: Vec<&str> = sent["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| m["sender"].as_str().expect("a sender"))
            .collect();
        assert_eq!(
            senders,
            vec!["bot:sigma", "bot:sigma"],
            "rendered under the CURRENT handle, not the one they were sent under: {sent}"
        );

        // PAIRED — a bot that genuinely sent nothing is still told zero.
        jojobot
            .add_entity(Parameters(add_args("bot", "epsilon", "Epsilon")))
            .await
            .expect("add ok");
        let other = booted(&jojobot, "epsilon").await;
        let empty = json_of(
            &jojobot
                .list_sent(Parameters(ListSentArgs {
                    limit: None,
                    offset: None,
                    sender: None,
                    to: None,
                    include_bodies: None,
                    sid: Some(other),
                }))
                .await
                .expect("list_sent ok"),
        );
        assert_eq!(
            empty["count"], 0,
            "a bot that never sent anything still reads zero: {empty}"
        );
    }

    /// **A rename that would turn the operator into a bot is refused before
    /// anything moves.** A box is private because a person owns it, and a retype
    /// repoints the box with its owner: the operator renamed to a bot would
    /// leave their box a bot's, read by booting as that bot. The refusal says
    /// why, the entity keeps its kind, the box keeps its owner, and the mail
    /// stays unreadable by every bot. A rename that keeps the kind still works
    /// and the box follows it, still private.
    #[tokio::test]
    async fn a_rename_that_would_turn_a_persons_box_into_a_bots_is_refused() {
        use crate::mailboxes::testing::*;
        let jojobot = mailbox_handler();
        let held = a_persons_box(&jojobot, "milhouse").await;
        let writer = owning(&jojobot, "epsilon").await;
        let posted = send(
            &jojobot,
            "person:milhouse",
            "epsilon",
            "the secret figure is wiggum",
        )
        .await;
        let id = posted["id"].as_str().expect("an id").to_string();
        assert_eq!(store_counts(&jojobot, &held).await, (1, 0, 0));

        // ── the retype away from person is refused, and nothing moved ───────
        for to in ["bot:milhouse", "thing:handcart"] {
            let refused = json_of(
                &jojobot
                    .rename_entity(Parameters(args("person:milhouse", to, &writer)))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["status"], "blocked", "{to}: {refused}");
            assert_eq!(refused["wrote"], false, "{to}: {refused}");
        }
        assert_eq!(
            boxes_owned_by(&jojobot, "person:milhouse").await,
            vec![held.clone()],
            "the box still belongs to the person"
        );
        assert!(
            boxes_owned_by(&jojobot, "bot:milhouse").await.is_empty(),
            "no bot owns it"
        );
        let after = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id.clone(),
                    sid: Some(writer.clone()),
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(after["status"], "blocked", "{after}");
        assert!(!after.to_string().contains("wiggum"), "{after}");

        // ── the positive: a rename that keeps the kind goes through ─────────
        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args(
                    "person:milhouse",
                    "person:ned-flanders",
                    &writer,
                )))
                .await
                .expect("rename ok"),
        );
        assert_ne!(renamed["status"], "blocked", "{renamed}");
        // The box follows its owner and keeps wearing the person's name, so it
        // can never be a bot's box name.
        assert_eq!(
            boxes_owned_by(&jojobot, "person:ned-flanders").await,
            vec![mailbox::MailboxName("person-ned-flanders".into())],
            "the box followed its owner"
        );
        assert!(
            boxes_owned_by(&jojobot, "person:milhouse").await.is_empty(),
            "…and left the old name {held:?}"
        );
        let still = json_of(
            &jojobot
                .read_message(Parameters(ReadMessageArgs {
                    history: None,
                    message_id: id,
                    sid: Some(writer),
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(still["status"], "blocked", "still private: {still}");
    }

    /// **A rename that would turn a booted bot into a person is refused too.**
    /// The bot's handles are rebound to the new handle, and a session bound to a
    /// person owns the person's box: later mail written to that person would be
    /// handed to the old session. Refused before anything moves, with the bot
    /// still a bot, still reading its own box.
    #[tokio::test]
    async fn a_rename_that_would_turn_a_bot_into_a_person_is_refused() {
        use crate::mailboxes::testing::*;
        let jojobot = mailbox_handler();
        let gamma = owning(&jojobot, "gamma").await;
        owning(&jojobot, "epsilon").await;
        send(&jojobot, "gamma", "epsilon", "a note for gamma").await;

        let refused = json_of(
            &jojobot
                .rename_entity(Parameters(args("bot:gamma", "person:gamma", &gamma)))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(boxes_owned_by(&jojobot, "bot:gamma").await.len(), 1);
        assert!(boxes_owned_by(&jojobot, "person:gamma").await.is_empty());
        // The positive: the bot still reads its own box through the same handle.
        let delivered = json_of(
            &jojobot
                .read_mailbox(Parameters(ReadMailboxArgs {
                    counts_only: None,
                    new_only: None,
                    sid: Some(gamma),
                }))
                .await
                .expect("read ok"),
        );
        assert!(
            delivered.to_string().contains("a note for gamma"),
            "{delivered}"
        );
    }

    /// **A retype that does not cross the person line still lands.** The refusal
    /// is about a box changing hands across the private line, and nothing wider:
    /// a person who owns no box can be retyped, and a bot with a box can be
    /// renamed within the bot kind, its box following it. This is the positive
    /// the two refusals above depend on, so a refusal of every retype would not
    /// pass.
    #[tokio::test]
    async fn a_retype_that_does_not_cross_the_person_line_still_lands() {
        use crate::mailboxes::testing::*;
        let jojobot = mailbox_handler();
        let writer = owning(&jojobot, "gamma").await;
        // A person who owns no box: nothing to hand across the line.
        crate::memory::testing::ensure(&jojobot, "person:ned-flanders").await;
        assert!(
            boxes_owned_by(&jojobot, "person:ned-flanders")
                .await
                .is_empty()
        );
        let retyped = json_of(
            &jojobot
                .rename_entity(Parameters(args(
                    "person:ned-flanders",
                    "thing:handcart",
                    &writer,
                )))
                .await
                .expect("rename ok"),
        );
        assert_ne!(retyped["status"], "blocked", "{retyped}");
        assert_eq!(retyped["id"], "thing:handcart", "{retyped}");

        // A bot with a box, renamed within the bot kind: the box follows.
        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("bot:gamma", "bot:delta", &writer)))
                .await
                .expect("rename ok"),
        );
        assert_ne!(renamed["status"], "blocked", "{renamed}");
        assert_eq!(boxes_owned_by(&jojobot, "bot:delta").await.len(), 1);
    }
    /// 🚨 **The person the instance names as operator cannot be retyped, box or
    /// no box.** Their box opens at the first post, so a retype before it left
    /// no box to protect and moved the operator into the bot kind; a later post
    /// then landed in an ordinary bot's box, readable by booting as that bot.
    /// Refused before anything moves, with the person still the operator. A
    /// person who is not the operator retypes as before, and the operator
    /// renamed within the person kind still lands and stays the operator.
    #[tokio::test]
    async fn the_operator_cannot_be_retyped_before_their_box_exists() {
        use crate::mailboxes::testing::*;
        let jojobot = mailbox_handler();
        let writer = owning(&jojobot, "gamma").await;
        name_the_operator(&jojobot, "milhouse").await;
        assert!(
            boxes_owned_by(&jojobot, "person:milhouse").await.is_empty(),
            "the case is the one with no box yet"
        );

        for to in ["bot:milhouse", "thing:handcart"] {
            let refused = json_of(
                &jojobot
                    .rename_entity(Parameters(args("person:milhouse", to, &writer)))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["status"], "blocked", "{to}: {refused}");
            assert_eq!(refused["wrote"], false, "{to}: {refused}");
            assert!(
                refused["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains("operator")),
                "the refusal says why: {refused}"
            );
        }
        let still = fields_of(&jojobot, "topic:instance").await;
        assert_eq!(still["operator"], "person:milhouse", "{still}");

        // ── the positives the refusal rests on ───────────────────────────────
        // A person who is not the operator retypes freely.
        crate::memory::testing::ensure(&jojobot, "person:ned-flanders").await;
        let free = json_of(
            &jojobot
                .rename_entity(Parameters(args(
                    "person:ned-flanders",
                    "thing:handcart",
                    &writer,
                )))
                .await
                .expect("rename ok"),
        );
        assert_ne!(free["status"], "blocked", "{free}");
        // The operator renamed within the person kind lands, and the record follows.
        let moved = json_of(
            &jojobot
                .rename_entity(Parameters(args("person:milhouse", "person:lisa", &writer)))
                .await
                .expect("rename ok"),
        );
        assert_ne!(moved["status"], "blocked", "{moved}");
        let followed = fields_of(&jojobot, "topic:instance").await;
        assert_eq!(followed["operator"], "person:lisa", "{followed}");
    }

    /// **A rename names the new handle with the old one beside it**, and
    /// nothing else: the claims the thing holds move with it and did not
    /// change.
    #[tokio::test]
    async fn a_rename_lists_the_handle_from_and_to_and_nothing_else() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        capture_ok(&jojobot, capture_args("thing:red-bike", "has a bell")).await;

        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(args("thing:red-bike", "thing:road-bike", &sid)))
                .await
                .expect("rename ok"),
        );
        let changed = renamed["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {renamed}"));
        assert_eq!(changed.len(), 1, "{renamed}");
        assert_eq!(changed[0]["on"], "thing:road-bike", "{renamed}");
        assert_eq!(changed[0]["what"], "handle", "{renamed}");
        assert_eq!(changed[0]["from"], "thing:red-bike", "{renamed}");
        assert_eq!(changed[0]["to"], "thing:road-bike", "{renamed}");
    }

    /// 🚨 **A rename whose held reason cannot be read keeps the reasons the thing
    /// held.** The rename lands before its reason is written, so a failed read of
    /// the held reasons is the reason part failing: nothing is written over them,
    /// and the answer says the reason was not kept. Taken as "none held", the read
    /// wrote the new reason alone in their place.
    #[tokio::test]
    async fn a_rename_whose_held_reason_cannot_be_read_keeps_the_held_reasons() {
        let (healthy, blind) = healthy_and_down(Down::FieldsOf("place:moes"));
        let sid = writing_as(&healthy);
        healthy
            .add_entity(Parameters(add_args("person", "person:ralph", "Moes")))
            .await
            .expect("add ok");
        healthy
            .add_entity(Parameters(AddEntityArgs {
                sets: Some([(guard::TOLD_APART_BY.to_string(), "a shop".to_string())].into()),
                ..add_args("place", "place:leftorium", "Leftorium")
            }))
            .await
            .expect("add ok");

        let rename = |token: Option<String>, reason: Option<&str>| RenameEntityArgs {
            override_token: token,
            override_reason: reason.map(str::to_string),
            ..args("place:leftorium", "place:moes", &sid)
        };
        let refused = json_of(
            &blind
                .rename_entity(Parameters(rename(None, None)))
                .await
                .expect("the guard answers in the body"),
        );
        let token = refused["how_to_proceed"]
            .as_str()
            .and_then(|how| how.split_once("override_token: \""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the advice hands over the token it mints")
            .0
            .to_string();

        let renamed = json_of(
            &blind
                .rename_entity(Parameters(rename(
                    Some(token),
                    Some("a place, not a person"),
                )))
                .await
                .expect("the rename answers"),
        );
        assert_eq!(renamed["id"], "place:moes", "{renamed}");
        assert!(
            renamed.get("reason_not_kept").is_some(),
            "a reason not kept is said: {renamed}"
        );
        assert_eq!(
            fields_of(&healthy, "place:moes").await[guard::TOLD_APART_BY],
            "a shop"
        );
    }
}
