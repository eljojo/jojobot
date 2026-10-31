//! **The mail rail as the operator's window reads it, and the operator's two
//! actions**, built from the ports so a page holds neither.
//!
//! The read moves nothing. The commands check, inside themselves, that the
//! operator is one person with one box and that the message is in it, which is
//! the same reading the verbs make of the record (rule 598).

use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::visibility::{MessageVisibility, VisibleBoxes, visible_boxes};
use jojobot_domain::mailbox::{Acting, Mailbox, MailboxError, Mailboxes, Message, MessageId};
use jojobot_domain::memory::{EntityId, Memory, MemoryError};
use jojobot_query_api::{
    MailHandle, MailRail, OperatorIs, OperatorMail, OperatorMailCommands, Refused,
};

use crate::orientation::instance_zone::operator_of;

struct Rail {
    memory: Arc<dyn Memory>,
    mailboxes: Arc<dyn Mailboxes>,
}

#[async_trait::async_trait]
impl MailRail for Rail {
    async fn operator(&self) -> Result<OperatorIs, MemoryError> {
        operator_of(&*self.memory).await
    }

    async fn boxes(&self, asker: &Actor) -> Result<Vec<Mailbox>, MailboxError> {
        let board = self.mailboxes.list_mailboxes().await?;
        if matches!(asker, Actor::Operator) {
            return Ok(board);
        }
        let visible = self.visible_to(asker, &board).await?;
        Ok(board
            .into_iter()
            .filter(|held| visible.message(&held.name, None) == MessageVisibility::Readable)
            .collect())
    }

    async fn messages(&self, asker: &Actor) -> Result<Vec<Message>, MailboxError> {
        if matches!(asker, Actor::Operator) {
            return self.mailboxes.scan_messages().await;
        }
        let board = self.mailboxes.list_mailboxes().await?;
        let visible = self.visible_to(asker, &board).await?;
        Ok(self
            .mailboxes
            .scan_messages()
            .await?
            .into_iter()
            .filter(|message| {
                visible.message(&message.mailbox, Some(&message.sender))
                    == MessageVisibility::Readable
            })
            .collect())
    }
}

impl Rail {
    /// **What this asker may read**, by the one rule search and read-by-id use.
    /// The operator reads the whole rail and does not come here; an actor that
    /// is no bot has no place on the chart and reads nothing.
    async fn visible_to(
        &self,
        asker: &Actor,
        board: &[Mailbox],
    ) -> Result<VisibleBoxes, MailboxError> {
        let chart = super::visibility::reports_to_chart(&*self.memory)
            .await
            .map_err(|err| MailboxError::Store(err.to_string()))?;
        Ok(visible_boxes(asker.bot(), &chart, board))
    }
}

/// The read handle for the mail rail.
pub fn rail(memory: Arc<dyn Memory>, mailboxes: Arc<dyn Mailboxes>) -> MailHandle {
    MailHandle::from(Arc::new(Rail { memory, mailboxes }) as Arc<dyn MailRail>)
}

struct Commands {
    memory: Arc<dyn Memory>,
    mailboxes: Arc<dyn Mailboxes>,
    clock: Clock,
}

impl Commands {
    /// **Everything both actions need to be true before they touch a message**:
    /// the instance names an operator who is a person and has one box, and the
    /// message is in it. Returns the operator and the box.
    async fn guard(&self, id: &MessageId) -> Result<(EntityId, Mailbox), Refused> {
        let operator = match operator_of(&*self.memory).await {
            Ok(OperatorIs::Person(operator)) => operator,
            Ok(OperatorIs::Nobody) => return Err(Refused::NoOperator),
            Ok(OperatorIs::NotAPerson(held)) => return Err(Refused::NotAPerson(held)),
            Err(err) => return Err(Refused::RecordUnreadable(err.to_string())),
        };
        let boxes = self
            .mailboxes
            .list_mailboxes()
            .await
            .map_err(|err| Refused::BoardUnreadable(err.to_string()))?;
        let own_box = match crate::owned_box(&boxes, &operator) {
            crate::OwnBox::The(name) => boxes
                .into_iter()
                .find(|held| held.name == name)
                .ok_or_else(|| {
                    Refused::BoardUnreadable("the box the board named is not on it".into())
                })?,
            crate::OwnBox::None => return Err(Refused::NoBox),
            crate::OwnBox::Several(names) => {
                return Err(Refused::SeveralBoxes(
                    names.iter().map(|name| name.as_str().to_string()).collect(),
                ));
            }
            crate::OwnBox::Unreadable => {
                return Err(Refused::BoardUnreadable(
                    "the board could not be read for the operator's box".into(),
                ));
            }
        };
        match self.mailboxes.message_by_id(id).await {
            Ok(Some(message)) if message.mailbox == own_box.name => Ok((operator, own_box)),
            Ok(_) => Err(Refused::NotTheirs),
            Err(err) => Err(Refused::BoardUnreadable(err.to_string())),
        }
    }
}

#[async_trait::async_trait]
impl OperatorMail for Commands {
    async fn open(&self, id: &MessageId) -> Result<EntityId, Refused> {
        let (operator, _) = self.guard(id).await?;
        self.mailboxes
            .read_message(id, Acting::unnamed(self.clock.now()))
            .await
            .map_err(|err| Refused::Failed(err.to_string()))?;
        Ok(operator)
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        note: Option<&str>,
    ) -> Result<EntityId, Refused> {
        let (operator, own_box) = self.guard(id).await?;
        self.mailboxes
            .mark_processed(id, &own_box.name, note, Acting::unnamed(self.clock.now()))
            .await
            .map_err(|err| Refused::Failed(err.to_string()))?;
        Ok(operator)
    }
}

/// The handle for the operator's two actions.
pub fn commands(
    memory: Arc<dyn Memory>,
    mailboxes: Arc<dyn Mailboxes>,
    clock: Clock,
) -> OperatorMailCommands {
    OperatorMailCommands::from(Arc::new(Commands {
        memory,
        mailboxes,
        clock,
    }) as Arc<dyn OperatorMail>)
}
