//! The Mailboxes contract, and the in-memory fake that must satisfy it.
//!
//! One behavioural spec, three tiers: the fake here (milliseconds), the real
//! Vikunja adapter over an in-memory API double (fast, no network), and the real
//! adapter against real Vikunja (gated). **The spec is the same code in all
//! three**, which is what stops the fake from drifting into a store that agrees
//! with the tests and disagrees with reality.
//!
//! Behind the `testing` feature, so it compiles for tests here and in downstream
//! crates but never ships in a production binary.

use std::sync::Mutex;

use jiff::Timestamp;

use super::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, MessageState, NewMessage, StateCounts, guard,
    normalize_body, normalize_notes, validate_body, validate_mailbox_name, validate_message_id,
    validate_notes, validate_sender,
};

/// The in-memory [`Mailboxes`] fake — a real store that holds a write, with no
/// network. Deterministic: ids are a monotonic counter, never a clock.
#[derive(Default)]
pub struct InMemoryMailboxes {
    boxes: Mutex<Vec<MailboxName>>,
    messages: Mutex<Vec<Message>>,
    next_id: Mutex<u64>,
}

impl InMemoryMailboxes {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
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
}

#[async_trait::async_trait]
impl Mailboxes for InMemoryMailboxes {
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        create_new: bool,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        validate_mailbox_name(name)?;
        let mut boxes = self.boxes.lock().expect("mailbox lock");
        if let guard::Decision::Block(candidates) = guard::decide_create(name, &boxes, create_new) {
            return Ok(Guarded::Blocked {
                attempted: name.clone(),
                candidates,
            });
        }
        boxes.push(name.clone());
        Ok(Guarded::Written(Mailbox {
            name: name.clone(),
            counts: StateCounts::default(),
            quarantined: Vec::new(),
        }))
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        let messages = self.messages.lock().expect("message lock");
        Ok(self
            .names()
            .into_iter()
            .map(|name| {
                let mut counts = StateCounts::default();
                for message in messages.iter().filter(|m| m.mailbox == name) {
                    counts.add(message.state);
                }
                // The fake can hold nothing unreadable: every message in it
                // passed validation on the way in.
                Mailbox {
                    name,
                    counts,
                    quarantined: Vec::new(),
                }
            })
            .collect())
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        validate_mailbox_name(&message.mailbox)?;
        validate_sender(&message.sender)?;
        validate_body(&message.body)?;

        let names = self.names();
        if let guard::Decision::Block(candidates) = guard::decide_existing(&message.mailbox, &names)
        {
            return Ok(Guarded::Blocked {
                attempted: message.mailbox,
                candidates,
            });
        }

        let stored = Message {
            id: self.mint_id(),
            mailbox: message.mailbox,
            body: normalize_body(&message.body),
            sender: message.sender.trim().to_string(),
            sent_at: message.sent_at,
            state: MessageState::New,
            notes: None,
        };
        self.messages.lock().expect("message lock").push(stored.clone());
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

        let mut messages = self.messages.lock().expect("message lock");
        let mut delivered: Vec<Delivered> = messages
            .iter_mut()
            .filter(|m| &m.mailbox == name && m.state.is_unprocessed())
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

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        validate_message_id(id)?;
        validate_notes(notes)?;

