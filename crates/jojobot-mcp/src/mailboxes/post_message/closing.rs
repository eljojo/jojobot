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
//!   thread in the box it lands in that the reader has read: the newer one is
//!   the sender's current word, and the older ones are the history of it. A
//!   message still new was never handed over, and `supersedes` is how to replace
//!   one by name; this closes by thread and does not need to be asked.
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
    /// **The messages a close was meant for and could not be made**: a store
    /// error closing one, or reading what the post answers or the thread it is
    /// on. The post stands, and the receipt says these are still open.
    pub(super) failed: Vec<MessageId>,
    /// **The thread could not be looked at**, so the sender's older messages on it
    /// were not checked and none was closed. The receipt says so; it names no id,
    /// because no message is known to be open for this reason.
    pub(super) unchecked: bool,
    changed: crate::changed::Changed,
}

/// The ids of a list of messages, as the receipt names them.
fn named(ids: &[MessageId]) -> Vec<&str> {
    ids.iter().map(MessageId::as_str).collect()
}

impl Closed {
    /// Whether the post closed nothing and failed to close nothing.
    pub(super) fn is_empty(&self) -> bool {
        self.answered.is_none()
            && self.older.is_empty()
            && self.failed.is_empty()
            && !self.unchecked
    }

    /// What the post closed, and what it could not close, as the receipt carries it.
    pub(super) fn receipt(&self) -> serde_json::Value {
        let mut block = serde_json::Map::new();
        if let Some(answered) = &self.answered {
            block.insert("answered".into(), answered.as_str().into());
        }
        if !self.older.is_empty() {
            block.insert("older_on_the_thread".into(), named(&self.older).into());
        }
        if !self.failed.is_empty() {
            block.insert("failed".into(), named(&self.failed).into());
            block.insert(
                "failed_say".into(),
                "the post landed, and a close it makes could not be made, so these messages \
                 are still open. Close the ones you acted on with mark_processed."
                    .into(),
            );
        }
        if self.unchecked {
            block.insert("older_unchecked".into(), true.into());
            block.insert(
                "older_unchecked_say".into(),
                "the thread could not be read, so the sender's older messages on it were not \
                 checked and none was closed. Close the ones you acted on with mark_processed."
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
        let mine = match self.own_box(&caller.bot).await {
            OwnBox::The(mine) => mine,
            // The reply landed and its own box cannot be found, so what it
            // answers cannot be closed: say it is still open.
            OwnBox::Unreadable => {
                closed.failed.push(replied_to.clone());
                return closed;
            }
            _ => return closed,
        };
        let Ok(run) = self.acting_run(Some(sid)) else {
            closed.failed.push(replied_to.clone());
            return closed;
        };
        let acting = mailbox::Acting {
            session: run.as_deref(),
            at: self.clock().now(),
        };
        match self.mailboxes.message_by_id(replied_to).await {
            Ok(Some(answered))
                if answered.mailbox == mine
                    && answered.state != mailbox::MessageState::Processed =>
            {
                let note = format!("answered by {}", posted.id);
                match self.close_for_a_post(&answered, &mine, &note, acting).await {
                    Ok(done) => {
                        closed.changed = closed.changed.and(done);
                        closed.answered = Some(answered.id.clone());
                    }
                    Err(_) => closed.failed.push(answered.id.clone()),
                }
            }
            Ok(_) => {}
            Err(_) => closed.failed.push(replied_to.clone()),
        }
        // A message to a person is the person's own to close, whoever it answers.
        match self.is_private_box(&posted.mailbox).await {
            Some(false) => {}
            Some(true) => return closed,
            None => {
                closed.unchecked = true;
                return closed;
            }
        }
        let Some(root) = self.thread_root(replied_to).await else {
            closed.unchecked = true;
            return closed;
        };
        let Ok(sent) = self.mailboxes.sent_by(&[posted.sender.as_str()]).await else {
            closed.unchecked = true;
            return closed;
        };
        // **Only an older message the reader has read is closed.** One still new
        // was never handed over, and `supersedes` is the way to replace it.
        for older in sent.into_iter().filter(|m| {
            m.mailbox == posted.mailbox
                && m.id != posted.id
                && m.sent_at <= posted.sent_at
                && m.state == mailbox::MessageState::Read
        }) {
            // The thread's own first message is on it, and so is a reply into it.
            let on_the_thread = match &older.in_reply_to {
                None => Some(older.id == root),
                Some(parent) => self.thread_root(parent).await.map(|found| found == root),
            };
            match on_the_thread {
                Some(true) => {}
                Some(false) => continue,
                // Its ancestry cannot be read, so it may be on the thread: it is
                // still open, and the receipt names it.
                None => {
                    closed.failed.push(older.id.clone());
                    continue;
                }
            }
            let note = format!(
                "closed by a newer message from the same sender on this thread: {}",
                posted.id
            );
            match self
                .close_for_a_post(&older, &posted.mailbox, &note, acting)
                .await
            {
                Ok(done) => {
                    closed.changed = closed.changed.and(done);
                    closed.older.push(older.id.clone());
                }
                Err(_) => closed.failed.push(older.id.clone()),
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

    /// Whether the box is a person's, which no bot closes mail in. `None` when the
    /// board cannot be read: that is not an answer either way.
    async fn is_private_box(&self, name: &mailbox::MailboxName) -> Option<bool> {
        let boxes = self.mailboxes.list_mailboxes().await.ok()?;
        Some(
            boxes
                .iter()
                .any(|held| &held.name == name && held.is_private()),
        )
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
    let failed = if !closed.failed.is_empty() {
        " A close it makes could not be made: closed.failed names what is still open."
    } else if closed.unchecked {
        " The thread could not be read, so no older message was closed."
    } else {
        ""
    };
    format!(
        "The message is in {}'s box, waiting. {moved}, and nobody is obliged to read it before \
         they next open the box.{replaces}{failed}",
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
