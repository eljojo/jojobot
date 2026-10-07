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
    // **Keep reading.** The binary logs to the pipe for as long as it runs, and
    // a pipe nobody reads fills, after which the binary blocks inside a write
    // and every call to it hangs.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    let surface = Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
        .await
        .expect("connecting to the old binary");

    let role_holder = seed_representative_records(&surface).await;

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
    // **The holder the recording's own boot was handed**, so the gate asserts
    // the claim read back is that claim and not merely some claim.
    std::fs::write(
        fixture_dir.join("role_holder.txt"),
        format!("{role_holder}\n"),
    )
    .expect("the role holder is recorded beside the dump");

    eprintln!(
        "recorded the upgrade fixture from {git_ref} into {}",
        fixture_dir.display()
    );
}

/// **One representative record per category the gate has to prove reads
/// back**, never exhaustive — the gate is a boot-and-read-back proof, not a
/// second copy of every other suite's coverage.
///
/// **Returns the session id of the run left holding a role**, which is what
/// that role records as its holder.
async fn seed_representative_records(surface: &Surface) -> String {
    // **The role is claimed at the boot door, the only door it opens
    // through.** The holder is the session that booted, so the claim is the
    // recording run's own.
    let booted = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "claim": "upgrade-fixture-recorder"}),
        )
        .await
        .expect("the recording session boots");
    assert_eq!(
        booted["session"]["claim"]["status"], "taken",
        "the recording run did not get its own role claim: {booted}"
    );
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
    let lived = surface
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

    // **A reference-typed value.** A type that declares a key as a reference to
    // a place, and a claim that holds one: the store keeps the permanent id the
    // value names and the upgrade has to serve it back as the handle.
    surface
        .must(
            "declare_type",
            json!({
                "name": "upgrade-fixture-visit",
                "fields": [{"key": "venue", "holds": "reference:place"}],
                "sid": sid,
            }),
        )
        .await
        .expect("the reference type is declared");
    // **A claim worked out from another claim.** The pointer is the claim's
    // address, and it has to read back as that address.
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "visited the recorded place",
                "provenance": "inference",
                "fields": {"venue": "place:upgrade-fixture-place"},
                "derived_from": lived["address"],
                "sid": sid,
            }),
        )
        .await
        .expect("the reference value and the derived claim are captured");

    // **A merge.** The duplicate carries a claim of its own, then folds into
    // the survivor: its handle goes on resolving and its claim has moved.
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "recorded-twin", "name": "A Spare Copy",
                   "source": "test", "sid": sid}),
        )
        .await
        .expect("the duplicate is added");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:recorded-twin",
                "content": "the twin carried a note",
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await
        .expect("the duplicate's claim is captured");
    surface
        .must(
            "merge_entities",
            json!({
                "duplicate": "thing:recorded-twin",
                "survivor": "thing:upgrade-fixture-thing",
                "reason": "recorded as one thing",
                "sid": sid,
            }),
        )
        .await
        .expect("the duplicate is merged into the survivor");

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

    // **A project and its work, written before any build declared keys for
    // either.** The deployed build holds `status`, `owner`, `depends_on` and
    // `columns` as free text. A later build declares them, and has to read
    // what was written under the old build and hold the next write to them.
    surface
        .must(
            "add_entity",
            json!({"kind": "project", "handle": "upgrade-fixture-project",
                   "name": "A Recorded Project", "source": "test", "sid": sid}),
        )
        .await
        .expect("the project is added");
    for (slug, name) in [
        ("upgrade-fixture-prior", "A Recorded Prior Task"),
        ("upgrade-fixture-task", "A Recorded Task"),
    ] {
        surface
            .must(
                "add_entity",
                json!({"kind": "work", "handle": slug, "name": name, "source": "test",
                       "parent": "project:upgrade-fixture-project", "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("add_entity work:{slug}: {e:#}"));
    }
    surface
        .must(
            "capture",
            json!({
                "subject": "project:upgrade-fixture-project",
                "content": "the recorded project is under way",
                "provenance": "testimony",
                "fields": {"status": "now",
                           "columns": "someday, next, now, waiting, done, review"},
                "sid": sid,
            }),
        )
        .await
        .expect("the project's status and columns are captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "work:upgrade-fixture-prior",
                "content": "the prior task is finished",
                "provenance": "testimony",
                "fields": {"status": "done"},
                "sid": sid,
            }),
        )
        .await
        .expect("the prior task's status is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "work:upgrade-fixture-task",
                "content": "the recorded task is in review",
                "provenance": "testimony",
                "fields": {"status": "review",
                           "owner": "person:upgrade-fixture-person",
                           "depends_on": "work:upgrade-fixture-prior"},
                "sid": sid,
            }),
        )
        .await
        .expect("the task's status, owner and dependency are captured");

    // **A due day written by hand on a thing that carries none of the keys
    // that make one.** The later build keeps the stored due day its own and
    // refuses a caller who sends it. What an earlier caller wrote has to
    // read back as written.
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:upgrade-fixture-thing",
                "content": "a day written by hand",
                "provenance": "testimony",
                "fields": {"due_on": "2026-12-01"},
                "sid": sid,
            }),
        )
        .await
        .expect("the hand-written due day is captured");

    // **A caller's type under the name of a kind a later build gives keys.**
    // `project` was a kind with no keys, so its name was a type's to hold
    // only until this build ships the kind's keys over it.
    surface
        .must(
            "declare_type",
            json!({
                "name": "project",
                "fields": [{"key": "budget", "holds": "text"}],
                "sid": sid,
            }),
        )
        .await
        .expect("the caller's type under the name of a keyless kind is declared");

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

    // **A second run that is never wrapped, and the role it holds.** Wrapping
    // releases what a run held, so the claim of the run above reads back with
    // no holder. This run is left open, so the gate reads a claim that is
    // still held after the upgrade.
    let held = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "claim": "upgrade-fixture-holder"}),
        )
        .await
        .expect("the second run boots");
    assert_eq!(
        held["session"]["claim"]["status"], "taken",
        "the second run did not get its role claim: {held}"
    );
    held["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string()
}
