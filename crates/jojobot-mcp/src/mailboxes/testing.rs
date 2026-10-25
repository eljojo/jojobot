//! **Mailboxes' test fixtures** — the boxes a test needs standing, the mail it
//! needs sent, and the doubles that make a store fail on purpose.
//!
//! Named for the context they belong to, mirroring `jojobot_domain::mailbox::
//! testing`. What builds a handler lives in [`crate::harness`]; what is ABOUT
//! mailboxes lives here.

use super::*;
use crate::harness::*;
use crate::memory::testing::SpySearch;
use async_trait::async_trait;
pub(crate) use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::testing::InMemorySessions;

pub(crate) fn mailbox_handler() -> Jojobot {
    with_mailboxes(Arc::new(InMemoryMailboxes::knowing_any_owner()))
}

/// A handler over a mailbox store the test still holds a typed handle to —
/// for the states only the store can put itself into.
pub(crate) fn with_mailboxes(mailboxes: Arc<InMemoryMailboxes>) -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        mailboxes,
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    )
}

/// What is waiting in this bot's own box, without taking delivery of any of
/// it.
///
/// **A fixture that counted through the delivery path would poison every test
/// that used it**: each call would quietly move the box's mail out of `new`,
/// and an assertion about counts would be measuring the fixture rather than
/// the code. It goes through the same `counts_only` a caller does.
pub(crate) async fn counts(jojobot: &Jojobot, bot: &str) -> serde_json::Value {
    let sid = as_bot(jojobot, bot);
    json_of(
        &jojobot
            .read_mailbox(Parameters(ReadMailboxArgs {
                counts_only: Some(true),
                new_only: None,
                sid: Some(sid),
            }))
            .await
            .expect("counting ok"),
    )
}

/// The bot whose box this is — because a box has no separate existence to
/// create. A fixture that wants a box named `n` is a fixture that wants the
/// bot `n`, and it gets both.
///
/// **`make_box_for` and `fixture_owner` are gone with it.** Both existed to
/// answer "whose is this one?" about a box minted on its own, and nothing
/// mints one on its own now: the owner is not chosen, it is the bot whose
/// name the box carries.
pub(crate) async fn make_box(jojobot: &Jojobot, name: &str) -> serde_json::Value {
    make_bot(jojobot, name).await;
    let listed = jojobot
        .mailboxes
        .list_mailboxes()
        .await
        .expect("list_mailboxes ok");
    let found = listed
        .into_iter()
        .find(|b| b.name.as_str() == name)
        .unwrap_or_else(|| panic!("the fixture box {name:?} was never opened"));
    mailbox_json(&found)
}

/// Post as a bot. `sender` is its bare slug now, not free text: the sender
/// recorded on the message is the identity behind the handle, so it lands
/// as `bot:<sender>`.
pub(crate) async fn send(
    jojobot: &Jojobot,
    mailbox: &str,
    sender: &str,
    body: &str,
) -> serde_json::Value {
    send_titled(jojobot, mailbox, sender, None, body).await
}

pub(crate) async fn send_titled(
    jojobot: &Jojobot,
    mailbox: &str,
    sender: &str,
    subject: Option<&str>,
    body: &str,
) -> serde_json::Value {
    let result = jojobot
        .post_message(Parameters(PostMessageArgs {
            request_id: None,
            to: mailbox.into(),
            sid: as_bot(jojobot, sender),
            subject: subject.map(str::to_string),
            body: body.into(),
            in_reply_to: None,
            supersedes: None,
        }))
        .await
        .expect("post_message call ok");
    let body = json_of(&result);
    assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
    body
}

/// **A held-open message stops costing its full size on every poll — and is
/// never hidden.** The crash contract keeps a message unprocessed until the
/// work it asks for is done, which is correct; but every poll of that box
/// then re-delivered the whole multi-KB body flagged `seen_before`. Over a
/// long pickup loop that is the same message downloaded all night.
///
/// A bot that exists, owns the box named for it, and has a handle to call
/// with. **The box is not a second argument** — it never was a choice.
pub(crate) async fn owning(jojobot: &Jojobot, bot: &str) -> String {
    make_bot(jojobot, bot).await;
    as_bot(jojobot, bot)
}

