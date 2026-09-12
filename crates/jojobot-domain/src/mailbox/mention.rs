//! **A handle written into a message is a link, not a spelling** — the same
//! mechanism [`crate::memory::mention`] gives a claim's words, carried onto
//! the mailbox world.
//!
//! A message's body, its subject and the notes a `mark_processed` records are
//! free text a caller writes, exactly as a claim's content is. `Mentioning`
//! resolves an `@kind:slug` written into any of the three into the badge its
//! row wears, so a rename that moves the handle reaches the message too — a
//! message posted before a rename still reads under the thing's current name
//! when it is read after one.
//!
//! **Resolved on the way in, rendered on the way out, and nothing else.**
//! Unlike [`crate::memory::mention::Mentioning::capture`], a mention here is
//! never screened: a message is not a claim about the thing it names, and
//! `post_message`'s existing guard is the mailbox name's, not the body's — an
//! unresolvable mention is left exactly as written, the same treatment a
//! thing's own prose already gets.

use std::sync::Arc;

use crate::memory::{
    Entity, EntityId, FormerHandle, Memory, entity_wearing, mention, resolve_handle,
};

use super::{
    Delivered, Delivery, Mailbox, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, TakenBy,
};

/// Wraps any [`Mailboxes`] so a handle written into a message's body,
/// subject or notes is stored as the permanent id and served as the handle
/// the thing wears today — **and so is `sender` itself**, the same rule
/// applied to the one column that names who sent a message rather than
/// mentioning them in its text. Without this, a message outlives the name
/// its sender was posted under: `list_sent`'s exact match against the
/// caller's current handle stops finding mail sent before a rename, and a
/// bot that has just been renamed asks what it sent and is told none —
/// a confident zero standing in for mail that is still there.
pub struct Mentioning {
    inner: Arc<dyn Mailboxes>,
    memory: Arc<dyn Memory>,
}

impl Mentioning {
    pub fn new(inner: Arc<dyn Mailboxes>, memory: Arc<dyn Memory>) -> Self {
        Mentioning { inner, memory }
    }

    /// What exists, for a mention to resolve against on the way in and render
    /// against on the way out — one read of the index answers both.
    async fn known(&self) -> Result<Vec<Entity>, MailboxError> {
        self.memory
            .list_entities(None)
            .await
            .map_err(|e| MailboxError::Store(e.to_string()))
    }

    /// Every rename this store still remembers — needed only to resolve a
    /// `sender` handle that may itself be stale. Rendering never needs this:
    /// [`entity_wearing`] finds the current wearer of a badge directly,
    /// whatever it used to be called.
    async fn former(&self) -> Result<Vec<FormerHandle>, MailboxError> {
        self.memory
            .former_handles()
            .await
            .map_err(|e| MailboxError::Store(e.to_string()))
    }

    /// **The value `sender` is stored under**: the badge of whoever it
    /// resolves to, or the raw string unchanged when it does not resolve at
    /// all, or resolves to a row with no badge yet — the same fallback the
    /// real store's own pointer columns use, so posting is never blocked on
    /// this.
    fn storage_sender_for(sender: &str, known: &[Entity], former: &[FormerHandle]) -> String {
        match resolve_handle(&EntityId(sender.to_string()), known, former) {
            Some(entity) => entity.badge.clone().unwrap_or_else(|| entity.id.0.clone()),
            None => sender.to_string(),
        }
    }

    /// **The handle `sender` renders as today**: whoever currently wears the
    /// stored badge, or the stored value unchanged when nobody does — a
    /// value stored before this existed, which is a plain handle rather than
    /// a badge, and reads back exactly as it did before.
    fn current_sender_for(stored: &str, known: &[Entity]) -> String {
        entity_wearing(stored, known)
            .map(|e| e.id.0.clone())
            .unwrap_or_else(|| stored.to_string())
    }

    fn render(message: &mut Message, known: &[Entity]) {
        message.sender = Self::current_sender_for(&message.sender, known);
        message.body = mention::rendered(&message.body, known);
        if let Some(subject) = &message.subject {
            message.subject = Some(mention::rendered(subject, known));
        }
        if let Some(notes) = &message.notes {
            message.notes = Some(mention::rendered(notes, known));
        }
    }

