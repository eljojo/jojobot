//! The Mailboxes contract, and the in-memory fake that must satisfy it.
//!
//! One behavioural spec, two tiers: the fake here (milliseconds), and the
//! real adapter against the real store it runs — a disposable database this
//! process spawns itself, so it needs no network and no credentials and runs
//! in an ordinary `cargo test`. **The spec is the same code on both**, which
//! is what stops the fake from drifting into a store that agrees with the
//! tests and disagrees with reality.
//!
//! Behind the `testing` feature, so it compiles for tests here and in downstream
//! crates but never ships in a production binary.

use std::sync::Mutex;

use jiff::Timestamp;

use crate::memory::{EntityId, guard as memory_guard};

use super::event::{Change, next_event};
use super::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageEvent, MessageEventKind, MessageId, MessageState, NOTES_BUDGET, NewMessage, Quarantined,
    StateCounts, SupersedeRefusal, Superseded, TakenBy, guard, normalize_body, normalize_notes,
    normalize_subject, validate_body, validate_mailbox_name, validate_message_id, validate_notes,
    validate_sender, validate_subject,
};

/// The in-memory [`Mailboxes`] fake — a real store that holds a write, with no
/// network. Deterministic: ids are a monotonic counter, never a clock.
#[derive(Default)]
pub struct InMemoryMailboxes {
    /// Who owns each box. A box cannot exist without an owner, so this is
    /// keyed by name and never absent for a box that is here.
    owners: Mutex<Vec<(MailboxName, EntityId)>>,
    /// The owners this store can resolve — the fake's stand-in for the entity
    /// index the real adapter reads. Seeded by [`InMemoryMailboxes::know_owner`].
    known_owners: Mutex<Vec<EntityId>>,
    /// Whether any well-formed owner resolves — see
    /// [`InMemoryMailboxes::knowing_any_owner`].
    permissive: Mutex<bool>,
    boxes: Mutex<Vec<MailboxName>>,
    messages: Mutex<Vec<Message>>,
    next_id: Mutex<u64>,
    /// **The ledger of spent request ids this store spends into**, in the same
    /// step as the write, as the real store's table is written in the write's
    /// transaction. Set once by [`InMemoryMailboxes::attach_requests`].
    requests: Mutex<Option<std::sync::Arc<crate::request::testing::InMemoryRequests>>>,
    quarantined: Mutex<Vec<(MailboxName, MessageId, String)>>,
    /// **Which of `quarantined`'s ids were quarantined ON PURPOSE**, and by
    /// whom — a subset, looked up by id, that turns the shared hiding list
    /// above into two different refusals rather than one. An id here is
    /// always also in `quarantined`; the reverse is not true.
    deliberate: Mutex<Vec<(MessageId, String, String, Timestamp)>>,
    /// **Every change of state a message went through**, in the order they
    /// happened, kept beside the message exactly as the real store keeps them
    /// beside the row.
    events: Mutex<Vec<(MessageId, MessageEvent)>>,
    /// Whether a board read fails. Writes still land: this is the store being
    /// unreachable for a read, not gone.
    blind: Mutex<bool>,
    /// **The outbox a post to a person's box writes into**, when this store was
    /// made to push. `None` is a store that pushes nothing.
    outbox: Option<std::sync::Arc<crate::push::testing::InMemoryOutbox>>,
}

/// The message that replaced `id`, when one did: the newest message whose
/// `supersedes` names it.
fn replacing(messages: &[Message], id: &MessageId) -> Option<MessageId> {
    messages
        .iter()
        .rev()
        .find(|m| m.supersedes.as_ref() == Some(id))
        .map(|m| m.id.clone())
}

/// `message` as it reads: with the id of the message that replaced it filled in.
fn read_as_stored(messages: &[Message], message: &Message) -> Message {
    Message {
        superseded_by: replacing(messages, &message.id),
        ..message.clone()
    }
}

impl InMemoryMailboxes {
    /// **Spend request ids into `ledger` from now on.** See the field.
    pub fn attach_requests(
        &self,
        ledger: std::sync::Arc<crate::request::testing::InMemoryRequests>,
    ) {
        *self.requests.lock().expect("requests lock") = Some(ledger);
    }

    /// **Spend the id of the call in flight**, when it carries one and this write
    /// is the verb's own. A write that finds the id already spent stores nothing.
    fn spend_request(&self, landed: impl FnOnce() -> String) -> Result<(), MailboxError> {
        let Some(claim) = crate::request::take() else {
            return Ok(());
        };
        match self.requests.lock().expect("requests lock").as_ref() {
            Some(ledger) => {
                if ledger.spend(claim, landed()) {
                    Ok(())
                } else {
                    Err(MailboxError::Conflict)
                }
            }
            None => {
                claim.failed();
                Ok(())
            }
        }
    }

    /// An empty store that already resolves the owners [`contract::OWNERS`]
    /// names.
    ///
    /// **Seeded rather than bare, because a box cannot exist without an owner.**
    /// Almost every test that creates a box wants one that resolves, and the
    /// real adapter gets that for free by reading the same store its entities
    /// live in — a fake with no owners at all would make every one of those
    /// tests carry setup for a question they are not about.
    ///
    /// The refusal has its own case, and it names an owner deliberately outside
    /// this set. [`know_owner`](Self::know_owner) adds others.
    pub fn new() -> Self {
        let store = Self::default();
        for owner in contract::OWNERS {
            store.know_owner(&EntityId((*owner).to_string()));
        }
        store
    }

    /// **This store writes a push into its outbox for every message that lands
    /// in a person's box**, as the real store does when the instance is set up to
    /// push. The outbox is handed back by [`InMemoryMailboxes::outbox`].
    #[must_use]
    pub fn pushing(mut self) -> Self {
        self.outbox = Some(std::sync::Arc::default());
        self
    }

    /// The outbox this store writes into. Panics when the store was not made to
    /// push, because a test that reads an outbox the store never wrote is wrong.
    pub fn outbox(&self) -> std::sync::Arc<crate::push::testing::InMemoryOutbox> {
        self.outbox
            .clone()
            .expect("this store was not made to push")
    }

    /// **Resolve any well-formed owner** — for tests whose subject is not
    /// ownership.
    ///
    /// The strict fake exists so the contract can prove the refusal, and that
    /// is the only place that needs it. A suite testing the mail *surface*
    /// stands bots up through Memory and would otherwise have to teach this
    /// store about each one — setup for a question those tests are not asking,
    /// and the real adapter never needs it because both contexts read one store.
    pub fn knowing_any_owner() -> Self {
        let store = Self::default();
        *store.permissive.lock().expect("owner lock") = true;
        store
    }

    /// **Make an owner resolvable.** A fixture, not a verb: no port method does
    /// this, and none should — the real adapter answers "does this owner exist"
    /// from the entity index, which this fake does not have.
    ///
    /// It is how the fake meets [`contract::OWNERS`]' precondition. See that
    /// constant for why the contract states a precondition rather than growing
    /// a trait to provision through.
    pub fn know_owner(&self, owner: &EntityId) {
        let mut known = self.known_owners.lock().expect("owner lock");
        if !known.contains(owner) {
            known.push(owner.clone());
        }
    }

    /// Put a card into quarantine BY DAMAGE, as a hand edit on a real board
    /// would — a parse failure that names no decision, distinct from the
    /// [`Mailboxes::quarantine`] trait verb, which is a box acting on its own
    /// mail on purpose and names who and why.
    ///
    /// **Seeding only — nothing in this fake can quarantine itself**, because
    /// everything that reaches it passed validation on the way in. That is
    /// exactly why it needs the vocabulary: without it the quarantine half of
    /// every surface above (the counts a caller reads, the answer
    /// `mark_processed` gives for one of these ids) has no store that can
    /// produce the condition, and so no test that can exercise it.
    ///
    /// A quarantined id is invisible to **every** verb here, exactly as it is
    /// in the real store — where a card the board read cannot parse is left out
    /// of the message list that counts and delivery are both built from. A fake
    /// that only taught two of its verbs the word would answer differently from
    /// the store on the other two, in a place the shared contract is silent.
    pub fn quarantine_by_damage(&self, mailbox: &MailboxName, card: &MessageId, reason: &str) {
        self.quarantined.lock().expect("quarantine lock").push((
            mailbox.clone(),
            card.clone(),
            reason.to_string(),
        ));
    }

    /// A board read fails from here until [`sighted`](Self::sighted) — the
    /// store is up and the reading of it is not.
    pub fn blind(&self) {
        *self.blind.lock().expect("blind lock") = true;
    }

    /// Board reads land again.
    pub fn sighted(&self) {
        *self.blind.lock().expect("blind lock") = false;
    }

    /// **A message is removed from the store by hand**, the way rule 60 says a
    /// record leaves at all: outside jojobot, with no verb of its own and
    /// nothing telling the index it happened.
    ///
    /// Distinct from [`quarantine_by_damage`](Self::quarantine_by_damage), which leaves the card on
    /// the board and unreadable. Both are invisible to a board read; only this
    /// one means the record is gone.
    pub fn lose(&self, card: &MessageId) {
        self.messages
            .lock()
            .expect("message lock")
            .retain(|m| &m.id != card);
    }

    /// The names currently on the board, in creation order.
    fn names(&self) -> Vec<MailboxName> {
        self.boxes.lock().expect("mailbox lock").clone()
    }

    /// A minted id as the number it is, for tie-breaking. Ids here are a
    /// counter rendered decimal, so comparing them as text would put `10`
    /// before `2`.
    fn numeric(id: &MessageId) -> u64 {
        id.as_str().parse().unwrap_or(u64::MAX)
    }

    fn mint_id(&self) -> MessageId {
        let mut next = self.next_id.lock().expect("id lock");
        *next += 1;
        MessageId(next.to_string())
    }

    /// **Keep one change of state**, made by [`next_event`] and numbered among the
    /// message's own, so a history reads in the order the changes happened.
    fn keep_event(&self, id: &MessageId, change: Change<'_>) {
        let mut events = self.events.lock().expect("event lock");
        let event = next_event(
            events
                .iter()
                .filter(|(held, _)| held == id)
                .map(|(_, event)| event.ordinal),
            change,
        );
        events.push((id.clone(), event));
    }

    /// The refusal every verb that addresses a card by id owes: a quarantined
    /// id is on the board and cannot be read, which is a different answer from
    /// "no such message" and has to stay one.
    fn refuse_if_quarantined(&self, id: &MessageId) -> Result<(), MailboxError> {
        // **Checked first, because it is the more specific answer.** An id
        // quarantined on purpose is always also in `quarantined` — the
        // shared hiding list every other filter reads — but it owes the
        // caller the decision, not the damage wording.
        if let Some((_, by, reason, _)) = self
            .deliberate
            .lock()
            .expect("deliberate lock")
            .iter()
            .find(|(card, ..)| card == id)
        {
            return Err(MailboxError::QuarantinedOnPurpose {
                attempted: id.to_string(),
                by: by.clone(),
                reason: reason.clone(),
            });
        }
        let reason = self
            .quarantined
            .lock()
            .expect("quarantine lock")
            .iter()
            .find(|(_, card, _)| card == id)
            .map(|(_, _, reason)| reason.clone());
        match reason {
            Some(reason) => Err(MailboxError::Quarantined {
                attempted: id.to_string(),
                reason,
            }),
            None => Ok(()),
        }
    }
}

#[async_trait::async_trait]
impl Mailboxes for InMemoryMailboxes {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        validate_mailbox_name(name)?;
        crate::memory::validate_subject(owner)
            .map_err(|e| MailboxError::InvalidName(e.to_string()))?;

        // The owner must exist. Screened before the name, because "there is no
        // such owner" is the more fundamental mistake and the caller should hear
        // it first — a near-miss on the name is advice about a box they may not
        // be entitled to create at all.
        {
            let known = self.known_owners.lock().expect("owner lock");
            let permissive = *self.permissive.lock().expect("owner lock");
            if !permissive && !known.contains(owner) {
                // **Screened against the owners this store holds, not against
                // nothing.** The real adapter has an entity index and reports
                // what a typo probably meant; a fake that always came back with
                // an empty list would agree with every test and disagree with
                // the store, which is how a polite fake ships a broken adapter.
                let index: Vec<crate::memory::Entity> =
                    known.iter().map(stand_in).collect::<Result<_, _>>()?;
                return Ok(Guarded::UnknownOwner {
                    attempted: owner.clone(),
                    candidates: memory_guard::screen(owner, &[], &index),
                });
            }
        }

