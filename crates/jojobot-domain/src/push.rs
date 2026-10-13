//! Pushes — a message that lands in a person's box reaches their phone.
//!
//! A **bounded context beside Mailboxes**, in a table of its own: the mail rail
//! stores messages and this stores the intent to tell somebody about one. The two
//! share exactly one thing, a [`MessageId`].
//!
//! Three rules shape the whole context:
//!
//! * **The intent is written with the message.** A store that accepts a post to a
//!   person's box also writes one row here, in the same transaction, so a restart
//!   between the two loses nothing and a crash cannot leave a message with no
//!   intent beside it.
//! * **Delivery never touches the post.** Posting writes a row and returns. The
//!   row is drained afterwards, by whoever nudges: a failed delivery is a row
//!   that stays, and posting never waits on it or fails because of it.
//! * **A push is retried for a bounded window and then given up on, aloud.** There
//!   is no poller. A row is looked at when a post arrives or when its own retry
//!   falls due, and a row older than the window is abandoned and named.
//!
//! The row holds no text. What is pushed is read from the message when it is sent,
//! through the same port every other reader uses, so a handle written into a body
//! reads as the handle it answers to now.
//!
//! **This is user-agnostic software: no user PII, fixtures included.**

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;

use crate::mailbox::{
    Delivered, Delivery, Guarded, MailboxError, MailboxName, Mailboxes, Message, MessageId,
    NewMessage, Quarantined, TakenBy,
};
use crate::memory::EntityId;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// **One push waiting to go out.** The row the outbox keeps for a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    /// The message this tells somebody about.
    pub message: MessageId,
    /// When the intent was written. The window is counted from here.
    pub enqueued_at: Timestamp,
    /// How many deliveries have failed so far.
    pub attempts: u32,
    /// The first moment this may be tried again.
    pub next_attempt_at: Timestamp,
}

/// Why the outbox could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    /// The store behind the outbox failed.
    #[error("the push outbox could not be reached: {0}")]
    Store(String),
}

/// **The durable list of pushes not yet delivered.**
///
/// Written by the mailbox store itself, in the transaction that accepts the post;
/// this port is only the reading side and the three ways a row ends.
#[async_trait]
pub trait Outbox: Send + Sync {
    /// Every pending push whose `next_attempt_at` is at or before `now`, oldest
    /// first.
    async fn due(&self, now: Timestamp) -> Result<Vec<Push>, PushError>;

    /// The earliest `next_attempt_at` among the pending pushes, or `None` when
    /// nothing is pending. What a runner sleeps until.
    async fn next_due(&self) -> Result<Option<Timestamp>, PushError>;

    /// The push went out. The row stays, marked sent, and is never due again.
    async fn sent(&self, message: &MessageId, at: Timestamp) -> Result<(), PushError>;

    /// The delivery failed: this is the `attempts`-th failure, and the next try
    /// is no sooner than `next_attempt_at`.
    async fn failed(
        &self,
        message: &MessageId,
        attempts: u32,
        next_attempt_at: Timestamp,
    ) -> Result<(), PushError>;

    /// The push will not be tried again. The row stays, marked abandoned.
    async fn abandoned(&self, message: &MessageId, at: Timestamp) -> Result<(), PushError>;
}

/// **The one thing a post does about a push once its row is written**: tell
/// whoever delivers that there is something to look at. It returns at once and
/// never fails, because posting must not wait on a push or fail because of one.
pub trait Nudge: Send + Sync {
    /// Something was written to the outbox; look at it.
    fn nudge(&self);
}

/// **Wakes the push after a post lands in a person's box, whichever caller
/// posted.** Every writer reaches the box through the port this wraps, so the
/// nudge is made once, where the writes go through, rather than beside each
/// caller that happens to post to the operator. A path added later cannot miss
/// it.
///
/// A post into a bot's box, a post the store blocked and a post that failed
/// nudge nothing. When the box's owner cannot be read the push is nudged
/// anyway: a wake with nothing to send costs a look at the outbox, and a missed
/// wake leaves a code to expire unannounced.
pub struct Nudging {
    inner: Arc<dyn Mailboxes>,
    push: Arc<dyn Nudge>,
}

impl Nudging {
    /// Wrap `inner` so a written post into a person's box tells `push`.
    pub fn new(inner: Arc<dyn Mailboxes>, push: Arc<dyn Nudge>) -> Self {
        Nudging { inner, push }
    }

