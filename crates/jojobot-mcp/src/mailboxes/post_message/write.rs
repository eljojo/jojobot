//! Message decisions shared by the MCP handler and account-free room furniture.

use super::*;

#[derive(serde::Deserialize)]
pub(crate) struct MessageWrite {
    pub(super) to: String,
    pub(super) body: String,
    pub(super) subject: Option<String>,
    pub(super) in_reply_to: Option<String>,
    pub(super) supersedes: Option<String>,
}

pub(crate) struct Recipient {
    pub(super) addressee: EntityId,
    pub(super) to_the_operator: bool,
    pub(super) fold: Option<folded::Fold>,
}

#[derive(Default)]
pub(crate) struct MessageAudit {
    pub(super) sender_mail_waiting_at_send: Option<usize>,
    pub(super) posted_by_session: Option<String>,
}

impl Jojobot {
    pub(crate) async fn message_recipient(
        &self,
        args: &MessageWrite,
    ) -> Result<Result<Recipient, CallToolResult>, McpError> {
        // **Screened here so the refusal is an ANSWER.** A subject the record
        // cannot carry is a caller mistake, and every other caller mistake on
        // this surface comes back blocked with a way forward — this one reached
        // the store's validator and came back as a protocol error, which is a
        // failure rather than a next move (rule 68). The domain still refuses
        // it; what this decides is the shape the caller sees.
        if let Err(e) = mailbox::validate_subject(args.subject.as_deref()) {
            return Ok(Err(subject_declined(
                args.subject.as_deref().unwrap_or_default(),
                &e,
            )));
        }
        // **The addressee is a bot, and its box is found by owner.** A box is
        // named for its handle by convention and nothing enforces that, so
        // reading ownership is what makes the address a fact rather than a
        // guess about what somebody called their box.
        let addressee = bot_handle(&args.to);
        // **A name that is no handle is refused as one**, before anything looks
        // for a box. Without this, `In Box!` reads as a colleague nobody has
        // heard of — which sends the caller looking for a missing bot when what
        // is wrong is the name they typed.
        if let Err(e) = jojobot_domain::memory::validate_subject(&addressee) {
            return Ok(Err(misused(format!(
                "Nothing was written: {e}. Write to a bot by name — a bare name like 'gamma', or \
                 its full handle."
            ))));
        }
        if let Some(refused) = self.refuses_a_non_person_operator(&addressee).await {
            return Ok(Err(refused));
        }
        // **A bot folded into another has no box of its own**: the post goes to
        // the survivor, and the receipt says so.
        let (addressee, fold) = match self.survivor_of(&addressee).await {
            Ok(Some(fold)) => (fold.survivor.clone(), Some(fold)),
            Ok(None) => (addressee, None),
            Err(e) => return crate::memory::declined::memory_declined("post_message", e).map(Err),
        };
        // **A person is addressed by their handle, and only the operator has a
        // box.** Posting to the operator is what opens it the first time.
        let to_the_operator = addressee.kind() == Some(EntityKind::PERSON);
        Ok(Ok(Recipient {
            addressee,
            to_the_operator,
            fold,
        }))
    }

    pub(crate) async fn message_destination(
        &self,
        recipient: &Recipient,
    ) -> Result<Result<MailboxName, CallToolResult>, McpError> {
        let addressee = &recipient.addressee;
        let destination = match addressee.kind() {
            Some(EntityKind::PERSON) => match self.operators_box(addressee).await {
                Ok(name) => name,
                Err(refused) => return Ok(Err(refused)),
            },
            _ => match self.own_box(addressee).await {
                OwnBox::The(name) => name,
                elsewhere => return Ok(Err(self.no_such_addressee(addressee, elsewhere).await)),
            },
        };
        Ok(Ok(destination))
    }

    pub(crate) async fn post_message_write(
        &self,
        author: &EntityId,
        args: MessageWrite,
        recipient: &Recipient,
        destination: MailboxName,
        audit: MessageAudit,
    ) -> Result<Result<Message, CallToolResult>, McpError> {
        let to_the_operator = recipient.to_the_operator;
        let supersedes = args
            .supersedes
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| MessageId(id.to_string()));
        // **A post to the operator quotes them by address, and the address is
        // checked.** Expanded here, before the body is measured and before it is
        // written, so what is stored and what is measured against the ceiling are
        // the same words.
        let body = if to_the_operator {
            match self.quotes_expanded(&args.body).await {
                Ok(expanded) => expanded,
                Err(e) => return mailbox_declined(e).map(Err),
            }
        } else {
            args.body
        };
        let new = NewMessage {
            mailbox: destination,
            body,
            subject: args.subject,
            sender: author.as_str().to_string(),
            // Stamped here, at the edge, for the same reason `capture` stamps a
            // date here: the domain stays clock-free, and a caller does not get
            // to backdate a message it is posting now.
            sent_at: self.clock().now(),
            in_reply_to: args
                .in_reply_to
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| MessageId(id.to_string())),
            supersedes,
            sender_mail_waiting_at_send: audit.sender_mail_waiting_at_send,
            posted_by_session: audit.posted_by_session,
        };
        // **A message nobody could be handed whole is not posted.** The reader
        // of a box is handed the messages in it under the answer ceiling, and
        // one whose body cannot fit even alone would come back with its body
        // left out. Refused here, before anything is written, with the ways
        // to send it.
        if let Some(room) = body_room_beyond(&new) {
            return mailbox_declined(MailboxError::InvalidMessage(format!(
                "this body is {} characters and the largest a reader can be handed whole is \
                 about {room}. Split it into several messages, or put the long text on a document \
                 with put_document and send its address",
                new.body.chars().count()
            )))
            .map(Err);
        }
        // Declined rather than errored: a reply naming a message jojobot does
        // not hold is a bad reference, and every other bad reference on this
        // surface comes back as the blocked shape.
        let posted = match jojobot_domain::request::armed_part(
            "post",
            None,
            self.mailboxes.post_message(new),
        )
        .await
        {
            Ok(posted) => posted,
            Err(e) => return mailbox_declined(e).map(Err),
        };
        match posted {
            mailbox::Guarded::Written(message) => Ok(Ok(message)),
            mailbox::Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(Err(mailbox_blocked(
                &attempted,
                &candidates,
                BlockedBox::MustExist("post_message"),
            ))),
            mailbox::Guarded::UnknownOwner {
                attempted,
                candidates,
            } => Ok(Err(unknown_owner(&attempted, &candidates))),
        }
    }
}
