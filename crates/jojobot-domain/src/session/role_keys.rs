//! **The previous shape of a role's lease, and the one pass that leaves it.**
//!
//! Before a role was its own object, the holder and the claim moment lived on
//! the bot as `role/<role>/holder` and `role/<role>/claimed_at`. Nothing reads
//! those keys as a lease any more: the role object answers for every lease. A
//! store restored from before the change still holds them, so
//! [`move_old_role_claims`] carries each such claim onto its role object once,
//! at start, and takes the keys off the bot. This is the only module that
//! names the old keys.

use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::civil::Date;

use crate::memory::{
    EntityId, EntityKind, FactPatch, FactStatus, Guarded, Memory, MemoryError, NewEntity, NewFact,
    guard,
};

use super::{ROLE_CLAIMED_AT, ROLE_HOLDER};

/// **The field key a bot once held a role's claimant under**, on the bot
/// entity's own fields. Only the old shape names it.
pub fn role_holder_key(role: &str) -> String {
    format!("role/{role}/holder")
}

/// **The field key a bot once held a role's last claim moment under**,
/// alongside [`role_holder_key`].
pub fn role_claimed_at_key(role: &str) -> String {
    format!("role/{role}/claimed_at")
}

/// **Whether `key` is one of the two keys the old shape kept on a bot, and
/// which role it names** — parsed by pattern, because a role's name is the
/// caller's own choice.
pub fn role_from_field_key(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("role/")?;
    rest.strip_suffix("/holder")
        .or_else(|| rest.strip_suffix("/claimed_at"))
}

/// **What the claim record of a role says about itself**, so a role claimed
/// before it was an object has somewhere to move to, and so the record the move
/// leaves empty can be told from a record somebody wrote.
pub fn role_claim_content(role: &str) -> String {
    format!("claimed the {role} role")
}

/// **Take `role`'s old holder and moment off `bot`**, and return how many of its
/// records held them. Call it only once the role's claim is on its object, or
/// the keys carry none, because the keys are no lease and are lost with the call.
///
/// A record keeps whatever else it holds. A record left with no field whose
/// words are the claim's own is archived: an active record with no role key
/// reads as a rule the bot has to follow. Nothing else of the bot changes, so
/// another role's keys stay.
///
/// A bot with none of the keys costs one folded read and no write. The pass is
/// idempotent.
///
/// **The role object's write and this one are two store calls, not one
/// transaction.** The object is written first. If this call then fails, the
/// object still answers for the lease and the keys that are left are inert. The
/// next start takes them off.
pub async fn drop_old_role_keys<M: Memory + ?Sized>(
    store: &M,
    bot: &EntityId,
    role: &str,
) -> Result<usize, MemoryError> {
    let keys = [role_holder_key(role), role_claimed_at_key(role)];
    let held = store.fields(bot).await?;
    if !keys.iter().any(|key| held.contains_key(key)) {
        return Ok(0);
    }
    let claim = role_claim_content(role);
    let mut touched = 0;
    for fact in store.recall(bot).await? {
        if fact.status != FactStatus::Active {
            continue;
        }
        let carried: Vec<String> = keys
            .iter()
            .filter(|key| fact.fields.contains_key(*key))
            .cloned()
            .collect();
        if carried.is_empty() {
            continue;
        }
        let left_empty = fact.fields.len() == carried.len() && fact.content == claim;
        store
            .update_fact(
                &fact.address(),
                FactPatch {
                    clear_fields: carried,
                    status: left_empty.then_some(FactStatus::Archived),
                    details: left_empty
                        .then(|| format!("Archived when the {role} role moved onto role:{role}.")),
                    ..FactPatch::default()
                },
                bot,
            )
            .await?;
        touched += 1;
    }
    Ok(touched)
}

/// **What a pass over every bot did.**
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OldRoleKeysPass {
    /// The claims the pass wrote onto a role object.
    pub moved: usize,
    /// The records the pass took old role keys off.
    pub cleared: usize,
    /// One line for each role the pass could not finish, and why.
    pub failed: Vec<String>,
}

