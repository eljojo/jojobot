//! **A person's box is read by no bot.** The operator's mail is left by a bot
//! and read by the operator, so every verb that could hand a message's text to
//! a bot asks this module first.
//!
//! A box is private when its owner is a person (see
//! [`jojobot_domain::mailbox::Mailbox::is_private`]). The answer comes from the
//! board, never from the message, so it holds for a message in any state.
//!
//! **A board that cannot be read refuses.** Whether a box is private is the
//! question every guard here rests on, and a guess that said no would hand out
//! the text it exists to keep.

use super::*;

impl Jojobot {
    /// Whether the box named is a person's. An error when the board cannot say,
    /// so the caller refuses rather than reading on.
    pub(crate) async fn box_is_private(
        &self,
        name: &mailbox::MailboxName,
    ) -> Result<bool, mailbox::MailboxError> {
        Ok(self
            .mailboxes
            .list_mailboxes()
            .await?
            .iter()
            .any(|held| &held.name == name && held.is_private()))
    }
}

/// **The refusal for a message in a person's box.** It names the message the
/// caller gave and nothing about it: not the box, not the sender, not the
/// subject, not whether it exists as mail.
pub(crate) fn private_box(id: &MessageId) -> CallToolResult {
    let how_to_proceed: WayForward = format!(
        "Nothing was delivered and nothing moved. Message '{id}' is not one any bot reads. \
         Mail written to a person is read by that person alone, on a surface that is not this \
         one, and no verb here opens it, in any state. To reach that person, post_message to \
         their handle."
    )
    .into();
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": id.as_str(),
        "wrote": false,
        "how_to_proceed": how_to_proceed.as_str(),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}