        let mut boxes = self.boxes.lock().expect("mailbox lock");
        if let guard::Decision::Block(candidates) =
            guard::decide_create_for(name, Some(owner.slug()), &boxes, override_token)
        {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }
        boxes.push(name.clone());
        self.owners
            .lock()
            .expect("owner lock")
            .push((name.clone(), owner.clone()));
        Ok(Guarded::Written(Mailbox {
            name: name.clone(),
            owner: owner.clone(),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        }))
    }

    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        // **Every row this owner holds, not the first one the loop meets.**
        // Nothing here makes an owner unique — the create guard screens the
        // box name, never the owner — so more than one row for the same
        // owner is representable, and picking one silently would repoint it
        // and orphan the rest.
        let held: Vec<MailboxName> = {
            let owners = self.owners.lock().expect("owner lock");
            owners
                .iter()
                .filter(|(_, owner)| owner == from)
                .map(|(name, _)| name.clone())
                .collect()
        };
        let old_name = match held.as_slice() {
            [] => return Ok(None),
            [one] => one.clone(),
            many => {
                return Err(MailboxError::OwnerHasMultipleBoxes {
                    owner: from.to_string(),
                    names: many.iter().map(|n| n.as_str().to_string()).collect(),
                });
            }
        };
        let new_name = MailboxName::named_for(to);
        // **Screened before anything moves**, on the box's own key rather
        // than the entity handle the rename already screened: a different
        // owner already wearing the destination name is a real collision,
        // caught here rather than reached as damage.
        if old_name != new_name {
            let held_by = {
                let owners = self.owners.lock().expect("owner lock");
                owners
                    .iter()
                    .find(|(name, owner)| name == &new_name && owner != from)
                    .map(|(_, owner)| owner.to_string())
            };
            if let Some(held_by) = held_by {
                return Err(MailboxError::NameTaken {
                    attempted: new_name.as_str().to_string(),
                    held_by,
                });
            }
        }
        {
            let mut owners = self.owners.lock().expect("owner lock");
            for entry in owners.iter_mut() {
                if entry.0 == old_name {
                    entry.0 = new_name.clone();
                    entry.1 = to.clone();
                }
            }
        }
        if old_name != new_name {
            let mut boxes = self.boxes.lock().expect("mailbox lock");
            for b in boxes.iter_mut() {
                if *b == old_name {
                    *b = new_name.clone();
                }
            }
            drop(boxes);
            let mut messages = self.messages.lock().expect("message lock");
            for m in messages.iter_mut() {
                if m.mailbox == old_name {
                    m.mailbox = new_name.clone();
                }
            }
            drop(messages);
            let mut quarantined = self.quarantined.lock().expect("quarantine lock");
            for q in quarantined.iter_mut() {
                if q.0 == old_name {
                    q.0 = new_name.clone();
                }
            }
        }
        let mailboxes = self.list_mailboxes().await?;
        Ok(mailboxes.into_iter().find(|m| m.name == new_name))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let messages = self.messages.lock().expect("message lock");
        let quarantined = self.quarantined.lock().expect("quarantine lock");
        let owners = self.owners.lock().expect("owner lock");
        Ok(self
            .names()
            .into_iter()
            .map(|name| {
                let mut counts = StateCounts::default();
                for message in messages.iter().filter(|m| {
                    // The guard above is still held — re-locking it here (via a
                    // helper that takes it again) is a deadlock, not a
                    // convenience: this mutex is not reentrant.
                    m.mailbox == name && !quarantined.iter().any(|(_, card, _)| card == &m.id)
                }) {
                    counts.add(message.state);
                    if message.state == MessageState::New
                        && replacing(&messages, &message.id).is_some()
                    {
                        counts.add_superseded();
                    }
                }
                Mailbox {
                    quarantined: quarantined
                        .iter()
                        .filter(|(mailbox, _, _)| mailbox == &name)
                        .map(|(_, card, _)| card.clone())
                        .collect(),
                    owner: owners
                        .iter()
                        .find(|(box_name, _)| box_name == &name)
                        .map(|(_, owner)| owner.clone())
                        .expect("a box on this store was created with an owner"),
                    name,
                    counts,
                }
            })
            .collect())
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        validate_mailbox_name(&message.mailbox)?;
        validate_sender(&message.sender)?;
        validate_body(&message.body)?;
        validate_subject(message.subject.as_deref())?;

        let names = self.names();
        if let guard::Decision::Block(candidates) = guard::decide_existing(&message.mailbox, &names)
        {
            return Ok(Guarded::Blocked {
                attempted: message.mailbox,
                candidates,
            });
        }

        // Everything a write names must already exist — a reply link included.
        if let Some(answered) = &message.in_reply_to {
            validate_message_id(answered)?;
            let known = self
                .messages
                .lock()
                .expect("message lock")
                .iter()
                .any(|m| &m.id == answered);
            if !known {
                return Err(MailboxError::UnknownMessage {
                    attempted: answered.to_string(),
                });
            }
        }

        // **A replacement names a message of the sender's own, in this box, that
        // nothing has replaced yet.**
        if let Some(target) = &message.supersedes {
            validate_message_id(target)?;
            self.refuse_if_quarantined(target)?;
            let messages = self.messages.lock().expect("message lock");
            let old = messages.iter().find(|m| &m.id == target).ok_or_else(|| {
                MailboxError::UnknownMessage {
                    attempted: target.to_string(),
                }
            })?;
            let attempted = target.to_string();
            if old.sender != message.sender.trim() {
                return Err(MailboxError::CannotSupersede(
                    SupersedeRefusal::OtherSender { attempted },
                ));
            }
            if old.mailbox != message.mailbox {
                return Err(MailboxError::CannotSupersede(SupersedeRefusal::OtherBox {
                    attempted,
                    sits_in: old.mailbox.as_str().to_string(),
                    posting_into: message.mailbox.as_str().to_string(),
                }));
            }
            if let Some(by) = replacing(&messages, target) {
                return Err(MailboxError::CannotSupersede(
                    SupersedeRefusal::AlreadySuperseded {
                        attempted,
                        by: by.to_string(),
                    },
                ));
            }
        }

        let stored = Message {
            id: self.mint_id(),
            mailbox: message.mailbox,
            body: normalize_body(&message.body),
            subject: normalize_subject(message.subject.as_deref()),
            sender: message.sender.trim().to_string(),
            sent_at: message.sent_at,
            state: MessageState::New,
            notes: None,
            in_reply_to: message.in_reply_to,
            taken_by: None,
            sender_mail_waiting_at_send: message.sender_mail_waiting_at_send,
            posted_by_session: message.posted_by_session,
            superseded_by: None,
            supersedes: message.supersedes,
        };
        self.spend_request(|| format!("{{\"id\":\"{}\"}}", stored.id))?;
        self.messages
            .lock()
            .expect("message lock")
            .push(stored.clone());
        // **The intent is written with the message**, for a box a person owns.
        if let Some(outbox) = &self.outbox {
            let to_a_person =
                self.owners
                    .lock()
                    .expect("owner lock")
                    .iter()
                    .any(|(name, owner)| {
                        name == &stored.mailbox
                            && owner.kind() == Some(crate::memory::EntityKind::PERSON)
                    });
            if to_a_person {
                outbox.enqueue(&stored.id, Timestamp::now());
            }
        }
        Ok(Guarded::Written(stored))
    }

    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        validate_mailbox_name(name)?;
        let names = self.names();
        if let guard::Decision::Block(candidates) = guard::decide_existing(name, &names) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }

        let quarantined: Vec<MessageId> = self
            .quarantined
            .lock()
            .expect("quarantine lock")
            .iter()
            .map(|(_, card, _)| card.clone())
            .collect();
        let mut messages = self.messages.lock().expect("message lock");
        // **A message nobody has taken, that a later one replaced, is withheld**
        // and named apart. Taken before the replacement, it is owed like any
        // other.
        let mut withheld: Vec<Superseded> = messages
            .iter()
            .filter(|m| &m.mailbox == name && m.state == MessageState::New)
            .filter(|m| !quarantined.contains(&m.id))
            .filter_map(|m| {
                replacing(&messages, &m.id).map(|by| Superseded {
                    id: m.id.clone(),
                    by,
                })
            })
            .collect();
        withheld.sort_by_key(|w| Self::numeric(&w.id));
        let snapshot = messages.clone();
        let mut moved_to_read: Vec<MessageId> = Vec::new();
        let mut delivered: Vec<Delivered> = messages
            .iter_mut()
            .filter(|m| {
                &m.mailbox == name
                    && m.state.is_unprocessed()
                    && !quarantined.contains(&m.id)
                    && !withheld.iter().any(|w| w.id == m.id)
            })
            .map(|m| {
                let seen_before = m.state == MessageState::Read;
                // **The FIRST delivery is the one recorded.** A leftover was
                // taken once already; saying it was taken again by whatever
                // moved it this time would rewrite the sender's signal every
                // time the recipient posted anything.
                if m.state == MessageState::New {
                    m.taken_by = Some(taken_by);
                    moved_to_read.push(m.id.clone());
                }
                m.state = MessageState::Read;
                Delivered {
                    message: read_as_stored(&snapshot, m),
                    seen_before,
                }
            })
            .collect();
        for id in &moved_to_read {
            self.keep_event(
                id,
                Change::new(MessageEventKind::Read, acting, name, MessageState::New),
            );
        }
        // Oldest **by the instant the sender declared**, not by the order this
        // store happened to be handed them — the same total order the real
        // adapter reads off the board, with the minted id breaking a tie.
        delivered.sort_by(|a, b| {
            a.message
                .sent_at
                .cmp(&b.message.sent_at)
                .then_with(|| Self::numeric(&a.message.id).cmp(&Self::numeric(&b.message.id)))
        });
        Ok(Guarded::Written(Delivery {
            mailbox: name.clone(),
            messages: delivered,
            superseded: withheld,
        }))
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        if *self.blind.lock().expect("blind lock") {
            return Err(MailboxError::Store("the board cannot be read".into()));
        }
        let quarantined = self.quarantined.lock().expect("quarantine lock");
        let messages = self.messages.lock().expect("message lock");
        let mut board: Vec<Message> = messages
            .iter()
            .filter(|m| !quarantined.iter().any(|(_, card, _)| card == &m.id))
            .map(|m| read_as_stored(&messages, m))
            .collect();
        // The real board lists by the instant the sender declared, the order the
        // messages were filed in breaking a tie; the sort is stable.
        board.sort_by_key(|m| m.sent_at);
        Ok(board)
    }

    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        Ok(self
            .scan_messages()
            .await?
            .into_iter()
            .find(|m| &m.id == id))
    }

    async fn messages_by_id(&self, ids: &[MessageId]) -> Result<Vec<Message>, MailboxError> {
        Ok(self
            .scan_messages()
            .await?
            .into_iter()
            .filter(|m| ids.contains(&m.id))
            .collect())
    }

    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        Ok(self
            .scan_messages()
            .await?
            .into_iter()
            .filter(|m| senders.contains(&m.sender.trim()))
            .collect())
    }

    async fn read_message(
        &self,
        id: &MessageId,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        validate_message_id(id)?;
        self.refuse_if_quarantined(id)?;

        let mut messages = self.messages.lock().expect("message lock");
        let message = messages.iter_mut().find(|m| &m.id == id).ok_or_else(|| {
            MailboxError::UnknownMessage {
                attempted: id.to_string(),
            }
        })?;
        // Anything but `new` has been handed over or handled already — the one
        // state this verb advances is the one nobody has taken.
        let seen_before = message.state != MessageState::New;
        if !seen_before {
            message.state = MessageState::Read;
            // Taking one message by id is somebody looking, so the delivery is
            // recorded the same way opening a box is. A pickup that recorded
            // nothing would read back as though nobody took it.
            message.taken_by = Some(TakenBy::Reading);
            self.keep_event(
                id,
                Change::new(
                    MessageEventKind::Read,
                    acting,
                    &message.mailbox,
                    MessageState::New,
                ),
            );
        }
        let message = message.clone();
        Ok(Delivered {
            message: read_as_stored(&messages, &message),
            seen_before,
        })
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        validate_message_id(id)?;
        validate_notes(notes)?;
        self.refuse_if_quarantined(id)?;

        let mut messages = self.messages.lock().expect("message lock");
        let message = messages.iter_mut().find(|m| &m.id == id).ok_or_else(|| {
            MailboxError::UnknownMessage {
                attempted: id.to_string(),
            }
        })?;
        let prior = message.state;
        self.spend_request(|| format!("{{\"id\":\"{id}\"}}"))?;
        message.state = MessageState::Processed;
        let given = normalize_notes(notes);
        if let Some(notes) = &given {
            message.notes = Some(notes.clone());
        }
        self.keep_event(
            id,
            Change::new(MessageEventKind::Processed, acting, &message.mailbox, prior)
                .with_notes(notes),
        );
        let message = message.clone();
        Ok(read_as_stored(&messages, &message))
    }

    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        validate_message_id(id)?;
        self.refuse_if_quarantined(id)?;
        let messages = self.messages.lock().expect("message lock");
        let message =
            messages
                .iter()
                .find(|m| &m.id == id)
                .ok_or_else(|| MailboxError::UnknownMessage {
                    attempted: id.to_string(),
                })?;
        if &message.mailbox != by {
            return Err(MailboxError::NotYourMessage {
                attempted: id.to_string(),
                mailbox: message.mailbox.as_str().to_string(),
                by: by.as_str().to_string(),
            });
        }
        let mailbox = message.mailbox.clone();
        let prior = message.state;
        drop(messages);
        let reason = reason.trim().to_string();
        let at = acting.at;
        self.spend_request(|| format!("{{\"id\":\"{id}\"}}"))?;
        self.keep_event(
            id,
            Change::new(MessageEventKind::Quarantined, acting, by, prior).with_reason(&reason),
        );
        // **A message that was set aside replaces nothing**, as in the real store,
        // where the replaced message is not read as replaced by a row in the
        // quarantine state. Nothing reads the set-aside message again, so its link
        // is dropped here rather than judged at every read.
        if let Some(set_aside) = self
            .messages
            .lock()
            .expect("message lock")
            .iter_mut()
            .find(|m| &m.id == id)
        {
            set_aside.supersedes = None;
        }
        self.quarantined.lock().expect("quarantine lock").push((
            mailbox.clone(),
            id.clone(),
            reason.clone(),
        ));
        self.deliberate.lock().expect("deliberate lock").push((
            id.clone(),
            by.as_str().to_string(),
            reason.clone(),
            at,
        ));
        Ok(Quarantined {
            id: id.clone(),
            mailbox,
            by: by.as_str().to_string(),
            reason,
            at,
        })
    }

    async fn message_history(&self, id: &MessageId) -> Result<Vec<MessageEvent>, MailboxError> {
        validate_message_id(id)?;
        if !self
            .messages
            .lock()
            .expect("message lock")
            .iter()
            .any(|m| &m.id == id)
        {
            return Err(MailboxError::UnknownMessage {
                attempted: id.to_string(),
            });
        }
        let mut kept: Vec<MessageEvent> = self
            .events
            .lock()
            .expect("event lock")
            .iter()
            .filter(|(held, _)| held == id)
            .map(|(_, event)| event.clone())
            .collect();
        kept.sort_by_key(|event| event.ordinal);
        Ok(kept)
    }

    async fn scan_message_events(
        &self,
    ) -> Result<Vec<(MessageId, Vec<MessageEvent>)>, MailboxError> {
        let held = self.events.lock().expect("event lock");
        let mut ids: Vec<&MessageId> = held.iter().map(|(id, _)| id).collect();
        ids.sort();
        ids.dedup();
        Ok(ids
            .into_iter()
            .map(|id| {
                let mut events: Vec<MessageEvent> = held
                    .iter()
                    .filter(|(held, _)| held == id)
                    .map(|(_, event)| event.clone())
                    .collect();
                events.sort_by_key(|event| event.ordinal);
                (id.clone(), events)
            })
            .collect())
    }
}

/// An owner handle as the entity record the near-miss screen needs.
///
/// The fake holds handles rather than entities, and the screen compares handles
/// and labels — so the name is the slug, which is what a screen over the real
/// index would be comparing against anyway for a bot named for its handle.
///
/// **A handle this process cannot read is a failure here, because it is one in
/// the store.** The real owner index reads entity rows, and a row whose kind
/// cannot be read fails that read and the lookup with it. Naming a kind for it
/// instead would let the fake screen a record the store refuses to serve, which
/// is how a polite double passes a test production fails.
fn stand_in(owner: &EntityId) -> Result<crate::memory::Entity, MailboxError> {
    crate::memory::validate_subject(owner).map_err(|e| MailboxError::Store(e.to_string()))?;
    Ok(crate::memory::Entity {
        folded_names: Vec::new(),
        kind: owner.kind().expect("a validated handle names a kind"),
        name: owner.slug().to_string(),
        id: owner.clone(),
        aliases: Vec::new(),
        source: "fake".into(),
        crm: None,
        parent: None,
        boot: Default::default(),
        // A stand-in for a lookup, not a row: it wears no badge because there
        // is no row behind it to keep a name for.
        badge: None,
        merged_into: None,
        archived: None,
    })
}

