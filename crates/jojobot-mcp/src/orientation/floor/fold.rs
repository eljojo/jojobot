//! **The boot floor of a bot that a merge leaves behind.**
//!
//! A merge serves the duplicate's rules and names under the survivor, so the
//! survivor's boot carries them. It is measured as the boot that follows the
//! merge, against the boot the survivor has now, and only a merge that grows the
//! floor past the ceiling is refused.

use super::*;
use crate::memory::claim_room::survivor_after_a_merge;

impl Jojobot {
    /// **The refusal for a merge that would leave the survivor bot's boot floor
    /// over the ceiling**, or `None`. The survivor holds the duplicate's claims
    /// and answers to the duplicate's names once the merge lands.
    pub(crate) async fn refuses_a_boot_floor_for_merge(
        &self,
        duplicate: &EntityId,
        survivor: &EntityId,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        if survivor.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let kept = self.memory.scan_entity(survivor).await.ok()??.entity?;
        let folded = self.memory.scan_entity(duplicate).await.ok()??.entity?;
        let merged = survivor_after_a_merge(&kept, &folded);
        let held = self.held_for_the_floor(survivor, reads).await?;
        let mut would_be = held.clone();
        for mut claim in self.held_for_the_floor(duplicate, reads).await? {
            claim.home = survivor.clone();
            would_be.push(claim);
        }
        let seats = self.seats_read(survivor, reads).await?;
        let Some(after) = self
            .boot_floor_if(
                survivor,
                &rules_in_force(&would_be),
                seats,
                None,
                None,
                Some(&merged),
            )
            .await
        else {
            Self::floor_unmeasured(survivor, reads);
            return None;
        };
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let Some(before) = self
            .boot_floor_if(survivor, &rules_in_force(&held), seats, None, None, None)
            .await
        else {
            Self::floor_unmeasured(survivor, reads);
            return None;
        };
        Self::over_the_ceiling(survivor, after, &before, 0)
    }
}
