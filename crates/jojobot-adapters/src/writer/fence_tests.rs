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
