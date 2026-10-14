//! `update_entity` — Edit what an entity is called, and its other metadata, in place.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `update_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityArgs {
    /// The entity's handle. It says which entity to edit; this verb does not
    /// edit it.
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
    /// on the name alone: the same name under another kind, or a name that holds
    /// or sits inside another. Without it that refusal stands. The reason is
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
    /// The claim is `inference`, as a capture's is when it says nothing. To put
    /// the operator's own word behind a key, write it with `capture`.
    #[serde(default)]
    pub(crate) sets: Option<std::collections::BTreeMap<String, String>>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes. Send the
    /// same call again with the same `request_id` after an answer was lost: if
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
                       parent — use `rename_entity` for that. THIS VERB \
                       DOES NOT TOUCH MAILBOXES: a box is not a property of an entity that can \
                       be edited or reassigned — it belongs to the bot it is named for and opens \
                       with it, in add_entity (the operator's opens at the first post to them), \
                       so there is nothing here to point at a different one. Any change to what it is CALLED — name or aliases — faces the same \
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
                       with fields {starred: \"true\"} or {starred: \"false\"} stars or unstars it. A handle that names \
                       nothing comes back blocked with the nearest handles — it never creates. Aliases or `sets` that would leave the thing, or its claim, too big to \
                       be read back whole in one answer are refused with nothing written. The \
                       receipt's `changed` lists what this call changed, or says nothing. A `request_id` makes a resend safe: a repeat returns the first receipt and writes nothing."
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
        let handle = EntityId::person(&args.handle);
        // **A voucher is issued, never edited.** Judged on the handle's kind.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&handle) {
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
        if let Some(refused) = self
            .refuses_a_boot_floor_for_relabelling(&handle, &patch)
            .await
        {
            return memory_declined("update_entity", refused);
        }
        // **The thing as it stands before the write**, for the receipt's account
        // of what the write changed.
        let before = self.as_it_stands(&handle).await;
        let sets = {
            let mut sets = args.sets.clone().unwrap_or_default();
            if let Some(reason) = override_reason {
                sets.insert(guard::TOLD_APART_BY.to_string(), reason.to_string());
            }
            (!sets.is_empty()).then_some(sets)
        };
        let mut claim = None;
        let mut fold_behind = None;
        let mut due_moved = jojobot_domain::attention::DueMove::Unchanged;
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
                let keys = set_keys.iter().cloned().collect::<Vec<_>>();
                let recorded_at = self.dated(None, args.sid.as_deref()).await?;
                let first = jojobot_domain::memory::NewFact {
                    sets: set_keys,
                    derived,
                    fields,
                    session: Some(caller.sid.as_str().to_string()),
                    ..jojobot_domain::memory::NewFact::about(
                        handle.clone(),
                        // **What the generated claim says**: which keys were set,
                        // on the thing as a link, so the words follow a rename.
                        format!("Set {} on @{}.", keys.join(", "), handle.as_str()),
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
                    .refuses_a_boot_floor_for_capture(&first, &caller.bot)
                    .await
                {
                    return memory_declined("update_entity", refused);
                }
                match jojobot_domain::request::armed(
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
                if matches!(due_moved, jojobot_domain::attention::DueMove::Cleared) {
                    self.clear_stored_due_moment(&entity.id, &caller.bot, caller.sid.as_str())
                        .await;
                }
                self.beat("update_entity", entity.id.as_str(), args.sid.as_deref())
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
                    self.note_status_word(fact, &mut body).await;
                    self.note_depends_on_read(fact, &caller, &mut body).await;
                }
                let changed = match fold_behind {
                    Some(_) => crate::changed::Changed::not_read(),
                    None => self.changed_on(&before, &entity.id).await,
                };
                crate::changed::note_changed(&mut body, changed);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

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
    /// the write that works.** The record is the build's own, so no token lifts
    /// the refusal. One that carried a token sent the caller round a loop: the
    /// resend was refused again, with a new token, for ever.
    ///
    /// **Three things at once.** Every field of the edit that the build keeps is
    /// refused with no token; a resend with a token the caller invented, which is
    /// the move the old advice asked for, is refused the same way and writes
    /// nothing; and the write the refusal names lands on the same record, so the
    /// way forward exists. Without the last, a refusal that named nothing real
    /// would pass the first two.
    #[tokio::test]
    async fn a_shipped_records_relabel_refusal_offers_no_token_and_names_a_write_that_lands() {
        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);

        let calls = [
            (
                "name",
                UpdateEntityArgs {
                    name: Some("Mine".into()),
                    ..edit("view:loops")
                },
            ),
            (
                "aliases",
                UpdateEntityArgs {
                    aliases: Some(vec!["rounds".into()]),
                    ..edit("view:loops")
                },
            ),
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
                    name: Some("Mine".into()),
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
                    name: Some("Mine".into()),
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
}
