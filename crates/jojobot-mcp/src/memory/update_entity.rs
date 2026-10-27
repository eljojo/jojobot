//! `update_entity` — Edit what an entity is called, and its other metadata, in place.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `update_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityArgs {
    /// The entity's handle. It says which entity to edit; this verb does not
    /// edit it. **With `reviewed`, several handles joined by commas** mark each
    /// thing reviewed in one call (at most 50), and take nothing else.
    pub(crate) handle: String,
    /// New display name.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// The whole alias set, replaced. Omit to leave it alone; pass `[]` to clear
    /// it. No commas.
    #[serde(default)]
    pub(crate) aliases: Option<Vec<String>>,
    /// New source.
    #[serde(default)]
    pub(crate) source: Option<String>,
    /// New cross-link to this entity in the task layer, in whatever form that
    /// layer addresses things. One reference, no space and no comma.
    #[serde(default)]
    pub(crate) crm: Option<String>,
    /// The token a previous call's refusal handed you, sent back after you read
    /// its candidates for the name or alias you are claiming here and judged
    /// them a different entity. It lifts only the refusal that minted it. Any
    /// change to what this entity is CALLED is screened exactly as a creation
    /// is.
    #[serde(default)]
    pub(crate) override_token: Option<String>,
    /// **Why you insist**, sent beside `override_token` when the refusal rests
    /// on the name alone: the same name under another kind, a name that holds or
    /// sits inside another, a known nickname of another, or two rare words shared
    /// with another of the same kind. Without it that refusal stands. The reason is
    /// kept on this entity as `told_apart_by`, shown beside its name.
    #[serde(default)]
    pub(crate) override_reason: Option<String>,
    /// **What this call sets on the thing**, as key/value pairs, written as one
    /// claim about it in the same act as the edit. The claim says what was set
    /// and carries these keys in its setting bag, and it faces every guard a
    /// `capture` does: a key the kind declares is held to what it declares, and a
    /// key that holds a handle must name a thing that exists. **Whole or not at
    /// all**: a key that is refused refuses the call, and the edit is not made.
    /// A call that sends only `sets` leaves the entity itself alone.
    ///
    /// **Sending again the keys your own earlier "Set ... on" claim holds, and no
    /// other claim does, rewrites that claim in place** rather than adding one;
    /// its earlier values stay readable in its history. Any other write adds a claim.
    ///
    /// The claim is `inference`, as a capture's is when it says nothing. To put
    /// the operator's own word behind a key, write it with `capture`.
    /// Setting `status` to `now` on work your bot owns also points your bot's
    /// `working_on` at it.
    /// A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone.
    #[serde(default)]
    pub(crate) sets: Option<std::collections::BTreeMap<String, String>>,
    /// **Mark the thing reviewed, and change nothing else.** Only the bot
    /// `defrag` may send it, and only `true`: jojobot stamps the thing with the
    /// moment, and it leaves the review queue (`recall` with `unreviewed`) until
    /// somebody else writes to it. Any other bot is refused. The mark is ONE
    /// standing record per thing, rewritten in place by every later mark, and it
    /// takes no slot under a record ceiling.
    ///
    /// **Several things at once**: send their handles joined by commas in
    /// `handle`. Each is marked as one handle would be, and the receipt's
    /// `reviewed` names how many and which. A handle that names nothing, or that
    /// cannot be edited (a voucher), refuses the call before any thing is marked.
    /// A list takes a `request_id`: if a store failure cuts it, send the same call
    /// again with the same `request_id` and it marks the rest and none twice. To
    /// read the queue one container at a time, `recall` with `unreviewed` and
    /// `parent`.
    #[serde(default)]
    pub(crate) reviewed: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    /// Receipt: `success`, `id`, any claim address, `derived`, `changed` and warnings; the calling bot's setting_token_saving=on (off by default) selects only id and derived values, deferring teaching and session notices; set it with capture sets on that bot and read claims with recall facts:true.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. If an answer is lost, send the
    /// same call again with the same `request_id`: if
    /// the first call wrote, you get its receipt back marked `repeat` and nothing
    /// is written a second time, so the edit is made once. It needs a `sid`, and a call that
    /// wrote nothing spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
}

/// **A deliberate, visible act**: this verb's own refusal is allowed to
/// promise an override because this impl says its `Args` really carries
/// one. See [`AcceptsOverride`].
impl AcceptsOverride for UpdateEntityArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
    fn override_reason(&self) -> Option<&str> {
        self.override_reason.as_deref()
    }
}

