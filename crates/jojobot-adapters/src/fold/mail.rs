//! [`FoldedMailboxes`] — every box and every readable message, held in RAM above
//! the store (decision log 292).
//!
//! jojobot is one server instance and the only writer, so it holds the board in
//! memory and answers a read from there. [`FoldedMailboxes`] wraps a
//! [`Mailboxes`] and, once [`rebuild`](FoldedMailboxes::rebuild) has run, serves
//! the reads of the port from an image filled by the store's own reads: the
//! boxes with their owners, counts and unreadable cards, and every readable
//! message in the order the store returns them.
//!
//! **A write goes to the store first, and the image takes the answer the store
//! gave.** A message posted, a delivery, a message marked processed: each comes
//! back from the store as it stored it, and the image holds that. A write that
//! can change many cards at once or the shape of a box (creating a box,
//! repointing an owner, a quarantine) is rare, and the image is filled again
//! after it.
//!
//! Before the first `rebuild`, and after a write the image could not follow, no
//! image is held and every read goes to the store underneath: slower and still
//! correct. The next write fills it again.
//!
//! ⚠️ **This is only correct with exactly one writer in the process**, for the
//! reason [`super::Folded`] gives.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NewMessage, Quarantined, StateCounts, Superseded, TakenBy,
    validate_mailbox_name,
};
use jojobot_domain::memory::EntityId;

/// One box, as `list_mailboxes` reports it, without the counts.
struct Box_ {
    name: MailboxName,
    owner: EntityId,
    /// Cards the store cannot read, deliberate quarantines included.
    quarantined: Vec<MessageId>,
    counts: StateCounts,
}

/// The board as the store holds it.
struct Image {
    /// Every box, in the store's order (by name).
    boxes: Vec<Box_>,
    /// Every readable message, in the store's order.
    messages: Vec<Message>,
    /// Where each message sits in `messages`.
    at: HashMap<MessageId, usize>,
}

impl Image {
    fn of(boxes: Vec<Mailbox>, messages: Vec<Message>) -> Self {
        let at = messages
            .iter()
            .enumerate()
            .map(|(place, m)| (m.id.clone(), place))
            .collect();
        Image {
            boxes: boxes
                .into_iter()
                .map(|b| Box_ {
                    name: b.name,
                    owner: b.owner,
                    quarantined: b.quarantined,
                    counts: b.counts,
                })
                .collect(),
            messages,
            at,
        }
    }

    fn counts_mut(&mut self, name: &MailboxName) -> Option<&mut StateCounts> {
        self.boxes
            .iter_mut()
            .find(|b| &b.name == name)
            .map(|b| &mut b.counts)
    }

    /// Hold `message` as the store answered it, in the place of the one of that
    /// id, keeping the box's counts as the store would count them.
    fn put(&mut self, message: Message) -> bool {
        let Some(place) = self.at.get(&message.id).copied() else {
            return false;
        };
        let old = std::mem::replace(&mut self.messages[place], message.clone());
        if let Some(counts) = self.counts_mut(&old.mailbox) {
            uncount(counts, &old);
        }
        match self.counts_mut(&message.mailbox) {
            Some(counts) => {
                count(counts, &message);
                true
            }
            None => false,
        }
    }

    /// Hold a message the store has just filed, last on the board.
    fn file(&mut self, message: Message) -> bool {
        // A reply's replaced message is replaced from now on.
        if let Some(target) = &message.supersedes {
            let Some(place) = self.at.get(target).copied() else {
                return false;
            };
            let mut old = self.messages[place].clone();
            old.superseded_by = Some(message.id.clone());
            if !self.put(old) {
                return false;
            }
        }
        match self.counts_mut(&message.mailbox) {
            Some(counts) => count(counts, &message),
            None => return false,
        }
        // The board lists by the instant the sender declared, the order of filing
        // breaking a tie, and this message was filed last.
        let place = self
            .messages
            .partition_point(|held| held.sent_at <= message.sent_at);
        self.messages.insert(place, message);
        self.reindex();
        true
    }

