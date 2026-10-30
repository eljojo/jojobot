//! **The mail rail's own read, and the operator's two actions, as a page sees them.**
//!
//! A page asks the rail the verbs read from and never holds the mailbox port: a
//! GET takes no delivery, because nothing here can. The two actions are the only
//! write a page can reach, and each is a command that checks the operator and
//! the box inside itself.

use std::sync::Arc;

use jojobot_domain::mailbox::{Mailbox, MailboxError, Message, MessageId};
use jojobot_domain::memory::{EntityId, MemoryError};

/// **What the instance's record makes of its operator**: the record names
/// nobody, names something that is not a person, or names a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorIs {
    /// The record or the key is absent.
    Nobody,
    /// The record holds a handle of a kind that is not a person, so there is no
    /// operator yet.
    NotAPerson(EntityId),
    /// The person the record names.
    Person(EntityId),
}

/// **The read side of the mail rail.** Every method moves nothing and marks
/// nothing.
#[async_trait::async_trait]
pub trait MailRail: Send + Sync {
    /// Who the instance's record names as operator.
    async fn operator(&self) -> Result<OperatorIs, MemoryError>;
    /// Every box, with its counts and its quarantined messages.
    async fn boxes(&self) -> Result<Vec<Mailbox>, MailboxError>;
    /// Every message, in every state.
    async fn messages(&self) -> Result<Vec<Message>, MailboxError>;
}

/// The handle a page holds for the mail rail.
#[derive(Clone)]
pub struct MailHandle(Arc<dyn MailRail>);

impl From<Arc<dyn MailRail>> for MailHandle {
    fn from(rail: Arc<dyn MailRail>) -> Self {
        Self(rail)
    }
}

impl MailHandle {
    pub async fn operator(&self) -> Result<OperatorIs, MemoryError> {
        self.0.operator().await
    }

    pub async fn boxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.0.boxes().await
    }

    pub async fn messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.0.messages().await
    }
}

/// **Why a command changed nothing.** Each is a different repair, so a page says
/// each apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The instance names no operator.
    NoOperator,
    /// The record names something that is not a person.
    NotAPerson(EntityId),
    /// The instance's record could not be read.
    RecordUnreadable(String),
    /// The operator has no mailbox yet.
    NoBox,
    /// The operator owns more than one box, which is damage and not a choice.
    SeveralBoxes(Vec<String>),
    /// The mail board could not be read, and why.
    BoardUnreadable(String),
    /// The message is not in the operator's box.
    NotTheirs,
    /// The write itself failed.
    Failed(String),
}

/// **The operator's two actions.** Each returns the operator, whose page the
/// action lands back on.
#[async_trait::async_trait]
pub trait OperatorMail: Send + Sync {
    /// Take delivery of one message in the operator's box.
    async fn open(&self, id: &MessageId) -> Result<EntityId, Refused>;
    /// Mark one message in the operator's box processed, with an optional note.
    async fn mark_processed(&self, id: &MessageId, note: Option<&str>)
    -> Result<EntityId, Refused>;
}

/// The handle a page holds for the operator's two actions.
#[derive(Clone)]
pub struct OperatorMailCommands(Arc<dyn OperatorMail>);

impl From<Arc<dyn OperatorMail>> for OperatorMailCommands {
    fn from(commands: Arc<dyn OperatorMail>) -> Self {
        Self(commands)
    }
}

impl OperatorMailCommands {
    pub async fn open(&self, id: &MessageId) -> Result<EntityId, Refused> {
        self.0.open(id).await
    }

    pub async fn mark_processed(
        &self,
        id: &MessageId,
        note: Option<&str>,
    ) -> Result<EntityId, Refused> {
        self.0.mark_processed(id, note).await
    }
}