/// Edit an entity's metadata in place. The handle itself never changes, and
/// any change to what it is CALLED — name or aliases — is screened by the
/// write guard just as a creation is.
#[tool_router(router = update_entity_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Edit what an entity is called and where it came from (name/aliases/source/\
                       crm), in place. This verb does not edit the handle — its slug, kind or \
                       parent — use `rename_entity` for that. It does \
                       not touch mailboxes: a box belongs to the bot it is named for and opens \
                       with it, in add_entity (the operator's, at the first post to them). Any change to what it is CALLED — name or aliases — faces the same \
                       check a creation does, because an alias is a name: it can come back \
                       status: blocked with candidates, and the override_token that refusal \
                       carries is how you confirm a genuinely shared name — a token lifts the one \
                       refusal that minted it and no other. A refusal on the name alone also \
                       needs override_reason, shown beside the name. Passing `aliases` REPLACES the whole \
                       set ([] clears \
                       it); source and crm edits are never questioned. `sets` sets keys on the \
                       thing in the same call, through a claim of inference; a call that sends \
                       only `sets` leaves the entity itself alone, and a key that needs the \
                       operator's word is written with capture. A rule's `starred` mark stays \
                       on its claim, so it is not set here: update_fact on the rule's address \
                       with fields {starred: \"true\"} or {starred: \"false\"} stars or unstars it. On a record the \
                       build supplies (a shipped view), `name` and `aliases` are yours, laid over \
                       the build's: once you name it, the shipped name no longer answers (it \
                       returns only as an alias you list), so a new thing may take it; your \
                       aliases ADD to the build's and `[]` clears only yours; an upgrade leaves \
                       yours on top. `source` and `crm` stay the build's, refused with no \
                       token. A handle that names \
                       nothing comes back blocked with the nearest handles — it never creates. Aliases or `sets` that would leave the thing, or its claim, too big to \
                       be read back whole in one answer are refused with nothing written. The receipt's `changed` lists changes."
    )]
    pub(crate) async fn update_entity(
        &self,
        Parameters(args): Parameters<UpdateEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **What this write could not work out for want of the instance record**,
        // named on the receipt.
        let unread = super::instance_unread::InstanceUnread::default();
        // **An edit that ended what is owed and whose claim an earlier call wrote
        // is being finished**: the stored due moment comes off, and the checks that
        // judged the thing before the edit are not asked again.
        if let Some(recorded) = jojobot_domain::request::skip("claim") {
            return self.finish_the_entity_edit(&args, &caller, recorded).await;
        }
        // **Several handles mark several things reviewed**, each through the one
        // path a single mark takes (rule 498). A handle holds no comma, so a comma
        // is what tells the two apart.
        if args.handle.contains(',') {
            return self.mark_reviewed_in_bulk(args).await;
        }
        let handle = match self.row_handle(&EntityId::person(&args.handle)).await {
            Ok(handle) => handle,
            Err(e) => return memory_declined("update_entity", e),
        };
        // **The review stamp of a write by defrag**, and the one mark that sets
        // it alone. Only defrag marks a thing reviewed, and only with `true`.
        let stamp = self.review_stamp(&caller.bot);
        if let Some(reviewed) = args.reviewed
            && (!reviewed || stamp.is_none())
        {
            return Ok(blocked_body(
                "UpdateEntity",
                &handle,
                &[],
                "Nothing was written. `reviewed` marks a thing reviewed and only the bot \
                 `defrag` sends it, with `true`. Send the call without it.",
            ));
        }
        if let Some(refused) = refuses_an_edit_of(&handle) {
            return memory_declined("update_entity", refused);
        }
        // Taken before `args`' fields are moved into `patch` below — this
        // owns its own copy, so it survives the moves that follow.
        let token_slot = TokenSlot::from(&args);
        let patch = EntityPatch {
            name: args.name,
            aliases: args.aliases,
            source: args.source,
            crm: args.crm,
            session: Some(caller.sid.as_str().to_string()),
            override_token: args.override_token.clone(),
            override_reason: args.override_reason.clone(),
        };
        // **An override's reason is a key on the thing**, set in the same act as the
        // edit through the combined write below.
        let override_reason = kept_override_reason(
            args.override_token.as_deref(),
            args.override_reason.as_deref(),
        );
        // **What this call sets, when it sends anything**, is written as one claim
        // about the thing in the same act, through the store's own combined write:
        // the store runs the guards of a capture inside the transaction that edits
        // the entity, and a refusal takes the edit back with it.
        // **A name or aliases that leave the thing's own row too big to be read
        // back whole are refused before anything lands.**
        if let Some(refused) = self
            .refuses_a_relabelling_too_big_to_read(&handle, &patch)
            .await
        {
            return memory_declined("update_entity", refused);
        }
        // **A bot's name and aliases ride in its own boot**, which a boot never
        // declines: an edit that takes the floor over the ceiling is refused.
        let floor_reads = super::reads_left::ReadsLeft::default();
        if let Some(refused) = self
            .refuses_a_boot_floor_for_relabelling(&handle, &patch, &floor_reads)
            .await
        {
            return memory_declined("update_entity", refused);
        }
        // **The thing as it stands before the write**, for the receipt's account
        // of what the write changed.
        let before = self.as_it_stands(&handle).await;
        // **A write by defrag stamps the thing, and an edit of the row has no
        // record to carry a key**, so it is written as the claim an edit with
        // `sets` is, and the stamp rides that.
        let edits_the_row = patch.name.is_some()
            || patch.aliases.is_some()
            || patch.source.is_some()
            || patch.crm.is_some();
        // **A thing is marked by ONE standing record**, rewritten in place by every
        // later mark, so a mark neither grows the thing nor leaves a trail of
        // them. The rewrite is defrag's own `update_fact`, which carries the new
        // stamp as any write by defrag does.
        let marks_only = args.reviewed == Some(true)
            && !edits_the_row
            && args.sets.as_ref().is_none_or(|sets| sets.is_empty());
        if marks_only
            && let Some(standing) = self.memory.recall(&handle).await.ok().and_then(|facts| {
                facts.into_iter().find(|fact| {
                    fact.status == FactStatus::Active
                        && jojobot_domain::memory::review::is_review_mark(&fact.fields)
                })
            })
        {
            return self
                .update_fact(Parameters(super::update_fact::UpdateFactArgs {
                    address: standing.address().to_string(),
                    sid: args.sid.clone(),
                    ..Default::default()
                }))
                .await;
        }
        // **Setting again what this bot's own "Set ... on" record already holds
        // rewrites that record** instead of piling up another (rule 396).
        if !edits_the_row
            && args.reviewed.is_none()
            && override_reason.is_none()
            && stamp.is_none()
            && let Some(sets) = args.sets.as_ref().filter(|sets| !sets.is_empty())
            && let Some((own, revision)) = self.own_set_record(&handle, sets, &caller.bot).await
        {
            let mut guard = super::update_fact::RewriteGuard {
                must_back: sets.keys().cloned().collect(),
                stale: false,
            };
            let answered = self
                .update_fact_checking_revision(
                    super::update_fact::UpdateFactArgs {
                        address: own.to_string(),
                        sets: Some(sets.clone()),
                        expected_revision: Some(revision),
                        sid: args.sid.clone(),
                        ..Default::default()
                    },
                    Some(&mut guard),
                )
                .await?;
            if !guard.stale {
                return Ok(answered);
            }
            // Another write invalidated the ownership check; append instead.
        }
        let sets = {
            let mut sets = args.sets.clone().unwrap_or_default();
            if let Some(reason) = override_reason {
                // **Added beside the reason the thing already holds**, read in this
                // write, never written over it. A read that fails refuses the call:
                // taken as "none held", it would write the new reason over them.
                let held = match self.memory.fields(&handle).await {
                    Ok(held) => held.get(guard::TOLD_APART_BY).cloned(),
                    Err(e) => return memory_declined("update_entity", e),
                };
                // A reason the caller sent under the key joins them too.
                let held = match sets.get(guard::TOLD_APART_BY) {
                    Some(sent) => sent
                        .split(guard::TOLD_APART_JOINER)
                        .fold(held, |held, part| {
                            Some(guard::add_told_apart_by(held.as_deref(), part))
                        }),
                    None => held,
                };
                let merged = guard::add_told_apart_by(held.as_deref(), reason);
                sets.insert(guard::TOLD_APART_BY.to_string(), merged);
            }
            (!sets.is_empty()
                || (stamp.is_some() && (args.reviewed == Some(true) || edits_the_row)))
                .then_some(sets)
        };
        let mut claim = None;
        let mut fold_behind = None;
        let mut due_moved = jojobot_domain::attention::DueMove::Unchanged;
        let mut moves_to_now = Ok(false);
        let written = match sets {
            None => match jojobot_domain::request::armed(self.memory.update_entity(&handle, patch))
                .await
            {
                Ok(written) => written,
                Err(e) => return memory_declined("update_entity", e),
            },
            Some(mut fields) => {
                // **The checks a capture makes before it writes**, on what the
                // caller sent: a role's own two fields are the boot door's, and
                // the stored due moment is jojobot's own.
                if let Some(refused) =
                    jojobot_domain::memory::refuses_role_fields(&handle, fields.keys())
                {
                    return memory_declined("update_entity", refused);
                }
                let named: Vec<&str> = fields.keys().map(String::as_str).collect();
                if let Some(done) = self
                    .refuses_placement("update_entity", &handle, None, &named)
                    .await
                {
                    return done;
                }
                // **A guarded key is licensed here as on a capture**, through
                // the same call: the store's combined write takes no caller, so
                // this is the only place an update's `sets` is judged.
                if let Some(refused) = self
                    .refuses_an_unlicensed_key("update_entity", &handle, true, &caller.bot, &fields)
                    .await?
                {
                    return Ok(refused);
                }
                if let Some(refused) = self
                    .refuses_a_hand_written_due_moment(&handle, &fields, &[])
                    .await
                {
                    return Ok(refused);
                }
                if let Some(refused) =
                    self.refuses_a_hand_written_review_stamp(&handle, &fields, &[])
                {
                    return Ok(refused);
                }
                // **The keys the caller sent are the settings.** The due moment worked
                // out below is jojobot's own arithmetic beside them, kept as one of
                // the claim's own fields.
                let set_keys: std::collections::BTreeSet<String> = fields.keys().cloned().collect();
                let (due_on, _) = self
                    .moved_due_moment(
                        &handle,
                        &fields,
                        &[],
                        self.dated(None, args.sid.as_deref()).await?,
                        &unread,
                    )
                    .await;
                due_moved = due_on;
                // **The due moment reaches the thing**: it is jojobot's own
                // arithmetic, not a setting the caller sent.
                let mut derived: std::collections::BTreeSet<String> = Default::default();
                if let jojobot_domain::attention::DueMove::Set(due_on) = due_on {
                    fields.insert(
                        jojobot_domain::attention::DUE_ON.to_string(),
                        due_on.to_string(),
                    );
                    derived.insert(jojobot_domain::attention::DUE_ON.to_string());
                }
                if let Some((key, moment)) = stamp {
                    fields.insert(key.clone(), moment);
                    derived.insert(key);
                }
                // **The label stays on the record** (it is neither a setting nor
                // derived), so it never reaches the thing.
                if marks_only {
                    fields.insert(
                        jojobot_domain::memory::review::REVIEW_MARK.to_string(),
                        "true".to_string(),
                    );
                }
                let keys = set_keys.iter().cloned().collect::<Vec<_>>();
                // **Work moved to `now` by the bot that owns it points the bot at
                // it**, as a capture or an edit of a claim does.
                moves_to_now = self.moves_to_now(&fields, &set_keys).await;
                let recorded_at = self.dated(None, args.sid.as_deref()).await?;
                let first = jojobot_domain::memory::NewFact {
                    sets: set_keys,
                    derived,
                    fields: jojobot_domain::memory::review::settings_fields(fields),
                    session: Some(caller.sid.as_str().to_string()),
                    ..jojobot_domain::memory::NewFact::about(
                        handle.clone(),
                        // **What the generated claim says**: which keys were set,
                        // on the thing as a link, so the words follow a rename.
                        match keys.is_empty() {
                            true => format!("Reviewed @{}.", handle.as_str()),
                            false => format!("Set {} on @{}.", keys.join(", "), handle.as_str()),
                        },
                        recorded_at,
                    )
                };
                // **A call that relabels the thing as well as setting keys is
                // measured as the one thing it would leave**, not as two writes
                // each beside the thing as it stands.
                let too_big = if patch.name.is_some() || patch.aliases.is_some() {
                    self.refuses_a_relabelling_with_a_claim_too_big_to_read(&handle, &patch, &first)
                        .await
                } else {
                    self.refuses_a_new_claim_too_big_to_read(&first, None).await
                };
                if let Some(refused) = too_big {
                    return memory_declined("update_entity", refused);
                }
                // **A star or a seat count set here is a capture's**: the claim
                // is measured as the boot it would leave, as `capture` does.
                if let Some(refused) = self
                    .refuses_a_boot_floor_for_capture(&first, &caller.bot, &floor_reads)
                    .await
                {
                    return memory_declined("update_entity", refused);
                }
                match super::due::writing_a_claim(
                    matches!(due_moved, jojobot_domain::attention::DueMove::Cleared),
                    self.memory.update_entity_with_claim(&handle, patch, first),
                )
                .await
                {
                    Ok(Guarded::Written((entity, fact))) => {
                        claim = Some(fact);
                        Guarded::Written(entity)
                    }
                    Ok(Guarded::Blocked {
                        attempted,
                        candidates,
                    }) => Guarded::Blocked {
                        attempted,
                        candidates,
                    },
                    // **A write that landed is never reported as failed**
                    // (rule 130), the same as on a capture.
                    Err(MemoryError::FoldBehind {
                        landed: Landed::Creation(created),
                        behind,
                        ..
                    }) => {
                        let (entity, fact) = *created;
                        fold_behind = Some(behind);
                        claim = Some(fact);
                        Guarded::Written(entity)
                    }
                    Err(e) => return memory_declined("update_entity", e),
                }
            }
        };
        match written {
            Guarded::Written(entity) => {
                // **A write that moved the thing out of owing anything takes the
                // stored due moment off**, as a capture and an edit do: it sits on
                // the record that carried the key it came from, not always the
                // claim this call wrote. See `clear_stored_due_moment`.
                if matches!(due_moved, jojobot_domain::attention::DueMove::Cleared)
                    && let Some(unfinished) = self
                        .end_what_is_owed("update_entity", &entity.id, &caller)
                        .await
                {
                    return Ok(unfinished);
                }
                self.beat("update_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
                self.fill_working_on(&caller, &entity.id, &moves_to_now, &unread)
                    .await;
                let mut body = entity_json(&entity);
                // **The claim's receipt**, so the claim has an address a later edit
                // goes through, and the answer says the claim exists.
                if let (Some(fact), Some(obj)) = (&claim, body.as_object_mut()) {
                    obj.insert(
                        "claim".into(),
                        fact_receipt_json(fact, self.dated(None, args.sid.as_deref()).await?),
                    );
                }
                if let Some(fact) = &claim {
                    self.note_status_word(fact, &mut body, &unread).await;
                    unread.note(&mut body);
                    self.note_depends_on_read(fact, &caller, &mut body).await;
                }
                let changed = match fold_behind {
                    Some(_) => crate::changed::Changed::not_read(),
                    None => self.changed_on(&before, &entity.id).await,
                };
                crate::changed::note_changed(&mut body, changed);
                floor_reads.note(&mut body);
                self.note_view_narrows_nothing(&mut body, &entity.id).await;
                if let Some(behind) = fold_behind {
                    crate::answer::note_fold_behind(&mut body, behind);
                }
                // **The line a write ends with, when it gave the thing a day to
                // fall due on.** Absent otherwise: a quiet answer says only what
                // changed.
                if let Some(due) = crate::answer::falls_due_sentence(&entity.id, due_moved) {
                    crate::answer::note_postcondition(&mut body, due);
                }
                // **No reason given, and that is the honest answer.**
                // Reading a bare handle as a person is what the argument
                // means; a sentence restating the comparison would be a
                // manufactured justification.
                crate::answer::note_delta(
                    &mut body,
                    crate::answer::Difference::between(
                        "handle",
                        Some(&args.handle),
                        entity.id.as_str(),
                    )
                    .into_iter()
                    .collect(),
                );
                crate::memory::wire::elide_aliases_past_the_ceiling(&mut body);
                json_result(&body)
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                // **Each candidate carries what tells it apart**, when anyone said.
                let candidates = self.told_apart(candidates).await;
                Ok(blocked_result(
                    &attempted,
                    &candidates,
                    Blocked::Relabelling(token_slot),
                ))
            }
        }
    }

    /// **The end of an edit whose claim an earlier call wrote**: the stored due
    /// moment is taken off the finished thing, as the plan in the claim's note
    /// says, and the receipt is built from the thing and the claim as they stand.
    async fn finish_the_entity_edit(
        &self,
        args: &UpdateEntityArgs,
        caller: &crate::caller::Caller,
        recorded: jojobot_domain::request::Recorded,
    ) -> Result<CallToolResult, McpError> {
        let gone = |what: &str| {
            Ok(crate::caller::handle_declined(
                "update_entity",
                crate::answer::WayForward::person(format!(
                    "The edit was written under this request_id, but {what}, so the rest \
                     cannot be finished."
                )),
            ))
        };
        let (entity, address) = match self.recorded_claim_target(&recorded).await {
            Ok(target) => target,
            Err(what) => return gone(what),
        };
        let Some(claim) = self
            .memory
            .recall(&address.home)
            .await
            .ok()
            .and_then(|facts| facts.into_iter().find(|fact| fact.address() == address))
        else {
            return gone("its claim cannot be read back");
        };
        if recorded.note.as_deref() == Some(super::due::CLEARS_DUE_ON) {
            if let Some(unfinished) = self
                .end_what_is_owed("update_entity", &entity.id, caller)
                .await
            {
                return Ok(unfinished);
            }
        } else {
            jojobot_domain::request::finished();
        }
        self.beat("update_entity", entity.id.as_str(), args.sid.as_deref())
            .await;
        let mut body = entity_json(&entity);
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "claim".into(),
                fact_receipt_json(&claim, self.dated(None, args.sid.as_deref()).await?),
            );
        }
        crate::request::note_resumed(&mut body);
        crate::changed::note_changed(&mut body, crate::changed::Changed::not_read());
        json_result(&body)
    }
}

