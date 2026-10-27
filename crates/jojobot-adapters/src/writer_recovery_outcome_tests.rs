//! Native outcomes keep publication separate from the caller's receipt.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};

use crate::dolt::Dolt;
use crate::dolt::teaching::DoltTeachings;
use crate::fold::teachings::FoldedTeachings;
use crate::testing::{Scratch, free_port, migrated_database};

#[tokio::test]
async fn writer_recovery_an_unheld_read_refuses_until_the_native_rail_recovers() {
    use crate::fold::sessions::FoldedSessions;
    use crate::{counted_rails::CountedSessions, dolt::sessions::DoltSessions};
    use jojobot_domain::session::{NewSession, SessionError, Sessions, history::WriteStamp};
    use jojobot_domain::{clock::Clock, memory::EntityId};

    jojobot_domain::memory::kinds::load_shipped();
    let scratch = Scratch::new("writer-unheld-read");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let pool = migrated_database(&server, "reload_delta")
        .await
        .expect("schema");
    let writer = crate::writer::Writer::default();
    let stamp = WriteStamp::new();
    let native = Arc::new(
        DoltSessions::open(pool)
            .stamped_by(stamp.clone())
            .with_writer(writer.clone()),
    );
    let minted = jojobot_domain::session::MintedRun {
        name: Sid("cd34".into()),
        key_hash: "b".repeat(64),
        bot: EntityId("bot:alpha".into()),
        zone: None,
        day: None,
        minted_at: Timestamp::UNIX_EPOCH,
    };
    native.note_minted(&minted).await.expect("an uncarded run");
    let counted = Arc::new(CountedSessions::new(native.clone()));
    let folded = Arc::new(
        FoldedSessions::new(counted.clone(), stamp, Clock::Real).with_writer(writer.clone()),
    );
    folded.rebuild().await.expect("last published image");
    counted.calls.lose_next_begin_reply();
    counted.calls.hang_after_next("all_sessions");
    let mut caller = {
        let folded = folded.clone();
        tokio::spawn(async move {
            folded
                .begin(NewSession {
                    bot: EntityId("bot:alpha".into()),
                    sid: Sid("ab12".into()),
                    focus: "Alpha".into(),
                    started_at: Timestamp::UNIX_EPOCH,
                    timezone: None,
                    started_on: None,
                    id: None,
                    key_hash: None,
                })
                .await
        })
    };
    tokio::select! {
        () = counted.calls.hung() => {},
        answer = &mut caller => panic!("the native rail did not enter recovery: {answer:?}"),
    }
    assert!(writer.rail_uncertain("runs"));
    counted.calls.reset();
    assert!(
        matches!(folded.write_summary().await, Err(SessionError::Store(_))),
        "the unheld write count crossed into the uncertain store"
    );
    assert!(
        matches!(
            folded.minted_without_a_card().await,
            Err(SessionError::Store(_))
        ),
        "an unheld read crossed into the uncertain store"
    );
    assert_eq!(
        folded.charter_changes().await.expect("held ledger"),
        Some(Default::default())
    );
    assert_eq!(
        folded
            .charter_change_of(&EntityId("bot:alpha".into()))
            .await
            .expect("held miss"),
        None
    );
    assert_eq!(
        counted.calls.reads(),
        0,
        "the refusal or held reads reached source"
    );
    counted.calls.release_hung();
    assert!(matches!(
        caller.await.expect("caller ends"),
        Err(SessionError::Indeterminate)
    ));
    assert!(!writer.rail_uncertain("runs"));
    assert_eq!(writer.recovery_counts().recovered, 1);
    assert!(writer.has_native_connection());
    counted.calls.reset();
    assert_eq!(
        folded
            .minted_without_a_card()
            .await
            .expect("the unheld read serves after recovery"),
        vec![minted]
    );
    assert_eq!(
        counted.calls.reads(),
        1,
        "known-state source behavior changed"
    );
    counted.calls.reset();
    assert_eq!(
        folded.write_summary().await.expect("recovered write count"),
        native.write_summary().await.expect("native write count")
    );
    assert_eq!(counted.calls.reads(), 1);
    let stored = native.all_sessions().await.expect("native run");
    assert_eq!(stored.len(), 1);
    counted.calls.reset();
    assert_eq!(folded.all_sessions().await.expect("published run"), stored);
    assert_eq!(counted.calls.reads(), 0);
    server.stop().await;
}

