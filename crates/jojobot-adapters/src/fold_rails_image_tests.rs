//! **The in-memory image of sessions, mail and teachings (decision log 292),
//! held to the store it mirrors.**
//!
//! The walks in [`crate::fold_rails_battery`] write every kind of write through
//! the image and, after each, ask every read of the image and of the store.
//! These cases run them over the fakes; `tests/fold_rails_image_dolt.rs` runs
//! the same walks over the real store.

use std::sync::Arc;

use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;

use crate::counted_rails::{CountedMailboxes, CountedSessions, CountedTeachings};
use crate::fold::mail::FoldedMailboxes;
use crate::fold::sessions::FoldedSessions;
use crate::fold::teachings::FoldedTeachings;
use crate::fold_rails_battery::{
    MailRails, SessionRails, TeachRails, walk_every_kind_of_mail_write,
    walk_every_kind_of_session_write, walk_every_kind_of_teaching,
};

/// The session reads served from memory once the image has loaded. Each read is
/// added here in the commit that switches it.
const SWITCHED_SESSIONS: &[&str] = &["read_session", "sessions_of", "summaries_of", "all_summaries"];

/// The mail reads served from memory once the image has loaded.
const SWITCHED_MAIL: &[&str] = &[];

/// The teaching calls served from memory once the ledger has loaded.
const SWITCHED_TEACHINGS: &[&str] = &[];

async fn session_walk(switched: &[&str]) {
    let store = Arc::new(InMemorySessions::new());
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    let rails = SessionRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched,
    };
    walk_every_kind_of_session_write(&rails).await;
}

async fn mail_walk(switched: &[&str]) {
    let store = Arc::new(InMemoryMailboxes::new());
    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    let rails = MailRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched,
    };
    walk_every_kind_of_mail_write(&rails).await;
}

async fn teaching_walk(switched: &[&str]) {
    let store = Arc::new(InMemoryTeachings::new());
    let counted = Arc::new(CountedTeachings::new(store.clone()));
    let folded = FoldedTeachings::new(counted.clone());
    folded.rebuild().await.expect("the ledger loads");
    let twin = InMemoryTeachings::new();
    let rails = TeachRails {
        folded: &folded,
        counted: &counted,
        twin: &twin,
        switched,
    };
    walk_every_kind_of_teaching(&rails).await;
}

/// **The instrument first.** With nothing switched the walk compares an image
/// that forwards every read with the store, so it can only pass; a failure here
/// is the battery disagreeing with itself and says nothing about the image.
#[tokio::test]
async fn the_session_battery_agrees_with_an_image_that_serves_nothing_from_memory() {
    session_walk(&[]).await;
}

#[tokio::test]
async fn the_mail_battery_agrees_with_an_image_that_serves_nothing_from_memory() {
    mail_walk(&[]).await;
}

#[tokio::test]
async fn the_teaching_battery_agrees_with_an_image_that_serves_nothing_from_memory() {
    teaching_walk(&[]).await;
}

/// **After startup the switched reads make no call to the store, and answer as
/// the store does, after every kind of write.**
#[tokio::test]
async fn switched_session_reads_answer_from_memory_exactly_as_the_store_does() {
    session_walk(SWITCHED_SESSIONS).await;
}

#[tokio::test]
async fn switched_mail_reads_answer_from_memory_exactly_as_the_store_does() {
    mail_walk(SWITCHED_MAIL).await;
}

#[tokio::test]
async fn switched_teaching_calls_answer_from_memory_exactly_as_the_store_does() {
    teaching_walk(SWITCHED_TEACHINGS).await;
}
