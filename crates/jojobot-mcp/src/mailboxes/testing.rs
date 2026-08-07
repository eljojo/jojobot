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
        Arc::new(InMemoryMemory::new()),
        Arc::new(SpySearch::default()),
        mailboxes,
        Arc::new(InMemorySessions::new()),
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
            mailbox: mailbox.into(),
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

/// A mailbox world that answers nothing. Shared by both orientation doors:
/// they make the same promise, so they are held to it by the same double.
pub(crate) struct DownMailboxes;

pub(crate) fn handler_with_mailboxes_down(memory: Arc<InMemoryMemory>) -> Jojobot {
    Jojobot::new(
        memory,
        Arc::new(SpySearch::default()),
        Arc::new(DownMailboxes),
        Arc::new(InMemorySessions::new()),
        crate::harness::seeded_registry(),
    )
}

/// A store that reads fine and refuses every creation — the shape the crash
/// window takes when the heal itself cannot land.
pub(crate) struct UnopenableMailboxes(pub(crate) InMemoryMailboxes);

/// Write a bot straight to Memory, with no box — the damage the heal exists
/// to repair. The surface cannot produce this state, which is the point.
pub(crate) async fn broken_bot(jojobot: &Jojobot, slug: &str) {
    jojobot
        .memory
        .add_entity(NewEntity {
            id: EntityId::new(EntityKind::Bot, slug),
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
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn list_mailboxes(&self) -> Result<Vec<mailbox::Mailbox>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn post_message(
        &self,
        _: mailbox::NewMessage,
    ) -> Result<mailbox::Guarded<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn read_mailbox(
        &self,
        _: &mailbox::MailboxName,
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn read_message(
        &self,
        _: &mailbox::MessageId,
    ) -> Result<mailbox::Delivered, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
            "the mailbox world is down".into(),
        ))
    }
    async fn mark_processed(
        &self,
        _: &mailbox::MessageId,
        _: Option<&str>,
    ) -> Result<mailbox::Message, mailbox::MailboxError> {
        Err(mailbox::MailboxError::NotConfigured(
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
    ) -> Result<mailbox::Guarded<mailbox::Delivery>, mailbox::MailboxError> {
        self.0.read_mailbox(name).await
    }
    async fn scan_messages(&self) -> Result<Vec<mailbox::Message>, mailbox::MailboxError> {
        self.0.scan_messages().await
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
}
