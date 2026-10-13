//! **The in-memory image of sessions, mail and teachings held to the real
//! store** (decision log 292, 435).
//!
//! The same walks the fakes run in `fold_rails_image_tests`, over a real Dolt
//! store: every kind of write goes through the image and, after each one, every
//! read is asked of the image and of the store, and the reads the image serves
//! must have made no call to the store.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use jojobot_adapters::counted_rails::{CountedMailboxes, CountedSessions, CountedTeachings};
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::dolt::teaching::DoltTeachings;
use jojobot_adapters::fold::mail::FoldedMailboxes;
use jojobot_adapters::fold::sessions::FoldedSessions;
use jojobot_adapters::fold::teachings::FoldedTeachings;
use jojobot_adapters::fold_rails_battery::{
    MailRails, SessionRails, TeachRails, walk_every_kind_of_mail_write,
    walk_every_kind_of_session_write, walk_every_kind_of_teaching,
};
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::EntityId;

/// The session reads the image serves. Each is added in the commit that
/// switches it.
const SWITCHED_SESSIONS: &[&str] = &[
    "read_session",
    "sessions_of",
    "summaries_of",
    "all_summaries",
    "summary_of",
    "run_counts",
    "all_sessions",
    "charter_change_of",
];

/// The mail reads the image serves.
const SWITCHED_MAIL: &[&str] = &[
    "list_mailboxes",
    "scan_messages",
    "message_by_id",
    "sent_by",
    "read_mailbox",
    "read_mailbox",
];

/// The teaching calls the image serves.
const SWITCHED_TEACHINGS: &[&str] = &["first_contact"];

struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-fold-rails-{}-{what}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every well-formed owner resolves: the walk is about the board, not about who
/// may own a box.
struct AnyOwner;

#[async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

async fn database(server: &Dolt, what: &str) -> sqlx::MySqlPool {
    let pool = migrated_database(server, what)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    pool
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_session_image_answers_as_the_real_store_does_after_every_kind_of_write() {
    let scratch = Scratch::new("sessions");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "sessions").await;
    let store = Arc::new(DoltSessions::open(pool));
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    let rails = SessionRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED_SESSIONS,
    };
    walk_every_kind_of_session_write(&rails).await;
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_mail_image_answers_as_the_real_store_does_after_every_kind_of_write() {
    let scratch = Scratch::new("mail");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "mail").await;
    let store = Arc::new(DoltMailboxes::open(pool, Arc::new(AnyOwner)));
    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    let rails = MailRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED_MAIL,
    };
    walk_every_kind_of_mail_write(&rails).await;
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_teaching_image_answers_as_the_real_store_does_after_every_kind_of_write() {
    let scratch = Scratch::new("teachings");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "teachings").await;
    let twin_pool = database(&server, "teachings_twin").await;
    let store = Arc::new(DoltTeachings::open(pool));
    let counted = Arc::new(CountedTeachings::new(store));
    let folded = FoldedTeachings::new(counted.clone());
    folded.rebuild().await.expect("the ledger loads");
    let twin = DoltTeachings::open(twin_pool);
    let rails = TeachRails {
        folded: &folded,
        counted: &counted,
        twin: &twin,
        switched: SWITCHED_TEACHINGS,
    };
    walk_every_kind_of_teaching(&rails).await;
    server.stop().await;
}

