//! **What the real store treats as one id and as two ids does not depend on the
//! driver's connection collation.**
//!
//! The driver opens each connection with `SET NAMES`. One version named a
//! case-insensitive collation and the next names none, so a comparison that
//! touches no column would change with it. Every table in the store is
//! `utf8mb4_0900_bin`, which compares by case. These cases write ids that
//! differ only in case and read each back through the port that owns it: a
//! message, a handle and a request id. Each read must answer for the id it was
//! given and for no other.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::requests::DoltRequests;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::mailbox::{
    MailboxError, MailboxName, Mailboxes, MessageId, NewMessage, OwnerIndex, OwnerLookup,
};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::request::{Request, RequestId, Requests, Scope, armed, digest_of, within};

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An owner index that answers for anybody: the cases need a box, not the
/// roster that decides who may have one.
struct AnyOwner;

#[async_trait::async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

fn sent(mailbox: &MailboxName, sender: &str, body: &str) -> NewMessage {
    NewMessage {
        mailbox: mailbox.clone(),
        body: body.into(),
        subject: None,
        sender: sender.into(),
        sent_at: "2026-09-01T10:00:00Z".parse().expect("a moment"),
        in_reply_to: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
        supersedes: None,
    }
}

/// **Two messages whose ids differ only in case, and two senders that differ
/// only in case, each read back by the exact spelling it was given.**
#[tokio::test]
async fn messages_and_senders_that_differ_only_in_case_are_two_in_the_real_store() {
    let scratch =
        Scratch(std::env::temp_dir().join(format!("jojobot-id-case-mail-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "id_case_mail")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    migrate::seed_kinds(&pool)
        .await
        .expect("the kinds are seeded");
    let mail = DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner));
    let owner = EntityId("bot:gamma".into());
    let mailbox = MailboxName::named_for(&owner);
    mail.create_mailbox(&mailbox, &owner, None)
        .await
        .expect("create ok")
        .written()
        .expect("not blocked");
    let lower = mail
        .post_message(sent(&mailbox, "sigma", "from the lower-case sender"))
        .await
        .expect("post ok")
        .written()
        .expect("not blocked")
        .id;
    let other = mail
        .post_message(sent(&mailbox, "Sigma", "from the capitalised sender"))
        .await
        .expect("post ok")
        .written()
        .expect("not blocked")
        .id;
    // The store mints lower-case ids. Spell the second one as the first one's
    // upper-case twin, the way a hand-written row would.
    let upper = MessageId(lower.as_str().to_uppercase());
    assert_ne!(lower, upper, "the minted id has a letter to change");
    sqlx::query("UPDATE message SET id = ? WHERE id = ?")
        .bind(upper.as_str())
        .bind(other.as_str())
        .execute(&pool)
        .await
        .expect("the twin id is written");

    let body = |found: Option<jojobot_domain::mailbox::Message>| found.map(|m| m.body);
    assert_eq!(
        body(mail.message_by_id(&lower).await.expect("reads")),
        Some("from the lower-case sender".to_string()),
        "the lower-case id answers for its own message"
    );
    assert_eq!(
        body(mail.message_by_id(&upper).await.expect("reads")),
        Some("from the capitalised sender".to_string()),
        "the upper-case id answers for its own message"
    );
    let by_ids = |found: Vec<jojobot_domain::mailbox::Message>| {
        found.into_iter().map(|m| m.body).collect::<Vec<_>>()
    };
    assert_eq!(
        by_ids(
            mail.messages_by_id(std::slice::from_ref(&upper))
                .await
                .expect("reads")
        ),
        vec!["from the capitalised sender".to_string()],
        "a set of one id reads the message of that spelling and no other"
    );
    assert_eq!(
        by_ids(
            mail.messages_by_id(std::slice::from_ref(&lower))
                .await
                .expect("reads")
        ),
        vec!["from the lower-case sender".to_string()]
    );
    assert_eq!(
        by_ids(mail.sent_by(&["Sigma"]).await.expect("reads")),
        vec!["from the capitalised sender".to_string()],
        "a sender read by one spelling is not the other's"
    );
    assert_eq!(
        by_ids(mail.sent_by(&["sigma"]).await.expect("reads")),
        vec!["from the lower-case sender".to_string()]
    );

    store.stop().await;
}

/// **A handle spelled with a capital is no handle, and it does not read the
/// thing the lower-case handle names.** A handle is `kind:slug` with a slug of
/// `[a-z0-9-]+`, so two handles never differ only in case and the store is never
/// asked for one. The read must refuse the capital spelling and must not answer
/// it with the lower-case thing, whichever collation the connection names. It
/// answers that it names nothing.
#[tokio::test]
async fn a_handle_spelled_with_a_capital_does_not_read_the_thing_the_lower_case_handle_names() {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("jojobot-id-case-handle-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "id_case_handle")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());

    let lower = EntityId("thing:kettle".into());
    memory
        .add_entity(NewEntity::new(lower.clone(), "Kettle", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let mut fact = NewFact::about(lower.clone(), "boils water", date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");

    let upper = EntityId("thing:KETTLE".into());
    assert_eq!(
        memory.recall(&lower).await.expect("recall").len(),
        1,
        "the lower-case handle reads its claim"
    );
    match memory.recall(&upper).await {
        Ok(found) => assert!(
            found.is_empty(),
            "the capital spelling must not read the lower-case thing's claims: {found:?}"
        ),
        Err(refused) => assert!(
            matches!(
                refused,
                jojobot_domain::memory::MemoryError::InvalidSubject(_)
                    | jojobot_domain::memory::MemoryError::UnknownEntity { .. }
            ),
            "the capital spelling names nothing: {refused:?}"
        ),
    }
    let created = memory
        .add_entity(NewEntity::new(upper, "Teapot", "fixture"))
        .await;
    assert!(
        matches!(
            created,
            Err(jojobot_domain::memory::MemoryError::InvalidSubject(_))
        ),
        "a capital spelling cannot make a second thing: {created:?}"
    );

    store.stop().await;
}

/// **Two request ids that differ only in case are two ids.** The ledger case
/// in `dolt_requests.rs` holds this over a capture; this holds it over the
/// lookup alone, so the three ids sit in one file.
#[tokio::test]
async fn request_ids_that_differ_only_in_case_are_two_in_the_real_ledger() {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("jojobot-id-case-request-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "id_case_request")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ledger = DoltRequests::new(pool.clone());
    let subject = EntityId("thing:kettle".into());
    memory
        .add_entity(NewEntity::new(subject.clone(), "Kettle", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    memory
        .add_entity(NewEntity::new(
            EntityId("bot:sigma".into()),
            "Sigma",
            "fixture",
        ))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let request = |id: &str| Request {
        bot: EntityId("bot:sigma".into()),
        id: RequestId::parse(id).expect("a good id"),
        verb: "capture".to_string(),
        digest: digest_of("{}"),
    };
    let upper = request("Req-A");
    let lower = request("req-a");
    let mut fact = NewFact::about(subject, "boils water", date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    let scope = Scope::new(upper.clone());
    within(Arc::clone(&scope), armed(memory.capture(fact)))
        .await
        .expect("the write lands");
    assert!(scope.was_spent(), "the write spent its id");

    assert!(
        ledger
            .lookup(&upper.bot, &upper.id)
            .await
            .expect("reads")
            .is_some(),
        "the id that was spent is found by its own spelling"
    );
    assert!(
        ledger
            .lookup(&lower.bot, &lower.id)
            .await
            .expect("reads")
            .is_none(),
        "the other spelling is not found: it reads no row of the first"
    );

    store.stop().await;
}
