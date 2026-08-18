//! **The store comes up where the process has no usable home directory.**
//!
//! The store's server keeps its own global configuration under a home
//! directory, and it refuses to start when it cannot find one. A service
//! manager that runs jojobot as a user without a home leaves the process in
//! exactly that state: the account exists for the run and nothing resolves it
//! to a directory. A build sandbox that points `HOME` at a path that is not
//! there is the same condition.
//!
//! jojobot spawns that server, so where it keeps its configuration is
//! jojobot's to decide rather than the environment's to supply. The store's
//! own data directory holds it, and the ambient `HOME` stops mattering.
//!
//! **A test binary of its own, because the environment is process-wide.**
//! Making `HOME` unusable for one case makes it unusable for every case that
//! runs beside it, so this file holds exactly one.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::testing::free_port;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-no-home-{}-{what}-{}",
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

/// **A home the process cannot reach does not stop the store.**
///
/// The server comes up and answers, and its configuration lands under the data
/// directory jojobot was given — which is the evidence that jojobot named the
/// place, rather than the run happening to sit next to a home that worked.
#[tokio::test]
async fn the_store_comes_up_where_the_home_directory_is_not_there() {
    let scratch = Scratch::new("start");
    let absent = scratch.0.join("no-home-here");

    // SAFETY: this binary holds one test, so nothing else is reading the
    // environment while it is written.
    unsafe {
        std::env::set_var("HOME", &absent);
    }

    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    let (answer,): (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(store.pool())
        .await
        .expect("the store answers");
    assert_eq!(answer, 1);

    assert!(
        !absent.exists(),
        "the server wrote to the home directory the environment named"
    );

    store.stop().await;
}
