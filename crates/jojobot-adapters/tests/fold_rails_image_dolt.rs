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
const SWITCHED_SESSIONS: &[&str] = &["read_session", "sessions_of", "summaries_of", "all_summaries", "summary_of", "run_counts", "all_sessions"];

/// The mail reads the image serves.
const SWITCHED_MAIL: &[&str] = &[];

/// The teaching calls the image serves.
const SWITCHED_TEACHINGS: &[&str] = &[];

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
