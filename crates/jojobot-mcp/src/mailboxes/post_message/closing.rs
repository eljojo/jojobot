//! **What a post closes.** Closing mail is acting on it, and a post is acting on
//! two kinds of mail, so the post closes them instead of leaving each to a call of
//! its own:
//!
//! * **A reply closes what it answers.** A message that names `in_reply_to` a
//!   message in the poster's own box processes that message: replying is the act
//!   the message was waiting for. A reply to a message in another box changes
//!   nothing there.
//! * **A sender's newer word on a thread closes its older ones.** A message that
//!   answers into a thread processes the same sender's earlier messages on that
//!   thread in the box it lands in, whether the reader had read them or not: the
//!   newer one is the sender's current word, and the older ones are the history
//!   of it. `supersedes` still replaces an unread message by name; this closes by
//!   thread and does not need to be asked.
//!
//! The transition is the domain's `mark_processed`, as in every other verb that
//! closes mail. Mail in a person's box is moved by that person alone, so a post
//! never closes anything there.

use super::*;

/// How far up a thread of replies a root is looked for. A thread this deep is
/// not a thread anybody follows, and a cycle must not loop.
const MOST_HOPS: usize = 64;

/// What one post closed.
#[derive(Default)]
pub(super) struct Closed {
    /// The message the post answered, when the post closed it.
    pub(super) answered: Option<MessageId>,
    /// The sender's earlier messages on the thread that the post closed.
    pub(super) older: Vec<MessageId>,
    changed: crate::changed::Changed,
}

impl Closed {
    /// Whether the post closed anything.
    pub(super) fn is_empty(&self) -> bool {
        self.answered.is_none() && self.older.is_empty()
    }

    /// What the post closed, as the receipt carries it.
    pub(super) fn receipt(&self) -> serde_json::Value {
        let mut block = serde_json::Map::new();
        if let Some(answered) = &self.answered {
            block.insert("answered".into(), answered.as_str().into());
        }
        if !self.older.is_empty() {
            block.insert(
                "older_on_the_thread".into(),
                self.older
                    .iter()
                    .map(MessageId::as_str)
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        serde_json::Value::Object(block)
    }

    /// What it changed, to be added to the post's own.
    pub(super) fn into_changed(self) -> crate::changed::Changed {
        self.changed
    }
}

impl Jojobot {
    /// **Close what `posted` answers and what it replaces on its thread.**
    /// Nothing here may fail the post: a message that cannot be read or closed is
    /// left as it was, and the sender closes it by hand.
    pub(super) async fn close_what_a_post_settles(
        &self,
        sid: &str,
        caller: &crate::caller::Caller,
        posted: &Message,
    ) -> Closed {
        let mut closed = Closed::default();
        let Some(replied_to) = &posted.in_reply_to else {
            return closed;
        };
        let OwnBox::The(mine) = self.own_box(&caller.bot).await else {
            return closed;
        };
        let Ok(run) = self.acting_run(Some(sid)) else {
            return closed;
        };
        let acting = mailbox::Acting {
            session: run.as_deref(),
            at: self.clock().now(),
        };
        if let Ok(Some(answered)) = self.mailboxes.message_by_id(replied_to).await
            && answered.mailbox == mine
            && answered.state != mailbox::MessageState::Processed
        {
            let note = format!("answered by {}", posted.id);
            if let Ok(done) = self.close_for_a_post(&answered, &mine, &note, acting).await {
                closed.changed = closed.changed.and(done);
                closed.answered = Some(answered.id.clone());
            }
        }
        // A message to a person is the person's own to close, whoever it answers.
        if self.is_private_box(&posted.mailbox).await {
            return closed;
        }
        let Some(root) = self.thread_root(replied_to).await else {
            return closed;
        };
        let Ok(sent) = self.mailboxes.sent_by(&[posted.sender.as_str()]).await else {
            return closed;
        };
        for older in sent.into_iter().filter(|m| {
            m.mailbox == posted.mailbox
                && m.id != posted.id
                && m.sent_at <= posted.sent_at
                && m.state != mailbox::MessageState::Processed
        }) {
            // The thread's own first message is on it, and so is a reply into it.
            let on_the_thread = match &older.in_reply_to {
                None => older.id == root,
                Some(parent) => self.thread_root(parent).await.as_ref() == Some(&root),
            };
            if !on_the_thread {
                continue;
            }
            let note = format!(
                "closed by a newer message from the same sender on this thread: {}",
                posted.id
            );
            if let Ok(done) = self
                .close_for_a_post(&older, &posted.mailbox, &note, acting)
                .await
            {
                closed.changed = closed.changed.and(done);
                closed.older.push(older.id.clone());
            }
        }
        closed
    }

    /// The close both rules make: the domain transition, and what it changed.
    async fn close_for_a_post(
        &self,
        message: &Message,
        by: &mailbox::MailboxName,
        note: &str,
        acting: mailbox::Acting<'_>,
    ) -> Result<crate::changed::Changed, MailboxError> {
        let processed = self
            .mailboxes
            .mark_processed(&message.id, by, Some(note), acting)
            .await?;
        Ok(crate::changed::Changed::value(
            processed.id.as_str(),
            crate::changed::STATE,
            Some(message.state.as_token()),
            Some(processed.state.as_token()),
        ))
    }

    /// **The first message of the thread `from` belongs to**, found by following
    /// each message's `in_reply_to` up. `None` when a link cannot be read.
    async fn thread_root(&self, from: &MessageId) -> Option<MessageId> {
        let mut at = from.clone();
        for _ in 0..MOST_HOPS {
            let message = self.mailboxes.message_by_id(&at).await.ok()??;
            match message.in_reply_to {
                Some(parent) => at = parent,
                None => return Some(at),
            }
        }
        None
    }

    /// Whether the box is a person's, which no bot closes mail in.
    async fn is_private_box(&self, name: &mailbox::MailboxName) -> bool {
        match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes
                .iter()
                .any(|held| &held.name == name && held.is_private()),
            // A board that cannot be read cannot say a box is not private.
            Err(_) => true,
        }
    }
}

/// **What a post left standing**, from the message and what the post closed. The
/// mail the same call took delivery of is added by [`took`].
pub(super) fn what_a_post_left_standing(message: &Message, closed: &Closed) -> String {
    let replaces = match &message.supersedes {
        Some(replaced) => format!(
            " It replaces {replaced}: if that one is still unread, its reader is handed this \
             one instead and is told that {replaced} was replaced."
        ),
        None => String::new(),
    };
    let moved = if closed.is_empty() {
        "Nothing else in that box moved"
    } else {
        "What it closed is named under closed, each processed with a note naming this \
         message, and nothing else in that box moved"
    };
    format!(
        "The message is in {}'s box, waiting. {moved}, and nobody is obliged to read it before \
         they next open the box.{replaces}",
        message.mailbox.as_str(),
    )
}

/// **The mail the same call took delivery of**, for the postcondition.
pub(super) fn took(collected: u64) -> String {
    match collected {
        0 => String::new(),
        n => format!(
            " This call also took delivery of {n} of your own: they are out of new and yours to \
             finish with mark_processed."
        ),
    }
}