/// A second box for a bot that already has one — **written straight to the
/// store**, because no verb on this surface can produce it: a box opens with
/// its bot, and nothing else opens one. One box per bot is what every read
/// path is entitled to assume, so this is the damage those paths must report.
pub(crate) async fn a_second_box(jojobot: &Jojobot, bot: &str, name: &str) {
    let written = jojobot
        .mailboxes
        .create_mailbox(
            &MailboxName(name.into()),
            &EntityId::new(EntityKind::BOT, bot),
            None,
        )
        .await
        .expect("the store writes it");
    assert!(
        matches!(written, mailbox::Guarded::Written(_)),
        "the fixture second box {name:?} was never opened, so nothing below is damage"
    );
}

/// **The operator's box, opened the way a post opens it**: the person is named
/// as the operator and the box is opened through the same route the first post
/// takes. Returns the box's name. Every guard below is about a box that exists,
/// and this one exists by the real route.
pub(crate) async fn a_persons_box(jojobot: &Jojobot, person: &str) -> MailboxName {
    name_the_operator(jojobot, person).await;
    let owner = EntityId::new(EntityKind::PERSON, person);
    match jojobot.operators_box(&owner).await {
        Ok(name) => name,
        Err(refused) => panic!(
            "the fixture person box for {owner} was never opened, so nothing below is private: \
             {refused:?}"
        ),
    }
}

/// **Name a person as the instance's operator**, the way the operator does: the
/// person exists, and `topic:instance` holds their handle under `operator`.
pub(crate) async fn name_the_operator(jojobot: &Jojobot, person: &str) {
    name_the_operator_as(jojobot, crate::harness::TEST_SID, person).await;
}

/// [`name_the_operator`], written by the session `sid`. **Naming a different
/// operator once one is named is the head of the chart's to do**, so a test that
/// changes the operator writes it as the bot that heads one.
pub(crate) async fn name_the_operator_as(jojobot: &Jojobot, sid: &str, person: &str) {
    let handle = format!("person:{person}");
    crate::memory::testing::ensure(jojobot, &handle).await;
    crate::memory::testing::capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some([("operator".to_string(), handle)].into()),
            provenance: Some("testimony".into()),
            sid: Some(sid.to_string()),
            ..crate::memory::testing::capture_args("topic:instance", "who the operator is")
        },
    )
    .await;
}

/// Every box a handle owns, from the store's own board read.
pub(crate) async fn boxes_owned_by(jojobot: &Jojobot, owner: &str) -> Vec<MailboxName> {
    jojobot
        .mailboxes
        .list_mailboxes()
        .await
        .expect("list_mailboxes ok")
        .into_iter()
        .filter(|held| held.owner.as_str() == owner)
        .map(|held| held.name)
        .collect()
}

/// What the store holds in a box, counted from the store's own board read and
/// not from any verb: the numbers a guard's refusal must leave exactly as they
/// were.
pub(crate) async fn store_counts(jojobot: &Jojobot, name: &MailboxName) -> (usize, usize, usize) {
    let board = jojobot
        .mailboxes
        .list_mailboxes()
        .await
        .expect("list_mailboxes ok");
    let held = board
        .iter()
        .find(|b| &b.name == name)
        .unwrap_or_else(|| panic!("no box {name:?} on the board"));
    (held.counts.new, held.counts.read, held.counts.processed)
}

/// A mailbox world that answers nothing. Shared by both orientation doors:
/// they make the same promise, so they are held to it by the same double.
pub(crate) struct DownMailboxes;

pub(crate) fn handler_with_mailboxes_down(memory: Arc<InMemoryMemory>) -> Jojobot {
    Jojobot::new(
        memory,
        Arc::new(SpySearch::default()),
        Arc::new(DownMailboxes),
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    )
}

