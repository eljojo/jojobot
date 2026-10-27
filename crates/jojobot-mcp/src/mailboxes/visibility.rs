//! Gather the current fold and board for the domain's one mail visibility rule.

use super::*;
#[cfg(feature = "room-harness")]
use jojobot_domain::mailbox::visibility::every_bot_box;
use jojobot_domain::mailbox::visibility::{VisibleBoxes, visible_boxes};
use jojobot_domain::memory::REPORTS_TO;
use std::collections::BTreeMap;

impl Jojobot {
    pub(crate) async fn mail_visibility(
        &self,
        caller: Option<&EntityId>,
    ) -> Result<VisibleBoxes, McpError> {
        let board = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(mailbox_error)?;
        self.mail_visibility_on(caller, &board).await
    }

    pub(crate) async fn mail_visibility_on(
        &self,
        caller: Option<&EntityId>,
        board: &[mailbox::Mailbox],
    ) -> Result<VisibleBoxes, McpError> {
        // A grading handler is built only by the room harness and has no caller.
        #[cfg(feature = "room-harness")]
        if self.grading {
            return Ok(every_bot_box(board));
        }
        let Some(caller) = caller else {
            return Ok(visible_boxes(None, &BTreeMap::new(), board));
        };
        let mut chart = BTreeMap::new();
        for bot in self
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .map_err(memory_error)?
        {
            if let Some(manager) = self
                .memory
                .fields(&bot.id)
                .await
                .map_err(memory_error)?
                .get(REPORTS_TO)
            {
                chart.insert(bot.id, manager.clone());
            }
        }
        Ok(visible_boxes(Some(caller), &chart, board))
    }
}
