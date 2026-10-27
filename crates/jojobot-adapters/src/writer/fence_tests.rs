//! The server-side gate from the adopted Dolt fence spike, through publication.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};
use sqlx::{MySqlPool, Row};

use super::{Writer, sql};
use crate::dolt::Dolt;
use crate::dolt::teaching::DoltTeachings;
use crate::fold::teachings::FoldedTeachings;
use crate::testing::{Scratch, free_port, migrated_database};

#[tokio::test]
async fn writer_recovery_independent_databases_do_not_share_a_native_fence() {
    let scratch = Scratch::new("writer-database-fence");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let alpha = migrated_database(&server, "fence_alpha")
        .await
        .expect("first database");
    let beta = migrated_database(&server, "fence_beta")
        .await
        .expect("independent database");
    let first = super::fence::Session::new(&alpha);
    let held = first
        .pool
        .acquire()
        .await
        .expect("first writer owns its fence");
    let duplicate = super::fence::Session::new(&alpha);
    assert!(
        duplicate.pool.acquire().await.is_err(),
        "a second writer acquired the same database's fence"
    );
    let independent = super::fence::Session::new(&beta);
    let other = independent
        .pool
        .acquire()
        .await
        .expect("a different database is not blocked by the retained first writer");
    held.close().await.expect("close first physical connection");
    other.close().await.expect("close independent connection");
    server.stop().await;
}

struct GatedTeaching {
    inner: DoltTeachings,
    pool: sql::Pool,
    observer: MySqlPool,
    lock: String,
    old_id: AtomicU64,
    writes: AtomicUsize,
    reads: AtomicUsize,
    lost: AtomicBool,
    child: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[async_trait]
impl Teachings for GatedTeaching {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let mut conn = self.pool.acquire().await.expect("the dedicated connection");
        let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut *conn)
            .await
            .expect("the old connection id");
        self.old_id.store(id, Ordering::SeqCst);
        let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK(?)")
            .bind(&self.lock)
            .fetch_one(&mut *conn)
            .await
            .expect("read the lock owner");
        if owner != Some(id) {
            let acquired: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK(?, 0)")
                .bind(&self.lock)
                .fetch_one(&mut *conn)
                .await
                .expect("hold the connection-lifetime lock");
            assert_eq!(acquired, Some(1));
        }
        let ready: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('alpha-ready', 0)")
            .fetch_one(&mut *conn)
            .await
            .expect("the server-entry signal");
        assert_eq!(ready, Some(1));
        sqlx::raw_sql("BEGIN")
            .execute(&mut *conn)
            .await
            .expect("begin");
        sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
            .bind(sid.as_str())
            .bind(domain)
            .bind(at.to_string())
            .execute(&mut *conn)
            .await
            .expect("the pending native write");
        // This is the spike's server-side gate. KILL can return while this
        // command still owns the old connection and can reach COMMIT.
        let child = tokio::spawn(async move {
            let _ = sqlx::raw_sql(
                "SET @ready = RELEASE_LOCK('alpha-ready'); \
                 SET @gate = GET_LOCK('alpha-gate', 30); COMMIT; \
                 SET @released = RELEASE_LOCK('alpha-gate')",
            )
            .execute(&mut *conn)
            .await;
            let _ = conn.close().await;
        });
        *self.child.lock().expect("child lock") = Some(child);
        let mut observer = self.observer.acquire().await.expect("independent observer");
        let entered: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('alpha-ready', 5)")
            .fetch_one(&mut *observer)
            .await
            .expect("the server entered the gated command");
        assert_eq!(entered, Some(1));
        sqlx::query("SELECT RELEASE_LOCK('alpha-ready')")
            .execute(&mut *observer)
            .await
            .expect("release the entry signal");
        let processes = sqlx::query("SHOW FULL PROCESSLIST")
            .fetch_all(&self.observer)
            .await
            .expect("the old command is executing inside the server");
        assert!(processes.iter().any(|row| {
            u64::try_from(row.get::<i64, _>("Id")).expect("positive connection id") == id
                && row.get::<String, _>("Command") == "Query"
        }));
        self.lost.store(true, Ordering::SeqCst);
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
        if self.lost.load(Ordering::SeqCst) {
            let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK(?)")
                .bind(&self.lock)
                .fetch_one(&self.observer)
                .await
                .expect("observe the recovery fence");
            assert!(owner.is_some(), "reload began without holding the fence");
            assert_ne!(
                owner,
                Some(self.old_id.load(Ordering::SeqCst)),
                "reload began before the old connection released its lock"
            );
        }
        self.inner.taught().await
    }
}

