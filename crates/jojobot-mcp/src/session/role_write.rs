//! **The one door through which jojobot writes a role's keys.**
//!
//! A role is its own object, a child of the bot that holds it, and its holder,
//! claim moment and agent are jojobot's own derived writes, not a caller's. So
//! the claim at boot, a renewal on every write the holder makes, and a release
//! at wrap all land through [`Jojobot::write_role_keys`] and nowhere else. A
//! later change in how a claim's keys reach their thing is then a change here.
//!
//! The decision on whether a write may land is not made here. The store makes
//! it in the same act as the write, against the role's state in both shapes —
//! see [`jojobot_domain::memory::refuses_role_write`]. This function only finds
//! or makes the role object and its claim record, and sends the right write.

use jiff::Timestamp;
use jiff::civil::Date;
use jojobot_domain::memory::{
    EntityId, EntityKind, FactPatch, Guarded, MemoryError, NewEntity, NewFact, guard,
};
use jojobot_domain::session::{ROLE_CLAIMED_AT, ROLE_HOLDER, RoleMove, RoleMoveKind, role_state};

use crate::Jojobot;

/// **What a write to a role's keys is.**
pub(crate) enum RoleWrite<'a> {
    /// The boot door takes the role, or takes it again.
    Claim { claimant: &'a str, now: Timestamp },
    /// A write the holder made keeps its lease fresh.
    Renew { claimant: &'a str, at: Timestamp },
    /// The holder gives the role up at wrap.
    Release { claimant: &'a str },
}

/// What the claim record of a role says about itself, so a role claimed before
/// it was an object has somewhere to move to.
fn claim_content(role: &str) -> String {
    format!("claimed the {role} role")
}

impl Jojobot {
    /// **Write a role's keys, creating the role object and its claim record the
    /// first time a role is written.**
    ///
    /// A claim sets the holder and the moment, and the store refuses it while
    /// another session's lease is fresh. A renewal or a release carries a
    /// [`RoleMove`], and the store applies it only while its own sid holds the
    /// role, so one that arrives after the holder gave the role up changes
    /// nothing. **A role claimed in the old shape has no claim record on its
    /// object yet.** The record is made empty and the move is applied to it, so
    /// the compare-and-write of a renewal or a release is the same one an
    /// established role gets, and the first write after the change moves the
    /// claim onto the object without a window in which it could be taken twice.
    ///
    /// The role object is the child of `bot`, the bot whose session writes. A
    /// role that exists under another bot is written where it is: one role is one
    /// object, whoever claims it.
    pub(crate) async fn write_role_keys(
        &self,
        bot: &EntityId,
        role: &str,
        write: RoleWrite<'_>,
        today: Date,
    ) -> Result<(), MemoryError> {
        let object = EntityId::new(EntityKind::ROLE, role);
        jojobot_domain::memory::validate_subject(&object)?;
        let record = match self.role_claim_record(&object, role).await? {
            Some(record) => record,
            None => {
                self.ensure_role_object(bot, &object, role).await?;
                None
            }
        };
        let (fields, role_move, clear) = match &write {
            RoleWrite::Claim { claimant, now } => (
                [
                    (ROLE_HOLDER.to_string(), claimant.to_string()),
                    (ROLE_CLAIMED_AT.to_string(), now.to_string()),
                ]
                .into_iter()
                .collect(),
                None,
                Vec::new(),
            ),
            RoleWrite::Renew { claimant, at } => (
                [
                    (ROLE_HOLDER.to_string(), claimant.to_string()),
                    (ROLE_CLAIMED_AT.to_string(), at.to_string()),
                ]
                .into_iter()
                .collect(),
                Some(RoleMove {
                    kind: RoleMoveKind::Renew,
                    role: role.to_string(),
                    claimant: claimant.to_string(),
                }),
                Vec::new(),
            ),
            RoleWrite::Release { claimant } => (
                [(
                    ROLE_CLAIMED_AT.to_string(),
                    Timestamp::UNIX_EPOCH.to_string(),
                )]
                .into_iter()
                .collect(),
                Some(RoleMove {
                    kind: RoleMoveKind::Release,
                    role: role.to_string(),
                    claimant: claimant.to_string(),
                }),
                vec![ROLE_HOLDER.to_string()],
            ),
        };
        match (record, role_move) {
            // The record exists: the write patches it in place, so a claim
            // never multiplies records.
            (Some(address), role_move) => self
                .memory
                .update_fact(
                    &address,
                    FactPatch {
                        fields,
                        clear_fields: clear,
                        role_move,
                        ..FactPatch::default()
                    },
                    bot,
                )
                .await
                .map(|_| ()),
            // The first claim ever made for a role: the record is captured
            // with the claim's own fields, and the store decides it as a claim.
            (None, None) => {
                let mut fact = NewFact::about(object.clone(), claim_content(role), today);
                fact.fields = fields;
                self.memory.capture(fact).await.map(|_| ())
            }
            // A renewal or a release of a role that has no record on its object
            // yet. The record is made empty, so nothing is claimed by making
            // it, and the move is applied to it under the store's own check.
            (None, Some(role_move)) => {
                let made = self
                    .memory
                    .capture(NewFact::about(object.clone(), claim_content(role), today))
                    .await?;
                let Guarded::Written(fact) = made else {
                    return Err(MemoryError::InvalidFact(format!(
                        "the claim record of {object} could not be made"
                    )));
                };
                self.memory
                    .update_fact(
                        &fact.address(),
                        FactPatch {
                            fields,
                            clear_fields: clear,
                            role_move: Some(role_move),
                            ..FactPatch::default()
                        },
                        bot,
                    )
                    .await
                    .map(|_| ())
            }
        }
    }

    /// **The bot a role belongs to, when it is not `claimant`.** A role
    /// belongs to the bot that carries its name in `claims_role`; when no bot
    /// does, to the bot whose child the role object is. A claimant that
    /// carries the name is the owner, whatever else exists. `None` means the
    /// claim may go on: the claimant owns the role, or nobody does.
    ///
    /// Read before the claim writes anything, so a refusal leaves no trace.
    /// Archived bots and archived role objects are skipped
    /// here, because `list_entities` returns them; skipping them is how a
    /// stray one stops owning.
    pub(crate) async fn owner_of_role(
        &self,
        claimant: &EntityId,
        role: &str,
    ) -> Result<Option<EntityId>, MemoryError> {
        let mut carriers: Vec<EntityId> = Vec::new();
        for bot in self.memory.list_entities(Some(EntityKind::BOT)).await? {
            if bot.archived.is_some() {
                continue;
            }
            let fields = self.memory.fields(&bot.id).await?;
            if fields
                .get(jojobot_domain::memory::CLAIMS_ROLE)
                .is_some_and(|carried| carried.trim() == role)
            {
                carriers.push(bot.id);
            }
        }
        if carriers.contains(claimant) {
            return Ok(None);
        }
        if let Some(carrier) = carriers.into_iter().min() {
            return Ok(Some(carrier));
        }
        let object = EntityId::new(EntityKind::ROLE, role);
        let parent = self
            .memory
            .list_entities(Some(EntityKind::ROLE))
            .await?
            .into_iter()
            .find(|entity| entity.id == object && entity.archived.is_none())
            .and_then(|entity| entity.parent);
        Ok(parent.filter(|parent| parent != claimant))
    }

    /// **The bots that head a chart**: a bot with no manager that something
    /// reports to. One listing of every entity and one folded-fields read per
    /// entity, so the cost grows with the entities in the store rather than
    /// with its bots. Only a stranger's archive of a role object reaches it.
    pub(crate) async fn chart_heads(&self) -> Result<Vec<EntityId>, MemoryError> {
        let mut managers: std::collections::BTreeSet<String> = Default::default();
        let mut unmanaged: Vec<EntityId> = Vec::new();
        for entity in self.memory.list_entities(None).await? {
            if entity.archived.is_some() {
                continue;
            }
            let held = self.memory.fields(&entity.id).await?;
            match held.get(jojobot_domain::memory::REPORTS_TO) {
                Some(manager) => {
                    managers.insert(manager.trim().to_string());
                }
                None if entity.kind == EntityKind::BOT => unmanaged.push(entity.id),
                None => {}
            }
        }
        unmanaged.retain(|bot| managers.contains(bot.as_str()));
        unmanaged.sort();
        Ok(unmanaged)
    }

    /// **Refuse archiving or restoring a role object to a bot that may not.**
    /// Only the bot the object is a child of, its owner, and the bots heading
    /// the chart may. Without this, archiving frees the name and any bot could
    /// archive another's role object and claim the role itself. `None` lets the
    /// call go on: the handle is not a role object, or the caller may.
    ///
    /// **A read that fails refuses**, because a check that could not be made is
    /// not a licence.
    pub(crate) async fn refuse_a_stranger_the_role_object(
        &self,
        by: &EntityId,
        handle: &EntityId,
    ) -> Option<rmcp::model::CallToolResult> {
        if handle.kind() != Some(EntityKind::ROLE) {
            return None;
        }
        let read = async {
            let parent = self
                .memory
                .list_entities(Some(EntityKind::ROLE))
                .await?
                .into_iter()
                .find(|entity| &entity.id == handle)
                .map(|entity| entity.parent);
            let Some(parent) = parent else {
                return Ok::<_, MemoryError>(None);
            };
            if parent.as_ref() == Some(by) {
                return Ok(None);
            }
            let heads = self.chart_heads().await?;
            if heads.contains(by) {
                return Ok(None);
            }
            Ok(Some((parent, heads)))
        }
        .await;
        match read {
            Ok(None) => None,
            Ok(Some((owner, heads))) => {
                let mut may: Vec<String> = owner.iter().map(|o| o.to_string()).collect();
                may.extend(heads.iter().map(|h| h.to_string()));
                Some(crate::caller::handle_declined(
                    handle.as_str(),
                    format!(
                        "Only the bot this role object belongs to or a bot heading the chart may \
                         archive or restore it: {}. Nothing was written.",
                        may.join(", ")
                    ),
                ))
            }
            Err(e) => {
                tracing::warn!(error = %e, %handle, "who may archive a role object could not be read");
                Some(crate::caller::handle_declined(
                    handle.as_str(),
                    "Who may archive this role object could not be read just now, so nothing was \
                     written. Try again in a moment."
                        .to_string(),
                ))
            }
        }
    }

    /// **The claim record of a role, if the role has one.** `Ok(None)` is a role
    /// object with no record, or one that does not exist yet, and the caller
    /// tells those apart by what it does next. The record is the fact that
    /// carries a holder or a moment, or failing that the empty one a move made
    /// for itself.
    async fn role_claim_record(
        &self,
        object: &EntityId,
        role: &str,
    ) -> Result<Option<Option<jojobot_domain::memory::FactAddress>>, MemoryError> {
        let facts = match self.memory.recall(object).await {
            Ok(facts) => facts,
            Err(MemoryError::UnknownEntity { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let record = facts
            .iter()
            .find(|f| f.fields.contains_key(ROLE_HOLDER) || f.fields.contains_key(ROLE_CLAIMED_AT))
            .or_else(|| facts.iter().find(|f| f.content == claim_content(role)))
            .map(|f| f.address());
        Ok(Some(record))
    }

    /// **Make the role object, as a child of the bot, the first time it is
    /// written.** The creation is jojobot's own, so a near miss against another
    /// seat, such as `dev` beside `dev2`, is not a question for a caller: the
    /// token the guard minted for it is the answer.
    async fn ensure_role_object(
        &self,
        bot: &EntityId,
        object: &EntityId,
        role: &str,
    ) -> Result<(), MemoryError> {
        let new = NewEntity {
            parent: Some(bot.clone()),
            ..NewEntity::new(object.clone(), role, "jojobot")
        };
        match self.memory.add_entity(new.clone()).await? {
            Guarded::Written(_) => Ok(()),
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                let token = guard::override_token(&attempted, &candidates);
                match self
                    .memory
                    .add_entity(NewEntity {
                        override_token: Some(token),
                        ..new
                    })
                    .await?
                {
                    Guarded::Written(_) => Ok(()),
                    Guarded::Blocked { .. } => Err(MemoryError::InvalidFact(format!(
                        "the role object {object} could not be made"
                    ))),
                }
            }
        }
    }

    /// **The roles this session's claimant holds, read in both shapes**, each with
    /// the moment its lease was last written. A role object under the bot that
    /// carries a moment answers for itself, with or without a holder; the bot's
    /// old `role/<role>/holder` keys answer for any role whose object does not.
    /// The moment is what a caller that writes only when a lease is old enough
    /// decides on, without a second read.
    pub(crate) async fn roles_held_by(
        &self,
        bot: &EntityId,
        claimant: &str,
    ) -> Result<Vec<HeldRole>, MemoryError> {
        let bot_fields = self.memory.fields(bot).await?;
        let mut held: Vec<HeldRole> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for child in self.memory.children(bot).await? {
            let Some(role) = jojobot_domain::memory::role_named_by(&child) else {
                continue;
            };
            let child_fields = self.memory.fields(&child).await?;
            seen.push(role.clone());
            let state = role_state(&role, Some(&child_fields), &bot_fields);
            if state.holder.as_deref() == Some(claimant) {
                held.push(HeldRole {
                    role,
                    claimed_at: state.claimed_at,
                });
            }
        }
        for key in bot_fields.keys() {
            let Some(role) = jojobot_domain::session::role_from_field_key(key) else {
                continue;
            };
            if seen.iter().any(|s| s == role) || held.iter().any(|h| h.role == role) {
                continue;
            }
            let state = role_state(role, None, &bot_fields);
            if state.holder.as_deref() == Some(claimant) {
                held.push(HeldRole {
                    role: role.to_string(),
                    claimed_at: state.claimed_at,
                });
            }
        }
        Ok(held)
    }
}

/// **A role a session holds, and when its lease was last written.** `None` is a
/// moment that is missing or does not read as one.
pub(crate) struct HeldRole {
    pub(crate) role: String,
    /// What a renewal that waits for the lease to be old enough decides on.
    pub(crate) claimed_at: Option<Timestamp>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::{capture_args, capture_ok, recall_args};
    use crate::memory::{CaptureArgs, RecallArgs, UpdateFactArgs};
    use crate::orientation::start_here::OrientArgs;
    use rmcp::handler::server::wrapper::Parameters;

    fn boot(bot: &str, claim: Option<&str>) -> OrientArgs {
        OrientArgs {
            claim: claim.map(str::to_string),
            timezone: None,
            bot: Some(bot.into()),
            brief: None,
            skill: None,
            section: None,
            resume: Some("new".into()),
            sid: None,
            today: None,
        }
    }

    async fn booted(jojobot: &Jojobot, bot: &str, claim: Option<&str>) -> serde_json::Value {
        json_of(
            &jojobot
                .start_here(Parameters(boot(bot, claim)))
                .await
                .expect("start_here ok"),
        )
    }

    /// The claim the previous build wrote, on the bot, written through the trait
    /// the way a store restored from before the change holds it. Its moment is
    /// ten minutes old: inside the lease and past the renewal age, so a write by
    /// the holder renews it and the claim moves.
    async fn an_old_shape_claim(jojobot: &Jojobot, bot: &str, role: &str, holder: &str) {
        let fields = [
            (
                jojobot_domain::session::role_holder_key(role),
                holder.to_string(),
            ),
            (
                jojobot_domain::session::role_claimed_at_key(role),
                (jojobot.clock().now() - jiff::SignedDuration::from_mins(10)).to_string(),
            ),
        ]
        .into_iter()
        .collect();
        jojobot
            .memory
            .capture(NewFact {
                fields,
                ..NewFact::about(
                    EntityId(bot.into()),
                    format!("claimed the {role} role"),
                    jiff::civil::date(2026, 10, 7),
                )
            })
            .await
            .expect("the old claim is written")
            .written()
            .expect("nothing blocks it");
    }

    /// **A claim makes the role its own object, a child of the bot, and one read
    /// over the kind answers who holds what.** The positive: the object exists
    /// under the bot with the session as holder and a moment that reads as one.
    /// The reads: the bot's children name it, and a recall of the kind under that
    /// parent returns it with its own keys, and no other bot's role.
    #[tokio::test]
    async fn a_claim_makes_the_role_its_own_object_under_the_bot_and_one_recall_reads_it() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let claimed = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
        let sid = sid_of(&claimed).expect("a handle");
        let other = booted(&jojobot, "delta", Some("pm-seat")).await;
        assert_eq!(other["session"]["claim"]["status"], "taken", "{other}");

        let role = EntityId("role:dev-dispatch".into());
        let children = jojobot
            .memory
            .children(&EntityId("bot:gamma".into()))
            .await
            .expect("children ok");
        assert!(children.contains(&role), "{children:?}");
        let fields = jojobot.memory.fields(&role).await.expect("fields ok");
        assert_eq!(fields.get("holder"), Some(&sid), "{fields:?}");
        assert!(
            fields
                .get("claimed_at")
                .is_some_and(|at| at.parse::<jiff::Timestamp>().is_ok()),
            "the moment reads as one: {fields:?}",
        );

        let under_gamma = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: None,
                    kind: Some("role".into()),
                    parent: Some("bot:gamma".into()),
                    facts: None,
                    ..recall_args("bot:gamma")
                }))
                .await
                .expect("recall ok"),
        );
        let ids: Vec<&str> = under_gamma["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .filter_map(|o| o["id"].as_str())
            .collect();
        assert_eq!(ids, vec!["role:dev-dispatch"], "{under_gamma}");
        assert_eq!(
            under_gamma["objects"][0]["fields"]["holder"], sid,
            "{under_gamma}"
        );

        let every_role = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: None,
                    kind: Some("role".into()),
                    facts: None,
                    ..recall_args("bot:gamma")
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            every_role["objects"].as_array().map(Vec::len),
            Some(2),
            "one read over the kind returns every role: {every_role}",
        );
    }

    /// **Seats whose names are near misses of each other are all made.** The
    /// creation guard would hold `dev2` beside `dev`; the seat is jojobot's own
    /// creation, so its own token answers the guard, and a claim never reads as
    /// unavailable for the name its role was given.
    #[tokio::test]
    async fn seats_named_like_each_other_are_all_made() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        for role in ["dev", "dev2", "dev3"] {
            let claimed = booted(&jojobot, "gamma", Some(role)).await;
            assert_eq!(
                claimed["session"]["claim"]["status"], "taken",
                "{role}: {claimed}"
            );
        }
        let children = jojobot
            .memory
            .children(&EntityId("bot:gamma".into()))
            .await
            .expect("children ok");
        for role in ["role:dev", "role:dev2", "role:dev3"] {
            assert!(
                children.contains(&EntityId(role.into())),
                "{role} is a child of the bot: {children:?}",
            );
        }
    }

    /// **A role claimed in the old shape refuses a rival at the boot door.** The
    /// previous build left the claim on the bot. It must read as held after the
    /// change, naming its holder, and the refusal must write nothing.
    #[tokio::test]
    async fn a_role_claimed_in_the_old_shape_refuses_a_rival_naming_its_holder() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        an_old_shape_claim(&jojobot, "bot:gamma", "dev-dispatch", "oldsid").await;

        let rival = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        assert_eq!(rival["session"]["claim"]["status"], "refused", "{rival}");
        assert_eq!(rival["session"]["claim"]["holder"], "oldsid", "{rival}");
    }

    /// **A write moves an old-shape claim onto its object, once, and the claim
    /// stays held across the move.** Before the write there is no object; after
    /// it the object carries the holder and a moment, the bot's old keys are left
    /// as written, and a rival is still refused.
    #[tokio::test]
    async fn a_beat_moves_a_role_claimed_in_the_old_shape_onto_its_object() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let first = booted(&jojobot, "gamma", None).await;
        let sid = sid_of(&first).expect("a handle");
        an_old_shape_claim(&jojobot, "bot:gamma", "dev-dispatch", &sid).await;
        let role = EntityId("role:dev-dispatch".into());
        assert!(
            matches!(
                jojobot.memory.recall(&role).await,
                Err(MemoryError::UnknownEntity { .. })
            ),
            "the role is not an object yet",
        );

        crate::session::testing::journal_entry(&jojobot, &sid, "did some of the work").await;

        let moved = jojobot.memory.fields(&role).await.expect("fields ok");
        assert_eq!(moved.get("holder"), Some(&sid), "{moved:?}");
        assert!(moved.contains_key("claimed_at"), "{moved:?}");
        let old = jojobot
            .memory
            .fields(&EntityId("bot:gamma".into()))
            .await
            .expect("fields ok");
        assert_eq!(
            old.get(&jojobot_domain::session::role_holder_key("dev-dispatch")),
            Some(&sid),
            "the old keys are left as written: {old:?}",
        );
        let rival = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        assert_eq!(rival["session"]["claim"]["status"], "refused", "{rival}");
        assert_eq!(rival["session"]["claim"]["holder"], sid, "{rival}");
    }

    /// **A wrap releases a role claimed in the old shape, and the role is free at
    /// once although the bot's old keys still name the holder.** The positive is
    /// the rival that takes it after the wrap; the negative before it is the same
    /// rival refused, so the release is what changed the answer.
    #[tokio::test]
    async fn a_wrap_releases_a_role_claimed_in_the_old_shape() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let first = booted(&jojobot, "gamma", None).await;
        let sid = sid_of(&first).expect("a handle");
        an_old_shape_claim(&jojobot, "bot:gamma", "dev-dispatch", &sid).await;

        let before = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        assert_eq!(before["session"]["claim"]["status"], "refused", "{before}");

        jojobot
            .wrap_session(Parameters(crate::session::wrap_session::WrapSessionArgs {
                story: "done".into(),
                sid: sid.clone(),
            }))
            .await
            .expect("wrap ok");

        let after = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        assert_eq!(after["session"]["claim"]["status"], "taken", "{after}");
        let role = jojobot
            .memory
            .fields(&EntityId("role:dev-dispatch".into()))
            .await
            .expect("fields ok");
        assert_eq!(role.get("holder"), sid_of(&after).as_ref(), "{role:?}");
    }

    /// **No caller writes a role object's holder or claim moment, by any verb.**
    /// A capture naming either, an edit setting or clearing either: all refused.
    /// The agent key beside them lands, which is what keeps the guard from being
    /// a refusal of everything on the object.
    #[tokio::test]
    async fn no_verb_writes_a_role_objects_holder_or_moment_and_the_agent_key_lands() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let claimed = booted(&jojobot, "gamma", Some("dev-dispatch")).await;
        let sid = sid_of(&claimed).expect("a handle");

        for key in ["holder", "claimed_at"] {
            let refused = blocked(
                &jojobot
                    .capture(Parameters(CaptureArgs {
                        fields: Some(
                            [(key.to_string(), "anything".to_string())]
                                .into_iter()
                                .collect(),
                        ),
                        ..capture_args("role:dev-dispatch", "taking the seat by hand")
                    }))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["wrote"], false, "{key}: {refused}");
        }

        let landed = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("agent".to_string(), "an-agent-id".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("role:dev-dispatch", "the line records its agent")
            },
        )
        .await;
        assert_ne!(landed["status"], "blocked", "{landed}");

        // And the claim record itself cannot be edited, set or cleared.
        let record = jojobot
            .memory
            .recall(&EntityId("role:dev-dispatch".into()))
            .await
            .expect("recall ok")
            .into_iter()
            .find(|f| f.fields.contains_key("holder"))
            .expect("the claim record")
            .address()
            .to_string();
        let edit = |set: Option<(&str, &str)>, clear: Option<&str>| UpdateFactArgs {
            fields: set.map(|(k, v)| [(k.to_string(), v.to_string())].into_iter().collect()),
            clear_fields: clear.map(|k| vec![k.to_string()]),
            ..crate::memory::testing::update_args(&record)
        };
        for args in [
            edit(Some(("holder", "me")), None),
            edit(None, Some("claimed_at")),
        ] {
            let refused = blocked(
                &jojobot
                    .update_fact(Parameters(args))
                    .await
                    .expect("a refusal is an answer"),
            );
            assert_eq!(refused["wrote"], false, "{refused}");
        }
        let fields = jojobot
            .memory
            .fields(&EntityId("role:dev-dispatch".into()))
            .await
            .expect("fields ok");
        assert_eq!(fields.get("holder"), Some(&sid), "{fields:?}");
        assert_eq!(fields.get("agent").map(String::as_str), Some("an-agent-id"));
    }

    /// Stand a bot's `claims_role` up the way another identity would have
    /// written it, which this fixture cannot be.
    async fn carries(jojobot: &Jojobot, bot: &str, role: &str) {
        jojobot
            .memory
            .capture(NewFact {
                fields: [(
                    jojobot_domain::memory::CLAIMS_ROLE.to_string(),
                    role.to_string(),
                )]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(bot.into()),
                    format!("{bot} runs {role}"),
                    jiff::civil::date(2026, 10, 7),
                )
            })
            .await
            .expect("the key is written")
            .written()
            .expect("nothing blocks it");
    }

    /// **Who a role belongs to, case by case.** The carrier of the name owns
    /// it and outranks the parent of an object; with no carrier the parent
    /// owns it; the claimant that owns it, either way, is let through; and a
    /// name nobody holds is free. An archived object under the wrong bot owns
    /// nothing, which is how a stray one is cleared.
    #[tokio::test]
    async fn a_role_belongs_to_its_carrier_then_to_its_parent_and_a_free_name_to_nobody() {
        let jojobot = handler();
        make_bot(&jojobot, "alpha").await;
        make_bot(&jojobot, "beta").await;
        let alpha = EntityId("bot:alpha".into());
        let beta = EntityId("bot:beta".into());
        let owner = |claimant: &EntityId, role: &'static str| {
            let jojobot = &jojobot;
            let claimant = claimant.clone();
            async move {
                jojobot
                    .owner_of_role(&claimant, role)
                    .await
                    .expect("read ok")
            }
        };

        // A name nobody holds.
        assert_eq!(owner(&alpha, "sigma").await, None, "a free name");

        // An object with a parent and no carrier: the parent owns it.
        jojobot
            .ensure_role_object(&beta, &EntityId("role:omega".into()), "omega")
            .await
            .expect("the object is made");
        assert_eq!(owner(&alpha, "omega").await, Some(beta.clone()));
        assert_eq!(owner(&beta, "omega").await, None, "its parent may claim it");

        // A carrier and no object.
        carries(&jojobot, "bot:alpha", "theta").await;
        assert_eq!(owner(&beta, "theta").await, Some(alpha.clone()));
        assert_eq!(
            owner(&alpha, "theta").await,
            None,
            "its carrier may claim it"
        );

        // A carrier outranks the parent of a stray object.
        carries(&jojobot, "bot:alpha", "kappa").await;
        jojobot
            .ensure_role_object(&beta, &EntityId("role:kappa".into()), "kappa")
            .await
            .expect("the stray is made");
        assert_eq!(
            owner(&alpha, "kappa").await,
            None,
            "the carrier is let through"
        );
        assert_eq!(owner(&beta, "kappa").await, Some(alpha.clone()));

        // An archived stray owns nothing.
        jojobot
            .ensure_role_object(&beta, &EntityId("role:lambda".into()), "lambda")
            .await
            .expect("the object is made");
        assert_eq!(owner(&alpha, "lambda").await, Some(beta.clone()));
        jojobot
            .memory
            .archive_entity(&EntityId("role:lambda".into()), "made under the wrong bot")
            .await
            .expect("archived");
        assert_eq!(
            owner(&alpha, "lambda").await,
            None,
            "an archived stray is cleared"
        );

        // An archived carrier owns nothing either.
        make_bot(&jojobot, "gamma").await;
        carries(&jojobot, "bot:gamma", "rho").await;
        let gamma = EntityId("bot:gamma".into());
        assert_eq!(owner(&alpha, "rho").await, Some(gamma.clone()));
        jojobot
            .memory
            .archive_entity(&gamma, "no longer a seat")
            .await
            .expect("archived");
        assert_eq!(
            owner(&alpha, "rho").await,
            None,
            "an archived carrier is cleared"
        );
    }
}