/// The shared behavioural spec — every adapter must satisfy all of it.
///
/// Names here come from a fixed, openly fictional roster; nothing in this file
/// names anything from the operator's life.
pub mod contract {
    use super::*;

    /// A fixed instant, so the spec never reads a clock. Every timestamp below
    /// is an offset from it, which is what makes "oldest first" assertable.
    pub fn epoch() -> Timestamp {
        Timestamp::from_second(1_780_000_000).expect("a valid fixed instant")
    }

    fn at(offset: i64) -> Timestamp {
        epoch() + jiff::SignedDuration::from_secs(offset)
    }

    fn name(n: &str) -> MailboxName {
        MailboxName(n.to_string())
    }

    /// **The owners this spec creates boxes for, and the store's precondition.**
    ///
    /// A mailbox cannot exist without an owner, so every create in this suite
    /// names one — which means the store handed to [`run_all`] must already
    /// resolve these before the suite starts. Each tier meets that its own way:
    /// the fake through [`InMemoryMailboxes::know_owner`], the real adapter by
    /// having the entity in the store.
    ///
    /// **A precondition rather than a provisioning trait**, because provisioning
    /// an entity is Memory's verb and this is the Mailboxes spec. A trait to
    /// reach across would make every implementor of a mail store answer a
    /// question about entities; a stated precondition leaves each tier to
    /// satisfy it with the tools it already has.
    pub const OWNERS: &[&str] = &["bot:gamma", "bot:delta", "person:milhouse"];

    /// An owner from [`OWNERS`] — the one this suite files boxes under unless a
    /// case is about ownership itself.
    fn owner() -> EntityId {
        EntityId(OWNERS[0].to_string())
    }

    /// Create a box, asserting the guard waved it through.
    pub async fn create(store: &dyn Mailboxes, n: &str) -> Mailbox {
        store
            .create_mailbox(&name(n), &owner(), None)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block creating '{n}'"))
    }

    /// Post a message with no subject — the ordinary case, and every message
    /// written before there was a field for one.
    pub async fn post(
        store: &dyn Mailboxes,
        mailbox: &str,
        sender: &str,
        body: &str,
        at_offset: i64,
    ) -> Message {
        titled(store, mailbox, sender, None, body, at_offset).await
    }

    /// Post a message, subject and all, asserting the guard waved it through.
    pub async fn titled(
        store: &dyn Mailboxes,
        mailbox: &str,
        sender: &str,
        subject: Option<&str>,
        body: &str,
        at_offset: i64,
    ) -> Message {
        store
            .post_message(NewMessage {
                mailbox: name(mailbox),
                body: body.to_string(),
                subject: subject.map(str::to_string),
                sender: sender.to_string(),
                sent_at: at(at_offset),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block posting to '{mailbox}'"))
    }

    /// Read a box, asserting the guard waved it through.
    pub async fn read(store: &dyn Mailboxes, mailbox: &str) -> Delivery {
        store
            .read_mailbox(
                &name(mailbox),
                TakenBy::Reading,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("read_mailbox should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block reading '{mailbox}'"))
    }

    /// The counts for one box, or `None` if the board doesn't have it.
    pub async fn counts(store: &dyn Mailboxes, mailbox: &str) -> Option<StateCounts> {
        store
            .list_mailboxes()
            .await
            .expect("list_mailboxes ok")
            .into_iter()
            .find(|m| m.name.as_str() == mailbox)
            .map(|m| m.counts)
    }

    /// A created box is on the board, empty, and stays there.
    pub async fn create_then_list(store: &dyn Mailboxes) {
        let created = create(store, "inbox").await;
        assert_eq!(created.name.as_str(), "inbox");
        assert_eq!(created.counts, StateCounts::default(), "a new box is empty");

        create(store, "errands").await;
        let listed = store.list_mailboxes().await.expect("list ok");
        let mut names: Vec<&str> = listed.iter().map(|m| m.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["errands", "inbox"]);
    }

    /// 🚨 **A mailbox follows its owner to a new handle — the shape a
    /// rename produces: the box's own name moves with a changed slug, and
    /// every message already filed is still there under the new one.**
    ///
    /// **What this closes**: a box is named for its owner's slug and owned
    /// by its owner's handle, neither of which shares a key with the entity
    /// world. Left alone, a renamed bot's next boot finds no box under its
    /// new handle and heals a second, empty one beside the first — the mail
    /// already there becomes unreachable through any normal path. Proven by
    /// what survives, not by a name matching.
    ///
    /// **Paired with the defensive no-op**: an owner with no box has nothing
    /// to repoint, and that is `None` rather than an error — a rename does
    /// not know in advance whether its subject ever had one.
    pub async fn repoint_owner_moves_the_box_and_its_mail(store: &dyn Mailboxes) {
        let was = EntityId(OWNERS[0].to_string());
        let now = EntityId("bot:contract-repointed".to_string());
        let box_name = was.slug().to_string();
        create(store, &box_name).await;
        let posted = post(store, &box_name, "someone", "mail before the move", 0).await;

        let repointed = store
            .repoint_owner(&was, &now)
            .await
            .expect("repoint_owner should succeed")
            .expect("the box existed and was found");
        assert_eq!(repointed.owner, now, "{repointed:?}");
        assert_eq!(repointed.name.as_str(), now.slug(), "{repointed:?}");

        // The old box is gone outright, not left standing beside a new one.
        let boxes = store.list_mailboxes().await.expect("list ok");
        assert_eq!(
            boxes.len(),
            1,
            "the old box must not survive beside the repointed one: {boxes:?}",
        );
        assert_eq!(boxes[0].name.as_str(), now.slug());
        assert_eq!(boxes[0].owner, now);

        let delivered = read(store, now.slug()).await;
        let ids: Vec<&MessageId> = delivered.messages.iter().map(|d| &d.message.id).collect();
        assert_eq!(
            ids,
            vec![&posted.id],
            "the message posted before the repoint did not survive it: {delivered:?}",
        );

        // An owner with no box: a defensive no-op, not an error.
        let nothing = store
            .repoint_owner(
                &EntityId("bot:contract-no-box-owner".to_string()),
                &EntityId("bot:contract-still-no-box".to_string()),
            )
            .await
            .expect("repoint_owner should succeed");
        assert!(
            nothing.is_none(),
            "an owner with no box has nothing to repoint: {nothing:?}",
        );
    }

    /// 🚨 **An owner holding more than one box refuses a repoint rather than
    /// picking one silently.**
    ///
    /// Nothing makes `owner` unique on a mailbox — the create guard screens
    /// the box NAME, never the owner — so more than one row can answer to
    /// one owner, and a repoint that picked one anyway would move it and
    /// orphan the rest: the exact failure a boot's own healing exists to
    /// prevent, reached instead by this call. **Paired against the ordinary
    /// case**, which [`repoint_owner_moves_the_box_and_its_mail`] already
    /// proves: an owner with exactly one box still moves normally, so this
    /// case is about the second box, not about repointing breaking outright.
    pub async fn a_repoint_refuses_when_the_owner_holds_more_than_one_box(store: &dyn Mailboxes) {
        let owner = EntityId(OWNERS[0].to_string());
        create(store, "contract-two-boxes-first").await;
        store
            .create_mailbox(&name("contract-two-boxes-second"), &owner, None)
            .await
            .expect("create ok")
            .written()
            .expect("nothing collides with it");

        let now = EntityId("bot:contract-two-boxes-now".to_string());
        let refused = store
            .repoint_owner(&owner, &now)
            .await
            .expect_err("an owner holding two boxes must refuse rather than pick one");
        assert!(
            matches!(refused, MailboxError::OwnerHasMultipleBoxes { .. }),
            "the wrong reason was given: {refused:?}",
        );

        // Neither box moved: the refusal left the board exactly as it was.
        let boxes = store.list_mailboxes().await.expect("list ok");
        let names: Vec<&str> = boxes.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"contract-two-boxes-first")
                && names.contains(&"contract-two-boxes-second"),
            "a refused repoint must not move either box: {boxes:?}",
        );
        assert!(
            boxes.iter().all(|m| m.owner == owner),
            "a refused repoint must not touch either box's owner: {boxes:?}",
        );
    }

    /// 🚨 **A repoint refuses a destination name a different owner already
    /// holds, rather than reaching the table's own primary key as a
    /// duplicate-key failure.**
    ///
    /// The box key is the slug alone, a different key from the entity handle
    /// a rename's own guard already screened — so a rename onto a handle
    /// whose slug a different box already wears is a real, unguarded
    /// collision until this check exists. **Paired against the ordinary
    /// case** the same way: this box's own repoint, onto a free name, is
    /// what [`repoint_owner_moves_the_box_and_its_mail`] already proves
    /// works: this case is about the collision, not about repointing
    /// breaking outright.
    pub async fn a_repoint_refuses_a_destination_name_a_different_owner_already_holds(
        store: &dyn Mailboxes,
    ) {
        let moving = EntityId(OWNERS[0].to_string());
        let staying = EntityId(OWNERS[1].to_string());
        create(store, "contract-name-taken-moving").await;
        store
            .create_mailbox(&name("contract-name-taken-target"), &staying, None)
            .await
            .expect("create ok")
            .written()
            .expect("nothing collides with it");

        let now = EntityId("bot:contract-name-taken-target".to_string());
        let refused = store
            .repoint_owner(&moving, &now)
            .await
            .expect_err("a rename onto a name a different owner holds must refuse");
        assert!(
            matches!(refused, MailboxError::NameTaken { .. }),
            "the wrong reason was given: {refused:?}",
        );

        // Neither box moved: the refused rename left the board exactly as
        // it was, and the collision was caught before either row changed.
        let boxes = store.list_mailboxes().await.expect("list ok");
        let moving_box = boxes
            .iter()
            .find(|m| m.name.as_str() == "contract-name-taken-moving")
            .unwrap_or_else(|| panic!("the box that was refused a move stayed put: {boxes:?}"));
        assert_eq!(moving_box.owner, moving);
        let staying_box = boxes
            .iter()
            .find(|m| m.name.as_str() == "contract-name-taken-target")
            .unwrap_or_else(|| panic!("the box already holding the name is untouched: {boxes:?}"));
        assert_eq!(staying_box.owner, staying);
    }

    /// **A box states its one owner, and the board reports it.**
    ///
    /// The owner is not a claim filed somewhere else that has to be kept in
    /// step: it is on the box, set when the box opens. A store that dropped it
    /// on the way to the board would leave every reader unable to say whose a
    /// box is, and nothing else records it.
    pub async fn a_box_carries_its_owner_onto_the_board(store: &dyn Mailboxes) {
        let first = EntityId(OWNERS[0].to_string());
        let second = EntityId(OWNERS[1].to_string());
        store
            .create_mailbox(&name("inbox"), &first, None)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        store
            .create_mailbox(&name("errands"), &second, None)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");

        let listed = store.list_mailboxes().await.expect("list ok");
        let owner_of = |n: &str| {
            listed
                .iter()
                .find(|m| m.name.as_str() == n)
                .unwrap_or_else(|| panic!("{n} is on the board: {listed:?}"))
                .owner
                .clone()
        };
        // Two different owners, so this cannot pass on a store that hands the
        // same handle back for every box.
        assert_eq!(owner_of("inbox"), first);
        assert_eq!(owner_of("errands"), second);
    }

    /// **A person can own a box, and the board says it is private.** The rule
    /// every read path rests on is the owner's kind, so it has to survive the
    /// store: a box a person owns comes back from the board with that owner, and
    /// is private, while a bot's box beside it is not. Two owners of two kinds
    /// so a store that answered the same for every box would fail one half.
    pub async fn a_box_owned_by_a_person_is_private_on_the_board(store: &dyn Mailboxes) {
        crate::memory::kinds::load_shipped();
        let bot = EntityId(OWNERS[0].to_string());
        let person = EntityId(OWNERS[2].to_string());
        assert_eq!(person.kind(), Some(crate::memory::EntityKind::PERSON));
        store
            .create_mailbox(&name("inbox"), &bot, None)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        store
            .create_mailbox(&name("person-milhouse"), &person, None)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");

        let listed = store.list_mailboxes().await.expect("list ok");
        let held = |n: &str| {
            listed
                .iter()
                .find(|m| m.name.as_str() == n)
                .unwrap_or_else(|| panic!("{n} is on the board: {listed:?}"))
        };
        assert_eq!(held("person-milhouse").owner, person);
        assert!(held("person-milhouse").is_private());
        assert!(!held("inbox").is_private());
    }

