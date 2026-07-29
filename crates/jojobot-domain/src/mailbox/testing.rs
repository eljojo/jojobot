//! The Mailboxes contract, and the in-memory fake that must satisfy it.
//!
//! One behavioural spec, three tiers: the fake here (milliseconds), the real
//! Outline adapter over an in-memory API double (fast, no network), and the real
//! adapter against real Outline (gated). **The spec is the same code in all
//! three**, which is what stops the fake from drifting into a store that agrees
//! with the tests and disagrees with reality.
//!
//! Behind the `testing` feature, so it compiles for tests here and in downstream
//! crates but never ships in a production binary.

use std::sync::Mutex;

use jiff::Timestamp;

use crate::memory::{EntityId, guard as memory_guard};

use super::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NOTES_BUDGET, NewMessage, StateCounts, guard, normalize_body,
    normalize_notes, normalize_subject, validate_body, validate_mailbox_name, validate_message_id,
    validate_notes, validate_sender, validate_subject,
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
    quarantined: Mutex<Vec<(MailboxName, MessageId, String)>>,
}

impl InMemoryMailboxes {
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

    /// Put a card into quarantine, as a hand edit on a real board would.
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
    pub fn quarantine(&self, mailbox: &MailboxName, card: &MessageId, reason: &str) {
        self.quarantined.lock().expect("quarantine lock").push((
            mailbox.clone(),
            card.clone(),
            reason.to_string(),
        ));
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

    /// The refusal every verb that addresses a card by id owes: a quarantined
    /// id is on the board and cannot be read, which is a different answer from
    /// "no such message" and has to stay one.
    fn refuse_if_quarantined(&self, id: &MessageId) -> Result<(), MailboxError> {
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
        create_new: bool,
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
                let index: Vec<crate::memory::Entity> = Vec::new();
                return Ok(Guarded::UnknownOwner {
                    attempted: owner.clone(),
                    candidates: memory_guard::screen(owner, &[], &index),
                });
            }
        }

