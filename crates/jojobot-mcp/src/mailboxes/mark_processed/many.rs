//! **Closing many messages in one call**, so closing mail costs what reading it
//! does. The transition itself is the domain's `mark_processed`, called once per
//! message; this file decides which messages a call may close, refuses the whole
//! call when any one may not be, and answers with the ids and a count.

use super::*;

/// The most messages one call closes.
const MOST_IDS: usize = 200;

impl Jojobot {
    /// **Close every message named, or none.** Every id is checked before any is
    /// written: one that names nothing, that cannot be read, that is in another
    /// box or in a person's refuses the call, names that id, and leaves every
    /// message as it was. A message already processed is left alone and counted,
    /// which is what makes a resend of the same call safe.
    pub(super) async fn close_many(
        &self,
        args: &MarkProcessedArgs,
        ids: &[String],
        acting: mailbox::Acting<'_>,
    ) -> Result<CallToolResult, McpError> {
        if args.quarantine.is_some() {
            return Ok(misused(
                "Nothing was written: `message_ids` closes messages, and a quarantine is a \
                 decision about one. Quarantine them one at a time with `message_id`.",
            ));
        }
        let mut wanted: Vec<MessageId> = Vec::new();
        for id in ids.iter().map(|id| id.trim()).filter(|id| !id.is_empty()) {
            let id = MessageId(id.to_string());
            if !wanted.contains(&id) {
                wanted.push(id);
            }
        }
        if wanted.is_empty() || wanted.len() > MOST_IDS {
            return Ok(misused(format!(
                "Nothing was written: `message_ids` names {} messages, and a call closes from 1 \
                 to {MOST_IDS}. Send the ids in calls of that size.",
                wanted.len()
            )));
        }
        let mine = match self.my_box(args.sid.as_deref()).await {
            Ok(mine) => mine,
            Err(refused) => return Ok(refused),
        };
        let mut open: Vec<Message> = Vec::new();
        let mut already: Vec<MessageId> = Vec::new();
        for id in &wanted {
            let located = match self.mailboxes.message_by_id(id).await {
                Ok(located) => located,
                Err(e) => return mailbox_declined(e),
            };
            if let Some(refused) = self
                .privacy_refusal(id, located.as_ref())
                .await
                .map_err(mailbox_error)?
            {
                return Ok(refused);
            }
            let Some(message) = located else {
                return mailbox_declined(MailboxError::UnknownMessage {
                    attempted: id.to_string(),
                });
            };
            if message.mailbox != mine.name {
                return mailbox_declined(MailboxError::NotYourMessage {
                    attempted: id.to_string(),
                    mailbox: message.mailbox.as_str().to_string(),
                    by: mine.name.as_str().to_string(),
                });
            }
            if message.state == mailbox::MessageState::Processed {
                already.push(id.clone());
            } else {
                open.push(message);
            }
        }
        let mut changed = crate::changed::Changed::default();
        let mut closed: Vec<&str> = Vec::new();
        for message in &open {
            let part = format!("close-{}", message.id);
            match jojobot_domain::request::armed_part(
                &part,
                None,
                self.mailboxes.mark_processed(
                    &message.id,
                    &mine.name,
                    args.notes.as_deref(),
                    acting,
                ),
            )
            .await
            {
                Ok(processed) => {
                    changed = changed.and(crate::changed::Changed::value(
                        processed.id.as_str(),
                        crate::changed::STATE,
                        Some(message.state.as_token()),
                        Some(processed.state.as_token()),
                    ));
                    closed.push(message.id.as_str());
                }
                Err(e) => return mailbox_declined(e),
            }
        }
        if let Some(first) = closed.first() {
            self.beat("mark_processed", first, args.sid.as_deref())
                .await;
        }
        let mut body = serde_json::json!({
            "processed": closed.len(),
            "ids": closed,
            "already_processed": already.iter().map(MessageId::as_str).collect::<Vec<_>>(),
        });
        if let Some(notes) = args
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            body["notes"] = notes.into();
        }
        crate::answer::note_postcondition(
            &mut body,
            format!(
                "{} messages are processed, which is the last state on this rail. Each is \
                 archived and not deleted: it stays readable, and a read of it moves nothing. \
                 The same call sent again closes none twice.",
                closed.len()
            ),
        );
        crate::changed::note_changed(&mut body, changed);
        json_result(&body)
    }
}