/// A store that reads fine and refuses every creation — the shape the crash
/// window takes when the heal itself cannot land.
pub(crate) struct UnopenableMailboxes(pub(crate) InMemoryMailboxes);

/// **Failures that begin once a post has landed**, so a case can fail what the
/// verb reads AFTER its write: the closes a reply makes. Armed before the post;
/// the Nth read of the kind after the post lands fails, once.
#[derive(Default)]
pub(crate) struct AfterPost {
    /// Whether a post that lands starts the counting.
    armed: std::sync::atomic::AtomicBool,
    /// Whether a post has landed since it was armed.
    landed: std::sync::atomic::AtomicBool,
    /// Which listing of the board after the post fails (1 is the first), 0 none.
    listing_at: std::sync::atomic::AtomicUsize,
    listings: std::sync::atomic::AtomicUsize,
    /// Which read of one message by id after the post fails, 0 none.
    by_id_at: std::sync::atomic::AtomicUsize,
    by_ids: std::sync::atomic::AtomicUsize,
    /// Whether the read of a sender's mail after the post fails.
    sent_by: std::sync::atomic::AtomicBool,
}

impl AfterPost {
    /// Whether this is the read that fails: `count` is how many of its kind have
    /// run since the post, and `at` which one is to fail.
    fn fails(
        &self,
        count: &std::sync::atomic::AtomicUsize,
        at: &std::sync::atomic::AtomicUsize,
    ) -> bool {
        use std::sync::atomic::Ordering::{AcqRel, Acquire};
        let at = at.load(Acquire);
        self.landed.load(Acquire) && at > 0 && count.fetch_add(1, AcqRel) + 1 == at
    }
}

/// **A store that says how often the board was read.** `list_mailboxes` hands
/// back the whole table, so how many times one answer reaches for it is the
/// difference between deriving several facts from one read and asking the
/// store the same question twice.
pub(crate) struct CountingMailboxes {
    inner: InMemoryMailboxes,
    listings: std::sync::atomic::AtomicUsize,
    /// Reads of messages by id, one for each call that reaches the store.
    message_queries: std::sync::atomic::AtomicUsize,
    /// Set by [`Self::fail_the_next_read_by_id`]; the next read of messages by id
    /// fails with a store error and clears it.
    fail_by_id: std::sync::atomic::AtomicBool,
    /// Set by [`Self::fail_the_next_history`]; the next read of a message's
    /// history fails with a store error and clears it.
    fail_history: std::sync::atomic::AtomicBool,
    /// Set by [`Self::fail_the_next_mark_processed`]; the next retirement fails
    /// with a store error and clears it.
    fail_mark: std::sync::atomic::AtomicBool,
    after_post: AfterPost,
}

impl CountingMailboxes {
    /// **Fail the Nth listing of the board after the next post lands**, once.
    pub(crate) fn fail_the_nth_listing_after_the_post(&self, n: usize) {
        use std::sync::atomic::Ordering::Release;
        self.after_post.armed.store(true, Release);
        self.after_post.listing_at.store(n, Release);
    }

    /// **Fail the Nth read of one message by id after the next post lands**, once.
    pub(crate) fn fail_the_nth_read_of_a_message_after_the_post(&self, n: usize) {
        use std::sync::atomic::Ordering::Release;
        self.after_post.armed.store(true, Release);
        self.after_post.by_id_at.store(n, Release);
    }

    /// **Fail the read of a sender's mail after the next post lands**, once.
    pub(crate) fn fail_the_read_of_a_senders_mail_after_the_post(&self) {
        use std::sync::atomic::Ordering::Release;
        self.after_post.armed.store(true, Release);
        self.after_post.sent_by.store(true, Release);
    }

