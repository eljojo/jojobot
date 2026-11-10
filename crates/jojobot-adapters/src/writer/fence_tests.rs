//! The server-side gate from the adopted Dolt fence spike, through publication.

use crate::dolt::Dolt;
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
    // The wait that ended without the lock is remembered, on the session that waited
    // and on no other: it is what tells a lock held elsewhere from a busy pool.
    assert!(
        duplicate.lock_missed(),
        "the session that missed the lock remembers it"
    );
    assert!(
        !first.lock_missed(),
        "the session that holds the lock did not miss it"
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

/// **The advisory lock name is a digest, so its length does not follow the
/// database's.** The server refuses a lock name over 64 characters, and a
/// database name may be long. Paired with the empty database, which has a key of
/// its own.
#[test]
fn the_fence_lock_name_is_64_characters_whatever_the_database_name() {
    use sqlx::mysql::MySqlConnectOptions;

    let long = "d".repeat(300);
    for options in [
        MySqlConnectOptions::new(),
        MySqlConnectOptions::new().database("fence_alpha"),
        MySqlConnectOptions::new().database(&long),
    ] {
        let name = super::fence::lock_name(&options);
        assert_eq!(name.len(), 64, "{name}");
        assert!(name.bytes().all(|b| b.is_ascii_hexdigit()), "{name}");
    }
}

/// **A connection with no database name has the documented key.** The name of
/// the lock is shared by every process that fences the same server, so the
/// spelling is pinned: the digest of the lock's name, a colon and nothing.
#[test]
fn the_fence_lock_name_of_no_database_is_the_digest_of_the_lock_and_a_colon() {
    use sqlx::mysql::MySqlConnectOptions;

    let none = super::fence::lock_name(&MySqlConnectOptions::new());
    assert_eq!(
        none,
        "398048b75d2ba1c969dbe7254b7b5dc191ca70903079d795b33841b1f37af175"
    );
    assert_eq!(
        none,
        super::fence::lock_name(&MySqlConnectOptions::new().database("")),
        "an empty name is no name"
    );
    assert_ne!(
        none,
        super::fence::lock_name(&MySqlConnectOptions::new().database("fence_alpha")),
    );
}

/// **A pool timeout is named as the lock held elsewhere only while the session's
/// last wait for the lock missed it.** A timeout from a pool that was only busy keeps
/// its own text, and no other error is touched. The pool here never connects, so
/// no server is needed: the cases are about what the error is called inside a
/// writer, not about the lock.
#[tokio::test]
async fn a_pool_timeout_names_the_lock_only_after_a_missed_lock() {
    let pool: super::sql::Pool = sqlx::MySqlPool::connect_lazy("mysql://root@127.0.0.1:1/unused")
        .expect("a pool that connects when asked")
        .into();
    let writer = super::Writer::default();
    let said = writer
        .run(async move {
            // Opens the writer's session; it has not waited for the lock yet.
            let _ = pool.writer();
            let timed_out = sqlx::Error::PoolTimedOut;
            let busy = super::sql::why(&timed_out);
            super::Writer::current()
                .expect("inside the writer")
                .0
                .sql
                .get()
                .expect("the session is open")
                .note_a_missed_lock();
            let held = super::sql::why(&timed_out);
            let other = super::sql::why(&sqlx::Error::PoolClosed);
            (busy, held, other)
        })
        .await;
    assert_eq!(
        said.0,
        sqlx::Error::PoolTimedOut.to_string(),
        "a busy pool keeps its own text"
    );
    assert_eq!(
        said.1,
        super::LOCK_HELD,
        "a timeout after a missed lock is the lock held elsewhere"
    );
    assert_eq!(
        said.2,
        sqlx::Error::PoolClosed.to_string(),
        "only a pool timeout is read as the lock"
    );
}

/// **Only an answer of 0 is a wait that ended, so only it is remembered as the lock
/// held elsewhere.** NULL is an error of the call: the session is refused the lock
/// and is not told another writer holds it. Paired with 1, the one answer that
/// grants the lock, and with 0, the wait that ended.
#[test]
fn a_null_answer_of_the_lock_is_refused_and_is_not_a_wait_that_ended() {
    use super::fence::{LockAnswer, lock_answer};

    assert_eq!(
        lock_answer(None),
        LockAnswer {
            missed: false,
            refused: true
        },
        "NULL is an error of the call, not another writer"
    );
    assert_eq!(
        lock_answer(Some(0)),
        LockAnswer {
            missed: true,
            refused: true
        },
        "0 is a wait that ended without the lock"
    );
    assert_eq!(
        lock_answer(Some(1)),
        LockAnswer {
            missed: false,
            refused: false
        },
        "1 grants the lock"
    );
}