    /// **A person's box keeps a person's name when it follows its owner.** The
    /// box is renamed with the rename of its owner, and the name is the one a
    /// person's box is opened with: a reslug of the person must not leave the
    /// box wearing a bare slug, which is a bot's name and could one day be a
    /// bot's. The mail follows the box to the new name.
    pub async fn a_persons_box_keeps_a_persons_name_when_it_follows_its_owner(
        store: &dyn Mailboxes,
    ) {
        crate::memory::kinds::load_shipped();
        let was = EntityId(OWNERS[2].to_string());
        let now = EntityId("person:ned-flanders".to_string());
        let box_name = MailboxName::named_for(&was);
        assert_eq!(box_name.as_str(), "person-milhouse");
        store
            .create_mailbox(&box_name, &was, None)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        post(
            store,
            box_name.as_str(),
            "someone",
            "mail before the move",
            0,
        )
        .await;

        let repointed = store
            .repoint_owner(&was, &now)
            .await
            .expect("repoint_owner should succeed")
            .expect("the box existed and was found");
        assert_eq!(repointed.owner, now, "{repointed:?}");
        assert_eq!(
            repointed.name.as_str(),
            "person-ned-flanders",
            "{repointed:?}"
        );
        assert!(repointed.is_private());
        let boxes = store.list_mailboxes().await.expect("list ok");
        assert_eq!(boxes.len(), 1, "the old box does not survive: {boxes:?}");
        let delivery = store
            .read_mailbox(
                &repointed.name,
                TakenBy::Reading,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("read ok")
            .written()
            .expect("not blocked");
        assert_eq!(delivery.messages.len(), 1, "the mail followed the box");
    }

    /// **A box is created FOR somebody, so an owner nobody knows is refused.**
    ///
    /// Its own answer rather than a near-miss on the name: the caller got a
    /// different thing wrong, and a refusal that said "no such box name" would
    /// send them renaming a box they have no business creating. Nothing is
    /// written, and the near misses come from Memory's own screen — a typo
    /// comes back with the handle it probably meant.
    pub async fn a_box_for_an_owner_nobody_knows_is_refused(store: &dyn Mailboxes) {
        // One letter off a roster handle, so a store that screens at all has
        // something to suggest.
        let typo = EntityId(format!("{}x", OWNERS[0]));
        let before = store.list_mailboxes().await.expect("list ok").len();

        let outcome = store
            .create_mailbox(&name("strangers"), &typo, None)
            .await
            .expect("an unknown owner is an answer, not a failure");
        let Guarded::UnknownOwner {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("a box for an owner nobody knows must be refused: {outcome:?}");
        };
        assert_eq!(attempted, typo);
        assert!(
            candidates.iter().any(|c| c.handle.as_str() == OWNERS[0]),
            "the refusal names the handle it probably meant: {candidates:?}"
        );
        assert_eq!(
            store.list_mailboxes().await.expect("list ok").len(),
            before,
            "a refused create writes no box"
        );

        // The positive the refusal rests on: the very same NAME opens for an
        // owner that does resolve. Without it this passes on a store that
        // refuses every creation for any reason at all.
        let opened = store
            .create_mailbox(&name("strangers"), &owner(), None)
            .await
            .expect("create ok")
            .written()
            .expect("a resolvable owner opens the box");
        assert_eq!(opened.name.as_str(), "strangers");
    }

    /// **The golden case: a typo never mints a second box.** Creating `inbx`
    /// beside `inbox` comes back with the box the caller meant, and the board is
    /// unchanged.
    pub async fn creating_a_near_miss_is_blocked_and_writes_nothing(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let before = store.list_mailboxes().await.expect("list ok").len();

        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .create_mailbox(&name("inbx"), &owner(), None)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("creating a near miss of an existing box must block");
        };
        assert_eq!(attempted.as_str(), "inbx");
        assert_eq!(candidates[0].name.as_str(), "inbox");
        assert_eq!(
            store.list_mailboxes().await.expect("list ok").len(),
            before,
            "a blocked create writes nothing"
        );
    }

    /// **A sibling fleet is deliberate — and creatable.** `worker-2` beside
    /// `worker-1` blocks as a near miss until the caller hands back the token
    /// that refusal minted, which lifts the similarity screen. An exact name
    /// stays blocked regardless: that box already exists.
    ///
    /// The token is read out of the refusal rather than made up beside the
    /// assertion, because reading it back is the whole behaviour under test: a
    /// store that accepts any string passes the first half of this and fails
    /// the third.
    pub async fn a_confirmed_near_miss_creates_the_sibling_box(store: &dyn Mailboxes) {
        create(store, "worker-1").await;

        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .create_mailbox(&name("worker-2"), &owner(), None)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("without a token, a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "worker-1");
        let token = guard::override_token(&attempted, &candidates);

        assert!(
            matches!(
                store
                    .create_mailbox(&name("worker-2"), &owner(), Some("0000000000000000"))
                    .await
                    .expect("a blocked create is a result, not a failure"),
                Guarded::Blocked { .. }
            ),
            "a token nobody minted lifts nothing"
        );

        let created = store
            .create_mailbox(&name("worker-2"), &owner(), Some(&token))
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("the refusal's own token must lift the near-miss screen");
        assert_eq!(created.name.as_str(), "worker-2");

        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .create_mailbox(&name("worker-1"), &owner(), None)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("an exact name blocks: the box exists");
        };
        assert_eq!(candidates[0].reason, guard::MatchReason::Exact);

        let exact_token = guard::override_token(&attempted, &candidates);
        assert!(
            matches!(
                store
                    .create_mailbox(&name("worker-1"), &owner(), Some(&exact_token))
                    .await
                    .expect("a blocked create is a result, not a failure"),
                Guarded::Blocked { .. }
            ),
            "an exact name stays blocked by its own token too: the box exists"
        );
    }

    /// A posted message lands in `new`, carrying exactly what was posted.
    pub async fn a_posted_message_lands_in_new(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;

        assert_eq!(posted.mailbox.as_str(), "inbox");
        assert_eq!(posted.sender, "alpha");
        assert_eq!(posted.body, "the shipment landed");
        assert_eq!(posted.sent_at, at(0));
        assert_eq!(posted.state, MessageState::New, "a posted message is new");
        assert_eq!(posted.notes, None);
        assert_eq!(posted.subject, None, "a message without a subject has none");
        assert!(!posted.id.as_str().is_empty(), "the store mints an id");

        let counts = counts(store, "inbox")
            .await
            .expect("the box is on the board");
        assert_eq!(counts.new, 1);
        assert_eq!(counts.total(), 1);
    }

    /// A body is prose: paragraphs survive the round trip verbatim.
    pub async fn a_body_survives_the_round_trip(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let body = "first line\n\nsecond paragraph, with a | pipe and a *star*";
        let posted = post(store, "inbox", "alpha", body, 0).await;
        assert_eq!(posted.body, body);

        let delivered = read(store, "inbox").await;
        assert_eq!(
            delivered.messages[0].message.body, body,
            "…and on the way back out"
        );
    }

    /// A body written on a CRLF platform reads back the same in every store.
    /// Line endings normalize to `\n` on the way in — one contract, one answer:
    /// without this, a store that reconstructs text line-by-line strips the
    /// `\r`s while a store that keeps bytes preserves them, and the same body
    /// round-trips in one tier and hard-errors in another.
    pub async fn a_crlf_body_normalizes_to_plain_newlines(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "line one\r\nline two", 0).await;
        assert_eq!(posted.body, "line one\nline two");

        let delivered = read(store, "inbox").await;
        assert_eq!(
            delivered.messages[0].message.body, "line one\nline two",
            "…and on the way back out"
        );

