//! Accepted writes finish publication even when their caller stops waiting.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::memory::EntityId;
use jojobot_domain::request::{self, Request, RequestId, Scope};
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::testing::InMemoryTeachings;
use jojobot_domain::teaching::{TeachingError, Teachings};
use tokio::sync::Notify;

use crate::fold::teachings::FoldedTeachings;

#[derive(Default)]
struct AfterCommit {
    store: InMemoryTeachings,
    committed: Notify,
    release: Notify,
    reads: AtomicUsize,
}

#[async_trait]
impl Teachings for AfterCommit {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        let first = self.store.first_contact(sid, domain, at).await?;
        if let Some(reservation) = request::take() {
            reservation.spent_with("the teaching landed".into());
        }
        self.committed.notify_one();
        self.release.notified().await;
        Ok(first)
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.store.forget(sid).await
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.store.has_contact(sid, domain).await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.store.taught().await
    }
}

#[tokio::test]
async fn writer_recovery_a_cancelled_caller_cannot_cut_publication_after_commit() {
    let store = Arc::new(AfterCommit::default());
    let folded = Arc::new(FoldedTeachings::new(store.clone()));
    folded.rebuild().await.expect("load ledger");
    store.reads.store(0, Ordering::SeqCst);
    let sid = Sid("alphaq".into());
    let control = Sid("betaqq".into());
    let scope = Scope::new(Request {
        bot: EntityId("bot:alpha".into()),
        id: RequestId::parse("cancel-after-commit").expect("valid request"),
        verb: "writer-recovery".into(),
        digest: "same arguments".into(),
    });
    let caller = {
        let folded = folded.clone();
        let sid = sid.clone();
        let scope = scope.clone();
        tokio::spawn(async move {
            request::within(
                scope,
                request::armed_part("teaching", Some("frozen plan".into()), async {
                    folded
                        .first_contact(&sid, "claims", Timestamp::UNIX_EPOCH)
                        .await
                }),
            )
            .await
        })
    };
    store.committed.notified().await;
    caller.abort();
    assert!(
        caller
            .await
            .expect_err("caller was cancelled")
            .is_cancelled()
    );
    store.release.notify_one();
    // A subsequent write cannot pass publication of the accepted earlier one.
    folded.forget(&control).await.expect("write fence");
    assert!(folded.has_contact(&sid, "claims").await.expect("published"));
    assert!(
        !folded
            .has_contact(&control, "claims")
            .await
            .expect("other run")
    );
    assert_eq!(
        store.reads.load(Ordering::SeqCst),
        0,
        "cancellation discarded the held ledger"
    );
    let recorded = scope
        .recorded("teaching")
        .expect("writer kept request part");
    assert_eq!(recorded.landed, "the teaching landed");
    assert_eq!(recorded.note.as_deref(), Some("frozen plan"));
}

/// A contact already in the ledger is a read. It must keep answering while an
/// unrelated write owns the task, without submitting another write behind it.
#[tokio::test]
async fn writer_recovery_a_known_teaching_read_passes_a_pending_write() {
    let store = Arc::new(AfterCommit::default());
    let control = Sid("betaqq".into());
    store
        .store
        .first_contact(&control, "claims", Timestamp::UNIX_EPOCH)
        .await
        .expect("seed control");
    let folded = Arc::new(FoldedTeachings::new(store.clone()));
    folded.rebuild().await.expect("load ledger");
    store.reads.store(0, Ordering::SeqCst);
    let caller = {
        let folded = folded.clone();
        tokio::spawn(async move {
            folded
                .first_contact(&Sid("alphaq".into()), "claims", Timestamp::UNIX_EPOCH)
                .await
        })
    };
    store.committed.notified().await;
    let read = folded.first_contact(&control, "claims", Timestamp::UNIX_EPOCH);
    tokio::pin!(read);
    std::future::poll_fn(|cx| {
        assert!(
            matches!(read.as_mut().poll(cx), std::task::Poll::Ready(Ok(false))),
            "a known teaching read queued behind an unrelated write"
        );
        std::task::Poll::Ready(())
    })
    .await;
    assert_eq!(
        store.reads.load(Ordering::SeqCst),
        0,
        "a known contact reread the ledger"
    );
    store.release.notify_one();
    assert!(
        caller
            .await
            .expect("caller finishes")
            .expect("write finishes")
    );
}

