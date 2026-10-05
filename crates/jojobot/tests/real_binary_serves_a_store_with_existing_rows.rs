//! **The gap a double cannot close.**
//!
//! Every other suite here calls `jojobot::wiring::boot_store` directly, in
//! process. That proves the sequence's decisions, but it cannot reproduce the
//! actual outage: the bug lived in `main.rs`, reachable only by running the
//! `jojobot` binary itself, against a store that already holds a row a
//! PREVIOUS process wrote — the exact state a restart is in, and the one no
//! in-process call reaches, because the kind set a fresh binary loads is a
//! fresh process's own.
//!
//! This spawns the real binary, located the same way every room in
//! `jojobot-exercise` locates it (`server_binary`, which also refuses a
//! binary older than the sources it is built from), against a real Dolt
//! store seeded by a separate process (this test, before the binary starts),
//! and asserts the binary reaches its own "serving" line rather than
//! crash-looping on a store that was perfectly fine.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, kinds};
use tokio::io::AsyncBufReadExt;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn the_real_server_binary_serves_a_store_a_previous_process_already_wrote_to() {
    let state_dir =
        std::env::temp_dir().join(format!("jojobot-real-binary-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    // The exact path `wiring::resolve_store_dir` derives from `STATE_DIRECTORY`.
    let db_dir = state_dir.join("db");
    let store_port = free_port();

    // **Seed the store as an EARLIER process would have**, then take that
    // process's own server down again. What the binary under test opens is a
    // store carrying a real row, written by a boot it was no part of — the
    // shape a restart is actually in, and the one the outage happened in.
    let mut seeding_server = Dolt::start(&db_dir, store_port)
        .await
        .expect("the store comes up");
    migrate::run(seeding_server.pool())
        .await
        .expect("the schema applies");
    let seeding = DoltMemory::open(seeding_server.pool().clone());
    kinds::seed(&seeding).await.expect("the kinds are declared");
    seeding
        .add_entity(NewEntity::new(
            EntityId("bot:milhouse".into()),
            "Milhouse",
            "test",
        ))
        .await
        .expect("a row lands before the binary under test ever opens this store");
    seeding_server.stop().await;
    // This process's own copy of the kind set must not leak past the seeding
    // above: the binary under test loads its own, in its own process, and a
    // case run after this one in this same test binary must not find it
    // already loaded.
    kinds::load_shipped();

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        // The serving line is an info event. An inherited RUST_LOG can filter it
        // out: the package build exports RUST_LOG="", which parses as a filter
        // with no directives and hides every info line.
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");

    // `init_tracing`'s `fmt::layer()` writes to stdout by default.
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    let mut seen = Vec::new();
    let served = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let hit = line.contains("serving http://");
                    seen.push(line);
                    if hit {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    let _ = child.start_kill();
    let _ = child.wait().await;

    assert!(
        served,
        "the real server did not reach its own serving line, against a store holding a row it \
         did not itself write — this is the exact class of the crash-loop this suite exists to \
         catch. What it said instead:\n{}",
        seen.join("\n")
    );

    drop(scratch);
}