    /// **The delivery of a box that holds nothing new**, or `None` when a
    /// delivery would move a message, the box is unknown, or its name could
    /// never be a box's: the store answers those, and writes the first.
    fn leftovers_of(&self, name: &MailboxName, taken_by: TakenBy) -> Option<Delivery> {
        if validate_mailbox_name(name).is_err() || !self.boxes.iter().any(|b| &b.name == name) {
            return None;
        }
        let mut messages = Vec::new();
        let mut superseded = Vec::new();
        for message in self
            .messages
            .iter()
            .filter(|m| &m.mailbox == name && m.state.is_unprocessed())
        {
            if message.state == MessageState::New {
                // A message nobody has taken that a later one replaced is withheld
                // and named; one nobody has taken and nothing replaced is mail.
                let by = message.superseded_by.clone()?;
                superseded.push(Superseded {
                    id: message.id.clone(),
                    by,
                });
                continue;
            }
            messages.push(Delivered {
                message: Message {
                    // The first delivery is the one recorded, so a leftover keeps
                    // the way it was taken the first time.
                    taken_by: message.taken_by.or(Some(taken_by)),
                    ..message.clone()
                },
                seen_before: true,
            });
        }
        Some(Delivery {
            mailbox: name.clone(),
            messages,
            superseded,
        })
    }

    fn reindex(&mut self) {
        self.at = self
            .messages
            .iter()
            .enumerate()
            .map(|(place, m)| (m.id.clone(), place))
            .collect();
    }
}

/// Count one message into its box's tally, as the store does.
fn count(counts: &mut StateCounts, message: &Message) {
    counts.add(message.state);
    if message.state == MessageState::New && message.superseded_by.is_some() {
        counts.add_superseded();
    }
}

/// Take one message out of its box's tally, the inverse of [`count`]. A `new`
/// message that a later one replaced is tallied as `superseded` only.
fn uncount(counts: &mut StateCounts, message: &Message) {
    match message.state {
        MessageState::New if message.superseded_by.is_some() => counts.superseded -= 1,
        MessageState::New => counts.new -= 1,
        MessageState::Read => counts.read -= 1,
        MessageState::Processed => counts.processed -= 1,
    }
}

/// A [`Mailboxes`] wrapped in a RAM image of everything its reads answer.
pub struct FoldedMailboxes {
    inner: Arc<dyn Mailboxes>,
    image: RwLock<Option<Image>>,
    /// **Held across a write and the image's following it.**
    writing: tokio::sync::Mutex<()>,
    /// Set by the first successful `rebuild`.
    wanted: AtomicBool,
}

impl FoldedMailboxes {
    /// Wrap a store with **no image**. Every read answers correctly before a
    /// rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Mailboxes>) -> Self {
        FoldedMailboxes {
            inner,
            image: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
            wanted: AtomicBool::new(false),
        }
    }

    /// Fill the image from a full read of the store — the boot path. Returns
    /// how many messages were held. A board the store cannot read leaves no
    /// image: the reads go to the store.
    pub async fn rebuild(&self) -> Result<usize, MailboxError> {
        let _gate = self.writing.lock().await;
        self.fill().await
    }

    async fn fill(&self) -> Result<usize, MailboxError> {
        let read = async {
            let boxes = self.inner.list_mailboxes().await?;
            let messages = self.inner.scan_messages().await?;
            Ok::<_, MailboxError>((boxes, messages))
        }
        .await;
        match read {
            Ok((boxes, messages)) => {
                let held = messages.len();
                *self.image.write().expect("fold lock") = Some(Image::of(boxes, messages));
                self.wanted.store(true, Ordering::SeqCst);
                Ok(held)
            }
            Err(e) => {
                *self.image.write().expect("fold lock") = None;
                Err(e)
            }
        }
    }

    /// Answer from the image when one is held; `None` sends the read to the
    /// store.
    fn held<T>(&self, read: impl FnOnce(&Image) -> T) -> Option<T> {
        self.image.read().expect("fold lock").as_ref().map(read)
    }

    /// Bring the image up to date with a write that landed. A write the image
    /// cannot follow drops the image rather than guessing; the next write fills
    /// it again.
    async fn follow(&self, verb: &str, apply: impl FnOnce(&mut Image) -> bool) {
        if !self.wanted.load(Ordering::SeqCst) {
            return;
        }
        let followed = {
            let mut slot = self.image.write().expect("fold lock");
            match slot.as_mut() {
                Some(image) => apply(image),
                None => false,
            }
        };
        if followed {
            return;
        }
        self.refill(verb).await;
    }

    /// Read the board again after a write that cannot be followed by arithmetic
    /// on what it answered.
    async fn refill(&self, verb: &str) {
        if !self.wanted.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = self.fill().await {
            tracing::warn!(
                verb,
                %error,
                "the in-memory image of the board could not follow a write, so reads go to the \
                 store until the next write fills it again"
            );
        }
    }
}