        // A stacked `\r\r\n` must not leave a CRLF behind: a single
        // non-overlapping replace turns it into exactly the sequence it was
        // meant to remove, and the store diverges again on that input.
        let stacked = post(store, "inbox", "alpha", "line one\r\r\nline two", 30).await;
        assert!(
            !stacked.body.contains('\r') || !stacked.body.contains("\r\n"),
            "no \\r may sit before a \\n after normalization: {:?}",
            stacked.body
        );
        assert_eq!(
            stacked.body, "line one\nline two",
            "normalized to a fixpoint"
        );
    }

    /// A body full of HTML-significant characters and an unterminated fence
    /// survives the round trip verbatim — the store may keep rich text or lean
    /// on fenced blocks itself, and neither is allowed to eat a message.
    pub async fn a_body_of_markup_and_a_loose_fence_survives(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let body = "compare a & b, note 1 < 2 and 2 > 1, keep &amp; literal\n\n\
                    ```\nan unterminated fence";
        let posted = post(store, "inbox", "alpha", body, 0).await;
        assert_eq!(posted.body, body);

        let delivered = read(store, "inbox").await;
        assert_eq!(
            delivered.messages[0].message.body, body,
            "…and on the way back out"
        );
    }

    /// **What was waiting for the sender at send time survives storage,
    /// including zero.**
    ///
    /// The caller supplies this — the domain never reads a board to compute it
    /// — so the store's whole job is to carry it verbatim. A store that stored
    /// `Some(0)` as absent, or that only carried a non-zero value, would pass
    /// every other case in this suite; this is the one that pins the value
    /// rather than just its presence.
    pub async fn senders_mail_waiting_at_send_survives_the_round_trip(store: &dyn Mailboxes) {
        create(store, "inbox").await;

        let with_mail = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "reporting in".to_string(),
                subject: None,
                sender: "alpha".to_string(),
                sent_at: at(0),
                in_reply_to: None,
                sender_mail_waiting_at_send: Some(3),
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .expect("the guard must not block posting to 'inbox'");
        assert_eq!(with_mail.sender_mail_waiting_at_send, Some(3));

        let empty_handed = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "nothing was waiting for me".to_string(),
                subject: None,
                sender: "alpha".to_string(),
                sent_at: at(1),
                in_reply_to: None,
                sender_mail_waiting_at_send: Some(0),
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post_message should succeed")
            .written()
            .expect("the guard must not block posting to 'inbox'");
        assert_eq!(
            empty_handed.sender_mail_waiting_at_send,
            Some(0),
            "zero waiting is a real answer, not the absence a plain post gives"
        );

        // The ordinary post carries none, exactly as every message posted
        // before this field existed does.
        let ordinary = post(store, "inbox", "alpha", "the old shape", 2).await;
        assert_eq!(ordinary.sender_mail_waiting_at_send, None);

        let delivered = read(store, "inbox").await;
        let by_id = |id: &MessageId| {
            delivered
                .messages
                .iter()
                .find(|d| &d.message.id == id)
                .unwrap_or_else(|| panic!("{id:?} was delivered"))
        };
        assert_eq!(
            by_id(&with_mail.id).message.sender_mail_waiting_at_send,
            Some(3),
            "…and on the way back out, through the delivery a reader actually sees"
        );
        assert_eq!(
            by_id(&empty_handed.id).message.sender_mail_waiting_at_send,
            Some(0)
        );
        assert_eq!(
            by_id(&ordinary.id).message.sender_mail_waiting_at_send,
            None
        );
    }

    /// **A subject the store rewrites still posts.**
    ///
    /// The message this is written from was refused three times in
    /// production. Its subject carried a tilde; the store escaped it on the
    /// way in; the read-back guard compared bytes, saw a difference, and
    /// rolled a write back that had SUCCEEDED. The rollback is where the
    /// damage came from — an orphaned body, a consumed id, a page a person
    /// had to repair by hand.
    ///
    /// Every string below was escaped by real Outline in a recorded golden.
    /// Against a store that rewrites nothing this passes trivially, which is
    /// correct and is why it lives in the shared contract: the adapter that
    /// has to survive it is the one standing in front of a markdown editor.
    pub async fn a_subject_the_store_rewrites_still_posts(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        // Both things the store does to a cell are here: an inserted escape,
        // and a respelled emphasis marker. The snake-cased one is the case
        // that decides whether ordinary technical subjects are writable at
        // all, and the store leaves it alone.
        for (n, subject) in [
            "_under_ emphasis",
            "parse_bodies and same_cell_value",
            "a ~ b ~ c",
            "# heading",
            "- a leading dash",
            "a | pipe",
            "<b>bold</b>",
        ]
        .into_iter()
        .enumerate()
        {
            let posted = titled(
                store,
                "inbox",
                "alpha",
                Some(subject),
                "the body is fenced and was never at risk",
                n as i64,
            )
            .await;
            // The subject came back — the point is that the write was not
            // refused. Whether the store escaped it on the page is the store's
            // business, and forgiving exactly that is the fix.
            assert!(
                posted.subject.is_some(),
                "a subject the store rewrites must still land: {subject:?}"
            );
        }
    }

    /// **A subject survives every path a message travels.** It is written on
    /// the way in, comes back on the posted record, and is still there on the
    /// delivery and on the processed archive — a title that only existed at the
    /// moment of posting would be a title no reader ever sees.
    pub async fn a_subject_rides_with_the_message(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = titled(
            store,
            "inbox",
            "alpha",
            Some("the shipment"),
            "it landed at dawn; the crates are stacked by the north door",
            0,
        )
        .await;
        assert_eq!(posted.subject.as_deref(), Some("the shipment"));
        assert_eq!(
            posted.body, "it landed at dawn; the crates are stacked by the north door",
            "the subject is beside the body, never carved out of it"
        );

        let delivered = read(store, "inbox").await;
        assert_eq!(
            delivered.messages[0].message.subject.as_deref(),
            Some("the shipment"),
            "…and on the way back out"
        );

        let processed = store
            .mark_processed(&posted.id, None, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("ok");
        assert_eq!(
            processed.subject.as_deref(),
            Some("the shipment"),
            "processing rewrites the outcome, not the title"
        );
    }

    /// A blank subject is no subject, and one carrying a line break is refused
    /// — it rides in a one-line field, exactly as `sender` does. Nothing
    /// malformed reaches the board.
    pub async fn a_blank_subject_is_absent_and_a_broken_one_is_refused(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let blank = titled(
            store,
            "inbox",
            "alpha",
            Some("   "),
            "the shipment landed",
            0,
        )
        .await;
        assert_eq!(blank.subject, None, "a blank subject is absent, not empty");

        let broken = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "the shipment landed".into(),
                subject: Some("two\nlines".into()),
                sender: "alpha".into(),
                sent_at: at(30),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await;
        assert!(broken.is_err(), "a subject is one plain line");
        assert_eq!(
            counts(store, "inbox").await.expect("inbox exists").total(),
            1,
            "the refused post never reached the board"
        );
    }

    /// **One message, taken by id.** A consumer that wants a single filed
    /// message must not have to take delivery of — and own — the whole box: the
    /// named message moves `new → read` and everything else stays exactly where
    /// it was.
    pub async fn read_message_takes_one_and_leaves_the_rest(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let wanted = post(store, "inbox", "alpha", "the one worth reading", 0).await;
        post(store, "inbox", "milhouse", "the rest of the box", 60).await;

        let delivered = store
            .read_message(&wanted.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(delivered.message.id, wanted.id);
        assert_eq!(delivered.message.body, "the one worth reading");
        assert_eq!(
            delivered.message.state,
            MessageState::Read,
            "delivery moves the column"
        );
        assert!(
            !delivered.seen_before,
            "a first delivery is nobody's leftover"
        );

        let after_one = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(after_one.read, 1, "exactly one message was taken");
        assert_eq!(after_one.new, 1, "…and the rest of the box was left alone");

        // Taking it twice is the leftover case, not a second delivery.
        let again = store
            .read_message(&wanted.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert!(
            again.seen_before,
            "a message already delivered comes back flagged, exactly as a box read flags it"
        );
        let after_two = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(after_two.read, 1, "…and nothing else moved with it");
        assert_eq!(after_two.new, 1);
    }

    /// **Reading an archive does not reopen it.** `processed` is terminal, so a
    /// processed message named by id is handed back in the state it is in —
    /// walking it back to `read` would put a handled message into the next
    /// delivery as owed work.
    pub async fn read_message_leaves_a_processed_message_terminal(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        store
            .mark_processed(
                &posted.id,
                Some("filed under shipments"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("mark_processed ok");

        let delivered = store
            .read_message(&posted.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(
            delivered.message.state,
            MessageState::Processed,
            "nothing moves out of processed"
        );
        assert_eq!(
            delivered.message.notes.as_deref(),
            Some("filed under shipments")
        );
        assert!(
            delivered.seen_before,
            "an archive read is nobody's fresh mail"
        );
        assert_eq!(
            counts(store, "inbox")
                .await
                .expect("inbox exists")
                .processed,
            1,
            "the counts agree: it is still processed"
        );
        assert!(
            read(store, "inbox").await.messages.is_empty(),
            "…and it is still out of the delivery set"
        );
    }

    /// **The scan behind search sees the whole board.** Every box, every state
    /// — `processed` included, because finding the report somebody filed last
    /// month is half the reason mail is searchable at all — and it moves
    /// nothing: a scan is a read, so running it over the whole board at boot
    /// cannot make a message owed to anybody.
    pub async fn a_scan_sees_every_box_and_every_state(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        let fresh = post(store, "inbox", "alpha", "still new", 0).await;
        let taken = post(store, "inbox", "milhouse", "already taken", 60).await;
        let done = post(store, "errands", "otto", "long since handled", 120).await;
        store
            .read_message(&taken.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        store
            .mark_processed(
                &done.id,
                Some("filed"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("ok");

        let scanned = store.scan_messages().await.expect("scan_messages ok");
        let mut seen: Vec<(&str, &str, &str)> = scanned
            .iter()
            .map(|m| (m.mailbox.as_str(), m.state.as_token(), m.body.as_str()))
            .collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            vec![
                ("errands", "processed", "long since handled"),
                ("inbox", "new", "still new"),
                ("inbox", "read", "already taken"),
            ],
            "every box, every state, nothing left out"
        );

        // A scan is a read: the counts are exactly what they were before it.
        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!((counts.new, counts.read, counts.processed), (1, 1, 0));
        assert!(
            !store
                .read_message(&fresh.id, crate::mailbox::Acting::anyone_anytime())
                .await
                .expect("ok")
                .seen_before,
            "the scan did not take delivery of anything"
        );
    }

    /// **One message is found by its id, from any box and in any state, and
    /// finding it moves nothing.** An id nobody wears is a miss rather than a
    /// failure, and a message that was quarantined on purpose is not served.
    pub async fn a_message_is_found_by_its_id_from_any_box_and_state(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        let fresh = post(store, "inbox", "alpha", "still new", 0).await;
        let done = post(store, "errands", "otto", "long since handled", 60).await;
        let doomed = post(store, "inbox", "alpha", "the count is wrong", 120).await;
        store
            .mark_processed(
                &done.id,
                Some("filed"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("ok");
        store
            .quarantine(
                &doomed.id,
                &name("inbox"),
                "not to be trusted",
                crate::mailbox::Acting::unnamed(at(5)),
            )
            .await
            .expect("quarantined");

        let found = store
            .message_by_id(&fresh.id)
            .await
            .expect("message_by_id ok")
            .expect("a new message is found");
        assert_eq!(
            (found.mailbox.as_str(), found.body.as_str()),
            ("inbox", "still new")
        );
        let archived = store
            .message_by_id(&done.id)
            .await
            .expect("message_by_id ok")
            .expect("a processed message is found from another box");
        assert_eq!(archived.state, MessageState::Processed);
        assert_eq!(archived.body, "long since handled");

        assert!(
            store
                .message_by_id(&MessageId("nosuch".into()))
                .await
                .expect("a miss is an answer")
                .is_none(),
            "an id nobody wears is no message"
        );
        assert!(
            store
                .message_by_id(&doomed.id)
                .await
                .expect("a quarantined message is an answer")
                .is_none(),
            "a message quarantined on purpose is not served"
        );

        // A find is a read: the new message is still new.
        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(counts.new, 1, "finding a message took delivery of it");
    }

    /// **Several messages are found by their ids in one read**, from any box and
    /// state, in the order the scan reads and not the order the ids were asked in.
    /// An id nobody wears, a quarantined message and an id asked twice add
    /// nothing, an empty ask is an empty answer, and finding them moves nothing.
    pub async fn several_messages_are_found_by_their_ids_in_scan_order(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        let first = post(store, "inbox", "alpha", "first", 0).await;
        let handled = post(store, "errands", "otto", "second, handled", 60).await;
        let left_out = post(store, "inbox", "alpha", "third, not asked for", 90).await;
        let doomed = post(store, "inbox", "alpha", "fourth, quarantined", 120).await;
        store
            .mark_processed(
                &handled.id,
                Some("filed"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("ok");
        store
            .quarantine(
                &doomed.id,
                &name("inbox"),
                "not to be trusted",
                crate::mailbox::Acting::unnamed(at(5)),
            )
            .await
            .expect("quarantined");

        // Asked newest first, with a miss, the quarantined one and a repeat.
        let found = store
            .messages_by_id(&[
                handled.id.clone(),
                MessageId("nosuch".into()),
                doomed.id.clone(),
                first.id.clone(),
                handled.id.clone(),
            ])
            .await
            .expect("messages_by_id ok");
        assert_eq!(
            found.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            ["first", "second, handled"],
            "the readable ones asked for, once each, in scan order"
        );
        assert_eq!(found[1].state, MessageState::Processed);
        assert!(
            !found.iter().any(|m| m.id == left_out.id),
            "a message nobody asked for is not in the answer"
        );
        assert!(
            store
                .messages_by_id(&[])
                .await
                .expect("an empty ask is an answer")
                .is_empty(),
            "no ids, no messages"
        );

        // A find is a read: the two new messages are still new.
        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(counts.new, 2, "finding messages took delivery of them");
    }

    /// **A sender's mail is found by sender, across boxes and states, in the
    /// order the scan reads, and finding it moves nothing.** Another sender's
    /// mail is not in it.
    pub async fn a_senders_mail_is_found_by_sender_across_boxes_and_states(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        post(store, "inbox", "alpha", "alpha one", 0).await;
        let handled = post(store, "errands", "alpha", "alpha two", 60).await;
        post(store, "inbox", "milhouse", "somebody else's", 90).await;
        post(store, "errands", "alpha", "alpha three", 120).await;
        store
            .mark_processed(
                &handled.id,
                Some("filed"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("ok");

        let sent = store.sent_by(&["alpha"]).await.expect("sent_by ok");
        let bodies: Vec<(&str, &str)> = sent
            .iter()
            .map(|m| (m.mailbox.as_str(), m.body.as_str()))
            .collect();
        assert_eq!(
            bodies,
            vec![
                ("inbox", "alpha one"),
                ("errands", "alpha two"),
                ("errands", "alpha three"),
            ],
            "every box, every state, oldest first, and nobody else's"
        );
        assert!(
            store
                .sent_by(&["nobody"])
                .await
                .expect("sent_by ok")
                .is_empty(),
            "a sender who sent nothing has an empty list"
        );

        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(counts.new, 2, "listing what was sent took delivery of mail");
    }

    /// An id nothing answers to is a miss here for the same reason it is one
    /// for `mark_processed` — and it is the same answer, so one client branch
    /// handles both.
    pub async fn reading_an_unknown_message_is_a_miss(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let err = store
            .read_message(
                &MessageId("999999".into()),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect_err("an unknown id must not report success");
        assert!(
            matches!(err, MailboxError::UnknownMessage { .. }),
            "got {err:?}"
        );
    }

    /// **A typo must never silently mint a box.** Posting into a name jojobot
    /// doesn't know comes back blocked, with the box it suspects — and creates
    /// nothing.
    pub async fn posting_into_an_unknown_mailbox_is_blocked(store: &dyn Mailboxes) {
        create(store, "inbox").await;

        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .post_message(NewMessage {
                mailbox: name("inbx"),
                body: "the shipment landed".into(),
                subject: None,
                sender: "alpha".into(),
                sent_at: at(0),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("a blocked post is a result, not a failure")
        else {
            panic!("posting into an unknown box must block");
        };
        assert_eq!(attempted.as_str(), "inbx");
        assert_eq!(candidates[0].name.as_str(), "inbox");

        assert!(
            counts(store, "inbx").await.is_none(),
            "a blocked post must not have minted the box"
        );
        assert_eq!(
            counts(store, "inbox").await.expect("inbox exists").total(),
            0,
            "…and must not have landed in the box it resembles either"
        );
    }

    /// A read delivers everything unprocessed, oldest first, and moves the
    /// column `new → read`.
    pub async fn a_read_delivers_everything_new_and_moves_the_column(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        // **Posted out of order on purpose.** With the later message posted
        // first, insertion order and `sent_at` order disagree, so "oldest
        // first" is an assertion about the instant rather than about whichever
        // order the store happened to return.
        post(store, "inbox", "milhouse", "second", 60).await;
        post(store, "inbox", "alpha", "first", 0).await;

        let delivery = read(store, "inbox").await;
        assert_eq!(delivery.mailbox.as_str(), "inbox");
        let bodies: Vec<&str> = delivery
            .messages
            .iter()
            .map(|d| d.message.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["first", "second"], "oldest first");
        assert!(
            delivery.messages.iter().all(|d| !d.seen_before),
            "a first delivery is nobody's leftover"
        );
        assert!(
            delivery
                .messages
                .iter()
                .all(|d| d.message.state == MessageState::Read),
            "delivery moves the column: {:?}",
            delivery.messages
        );

        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(counts.new, 0, "nothing is left in new");
        assert_eq!(counts.read, 2);
    }

    /// **Two messages inside one second still deliver oldest first.**
    ///
    /// A second's resolution is not the board's resolution: two sessions
    /// posting in the same second is ordinary, and a delivery that put them
    /// the wrong way round would tell a reader the reply came before the
    /// question.
    ///
    /// Its own case because whole-second stamps cannot catch it, and because a
    /// store is free to keep the instant in a form whose natural sort is not
    /// chronological — a text stamp is the obvious example, where a stamp with
    /// no fractional part sorts after every stamp that has one. A store that
    /// ordered by that form passes every other case here.
    pub async fn a_delivery_orders_inside_one_second(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let within = |nanos: i64| epoch() + jiff::SignedDuration::from_nanos(nanos);
        // Posted newest first, so insertion order and instant order disagree.
        for (body, nanos) in [("third", 500_000_000), ("second", 1), ("first", 0)] {
            store
                .post_message(NewMessage {
                    mailbox: name("inbox"),
                    body: body.to_string(),
                    subject: None,
                    sender: "alpha".into(),
                    sent_at: within(nanos),
                    in_reply_to: None,
                    sender_mail_waiting_at_send: None,
                    posted_by_session: None,
                    supersedes: None,
                })
                .await
                .expect("post ok")
                .written()
                .expect("not blocked");
        }

        let delivery = read(store, "inbox").await;
        assert_eq!(
            delivery
                .messages
                .iter()
                .map(|d| d.message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"],
            "oldest first, to the nanosecond"
        );
    }

    /// **The crashed consumer.** A second read hands the same messages over
    /// again — flagged as already seen, so a consumer that took a batch and
    /// died is visible as such rather than looking like fresh mail.
    pub async fn a_second_read_redelivers_leftovers_flagged(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        post(store, "inbox", "alpha", "first", 0).await;
        read(store, "inbox").await;

        // Sent *earlier* than the leftover, and posted later — so the ordering
        // has to hold across the two columns as well as within one.
        post(store, "inbox", "milhouse", "earlier", -60).await;
        post(store, "inbox", "otto", "later", 60).await;
        let again = read(store, "inbox").await;
        assert_eq!(again.messages.len(), 3, "the leftover and both fresh ones");
        let bodies: Vec<&str> = again
            .messages
            .iter()
            .map(|d| d.message.body.as_str())
            .collect();
        assert_eq!(
            bodies,
            vec!["earlier", "first", "later"],
            "oldest first spans the columns: a leftover is not automatically first"
        );

        let leftovers: Vec<&str> = again.leftovers().map(|d| d.message.body.as_str()).collect();
        assert_eq!(leftovers, vec!["first"], "the leftover is flagged apart");
        let fresh: Vec<&str> = again
            .messages
            .iter()
            .filter(|d| !d.seen_before)
            .map(|d| d.message.body.as_str())
            .collect();
        assert_eq!(fresh, vec!["earlier", "later"]);
    }

    /// `mark_processed` is terminal: the message leaves the delivery set for
    /// good, and the outcome the consumer recorded is on it.
    pub async fn mark_processed_is_terminal_and_records_the_outcome(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        read(store, "inbox").await;

        let processed = store
            .mark_processed(
                &posted.id,
                Some("filed under shipments"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("mark_processed ok");
        assert_eq!(processed.id, posted.id);
        assert_eq!(processed.state, MessageState::Processed);
        assert_eq!(processed.notes.as_deref(), Some("filed under shipments"));
        assert_eq!(
            processed.body, posted.body,
            "processing does not rewrite the message"
        );
        assert_eq!(processed.sender, posted.sender);
        assert_eq!(processed.sent_at, posted.sent_at);

        let after = read(store, "inbox").await;
        assert!(
            after.messages.is_empty(),
            "a processed message is never delivered again: {:?}",
            after.messages
        );
        let counts = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(counts.processed, 1);
        assert_eq!(counts.total(), 1, "processed is archive, not deletion");
    }

    /// A failure is data, not a state: it is recorded as the outcome of a
    /// message that WAS handled, and there is no column for it.
    pub async fn a_failure_is_recorded_as_an_outcome(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "deliver the crates", 0).await;
        read(store, "inbox").await;

        let processed = store
            .mark_processed(
                &posted.id,
                Some("FAILED: the loading dock was closed"),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("mark_processed ok");
        assert_eq!(processed.state, MessageState::Processed);
        assert!(
            processed
                .notes
                .as_deref()
                .is_some_and(|n| n.contains("FAILED"))
        );
    }

    /// **A reply says what it is replying to, and the link must resolve.** A
    /// hand-off and its report were correlated only by prose convention
    /// ("report = message 935"), which is fine at today's volume and archaeology
    /// later. The link is optional and carries no semantics beyond itself: it
    /// says these two messages are one exchange, not that either is owed.
    pub async fn a_reply_names_the_message_it_answers(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let original = post(store, "inbox", "alpha", "please count the crates", 0).await;

        let reply = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "counted them: forty".into(),
                subject: None,
                sender: "beta".into(),
                sent_at: at(1),
                in_reply_to: Some(original.id.clone()),
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("a reply to a message that exists is written");
        assert_eq!(reply.in_reply_to.as_ref(), Some(&original.id));

        // …and it survives the round trip, which is the whole point of a link
        // nobody is going to re-derive from prose later.
        let seen = store
            .read_message(&reply.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(seen.message.in_reply_to.as_ref(), Some(&original.id));

        // The message it answers is untouched — a reply is not a delivery.
        let original_now = store
            .read_message(&original.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read ok");
        assert_eq!(original_now.message.in_reply_to, None);
    }

    /// **A reply answers ACROSS boxes, which is the shape it will actually run
    /// in.** A hand-off is left in one box and its report goes back in another;
    /// a reply into the same box is the easy case and not the real one. It runs
    /// on every tier because this is precisely where a fake and a store can
    /// quietly disagree: the real adapter looks up `in_reply_to` by the
    /// message's own id, unscoped to any box, and if a fake narrowed that
    /// lookup to the sender's own box it would stay green while production
    /// stopped linking.
    pub async fn a_reply_can_answer_a_message_in_another_box(store: &dyn Mailboxes) {
        create(store, "dev").await;
        create(store, "pm").await;
        let handoff = post(store, "dev", "coordinator", "build the kiln slice", 0).await;

        let report = store
            .post_message(NewMessage {
                mailbox: name("pm"),
                body: "the kiln slice is done".into(),
                subject: None,
                sender: "implementer".into(),
                sent_at: at(1),
                in_reply_to: Some(handoff.id.clone()),
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("a reply across boxes is written");
        assert_eq!(
            report.mailbox,
            name("pm"),
            "the reply is in the box it was posted to"
        );
        assert_eq!(
            report.in_reply_to.as_ref(),
            Some(&handoff.id),
            "…and it answers the message in the other one"
        );

        let seen = store
            .read_message(&report.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(seen.message.in_reply_to.as_ref(), Some(&handoff.id));
    }

    /// **Everything a write names must already exist**, and a reply naming a
    /// message jojobot does not hold is the same miss every other dangling
    /// reference is. Nothing is written.
    pub async fn a_reply_to_an_unknown_message_is_refused(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let missing = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "answering something that was never said".into(),
                subject: None,
                sender: "beta".into(),
                sent_at: at(0),
                in_reply_to: Some(MessageId("9999".into())),
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await;
        assert!(
            matches!(missing, Err(MailboxError::UnknownMessage { .. })),
            "a dangling reply link is a miss, not a stored message: {missing:?}"
        );

        // **A malformed id is refused as malformed**, not looked up and
        // reported as absent: an id carrying a path segment or a quote never
        // reaches a store, and saying "no such message" about it would send the
        // caller hunting for a message rather than fixing their id.
        let malformed = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "answering something misspelt".into(),
                subject: None,
                sender: "beta".into(),
                sent_at: at(0),
                in_reply_to: Some(MessageId("../42".into())),
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await;
        assert!(
            matches!(malformed, Err(MailboxError::InvalidMessageId(_))),
            "a reply link outside the id grammar is malformed, not missing: {malformed:?}"
        );
        assert_eq!(
            counts(store, "inbox").await.expect("inbox exists").total(),
            0,
            "nothing reached the board"
        );
    }

    /// **The terminal verb never refuses an outcome record for being long.**
    /// The crash contract asks a consumer to write down what happened,
    /// including a failure — and a cap that rejected the whole call made the
    /// ask and the answer contradict each other. It bit a real caller in
    /// production: the message was left unprocessed because its account of the
    /// work did not fit, so the cap cost exactly the record it was policing.
    ///
    /// Long notes are kept as far as they fit and **say they were cut**, which
    /// is the one thing silence could not do.
    pub async fn long_notes_are_kept_as_far_as_they_fit(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;

        let long = "counted the crates and reconciled them against the manifest ".repeat(200);
        let processed = store
            .mark_processed(
                &posted.id,
                Some(&long),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("a long outcome record must not fail the verb");
        assert_eq!(
            processed.state,
            MessageState::Processed,
            "the message WAS handled"
        );

        let kept = processed.notes.as_deref().expect("the outcome is recorded");
        assert!(
            kept.chars().count() <= NOTES_BUDGET,
            "cut to fit: {} chars",
            kept.chars().count()
        );
        assert!(kept.ends_with('…'), "…and it says it was cut: {kept:?}");
        assert!(
            kept.starts_with("counted the crates"),
            "what was kept is the start of what they wrote: {kept:?}"
        );

        // Read back through the ordinary path: what the verb returned is what
        // the store holds, cut and all.
        let seen = store
            .read_message(&posted.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(seen.message.notes.as_deref(), Some(kept));
    }

    /// Notes that fit are stored exactly as written — the cut is for the ones
    /// that don't, and touches nothing else.
    pub async fn notes_that_fit_are_untouched(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        let notes = "x".repeat(NOTES_BUDGET);
        let processed = store
            .mark_processed(
                &posted.id,
                Some(&notes),
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("ok");
        assert_eq!(
            processed.notes.as_deref(),
            Some(notes.as_str()),
            "a record that fits is stored whole, to the last character of the budget"
        );
    }

    /// Marking processed without notes is ordinary — the outcome is optional.
    pub async fn processing_without_notes_is_allowed(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        let processed = store
            .mark_processed(&posted.id, None, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("ok");
        assert_eq!(processed.notes, None);
        assert_eq!(processed.state, MessageState::Processed);
    }

    /// **A message can be processed straight out of `new`.** A consumer that
    /// acts on something it heard about another way must not have to fake a
    /// read first, and the column still ends up terminal.
    pub async fn a_new_message_can_be_processed_without_a_read(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        let processed = store
            .mark_processed(&posted.id, None, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("ok");
        assert_eq!(processed.state, MessageState::Processed);
        assert_eq!(counts(store, "inbox").await.expect("inbox").new, 0);
    }

    /// Boxes are boxes: a read of one never delivers another's mail.
    pub async fn boxes_do_not_leak_into_each_other(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        post(store, "inbox", "alpha", "for the inbox", 0).await;
        post(store, "errands", "milhouse", "for the errands box", 0).await;

        let delivery = read(store, "inbox").await;
        let bodies: Vec<&str> = delivery
            .messages
            .iter()
            .map(|d| d.message.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["for the inbox"]);

        assert_eq!(
            counts(store, "errands").await.expect("errands exists").new,
            1,
            "reading one box must not move another box's column"
        );
    }

    /// Reading a box jojobot doesn't know is blocked with candidates — never an
    /// empty delivery, which would read as "your box is empty" for a name that
    /// does not exist.
    pub async fn reading_an_unknown_mailbox_is_blocked(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .read_mailbox(
                &name("inbx"),
                TakenBy::Reading,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("a blocked read is a result, not a failure")
        else {
            panic!("reading an unknown box must block, not come back empty");
        };
        assert_eq!(attempted.as_str(), "inbx");
        assert_eq!(candidates[0].name.as_str(), "inbox");
    }

    /// **A deliberate quarantine is a decision, not damage** — it names who
    /// made it and why, and it stops the message being served or retired
    /// anywhere else, while its neighbour is untouched.
    pub async fn a_deliberate_quarantine_stops_serving_the_message_and_names_why(
        store: &dyn Mailboxes,
    ) {
        create(store, "inbox").await;
        let target = post(store, "inbox", "alpha", "the count on this one is wrong", 0).await;
        let neighbor = post(store, "inbox", "alpha", "this one is fine", 1).await;

        let quarantined = store
            .quarantine(
                &target.id,
                &name("inbox"),
                "the count cannot be trusted",
                crate::mailbox::Acting::unnamed(at(5)),
            )
            .await
            .expect("quarantining your own mail should succeed");
        assert_eq!(quarantined.id, target.id);
        assert_eq!(quarantined.mailbox, name("inbox"));
        assert_eq!(quarantined.by, "inbox");
        assert_eq!(quarantined.reason, "the count cannot be trusted");
        assert_eq!(quarantined.at, at(5));

        let remaining: Vec<MessageId> = scanned(store, "inbox")
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            remaining,
            vec![neighbor.id.clone()],
            "the quarantined message must not be scanned, and its neighbour must be"
        );

        let delivery = read(store, "inbox").await;
        assert_eq!(
            delivery.messages.len(),
            1,
            "the quarantined message must not be delivered"
        );
        assert_eq!(delivery.messages[0].message.id, neighbor.id);

        let read_err = store
            .read_message(&target.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect_err("a quarantined message must not be readable by id");
        assert!(
            matches!(read_err, MailboxError::QuarantinedOnPurpose { .. }),
            "got {read_err:?}"
        );

        let processed_err = store
            .mark_processed(&target.id, None, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect_err("a quarantined message must not be processable");
        assert!(
            matches!(processed_err, MailboxError::QuarantinedOnPurpose { .. }),
            "got {processed_err:?}"
        );
    }

    /// A box may quarantine only its own mail.
    pub async fn quarantining_another_boxs_message_is_refused(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        create(store, "errands").await;
        let theirs = post(store, "errands", "alpha", "errands' own shipment", 0).await;

        let err = store
            .quarantine(
                &theirs.id,
                &name("inbox"),
                "not mine to decide",
                crate::mailbox::Acting::unnamed(at(0)),
            )
            .await
            .expect_err("a box must not quarantine mail sitting in another box");
        assert!(
            matches!(err, MailboxError::NotYourMessage { ref mailbox, .. } if mailbox == "errands"),
            "got {err:?}"
        );

        // Nothing was written: the message is exactly as readable as before.
        store
            .read_message(&theirs.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("the refused quarantine must not have touched the message");
    }

    /// An id nothing answers to is a miss — never a create, never a silent
    /// success.
    pub async fn processing_an_unknown_message_is_a_miss(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let err = store
            .mark_processed(
                &MessageId("999999".into()),
                None,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect_err("an unknown id must not report success");
        assert!(
            matches!(err, MailboxError::UnknownMessage { .. }),
            "got {err:?}"
        );
    }

    /// Malformed input is refused before anything is written.
    pub async fn malformed_input_is_refused(store: &dyn Mailboxes) {
        assert!(
            store
                .create_mailbox(&name("Inbox"), &owner(), None)
                .await
                .is_err(),
            "a name outside the grammar is refused"
        );
        create(store, "inbox").await;

        let bad_body = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "   ".into(),
                subject: None,
                sender: "alpha".into(),
                sent_at: at(0),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await;
        assert!(bad_body.is_err(), "an empty body is not a message");

        let bad_sender = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "the shipment landed".into(),
                subject: None,
                sender: "  ".into(),
                sent_at: at(0),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await;
        assert!(
            bad_sender.is_err(),
            "a message with no sender has no provenance"
        );

        assert_eq!(
            counts(store, "inbox").await.expect("inbox exists").total(),
            0,
            "nothing malformed reached the board"
        );
    }

    /// The whole spec, against one store. Each case runs on a **fresh** store,
    /// so nothing here depends on the order the others ran in.
    /// Every case, each against a store this hands back fresh.
    ///
    /// **The factory is async because a real store's precondition is.** Every
    /// case files boxes under [`OWNERS`], and a store that resolves owners by
    /// reading Memory cannot be given them by a constructor — the entities have
    /// to be written, which is I/O. The fake seeds them synchronously and does
    /// not need this; the real adapter does, and running this suite against
    /// it is the point of the shape.
    /// **How a delivery was taken is recorded, and the two ways differ.**
    ///
    /// `new` is the only pickup signal a sender has, and mail now leaves it two
    /// ways: somebody opened their box, or somebody posted and their waiting
    /// mail came back with the answer. Both are real deliveries; only one means
    /// anybody looked. A store that recorded them the same would leave a sender
    /// reading `read` and concluding somebody picked their message up.
    ///
    /// Every claim in one case, against one board, because they are one fact
    /// seen from three moments: before delivery, after one taken by reading,
    /// and after one taken by posting.
    pub async fn a_delivery_records_how_it_was_taken(store: &dyn Mailboxes) {
        create(store, "takenby-read").await;
        create(store, "takenby-post").await;
        post(store, "takenby-read", "bot:gamma", "went and got this", 0).await;
        post(store, "takenby-post", "bot:gamma", "this came along", 0).await;

        // Waiting: nothing has happened to it, and that is not "nobody looked".
        let waiting = scanned(store, "takenby-read").await;
        assert_eq!(
            waiting[0].taken_by, None,
            "a message still in new has no delivery to describe: {waiting:?}",
        );

        // Taken by reading — somebody opened the box.
        let by_reading = read(store, "takenby-read").await;
        assert_eq!(
            by_reading.messages[0].message.taken_by,
            Some(TakenBy::Reading),
        );

        // Taken by posting — it came back with an answer nobody asked for.
        let taken = store
            .read_mailbox(
                &name("takenby-post"),
                TakenBy::Posting,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("read_mailbox should succeed")
            .written()
            .expect("the guard must not block it");
        assert_eq!(
            taken.messages[0].message.taken_by,
            Some(TakenBy::Posting),
            "the two ways are told apart, or the sender's signal is worthless",
        );

        // …and it survives storage rather than only the answer that wrote it.
        let stored = scanned(store, "takenby-post").await;
        assert_eq!(stored[0].taken_by, Some(TakenBy::Posting));

        // **The FIRST delivery is the one recorded.** A leftover taken again
        // by a later posting keeps how it was really taken; rewriting it would
        // destroy the signal every time the recipient posted anything.
        let again = store
            .read_mailbox(
                &name("takenby-read"),
                TakenBy::Posting,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("read_mailbox should succeed")
            .written()
            .expect("the guard must not block it");
        assert!(again.messages[0].seen_before, "it is a leftover");
        assert_eq!(
            again.messages[0].message.taken_by,
            Some(TakenBy::Reading),
            "a leftover keeps the way it was first taken: {again:?}",
        );

        // **A delivery by id is a delivery.** Taking one message deliberately
        // is the most looked-at a message ever gets, so a store that recorded
        // nothing here would read back as though nobody took it — the same
        // wrong answer the two ways above exist to prevent.
        create(store, "takenby-id").await;
        post(store, "takenby-id", "bot:gamma", "picked out by hand", 0).await;
        let one = scanned(store, "takenby-id").await;
        assert_eq!(one[0].taken_by, None, "nothing has taken it yet: {one:?}",);
        let by_id = store
            .read_message(&one[0].id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message should succeed");
        assert_eq!(
            by_id.message.taken_by,
            Some(TakenBy::Reading),
            "taking one message by id is somebody looking: {by_id:?}",
        );
        let stored_by_id = scanned(store, "takenby-id").await;
        assert_eq!(
            stored_by_id[0].taken_by,
            Some(TakenBy::Reading),
            "…and it survives storage rather than only the answer: {stored_by_id:?}",
        );
    }

    /// Every message in one box, as the store holds it — the read that proves
    /// a claim survived storage rather than only the answer that wrote it.
    async fn scanned(store: &dyn Mailboxes, mailbox: &str) -> Vec<Message> {
        store
            .scan_messages()
            .await
            .expect("scan_messages should succeed")
            .into_iter()
            .filter(|m| m.mailbox.as_str() == mailbox)
            .collect()
    }

    /// **A box holding processed mail delivers what is owed and nothing else.**
    /// Two messages are retired, one straight from `new` and one after a read,
    /// and two are not. A delivery hands over the two that are owed, a second
    /// hands them over again as leftovers, and the retired two stay retired.
    /// This is the behaviour a store may answer from a narrower read of its
    /// rows than the whole box, so it has to hold on every store.
    pub async fn a_delivery_skips_processed_mail_and_hands_over_what_is_owed(
        store: &dyn Mailboxes,
    ) {
        create(store, "inbox").await;
        let retired_new = post(store, "inbox", "alpha", "retired from new", 0).await;
        let retired_read = post(store, "inbox", "alpha", "retired after a read", 1).await;
        post(store, "inbox", "alpha", "owed first", 2).await;
        post(store, "inbox", "alpha", "owed second", 3).await;
        store
            .read_message(&retired_read.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("the message is taken");
        store
            .mark_processed(
                &retired_new.id,
                None,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("retired");
        store
            .mark_processed(
                &retired_read.id,
                None,
                crate::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("retired");

        let first = read(store, "inbox").await;
        let bodies: Vec<&str> = first
            .messages
            .iter()
            .map(|d| d.message.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["owed first", "owed second"]);
        assert!(
            first.messages.iter().all(|d| !d.seen_before),
            "nothing was handed over before"
        );

        let second = read(store, "inbox").await;
        assert_eq!(second.messages.len(), 2, "the owed two come back");
        assert!(
            second.messages.iter().all(|d| d.seen_before),
            "and come back flagged as leftovers"
        );
        let box_counts = counts(store, "inbox").await.expect("inbox");
        assert_eq!(box_counts.processed, 2, "the retired two stay retired");
    }

    /// Post `body` to `mailbox` from `sender`, replacing the message `target`.
    /// The store's answer is returned as it came, so a case can assert a refusal
    /// as readily as a write.
    async fn superseding(
        store: &dyn Mailboxes,
        mailbox: &str,
        sender: &str,
        body: &str,
        at_offset: i64,
        target: &MessageId,
    ) -> Result<Guarded<Message>, MailboxError> {
        store
            .post_message(NewMessage {
                mailbox: name(mailbox),
                body: body.to_string(),
                subject: None,
                sender: sender.to_string(),
                sent_at: at(at_offset),
                in_reply_to: None,
                supersedes: Some(target.clone()),
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
            })
            .await
    }

    /// **A message whose replacement was set aside is delivered, and the box
    /// does not count it as replaced.** The sender posts A and then B replacing
    /// A. The reader sets B aside. B can no longer be read, so A is the only
    /// words that reach the reader: the delivery hands A over, nothing is named
    /// apart, A reads as replaced by nothing, and B stays set aside.
    ///
    /// The control is a replacement that was not set aside: with it in place A is
    /// withheld, so the case does not hold on a store that never withholds.
    pub async fn a_message_whose_replacement_was_set_aside_is_delivered(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let a = post(store, "inbox", "alpha", "ship it on friday", 0).await;
        let b = superseding(store, "inbox", "alpha", "ship it on monday", 1, &a.id)
            .await
            .expect("post ok")
            .written()
            .expect("a replacement of the sender's own message is written");
        let with_it = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            (with_it.new, with_it.superseded),
            (1, 1),
            "the control: with the replacement in place the original is counted apart"
        );

        store
            .quarantine(
                &b.id,
                &name("inbox"),
                "the sender meant a different box",
                crate::mailbox::Acting::unnamed(at(5)),
            )
            .await
            .expect("setting the replacement aside succeeds");

        let after = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            (after.new, after.superseded),
            (1, 0),
            "the original is waiting and is not counted as replaced: {after:?}"
        );
        let handed = store
            .message_by_id(&a.id)
            .await
            .expect("read ok")
            .expect("there");
        assert_eq!(
            handed.superseded_by, None,
            "a message set aside replaces nothing"
        );

        let delivery = read(store, "inbox").await;
        let ids: Vec<&MessageId> = delivery.messages.iter().map(|d| &d.message.id).collect();
        assert_eq!(ids, vec![&a.id], "the original is delivered");
        assert!(
            delivery.superseded.is_empty(),
            "nothing is named apart: {:?}",
            delivery.superseded
        );
        assert!(
            scanned(store, "inbox")
                .await
                .iter()
                .all(|message| message.id != b.id),
            "the replacement stays set aside"
        );

        // A second replacement of the same message lands: the first one is not
        // standing in its way.
        superseding(store, "inbox", "alpha", "ship it on tuesday", 2, &a.id)
            .await
            .expect("post ok")
            .written()
            .expect("a message whose replacement was set aside can be replaced again");
    }

    /// **A replaced message nobody has read is not delivered, and the box says
    /// so.** The sender posts A and then B replacing A. The reader's delivery
    /// hands over B, names A apart with the id that replaced it, and leaves A in
    /// `new`. The counts agree with the delivery: A is not counted as waiting,
    /// `superseded` counts it apart, and the total still adds up to both.
    ///
    /// The control is the same two posts without `supersedes`: both are
    /// delivered and nothing is named apart. Without it the case holds on a
    /// store that withholds every second message.
    pub async fn an_unread_message_that_was_replaced_is_listed_apart_not_delivered(
        store: &dyn Mailboxes,
    ) {
        create(store, "inbox").await;
        let a = post(store, "inbox", "alpha", "ship it on friday", 0).await;
        let b = superseding(store, "inbox", "alpha", "ship it on monday", 1, &a.id)
            .await
            .expect("post ok")
            .written()
            .expect("a replacement of the sender's own message in the same box is written");
        assert_eq!(
            b.supersedes.as_ref(),
            Some(&a.id),
            "the link rides on the new one"
        );

        let before = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            (before.new, before.read, before.processed, before.superseded),
            (1, 0, 0, 1),
            "one is waiting and one is counted as replaced: {before:?}"
        );
        assert_eq!(before.total(), 2, "the replaced one is counted, once");

        let delivery = read(store, "inbox").await;
        let handed: Vec<&MessageId> = delivery.messages.iter().map(|d| &d.message.id).collect();
        assert_eq!(handed, vec![&b.id], "only the replacement is delivered");
        assert_eq!(
            delivery.superseded,
            vec![Superseded {
                id: a.id.clone(),
                by: b.id.clone(),
            }],
            "the replaced one is named apart with the id that replaced it"
        );

        let after = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            (after.new, after.read, after.superseded),
            (0, 1, 1),
            "nothing is waiting, the delivered one moved, and the withheld one is counted \
             as replaced: {after:?}"
        );
        assert_eq!(after.total(), 2, "{after:?}");
        let withheld = store
            .message_by_id(&a.id)
            .await
            .expect("read ok")
            .expect("there");
        assert_eq!(withheld.state, MessageState::New, "it was not taken");
        assert_eq!(withheld.superseded_by.as_ref(), Some(&b.id));

        // The control: the same two posts, no link, are both delivered.
        create(store, "plain").await;
        let c = post(store, "plain", "alpha", "ship it on friday", 0).await;
        let d = post(store, "plain", "alpha", "ship it on monday", 1).await;
        let plain = read(store, "plain").await;
        let handed: Vec<&MessageId> = plain.messages.iter().map(|m| &m.message.id).collect();
        assert_eq!(handed, vec![&c.id, &d.id], "without supersedes both arrive");
        assert!(
            plain.superseded.is_empty(),
            "nothing is named apart: {plain:?}"
        );
    }

    /// **A replaced message the reader already has is still owed, and the
    /// replacement is delivered beside it.** A is read, then B replaces it. The
    /// next read hands over A again as a leftover and B as fresh, and names
    /// nothing apart: A is no longer withheld because it was taken before it was
    /// replaced.
    pub async fn a_replaced_message_the_reader_already_has_is_still_delivered(
        store: &dyn Mailboxes,
    ) {
        create(store, "inbox").await;
        let a = post(store, "inbox", "alpha", "ship it on friday", 0).await;
        read(store, "inbox").await;
        let b = superseding(store, "inbox", "alpha", "ship it on monday", 1, &a.id)
            .await
            .expect("post ok")
            .written()
            .expect("a replacement is written");

        let delivery = read(store, "inbox").await;
        let handed: Vec<(&MessageId, bool)> = delivery
            .messages
            .iter()
            .map(|d| (&d.message.id, d.seen_before))
            .collect();
        assert_eq!(handed, vec![(&a.id, true), (&b.id, false)]);
        assert!(
            delivery.superseded.is_empty(),
            "nothing was withheld: {delivery:?}"
        );
        let counted = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            counted.superseded, 0,
            "a taken message is not counted as withheld"
        );
        let a_now = store
            .message_by_id(&a.id)
            .await
            .expect("read ok")
            .expect("there");
        assert_eq!(
            a_now.superseded_by.as_ref(),
            Some(&b.id),
            "it still says it was replaced"
        );
    }

    /// **Taking a replaced message by id works, and ends its being withheld.**
    /// `read_message` hands it over with the id that replaced it. Once taken it
    /// is owed like any other message, so the next delivery returns it as a
    /// leftover.
    pub async fn a_replaced_message_is_readable_by_id_on_request(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let a = post(store, "inbox", "alpha", "ship it on friday", 0).await;
        let b = superseding(store, "inbox", "alpha", "ship it on monday", 1, &a.id)
            .await
            .expect("post ok")
            .written()
            .expect("written");

        let taken = store
            .read_message(&a.id, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("read_message ok");
        assert_eq!(taken.message.body, "ship it on friday");
        assert_eq!(taken.message.superseded_by.as_ref(), Some(&b.id));
        assert_eq!(taken.message.state, MessageState::Read);

        let delivery = read(store, "inbox").await;
        assert!(delivery.superseded.is_empty(), "{delivery:?}");
        assert_eq!(
            delivery.messages.len(),
            2,
            "the one taken by id and the new one"
        );

        store
            .mark_processed(&a.id, None, crate::mailbox::Acting::anyone_anytime())
            .await
            .expect("processed");
        let archived = store
            .message_by_id(&a.id)
            .await
            .expect("read ok")
            .expect("there");
        assert_eq!(
            archived.superseded_by.as_ref(),
            Some(&b.id),
            "an archive keeps the link"
        );
    }

    /// **A post may replace only the sender's own message, in the box it goes
    /// to, once.** Each refusal names its reason and writes nothing; the id that
    /// names nothing is the same miss a reply to nothing is.
    pub async fn a_replacement_is_refused_unless_it_is_the_senders_own_in_the_same_box(
        store: &dyn Mailboxes,
    ) {
        create(store, "inbox").await;
        create(store, "elsewhere").await;
        let mine = post(store, "inbox", "alpha", "ship it on friday", 0).await;
        let theirs = post(store, "inbox", "beta", "ship it on tuesday", 1).await;
        let total = |c: StateCounts| c.total();
        let before = total(counts(store, "inbox").await.expect("inbox"))
            + total(counts(store, "elsewhere").await.expect("elsewhere"));

        let other_sender = superseding(store, "inbox", "alpha", "no", 2, &theirs.id).await;
        assert!(
            matches!(
                other_sender,
                Err(MailboxError::CannotSupersede(
                    SupersedeRefusal::OtherSender { .. }
                ))
            ),
            "another sender's message is refused: {other_sender:?}"
        );

        let other_box = superseding(store, "elsewhere", "alpha", "no", 2, &mine.id).await;
        match other_box {
            Err(MailboxError::CannotSupersede(SupersedeRefusal::OtherBox {
                sits_in,
                posting_into,
                ..
            })) => {
                assert_eq!(
                    (sits_in.as_str(), posting_into.as_str()),
                    ("inbox", "elsewhere")
                );
            }
            other => panic!("a message in another box is refused: {other:?}"),
        }

        let nothing =
            superseding(store, "inbox", "alpha", "no", 2, &MessageId("9999".into())).await;
        assert!(
            matches!(nothing, Err(MailboxError::UnknownMessage { .. })),
            "an id that names nothing is a miss: {nothing:?}"
        );
        let malformed =
            superseding(store, "inbox", "alpha", "no", 2, &MessageId("../42".into())).await;
        assert!(
            matches!(malformed, Err(MailboxError::InvalidMessageId(_))),
            "an id outside the grammar is malformed: {malformed:?}"
        );

        let after = total(counts(store, "inbox").await.expect("inbox"))
            + total(counts(store, "elsewhere").await.expect("elsewhere"));
        assert_eq!(before, after, "no refusal wrote anything");

        // The positive the refusals depend on: the same call, with the sender's
        // own message in its own box, is written.
        let replacement = superseding(store, "inbox", "alpha", "ship it on monday", 3, &mine.id)
            .await
            .expect("post ok")
            .written()
            .expect("the allowed case is written");

        let twice = superseding(store, "inbox", "alpha", "ship it on sunday", 4, &mine.id).await;
        match twice {
            Err(MailboxError::CannotSupersede(SupersedeRefusal::AlreadySuperseded {
                by, ..
            })) => {
                assert_eq!(
                    by,
                    replacement.id.as_str(),
                    "it names the newest to replace instead"
                );
            }
            other => panic!("a second replacement of one message is refused: {other:?}"),
        }
    }

    /// **Replacements of one message that arrive at the same moment land
    /// exactly one.** Four posts from the sender each replace the same message
    /// and are in flight together: one is written, and each of the others is
    /// refused as already replaced, naming the one that landed. The box then
    /// holds the original and that one replacement, and no more.
    ///
    /// The paired positive is the sequential case above: one replacement at a
    /// time is written, and the second is refused.
    pub async fn simultaneous_replacements_of_one_message_land_exactly_one(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let original = post(store, "inbox", "alpha", "ship it on friday", 0).await;

        let (first, second, third, fourth) = tokio::join!(
            superseding(
                store,
                "inbox",
                "alpha",
                "ship it on monday",
                1,
                &original.id
            ),
            superseding(
                store,
                "inbox",
                "alpha",
                "ship it on tuesday",
                2,
                &original.id
            ),
            superseding(
                store,
                "inbox",
                "alpha",
                "ship it on sunday",
                3,
                &original.id
            ),
            superseding(
                store,
                "inbox",
                "alpha",
                "ship it on wednesday",
                4,
                &original.id
            ),
        );
        let answers = [first, second, third, fourth];
        let written: Vec<&Message> = answers
            .iter()
            .filter_map(|answer| match answer {
                Ok(Guarded::Written(message)) => Some(message),
                _ => None,
            })
            .collect();
        assert_eq!(
            written.len(),
            1,
            "exactly one replacement of a message lands: {answers:?}"
        );
        for answer in &answers {
            match answer {
                Ok(Guarded::Written(_)) => {}
                Err(MailboxError::CannotSupersede(SupersedeRefusal::AlreadySuperseded {
                    by,
                    ..
                })) => assert_eq!(
                    by,
                    written[0].id.as_str(),
                    "a refused replacement names the one that landed"
                ),
                other => panic!("the others are refused as already replaced: {other:?}"),
            }
        }
        let counted = counts(store, "inbox").await.expect("inbox exists");
        assert_eq!(
            (counted.total(), counted.superseded),
            (2, 1),
            "the box holds the original and the one replacement: {counted:?}"
        );
    }

    /// **A message's history keeps each change of state, in order, naming the
    /// run that made it.** One run reads the message, then two runs retire it
    /// with two notes: the history is read, processed, processed, with the state
    /// each change left and only the notes that call was given. **The current
    /// read answers as it always did**, which is what keeps the history from
    /// being a second copy of it.
    pub async fn a_messages_history_keeps_each_change_of_state_in_order(store: &dyn Mailboxes) {
        use crate::mailbox::{Acting, MessageEventKind};
        create(store, "inbox").await;
        let message = post(store, "inbox", "alpha", "the count on this one is wrong", 0).await;
        assert_eq!(
            store
                .message_history(&message.id)
                .await
                .expect("a posted message has a history, and it is empty"),
            Vec::new(),
            "a message nobody has changed has no event"
        );

        store
            .read_message(&message.id, Acting::by("run-a", at(10)))
            .await
            .expect("reading one message succeeds");
        store
            .mark_processed(&message.id, Some("first note"), Acting::by("run-a", at(11)))
            .await
            .expect("the first retirement succeeds");
        store
            .mark_processed(
                &message.id,
                Some("second note"),
                Acting::by("run-b", at(12)),
            )
            .await
            .expect("a second retirement succeeds");

        let history = store
            .message_history(&message.id)
            .await
            .expect("the history reads");
        assert_eq!(
            history.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![
                MessageEventKind::Read,
                MessageEventKind::Processed,
                MessageEventKind::Processed
            ],
            "{history:?}"
        );
        assert_eq!(
            history.iter().map(|e| e.ordinal).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            history.iter().map(|e| e.at).collect::<Vec<_>>(),
            vec![at(10), at(11), at(12)]
        );
        assert_eq!(
            history
                .iter()
                .map(|e| e.by_session.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("run-a"), Some("run-a"), Some("run-b")],
            "each change names the run that made it: {history:?}"
        );
        assert_eq!(
            history.iter().map(|e| e.prior_state).collect::<Vec<_>>(),
            vec![
                MessageState::New,
                MessageState::Read,
                MessageState::Processed
            ]
        );
        assert_eq!(
            history
                .iter()
                .map(|e| e.notes.as_deref())
                .collect::<Vec<_>>(),
            vec![None, Some("first note"), Some("second note")],
            "the second retirement overwrote the first's notes on the message, and the history keeps both: {history:?}"
        );
        assert!(
            history.iter().all(|e| e.by_box == Some(name("inbox"))),
            "{history:?}"
        );

        let current = store
            .message_by_id(&message.id)
            .await
            .expect("the board reads")
            .expect("the message is on the board");
        assert_eq!(current.state, MessageState::Processed);
        assert_eq!(current.notes.as_deref(), Some("second note"));
    }

    /// **A retirement that sends no notes records none**, though the message
    /// carries the earlier call's notes forward. The history says only what each
    /// call was given.
    pub async fn a_retirement_with_no_notes_is_recorded_with_none(store: &dyn Mailboxes) {
        use crate::mailbox::Acting;
        create(store, "inbox").await;
        let message = post(store, "inbox", "alpha", "handled twice", 0).await;
        store
            .mark_processed(&message.id, Some("done"), Acting::by("run-a", at(1)))
            .await
            .expect("the first retirement succeeds");
        let carried = store
            .mark_processed(&message.id, None, Acting::by("run-b", at(2)))
            .await
            .expect("a second retirement succeeds");
        assert_eq!(
            carried.notes.as_deref(),
            Some("done"),
            "the message keeps the notes it had"
        );
        let history = store
            .message_history(&message.id)
            .await
            .expect("the history reads");
        assert_eq!(
            history
                .iter()
                .map(|e| e.notes.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("done"), None],
            "the second call sent no notes, and the history does not say it did: {history:?}"
        );
    }

    /// **A delivery of a box is one event per message it moved, naming the box.**
    /// A leftover handed over again moved nothing and records nothing.
    pub async fn a_delivery_records_one_read_per_message_it_moved(store: &dyn Mailboxes) {
        use crate::mailbox::{Acting, MessageEventKind};
        create(store, "inbox").await;
        let first = post(store, "inbox", "alpha", "one", 0).await;
        let second = post(store, "inbox", "alpha", "two", 1).await;
        let delivery = store
            .read_mailbox(&name("inbox"), TakenBy::Reading, Acting::by("run-a", at(5)))
            .await
            .expect("the box reads")
            .written()
            .expect("the guard waves it through");
        assert_eq!(delivery.messages.len(), 2);
        store
            .read_mailbox(&name("inbox"), TakenBy::Reading, Acting::by("run-b", at(6)))
            .await
            .expect("the box reads again");
        for id in [&first.id, &second.id] {
            let history = store.message_history(id).await.expect("the history reads");
            assert_eq!(
                history.len(),
                1,
                "one move to read, and the second delivery moved nothing: {history:?}"
            );
            assert_eq!(history[0].kind, MessageEventKind::Read);
            assert_eq!(history[0].by_session.as_deref(), Some("run-a"));
            assert_eq!(history[0].by_box, Some(name("inbox")));
            assert_eq!(history[0].at, at(5));
        }
    }

    /// **A quarantine keeps the state the message was set aside from**, which the
    /// message no longer says, and the reason and the run that decided. The
    /// history of a quarantined message is answered, though a delivery of it is
    /// refused.
    pub async fn a_quarantine_keeps_the_state_the_message_was_set_aside_from(
        store: &dyn Mailboxes,
    ) {
        use crate::mailbox::{Acting, MessageEventKind};
        create(store, "inbox").await;
        let message = post(store, "inbox", "alpha", "the count cannot be trusted", 0).await;
        read(store, "inbox").await;
        store
            .quarantine(
                &message.id,
                &name("inbox"),
                "the count cannot be trusted",
                Acting::by("run-a", at(7)),
            )
            .await
            .expect("quarantining your own mail succeeds");

        let history = store
            .message_history(&message.id)
            .await
            .expect("a quarantined message still has a history");
        assert_eq!(
            history.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![MessageEventKind::Read, MessageEventKind::Quarantined],
            "{history:?}"
        );
        let set_aside = &history[1];
        assert_eq!(set_aside.prior_state, MessageState::Read);
        assert_eq!(
            set_aside.reason.as_deref(),
            Some("the count cannot be trusted")
        );
        assert_eq!(set_aside.by_session.as_deref(), Some("run-a"));
        assert_eq!(set_aside.by_box, Some(name("inbox")));
        assert_eq!(set_aside.at, at(7));
        assert_eq!(set_aside.notes, None);
    }

    /// An id that names nothing has no history, and says so.
    pub async fn the_history_of_an_unknown_message_is_a_miss(store: &dyn Mailboxes) {
        let unknown = store
            .message_history(&MessageId("9999999".into()))
            .await
            .expect_err("a message that is not there has no history to read");
        assert!(
            matches!(unknown, MailboxError::UnknownMessage { .. }),
            "got {unknown:?}"
        );
    }

    /// **One scan lists the events of every message that has any, each as its
    /// own history reads.** A message nothing has changed is not listed, and the
    /// one that was read and then processed has both events in order.
    pub async fn a_scan_of_events_lists_each_changed_message_as_its_history_does(
        store: &dyn Mailboxes,
    ) {
        use crate::mailbox::{Acting, MessageEventKind};
        create(store, "inbox").await;
        let first = post(store, "inbox", "alpha", "one", 0).await;
        let second = post(store, "inbox", "alpha", "two", 1).await;
        read(store, "inbox").await;
        store
            .mark_processed(&first.id, Some("done"), Acting::by("run-a", at(9)))
            .await
            .expect("the message is processed");
        let untouched = post(store, "inbox", "alpha", "three", 2).await;

        let scanned = store.scan_message_events().await.expect("the events scan");

        let mut listed: Vec<&MessageId> = scanned.iter().map(|(id, _)| id).collect();
        listed.sort();
        let mut changed = vec![&first.id, &second.id];
        changed.sort();
        assert_eq!(
            listed, changed,
            "the message nothing changed has no entry: {scanned:?}"
        );
        assert!(!listed.contains(&&untouched.id));
        for (id, events) in &scanned {
            assert_eq!(
                events,
                &store.message_history(id).await.expect("the history reads"),
                "the scan and the history of {id:?} disagree"
            );
        }
        let (_, of_first) = scanned
            .iter()
            .find(|(id, _)| id == &first.id)
            .expect("the processed message is listed");
        assert_eq!(
            of_first.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![MessageEventKind::Read, MessageEventKind::Processed]
        );
    }

    pub async fn run_all<S, F, Fut>(fresh: F) -> ()
    where
        S: Mailboxes,
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = S>,
    {
        create_then_list(&fresh().await).await;
        repoint_owner_moves_the_box_and_its_mail(&fresh().await).await;
        a_repoint_refuses_when_the_owner_holds_more_than_one_box(&fresh().await).await;
        a_repoint_refuses_a_destination_name_a_different_owner_already_holds(&fresh().await).await;
        a_box_carries_its_owner_onto_the_board(&fresh().await).await;
        a_box_owned_by_a_person_is_private_on_the_board(&fresh().await).await;
        a_persons_box_keeps_a_persons_name_when_it_follows_its_owner(&fresh().await).await;
        a_box_for_an_owner_nobody_knows_is_refused(&fresh().await).await;
        creating_a_near_miss_is_blocked_and_writes_nothing(&fresh().await).await;
        a_confirmed_near_miss_creates_the_sibling_box(&fresh().await).await;
        a_posted_message_lands_in_new(&fresh().await).await;
        a_subject_rides_with_the_message(&fresh().await).await;
        a_subject_the_store_rewrites_still_posts(&fresh().await).await;
        a_blank_subject_is_absent_and_a_broken_one_is_refused(&fresh().await).await;
        read_message_takes_one_and_leaves_the_rest(&fresh().await).await;
        read_message_leaves_a_processed_message_terminal(&fresh().await).await;
        reading_an_unknown_message_is_a_miss(&fresh().await).await;
        a_scan_sees_every_box_and_every_state(&fresh().await).await;
        a_message_is_found_by_its_id_from_any_box_and_state(&fresh().await).await;
        several_messages_are_found_by_their_ids_in_scan_order(&fresh().await).await;
        a_senders_mail_is_found_by_sender_across_boxes_and_states(&fresh().await).await;
        a_body_survives_the_round_trip(&fresh().await).await;
        a_crlf_body_normalizes_to_plain_newlines(&fresh().await).await;
        a_body_of_markup_and_a_loose_fence_survives(&fresh().await).await;
        senders_mail_waiting_at_send_survives_the_round_trip(&fresh().await).await;
        posting_into_an_unknown_mailbox_is_blocked(&fresh().await).await;
        a_read_delivers_everything_new_and_moves_the_column(&fresh().await).await;
        a_delivery_orders_inside_one_second(&fresh().await).await;
        a_second_read_redelivers_leftovers_flagged(&fresh().await).await;
        mark_processed_is_terminal_and_records_the_outcome(&fresh().await).await;
        a_deliberate_quarantine_stops_serving_the_message_and_names_why(&fresh().await).await;
        quarantining_another_boxs_message_is_refused(&fresh().await).await;
        a_failure_is_recorded_as_an_outcome(&fresh().await).await;
        a_reply_names_the_message_it_answers(&fresh().await).await;
        a_reply_can_answer_a_message_in_another_box(&fresh().await).await;
        a_reply_to_an_unknown_message_is_refused(&fresh().await).await;
        long_notes_are_kept_as_far_as_they_fit(&fresh().await).await;
        notes_that_fit_are_untouched(&fresh().await).await;
        processing_without_notes_is_allowed(&fresh().await).await;
        a_new_message_can_be_processed_without_a_read(&fresh().await).await;
        boxes_do_not_leak_into_each_other(&fresh().await).await;
        reading_an_unknown_mailbox_is_blocked(&fresh().await).await;
        processing_an_unknown_message_is_a_miss(&fresh().await).await;
        malformed_input_is_refused(&fresh().await).await;
        a_delivery_records_how_it_was_taken(&fresh().await).await;
        a_delivery_skips_processed_mail_and_hands_over_what_is_owed(&fresh().await).await;
        an_unread_message_that_was_replaced_is_listed_apart_not_delivered(&fresh().await).await;
        a_message_whose_replacement_was_set_aside_is_delivered(&fresh().await).await;
        a_replaced_message_the_reader_already_has_is_still_delivered(&fresh().await).await;
        a_replaced_message_is_readable_by_id_on_request(&fresh().await).await;
        a_replacement_is_refused_unless_it_is_the_senders_own_in_the_same_box(&fresh().await).await;
        simultaneous_replacements_of_one_message_land_exactly_one(&fresh().await).await;
        a_messages_history_keeps_each_change_of_state_in_order(&fresh().await).await;
        a_retirement_with_no_notes_is_recorded_with_none(&fresh().await).await;
        a_delivery_records_one_read_per_message_it_moved(&fresh().await).await;
        a_quarantine_keeps_the_state_the_message_was_set_aside_from(&fresh().await).await;
        the_history_of_an_unknown_message_is_a_miss(&fresh().await).await;
        a_scan_of_events_lists_each_changed_message_as_its_history_does(&fresh().await).await;
    }
}