/// **A message taken before deliveries were recorded has no way of being taken
/// on the board**, and a later delivery that hands it over again as a leftover
/// says how THIS delivery happened without writing it. The image must keep what
/// the store keeps, so the way is still unknown after that delivery, and a board
/// read says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_leftover_taken_before_deliveries_were_recorded_stays_without_a_way_it_was_taken() {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage, TakenBy};
    let scratch = Scratch::new("legacy-read");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "legacy_read").await;
    let store = Arc::new(DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner)));
    let inbox = MailboxName("gamma".into());
    store
        .create_mailbox(&inbox, &EntityId("bot:gamma".into()), None)
        .await
        .expect("create")
        .written()
        .expect("not blocked");
    let post = |body: &str| NewMessage {
        mailbox: inbox.clone(),
        body: body.into(),
        subject: None,
        sender: "delta".into(),
        sent_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
        in_reply_to: None,
        supersedes: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
    };
    let old = store
        .post_message(post("taken long ago"))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    // The state a delivery of an older build left: read, with no delivery row.
    sqlx::query("UPDATE message SET state = 'read' WHERE id = ?")
        .bind(old.id.as_str())
        .execute(&pool)
        .await
        .expect("the legacy state is written");

    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    folded
        .post_message(post("arrives later"))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    let delivery = folded
        .read_mailbox(&inbox, TakenBy::Posting)
        .await
        .expect("read_mailbox")
        .written()
        .expect("not blocked");
    assert_eq!(
        delivery.messages.len(),
        2,
        "the old message comes back as a leftover beside the new one: {delivery:?}"
    );
    assert_eq!(
        format!("{:?}", folded.scan_messages().await),
        format!("{:?}", store.scan_messages().await),
        "the image keeps what the store keeps about how each message was taken"
    );
    // A delivery of a box with nothing new says how THIS delivery happened for
    // the message nobody recorded a way for, and still writes nothing.
    let again = folded
        .read_mailbox(&inbox, TakenBy::Reading)
        .await
        .expect("read_mailbox again");
    assert_eq!(
        format!("{again:?}"),
        format!(
            "{:?}",
            store
                .read_mailbox(&inbox, TakenBy::Reading)
                .await
                .expect("the store reads")
        ),
        "a delivery with nothing new answers as the store does"
    );
    let unrecorded = again
        .written()
        .expect("not blocked")
        .messages
        .into_iter()
        .find(|d| d.message.id == old.id)
        .expect("the old message is handed over again");
    assert_eq!(
        unrecorded.message.taken_by,
        Some(TakenBy::Reading),
        "the way this delivery happened is told for a message with none recorded"
    );
    assert_eq!(
        format!("{:?}", folded.scan_messages().await),
        format!("{:?}", store.scan_messages().await),
        "and the board still records none for it"
    );
    server.stop().await;
}

/// **A card the store cannot read is named by its box, and stays named after the
/// board moves.** The image is filled from what the store reads, so the damaged
/// card is in the box's unreadable list and in no count, and a message filed
/// beside it moves the counts and leaves the list as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_damaged_card_stays_named_by_its_box_while_the_board_moves() {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes};
    let scratch = Scratch::new("damaged-card");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "damaged_card").await;
    let store = Arc::new(DoltMailboxes::open(pool.clone(), Arc::new(AnyOwner)));
    let inbox = MailboxName("gamma".into());
    store
        .create_mailbox(&inbox, &EntityId("bot:gamma".into()), None)
        .await
        .expect("create")
        .written()
        .expect("not blocked");
    sqlx::query(
        "INSERT INTO message
           (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
         VALUES ('damaged1', 'gamma', 99, 'whatever this was', NULL, 'delta', 'not a time', 'new', NULL, NULL)",
    )
    .execute(&pool)
    .await
    .expect("the board takes the row");

    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    let boxes = folded.list_mailboxes().await.expect("list");
    assert_eq!(
        boxes[0].quarantined.len(),
        1,
        "the damaged card is named by its box: {boxes:?}"
    );
    folded
        .post_message(jojobot_domain::mailbox::NewMessage {
            mailbox: inbox.clone(),
            body: "filed beside it".into(),
            subject: None,
            sender: "delta".into(),
            sent_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    assert_eq!(
        format!("{:?}", folded.list_mailboxes().await),
        format!("{:?}", store.list_mailboxes().await),
        "the image names the same cards and counts the same messages as the store"
    );
    assert_eq!(
        format!("{:?}", folded.scan_messages().await),
        format!("{:?}", store.scan_messages().await),
    );
    server.stop().await;
}

/// **A run the store cannot read leaves no image, and the reads stay correct**
/// because they go to the store, which refuses the way it always did.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_the_store_cannot_read_leaves_no_image_and_the_reads_go_to_the_store() {
    use jojobot_domain::session::Sessions;
    let scratch = Scratch::new("damaged-run");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = database(&server, "damaged_run").await;
    sqlx::query(
        "INSERT INTO session (id, sid, bot, focus, started_at, state)
         VALUES ('damaged', NULL, 'bot:gamma', 'what it was doing', '2026-01-01T00:00:00Z', 'garbage')",
    )
    .execute(&pool)
    .await
    .expect("the board takes the row");
    let store = Arc::new(DoltSessions::open(pool));
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone());
    assert!(
        folded.rebuild().await.is_err(),
        "a board with a run nobody can read does not fill an image"
    );
    counted.calls.reset();
    assert_eq!(
        format!("{:?}", folded.all_sessions().await),
        format!("{:?}", store.all_sessions().await),
        "the reads answer as the store does"
    );
    assert_eq!(
        counted.calls.reads(),
        1,
        "with no image the read reaches the store: {:?}",
        counted.calls.seen()
    );
    server.stop().await;
}