#[async_trait]
impl Mailboxes for FoldedMailboxes {
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }

    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        let _gate = self.writing.lock().await;
        let made = self
            .inner
            .create_mailbox(name, owner, override_token)
            .await?;
        if matches!(made, Guarded::Written(_)) {
            self.refill("create_mailbox").await;
        }
        Ok(made)
    }

    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        let _gate = self.writing.lock().await;
        let moved = self.inner.repoint_owner(from, to).await?;
        if moved.is_some() {
            self.refill("repoint_owner").await;
        }
        Ok(moved)
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        match self.held(|image| {
            image
                .boxes
                .iter()
                .map(|b| Mailbox {
                    name: b.name.clone(),
                    owner: b.owner.clone(),
                    counts: b.counts,
                    quarantined: b.quarantined.clone(),
                })
                .collect::<Vec<_>>()
        }) {
            Some(boxes) => Ok(boxes),
            None => self.inner.list_mailboxes().await,
        }
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        match self.held(|image| image.messages.clone()) {
            Some(messages) => Ok(messages),
            None => self.inner.scan_messages().await,
        }
    }

    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        match self.held(|image| image.at.get(id).map(|place| image.messages[*place].clone())) {
            Some(message) => Ok(message),
            None => self.inner.message_by_id(id).await,
        }
    }

    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        if senders.is_empty() {
            return Ok(Vec::new());
        }
        match self.held(|image| {
            image
                .messages
                .iter()
                .filter(|m| senders.contains(&m.sender.trim()))
                .cloned()
                .collect::<Vec<_>>()
        }) {
            Some(messages) => Ok(messages),
            None => self.inner.sent_by(senders).await,
        }
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        let _gate = self.writing.lock().await;
        let posted = self.inner.post_message(message).await?;
        if let Guarded::Written(filed) = &posted {
            self.follow("post_message", |image| image.file(filed.clone()))
                .await;
        }
        Ok(posted)
    }

    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        // **A box with nothing new in it has nothing to move**, so the delivery is
        // a read: the messages already taken, and the replaced ones by name.
        if let Some(Some(delivery)) = self.held(|image| image.leftovers_of(name, taken_by)) {
            return Ok(Guarded::Written(delivery));
        }
        let _gate = self.writing.lock().await;
        let delivery = self.inner.read_mailbox(name, taken_by, acting).await?;
        if let Guarded::Written(delivery) = &delivery {
            self.follow("read_mailbox", |image| {
                // **Only the messages this delivery took.** A leftover is handed
                // over again with the way THIS delivery happened, which the store
                // does not write for it: the store still holds the way it was
                // taken the first time, or none.
                delivery
                    .messages
                    .iter()
                    .filter(|d| !d.seen_before)
                    .all(|d| image.put(d.message.clone()))
            })
            .await;
        }
        Ok(delivery)
    }

    async fn read_message(
        &self,
        id: &MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        let _gate = self.writing.lock().await;
        let delivered = self.inner.read_message(id, acting).await?;
        self.follow("read_message", |image| image.put(delivered.message.clone()))
            .await;
        Ok(delivered)
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        let _gate = self.writing.lock().await;
        let processed = self.inner.mark_processed(id, notes, acting).await?;
        self.follow("mark_processed", |image| image.put(processed.clone()))
            .await;
        Ok(processed)
    }

    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        let _gate = self.writing.lock().await;
        let quarantined = self.inner.quarantine(id, by, reason, acting).await?;
        self.refill("quarantine").await;
        Ok(quarantined)
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        self.inner.message_history(id).await
    }
}