/// Opening a held box with no new messages moves nothing. The read keeps its
/// fast path while a different rail's write is still awaiting its commit reply.
#[tokio::test]
async fn writer_recovery_a_known_mailbox_read_passes_a_pending_write() {
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{Acting, Guarded, MailboxName, Mailboxes, TakenBy};
    jojobot_domain::memory::kinds::load_shipped();
    let writer = crate::writer::Writer::default();
    let mail_store = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let box_name = MailboxName("beta".into());
    mail_store
        .create_mailbox(&box_name, &EntityId("bot:alpha".into()), None)
        .await
        .expect("seed box");
    let mail = crate::fold::mail::FoldedMailboxes::new(mail_store).with_writer(writer.clone());
    mail.rebuild().await.expect("load board");
    let index = Arc::new(crate::search::FullTextIndex::with_owner(writer.clone()).unwrap());
    let mentioned = jojobot_domain::mailbox::mention::Mentioning::new(
        Arc::new(crate::changes::MailChanges::new(Arc::new(mail))),
        Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted()),
    );
    let mail = crate::search::IndexedMailboxes::new(Arc::new(mentioned), index);
    let reloads = writer.submitted_jobs();
    let store = Arc::new(AfterCommit::default());
    let teachings = FoldedTeachings::new(store.clone()).with_writer(writer.clone());
    let caller = tokio::spawn(async move {
        teachings
            .first_contact(&Sid("alphaq".into()), "claims", Timestamp::UNIX_EPOCH)
            .await
    });
    store.committed.notified().await;
    let read = mail.read_mailbox(&box_name, TakenBy::Reading, Acting::anyone_anytime());
    tokio::pin!(read);
    std::future::poll_fn(|cx| {
        assert!(matches!(read.as_mut().poll(cx), std::task::Poll::Ready(Ok(Guarded::Written(delivery))) if delivery.messages.is_empty()), "a held mailbox read queued behind an unrelated write");
        std::task::Poll::Ready(())
    }).await;
    assert_eq!(
        writer.submitted_jobs(),
        reloads + 1,
        "the read submitted a write"
    );
    store.release.notify_one();
    assert!(
        caller
            .await
            .expect("caller finishes")
            .expect("write finishes")
    );
}

/// A pending teaching commit owns the writer across rails, not just this rail's
/// mutex. Polling once makes acceptance deterministic, without a timer or a
/// scheduler race deciding whether the competing write got a turn.
async fn behind_teaching<F, W>(write: F)
where
    F: FnOnce(crate::writer::Writer) -> W,
    W: std::future::Future<Output = ()>,
{
    jojobot_domain::memory::kinds::load_shipped();
    let owner = crate::writer::Writer::default();
    let store = Arc::new(AfterCommit::default());
    let teachings = FoldedTeachings::new(store.clone()).with_writer(owner.clone());
    let first = tokio::spawn(async move {
        teachings
            .first_contact(&Sid("alphaq".into()), "claims", Timestamp::UNIX_EPOCH)
            .await
            .expect("first write finishes")
    });
    store.committed.notified().await;
    let other = write(owner.clone());
    tokio::pin!(other);
    std::future::poll_fn(|cx| {
        assert!(
            other.as_mut().poll(cx).is_pending(),
            "another rail passed the writer that owns the commit"
        );
        std::task::Poll::Ready(())
    })
    .await;
    assert_eq!(
        owner.submitted_jobs(),
        2,
        "the two rails submitted to different owners"
    );
    store.release.notify_one();
    other.await;
    assert!(first.await.expect("first caller finishes"));
}

