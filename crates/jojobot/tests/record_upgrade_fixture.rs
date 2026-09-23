//! **Records the fixture the upgrade gate reads** — never run by `make
//! check`. Spawns a binary built from an older ref, against a fresh
//! disposable store, seeds a representative set of records through that
//! binary's own served surface, stops it, and dumps the store to a text
//! file this crate's tests carry as a fixture.
//!
//! Driven by `make refresh-upgrade-fixture`, never by hand: the two
//! environment variables below name the binary to record from and the ref
//! it was built at, and the Makefile target is what resolves both without
//! moving this checkout.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::testing::free_port;
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// Where the fixture the gate reads is kept.
const FIXTURE_DIR: &str = "tests/fixtures/upgrade";

/// A directory of this run's own, removed only on request — the caller
/// needs to read the store back out of it after the binary stops.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-upgrade-record-{}-{what}-{}",
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

#[tokio::test]
#[ignore = "run through `make refresh-upgrade-fixture`, never by `make check`"]
async fn record_the_upgrade_fixture() {
    let binary = std::env::var("UPGRADE_FIXTURE_BINARY")
        .expect("UPGRADE_FIXTURE_BINARY names the binary to record from");
    let git_ref = std::env::var("UPGRADE_FIXTURE_REF")
        .expect("UPGRADE_FIXTURE_REF names the ref that binary was built at");

    let scratch = Scratch::new("state");
    let state_dir = scratch.0.clone();
    let store_port = free_port();
    let http_port = free_port();

    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("the old binary runs");

    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdout));
    let served = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.contains("serving http://") {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        served,
        "the old binary at {git_ref} did not reach its own serving line"
    );

    let surface = Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
        .await
        .expect("connecting to the old binary");

    seed_representative_records(&surface).await;

    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;
    // The server holds the store's own lock file until its process is
    // truly gone — `dolt dump` below needs it released.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let database_dir = state_dir.join("db").join("jojobot");
    assert!(
        database_dir.join(".dolt").is_dir(),
        "the old binary's own database directory is not where boot_store puts it: {}",
        database_dir.display()
    );
    let dump = tokio::process::Command::new("dolt")
        .arg("dump")
        .arg("-r")
        .arg("sql")
        .arg("-f")
        .arg("-fn")
        .arg("doltdump.sql")
        .current_dir(&database_dir)
        .output()
        .await
        .expect("dolt dump runs");
    assert!(
        dump.status.success(),
        "dolt dump failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );

    let fixture_dir = std::path::Path::new(FIXTURE_DIR);
    std::fs::create_dir_all(fixture_dir).expect("the fixture directory exists");
    std::fs::copy(
        database_dir.join("doltdump.sql"),
        fixture_dir.join("doltdump.sql"),
    )
    .expect("the dump is copied into the fixture directory");
    std::fs::write(fixture_dir.join("ref.txt"), format!("{git_ref}\n"))
        .expect("the ref is recorded beside the dump");

    eprintln!(
        "recorded the upgrade fixture from {git_ref} into {}",
        fixture_dir.display()
    );
}

/// **One representative record per category the gate has to prove reads
/// back**, never exhaustive — the gate is a boot-and-read-back proof, not a
/// second copy of every other suite's coverage.
async fn seed_representative_records(surface: &Surface) {
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the recording session boots");
    let sid = booted["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();

    // Entities of several kinds.
    for (kind, slug, name) in [
        ("person", "upgrade-fixture-person", "A Recorded Person"),
        ("place", "upgrade-fixture-place", "A Recorded Place"),
        ("thing", "upgrade-fixture-thing", "A Recorded Thing"),
    ] {
        surface
            .must(
                "add_entity",
                json!({"kind": kind, "handle": slug, "name": name, "source": "test", "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("add_entity {kind}:{slug}: {e:#}"));
    }

    // A fact with fields, an edge, and a stated provenance.
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "lives at the recorded place",
                "provenance": "testimony",
                "shape": "location",
                "object": "place:upgrade-fixture-place",
                "fields": {"since": "2026-01-01"},
                "sid": sid,
            }),
        )
        .await
        .expect("the fact with fields and an edge is captured");

    // A declared type.
    surface
        .must(
            "declare_type",
            json!({
                "name": "upgrade-fixture-type",
                "fields": [{"key": "status", "holds": "text",
                            "one_of": ["open", "closed"]}],
                "sid": sid,
            }),
        )
        .await
        .expect("the type is declared");

    // A thought in a room: an active claim on the bot's own handle, drawing
    // a connection edge.
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "a recorded thought, live in the room",
                "provenance": "inference",
                "shape": "connection",
                "object": "thing:upgrade-fixture-thing",
                "sid": sid,
            }),
        )
        .await
        .expect("the thought is captured");

    // Role-claim fields, in the exact shape claim_role/role_holder_key mint
    // them — this binary's own claim on a role, on its own handle.
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "claims the upgrade-fixture-recorder role",
                "provenance": "observation",
                "fields": {
                    "role/upgrade-fixture-recorder/holder": "bot:assistant",
                    "role/upgrade-fixture-recorder/claimed_at": jiff::Timestamp::now().to_string(),
                    "read_from": "this recording run",
                },
                "sid": sid,
            }),
        )
        .await
        .expect("the role claim fields are captured");

    // Mail in several states: one left new, one read, one processed.
    surface
        .must(
            "post_message",
            json!({"to": "assistant", "body": "left new", "sid": sid}),
        )
        .await
        .expect("a message is posted and left new");
    let to_read = surface
        .must(
            "post_message",
            json!({"to": "assistant", "body": "will be read", "sid": sid}),
        )
        .await
        .expect("a message is posted to be read");
    let to_process = surface
        .must(
            "post_message",
            json!({"to": "assistant", "body": "will be processed", "sid": sid}),
        )
        .await
        .expect("a message is posted to be processed");

    // The recording session is itself `assistant`'s, so its own sid already
    // addresses the box these two mail verbs read from.
    let read_id = to_read["id"].as_str().expect("a message id");
    surface
        .must("read_message", json!({"message_id": read_id, "sid": sid}))
        .await
        .expect("the message is read");
    let process_id = to_process["id"].as_str().expect("a message id");
    surface
        .must(
            "mark_processed",
            json!({"message_id": process_id, "sid": sid}),
        )
        .await
        .expect("the message is marked processed");

    // A session, journalled and then wrapped — the one this recording ran
    // in, so the gate reads back a real wrapped run rather than a bare
    // fixture record nothing ever produced.
    surface
        .must(
            "journal",
            json!({"entry": "recorded the upgrade fixture", "sid": sid}),
        )
        .await
        .expect("a beat is journalled before the session wraps");
    surface
        .must(
            "wrap_session",
            json!({"sid": sid, "story": "recorded the upgrade fixture"}),
        )
        .await
        .expect("the session wraps");
}