/// **Make the role object, as a child of the bot, the first time it is
/// written.** The creation is jojobot's own, so a near miss against another
/// seat, such as `dev` beside `dev2`, is not a question for a caller: the token
/// the guard minted for it is the answer.
pub async fn ensure_role_object<M: Memory + ?Sized>(
    store: &M,
    bot: &EntityId,
    object: &EntityId,
    role: &str,
) -> Result<(), MemoryError> {
    let new = NewEntity {
        parent: Some(bot.clone()),
        ..NewEntity::new(object.clone(), role, "jojobot")
    };
    match store.add_entity(new.clone()).await? {
        Guarded::Written(_) => Ok(()),
        Guarded::Blocked {
            attempted,
            candidates,
        } => {
            let token = guard::override_token(&attempted, &candidates);
            match store
                .add_entity(NewEntity {
                    override_token: Some(token),
                    // A resemblance of the name alone takes a reason (rule 498):
                    // a role is named for what it does, never for a bot.
                    override_reason: Some("a role object is named for the work it holds".into()),
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

/// **Carry one old-shape claim onto its role object.** The object is made when
/// it does not exist. The claim lands on the object's claim record, or on a new
/// one when the object has none, as the claim's own write: the store decides it
/// against the object's state, which holds no moment yet, so it lands.
async fn carry_claim_onto_object<M: Memory + ?Sized>(
    store: &M,
    bot: &EntityId,
    role: &str,
    holder: &str,
    at: Timestamp,
    object_exists: bool,
    today: Date,
) -> Result<(), MemoryError> {
    let object = EntityId::new(EntityKind::ROLE, role);
    if !object_exists {
        ensure_role_object(store, bot, &object, role).await?;
    }
    let fields: std::collections::BTreeMap<String, String> = [
        (ROLE_HOLDER.to_string(), holder.to_string()),
        (ROLE_CLAIMED_AT.to_string(), at.to_string()),
    ]
    .into_iter()
    .collect();
    let facts = store.recall(&object).await?;
    let record = facts
        .iter()
        .find(|f| f.fields.contains_key(ROLE_HOLDER) || f.fields.contains_key(ROLE_CLAIMED_AT))
        .or_else(|| facts.iter().find(|f| f.content == role_claim_content(role)))
        .map(|f| f.address());
    match record {
        Some(address) => {
            store
                .update_fact(
                    &address,
                    FactPatch {
                        derived: fields.keys().cloned().collect(),
                        fields,
                        ..FactPatch::default()
                    },
                    bot,
                )
                .await?;
        }
        None => {
            let mut fact = NewFact::about(object, role_claim_content(role), today);
            fact.derived = fields.keys().cloned().collect();
            fact.fields = fields;
            store.capture(fact).await?;
        }
    }
    Ok(())
}

/// **Move every old-shape claim onto its role object, and take the old keys
/// off every bot.** A role whose object carries a claim moment has moved
/// already, and only its keys come off. A role whose object has no moment, or
/// that has no object, is carried over with its holder and its moment when the
/// bot holds both and the moment reads; a half-written claim says nothing about
/// a lease, so only its keys come off. Archived bots are left as they are.
///
/// **One role that cannot be finished is named in the report and does not stop
/// the rest.** The keys of a role come off only after its claim has landed, so a
/// failure between the two leaves the bot's keys in place and the next run
/// finishes it. A bot with nothing to move costs one folded read, and a second
/// run finds nothing to write.
pub async fn move_old_role_claims<M: Memory + ?Sized>(
    store: &M,
    today: Date,
) -> Result<OldRoleKeysPass, MemoryError> {
    let mut pass = OldRoleKeysPass::default();
    for bot in store.list_entities(Some(EntityKind::BOT)).await? {
        if bot.archived.is_some() {
            continue;
        }
        let held = match store.fields(&bot.id).await {
            Ok(held) => held,
            Err(e) => {
                pass.failed.push(format!("{}: {e}", bot.id));
                continue;
            }
        };
        let roles: BTreeSet<&str> = held
            .keys()
            .filter_map(|key| role_from_field_key(key))
            .collect();
        for role in roles {
            let object = EntityId::new(EntityKind::ROLE, role);
            let (moved, exists) = match store.fields(&object).await {
                Ok(on_object) => (on_object.contains_key(ROLE_CLAIMED_AT), true),
                Err(MemoryError::UnknownEntity { .. }) => (false, false),
                Err(e) => {
                    pass.failed.push(format!("{} {role}: {e}", bot.id));
                    continue;
                }
            };
            if !moved {
                let claim = held.get(&role_holder_key(role)).zip(
                    held.get(&role_claimed_at_key(role))
                        .and_then(|at| at.parse::<Timestamp>().ok()),
                );
                if let Some((holder, at)) = claim {
                    match carry_claim_onto_object(store, &bot.id, role, holder, at, exists, today)
                        .await
                    {
                        Ok(()) => pass.moved += 1,
                        Err(e) => {
                            pass.failed.push(format!("{} {role}: {e}", bot.id));
                            continue;
                        }
                    }
                }
            }
            match drop_old_role_keys(store, &bot.id, role).await {
                Ok(touched) => pass.cleared += touched,
                Err(e) => pass.failed.push(format!("{} {role}: {e}", bot.id)),
            }
        }
    }
    Ok(pass)
}
