//! **A bot folded into another has no box, and its mail goes to the survivor**
//! (rule 466).
//!
//! A fold leaves the folded bot's handle answering and leaves any box it had
//! where it was, so a post to the old handle would land in a box nobody reads.
//! The survivor is the one the store says the fold went to (rule 598): a bot's
//! `merged_into` is served as the handle the survivor wears now, whether the
//! merge kept an event or only a forwarding row, so this asks the listing and
//! decides nothing about where a fold went.

use jojobot_domain::memory::claim_walk::ThingWalk;
use jojobot_domain::memory::{Entity, MemoryError};

use super::*;

/// **A post that was redirected**: the handle the sender wrote and the bot that
/// received it.
pub(super) struct Fold {
    pub(super) sent_to: EntityId,
    pub(super) survivor: EntityId,
}

impl Jojobot {
    /// **The bot an addressee was folded into, at the end of any chain of
    /// folds**, or `None` when it was not folded. A memory that cannot be read is
    /// an error to retry, and a chain of folds that leads back to itself is stored
    /// damage: neither is read as an addressee that was not folded (rule 130).
    pub(super) async fn survivor_of(
        &self,
        addressee: &EntityId,
    ) -> Result<Option<Fold>, MemoryError> {
        if addressee.kind() != Some(EntityKind::BOT) {
            return Ok(None);
        }
        let bots = self.memory.list_entities(Some(EntityKind::BOT)).await?;
        if !bots
            .iter()
            .any(|bot| &bot.id == addressee && bot.merged_into.is_some())
        {
            return Ok(None);
        }
        follow(addressee, &bots)
    }
}

/// **Follow a bot's folds to their end.** Each hop is the bot's `merged_into`, as
/// the store serves it. A survivor that is no bot is damage, not an addressee.
pub(super) fn follow(addressee: &EntityId, bots: &[Entity]) -> Result<Option<Fold>, MemoryError> {
    let mut walk = ThingWalk::new(addressee.clone());
    loop {
        let at = walk.at().clone();
        let next = bots
            .iter()
            .find(|bot| bot.id == at)
            .and_then(|bot| bot.merged_into.clone());
        if let Some(next) = &next
            && !bots.iter().any(|bot| &bot.id == next)
        {
            return Err(MemoryError::StoredDamage {
                what: format!("{at} was folded into {next}, which is no bot jojobot holds"),
            });
        }
        if !walk.step(next) {
            break;
        }
    }
    let survivor = walk.finish()?;
    Ok((&survivor != addressee).then(|| Fold {
        sent_to: addressee.clone(),
        survivor,
    }))
}

/// **Say on the receipt that the post was redirected.**
pub(super) fn note(body: &mut serde_json::Value, fold: &Fold) {
    body["folded"] = serde_json::json!({
        "sent_to": fold.sent_to,
        "folded_into": fold.survivor,
        "note": format!(
            "sent to {}, which was folded into {}: the message is in {}'s box",
            fold.sent_to, fold.survivor, fold.survivor
        ),
    });
}