    /// **Fail the next retirement of a message, once.**
    pub(crate) fn fail_the_next_mark_processed(&self) {
        self.fail_mark
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// How many times the board has been read so far. A test takes it either
    /// side of the call it is measuring, because the fixtures that stage the
    /// world read the board too.
    pub(crate) fn listings(&self) -> usize {
        self.listings.load(std::sync::atomic::Ordering::Acquire)
    }

    /// **Fail the next read of messages by id, once.** Whether the failure
    /// happened is what [`Self::failure_pending`] answers `false` to.
    pub(crate) fn fail_the_next_read_by_id(&self) {
        self.fail_by_id
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// **Fail the next read of a message's history, once.**
    pub(crate) fn fail_the_next_history(&self) {
        self.fail_history
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Whether an armed failure has not happened yet: a case that arms one and
    /// finds this `false` has seen the read fail.
    pub(crate) fn failure_pending(&self) -> bool {
        self.fail_by_id.load(std::sync::atomic::Ordering::Acquire)
            || self.fail_history.load(std::sync::atomic::Ordering::Acquire)
    }

    /// How many calls asking for messages by id have reached the store so far:
    /// a call for one id and a call for many each count once.
    pub(crate) fn message_queries(&self) -> usize {
        self.message_queries
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

/// A handler over a board that counts its own reads, and the handle the test
/// asks for the count.
pub(crate) fn counting_handler() -> (Jojobot, Arc<CountingMailboxes>) {
    let mailboxes = Arc::new(CountingMailboxes {
        inner: InMemoryMailboxes::knowing_any_owner(),
        listings: std::sync::atomic::AtomicUsize::new(0),
        message_queries: std::sync::atomic::AtomicUsize::new(0),
        fail_by_id: std::sync::atomic::AtomicBool::new(false),
        fail_history: std::sync::atomic::AtomicBool::new(false),
        fail_mark: std::sync::atomic::AtomicBool::new(false),
        after_post: AfterPost::default(),
    });
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        mailboxes.clone(),
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    );
    (jojobot, mailboxes)
}

/// **A store whose box is renamed between the two reads a guard makes.** A
/// guard locates a message, then reads the board to learn whose box it sits in;
/// a rename of the owner lands in between and moves the box to a new name. The
/// double runs that rename right after the first lookup of a message or of a
/// sender's mail returns, once, so the lookup holds the old name and the board
/// only the new one.
pub(crate) struct RenamingMailboxes {
    inner: InMemoryMailboxes,
    rename: std::sync::Mutex<Option<(EntityId, EntityId)>>,
}

impl RenamingMailboxes {
    /// Rename `from` to `to` after the next lookup of a message or of a
    /// sender's mail. Armed after the fixtures, which look messages up too.
    pub(crate) fn arm(&self, from: &str, to: &str) {
        *self.rename.lock().expect("rename lock") =
            Some((EntityId(from.into()), EntityId(to.into())));
    }

    async fn rename_now(&self) {
        let due = self.rename.lock().expect("rename lock").take();
        if let Some((from, to)) = due {
            self.inner
                .repoint_owner(&from, &to)
                .await
                .expect("the rename lands")
                .expect("the renamed owner had a box");
        }
    }
}

/// A handler over a board that renames a box mid-call, and the handle the test
/// arms it with.
pub(crate) fn renaming_handler() -> (Jojobot, Arc<RenamingMailboxes>) {
    let mailboxes = Arc::new(RenamingMailboxes {
        inner: InMemoryMailboxes::knowing_any_owner(),
        rename: std::sync::Mutex::new(None),
    });
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        mailboxes.clone(),
        Arc::new(InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    );
    (jojobot, mailboxes)
}

/// Write a bot straight to Memory, with no box — the damage the heal exists
/// to repair. The surface cannot produce this state, which is the point.
pub(crate) async fn broken_bot(jojobot: &Jojobot, slug: &str) {
    jojobot
        .memory
        .add_entity(NewEntity {
            session: None,
            id: EntityId::new(EntityKind::BOT, slug),
            name: slug.into(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            override_token: None,
            override_reason: None,
        })
        .await
        .expect("the store writes it");
}

#[async_trait]
impl mailbox::Mailboxes for DownMailboxes {
    /// A mailbox world that answers nothing keeps no count, which is the trait's
    /// default said out loud so that the guard beside the doubles can tell a
    /// default left alone from a default meant.
    fn changes(&self) -> Option<u64> {
        None
    }
    async fn create_mailbox(
        &self,
        _: &mailbox::MailboxName,
        _: &EntityId,
        _: Option<&str>,
    ) -> Result<mailbox::Guarded<mailbox::Mailbox>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn repoint_owner(
        &self,
        _: &EntityId,
        _: &EntityId,
    ) -> Result<Option<mailbox::Mailbox>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn post_message(
        &self,
        _: mailbox::NewMessage,
    ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn read_mailbox(
        &self,
        _: &mailbox::MailboxName,
        _: mailbox::TakenBy,
        _acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn message_by_id(
        &self,
        _: &mailbox::MessageId,
    ) -> Result<Option<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn messages_by_id(
        &self,
        _: &[mailbox::MessageId],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn sent_by(&self, _: &[&str]) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn read_message(
        &self,
        _: &mailbox::MessageId,
        _acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn mark_processed(
        &self,
        _: &mailbox::MessageId,
        _: &mailbox::MailboxName,
        _: Option<&str>,
        _acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn quarantine(
        &self,
        _: &mailbox::MessageId,
        _: &mailbox::MailboxName,
        _: &str,
        _acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }

    async fn message_history(
        &self,
        _id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        Err(jojobot_domain::mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
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
        Err(jojobot_domain::mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
}

#[async_trait]
impl mailbox::Mailboxes for UnopenableMailboxes {
    fn changes(&self) -> Option<u64> {
        self.0.changes()
    }
    async fn create_mailbox(
        &self,
        _: &mailbox::MailboxName,
        _: &EntityId,
        _: Option<&str>,
    ) -> Result<mailbox::Guarded<mailbox::Mailbox>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the board refuses writes".into(),
        ))
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<mailbox::Mailbox>, mailbox::MailboxError> {
        self.0.repoint_owner(from, to).await
    }
    async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
        self.0.list_mailboxes().await
    }
    async fn post_message(
        &self,
        new: mailbox::NewMessage,
    ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
        self.0.post_message(new).await
    }
    async fn read_mailbox(
        &self,
        name: &mailbox::MailboxName,
        taken_by: mailbox::TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.0.read_mailbox(name, taken_by, acting).await
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.0.scan_messages().await
    }
    async fn message_by_id(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<Option<mailbox::Message>, mailbox::MailboxError> {
        self.0.message_by_id(id).await
    }
    async fn messages_by_id(
        &self,
        ids: &[mailbox::MessageId],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.0.messages_by_id(ids).await
    }
    async fn sent_by(
        &self,
        senders: &[&str],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.0.sent_by(senders).await
    }
    async fn read_message(
        &self,
        id: &mailbox::MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.0.read_message(id, acting).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        by: &jojobot_domain::mailbox::MailboxName,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        self.0.mark_processed(id, by, notes, acting).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.0.quarantine(id, by, reason, acting).await
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        self.0.message_history(id).await
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
        self.0.scan_message_events().await
    }
}

#[async_trait]
impl mailbox::Mailboxes for CountingMailboxes {
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }
    async fn create_mailbox(
        &self,
        name: &mailbox::MailboxName,
        owner: &EntityId,
        note: Option<&str>,
    ) -> Result<mailbox::Guarded<mailbox::Mailbox>, mailbox::MailboxError> {
        self.inner.create_mailbox(name, owner, note).await
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<mailbox::Mailbox>, mailbox::MailboxError> {
        self.inner.repoint_owner(from, to).await
    }
    async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
        self.listings
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if self
            .after_post
            .fails(&self.after_post.listings, &self.after_post.listing_at)
        {
            return Err(mailbox::MailboxError::Store(
                "the board cannot be read".into(),
            ));
        }
        self.inner.list_mailboxes().await
    }
    async fn post_message(
        &self,
        new: mailbox::NewMessage,
    ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
        let posted = self.inner.post_message(new).await;
        if posted.is_ok()
            && self
                .after_post
                .armed
                .load(std::sync::atomic::Ordering::Acquire)
        {
            self.after_post
                .landed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        posted
    }
    async fn read_mailbox(
        &self,
        name: &mailbox::MailboxName,
        taken_by: mailbox::TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.inner.read_mailbox(name, taken_by, acting).await
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.inner.scan_messages().await
    }
    async fn message_by_id(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<Option<mailbox::Message>, mailbox::MailboxError> {
        self.message_queries
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if self
            .after_post
            .fails(&self.after_post.by_ids, &self.after_post.by_id_at)
        {
            return Err(mailbox::MailboxError::Store(
                "the message cannot be read".into(),
            ));
        }
        self.inner.message_by_id(id).await
    }
    async fn messages_by_id(
        &self,
        ids: &[mailbox::MessageId],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.message_queries
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if self
            .fail_by_id
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(mailbox::MailboxError::Store(
                "messages cannot be read by id".into(),
            ));
        }
        self.inner.messages_by_id(ids).await
    }
    async fn sent_by(
        &self,
        senders: &[&str],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        if self
            .after_post
            .landed
            .load(std::sync::atomic::Ordering::Acquire)
            && self
                .after_post
                .sent_by
                .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(mailbox::MailboxError::Store(
                "the sender's mail cannot be read".into(),
            ));
        }
        self.inner.sent_by(senders).await
    }
    async fn read_message(
        &self,
        id: &mailbox::MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.inner.read_message(id, acting).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        by: &jojobot_domain::mailbox::MailboxName,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        if self
            .fail_mark
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(mailbox::MailboxError::Store(
                "messages cannot be retired".into(),
            ));
        }
        self.inner.mark_processed(id, by, notes, acting).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.inner.quarantine(id, by, reason, acting).await
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        if self
            .fail_history
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(jojobot_domain::mailbox::MailboxError::Store(
                "a message's history cannot be read".into(),
            ));
        }
        self.inner.message_history(id).await
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
        self.inner.scan_message_events().await
    }
}

#[async_trait]
impl mailbox::Mailboxes for RenamingMailboxes {
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }
    async fn create_mailbox(
        &self,
        name: &mailbox::MailboxName,
        owner: &EntityId,
        note: Option<&str>,
    ) -> Result<mailbox::Guarded<mailbox::Mailbox>, mailbox::MailboxError> {
        self.inner.create_mailbox(name, owner, note).await
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<mailbox::Mailbox>, mailbox::MailboxError> {
        self.inner.repoint_owner(from, to).await
    }
    async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
        self.inner.list_mailboxes().await
    }
    async fn post_message(
        &self,
        new: mailbox::NewMessage,
    ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
        self.inner.post_message(new).await
    }
    async fn read_mailbox(
        &self,
        name: &mailbox::MailboxName,
        taken_by: mailbox::TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.inner.read_mailbox(name, taken_by, acting).await
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.inner.scan_messages().await
    }
    async fn message_by_id(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<Option<mailbox::Message>, mailbox::MailboxError> {
        let found = self.inner.message_by_id(id).await;
        self.rename_now().await;
        found
    }
    async fn messages_by_id(
        &self,
        ids: &[mailbox::MessageId],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        let found = self.inner.messages_by_id(ids).await;
        self.rename_now().await;
        found
    }
    async fn sent_by(
        &self,
        senders: &[&str],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        let found = self.inner.sent_by(senders).await;
        self.rename_now().await;
        found
    }
    async fn read_message(
        &self,
        id: &mailbox::MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.inner.read_message(id, acting).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        by: &jojobot_domain::mailbox::MailboxName,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        self.inner.mark_processed(id, by, notes, acting).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.inner.quarantine(id, by, reason, acting).await
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        self.inner.message_history(id).await
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
        self.inner.scan_message_events().await
    }
}
