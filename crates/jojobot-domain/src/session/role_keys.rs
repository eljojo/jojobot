//! **The previous shape of a role's lease, taken off the bot that held it.**
//!
//! Before a role was its own object, the holder and the claim moment lived on
//! the bot as `role/<role>/holder` and `role/<role>/claimed_at`. Once the role
//! object carries a moment it answers for the lease and those keys say nothing
//! true, but they stay on the bot until something takes them off, and a reader
//! that scans a bot's keys reads them as a lapsed lease.

use std::collections::BTreeSet;

use crate::memory::{EntityId, EntityKind, FactPatch, FactStatus, Memory, MemoryError};

use super::{ROLE_CLAIMED_AT, role_claimed_at_key, role_from_field_key, role_holder_key};

/// **What the claim record of a role says about itself**, so a role claimed
/// before it was an object has somewhere to move to, and so the record the move
/// leaves empty can be told from a record somebody wrote.
pub fn role_claim_content(role: &str) -> String {
    format!("claimed the {role} role")
}

/// **Take `role`'s old holder and moment off `bot`**, and return how many of its
/// records held them. Call it only once the role object carries a moment of its
/// own, because until then the old keys are the only lease there is.
///
/// A record keeps whatever else it holds. A record left with no field whose
/// words are the claim's own is archived: an active record with no role key
/// reads as a rule the bot has to follow. Nothing else of the bot changes, so
/// another role's keys stay.
///
/// A bot with none of the keys costs one folded read and no write, which is what
/// lets a renewal call this on every write. The pass is idempotent.
///
/// **The role object's write and this one are two store calls, not one
/// transaction.** The object is written first. If this call then fails, the
/// object still answers for the lease, because [`super::role_state`] reads it
/// before the bot's keys, so the keys that are left are inert. The next write of
/// the role, or the next start, takes them off.
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
    /// The records the pass took old role keys off.
    pub cleared: usize,
    /// One line for each role whose keys could not be taken off, and why.
    pub failed: Vec<String>,
}

/// **Take the old keys off every bot, for each role that has moved.** A role
/// has moved when its object carries a claim moment: from then on the object
/// answers for the lease. A bot whose role has no such object keeps its keys,
/// because they are the lease. Archived bots are left as they are.
///
/// One role that cannot be cleared is named in the report and does not stop the
/// rest, so the pass can run at every start and finish a little more each time.
/// It is idempotent, and a bot with nothing to take off costs one folded read.
pub async fn drop_old_role_keys_of_moved_roles<M: Memory + ?Sized>(
    store: &M,
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
            match store.fields(&object).await {
                Ok(on_object) if on_object.contains_key(ROLE_CLAIMED_AT) => {}
                Ok(_) | Err(MemoryError::UnknownEntity { .. }) => continue,
                Err(e) => {
                    pass.failed.push(format!("{} {role}: {e}", bot.id));
                    continue;
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