        let mut messages = self.messages.lock().expect("message lock");
        let message = messages
            .iter_mut()
            .find(|m| &m.id == id)
            .ok_or_else(|| MailboxError::UnknownMessage {
                attempted: id.to_string(),
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

    /// Create a box, asserting the guard waved it through.
    pub async fn create(store: &dyn Mailboxes, n: &str) -> Mailbox {
        store
            .create_mailbox(&name(n), false)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block creating '{n}'"))
    }

    /// Post a message, asserting the guard waved it through.
    pub async fn post(store: &dyn Mailboxes, mailbox: &str, sender: &str, body: &str, at_offset: i64) -> Message {
        store
            .post_message(NewMessage {
                mailbox: name(mailbox),
                body: body.to_string(),
                sender: sender.to_string(),
                sent_at: at(at_offset),
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

        let Guarded::Blocked { attempted, candidates } = store
            .create_mailbox(&name("inbx"), false)
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
            .create_mailbox(&name("worker-2"), false)
            .await
            .expect("a blocked create is a result, not a failure")
        else {
            panic!("without the signal, a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "worker-1");

        let created = store
            .create_mailbox(&name("worker-2"), true)
            .await
            .expect("create_mailbox should succeed")
            .written()
            .expect("create_new must override the near-miss screen");
        assert_eq!(created.name.as_str(), "worker-2");

        let Guarded::Blocked { candidates, .. } = store
            .create_mailbox(&name("worker-1"), true)
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
        assert!(!posted.id.as_str().is_empty(), "the store mints an id");

        let counts = counts(store, "inbox").await.expect("the box is on the board");
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
        assert_eq!(delivered.messages[0].message.body, body, "…and on the way back out");
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
        assert_eq!(stacked.body, "line one\nline two", "normalized to a fixpoint");
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
        assert_eq!(delivered.messages[0].message.body, body, "…and on the way back out");
    }

    /// **A typo must never silently mint a box.** Posting into a name jojobot
    /// doesn't know comes back blocked, with the box it suspects — and creates
    /// nothing.
    pub async fn posting_into_an_unknown_mailbox_is_blocked(store: &dyn Mailboxes) {
        create(store, "inbox").await;

        let Guarded::Blocked { attempted, candidates } = store
            .post_message(NewMessage {
                mailbox: name("inbx"),
                body: "the shipment landed".into(),
                sender: "alpha".into(),
                sent_at: at(0),
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
        let bodies: Vec<&str> = delivery.messages.iter().map(|d| d.message.body.as_str()).collect();
        assert_eq!(bodies, vec!["first", "second"], "oldest first");
        assert!(
            delivery.messages.iter().all(|d| !d.seen_before),
            "a first delivery is nobody's leftover"
        );
        assert!(
            delivery.messages.iter().all(|d| d.message.state == MessageState::Read),
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
        let bodies: Vec<&str> = again.messages.iter().map(|d| d.message.body.as_str()).collect();
        assert_eq!(
            bodies,
            vec!["earlier", "first", "later"],
            "oldest first spans the columns: a leftover is not automatically first"
        );

        let leftovers: Vec<&str> = again
            .leftovers()
            .map(|d| d.message.body.as_str())
            .collect();
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
        assert_eq!(processed.body, posted.body, "processing does not rewrite the message");
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
        assert!(processed.notes.as_deref().is_some_and(|n| n.contains("FAILED")));
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
        let bodies: Vec<&str> = delivery.messages.iter().map(|d| d.message.body.as_str()).collect();
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
        let Guarded::Blocked { attempted, candidates } = store
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
            store.create_mailbox(&name("Inbox"), false).await.is_err(),
            "a name outside the grammar is refused"
        );
        create(store, "inbox").await;

        let bad_body = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "   ".into(),
                sender: "alpha".into(),
                sent_at: at(0),
            })
            .await;
        assert!(bad_body.is_err(), "an empty body is not a message");

        let bad_sender = store
            .post_message(NewMessage {
                mailbox: name("inbox"),
                body: "the shipment landed".into(),
                sender: "  ".into(),
                sent_at: at(0),
            })
            .await;
        assert!(bad_sender.is_err(), "a message with no sender has no provenance");

        assert_eq!(
            counts(store, "inbox").await.expect("inbox exists").total(),
            0,
            "nothing malformed reached the board"
        );
    }

    /// The whole spec, against one store. Each case runs on a **fresh** store,
    /// so nothing here depends on the order the others ran in.
    pub async fn run_all<S: Mailboxes, F: Fn() -> S>(fresh: F) {
        create_then_list(&fresh()).await;
        creating_a_near_miss_is_blocked_and_writes_nothing(&fresh()).await;
        a_confirmed_near_miss_creates_the_sibling_box(&fresh()).await;
        a_posted_message_lands_in_new(&fresh()).await;
        a_body_survives_the_round_trip(&fresh()).await;
        a_crlf_body_normalizes_to_plain_newlines(&fresh()).await;
        a_body_of_markup_and_a_loose_fence_survives(&fresh()).await;
        posting_into_an_unknown_mailbox_is_blocked(&fresh()).await;
        a_read_delivers_everything_new_and_moves_the_column(&fresh()).await;
        a_second_read_redelivers_leftovers_flagged(&fresh()).await;
        mark_processed_is_terminal_and_records_the_outcome(&fresh()).await;
        a_failure_is_recorded_as_an_outcome(&fresh()).await;
        processing_without_notes_is_allowed(&fresh()).await;
        a_new_message_can_be_processed_without_a_read(&fresh()).await;
        boxes_do_not_leak_into_each_other(&fresh()).await;
        reading_an_unknown_mailbox_is_blocked(&fresh()).await;
        processing_an_unknown_message_is_a_miss(&fresh()).await;
        malformed_input_is_refused(&fresh()).await;
    }
}
