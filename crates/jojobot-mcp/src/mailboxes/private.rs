//! **A person's box is read by no bot.** The operator's mail is left by a bot
//! and read by the operator, so every verb that could hand a message's text to
//! a bot asks this module first.
//!
//! A box is private when its owner is a person (see
//! [`jojobot_domain::mailbox::Mailbox::is_private`]). The answer comes from the
//! board, never from the message, so it holds for a message in any state.
//!
//! **The operator's box opens at the first post to them.** A bot addresses the
//! operator by their handle, and only the person `topic:instance` names has a
//! box. Naming the operator opens nothing; the post is the intentional act, and
//! the handle is checked against the record exactly, so no typo opens a box.
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

/// **The name a person's box wears**: the kind's word and the slug, derived from
/// the owner and never chosen by a caller. A bot's box is named for its bare
/// slug, so this cannot be a bot's name unless a bot's slug begins `person-`,
/// and the create guard refuses an exact collision.
fn persons_box_name(owner: &EntityId) -> mailbox::MailboxName {
    mailbox::MailboxName(format!("person-{}", owner.slug()))
}

impl Jojobot {
    /// **The box a post to `addressee` goes into**, opening it when this is the
    /// first post to the operator. A refusal as the answer when the addressee is
    /// a person who is not the operator, when nobody is, or when the box cannot
    /// be found or opened.
    pub(crate) async fn operators_box(
        &self,
        addressee: &EntityId,
    ) -> Result<mailbox::MailboxName, CallToolResult> {
        let Some(operator) = self.instance_operator().await else {
            return Err(blocked_body(
                addressee,
                &[],
                format!(
                    "Nothing was written. No person is the operator yet, so no person has a \
                     mailbox. The operator is named on the instance's record: {}.",
                    crate::orientation::instance_zone::no_operator_yet(),
                ),
            ));
        };
        if &operator != addressee {
            return Err(blocked_body(
                addressee,
                &[],
                "Nothing was written. Only the operator has a mailbox among people, and this \
                 person is not the operator. To reach the operator, write to the handle the \
                 boot names under operator.",
            ));
        }
        match self.own_box(addressee).await {
            OwnBox::The(name) => return Ok(name),
            OwnBox::Several(boxes) => {
                return Err(blocked_body(
                    addressee,
                    &[],
                    format!(
                        "Nothing was written. The operator owns more than one mailbox ({}), and \
                         one person has exactly one. This is damage rather than anything you \
                         did: report it, it needs a person.",
                        boxes
                            .iter()
                            .map(|b| b.as_str().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ));
            }
            OwnBox::Unreadable => {
                return Err(blocked_body(
                    addressee,
                    &[],
                    "Nothing was written. jojobot could not read the mail board, so it cannot \
                     say where this belongs. Nothing is wrong with your call: try it again.",
                ));
            }
            OwnBox::None => {}
        }
        // The first post to the operator: open their box now.
        let name = persons_box_name(addressee);
        let mut token: Option<String> = None;
        for _ in 0..2 {
            match self
                .mailboxes
                .create_mailbox(&name, addressee, token.as_deref())
                .await
            {
                Ok(mailbox::Guarded::Written(opened)) => return Ok(opened.name),
                // **A resemblance is not a collision.** The name is derived, so
                // a bot's box `lisa` beside `person-lisa` is two boxes of two
                // owners, and the screen's token is what it hands back for
                // that. An exact name is a real collision and is never lifted.
                Ok(mailbox::Guarded::Blocked {
                    attempted,
                    candidates,
                }) if token.is_none()
                    && candidates
                        .iter()
                        .all(|c| c.reason != mailbox::guard::MatchReason::Exact) =>
                {
                    token = Some(mailbox::guard::override_token(&attempted, &candidates));
                }
                // Two first posts at once: the other one opened it. Take it.
                Ok(mailbox::Guarded::Blocked { .. }) => {
                    if let OwnBox::The(name) = self.own_box(addressee).await {
                        return Ok(name);
                    }
                    break;
                }
                Ok(mailbox::Guarded::UnknownOwner { .. }) | Err(_) => break,
            }
        }
        Err(blocked_body(
            addressee,
            &[],
            "Nothing was written. The operator has no mailbox yet and it could not be opened \
             now. Nothing is wrong with your call: try it again, and report it if it \
             repeats.",
        ))
    }
}
