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
            to: mailbox.into(),
            sid: as_bot(jojobot, sender),
            subject: subject.map(str::to_string),
            body: body.into(),
            in_reply_to: None,
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
    let handle = format!("person:{person}");
    crate::memory::testing::ensure(jojobot, &handle).await;
    crate::memory::testing::capture_ok(
        jojobot,
        CaptureArgs {
            fields: Some([("operator".to_string(), handle)].into()),
            provenance: Some("testimony".into()),
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

/// **A store that says how often the board was read.** `list_mailboxes` hands
/// back the whole table, so how many times one answer reaches for it is the
/// difference between deriving several facts from one read and asking the
/// store the same question twice.
pub(crate) struct CountingMailboxes {
    inner: InMemoryMailboxes,
    listings: std::sync::atomic::AtomicUsize,
}

impl CountingMailboxes {
    /// How many times the board has been read so far. A test takes it either
    /// side of the call it is measuring, because the fixtures that stage the
    /// world read the board too.
    pub(crate) fn listings(&self) -> usize {
        self.listings.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// A handler over a board that counts its own reads, and the handle the test
/// asks for the count.
pub(crate) fn counting_handler() -> (Jojobot, Arc<CountingMailboxes>) {
    let mailboxes = Arc::new(CountingMailboxes {
        inner: InMemoryMailboxes::knowing_any_owner(),
        listings: std::sync::atomic::AtomicUsize::new(0),
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
            id: EntityId::new(EntityKind::BOT, slug),
            name: slug.into(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            override_token: None,
        })
        .await
        .expect("the store writes it");
}

#[async_trait]
impl mailbox::Mailboxes for DownMailboxes {
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
    async fn sent_by(&self, _: &[&str]) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn read_message(
        &self,
        _: &mailbox::MessageId,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
    async fn mark_processed(
        &self,
        _: &mailbox::MessageId,
        _: Option<&str>,
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
        _: jiff::Timestamp,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        Err(mailbox::MailboxError::Store(
            "the mailbox world is down".into(),
        ))
    }
}

#[async_trait]
impl mailbox::Mailboxes for UnopenableMailboxes {
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
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.0.read_mailbox(name, taken_by).await
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
    async fn sent_by(
        &self,
        senders: &[&str],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.0.sent_by(senders).await
    }
    async fn read_message(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.0.read_message(id).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        notes: Option<&str>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        self.0.mark_processed(id, notes).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        at: jiff::Timestamp,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.0.quarantine(id, by, reason, at).await
    }
}

#[async_trait]
impl mailbox::Mailboxes for CountingMailboxes {
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
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.inner.read_mailbox(name, taken_by).await
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.inner.scan_messages().await
    }
    async fn message_by_id(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<Option<mailbox::Message>, mailbox::MailboxError> {
        self.inner.message_by_id(id).await
    }
    async fn sent_by(
        &self,
        senders: &[&str],
    ) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.inner.sent_by(senders).await
    }
    async fn read_message(
        &self,
        id: &mailbox::MessageId,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.inner.read_message(id).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        notes: Option<&str>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        self.inner.mark_processed(id, notes).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        at: jiff::Timestamp,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.inner.quarantine(id, by, reason, at).await
    }
}

#[async_trait]
impl mailbox::Mailboxes for RenamingMailboxes {
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
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.inner.read_mailbox(name, taken_by).await
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
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        self.inner.read_message(id).await
    }
    async fn mark_processed(
        &self,
        id: &mailbox::MessageId,
        notes: Option<&str>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        self.inner.mark_processed(id, notes).await
    }
    async fn quarantine(
        &self,
        id: &mailbox::MessageId,
        by: &mailbox::MailboxName,
        reason: &str,
        at: jiff::Timestamp,
    ) -> Result<mailbox::Quarantined, mailbox::MailboxError> {
        self.inner.quarantine(id, by, reason, at).await
    }
}
