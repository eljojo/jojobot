//! **The one failure `boot_store` does not swallow.**
//!
//! Mail and sessions are served from the store, so a server that came up
//! without one would have no board to read and no run to resume, and would
//! report that emptiness as the truth. Every OTHER read `boot_store` takes
//! is best-effort (see `boot_store_survives_a_downstream_read_failure.rs`);
//! this is the one decision that has to go the other way — the counterpart
//! that makes the "everything else is not fatal" claim meaningful rather than
//! a sequence that never refuses at all.
//!
//! Occupying the port ahead of time is enough to make the store itself fail
//! to come up, with nothing about the data directory or the schema in play.

use std::path::PathBuf;

use jojobot_adapters::testing::free_port;
use jojobot_domain::clock::Clock;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn boot_store_refuses_when_the_port_is_already_taken() {
    let path = std::env::temp_dir().join(format!("jojobot-boot-refuses-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // Hold the port so the store's own server cannot bind it.
    let blocker = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("a free port stays free long enough to bind it first");

    let failed = jojobot::wiring::boot_store(&path, port, Clock::Real).await;
    assert!(
        failed.is_err(),
        "the store failing to come up must refuse the boot rather than continue past it"
    );

    drop(blocker);
    drop(scratch);
}