/// **Whether an edit of this handle is refused whatever it carries.** A voucher is
/// issued, never edited, and judged on the handle's kind. A single edit and every
/// item of a list ask here (rule 498), so a list is refused whole by the item the
/// single edit would refuse.
fn refuses_an_edit_of(handle: &EntityId) -> Option<MemoryError> {
    jojobot_domain::voucher::refuses_engine_only_write(handle)
}

impl Jojobot {
    /// **Mark every thing a comma list names reviewed**, in the order sent. Each
    /// thing goes through the single mark ([`Jojobot::update_entity`] with one
    /// handle), so each gets its own one review mark and leaves the queue on its
    /// own. Every handle is judged first, so a handle that names nothing, or that no
    /// edit may reach, refuses the call before any thing is marked. Under a
    /// `request_id` each mark is one
    /// part of the write: a call cut partway is finished by the same call again,
    /// and no thing is marked twice under that id.
    async fn mark_reviewed_in_bulk(
        &self,
        args: UpdateEntityArgs,
    ) -> Result<CallToolResult, McpError> {
        use jojobot_domain::request;
        let listed = EntityId(args.handle.clone());
        // **Who may mark is the single mark's question**: another bot's first mark
        // is refused there, before any thing is marked.
        if args.reviewed != Some(true) {
            return Ok(blocked_body(
                "MarkReviewedInBulk",
                &listed,
                &[],
                "Nothing was written. A list of handles marks things reviewed, and only the \
                 bot `defrag` sends it, with `reviewed: true`. Send one handle for any other \
                 edit.",
            ));
        }
        if args.name.is_some()
            || args.aliases.is_some()
            || args.source.is_some()
            || args.crm.is_some()
            || args.sets.is_some()
            || args.override_token.is_some()
            || args.override_reason.is_some()
        {
            return Ok(blocked_body(
                "MarkReviewedInBulk",
                &listed,
                &[],
                "Nothing was written. A list of handles takes `reviewed` and nothing else, \
                 because every other edit is made to one thing. Send the other edit with one \
                 handle.",
            ));
        }
        let mut handles: Vec<String> = Vec::new();
        for item in args.handle.split(',').map(str::trim) {
            if item.is_empty() {
                return Ok(blocked_body(
                    "MarkReviewedInBulk",
                    &listed,
                    &[],
                    "Nothing was written. The list holds an empty item. Send the handles \
                     joined by commas, with nothing between two commas.",
                ));
            }
            if !handles.iter().any(|held| held == item) {
                handles.push(item.to_string());
            }
        }
        if handles.len() > super::capture::BATCH_CAP {
            return Ok(blocked_body(
                "MarkReviewedInBulk",
                &listed,
                &[],
                format!(
                    "Nothing was written. The list names {} things, and one call marks at \
                     most {}. Send the rest in another call.",
                    handles.len(),
                    super::capture::BATCH_CAP
                ),
            ));
        }
        // **The whole list is judged before the first thing is marked**: a handle
        // that names nothing, and a handle no edit may reach, each refuse the call
        // with nothing marked. The second question is the single mark's own.
        for handle in &handles {
            if let Err(missing) = self.memory.recall(&EntityId::person(handle)).await {
                return memory_declined("update_entity", missing);
            }
            if let Some(refused) = refuses_an_edit_of(&EntityId::person(handle)) {
                return memory_declined("update_entity", refused);
            }
        }
        let mut changed = crate::changed::Changed::default();
        for (at, handle) in handles.iter().enumerate() {
            let id = EntityId::person(handle);
            let before = self.as_it_stands(&id).await;
            let single = UpdateEntityArgs {
                handle: handle.clone(),
                name: None,
                aliases: None,
                source: None,
                crm: None,
                override_token: None,
                override_reason: None,
                sets: None,
                reviewed: Some(true),
                sid: args.sid.clone(),
                request_id: None,
            };
            let part = format!("mark-{at}");
            match request::part(
                &part,
                None,
                Box::pin(self.update_entity(Parameters(single))),
            )
            .await
            {
                request::Part::Ran(answered) => {
                    let answered = answered?;
                    let body = answered
                        .content
                        .first()
                        .and_then(|block| block.as_text())
                        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text.text).ok())
                        .unwrap_or_default();
                    // **A mark that did not land stops the list there.** Under an
                    // id the call is not finished, and the same call marks the rest.
                    if answered.is_error == Some(true) || body["status"] == "blocked" {
                        return Ok(answered);
                    }
                    changed = changed.and(self.changed_on(&before, &id).await);
                }
                request::Part::Recorded(_) => {}
            }
        }
        // **Every thing in the list holds its mark**, so the id is spent now.
        request::finished();
        let mut body = serde_json::json!({
            "reviewed": { "count": handles.len(), "handles": handles },
        });
        let skipped = request::skipped();
        if !skipped.is_empty() {
            body["finished_a_cut_list"] = serde_json::json!({
                "already_marked": skipped.len(),
                "marked_now": handles.len() - skipped.len(),
            });
        }
        crate::changed::note_changed(&mut body, changed);
        crate::answer::note_postcondition(
            &mut body,
            format!(
                "{} things are marked reviewed, each with its own review mark.",
                handles.len()
            ),
        );
        json_result(&body)
    }
}

