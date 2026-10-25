//! **The boot floor of a bot that a rename leaves behind.**
//!
//! A bot's handle rides in its id, in the name of its box and in the address of
//! every rule it carries, so a longer handle makes a longer boot. A rename is
//! measured as the boot under the new handle, against the boot the bot has now,
//! and only a rename that grows the floor past the ceiling is refused.

use super::*;

impl Jojobot {
    /// **The refusal for a rename that would leave the bot's boot floor over the
    /// ceiling**, or `None`.
    pub(crate) async fn refuses_a_boot_floor_for_rename(
        &self,
        from: &EntityId,
        to: &EntityId,
        reads: &ReadsLeft,
    ) -> Option<MemoryError> {
        if from.kind() != Some(EntityKind::BOT) || to.kind() != Some(EntityKind::BOT) {
            return None;
        }
        let mut renamed = self.memory.scan_entity(from).await.ok()??.entity?;
        renamed.id = to.clone();
        let held = self.held_for_the_floor(from, reads).await?;
        let mut rehomed = held.clone();
        for claim in &mut rehomed {
            claim.home = to.clone();
            claim.subject = to.clone();
        }
        let seats = self.seats_read(from, reads).await?;
        let Some(after) = self
            .boot_floor_if(
                from,
                &rules_in_force(&rehomed),
                seats,
                None,
                None,
                Some(&renamed),
            )
            .await
        else {
            Self::floor_unmeasured(from, reads);
            return None;
        };
        if after.total <= jojobot_domain::text::BOOT_ANSWER.budget {
            return None;
        }
        let Some(before) = self
            .boot_floor_if(from, &rules_in_force(&held), seats, None, None, None)
            .await
        else {
            Self::floor_unmeasured(from, reads);
            return None;
        };
        Self::over_the_ceiling(to, after, &before, 0)
    }
}