    /// Whether the box named is a person's.
    async fn is_a_persons(&self, name: &MailboxName) -> bool {
        match self.inner.list_mailboxes().await {
            Ok(boxes) => boxes
                .iter()
                .any(|held| &held.name == name && held.is_private()),
            Err(_) => true,
        }
    }
}

#[async_trait]
impl Mailboxes for Nudging {
    /// **Forwarded**: the count of the port beneath is the one that matters.
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }

    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<crate::mailbox::Mailbox>, MailboxError> {
        self.inner.create_mailbox(name, owner, override_token).await
    }

    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<crate::mailbox::Mailbox>, MailboxError> {
        self.inner.repoint_owner(from, to).await
    }

    async fn list_mailboxes(&self) -> Result<Vec<crate::mailbox::Mailbox>, MailboxError> {
        self.inner.list_mailboxes().await
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.inner.scan_messages().await
    }

    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        self.inner.message_by_id(id).await
    }

    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.inner.sent_by(senders).await
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        let to = message.mailbox.clone();
        let posted = self.inner.post_message(message).await?;
        if matches!(posted, Guarded::Written(_)) && self.is_a_persons(&to).await {
            self.push.nudge();
        }
        Ok(posted)
    }

    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        self.inner.read_mailbox(name, taken_by, acting).await
    }

    async fn read_message(
        &self,
        id: &MessageId,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        self.inner.read_message(id, acting).await
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        self.inner.mark_processed(id, notes, acting).await
    }

    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: crate::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        self.inner.quarantine(id, by, reason, acting).await
    }

    async fn message_history(
        &self,
        id: &crate::mailbox::MessageId,
    ) -> Result<Vec<crate::mailbox::MessageEvent>, crate::mailbox::MailboxError> {
        self.inner.message_history(id).await
    }
}

/// **What is sent**, composed from a message and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// The subject, or the sender's handle when the message has none.
    pub title: String,
    /// The sender on the first line, a blank line, then the body.
    pub body: String,
}

impl Notification {
    /// The notification a message becomes.
    pub fn of(message: &Message) -> Self {
        let sender = message.sender.trim();
        let title = match message.subject.as_deref().map(str::trim) {
            Some(subject) if !subject.is_empty() => subject.to_string(),
            _ => sender.to_string(),
        };
        Notification {
            title,
            body: format!("From {sender}\n\n{}", message.body),
        }
    }
}

/// Why a delivery did not go out.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Undelivered(pub String);

/// **The service that carries a notification to the phone.** One method, so the
/// service is a detail of an adapter and the policy below never names it.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Deliver one notification, or say why it did not go.
    async fn deliver(&self, notification: &Notification) -> Result<(), Undelivered>;
}

/// **How long a push is chased.** Counted from the moment the intent was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// The wait after the first failure. Each failure doubles it.
    pub first_delay: Duration,
    /// The longest a single wait grows to.
    pub longest_delay: Duration,
    /// How long after the intent was written the last try is made. A push that
    /// has not gone out by then is abandoned.
    pub window: Duration,
}

impl Policy {
    /// The wait after the `attempts`-th failure.
    pub fn delay_after(&self, attempts: u32) -> Duration {
        let doublings = attempts.saturating_sub(1).min(32);
        self.first_delay
            .saturating_mul(2u32.saturating_pow(doublings))
            .min(self.longest_delay)
    }
}

impl Default for Policy {
    /// Ten seconds, doubling to five minutes, for one hour.
    fn default() -> Self {
        Policy {
            first_delay: Duration::from_secs(10),
            longest_delay: Duration::from_secs(300),
            window: Duration::from_secs(3600),
        }
    }
}

/// **What one pass over the outbox did.**
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Pushes that went out in this pass.
    pub sent: Vec<MessageId>,
    /// Pushes given up on in this pass, each with the reason it is not coming.
    pub abandoned: Vec<(MessageId, String)>,
    /// Pushes whose outcome the outbox could not record, each with the store's
    /// error. The row stays due, so the next pass tries the push again.
    pub unrecorded: Vec<(MessageId, String)>,
    /// The earliest moment a pending push is next due, or `None` when nothing
    /// is pending. A runner that sleeps until this moment never polls.
    pub next_due: Option<Timestamp>,
}

/// **The policy of delivering pushes**, over the ports it needs.
pub struct Pushing<'a> {
    /// Where the pending pushes are kept.
    pub outbox: &'a dyn Outbox,
    /// Where the messages they tell about are read.
    pub mailboxes: &'a dyn Mailboxes,
    /// The service that carries them.
    pub notifier: &'a dyn Notifier,
    /// How long a push is chased.
    pub policy: Policy,
}

