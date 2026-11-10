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

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, kinds};

use jojobot_adapters::testing::Scratch;

/// The real binary over the store in `state_dir`, as the service runs it.
fn spawn_binary(state_dir: &std::path::Path, store_port: u16) -> tokio::process::Child {
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", "127.0.0.1:0")
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
        .expect("the binary runs")
}

#[tokio::test]
async fn the_real_server_binary_serves_a_store_a_previous_process_already_wrote_to() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-real-binary");
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

    let mut child = spawn_binary(&state_dir, store_port);

    // `init_tracing`'s `fmt::layer()` writes to stdout by default.
    let serving = match jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    {
        Ok(serving) => serving,
        Err(failure) => panic!(
            "the real server {failure} (a boot is given {:?}), against a store holding a row it \
             did not itself write — this is the exact class of the crash-loop this suite exists \
             to catch.",
            jojobot_exercise::room::READY_TIMEOUT
        ),
    };
    let http_port = serving.port();
    let mut lines = serving.stdout;
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    // **The binary made the instance's own record at boot**, over a store that
    // held none: the path production takes, which no in-process seed call reaches.
    let surface =
        jojobot_exercise::surface::Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
            .await
            .expect("connecting to the real binary");
    let read = surface
        .call("recall", serde_json::json!({"subject": "topic:instance"}))
        .await;
    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;
    assert!(
        read.contains("\"id\":\"topic:instance\"") && read.contains("\"source\":\"jojobot\""),
        "a store that held no instance record has one after the binary boots: {read}"
    );

    drop(scratch);
}

/// **The boot that imports the claims covers the import with a snapshot before it
/// serves, and a boot whose import was done takes none.** The snapshot the
/// teaching ledger takes at its own import is of a log that holds no claims yet,
/// so a process stopped with no orderly stop would leave the next boot to replay
/// the whole import.
///
/// The first boot is killed, not stopped: an orderly stop takes its own snapshot,
/// which would cover the log for the second boot whatever the first boot did. The
/// first snapshot stops at the import, and the records the build supplies are bound
/// right after it, so the log is past that snapshot and a second boot that took a
/// snapshot for an import it did not run would take one here.
#[tokio::test]
async fn the_real_server_binary_covers_the_claims_import_with_a_snapshot_at_boot() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-real-binary-import");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let store_port = free_port();

    // A store an earlier build left: a row and a claim, nothing in the log.
    let mut seeding_server = Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the store comes up");
    migrate::run(seeding_server.pool())
        .await
        .expect("the schema applies");
    let seeding = DoltMemory::open(seeding_server.pool().clone());
    kinds::seed(&seeding).await.expect("the kinds are declared");
    let homer = EntityId("person:homer".into());
    seeding
        .add_entity(NewEntity::new(homer.clone(), "Homer", "test"))
        .await
        .expect("a row lands")
        .written()
        .expect("not blocked");
    seeding
        .capture(NewFact::about(homer, "likes donuts", date(2026, 9, 2)))
        .await
        .expect("a claim lands")
        .written()
        .expect("not blocked");
    seeding_server.stop().await;
    kinds::load_shipped();

    let boot = |label: &'static str| {
        let state_dir = state_dir.clone();
        async move {
            let mut child = spawn_binary(&state_dir, store_port);
            let serving = jojobot_adapters::testing::wait_for_serving_line(
                &mut child,
                jojobot_exercise::room::READY_TIMEOUT,
                "serving http://",
            )
            .await
            .unwrap_or_else(|failure| panic!("the {label} boot {failure}"));
            let seen = serving.seen.clone();
            let _ = child.start_kill();
            let _ = child.wait().await;
            seen
        }
    };
    let said = |seen: &[String], what: &str| seen.iter().any(|line| line.contains(what));

    let first = boot("first").await;
    assert!(
        said(&first, "imported today's claims into the event log"),
        "the first boot ran the import:\n{}",
        first.join("\n")
    );
    assert!(
        said(&first, "snapshot taken after import"),
        "the first boot covered the import with a snapshot:\n{}",
        first.join("\n")
    );

    let second = boot("second").await;
    assert!(
        said(&second, "claims import: already done"),
        "the second boot found the import done:\n{}",
        second.join("\n")
    );
    assert!(
        !said(&second, "snapshot taken after import"),
        "a boot whose import was done took a snapshot for it:\n{}",
        second.join("\n")
    );

    drop(scratch);
}
