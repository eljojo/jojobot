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
    "call_counts",
    "run_calls",
    "served_days",
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
    "messages_by_id",
    "sent_by",
    "read_mailbox",
    "read_mailbox",
    "message_history",
];

/// The teaching calls served from memory once the ledger has loaded.
const SWITCHED_TEACHINGS: &[&str] = &["first_contact"];

async fn session_walk(switched: &[&str]) {
    session_walk_over(Arc::new(InMemorySessions::new()), switched).await;
}

async fn session_walk_over(store: Arc<InMemorySessions>, switched: &[&str]) {
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

/// **A store that keeps no list of its charter changes still has its runs in
/// memory.** The image holds the runs and their histories whatever the store can
/// list, so a write refills nothing and every switched read but the charter
/// change, which is asked of the store bot by bot, makes no call to the store.
#[tokio::test]
async fn a_store_with_no_charter_list_still_serves_the_other_session_reads_from_memory() {
    let store = Arc::new(InMemorySessions::new().without_a_charter_list());
    let switched: Vec<&str> = SWITCHED_SESSIONS
        .iter()
        .copied()
        .filter(|read| *read != "charter_change_of")
        .collect();
    session_walk_over(store, &switched).await;
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
        .mark_processed(
            &posted.id,
            &posted.mailbox,
            Some("  done  "),
            Acting::by("run-b", at(6)),
        )
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

/// **How the first begin's answer is lost after its commit landed.**
#[derive(Clone, Copy)]
enum Lost {
    /// The caller stops waiting: the call is cancelled.
    Cancelled,
    /// The store answers with a failure: the outcome is not acknowledged.
    Unacknowledged,
}

/// A session store whose **first begin commits and then loses its answer**, in
/// one of the two ways [`Lost`] names. Every other call goes to the store.
struct CommitsThenLoses {
    inner: Arc<InMemorySessions>,
    lost: Lost,
    once: std::sync::atomic::AtomicBool,
    release: tokio::sync::Notify,
}

#[jojobot_domain::forward(jojobot_domain::forward_sessions, via = inner)]
impl jojobot_domain::session::Sessions for CommitsThenLoses {
    async fn begin(
        &self,
        new: jojobot_domain::session::NewSession,
    ) -> Result<jojobot_domain::session::Session, jojobot_domain::session::SessionError> {
        let begun = self.inner.begin(new).await?;
        if !self.once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            match self.lost {
                Lost::Cancelled => self.release.notified().await,
                Lost::Unacknowledged => {
                    return Err(jojobot_domain::session::SessionError::Store(
                        "the row committed and the answer did not arrive".into(),
                    ));
                }
            }
        }
        Ok(begun)
    }
}

/// Lose the first begin's answer the way `lost` says, retry it with the same
/// sid through the image, and hand back what the store and the image hold.
async fn begin_lost_then_retried(
    lost: Lost,
) -> (
    Vec<jojobot_domain::session::history::SessionChange>,
    Vec<jojobot_domain::session::history::SessionChange>,
) {
    use jojobot_domain::session::{NewSession, Sessions, Sid};
    let store = Arc::new(InMemorySessions::new());
    let losing = Arc::new(CommitsThenLoses {
        inner: store.clone(),
        lost,
        once: std::sync::atomic::AtomicBool::new(false),
        release: tokio::sync::Notify::new(),
    });
    let folded = FoldedSessions::new(losing.clone(), store.write_stamp(), Clock::Real);
    folded.rebuild().await.expect("the image loads");
    let new = || NewSession {
        bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
        sid: Sid("s001".into()),
        focus: "reading the hand-off".into(),
        started_at: jiff::Timestamp::from_second(1_780_000_000).expect("a valid instant"),
        timezone: None,
        started_on: None,
        id: None,
        key_hash: None,
    };
    match lost {
        Lost::Cancelled => {
            let waited =
                tokio::time::timeout(std::time::Duration::from_millis(50), folded.begin(new()))
                    .await;
            assert!(waited.is_err(), "the first begin is cancelled: {waited:?}");
            losing.release.notify_one();
            folded.wait_for_writes().await;
        }
        Lost::Unacknowledged => {
            let failed = folded.begin(new()).await;
            assert!(failed.is_err(), "the first begin fails: {failed:?}");
        }
    }
    let landed = store
        .sessions_of(&jojobot_domain::memory::EntityId("bot:gamma".into()))
        .await
        .expect("the store answers");
    assert_eq!(landed.len(), 1, "the first begin committed: {landed:?}");

    let retried = folded
        .begin(new())
        .await
        .expect("the retry hands the run back");

    assert_eq!(retried.id, landed[0].id, "the retry is the same run");
    let in_store = store
        .session_history(&retried.id)
        .await
        .expect("the store answers");
    let in_image = folded
        .session_history(&retried.id)
        .await
        .expect("the image answers");
    (in_store, in_image)
}

/// **A begin retried after a cancelled or unacknowledged commit records no
/// second Begin.** The first begin committed in the store and its answer never
/// reached the image; the retry under the same sid writes nothing in the store.
/// The image then holds the store's one Begin, at the moment of the commit.
#[tokio::test]
async fn a_begin_retried_after_an_uncertain_commit_holds_the_one_begin_the_store_holds() {
    use jojobot_domain::session::history::{ChangeKind, SessionChange};
    let begins = |changes: &[SessionChange]| {
        changes
            .iter()
            .filter(|c| c.changed == Some(ChangeKind::Begin))
            .count()
    };
    for lost in [Lost::Cancelled, Lost::Unacknowledged] {
        let (in_store, in_image) = begin_lost_then_retried(lost).await;
        assert_eq!(begins(&in_store), 1, "the store holds one Begin");
        assert_eq!(begins(&in_image), 1, "the image holds one Begin");
        assert_eq!(
            in_image, in_store,
            "the image holds the store's Begin, at the moment of the commit"
        );
    }
}

/// **A first fill that failed is filled again by the next write.** The store was
/// unreachable when the server booted and recovered a moment later. After one
/// write the next read comes from the image (the store counts none) and carries
/// that write.
#[tokio::test]
async fn a_sessions_image_that_failed_to_fill_is_filled_again_by_the_next_write() {
    use jojobot_domain::session::{NewSession, Sessions, Sid};
    let store = Arc::new(InMemorySessions::new());
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone(), store.write_stamp(), Clock::Real);
    counted.calls.set_unreachable(true);
    folded
        .rebuild()
        .await
        .expect_err("the store cannot be reached");
    counted.calls.set_unreachable(false);

    let begun = folded
        .begin(NewSession {
            bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
            sid: Sid("s001".into()),
            focus: "reading the hand-off".into(),
            started_at: jiff::Timestamp::from_second(1_780_000_000).expect("a valid instant"),
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("the begin lands");

    counted.calls.reset();
    let read = folded
        .read_session(&begun.id)
        .await
        .expect("the image answers");
    assert_eq!(counted.calls.reads(), 0, "{:?}", counted.calls.seen());
    assert_eq!(read.focus, "reading the hand-off");
}

#[tokio::test]
async fn a_mail_image_that_failed_to_fill_is_filled_again_by_the_next_write() {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
    jojobot_domain::memory::kinds::load_shipped();
    let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    counted.calls.set_unreachable(true);
    folded
        .rebuild()
        .await
        .expect_err("the store cannot be reached");
    counted.calls.set_unreachable(false);

    folded
        .create_mailbox(
            &MailboxName("alpha".into()),
            &jojobot_domain::memory::EntityId("bot:alpha".into()),
            None,
        )
        .await
        .expect("the box is made");
    let posted = folded
        .post_message(NewMessage {
            mailbox: MailboxName("alpha".into()),
            body: "written after the store came back".into(),
            subject: None,
            sender: "beta".into(),
            sent_at: jiff::Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("the post lands")
        .written()
        .expect("nothing blocks it");

    counted.calls.reset();
    let read = folded
        .message_by_id(&posted.id)
        .await
        .expect("the image answers")
        .expect("the message is held");
    assert_eq!(counted.calls.reads(), 0, "{:?}", counted.calls.seen());
    assert_eq!(read.body, "written after the store came back");
}

#[tokio::test]
async fn a_teaching_ledger_that_failed_to_fill_is_filled_again_by_the_next_write() {
    use jojobot_domain::session::Sid;
    use jojobot_domain::teaching::Teachings;
    let store = Arc::new(InMemoryTeachings::new());
    let counted = Arc::new(CountedTeachings::new(store.clone()));
    let folded = FoldedTeachings::new(counted.clone());
    counted.calls.set_unreachable(true);
    folded
        .rebuild()
        .await
        .expect_err("the store cannot be reached");
    counted.calls.set_unreachable(false);

    let sid = Sid("s001".into());
    let at = jiff::Timestamp::UNIX_EPOCH;
    assert!(
        folded
            .first_contact(&sid, "claims", at)
            .await
            .expect("the write lands"),
        "the first contact is a first contact"
    );

    counted.calls.reset();
    assert!(
        !folded
            .first_contact(&sid, "claims", at)
            .await
            .expect("the ledger answers"),
        "the second contact is not"
    );
    assert_eq!(
        (counted.calls.reads(), counted.calls.writes()),
        (0, 0),
        "the repeat reached the store: {:?}",
        counted.calls.seen()
    );
}

/// **A write whose future is dropped after the store committed leaves the next
/// read correct.** The runs image: an entry was appended in the store and the
/// caller left before the image followed it.
#[tokio::test]
async fn a_dropped_append_leaves_the_next_read_of_the_run_correct() {
    use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};
    let store = Arc::new(InMemorySessions::new());
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone(), store.write_stamp(), Clock::Real);
    folded.rebuild().await.expect("the image loads");
    let begun = folded
        .begin(NewSession {
            bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
            sid: Sid("s001".into()),
            focus: "reading the hand-off".into(),
            started_at: jiff::Timestamp::from_second(1_780_000_000).expect("a valid instant"),
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("the begin lands");

    counted.calls.hang_after_next("append");
    let waited = crate::fold_dropped_tests::cancelled_after_commit(
        counted.calls.hung(),
        folded.append(
            &begun.id,
            NewEntry::manual(
                "written, then the caller left",
                jiff::Timestamp::from_second(1_780_000_100).expect("a valid instant"),
                None,
            ),
        ),
    )
    .await;
    assert!(waited.is_err(), "the append is cancelled: {waited:?}");
    counted.calls.release_hung();
    folded.wait_for_writes().await;
    let in_store = store
        .read_session(&begun.id)
        .await
        .expect("the store answers");
    assert_eq!(
        in_store.entries.len(),
        1,
        "the append committed in the store"
    );

    let in_image = folded
        .read_session(&begun.id)
        .await
        .expect("the image answers");
    assert_eq!(
        in_image.entries, in_store.entries,
        "the image misses a committed append"
    );
}

/// The mail image: a message was posted in the store and the caller left.
#[tokio::test]
async fn a_dropped_post_leaves_the_next_read_of_the_board_correct() {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
    jojobot_domain::memory::kinds::load_shipped();
    let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    folded
        .create_mailbox(
            &MailboxName("alpha".into()),
            &jojobot_domain::memory::EntityId("bot:alpha".into()),
            None,
        )
        .await
        .expect("the box is made");

    counted.calls.hang_after_next("post_message");
    let waited = crate::fold_dropped_tests::cancelled_after_commit(
        counted.calls.hung(),
        folded.post_message(NewMessage {
            mailbox: MailboxName("alpha".into()),
            body: "written, then the caller left".into(),
            subject: None,
            sender: "beta".into(),
            sent_at: jiff::Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        }),
    )
    .await;
    assert!(waited.is_err(), "the post is cancelled: {waited:?}");
    counted.calls.release_hung();
    folded.wait_for_writes().await;
    let in_store = store.scan_messages().await.expect("the store answers");
    assert_eq!(in_store.len(), 1, "the post committed in the store");

    let in_image = folded.scan_messages().await.expect("the image answers");
    assert_eq!(
        in_image.len(),
        in_store.len(),
        "the image misses a committed post"
    );
}

/// The teaching ledger: the store forgot a session and the caller left, so the
/// ledger must not go on saying the session was told.
#[tokio::test]
async fn a_dropped_forget_leaves_the_next_first_contact_correct() {
    use jojobot_domain::session::Sid;
    use jojobot_domain::teaching::Teachings;
    let store = Arc::new(InMemoryTeachings::new());
    let counted = Arc::new(CountedTeachings::new(store.clone()));
    let folded = FoldedTeachings::new(counted.clone());
    folded.rebuild().await.expect("the ledger loads");
    let sid = Sid("s001".into());
    let at = jiff::Timestamp::UNIX_EPOCH;
    assert!(
        folded
            .first_contact(&sid, "claims", at)
            .await
            .expect("lands")
    );
    assert!(
        !folded
            .first_contact(&sid, "claims", at)
            .await
            .expect("held"),
        "the ledger holds the pair before the forget"
    );

    counted.calls.hang_after_next("forget");
    let waited = crate::fold_dropped_tests::cancelled_after_commit(
        counted.calls.hung(),
        folded.forget(&sid),
    )
    .await;
    assert!(waited.is_err(), "the forget is cancelled: {waited:?}");
    counted.calls.release_hung();
    folded.wait_for_writes().await;

    assert!(
        folded
            .first_contact(&sid, "claims", at)
            .await
            .expect("answers"),
        "the store forgot the session, so its next contact is a first one again"
    );
}