impl Pushing<'_> {
    /// **Try every push that is due as of `now`, once each.**
    ///
    /// A push that goes out is marked sent. One that fails is rescheduled by the
    /// policy, or abandoned when `now` has reached the end of its window. One whose
    /// message cannot be read is abandoned at once with that reason: there is
    /// nothing to say, so nothing is retried. A push whose outcome the outbox
    /// cannot record is listed in [`Drained::unrecorded`] and the pass goes on;
    /// only a store that cannot list the due pushes fails the pass.
    pub async fn drain(&self, now: Timestamp) -> Result<Drained, PushError> {
        let mut drained = Drained::default();
        for push in self.outbox.due(now).await? {
            // **One row the store cannot record does not end the pass.** The row
            // stays due and is tried again, and the rows behind it still go out.
            if let Err(e) = self.settle(&push, now, &mut drained).await {
                drained
                    .unrecorded
                    .push((push.message.clone(), e.to_string()));
            }
        }
        drained.next_due = self.outbox.next_due().await?;
        Ok(drained)
    }

    /// Try one due push and record how it ended.
    async fn settle(
        &self,
        push: &Push,
        now: Timestamp,
        drained: &mut Drained,
    ) -> Result<(), PushError> {
        // **Past its window a push is owed no longer.** The last try is made
        // on the end of the window itself, and a row found after that, as on
        // a restart, is given up on without being sent late.
        let deadline = push
            .enqueued_at
            .checked_add(self.policy.window)
            .unwrap_or(Timestamp::MAX);
        if now > deadline {
            return self
                .give_up(
                    push,
                    now,
                    "the window for this push ran out before it was sent",
                    drained,
                )
                .await;
        }
        let message = match self.mailboxes.message_by_id(&push.message).await {
            Ok(Some(message)) => message,
            Ok(None) => {
                return self
                    .give_up(push, now, "no readable message wears that id", drained)
                    .await;
            }
            Err(e) => {
                // The mail store failing is not the message being gone: it
                // is tried again like any other failure.
                if let Some(reason) = self
                    .fail(push, now, format!("the message could not be read: {e}"))
                    .await?
                {
                    drained.abandoned.push((push.message.clone(), reason));
                }
                return Ok(());
            }
        };
        match self.notifier.deliver(&Notification::of(&message)).await {
            Ok(()) => {
                self.outbox.sent(&push.message, now).await?;
                drained.sent.push(push.message.clone());
            }
            Err(Undelivered(why)) => {
                if let Some(reason) = self.fail(push, now, why).await? {
                    drained.abandoned.push((push.message.clone(), reason));
                }
            }
        }
        Ok(())
    }

    /// Record one failed delivery. `Some(reason)` when the window has run out and
    /// the push was abandoned, `None` when it was scheduled again.
    async fn fail(
        &self,
        push: &Push,
        now: Timestamp,
        why: String,
    ) -> Result<Option<String>, PushError> {
        let deadline = push
            .enqueued_at
            .checked_add(self.policy.window)
            .unwrap_or(Timestamp::MAX);
        if now >= deadline {
            self.outbox.abandoned(&push.message, now).await?;
            return Ok(Some(format!(
                "still undelivered after {} attempts and the window of {:?}: {why}",
                push.attempts + 1,
                self.policy.window
            )));
        }
        let attempts = push.attempts + 1;
        // The last try lands on the deadline itself, so a window of an hour is
        // an hour and not the next doubling past it.
        let next = now
            .checked_add(self.policy.delay_after(attempts))
            .unwrap_or(deadline)
            .min(deadline);
        self.outbox.failed(&push.message, attempts, next).await?;
        Ok(None)
    }

    async fn give_up(
        &self,
        push: &Push,
        now: Timestamp,
        reason: &str,
        drained: &mut Drained,
    ) -> Result<(), PushError> {
        self.outbox.abandoned(&push.message, now).await?;
        drained
            .abandoned
            .push((push.message.clone(), reason.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{FakeNotifier, InMemoryOutbox, Standing};
    use super::*;
    use crate::mailbox::testing::InMemoryMailboxes;
    use crate::mailbox::{Guarded, MailboxName, NewMessage};
    use crate::memory::EntityId;

    const SHORT: Policy = Policy {
        first_delay: Duration::from_secs(10),
        longest_delay: Duration::from_secs(40),
        window: Duration::from_secs(100),
    };

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + seconds).expect("a moment")
    }

    /// A store that pushes, holding the one box a person owns and one a bot owns.
    async fn store() -> InMemoryMailboxes {
        crate::memory::kinds::load_shipped();
        let boxes = InMemoryMailboxes::knowing_any_owner().pushing();
        for (name, owner) in [
            ("person-ned-flanders", "person:ned-flanders"),
            ("epsilon", "bot:epsilon"),
        ] {
            boxes
                .create_mailbox(&MailboxName(name.into()), &EntityId(owner.into()), None)
                .await
                .expect("a box");
        }
        boxes
    }

    /// A nudge that counts, standing in for the task that drains the outbox.
    #[derive(Default)]
    struct Counted(std::sync::atomic::AtomicUsize);

    impl Nudge for Counted {
        fn nudge(&self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl Counted {
        fn count(&self) -> usize {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// **A post wakes the push when it lands in a person's box, and only then.**
    /// Each negative sits beside the positive it depends on: the post into the
    /// person's box is counted first, so a decorator that never nudges and one
    /// that always nudges are each told apart.
    #[tokio::test]
    async fn only_a_post_that_lands_in_a_persons_box_nudges_the_push() {
        let nudges = Arc::new(Counted::default());
        let boxes = Nudging::new(Arc::new(store().await), nudges.clone());
        let new = |to: &str| NewMessage {
            mailbox: MailboxName(to.into()),
            body: "a body".into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: at(0),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        };

        let landed = boxes
            .post_message(new("person-ned-flanders"))
            .await
            .expect("the post is accepted");
        assert!(matches!(landed, Guarded::Written(_)), "{landed:?}");
        assert_eq!(nudges.count(), 1, "a post into a person's box nudges once");

        let to_a_bot = boxes
            .post_message(new("epsilon"))
            .await
            .expect("the post is accepted");
        assert!(matches!(to_a_bot, Guarded::Written(_)), "{to_a_bot:?}");
        assert_eq!(nudges.count(), 1, "a post into a bot's box nudges nothing");

        let to_nobody = boxes.post_message(new("nobody-here")).await;
        assert!(
            !matches!(to_nobody, Ok(Guarded::Written(_))),
            "{to_nobody:?}"
        );
        assert_eq!(nudges.count(), 1, "a post that did not land nudges nothing");
    }

    async fn post(
        boxes: &InMemoryMailboxes,
        to: &str,
        subject: Option<&str>,
        body: &str,
    ) -> MessageId {
        let posted = boxes
            .post_message(NewMessage {
                mailbox: MailboxName(to.into()),
                body: body.into(),
                subject: subject.map(str::to_string),
                sender: "gamma".into(),
                sent_at: at(0),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("the post lands");
        match posted {
            Guarded::Written(message) => message.id,
            other => panic!("the post did not land: {other:?}"),
        }
    }

    fn pushing<'a>(
        outbox: &'a InMemoryOutbox,
        boxes: &'a InMemoryMailboxes,
        notifier: &'a FakeNotifier,
    ) -> Pushing<'a> {
        Pushing {
            outbox,
            mailboxes: boxes,
            notifier,
            policy: SHORT,
        }
    }

    /// **A delivered push the outbox could not mark sent does not starve the
    /// pushes behind it.** The pass goes on to the next row and names the one it
    /// could not record; the next pass, with the store back, delivers it again
    /// and marks it. The positive (the later row is delivered and marked) sits in
    /// the same pass as the negative (the earlier row stays pending).
    #[tokio::test]
    async fn a_push_the_outbox_could_not_mark_sent_does_not_starve_the_ones_behind_it() {
        use super::testing::{Call, FlakyOutbox};
        let boxes = store().await;
        let rows = boxes.outbox();
        let first = post(&boxes, "person-ned-flanders", Some("first"), "one").await;
        let second = post(&boxes, "person-ned-flanders", Some("second"), "two").await;
        let flaky = FlakyOutbox::over(rows.clone());
        flaky.fail_next(Call::Sent, 1);
        let notifier = FakeNotifier::new();
        let now = rows.rows()[1].push.enqueued_at;
        let pushing = Pushing {
            outbox: &flaky,
            mailboxes: &boxes,
            notifier: &notifier,
            policy: SHORT,
        };

        let pass = pushing.drain(now).await.expect("the pass answers");
        assert_eq!(pass.sent, vec![second.clone()], "{pass:?}");
        assert_eq!(pass.unrecorded.len(), 1, "{pass:?}");
        assert_eq!(pass.unrecorded[0].0, first, "{pass:?}");
        let standing = |id: &MessageId| {
            rows.rows()
                .into_iter()
                .find(|r| &r.push.message == id)
                .expect("a row")
                .standing
        };
        assert_eq!(standing(&second), Standing::Sent);
        assert_eq!(standing(&first), Standing::Pending);

        let again = pushing.drain(now).await.expect("the next pass answers");
        assert_eq!(again.sent, vec![first.clone()], "{again:?}");
        assert!(again.unrecorded.is_empty(), "{again:?}");
        assert_eq!(standing(&first), Standing::Sent);
    }

    /// **A post to the person's box is delivered once, with the subject for the
    /// title and the sender and the body under it.** And a second pass over the
    /// same outbox delivers nothing, because the row is spent.
    #[tokio::test]
    async fn a_post_to_a_persons_box_is_delivered_once_with_subject_sender_and_body() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        let id = post(
            &boxes,
            "person-ned-flanders",
            Some("the lease"),
            "it is signed",
        )
        .await;
        let now = outbox.rows()[0].push.enqueued_at;

        let first = pushing(&outbox, &boxes, &notifier)
            .drain(now)
            .await
            .unwrap();
        assert_eq!(first.sent, vec![id]);
        assert_eq!(
            notifier.delivered(),
            vec![Notification {
                title: "the lease".into(),
                body: "From gamma\n\nit is signed".into(),
            }],
        );

        let again = pushing(&outbox, &boxes, &notifier)
            .drain(now)
            .await
            .unwrap();
        assert_eq!(again, Drained::default());
        assert_eq!(notifier.delivered().len(), 1, "a spent row goes out once");
    }

    /// **A post to a bot's box writes no intent**, so nothing is delivered. Beside
    /// the case above, which is what stops this passing on a store that never
    /// delivers anything.
    #[tokio::test]
    async fn a_post_to_a_bots_box_is_never_pushed() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        post(&boxes, "epsilon", Some("a note"), "for a colleague").await;
        assert!(outbox.rows().is_empty(), "a bot's box writes no intent");
        let drained = pushing(&outbox, &boxes, &notifier)
            .drain(at(0))
            .await
            .unwrap();
        assert_eq!(drained, Drained::default());
        assert!(notifier.delivered().is_empty());
    }

    /// **A message with no subject is titled with its sender.**
    #[tokio::test]
    async fn a_message_with_no_subject_is_titled_with_its_sender() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        post(&boxes, "person-ned-flanders", None, "no title on this one").await;
        let now = outbox.rows()[0].push.enqueued_at;
        pushing(&outbox, &boxes, &notifier)
            .drain(now)
            .await
            .unwrap();
        assert_eq!(notifier.delivered()[0].title, "gamma");
    }

    /// **The endpoint down and then up delivers once, never twice.** The failure
    /// leaves the row pending and due after the first delay; a pass before then
    /// tries nothing; a pass after it delivers; a later pass delivers nothing.
    #[tokio::test]
    async fn the_endpoint_down_and_then_up_delivers_once() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        notifier.go_down();
        post(
            &boxes,
            "person-ned-flanders",
            Some("the lease"),
            "it is signed",
        )
        .await;
        let start = outbox.rows()[0].push.enqueued_at;

        let down = pushing(&outbox, &boxes, &notifier)
            .drain(start)
            .await
            .unwrap();
        assert!(down.sent.is_empty() && down.abandoned.is_empty());
        let row = &outbox.rows()[0];
        assert_eq!(row.standing, Standing::Pending);
        assert_eq!(row.push.attempts, 1);
        assert_eq!(
            row.push.next_attempt_at,
            start.checked_add(Duration::from_secs(10)).unwrap()
        );
        assert_eq!(down.next_due, Some(row.push.next_attempt_at));

        notifier.come_up();
        let too_soon = pushing(&outbox, &boxes, &notifier)
            .drain(start.checked_add(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
        assert!(
            too_soon.sent.is_empty(),
            "a retry is not tried before it is due"
        );
        assert!(notifier.delivered().is_empty());

        let due = start.checked_add(Duration::from_secs(10)).unwrap();
        let up = pushing(&outbox, &boxes, &notifier)
            .drain(due)
            .await
            .unwrap();
        assert_eq!(up.sent.len(), 1);
        let later = pushing(&outbox, &boxes, &notifier)
            .drain(due.checked_add(Duration::from_secs(600)).unwrap())
            .await
            .unwrap();
        assert!(later.sent.is_empty());
        assert_eq!(notifier.delivered().len(), 1, "never twice");
        assert_eq!(outbox.rows()[0].standing, Standing::Sent);
    }

    /// **A push still undelivered when its window ends is abandoned and named**,
    /// and the message it told about is still in the box.
    #[tokio::test]
    async fn a_push_past_its_window_is_abandoned_and_named() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        notifier.go_down();
        let id = post(
            &boxes,
            "person-ned-flanders",
            Some("the lease"),
            "it is signed",
        )
        .await;
        let start = outbox.rows()[0].push.enqueued_at;

        // Failures at 0, 10, 30, 70: the last try is scheduled on the deadline.
        let mut now = start;
        let mut last = Drained::default();
        for _ in 0..8 {
            last = pushing(&outbox, &boxes, &notifier)
                .drain(now)
                .await
                .unwrap();
            match last.next_due {
                Some(next) => now = next,
                None => break,
            }
        }
        assert_eq!(last.abandoned.len(), 1, "the window ran out: {last:?}");
        assert_eq!(last.abandoned[0].0, id);
        assert!(
            now <= start.checked_add(SHORT.window).unwrap(),
            "no try after the window"
        );
        assert_eq!(outbox.rows()[0].standing, Standing::Abandoned);
        assert!(notifier.delivered().is_empty());
        assert!(
            boxes.message_by_id(&id).await.unwrap().is_some(),
            "the message is still in the box"
        );
        assert_eq!(last.next_due, None, "nothing is left to wake for");
    }

    /// **A row found long after its window, as after a restart, is abandoned on
    /// the first pass** rather than tried one more time.
    #[tokio::test]
    async fn a_row_found_past_its_window_is_abandoned_without_a_last_try_going_out() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        notifier.go_down();
        post(&boxes, "person-ned-flanders", None, "left for a while").await;
        let start = outbox.rows()[0].push.enqueued_at;
        let drained = pushing(&outbox, &boxes, &notifier)
            .drain(start.checked_add(Duration::from_secs(10_000)).unwrap())
            .await
            .unwrap();
        assert_eq!(drained.abandoned.len(), 1);
        assert_eq!(outbox.rows()[0].standing, Standing::Abandoned);
    }

    /// **A row found past its window is not sent late, even when the server is
    /// up.** The window is how long a push is owed; a message that arrives after
    /// it is a surprise and not a notification. The row is abandoned and named.
    #[tokio::test]
    async fn a_row_found_past_its_window_is_not_delivered_late_when_the_server_is_up() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        post(&boxes, "person-ned-flanders", None, "left for a while").await;
        let start = outbox.rows()[0].push.enqueued_at;
        let drained = pushing(&outbox, &boxes, &notifier)
            .drain(start.checked_add(Duration::from_secs(10_000)).unwrap())
            .await
            .unwrap();
        assert!(
            notifier.delivered().is_empty(),
            "nothing is sent past the window"
        );
        assert_eq!(drained.abandoned.len(), 1);
        assert_eq!(outbox.rows()[0].standing, Standing::Abandoned);
    }

    /// **A row whose message cannot be read is abandoned at once**, naming why,
    /// and nothing is sent.
    #[tokio::test]
    async fn a_row_whose_message_is_gone_is_abandoned_at_once() {
        let boxes = store().await;
        let outbox = boxes.outbox();
        let notifier = FakeNotifier::new();
        let id = post(&boxes, "person-ned-flanders", None, "lost").await;
        boxes.lose(&id);
        let now = outbox.rows()[0].push.enqueued_at;
        let drained = pushing(&outbox, &boxes, &notifier)
            .drain(now)
            .await
            .unwrap();
        assert_eq!(drained.abandoned.len(), 1);
        assert!(drained.abandoned[0].1.contains("no readable message"));
        assert!(notifier.delivered().is_empty());
    }

    /// The wait doubles from the first delay and stops at the longest.
    #[test]
    fn the_wait_doubles_and_is_capped() {
        let waits: Vec<u64> = (1..=6).map(|n| SHORT.delay_after(n).as_secs()).collect();
        assert_eq!(waits, vec![10, 20, 40, 40, 40, 40]);
    }
}