#[tokio::test]
async fn writer_recovery_memory_reloads_the_whole_rail_and_reports_its_time() {
    use crate::{counted::Counted, dolt::memory::DoltMemory, fold::Folded};
    use jojobot_domain::memory::testing::contract::state;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    jojobot_domain::memory::kinds::load_shipped();
    let scratch = Scratch::new("writer-memory-reload");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let pool = migrated_database(&server, "reload_gamma")
        .await
        .expect("schema");
    let native = Arc::new(DoltMemory::open(pool));
    jojobot_domain::memory::kinds::seed(&*native)
        .await
        .expect("kinds");
    let date = jiff::civil::date(2026, 9, 1);
    let mut seeded = Vec::new();
    for name in [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "kappa", "theta",
    ] {
        let id = EntityId(format!("topic:{name}"));
        let new = NewEntity::new(id.clone(), name, "writer-recovery");
        let mut added = native.add_entity(new.clone()).await.expect("entity");
        if let jojobot_domain::memory::Guarded::Blocked {
            attempted,
            candidates,
        } = added
        {
            added = native
                .add_entity(NewEntity {
                    override_token: Some(jojobot_domain::memory::guard::override_token(
                        &attempted,
                        &candidates,
                    )),
                    override_reason: Some(
                        "different Greek entities in the recovery fixture".into(),
                    ),
                    ..new
                })
                .await
                .expect("the refusal's token lifts the screen");
        }
        assert!(
            matches!(added, jojobot_domain::memory::Guarded::Written(_)),
            "{id}: {added:?}"
        );
        for number in 0..10 {
            native
                .capture(state(NewFact::about(
                    id.clone(),
                    format!("{name} {number}"),
                    date,
                )))
                .await
                .expect("claim")
                .written()
                .expect("captured");
        }
        seeded.push(id);
    }
    let counted = Arc::new(Counted::new(native.clone()));
    let writer = crate::writer::Writer::default();
    let image = Arc::new(Folded::new(counted.clone()).with_writer(writer.clone()));
    image.rebuild().await.expect("published rail");
    let folded = crate::search::IndexedMemory::with_writer(image, writer.clone())
        .expect("the production search decorator");
    folded.rebuild().await.expect("published index");
    let epoch = folded.index().names_epoch();
    counted.lose_next_capture_reply();
    let error = folded
        .capture(state(NewFact::about(
            seeded[0].clone(),
            "Alpha recovered",
            date,
        )))
        .await
        .expect_err("receipt lost");
    assert!(
        matches!(error, jojobot_domain::memory::MemoryError::Indeterminate),
        "{error}"
    );
    assert!(
        folded.index().names_epoch() > epoch,
        "recovery did not invalidate dependent name caches"
    );
    let stored = native.recall(&seeded[0]).await.expect("source");
    assert_eq!(stored.len(), 11);
    counted.reset();
    for id in &seeded {
        assert_eq!(
            folded.recall(id).await.expect("published"),
            native.recall(id).await.expect("source")
        );
    }
    assert_eq!(counted.calls(), 0, "recovery discarded the image");
    let counts = writer.recovery_counts();
    assert!(writer.has_native_connection());
    assert_eq!(
        (counts.attempted, counts.recovered, counts.failed),
        (1, 1, 0)
    );
    assert!(counts.reload_micros > 0);
    println!(
        "memory reload: {} entities, 81 claims, {} microseconds",
        seeded.len(),
        counts.reload_micros
    );
    server.stop().await;
}