#[tokio::test]
async fn writer_recovery_rail_memory_waits_for_teaching_publication() {
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Memory, NewEntity};
    behind_teaching(|writer| async move {
        let memory =
            crate::fold::Folded::new(Arc::new(InMemoryMemory::booted())).with_writer(writer);
        memory
            .add_entity(NewEntity::new(
                EntityId("person:alpha".into()),
                "Writer Queue",
                "user-named",
            ))
            .await
            .expect("memory write")
            .written()
            .expect("not blocked");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_rail_mail_waits_for_teaching_publication() {
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{MailboxName, Mailboxes};
    behind_teaching(|writer| async move {
        let mail = crate::fold::mail::FoldedMailboxes::new(Arc::new(
            InMemoryMailboxes::knowing_any_owner(),
        ))
        .with_writer(writer);
        mail.create_mailbox(
            &MailboxName("alpha".into()),
            &EntityId("bot:alpha".into()),
            None,
        )
        .await
        .expect("mail write")
        .written()
        .expect("not blocked");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_rail_sessions_waits_for_teaching_publication() {
    use jojobot_domain::clock::Clock;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::session::{NewSession, Sessions};
    behind_teaching(|writer| async move {
        let store = Arc::new(InMemorySessions::new());
        let sessions = crate::fold::sessions::FoldedSessions::new(
            store.clone(),
            store.write_stamp(),
            Clock::Real,
        )
        .with_writer(writer);
        sessions
            .begin(NewSession {
                bot: EntityId("bot:alpha".into()),
                sid: Sid("w001".into()),
                focus: "writer queue".into(),
                started_at: Timestamp::UNIX_EPOCH,
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("session write");
    })
    .await;
}

/// A reload replaces held state, so it must have the same owner as the writes
/// whose publication it could otherwise overtake.
#[tokio::test]
async fn writer_recovery_rebuild_memory_uses_the_process_writer() {
    behind_teaching(|writer| async move {
        let memory = crate::fold::Folded::new(Arc::new(
            jojobot_domain::memory::testing::InMemoryMemory::booted(),
        ))
        .with_writer(writer);
        memory.rebuild().await.expect("memory reload");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_rebuild_mail_uses_the_process_writer() {
    behind_teaching(|writer| async move {
        let mail = crate::fold::mail::FoldedMailboxes::new(Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ))
        .with_writer(writer);
        mail.rebuild().await.expect("mail reload");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_rebuild_sessions_uses_the_process_writer() {
    behind_teaching(|writer| async move {
        let store = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
        let sessions = crate::fold::sessions::FoldedSessions::new(
            store.clone(),
            store.write_stamp(),
            jojobot_domain::clock::Clock::Real,
        )
        .with_writer(writer);
        sessions.rebuild().await.expect("session reload");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_rebuild_teachings_uses_the_process_writer() {
    behind_teaching(|writer| async move {
        let teachings =
            FoldedTeachings::new(Arc::new(InMemoryTeachings::new())).with_writer(writer);
        teachings.rebuild().await.expect("teaching reload");
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_indexed_memory_uses_the_process_writer() {
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{Memory, NewEntity};
    behind_teaching(|writer| async move {
        let memory =
            crate::search::IndexedMemory::with_writer(Arc::new(InMemoryMemory::booted()), writer)
                .unwrap();
        memory
            .add_entity(NewEntity::new(
                EntityId("person:alpha".into()),
                "Alpha",
                "contract-fixture",
            ))
            .await
            .unwrap()
            .written()
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_indexed_mail_uses_the_process_writer() {
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{MailboxName, Mailboxes};
    behind_teaching(|writer| async move {
        let index = Arc::new(crate::search::FullTextIndex::with_owner(writer).unwrap());
        let mail = crate::search::IndexedMailboxes::new(
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            index,
        );
        mail.create_mailbox(
            &MailboxName("alpha".into()),
            &EntityId("bot:alpha".into()),
            None,
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    })
    .await;
}

// A closed pool makes an unowned native operation finish immediately. These
// cases assert queue acceptance before any I/O, without starting a store.
async fn closed_pool() -> sqlx::MySqlPool {
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .connect_lazy_with(sqlx::mysql::MySqlConnectOptions::new());
    pool.close().await;
    pool
}

#[tokio::test]
async fn writer_recovery_native_session_add_uses_the_process_writer() {
    use jojobot_domain::session::Sessions;
    use jojobot_domain::usage::{CallKey, CallOutcome};
    let pool = closed_pool().await;
    behind_teaching(move |writer| async move {
        let sessions = crate::dolt::sessions::DoltSessions::open(pool).with_writer(writer);
        let key = CallKey {
            bot: None,
            run: None,
            verb: "recall".into(),
            day: jiff::civil::date(2026, 8, 1),
            outcome: CallOutcome::Landed,
        };
        assert!(
            sessions.count_call(&key).await.is_err(),
            "the pool is closed"
        );
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_native_mail_uses_the_process_writer() {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
    let pool = closed_pool().await;
    behind_teaching(move |writer| async move {
        let owners = Arc::new(crate::owners::MemoryOwners::new(Arc::new(
            jojobot_domain::memory::testing::InMemoryMemory::booted(),
        )));
        let mail = crate::dolt::mailboxes::DoltMailboxes::open(pool, owners).with_writer(writer);
        let message = NewMessage {
            mailbox: MailboxName("alpha".into()),
            body: "queue".into(),
            subject: None,
            sender: "bot:alpha".into(),
            sent_at: Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        };
        assert!(
            mail.post_message(message).await.is_err(),
            "the pool is closed"
        );
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_native_outbox_uses_the_process_writer() {
    use jojobot_domain::mailbox::MessageId;
    use jojobot_domain::push::Outbox;
    let pool = closed_pool().await;
    behind_teaching(move |writer| async move {
        let owners = Arc::new(crate::owners::MemoryOwners::new(Arc::new(
            jojobot_domain::memory::testing::InMemoryMemory::booted(),
        )));
        let mail = crate::dolt::mailboxes::DoltMailboxes::open(pool, owners).with_writer(writer);
        assert!(
            mail.failed(&MessageId("m001".into()), 1, Timestamp::UNIX_EPOCH)
                .await
                .is_err(),
            "the pool is closed"
        );
    })
    .await;
}

#[tokio::test]
async fn writer_recovery_native_request_receipt_uses_the_process_writer() {
    use jojobot_domain::request::Requests;
    let pool = closed_pool().await;
    behind_teaching(move |writer| async move {
        let requests = crate::dolt::requests::DoltRequests::new(pool).with_writer(writer);
        let request = Request {
            bot: EntityId("bot:alpha".into()),
            id: RequestId::parse("closed-pool").unwrap(),
            verb: "capture".into(),
            digest: "same arguments".into(),
        };
        assert!(
            requests.fill(&request, "{}").await.is_err(),
            "the pool is closed"
        );
    })
    .await;
}