    async fn render_all(&self, messages: &mut [Message]) -> Result<(), MailboxError> {
        if messages.is_empty() {
            return Ok(());
        }
        let known = self.known().await?;
        for message in messages {
            Self::render(message, &known);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Mailboxes for Mentioning {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &crate::memory::EntityId,
        override_token: Option<&str>,
    ) -> Result<super::Guarded<Mailbox>, MailboxError> {
        self.inner.create_mailbox(name, owner, override_token).await
    }

    async fn repoint_owner(
        &self,
        from: &crate::memory::EntityId,
        to: &crate::memory::EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        self.inner.repoint_owner(from, to).await
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.inner.list_mailboxes().await
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        let mut messages = self.inner.scan_messages().await?;
        self.render_all(&mut messages).await?;
        Ok(messages)
    }

    /// **Resolved on the way in.** The handle an author wrote becomes the
    /// badge its row wears, so the message keeps a pointer rather than a
    /// spelling.
    ///
    /// **Never screened, on purpose — do not add it.** A message is not a
    /// claim about the thing it names, so an unknown handle is not this
    /// verb's write to refuse: refusing to deliver mail over a mention
    /// jojobot has not met would make the mailbox worse than serving it
    /// marked. `post_message`'s own guard is the mailbox name's, never the
    /// body's.
    async fn post_message(
        &self,
        message: NewMessage,
    ) -> Result<super::Guarded<Message>, MailboxError> {
        let known = self.known().await?;
        let former = self.former().await?;
        let sender = Self::storage_sender_for(&message.sender, &known, &former);
        let written = self
            .inner
            .post_message(NewMessage {
                body: mention::resolved(&message.body, &known),
                subject: message
                    .subject
                    .as_deref()
                    .map(|s| mention::resolved(s, &known)),
                sender,
                ..message
            })
            .await?;
        Ok(match written {
            super::Guarded::Written(mut stored) => {
                Self::render(&mut stored, &known);
                super::Guarded::Written(stored)
            }
            blocked => blocked,
        })
    }

    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
    ) -> Result<super::Guarded<Delivery>, MailboxError> {
        let delivery = self.inner.read_mailbox(name, taken_by).await?;
        Ok(match delivery {
            super::Guarded::Written(mut delivery) => {
                if !delivery.messages.is_empty() {
                    let known = self.known().await?;
                    for delivered in &mut delivery.messages {
                        Self::render(&mut delivered.message, &known);
                    }
                }
                super::Guarded::Written(delivery)
            }
            blocked => blocked,
        })
    }

    async fn read_message(&self, id: &MessageId) -> Result<Delivered, MailboxError> {
        let mut delivered = self.inner.read_message(id).await?;
        let known = self.known().await?;
        Self::render(&mut delivered.message, &known);
        Ok(delivered)
    }

    /// **Resolved on the way in, exactly as `post_message`'s body is.**
    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        let known = self.known().await?;
        let resolved_notes = notes.map(|n| mention::resolved(n, &known));
        let mut message = self
            .inner
            .mark_processed(id, resolved_notes.as_deref())
            .await?;
        Self::render(&mut message, &known);
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::Timestamp;
    use jiff::civil::date;

    use super::*;
    use crate::mailbox::testing::InMemoryMailboxes;
    use crate::memory::NewEntity;
    use crate::memory::testing::InMemoryMemory;

    fn epoch() -> Timestamp {
        Timestamp::from_second(1_780_000_000).expect("a valid fixed instant")
    }

    async fn add(memory: &dyn Memory, id: &crate::memory::EntityId) {
        memory
            .add_entity(NewEntity::new(id.clone(), id.slug(), "contract-fixture"))
            .await
            .expect("add_entity should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block a fresh handle {id}"));
    }

    #[tokio::test]
    async fn a_renamed_things_mailbox_mention_follows_it() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let was = crate::memory::EntityId("thing:contract-mailbox-mention-was".into());
        let now = crate::memory::EntityId("thing:contract-mailbox-mention-now".into());
        add(memory.as_ref(), &was).await;

        let owner = crate::memory::EntityId("bot:gamma".into());
        let bare = Arc::new(InMemoryMailboxes::new());
        bare.know_owner(&owner);
        let mailboxes = Mentioning::new(bare.clone(), memory.clone());
        let mailbox_name = MailboxName("gamma".to_string());

        mailboxes
            .create_mailbox(&mailbox_name, &owner, None)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("the guard must not block creating gamma's box");