struct RolledBack {
    inner: DoltTeachings,
    pool: crate::writer::sql::Pool,
    reads: AtomicUsize,
}

#[async_trait]
impl Teachings for RolledBack {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        let mut tx = self.pool.begin().await.expect("begin the real transaction");
        sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
            .bind(sid.as_str())
            .bind(domain)
            .bind(at.to_string())
            .execute(&mut *tx)
            .await
            .expect("the write is pending in the transaction");
        tx.rollback()
            .await
            .expect("the server acknowledges rollback");
        // A store error alone must not erase the acknowledged rollback.
        Err(TeachingError::Store(
            "the transaction was rolled back".into(),
        ))
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.inner.forget(sid).await
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.has_contact(sid, domain).await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.taught().await
    }
}

#[tokio::test]
async fn writer_recovery_a_definite_native_rollback_keeps_the_published_rail() {
    let scratch = Scratch::new("writer-rollback");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store starts");
    let pool = migrated_database(&server, "rollback_alpha")
        .await
        .expect("the native schema");
    let native = DoltTeachings::open(pool.clone());
    let sid = Sid("alphaq".into());
    let control = Sid("betaqq".into());
    native
        .first_contact(&control, "claims", Timestamp::UNIX_EPOCH)
        .await
        .expect("seed the published contact");
    let store = Arc::new(RolledBack {
        inner: native.clone(),
        pool: pool.into(),
        reads: AtomicUsize::new(0),
    });
    let folded = FoldedTeachings::new(store.clone());
    folded.rebuild().await.expect("boot the rail");
    store.reads.store(0, Ordering::SeqCst);
    let refused = folded
        .first_contact(&sid, "claims", Timestamp::UNIX_EPOCH)
        .await
        .expect_err("the transaction did not commit");
    assert!(matches!(refused, TeachingError::Store(_)));
    assert!(
        !native
            .has_contact(&sid, "claims")
            .await
            .expect("read native"),
        "the acknowledged rollback really left no row"
    );
    assert!(folded.has_contact(&control, "claims").await.expect("held"));
    assert!(
        !folded
            .has_contact(&sid, "claims")
            .await
            .expect("not written")
    );
    assert_eq!(
        store.reads.load(Ordering::SeqCst),
        0,
        "a definite rollback discarded the published rail"
    );
    server.stop().await;
}

struct LostTeachingReply {
    inner: DoltTeachings,
    reload: std::sync::atomic::AtomicBool,
    reloading: tokio::sync::Notify,
    release: tokio::sync::Notify,
    reads: AtomicUsize,
}

#[async_trait]
impl Teachings for LostTeachingReply {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        self.inner.first_contact(sid, domain, at).await?;
        self.reload.store(true, Ordering::SeqCst);
        Err(TeachingError::Store("the commit reply was lost".into()))
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.inner.forget(sid).await
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.has_contact(sid, domain).await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        if self.reload.swap(false, Ordering::SeqCst) {
            self.reloading.notify_one();
            self.release.notified().await;
        }
        self.inner.taught().await
    }
}