impl Jojobot {
    /// **The record an `update_entity` that sets `sets` may rewrite in place**,
    /// when there is one. All of this holds, and anything else appends:
    /// - for every key sent, the newest active record holding it is the same one;
    /// - that record is a plain "Set ... on" record of exactly these keys, with
    ///   no other field except its engine settings label, and it is an inference;
    /// - no key sent is a counter, and every write of the record was made by a
    ///   run of `bot`.
    ///
    /// A doubt (a record or a run that cannot be read) answers `None`, so the
    /// write appends.
    async fn own_set_record(
        &self,
        handle: &EntityId,
        sets: &std::collections::BTreeMap<String, String>,
        bot: &EntityId,
    ) -> Option<(FactAddress, u32)> {
        // **The record holding a key's newest write is the store's answer about the
        // key**, never the last record a listing happens to end with: a store may
        // list records by id as text, where `f10` comes before `f9`.
        let backing = self.memory.backing(handle).await.ok()?;
        let newest = &backing.get(sets.keys().next()?)?.fact;
        if sets
            .keys()
            .any(|key| backing.get(key).map(|held| &held.fact) != Some(newest))
        {
            return None;
        }
        // Pin the revision before reading eligibility. The write validates it
        // atomically, so a shape read across an intervening edit cannot borrow
        // that edit's newer revision and pass the guard.
        let address = FactAddress::new(handle.clone(), newest.clone());
        let chain = self.memory.claim_history(&address).await.ok()?;
        let revision = u32::try_from(chain.len()).ok()?;
        let facts = self.memory.recall(handle).await.ok()?;
        let candidate = facts
            .iter()
            .find(|fact| fact.status == FactStatus::Active && &fact.id == newest)?;
        let keys: Vec<&String> = sets.keys().collect();
        let plain = candidate.provenance == Provenance::Inference
            && candidate.content
                == format!(
                    "Set {} on @{}.",
                    keys.iter()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    handle.as_str()
                )
            && candidate
                .fields
                .keys()
                .filter(|key| key.as_str() != jojobot_domain::memory::review::SETTINGS_RECORD)
                .eq(keys.iter().copied())
            && candidate.sets.iter().eq(keys.iter().copied());
        if !plain {
            return None;
        }
        // **A counter key always appends**: its value is the sum of its
        // records, so a rewrite would drop the earlier write from the total.
        let declared = self.memory.declared_types().await.ok()?;
        if sets.keys().any(|key| {
            jojobot_domain::memory::types::fold_of(key, &declared)
                == jojobot_domain::memory::types::Fold::Sum
        }) {
            return None;
        }
        // **Every write of the record was made by a run of this bot**, not only
        // the first: a record another bot edited since holds that bot's values.
        let mut runs: Vec<&String> = Vec::new();
        for write in &chain {
            let run = write.session.as_ref()?;
            if !runs.contains(&run) {
                runs.push(run);
            }
        }
        for run in runs {
            let writer = self
                .sessions
                .summary_of(&jojobot_domain::session::SessionId(run.clone()))
                .await
                .ok()??;
            if writer.bot != *bot {
                return None;
            }
        }
        Some((address, revision))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// An old cut receipt has no immutable owner. A person must repair it;
    /// a receipt keeping that identity finishes the same remaining clear.
    #[tokio::test]
    async fn a_repeated_set_with_a_legacy_cut_receipt_requires_a_person() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                sets: Some([("runs_out".into(), "2099-12-01".into())].into()),
                ..capture_args("work:handcart", "the cart is due")
            },
        )
        .await;
        let address = captured["address"].as_str().expect("the claim address");
        let args: UpdateEntityArgs = serde_json::from_value(serde_json::json!({
            "handle": "work:handcart", "sets": {"status": "done"}, "sid": sid
        }))
        .unwrap();
        let caller = crate::caller::Caller {
            sid: crate::sid::Sid(TEST_RUN.into()),
            bot: EntityId("bot:otto".into()),
            card: None,
            zone: None,
            day: None,
        };
        let note = Some(super::super::due::CLEARS_DUE_ON.to_string());
        let refused = json_of(
            &jojobot
                .finish_the_entity_edit(
                    &args,
                    &caller,
                    jojobot_domain::request::Recorded {
                        landed: serde_json::json!({"address": address}).to_string(),
                        note: note.clone(),
                    },
                )
                .await
                .unwrap(),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["fix_by"], "person", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        let way = refused["how_to_proceed"].as_str().unwrap();
        assert!(
            way.contains("operator") && way.contains("identity") && way.contains("request"),
            "{way}"
        );
        assert_eq!(
            fields_of(&jojobot, "work:handcart").await["due_on"],
            "2099-12-01"
        );

        let entity = jojobot
            .entity_now(&EntityId("work:handcart".into()))
            .await
            .unwrap();
        let finished = json_of(
            &jojobot
                .finish_the_entity_edit(
                    &args,
                    &caller,
                    jojobot_domain::request::Recorded {
                        landed: serde_json::json!({"address": address, "entity": entity.badge})
                            .to_string(),
                        note,
                    },
                )
                .await
                .unwrap(),
        );
        assert_eq!(finished["id"], "work:handcart", "{finished}");
        assert!(
            fields_of(&jojobot, "work:handcart")
                .await
                .get("due_on")
                .is_none()
        );
        assert!(
            finished.get("entity").is_none(),
            "the private identity is never served"
        );
    }

    /// 🚨 **A bare handle is read as a person, and the receipt says so.**
    ///
    /// `capture` already announces this same substitution on its subject. It
    /// was silent here, so the identical conversion was a documented one on one
    /// verb and invisible on another — and a caller that meets one unannounced
    /// substitution has to price every write at its worst case.
    ///
    /// **Paired with a handle that needed no reading.** A caller sending the
    /// qualified form gets no `delta` at all: a line on every call is one a
    /// reader learns to skip, and it would be gone from view on the call that
    /// needed it.
    ///
    /// **No reason, and that is the honest answer.** `capture`'s provenance
    /// substitution has a real one behind it; reading a bare handle as a person
    /// is what the argument means, so `because` is absent rather than filled
    /// with a sentence restating the comparison.
    #[tokio::test]
    async fn a_bare_handle_is_reported_as_read_and_a_qualified_one_is_not() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:alpha").await;
        let rename = |handle: &str, name: &str| UpdateEntityArgs {
            reviewed: None,
            request_id: None,
            handle: handle.into(),
            name: Some(name.into()),
            aliases: None,
            source: None,
            crm: None,
            sets: None,
            override_token: None,
            override_reason: None,
            sid: Some(sid.clone()),
        };

        let bare = json_of(
            &jojobot
                .update_entity(Parameters(rename("alpha", "Alpha the first")))
                .await
                .expect("update_entity ok"),
        );
        assert_eq!(bare["id"], "person:alpha", "{bare}");
        assert_eq!(bare["delta"][0]["field"], "handle", "{bare}");
        assert_eq!(bare["delta"][0]["sent"], "alpha", "{bare}");
        assert_eq!(bare["delta"][0]["stored"], "person:alpha", "{bare}");
        assert_eq!(
            bare["delta"][0]["because"],
            serde_json::Value::Null,
            "a difference with nothing to explain carries no explanation: {bare}",
        );

        let qualified = json_of(
            &jojobot
                .update_entity(Parameters(rename("person:alpha", "Alpha the second")))
                .await
                .expect("update_entity ok"),
        );
        assert_eq!(qualified["id"], "person:alpha", "{qualified}");
        assert!(
            qualified.get("delta").is_none(),
            "a handle that needed no reading carries no delta: {qualified}",
        );
    }

    /// `update_entity` edits metadata and leaves the handle alone.
    #[tokio::test]
    async fn update_entity_edits_metadata() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let updated = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                reviewed: None,
                request_id: None,
                handle: "thing:red-bike".into(),
                name: Some("Red Bike (the gravel one)".into()),
                aliases: None,
                source: None,
                crm: Some("card:551".into()),
                sets: None,
                override_token: None,
                override_reason: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("update ok");
        let body = json_of(&updated);
        assert_eq!(body["id"], "thing:red-bike", "the handle is immutable");
        assert_eq!(body["name"], "Red Bike (the gravel one)");
        assert_eq!(
            body["source"], "user-named",
            "an omitted field is left alone"
        );
    }

    /// A rename onto a name the index already holds comes back as the same
    /// error-flagged candidates response a blocked creation does — the guard
    /// cannot be side-stepped by creating under a throwaway name and renaming.
    #[tokio::test]
    async fn a_rename_onto_an_existing_name_is_blocked() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let rename = |override_token: Option<String>| UpdateEntityArgs {
            reviewed: None,
            override_reason: None,
            request_id: None,
            handle: "person:zenith".into(),
            name: Some("Alpha".into()),
            aliases: None,
            source: None,
            crm: None,
            override_token,
            sets: None,
            sid: Some(crate::harness::TEST_SID.into()),
        };

        let result = jojobot
            .update_entity(Parameters(rename(None)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        let token = token_in(body["how_to_proceed"].as_str().expect("advice is a string"));

        // …and the name did not move.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    parent: None,
                    offset: None,
                    sid: Some(crate::harness::TEST_SID.into()),
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

        // A token nobody minted must not move it. Without this half, the
        // assertion below holds identically on a build that accepts any string.
        let invented = json_of(
            &jojobot
                .update_entity(Parameters(rename(Some("0000000000000000".into()))))
                .await
                .expect("the call succeeds; the guard answers in the body"),
        );
        assert_eq!(
            invented["status"], "blocked",
            "a token nobody minted moves nothing: {invented}"
        );

        let forced = json_of(
            &jojobot
                .update_entity(Parameters(rename(Some(token))))
                .await
                .expect("confirmed rename ok"),
        );
        assert_ne!(forced["status"], "blocked");
        assert_eq!(forced["name"], "Alpha");
    }

    /// **The token as a caller gets it: out of the refusal's own advice.**
    ///
    /// Recomputing it from the guard would prove the guard agrees with itself
    /// and would pass on a build that mints a token and tells nobody, which is
    /// a secret rather than a mechanism (rule 68). Sixteen hex digits is the
    /// token's shape, not a sentence somebody wrote, so this survives the
    /// advice being reworded.
    fn token_in(advice: &str) -> String {
        let chars: Vec<char> = advice.chars().collect();
        chars
            .windows(16)
            .find(|w| w.iter().all(char::is_ascii_hexdigit))
            .map(|w| w.iter().collect())
            .unwrap_or_else(|| panic!("the refusal must carry the token it minted: {advice}"))
    }

    /// The guard's last door, through the real handler: a patch carrying
    /// only aliases renames nothing, so it must still be screened, and the
    /// advice it gets back must not describe a rename the caller never made.
    #[tokio::test]
    async fn an_alias_onto_a_taken_name_is_blocked_and_says_so_in_its_own_words() {
        let jojobot = handler();
        for (handle, name) in [("homer-simpson", "Homer Simpson"), ("zenith", "Zenith")] {
            jojobot
                .add_entity(Parameters(add_args("person", handle, name)))
                .await
                .expect("add ok");
        }

        let result = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                reviewed: None,
                request_id: None,
                handle: "person:zenith".into(),
                name: None,
                aliases: Some(vec!["Homer Simpson".into()]),
                source: None,
                crm: None,
                sets: None,
                override_token: None,
                override_reason: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:homer-simpson");
        let advice = body["how_to_proceed"].as_str().expect("advice is a string");
        assert!(
            advice.contains("alias"),
            "the advice must name the thing that was actually refused: {advice}"
        );
        assert!(
            !advice.contains("renamed"),
            "nothing was renamed — telling them so sends them hunting for a rename: {advice}"
        );

        // …and the alias did not land.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    parent: None,
                    offset: None,
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("list ok"),
        );
        let zenith = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "person:zenith")
            .expect("zenith is still there");
        assert_eq!(
            zenith["alternateName"].as_array().map(Vec::len),
            Some(0),
            "a blocked alias write lands nothing: {zenith}"
        );
    }

    /// **Alternate names go in and come back**, under schema.org's word for
    /// them. `update_entity` replaces the set whole — including with nothing,
    /// because "it has none" is a thing a caller must be able to say.
    #[tokio::test]
    async fn an_entity_carries_its_alternate_names_through_the_handler() {
        let jojobot = handler();
        let added = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    aliases: Some(vec!["Max Power".into(), "H.".into()]),
                    ..add_args("person", "homer-simpson", "Homer Simpson")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(added["alternateName"][0], "Max Power");
        assert_eq!(added["alternateName"][1], "H.");

        let patch = |aliases: Vec<String>| UpdateEntityArgs {
            reviewed: None,
            request_id: None,
            handle: "person:homer-simpson".into(),
            name: None,
            aliases: Some(aliases),
            source: None,
            crm: None,
            sets: None,
            override_token: None,
            override_reason: None,
            sid: Some(crate::harness::TEST_SID.into()),
        };

        let replaced = json_of(
            &jojobot
                .update_entity(Parameters(patch(vec!["Max Power".into()])))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            replaced["alternateName"].as_array().expect("a list").len(),
            1,
            "the set is replaced, not appended to: {replaced}"
        );

        let cleared = json_of(
            &jojobot
                .update_entity(Parameters(patch(Vec::new())))
                .await
                .expect("update ok"),
        );
        assert!(
            cleared["alternateName"]
                .as_array()
                .expect("a list")
                .is_empty()
        );

        // An alias carrying the separator is refused, not silently split — and
        // refused as a blocked answer, because it is the caller's mistake
        // (rule 68).
        let refused = blocked(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    aliases: Some(vec!["one, two".into()]),
                    ..add_args("person", "contract-comma-carrier", "Comma Carrier")
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// Updating an entity that isn't there is a client error naming near misses
    /// — it never creates one.
    #[tokio::test]
    async fn update_entity_unknown_handle_is_a_client_error() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let err = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                reviewed: None,
                request_id: None,
                handle: "thing:red-bikee".into(),
                name: Some("nope".into()),
                aliases: None,
                source: None,
                crm: None,
                sets: None,
                override_token: None,
                override_reason: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("an unknown handle is an answer, not a protocol failure");
        let body = blocked(&err);
        assert_eq!(body["attempted"], "thing:red-bikee");
        assert_eq!(
            body["candidates"][0]["handle"], "thing:red-bike",
            "must name the near miss: {body}"
        );
    }
    /// The call a caller sends to edit a record: only the pieces it names.
    fn edit(handle: &str) -> UpdateEntityArgs {
        UpdateEntityArgs {
            reviewed: None,
            request_id: None,
            handle: handle.into(),
            name: None,
            aliases: None,
            source: None,
            crm: None,
            override_token: None,
            override_reason: None,
            sets: None,
            sid: Some(crate::harness::TEST_SID.into()),
        }
    }

    /// A run of sixteen hex digits, the shape of the token a refusal mints.
    /// Asked of the shape and not of a sentence, so a refusal reworded still
    /// answers the question: does this refusal hand out a token.
    fn carries_a_token(advice: &str) -> bool {
        advice
            .chars()
            .collect::<Vec<_>>()
            .windows(16)
            .any(|w| w.iter().all(char::is_ascii_hexdigit))
    }

    /// 🚨 **A refusal on a record the build ships never offers a token, and names
    /// the write that works.** The source and crm are the build's own, so no token
    /// lifts the refusal. One that carried a token sent the caller round a loop:
    /// the resend was refused again, with a new token, for ever. (The name and
    /// aliases are the operator's to set: see
    /// `the_operator_names_a_shipped_record_and_it_reads_and_searches_by_both`.)
    ///
    /// **Three things at once.** Every field of the edit that the build keeps is
    /// refused with no token; a resend with a token the caller invented, which is
    /// the move the old advice asked for, is refused the same way and writes
    /// nothing; and the write the refusal names lands on the same record, so the
    /// way forward exists. Without the last, a refusal that named nothing real
    /// would pass the first two.
    #[tokio::test]
    async fn a_shipped_records_source_and_crm_refusal_offers_no_token_and_names_a_write_that_lands()
    {
        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);

        let calls = [
            (
                "source",
                UpdateEntityArgs {
                    source: Some("me".into()),
                    ..edit("view:loops")
                },
            ),
            (
                "crm",
                UpdateEntityArgs {
                    crm: Some("ref-1".into()),
                    ..edit("view:loops")
                },
            ),
            (
                "a token nobody minted",
                UpdateEntityArgs {
                    source: Some("me".into()),
                    override_token: Some("0123456789abcdef".into()),
                    ..edit("view:loops")
                },
            ),
        ];
        for (label, args) in calls {
            let result = jojobot
                .update_entity(Parameters(args))
                .await
                .expect("a shipped record's refusal is an answer");
            let body = blocked(&result);
            let way = body["how_to_proceed"].as_str().expect("advice is a string");
            assert!(
                !way.contains("override_token") && !carries_a_token(way),
                "{label}: a refusal nothing can lift offers no token: {body}",
            );
            assert!(
                way.contains("capture"),
                "{label}: the refusal names the write that works: {body}",
            );
            assert_eq!(body["fix_by"], "change", "{label}: {body}");
        }

        // Nothing moved: the name the build ships is the name that reads back.
        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid.clone()),
                    ..recall_args("view:loops")
                }))
                .await
                .expect("recall answers"),
        );
        assert_eq!(read["objects"][0]["name"], "The Loops", "{read}");

        // The write the refusal names, made on the same record, lands.
        let wrote = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(sid),
                    sets: Some([("note".to_string(), "kept on top".to_string())].into()),
                    ..capture_args("view:loops", "A caller's own note on the loops view")
                }))
                .await
                .expect("a write to a shipped record answers"),
        );
        assert_ne!(wrote["status"], "blocked", "{wrote}");
    }

    /// **A sets-only edit of a record the build ships lands on top of it**, as a
    /// capture with sets does, and what the build ships stays under it.
    ///
    /// Paired with the same call on a stored entity, which behaves as it always
    /// did, and with an edit that names a piece the build keeps beside the sets:
    /// that one is refused whole, so the sets do not land either.
    #[tokio::test]
    async fn a_sets_only_edit_of_a_shipped_record_lands_on_top_of_what_ships() {
        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);

        let landed = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    sets: Some([("note".to_string(), "kept on top".to_string())].into()),
                    ..edit("view:loops")
                }))
                .await
                .expect("a sets-only edit answers"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert_eq!(landed["id"], "view:loops", "{landed}");
        assert!(landed["claim"]["address"].is_string(), "{landed}");

        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid.clone()),
                    ..recall_args("view:loops")
                }))
                .await
                .expect("recall answers"),
        );
        let loops = &read["objects"][0];
        assert_eq!(loops["fields"]["note"], "kept on top", "{read}");
        assert!(
            loops["fields"]["selects"].is_string(),
            "what the build ships is still there under the edit: {read}",
        );
        assert_eq!(
            loops["name"], "The Loops",
            "the build keeps the name: {read}"
        );

        // The same call on a stored entity still lands, as before.
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let stored = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    sets: Some([("colour".to_string(), "red".to_string())].into()),
                    ..edit("thing:red-bike")
                }))
                .await
                .expect("a stored entity's sets-only edit answers"),
        );
        assert_ne!(stored["status"], "blocked", "{stored}");
        assert_eq!(stored["id"], "thing:red-bike", "{stored}");

        // A piece the build keeps, beside sets: refused whole.
        let whole = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    source: Some("me".into()),
                    sets: Some([("other".to_string(), "x".to_string())].into()),
                    ..edit("view:loops")
                }))
                .await
                .expect("answers"),
        );
        assert_eq!(whole["status"], "blocked", "{whole}");
        let after = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid),
                    ..recall_args("view:loops")
                }))
                .await
                .expect("recall answers"),
        );
        assert!(
            after["objects"][0]["fields"]["other"].is_null(),
            "a refused edit writes none of its sets: {after}",
        );
    }

    /// **The operator names a shipped record and gives it another name, and a
    /// recall, a search and the creation screen all answer to both.** The edit goes
    /// through the verb a caller uses, over the real index.
    ///
    /// Paired three ways: the record read before the edit is the build's; a name
    /// another thing holds is refused rather than taken; and the build's source
    /// is unmoved by the edit that moved the name.
    #[tokio::test]
    async fn the_operator_names_a_shipped_record_and_it_reads_and_searches_by_both() {
        let jojobot = crate::harness::handler_shipped_indexed();
        let sid = writing_as(&jojobot);
        let recall = |sid: String| {
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .recall(Parameters(RecallArgs {
                            sid: Some(sid),
                            ..recall_args("view:loops")
                        }))
                        .await
                        .expect("recall answers"),
                )
            }
        };
        let before = recall(sid.clone()).await;
        assert_eq!(before["objects"][0]["name"], "The Loops", "{before}");

        let named = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    name: Some("Weekly rounds".into()),
                    aliases: Some(vec!["Orbit".into()]),
                    ..edit("view:loops")
                }))
                .await
                .expect("a name on a shipped record answers"),
        );
        assert_ne!(named["status"], "blocked", "{named}");
        assert_eq!(named["name"], "Weekly rounds", "{named}");

        let after = recall(sid.clone()).await;
        let loops = &after["objects"][0];
        assert_eq!(loops["name"], "Weekly rounds", "{after}");
        assert_eq!(
            loops["source"], "jojobot",
            "the build keeps the source: {after}"
        );
        assert!(
            loops["fields"].get("overlay_name").is_none(),
            "the overlay's keys are not shown as fields: {after}"
        );

        for word in ["Orbit", "Weekly"] {
            let found = json_of(
                &jojobot
                    .search(Parameters(SearchArgs {
                        query: Some(word.into()),
                        sid: Some(sid.clone()),
                        ..search_args()
                    }))
                    .await
                    .expect("search answers"),
            );
            // The RECORD is the hit, not only the claim that carries the words:
            // the index took the operator's name from the record's own document.
            assert!(
                found["results"].as_array().is_some_and(|hits| hits
                    .iter()
                    .any(|hit| hit["hit"] == "entity" && hit["id"] == "view:loops")),
                "a search for {word} finds the record itself: {found}"
            );
        }

        let created = json_of(
            &jojobot
                .add_entity(Parameters(add_args("view", "orbit", "Orbit")))
                .await
                .expect("add answers"),
        );
        assert_eq!(
            created["status"], "blocked",
            "a new thing under the operator's alias is refused: {created}"
        );

        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let taken = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    name: Some("Weekly rounds".into()),
                    ..edit("thing:red-bike")
                }))
                .await
                .expect("answers"),
        );
        assert_eq!(
            taken["status"], "blocked",
            "another thing cannot take the operator's name: {taken}"
        );
    }

    /// **A rename of `person:zenith` onto Alpha's name, as the call a caller sends
    /// once the near miss handed it a token**, with the reason given for it.
    fn forced_onto_alpha(token: Option<String>, reason: Option<&str>) -> UpdateEntityArgs {
        UpdateEntityArgs {
            name: Some("Alpha".into()),
            override_token: token,
            override_reason: reason.map(str::to_string),
            ..edit("person:zenith")
        }
    }

    /// Alpha and Zenith, Zenith already holding one reason under `told_apart_by`.
    async fn zenith_told_apart_by(jojobot: &Jojobot, held: &str) {
        for (slug, name) in [("alpha", "Alpha"), ("zenith", "Zenith")] {
            jojobot
                .add_entity(Parameters(add_args("person", slug, name)))
                .await
                .expect("add ok");
        }
        let set = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    sets: Some([(guard::TOLD_APART_BY.to_string(), held.to_string())].into()),
                    ..edit("person:zenith")
                }))
                .await
                .expect("a sets-only edit answers"),
        );
        assert_ne!(set["status"], "blocked", "{set}");
        assert_eq!(
            fields_of(jojobot, "person:zenith").await[guard::TOLD_APART_BY],
            held
        );
    }

    /// 🚨 **An override whose held reason cannot be read writes nothing.** The
    /// new reason is added beside the reasons the thing holds; a read of them
    /// that fails and is taken as "none held" writes the new reason alone over
    /// them, which is the loss the merge exists to stop. So the read's failure
    /// is the verb's refusal, and the name does not move either.
    #[tokio::test]
    async fn an_override_whose_held_reason_cannot_be_read_writes_nothing() {
        let (healthy, blind) = healthy_and_down(Down::FieldsOf("person:zenith"));
        zenith_told_apart_by(&healthy, "the first one").await;
        let refused = json_of(
            &blind
                .update_entity(Parameters(forced_onto_alpha(None, None)))
                .await
                .expect("the guard answers in the body"),
        );
        let token = token_in(refused["how_to_proceed"].as_str().expect("advice"));

        let answer = blind
            .update_entity(Parameters(forced_onto_alpha(
                Some(token),
                Some("the second one"),
            )))
            .await;
        assert!(
            answer.is_err(),
            "a read that failed is a refusal: {:?}",
            answer.map(|ok| json_of(&ok))
        );
        let held = fields_of(&healthy, "person:zenith").await;
        assert_eq!(held[guard::TOLD_APART_BY], "the first one", "{held}");
        let listed = json_of(
            &healthy
                .recall(Parameters(recall_args("person:zenith")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(listed["objects"][0]["name"], "Zenith", "{listed}");
    }

    /// **A `told_apart_by` the caller sends beside an override is kept**, with the
    /// reason the thing held and the override's own reason, in that order. Paired
    /// with the healthy store the case above lacks, so the merge is what it reads.
    #[tokio::test]
    async fn a_told_apart_by_sent_beside_an_override_joins_the_held_reasons() {
        let jojobot = handler();
        zenith_told_apart_by(&jojobot, "the first one").await;
        let refused = json_of(
            &jojobot
                .update_entity(Parameters(forced_onto_alpha(None, None)))
                .await
                .expect("the guard answers in the body"),
        );
        let token = token_in(refused["how_to_proceed"].as_str().expect("advice"));

        let forced = json_of(
            &jojobot
                .update_entity(Parameters(UpdateEntityArgs {
                    sets: Some(
                        [(guard::TOLD_APART_BY.to_string(), "the sent one".to_string())].into(),
                    ),
                    ..forced_onto_alpha(Some(token), Some("the second one"))
                }))
                .await
                .expect("the forced edit answers"),
        );
        assert_ne!(forced["status"], "blocked", "{forced}");
        assert_eq!(
            fields_of(&jojobot, "person:zenith").await[guard::TOLD_APART_BY],
            "the first one | the sent one | the second one"
        );
    }

    /// 🚨 **The record a key is set again in is the one holding the key's newest
    /// write, never the last one a store lists.** A store that files records
    /// under ids of different lengths lists `f10` before `f9`, so the last record
    /// listed is not the newest. Gamma's record `f9` holds the first `mood`, and
    /// marge's `f10` the later one. Gamma setting `mood` again reads the newest
    /// write, which is marge's, so it appends and leaves `f9` as it was.
    ///
    /// **Paired with the same two writes listed in write order**, where the old
    /// reading was also right: the case holds in both listings.
    #[tokio::test]
    async fn the_record_set_again_is_the_one_holding_the_keys_newest_write_in_any_listing_order() {
        for text_order in [true, false] {
            let inner = Arc::new(InMemoryMemory::seeded());
            let served: Arc<dyn Memory> = match text_order {
                true => Arc::new(jojobot_domain::memory::testing::ServedInIdTextOrder(
                    inner.clone(),
                )),
                false => inner.clone(),
            };
            let jojobot = handler_over_mentions(served.clone());
            ensure(&jojobot, "person:alpha").await;
            seed_bot(&inner, "gamma").await;
            seed_bot(&inner, "marge").await;
            let alpha = EntityId("person:alpha".into());
            // The next two records are `f9` and `f10`.
            let day = jiff::civil::date(2026, 10, 23);
            loop {
                let filler = NewFact::about(alpha.clone(), "a filler claim".to_string(), day);
                let Ok(Guarded::Written(filler)) = inner.capture(filler).await else {
                    panic!("a filler claim lands");
                };
                if filler.id.ordinal() == Some(8) {
                    break;
                }
            }
            let gamma = booted(&jojobot, "gamma").await;
            let marge = booted(&jojobot, "marge").await;
            let set_mood = |sid: &str, mood: &str| UpdateEntityArgs {
                reviewed: None,
                request_id: None,
                handle: "person:alpha".into(),
                name: None,
                aliases: None,
                source: None,
                crm: None,
                sets: Some([("mood".to_string(), mood.to_string())].into()),
                override_token: None,
                override_reason: None,
                sid: Some(sid.to_string()),
            };
            for (sid, mood) in [(&gamma, "calm"), (&marge, "restless")] {
                let written = json_of(
                    &jojobot
                        .update_entity(Parameters(set_mood(sid, mood)))
                        .await
                        .expect("update_entity ok"),
                );
                assert_ne!(written["status"], "blocked", "{written}");
            }
            let listed = served.recall(&alpha).await.expect("the store answers");
            let ids: Vec<&str> = listed
                .iter()
                .filter(|fact| fact.fields.contains_key("mood"))
                .map(|fact| fact.id.as_str())
                .collect();
            let expected: &[&str] = match text_order {
                true => &["f10", "f9"],
                false => &["f9", "f10"],
            };
            assert_eq!(ids, expected, "the fixture lists the records as intended");

            let third = json_of(
                &jojobot
                    .update_entity(Parameters(set_mood(&gamma, "tired")))
                    .await
                    .expect("update_entity ok"),
            );
            assert_ne!(third["status"], "blocked", "{third}");
            let held = inner.recall(&alpha).await.expect("the store answers");
            let mut moods: Vec<(String, String)> = held
                .iter()
                .filter(|fact| fact.fields.contains_key("mood"))
                .map(|fact| (fact.id.as_str().to_string(), fact.fields["mood"].clone()))
                .collect();
            moods.sort_by_key(|(id, _)| id.trim_start_matches('f').parse::<u64>().unwrap_or(0));
            assert_eq!(
                moods,
                [
                    ("f9".to_string(), "calm".to_string()),
                    ("f10".to_string(), "restless".to_string()),
                    ("f11".to_string(), "tired".to_string()),
                ],
                "gamma's older record was rewritten in place behind marge's newer one \
                 (store listing in text order: {text_order})"
            );
        }
    }
}
