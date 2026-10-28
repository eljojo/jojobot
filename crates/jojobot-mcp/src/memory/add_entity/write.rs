//! Creation decisions shared by the MCP handler and room furniture.

use super::*;

pub(crate) struct Added {
    pub(super) entity: Entity,
    pub(super) first_claim: Option<Fact>,
    pub(super) fold_behind: Option<jojobot_domain::memory::search::Behind>,
    pub(super) before: Result<Option<jojobot_domain::memory::search::DocScan>, ()>,
    pub(super) moves_to_now: Result<bool, MemoryError>,
    pub(super) unread: super::super::instance_unread::InstanceUnread,
    pub(super) floor_reads: super::super::reads_left::ReadsLeft,
    pub(super) floors_before: Option<Vec<(EntityId, usize)>>,
    pub(super) due_moved: jojobot_domain::attention::DueMove,
    pub(super) opened: OpenedBox,
    pub(super) dropped_keys: super::super::placement_gate::DroppedKeys,
}

impl Added {
    #[cfg(any(test, feature = "room-harness"))]
    pub(crate) fn complete(&self) -> bool {
        !self.opened.failed
    }
}

impl Jojobot {
    pub(crate) async fn add_entity_write(
        &self,
        author: &EntityId,
        args: AddEntityArgs,
        day: jiff::civil::Date,
        by_session: Option<&str>,
        declared_near: Option<&str>,
    ) -> Result<Result<Added, CallToolResult>, McpError> {
        let retry = declared_near.map(|near| (args.clone(), near));
        // **What this write could not work out for want of the instance record**,
        // named on the receipt.
        let unread = super::instance_unread::InstanceUnread::default();
        let id = entity_id(&args.kind, &args.handle)?;
        // Kept for the refusal below: which handle the guard turned back is
        // what says whether it was this entity or the one it named as parent.
        let creating = id.clone();
        // **A voucher is issued, never made by hand.** Judged on the handle's
        // kind, ahead of everything the call carries.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&creating) {
            return memory_declined("add_entity", refused).map(Err);
        }
        // **A new bot grows every other bot's snapshot**, which is a write that
        // is never refused: a colleague is not a mistake. What it can do is
        // take a bot's boot over the ceiling, so each bot's floor is read now
        // and again once the bot is there.
        // **What the boot-floor check could not read**, noted after its second measure.
        let floor_reads = super::reads_left::ReadsLeft::default();
        let floors_before = match creating.kind() == Some(EntityKind::BOT) {
            true => Some(self.boot_floors_of_bots(&floor_reads).await),
            false => None,
        };
        // Taken before `args`' fields are moved into `new` below — this
        // owns its own copy, so it survives the moves that follow.
        let token_slot = TokenSlot::from(&args);
        // **What the first claim says**, when there is one: the thing's own
        // name, as it will be stored. A claim needs words and the caller sent
        // fields, so the name is what a fields-only claim about a new thing says.
        // **An override's reason is a key on the new thing**, so what tells it apart
        // from the thing it resembles is shown beside its name wherever the two are
        // seen together. It rides only with a token: a reason alone overrides
        // nothing.
        let override_reason = kept_override_reason(
            args.override_token.as_deref(),
            args.override_reason.as_deref(),
        );
        let claim_words = args.name.trim().to_string();
        let new = NewEntity {
            id,
            name: args.name,
            aliases: args.aliases.unwrap_or_default(),
            source: args.source,
            crm: args.crm,
            session: by_session.map(str::to_string),
            // **Where a thing sits is the caller's to say.** A tree the domain
            // holds and the door does not carry makes every write through this
            // verb a root, so a kind that requires a parent has no reachable
            // way to be created at all.
            parent: args.parent.as_deref().map(EntityId::person),
            boot: parse_boot(args.boot.as_deref())?,
            override_token: args.override_token.clone(),
            override_reason: args.override_reason.clone(),
        };
        // Routed through the declined path rather than straight to the mapper,
        // for the reason capture is: an entity the validators refuse is a
        // caller mistake and comes back as an answer (rule 68).
        // **The first fields, when the caller sent any, are written as the thing's
        // first claim in the same act**, through the store's own combined write:
        // the store runs the guards of a capture inside the transaction that
        // creates the thing, and a refusal takes the creation back with it.
        let (first_fields, dropped_keys) = {
            let mut sets = args.sets.clone().unwrap_or_default();
            if let Some(reason) = override_reason {
                // A new thing holds only what this call sent.
                let merged = guard::add_told_apart_by(
                    sets.get(guard::TOLD_APART_BY).map(String::as_str),
                    reason,
                );
                sets.insert(guard::TOLD_APART_BY.to_string(), merged);
            }
            // **A creation by defrag carries the review stamp**, so the first claim
            // is written for it even when the caller set nothing.
            let dropped = match self
                .drop_read_keys(&creating, None, [&mut sets], &mut Vec::new())
                .await
            {
                Ok(dropped) => dropped,
                Err(e) => return memory_declined("add_entity", e).map(Err),
            };
            (
                (!sets.is_empty() || self.review_stamp(author).is_some()).then_some(sets),
                dropped,
            )
        };
        let mut fold_behind = None;
        let mut due_moved = jojobot_domain::attention::DueMove::Unchanged;
        let mut moves_to_now = Ok(false);
        // **The thing as it stands before the write** (none, for a creation),
        // for the receipt's account of what the write changed.
        let before = self.as_it_stands(&creating).await;
        // **A bot whose entity an earlier call wrote under this id is being
        // finished.** The checks below judge a thing that is not there yet, and it
        // is, so they are skipped; the receipt names the entity and its box.
        let resumed = (creating.kind() == Some(EntityKind::BOT))
            .then(|| jojobot_domain::request::skip("entity"))
            .flatten();
        let (added, first_claim) = match (resumed, first_fields) {
            (Some(_), _) => match self.entity_now(&creating).await {
                Some(entity) => (Guarded::Written(entity), None),
                None => {
                    return Ok(Err(crate::caller::handle_declined(
                        "add_entity",
                        crate::answer::WayForward::person(
                            "The bot was made under this request_id, but it cannot be read \
                             back, so its box cannot be opened.",
                        ),
                    )));
                }
            },
            (None, None) => {
                // **A bot is measured as the boot it will have, whatever the call
                // sets.** Its name, aliases and source ride in that boot, and a
                // creation that sets nothing can still take it over the ceiling.
                if creating.kind() == Some(EntityKind::BOT) {
                    let recorded_at = day;
                    let bare = jojobot_domain::memory::NewFact::about(
                        creating.clone(),
                        claim_words,
                        recorded_at,
                    );
                    if let Some(refused) = self
                        .refuses_a_boot_floor_for_creation(
                            &bare,
                            &entity_as_it_will_be(&new),
                            author,
                            &floor_reads,
                        )
                        .await
                    {
                        return memory_declined("add_entity", refused).map(Err);
                    }
                }
                // **A thing whose own row could not be read back whole is not
                // made**: its aliases are served on every read of it.
                if let Some(refused) =
                    Jojobot::refuses_a_new_thing_too_big_to_read(&entity_as_it_will_be(&new))
                {
                    return memory_declined("add_entity", refused).map(Err);
                }
                // **A bot lands with the record of the notes it was told**, as its
                // first claim, so the bot and its ledger are one write (rule 130).
                if creating.kind() == Some(EntityKind::BOT) {
                    let mut ledger = crate::release_notes::ledger_record(
                        &creating,
                        &crate::release_notes::shipped_ids(),
                        day,
                    );
                    ledger.session = by_session.map(str::to_string);
                    match armed_for(true, self.memory.add_entity_with_first_claim(new, ledger))
                        .await
                    {
                        Ok(Guarded::Written((entity, _))) => (Guarded::Written(entity), None),
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
                        Err(MemoryError::FoldBehind {
                            landed: Landed::Creation(created),
                            behind,
                            ..
                        }) => {
                            fold_behind = Some(behind);
                            (Guarded::Written(created.0), None)
                        }
                        Err(e) => return memory_declined("add_entity", e).map(Err),
                    }
                } else {
                    match armed_for(false, self.memory.add_entity(new)).await {
                        Ok(added) => (added, None),
                        Err(e) => return memory_declined("add_entity", e).map(Err),
                    }
                }
            }
            (None, Some(mut fields)) => {
                // **The checks a capture makes before it writes**, on what the
                // caller sent: a role's own two fields are the boot door's, and
                // the stored due moment is jojobot's own.
                if let Some(refused) =
                    jojobot_domain::memory::refuses_role_fields(&creating, fields.keys())
                {
                    return memory_declined("add_entity", refused).map(Err);
                }
                // **A guarded key is licensed here as on a capture**, through
                // the same call: the store's combined write takes no caller, so
                // this is the only place a creation's `sets` is judged.
                if let Some(refused) = self
                    .refuses_an_unlicensed_key("add_entity", &creating, false, author, &fields)
                    .await?
                {
                    return Ok(Err(refused));
                }
                if let Some(refused) = self
                    .refuses_a_hand_written_due_moment(&creating, &fields, &[])
                    .await
                {
                    return Ok(Err(refused));
                }
                if let Some(refused) =
                    self.refuses_a_hand_written_review_stamp(&creating, &fields, &[])
                {
                    return Ok(Err(refused));
                }
                // **A key that only labels a claim is never a property of the new
                // thing**, so a creation refuses it by name, as a capture does in
                // `sets`. A creation has no claim fields of its own to hold it.
                let describing = crate::seed::describing_keys();
                if let Some(label) = fields.keys().find(|key| describing.contains(*key)) {
                    return memory_declined(
                        "add_entity",
                        MemoryError::InvalidFact(format!(
                            "'{label}' describes a claim and is never a property of a thing, so \
                             a creation cannot set it. Create the thing without it, then write \
                             the label with capture, in that claim's fields"
                        )),
                    )
                    .map(Err);
                }
                // **The keys the caller sent are the settings.** The due moment worked
                // out below is jojobot's own arithmetic beside them, kept as one of
                // the claim's own fields, so a later edit that moves it keeps it in
                // the bag it was written under.
                let set_keys: std::collections::BTreeSet<String> = fields.keys().cloned().collect();
                // **Worked out here too**: a loop made with its cadence is whole
                // from the moment it is made, with the day it falls due beside
                // the keys that set it.
                let (due_on, _) = self
                    .moved_due_moment(&creating, &fields, &[], day, &unread)
                    .await;
                due_moved = due_on;
                // **The due moment reaches the new thing**: it is jojobot's own
                // arithmetic, not a setting the caller sent.
                let mut derived: std::collections::BTreeSet<String> = Default::default();
                if let jojobot_domain::attention::DueMove::Set(due_on) = due_on {
                    fields.insert(
                        jojobot_domain::attention::DUE_ON.to_string(),
                        due_on.to_string(),
                    );
                    derived.insert(jojobot_domain::attention::DUE_ON.to_string());
                }
                // **The stamp of a write by defrag**, a key on the new thing riding
                // its first claim, as it rides every other write by defrag.
                if let Some((key, moment)) = self.review_stamp(author) {
                    fields.insert(key.clone(), moment);
                    derived.insert(key);
                }
                // **Work created at `now` by the bot that owns it points the bot at
                // it**, as a capture or an edit of work does.
                moves_to_now = self.moves_to_now(&fields, &set_keys).await;
                let recorded_at = day;
                let first = jojobot_domain::memory::NewFact {
                    // **Every key the caller sent is a setting on the new thing**:
                    // the claim says them as the thing's own, and a later edit
                    // keeps each in that bag.
                    sets: set_keys,
                    derived,
                    fields: jojobot_domain::memory::review::settings_fields(fields),
                    // **The session that made it, as a capture records it**: a
                    // claim's first write names the run that wrote it, and a
                    // later rewrite of the claim is judged against that run.
                    session: by_session.map(str::to_string),
                    written_by_bot: Some(author.clone()),
                    // **A bot is born with the record of the notes it was told**, as a
                    // record of the engine's own beside the caller's claim, in the
                    // same act (rule 130).
                    also_written: match creating.kind() == Some(EntityKind::BOT) {
                        true => {
                            let mut ledger = crate::release_notes::ledger_record(
                                &creating,
                                &crate::release_notes::shipped_ids(),
                                recorded_at,
                            );
                            ledger.session = by_session.map(str::to_string);
                            vec![ledger]
                        }
                        false => Vec::new(),
                    },
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
                let creating = entity_as_it_will_be(&new);
                if let Some(refused) = self
                    .refuses_a_boot_floor_for_creation(&first, &creating, author, &floor_reads)
                    .await
                {
                    return memory_declined("add_entity", refused).map(Err);
                }
                // **A first claim no reader could be handed whole is refused
                // with the creation**, so nothing is made: the claim is measured
                // on the thing as it will exist, sets folded onto it.
                if let Some(refused) = self
                    .refuses_a_new_claim_too_big_to_read(&first, Some(&creating))
                    .await
                {
                    return memory_declined("add_entity", refused).map(Err);
                }
                let new_is_a_bot = new.id.kind() == Some(EntityKind::BOT);
                match armed_for(
                    new_is_a_bot,
                    self.memory.add_entity_with_first_claim(new, first),
                )
                .await
                {
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
                    Err(e) => return memory_declined("add_entity", e).map(Err),
                }
            }
        };
        if let (
            Some((mut retry, near)),
            Guarded::Blocked {
                attempted,
                candidates,
            },
        ) = (retry, &added)
            && attempted == &creating
            && candidates
                .iter()
                .any(|candidate| candidate.handle.as_str() == near)
        {
            retry.override_token = Some(guard::override_token(attempted, candidates));
            retry.override_reason = Some(format!(
                "The scenario declares {creating} and {near} to be distinct things."
            ));
            return Box::pin(self.add_entity_write(author, retry, day, by_session, None)).await;
        }
        let refused = match added {
            Guarded::Written(entity) => {
                let opened = self.open_box_with(&entity).await;
                return Ok(Ok(Added {
                    entity,
                    first_claim,
                    fold_behind,
                    before,
                    moves_to_now,
                    unread,
                    floor_reads,
                    floors_before,
                    due_moved,
                    opened,
                    dropped_keys,
                }));
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
                    "AddEntity",
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
                    "AddEntity",
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
                "AddEntity",
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
            } => {
                // **Each candidate carries what tells it apart**, when anyone said.
                let candidates = self.told_apart(candidates).await;
                Ok(blocked_result(
                    &attempted,
                    &candidates,
                    Blocked::Creating(token_slot),
                ))
            }
        };
        refused.map(Err)
    }
}