        let mut boxes = self.boxes.lock().expect("mailbox lock");
        if let guard::Decision::Block(candidates) = guard::decide_create(name, &boxes, create_new) {
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
        };
        self.messages
            .lock()
            .expect("message lock")
            .push(stored.clone());
        Ok(Guarded::Written(stored))
    }

    async fn read_mailbox(&self, name: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
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
        let mut delivered: Vec<Delivered> = messages
            .iter_mut()
            .filter(|m| {
                &m.mailbox == name && m.state.is_unprocessed() && !quarantined.contains(&m.id)
            })
            .map(|m| {
                let seen_before = m.state == MessageState::Read;
                m.state = MessageState::Read;
                Delivered {
                    message: m.clone(),
                    seen_before,
                }
            })
            .collect();
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
        }))
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        let quarantined = self.quarantined.lock().expect("quarantine lock");
        Ok(self
            .messages
            .lock()
            .expect("message lock")
            .iter()
            .filter(|m| !quarantined.iter().any(|(_, card, _)| card == &m.id))
            .cloned()
            .collect())
    }

    async fn read_message(&self, id: &MessageId) -> Result<Delivered, MailboxError> {
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
        }
        Ok(Delivered {
            message: message.clone(),
            seen_before,
        })
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
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
        message.state = MessageState::Processed;
        if let Some(notes) = normalize_notes(notes) {
            message.notes = Some(notes);
        }
        Ok(message.clone())
    }
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
    pub const OWNERS: &[&str] = &["bot:gamma", "bot:delta"];

    /// An owner from [`OWNERS`] — the one this suite files boxes under unless a
    /// case is about ownership itself.
    fn owner() -> EntityId {
        EntityId(OWNERS[0].to_string())
    }

    /// Create a box, asserting the guard waved it through.
    pub async fn create(store: &dyn Mailboxes, n: &str) -> Mailbox {
        store
            .create_mailbox(&name(n), &owner(), false)
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
            })
            .await
            .expect("post_message should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block posting to '{mailbox}'"))
    }

    /// Read a box, asserting the guard waved it through.
    pub async fn read(store: &dyn Mailboxes, mailbox: &str) -> Delivery {
        store
            .read_mailbox(&name(mailbox))
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
            .create_mailbox(&name("inbx"), &owner(), false)
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
    /// `worker-1` blocks as a near miss until the caller passes `create_new`,
    /// which overrides the similarity screen. An exact name stays blocked
    /// regardless: that box already exists.
    pub async fn a_confirmed_near_miss_creates_the_sibling_box(store: &dyn Mailboxes) {
        create(store, "worker-1").await;

        let Guarded::Blocked { candidates, .. } = store
            .create_mailbox(&name("worker-2"), &owner(), false)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("without the signal, a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "worker-1");

        let created = store
            .create_mailbox(&name("worker-2"), &owner(), true)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("create_new must override the near-miss screen");
        assert_eq!(created.name.as_str(), "worker-2");

        let Guarded::Blocked { candidates, .. } = store
            .create_mailbox(&name("worker-1"), &owner(), true)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("an exact name stays blocked, create_new or not: the box exists");
        };
        assert_eq!(candidates[0].reason, guard::MatchReason::Exact);
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

        let processed = store.mark_processed(&posted.id, None).await.expect("ok");
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
            .read_message(&wanted.id)
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
            .read_message(&wanted.id)
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
            .mark_processed(&posted.id, Some("filed under shipments"))
            .await
            .expect("mark_processed ok");

        let delivered = store
            .read_message(&posted.id)
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
            .read_message(&taken.id)
            .await
            .expect("read_message ok");
        store
            .mark_processed(&done.id, Some("filed"))
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
            !store.read_message(&fresh.id).await.expect("ok").seen_before,
            "the scan did not take delivery of anything"
        );
    }

    /// An id nothing answers to is a miss here for the same reason it is one
    /// for `mark_processed` — and it is the same answer, so one client branch
    /// handles both.
    pub async fn reading_an_unknown_message_is_a_miss(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let err = store
            .read_message(&MessageId("999999".into()))
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
            .mark_processed(&posted.id, Some("filed under shipments"))
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
            .mark_processed(&posted.id, Some("FAILED: the loading dock was closed"))
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
            })
            .await
            .expect("post ok")
            .written()
            .expect("a reply to a message that exists is written");
        assert_eq!(reply.in_reply_to.as_ref(), Some(&original.id));

        // …and it survives the round trip, which is the whole point of a link
        // nobody is going to re-derive from prose later.
        let seen = store
            .read_message(&reply.id)
            .await
            .expect("read_message ok");
        assert_eq!(seen.message.in_reply_to.as_ref(), Some(&original.id));

        // The message it answers is untouched — a reply is not a delivery.
        let original_now = store.read_message(&original.id).await.expect("read ok");
        assert_eq!(original_now.message.in_reply_to, None);
    }

    /// **A reply answers ACROSS boxes, which is the shape it will actually run
    /// in.** A hand-off is left in one box and its report goes back in another;
    /// a reply into the same box is the easy case and not the real one. It runs
    /// on every tier because this is precisely where a fake and a store can
    /// quietly disagree: the real adapter scopes its reads to the mailbox
    /// PROJECT rather than the box, and if that ever narrowed, a fake checking
    /// globally would stay green while production stopped linking.
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
            .read_message(&report.id)
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
            .mark_processed(&posted.id, Some(&long))
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
            .read_message(&posted.id)
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
            .mark_processed(&posted.id, Some(&notes))
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
        let processed = store.mark_processed(&posted.id, None).await.expect("ok");
        assert_eq!(processed.notes, None);
        assert_eq!(processed.state, MessageState::Processed);
    }

    /// **A message can be processed straight out of `new`.** A consumer that
    /// acts on something it heard about another way must not have to fake a
    /// read first, and the column still ends up terminal.
    pub async fn a_new_message_can_be_processed_without_a_read(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let posted = post(store, "inbox", "alpha", "the shipment landed", 0).await;
        let processed = store.mark_processed(&posted.id, None).await.expect("ok");
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
            .read_mailbox(&name("inbx"))
            .await
            .expect("a blocked read is a result, not a failure")
        else {
            panic!("reading an unknown box must block, not come back empty");
        };
        assert_eq!(attempted.as_str(), "inbx");
        assert_eq!(candidates[0].name.as_str(), "inbox");
    }

    /// An id nothing answers to is a miss — never a create, never a silent
    /// success.
    pub async fn processing_an_unknown_message_is_a_miss(store: &dyn Mailboxes) {
        create(store, "inbox").await;
        let err = store
            .mark_processed(&MessageId("999999".into()), None)
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
                .create_mailbox(&name("Inbox"), &owner(), false)
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
    /// not need this; the Outline adapter does, and running this suite against
    /// it is the point of the shape.
    pub async fn run_all<S, F, Fut>(fresh: F) -> ()
    where
        S: Mailboxes,
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = S>,
    {
        create_then_list(&fresh().await).await;
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
        a_body_survives_the_round_trip(&fresh().await).await;
        a_crlf_body_normalizes_to_plain_newlines(&fresh().await).await;
        a_body_of_markup_and_a_loose_fence_survives(&fresh().await).await;
        posting_into_an_unknown_mailbox_is_blocked(&fresh().await).await;
        a_read_delivers_everything_new_and_moves_the_column(&fresh().await).await;
        a_second_read_redelivers_leftovers_flagged(&fresh().await).await;
        mark_processed_is_terminal_and_records_the_outcome(&fresh().await).await;
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
    }
}