        let posted = mailboxes
            .post_message(NewMessage {
                mailbox: mailbox_name.clone(),
                body: format!("about @{was}"),
                subject: Some(format!("re @{was}")),
                sender: "bot:epsilon".to_string(),
                sent_at: epoch(),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .expect("the guard must not block posting");
        assert_eq!(posted.body, format!("about @{was}"));
        assert_eq!(
            posted.subject.as_deref(),
            Some(format!("re @{was}")).as_deref()
        );

        mailboxes
            .mark_processed(&posted.id, Some(&format!("handled, see @{was}")))
            .await
            .expect("mark_processed should succeed");

        memory
            .rename_entity(&was, &now, None, date(2026, 6, 2), None)
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the rename");

        let delivered = mailboxes
            .read_message(&posted.id)
            .await
            .expect("read_message should succeed");
        assert_eq!(delivered.message.body, format!("about @{now}"));
        assert_eq!(
            delivered.message.subject.as_deref(),
            Some(format!("re @{now}")).as_deref()
        );
        assert_eq!(
            delivered.message.notes.as_deref(),
            Some(format!("handled, see @{now}")).as_deref()
        );

        // The bare store never held either spelling — only the badge, which is
        // what proves the rename reached the message rather than the read
        // guessing at the current handle from the old text.
        let stored = bare
            .scan_messages()
            .await
            .expect("scan_messages should succeed");
        let raw = stored
            .iter()
            .find(|m| m.id == posted.id)
            .expect("the message is still there");
        assert!(!raw.body.contains(was.as_str()));
        assert!(!raw.body.contains(now.as_str()));
        assert!(raw.body.contains(mention::MARK));
    }

    /// 🚨 **A renamed bot's SENT mail is still found under its new name.**
    ///
    /// This is the `sender` column itself, not a mention written into free
    /// text: it is compared exactly, by `list_sent`, against the caller's
    /// current handle — so a stored value that never moves off the old
    /// handle leaves every message this bot sent before the rename
    /// unfindable by its new name.
    #[tokio::test]
    async fn a_renamed_bots_sent_message_renders_under_the_current_handle() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let was = crate::memory::EntityId("bot:gamma".into());
        let now = crate::memory::EntityId("bot:sigma".into());
        add(memory.as_ref(), &was).await;

        let owner = crate::memory::EntityId("bot:delta".into());
        let bare = Arc::new(InMemoryMailboxes::new());
        bare.know_owner(&owner);
        let mailboxes = Mentioning::new(bare.clone(), memory.clone());
        let mailbox_name = MailboxName("delta".to_string());

        mailboxes
            .create_mailbox(&mailbox_name, &owner, None)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("the guard must not block creating delta's box");

        let posted = mailboxes
            .post_message(NewMessage {
                mailbox: mailbox_name.clone(),
                body: "the kiln slice is done".to_string(),
                subject: None,
                sender: was.to_string(),
                sent_at: epoch(),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .expect("the guard must not block posting");
        assert_eq!(posted.sender, was.to_string());

        memory
            .rename_entity(&was, &now, None, date(2026, 6, 2), None)
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the rename");

        let delivered = mailboxes
            .read_message(&posted.id)
            .await
            .expect("read_message should succeed");
        assert_eq!(
            delivered.message.sender,
            now.to_string(),
            "the sent message must render under the current handle"
        );

        // The bare store never held either spelling — only the badge, which
        // is what proves the rename reached `sender` rather than the read
        // guessing at the current handle from the old one.
        let stored = bare
            .scan_messages()
            .await
            .expect("scan_messages should succeed");
        let raw = stored
            .iter()
            .find(|m| m.id == posted.id)
            .expect("the message is still there");
        assert_ne!(raw.sender, was.to_string());
        assert_ne!(raw.sender, now.to_string());
    }

    #[tokio::test]
    async fn a_never_existed_handle_is_marked_but_not_screened() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let owner = crate::memory::EntityId("bot:delta".into());
        let bare = Arc::new(InMemoryMailboxes::new());
        bare.know_owner(&owner);
        let mailboxes = Mentioning::new(bare.clone(), memory);
        let mailbox_name = MailboxName("delta".to_string());
        mailboxes
            .create_mailbox(&mailbox_name, &owner, None)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("the guard must not block creating delta's box");

        let never = crate::memory::EntityId("thing:contract-mailbox-mention-never-existed".into());
        let posted = mailboxes
            .post_message(NewMessage {
                mailbox: mailbox_name,
                body: format!("about @{never}"),
                subject: None,
                sender: "bot:epsilon".to_string(),
                sent_at: epoch(),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .expect("posting an unresolvable mention is not screened here");
        assert_eq!(posted.body, format!("about @{never} (no such handle)"));

        // Storage itself holds exactly what was typed — the marker above is
        // read-side, the same as an unmentioned handle in a claim's content.
        let stored = bare
            .scan_messages()
            .await
            .expect("scan_messages should succeed");
        let raw = stored
            .iter()
            .find(|m| m.id == posted.id)
            .expect("the message is still there");
        assert_eq!(raw.body, format!("about @{never}"));
    }
}