#[tokio::test]
async fn writer_recovery_an_ambiguous_teaching_commit_reloads_the_native_rail() {
    let scratch = Scratch::new("writer-reload");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store starts");
    let pool = migrated_database(&server, "reload_alpha")
        .await
        .expect("the native schema");
    let native = DoltTeachings::open(pool);
    let sid = Sid("alphaq".into());
    let control = Sid("betaqq".into());
    native
        .first_contact(&control, "claims", Timestamp::UNIX_EPOCH)
        .await
        .expect("seed the published contact");
    let store = Arc::new(LostTeachingReply {
        inner: native,
        reload: false.into(),
        reloading: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        reads: AtomicUsize::new(0),
    });
    let writer = crate::writer::Writer::default();
    let folded = Arc::new(FoldedTeachings::new(store.clone()).with_writer(writer.clone()));
    folded.rebuild().await.expect("boot the rail");
    let mut caller = {
        let folded = folded.clone();
        let sid = sid.clone();
        tokio::spawn(async move {
            folded
                .first_contact(&sid, "claims", Timestamp::UNIX_EPOCH)
                .await
        })
    };
    tokio::select! {
        () = store.reloading.notified() => {},
        answer = &mut caller => panic!("the uncertain write returned before reloading its rail: {answer:?}"),
    }
    assert!(folded.has_contact(&control, "claims").await.expect("held"));
    assert!(
        !folded
            .has_contact(&sid, "claims")
            .await
            .expect("last published"),
        "an unfinished reload was published"
    );
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
    store.release.notify_one();
    let error = caller
        .await
        .expect("the caller ends")
        .expect_err("uncertain receipt");
    assert!(error.to_string().contains("indeterminate"), "{error}");
    assert!(folded.has_contact(&sid, "claims").await.expect("reloaded"));
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
    let counts = writer.recovery_counts();
    assert!(
        writer.has_native_connection(),
        "the production native connection was bypassed"
    );
    assert_eq!(
        (counts.attempted, counts.recovered, counts.failed),
        (1, 1, 0)
    );
    server.stop().await;
}

#[tokio::test]
async fn writer_recovery_an_ambiguous_mail_commit_uses_the_same_native_rail_reload() {
    use crate::counted_rails::CountedMailboxes;
    use crate::dolt::mailboxes::DoltMailboxes;
    use crate::fold::mail::FoldedMailboxes;
    use jojobot_domain::mailbox::{
        MailboxError, MailboxName, Mailboxes, NewMessage, OwnerIndex, OwnerLookup,
    };
    use jojobot_domain::memory::EntityId;

    struct AnyOwner;
    #[async_trait]
    impl OwnerIndex for AnyOwner {
        async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
            Ok(OwnerLookup::Known)
        }
    }

    jojobot_domain::memory::kinds::load_shipped();
    let scratch = Scratch::new("writer-mail-reload");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store starts");
    let pool = migrated_database(&server, "reload_beta")
        .await
        .expect("the native schema");
    let writer = crate::writer::Writer::default();
    let native =
        Arc::new(DoltMailboxes::open(pool, Arc::new(AnyOwner)).with_writer(writer.clone()));
    let name = MailboxName("alpha".into());
    native
        .create_mailbox(&name, &EntityId("bot:alpha".into()), None)
        .await
        .expect("seed the box")
        .written()
        .expect("the box was created");
    let counted = Arc::new(CountedMailboxes::new(native.clone()));
    let folded = FoldedMailboxes::new(counted.clone()).with_writer(writer.clone());
    folded.rebuild().await.expect("boot the board");
    counted.calls.lose_next_post_reply();
    let error = folded
        .post_message(NewMessage {
            mailbox: name,
            body: "Alpha".into(),
            subject: None,
            sender: "bot:alpha".into(),
            sent_at: Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect_err("the commit reply was lost");
    assert!(error.to_string().contains("indeterminate"), "{error}");
    let stored = native
        .scan_messages()
        .await
        .expect("read the committed row");
    assert_eq!(stored.len(), 1);
    counted.calls.reset();
    assert_eq!(
        folded.message_by_id(&stored[0].id).await.expect("held row"),
        Some(stored[0].clone())
    );
    assert_eq!(
        counted.calls.reads(),
        0,
        "the rail was discarded instead of reloaded"
    );
    let counts = writer.recovery_counts();
    assert!(
        writer.has_native_connection(),
        "the production native connection was bypassed"
    );
    assert_eq!(
        (counts.attempted, counts.recovered, counts.failed),
        (1, 1, 0)
    );
    server.stop().await;
}
