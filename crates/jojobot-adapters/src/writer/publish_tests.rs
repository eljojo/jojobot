//! What the writer owes its subscribers: each write's changes once, in the order
//! the State recorded them, after the commit, whatever happens to the caller, and
//! one job at a time.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
use jojobot_domain::memory::EntityId;
use tokio::sync::Notify;

use super::Writer;
use super::publish::{Change, Subscriber};
use crate::fold::mail::FoldedMailboxes;

/// Keeps every batch it was handed, one entry per delivery.
#[derive(Default)]
struct Batches(Mutex<Vec<Vec<Change>>>);

#[async_trait]
impl Subscriber for Batches {
    async fn changed(&self, changes: &[Change]) {
        self.0.lock().expect("batches").push(changes.to_vec());
    }
}

fn a_post(to: &str) -> NewMessage {
    NewMessage {
        mailbox: MailboxName(to.into()),
        body: "a body".into(),
        subject: None,
        sender: "gamma".into(),
        sent_at: Timestamp::UNIX_EPOCH,
        in_reply_to: None,
        supersedes: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
    }
}

/// **A mail write reaches its subscriber once, with the messages before the
/// position that closes them.** The index needs the position after the messages it
/// counts. Two posts make two deliveries, so a delivery never repeats; a write that
/// records nothing delivers nothing. Travels the real mail State, not a hand-made
/// change.
#[tokio::test]
async fn a_mail_write_is_delivered_once_with_its_messages_before_its_position() {
    jojobot_domain::memory::kinds::load_shipped();
    let store = InMemoryMailboxes::knowing_any_owner();
    store
        .create_mailbox(
            &MailboxName("epsilon".into()),
            &EntityId("bot:epsilon".into()),
            None,
        )
        .await
        .expect("a box");
    let writer = Writer::default();
    let boxes = FoldedMailboxes::new(Arc::new(store)).with_writer(writer.clone());
    let seen = Arc::new(Batches::default());
    writer.subscribe(seen.clone());

    boxes
        .post_message(a_post("epsilon"))
        .await
        .expect("first post");
    writer.drain().await;
    assert_eq!(
        seen.0.lock().expect("batches").len(),
        1,
        "one delivery for one post"
    );

    boxes
        .post_message(a_post("epsilon"))
        .await
        .expect("second post");
    writer.drain().await;
    let batches = seen.0.lock().expect("batches").clone();
    assert_eq!(
        batches.len(),
        2,
        "one delivery per post, none repeated: {batches:?}"
    );
    for batch in &batches {
        let messages = batch
            .iter()
            .position(|change| matches!(change, Change::Messages(_)))
            .expect("the post published its message");
        let position = batch
            .iter()
            .position(|change| matches!(change, Change::MailPosition(_)))
            .expect("the post published its position");
        assert!(
            messages < position,
            "messages come before the position: {batch:?}"
        );
    }
    assert_ne!(
        batches[0].last(),
        batches[1].last(),
        "the position moves with each write"
    );
}

/// **An accepted write is published after the caller stops waiting.** The work
/// holds at a gate, the caller is dropped, and the gate opens: the subscriber still
/// gets the change. Paired with the delivery above, where the caller waits.
#[tokio::test]
async fn a_write_whose_caller_stopped_waiting_is_still_published() {
    let writer = Writer::default();
    let seen = Arc::new(Batches::default());
    writer.subscribe(seen.clone());
    let reached = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());

    let caller = tokio::spawn({
        let (writer, reached, gate) = (writer.clone(), reached.clone(), gate.clone());
        async move {
            let inner = writer.clone();
            writer
                .run(async move {
                    reached.notify_one();
                    gate.notified().await;
                    inner.publish(Change::MailPosition(7));
                })
                .await;
        }
    });
    reached.notified().await;
    caller.abort();
    assert!(
        caller
            .await
            .expect_err("the caller was dropped")
            .is_cancelled()
    );
    gate.notify_one();
    writer.drain().await;

    assert_eq!(
        *seen.0.lock().expect("batches"),
        vec![vec![Change::MailPosition(7)]],
        "the write finished and was published without its caller"
    );
}

struct Panicking(AtomicBool);

