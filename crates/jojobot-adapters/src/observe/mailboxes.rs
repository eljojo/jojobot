//! Forward the Mailboxes port without losing its overrides.

use super::{MailboxOperation, Observed, Operation};
use async_trait::async_trait;
use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, NewMessage, Quarantined, TakenBy,
};
use jojobot_domain::memory::EntityId;

#[async_trait]
impl Mailboxes for Observed<dyn Mailboxes> {
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::CreateMailbox),
            self.inner.create_mailbox(name, owner, override_token),
        )
        .await
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::RepointOwner),
            self.inner.repoint_owner(from, to),
        )
        .await
    }
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::ListMailboxes),
            self.inner.list_mailboxes(),
        )
        .await
    }
    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::ScanMessages),
            self.inner.scan_messages(),
        )
        .await
    }
    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::MessageById),
            self.inner.message_by_id(id),
        )
        .await
    }
    async fn messages_by_id(&self, ids: &[MessageId]) -> Result<Vec<Message>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::MessagesById),
            self.inner.messages_by_id(ids),
        )
        .await
    }
    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::SentBy),
            self.inner.sent_by(senders),
        )
        .await
    }
    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::PostMessage),
            self.inner.post_message(message),
        )
        .await
    }
    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::ReadMailbox),
            self.inner.read_mailbox(name, taken_by, acting),
        )
        .await
    }
    async fn read_message(
        &self,
        id: &MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::ReadMessage),
            self.inner.read_message(id, acting),
        )
        .await
    }
    async fn mark_processed(
        &self,
        id: &MessageId,
        by: &MailboxName,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::MarkProcessed),
            self.inner.mark_processed(id, by, notes, acting),
        )
        .await
    }
    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        self.result(
            Operation::Mailboxes(MailboxOperation::Quarantine),
            self.inner.quarantine(id, by, reason, acting),
        )
        .await
    }
    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        self.result(
            Operation::Mailboxes(MailboxOperation::MessageHistory),
            self.inner.message_history(id),
        )
        .await
    }
    async fn scan_message_events(
        &self,
    ) -> Result<
        Vec<(
            jojobot_domain::mailbox::MessageId,
            Vec<jojobot_domain::mailbox::MessageEvent>,
        )>,
        jojobot_domain::mailbox::MailboxError,
    > {
        self.result(
            Operation::Mailboxes(MailboxOperation::ScanMessageEvents),
            self.inner.scan_message_events(),
        )
        .await
    }
}