async fn gated_commit(release_before_receipt: bool) {
    let scratch = Scratch::new("writer-fence");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let pool = migrated_database(&server, "fence_alpha")
        .await
        .expect("native schema");
    let native = DoltTeachings::open(pool.clone());
    let sid = Sid("alphaq".into());
    let control = Sid("betaqq".into());
    native
        .first_contact(&control, "claims", Timestamp::UNIX_EPOCH)
        .await
        .expect("seed");
    let mut gate = pool.acquire().await.expect("gate controller");
    let held: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK('alpha-gate', 0)")
        .fetch_one(&mut *gate)
        .await
        .expect("hold the server-side gate");
    assert_eq!(held, Some(1));
    let store = Arc::new(GatedTeaching {
        inner: native.clone(),
        pool: pool.clone().into(),
        observer: pool.clone(),
        lock: super::fence::lock_name(&pool.connect_options()),
        old_id: AtomicU64::new(0),
        writes: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        lost: AtomicBool::new(false),
        child: std::sync::Mutex::new(None),
    });
    let writer = Writer::default();
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
        () = writer.fence_contended() => {},
        answer = &mut caller => panic!("the write returned before encountering the old connection's fence: {answer:?}"),
    }
    assert!(writer.rail_uncertain("teachings"));
    assert_eq!(writer.recovery_counts().recovered, 0);
    let owner: Option<u64> = sqlx::query_scalar("SELECT IS_USED_LOCK(?)")
        .bind(&store.lock)
        .fetch_one(&pool)
        .await
        .expect("the old lock is still held");
    assert_eq!(owner, Some(store.old_id.load(Ordering::SeqCst)));
    assert!(
        !native
            .has_contact(&sid, "claims")
            .await
            .expect("not committed yet")
    );
    assert!(
        folded
            .has_contact(&control, "claims")
            .await
            .expect("published control")
    );
    assert!(
        !folded
            .has_contact(&sid, "claims")
            .await
            .expect("last published image")
    );
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
    if release_before_receipt {
        sqlx::query("SELECT RELEASE_LOCK('alpha-gate')")
            .execute(&mut *gate)
            .await
            .expect("release");
    }
    let error = caller
        .await
        .expect("caller ends")
        .expect_err("uncertain receipt");
    assert!(error.to_string().contains("indeterminate"), "{error}");
    if release_before_receipt {
        assert!(!writer.rail_uncertain("teachings"));
        assert_eq!(writer.recovery_counts().recovered, 1);
        assert_eq!(
            folded
                .has_contact(&sid, "claims")
                .await
                .expect("published after fence"),
            native
                .has_contact(&sid, "claims")
                .await
                .expect("new source read")
        );
    } else {
        assert!(writer.rail_uncertain("teachings"));
        assert_eq!(writer.recovery_counts().failed, 1);
        assert_eq!(writer.recovery_counts().recovered, 0);
        assert!(
            folded
                .first_contact(&Sid("gammaq".into()), "claims", Timestamp::UNIX_EPOCH)
                .await
                .is_err()
        );
        assert_eq!(
            store.writes.load(Ordering::SeqCst),
            1,
            "an uncertain rail accepted another write"
        );
        assert!(
            folded
                .has_contact(&control, "claims")
                .await
                .expect("published control")
        );
        assert!(
            !folded
                .has_contact(&sid, "claims")
                .await
                .expect("last published image")
        );
        assert_eq!(store.reads.load(Ordering::SeqCst), 0);
        sqlx::query("SELECT RELEASE_LOCK('alpha-gate')")
            .execute(&mut *gate)
            .await
            .expect("release");
    }
    let child = store
        .child
        .lock()
        .expect("child lock")
        .take()
        .expect("old query");
    child.await.expect("the old client ends");
    server.stop().await;
}

#[tokio::test]
async fn writer_recovery_the_native_fence_precedes_reload_even_when_kill_returns_first() {
    gated_commit(true).await;
}

#[tokio::test]
async fn writer_recovery_a_native_fence_timeout_keeps_the_rail_uncertain_and_published() {
    gated_commit(false).await;
}