#[async_trait]
impl Subscriber for Panicking {
    async fn changed(&self, _: &[Change]) {
        if self.0.swap(false, Ordering::SeqCst) {
            panic!("a subscriber failed");
        }
    }
}

/// **A subscriber that panics is loud and not fatal to the writer.** The write has
/// committed by then, so the State holds it; the caller of that write fails
/// instead of being told the write did not happen quietly, and the next write is
/// served. A change recorded by a job that panicked before its delivery is not
/// lost: it rides out with the next delivery, ahead of that job's own changes.
#[tokio::test]
async fn a_subscriber_panic_fails_its_caller_after_the_commit_and_the_writer_serves_on() {
    let writer = Writer::default();
    let panicking = Arc::new(Panicking(AtomicBool::new(true)));
    let seen = Arc::new(Batches::default());
    writer.subscribe(panicking);
    writer.subscribe(seen.clone());
    let committed = Arc::new(AtomicBool::new(false));

    let failed = tokio::spawn({
        let (writer, committed) = (writer.clone(), committed.clone());
        async move {
            let inner = writer.clone();
            writer
                .run(async move {
                    committed.store(true, Ordering::SeqCst);
                    inner.publish(Change::MailPosition(1));
                })
                .await;
        }
    })
    .await;
    assert!(failed.is_err(), "the caller of the write is told it failed");
    assert!(
        committed.load(Ordering::SeqCst),
        "the write itself had committed"
    );

    let inner = writer.clone();
    writer
        .run(async move { inner.publish(Change::MailPosition(2)) })
        .await;
    assert_eq!(
        *seen.0.lock().expect("batches"),
        vec![vec![Change::MailPosition(1)], vec![Change::MailPosition(2)]],
        "a subscriber after the panicking one was told both writes, and the writer served the next write"
    );
}

/// **A job that panics after it recorded and before its delivery loses nothing.**
/// What it recorded is delivered with the next job's changes, first.
#[tokio::test]
async fn a_change_recorded_by_a_job_that_panicked_rides_out_with_the_next_delivery() {
    let writer = Writer::default();
    let seen = Arc::new(Batches::default());
    writer.subscribe(seen.clone());

    let inner = writer.clone();
    let failed = tokio::spawn({
        let writer = writer.clone();
        async move {
            writer
                .run(async move {
                    inner.publish(Change::MailPosition(1));
                    panic!("the job failed after it recorded");
                })
                .await
        }
    })
    .await;
    assert!(failed.is_err(), "the caller is told the job failed");
    assert!(
        seen.0.lock().expect("batches").is_empty(),
        "nothing delivered yet"
    );

    let inner = writer.clone();
    writer
        .run(async move { inner.publish(Change::MailPosition(2)) })
        .await;
    assert_eq!(
        *seen.0.lock().expect("batches"),
        vec![vec![Change::MailPosition(1), Change::MailPosition(2)]]
    );
}

/// **The writer runs one job at a time.** The index counts changes as a run
/// `first..=last` and depends on it. Two jobs are submitted while the first holds
/// at a gate; the second must not start until the first has ended.
#[tokio::test]
async fn two_submitted_jobs_never_run_at_the_same_time() {
    let writer = Writer::default();
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let first_in = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());

    let job = |hold: Option<(Arc<Notify>, Arc<Notify>)>| {
        let (running, most) = (running.clone(), most.clone());
        async move {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            if let Some((entered, gate)) = hold {
                entered.notify_one();
                gate.notified().await;
            }
            tokio::task::yield_now().await;
            running.fetch_sub(1, Ordering::SeqCst);
        }
    };
    let first = tokio::spawn({
        let writer = writer.clone();
        let work = job(Some((first_in.clone(), gate.clone())));
        async move { writer.run(work).await }
    });
    first_in.notified().await;
    let second = tokio::spawn({
        let writer = writer.clone();
        let work = job(None);
        async move { writer.run(work).await }
    });
    while writer.submitted_jobs() < 2 {
        tokio::task::yield_now().await;
    }
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    gate.notify_one();
    first.await.expect("first job");
    second.await.expect("second job");

    assert_eq!(
        most.load(Ordering::SeqCst),
        1,
        "two jobs were in flight together"
    );
}
