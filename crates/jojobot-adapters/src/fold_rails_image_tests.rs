//! **The in-memory image of sessions, mail and teachings (decision log 292),
//! held to the store it mirrors.**
//!
//! The walks in [`crate::fold_rails_battery`] write every kind of write through
//! the image and, after each, ask every read of the image and of the store.
//! These cases run them over the fakes; `tests/fold_rails_image_dolt.rs` runs
//! the same walks over the real store.

use std::sync::Arc;

use jojobot_domain::clock::Clock;
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
const SWITCHED_SESSIONS: &[&str] = &[
    "read_session",
    "sessions_of",
    "summaries_of",
    "all_summaries",
    "summary_of",
    "run_counts",
    "all_sessions",
    "charter_change_of",
    "session_history",
    "journal_history",
    "scan_session_history",
];

/// The mail reads served from memory once the image has loaded.
const SWITCHED_MAIL: &[&str] = &[
    "list_mailboxes",
    "scan_messages",
    "message_by_id",
    "sent_by",
    "read_mailbox",
    "read_mailbox",
    "message_history",
];

/// The teaching calls served from memory once the ledger has loaded.
const SWITCHED_TEACHINGS: &[&str] = &["first_contact"];

async fn session_walk(switched: &[&str]) {
    let store = Arc::new(InMemorySessions::new());
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone(), store.write_stamp(), Clock::Real);
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

/// **An event the image appends itself is numbered from one, in order, and says
/// who made it.** The battery holds the image to the store, and both make their
/// events with the same function, so this case pins the values by literal: a
/// number that was wrong in both would pass the battery.
#[tokio::test]
async fn an_event_the_image_appends_is_numbered_and_attributed_as_written() {
    use jojobot_domain::mailbox::{
        Acting, MailboxName, Mailboxes, MessageEventKind, MessageState, NewMessage, TakenBy,
    };
    jojobot_domain::memory::kinds::load_shipped();
    let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let folded = FoldedMailboxes::new(store);
    folded.rebuild().await.expect("the image loads");
    let at = |second: i64| {
        jiff::Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
    };
    let inbox = MailboxName("gamma".into());
    folded
        .create_mailbox(
            &inbox,
            &jojobot_domain::memory::EntityId("bot:gamma".into()),
            None,
        )
        .await
        .expect("create")
        .written()
        .expect("not blocked");
    let posted = folded
        .post_message(NewMessage {
            mailbox: inbox.clone(),
            body: "one".into(),
            subject: None,
            sender: "delta".into(),
            sent_at: at(0),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    folded
        .read_mailbox(&inbox, TakenBy::Reading, Acting::by("run-a", at(5)))
        .await
        .expect("delivery");
    folded
        .mark_processed(&posted.id, Some("  done  "), Acting::by("run-b", at(6)))
        .await
        .expect("processed");

    let history = folded
        .message_history(&posted.id)
        .await
        .expect("the image answers");

    assert_eq!(
        history.iter().map(|e| e.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(history[0].kind, MessageEventKind::Read);
    assert_eq!(history[0].by_session.as_deref(), Some("run-a"));
    assert_eq!(history[0].prior_state, MessageState::New);
    assert_eq!(history[1].kind, MessageEventKind::Processed);
    assert_eq!(history[1].by_session.as_deref(), Some("run-b"));
    assert_eq!(history[1].prior_state, MessageState::Read);
    assert_eq!(history[1].notes.as_deref(), Some("done"));
}
