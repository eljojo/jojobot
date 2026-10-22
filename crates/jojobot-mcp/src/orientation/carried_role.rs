//! **The role a bot claims by default**, read off its own fields at the boot door.
//! A read that fails never reads as a bot with no role (rule 130): the boot goes on
//! without claiming and the answer names the claim it could not make.

use super::*;
use crate::memory::reads_left::{CARRIED_ROLE, ReadsLeft};

impl Jojobot {
    /// **The role the bot carries in `claims_role`**, when the call named no claim
    /// of its own and is not presenting a wrap code. `None` when it carries none,
    /// and `None` with the read named in `reads` when the fields could not be read.
    pub(super) async fn carried_role(
        &self,
        bot: Option<&EntityId>,
        claim: Option<&str>,
        by_wrap_code: bool,
        reads: &ReadsLeft,
    ) -> Option<String> {
        let (None, Some(bot)) = (claim, bot) else {
            return None;
        };
        if by_wrap_code {
            return None;
        }
        match self.memory.fields(bot).await {
            Ok(fields) => fields
                .get(jojobot_domain::memory::CLAIMS_ROLE)
                .map(|role| role.trim().to_string())
                .filter(|role| !role.is_empty()),
            // A bot nobody created carries no role, and the door answers it with the
            // roster: that is a miss and no failed read.
            Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }) => None,
            Err(e) => {
                reads.left(CARRIED_ROLE, bot, &e);
                None
            }
        }
    }
}
