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

/// Where the fixture the gate reads is kept, unless the run names another with
/// `UPGRADE_FIXTURE_DIR`: the gate keeps one fixture per build it proves an
/// upgrade from.
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
        // The serving line is an info event, and the deployed binary treats an
        // inherited RUST_LOG="" as a filter that drops it.
        .env("RUST_LOG", "info")
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
    // The store's tool keeps its global configuration under a home directory
    // and fails where the environment names none. The run's own state
    // directory holds it, as the adapter does for every store it spawns.
    let dolt_home = state_dir.join("dolt-dump-home");
    std::fs::create_dir_all(&dolt_home).expect("a directory for the dump tool's configuration");
    let dump = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", &dolt_home)
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

    let named_dir = std::env::var("UPGRADE_FIXTURE_DIR")
        .ok()
        .filter(|dir| !dir.is_empty());
    let fixture_dir = std::path::Path::new(named_dir.as_deref().unwrap_or(FIXTURE_DIR));
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

    // **A pair written before a write had a moment of its own, left unmerged.**
    // Each holds `status` once, so both own ordinal 1 of the key. The gate
    // merges them with the current binary, over a store whose writes carry no
    // stamp: the fold has to land and has to rank two unstamped writes.
    for (slug, name, status) in [
        ("red-kite", "A Recorded Kite", "now"),
        ("blue-kite", "A Recorded Spare Kite", "done"),
    ] {
        surface
            .must(
                "add_entity",
                json!({"kind": "thing", "handle": slug, "name": name,
                       "source": "test", "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("add_entity thing:{slug}: {e:#}"));
        surface
            .must(
                "capture",
                json!({
                    "subject": format!("thing:{slug}"),
                    "content": "where it stands",
                    "provenance": "testimony",
                    "fields": {"status": status},
                    "sid": sid,
                }),
            )
            .await
            .unwrap_or_else(|e| panic!("capture on thing:{slug}: {e:#}"));
    }

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

    // **A stored due day, derived by the deployed build from the key that makes
    // one.** The deployed build refuses a caller who writes the day itself, so
    // the recording holds the day it computed from `decide_by`. The later build
    // has to read it back, and an edit beside it must leave it where it was.
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:upgrade-fixture-thing",
                "content": "a day kept under decide_by",
                "provenance": "testimony",
                "fields": {"decide_by": "2026-12-01"},
                "sid": sid,
            }),
        )
        .await
        .expect("the decide_by day is captured");

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

    // **A rule the boot carries.** A claim on the bot's own handle marked to
    // ride its boot.
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "the recorder keeps its recording rule",
                "provenance": "testimony",
                "fields": {"starred": "true"},
                "sid": sid,
            }),
        )
        .await
        .expect("the starred rule is captured");

    // **A claim read out of a system of record**, naming the system and what
    // was read there.
    surface
        .must(
            "capture",
            json!({
                "subject": "place:upgrade-fixture-place",
                "content": "the recorded place opens at nine",
                "provenance": "observation",
                "fields": {"read_from": "the recorded place's page", "read_ref": "opening hours"},
                "sid": sid,
            }),
        )
        .await
        .expect("the observation is captured");

    // **A handle under a key the deployed build declares as a reference to a
    // bot, between two bots.** The store keeps the permanent id it names.
    surface
        .must(
            "add_entity",
            json!({"kind": "bot", "handle": "upgrade-fixture-lead",
                   "name": "A Recorded Lead", "source": "test", "sid": sid}),
        )
        .await
        .expect("the second bot is added");
    // **Written by the lead.** A later build lets only the bot that would sit
    // above the assistant in the chart write who the assistant reports to, and an
    // earlier build lets anyone, so the lead writes it and both can be recorded.
    let lead = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-lead", "brief": true}),
        )
        .await
        .expect("the recorded lead boots");
    let lead_sid = lead["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "the assistant reports to the recorded lead",
                "provenance": "testimony",
                "fields": {"reports_to": "bot:upgrade-fixture-lead"},
                "sid": lead_sid,
            }),
        )
        .await
        .expect("the reports_to field is captured");

    // **A handle under a key nothing declares.** The deployed build keeps it as
    // the text it was sent; a later build lowers it onto the permanent id and
    // links it, and has to serve it back as the handle.
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "the assistant pairs with the recorded lead",
                "provenance": "testimony",
                "fields": {"pairs_with": "bot:upgrade-fixture-lead"},
                "sid": sid,
            }),
        )
        .await
        .expect("the undeclared handle field is captured");

    // **The instance's own record, with the zone it works in.**
    surface
        .must(
            "add_entity",
            json!({"kind": "topic", "handle": "instance", "name": "This instance",
                   "source": "test", "sid": sid}),
        )
        .await
        .expect("the instance topic is added");
    surface
        .must(
            "capture",
            json!({
                "subject": "topic:instance",
                "content": "the instance works in one zone",
                "provenance": "testimony",
                "fields": {"timezone": "America/New_York"},
                "sid": sid,
            }),
        )
        .await
        .expect("the instance zone is captured");

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

    seed_what_a_real_store_holds_beyond_the_plain_records(surface, &sid).await;

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

/// **What a store that has run for months holds and the plain records above do
/// not**: a type a caller named after a shipped kind, a handle in a field that
/// two things have worn, a list of handles under a key nobody declared, a message somebody decided is unreadable, an archived thing, and a
/// bot filled to the deployed build's boot ceiling. Each is written through the
/// deployed build's own surface, so each is a row that build really wrote.
async fn seed_what_a_real_store_holds_beyond_the_plain_records(surface: &Surface, sid: &str) {
    // **A type named for a shipped kind.** The deployed build lets a caller
    // declare it; a later build ships a kind of the same name and has to boot.
    surface
        .must(
            "declare_type",
            json!({
                "name": "topic",
                "fields": [{"key": "upgrade_fixture_mood", "holds": "text"}],
                "sid": sid,
            }),
        )
        .await
        .expect("a type named topic is declared");

    // **A handle that two things have worn.** One thing is renamed, which frees
    // its handle, and a new thing takes it: the handle now names the new thing
    // and is also a former handle of the old one.
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "upgrade-fixture-namesake",
                   "name": "A Recorded Namesake", "source": "test", "sid": sid}),
        )
        .await
        .expect("the first namesake is added");
    surface
        .must(
            "rename_entity",
            json!({"handle": "thing:upgrade-fixture-namesake",
                   "to": "thing:upgrade-fixture-successor", "sid": sid}),
        )
        .await
        .expect("the first namesake is renamed");
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "upgrade-fixture-namesake",
                   "name": "A Later Heir", "source": "test", "sid": sid}),
        )
        .await
        .expect("a second thing takes the freed handle");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:upgrade-fixture-thing",
                "content": "points at a handle two things have worn",
                "provenance": "testimony",
                "fields": {"upgrade_fixture_points_at": "thing:upgrade-fixture-namesake"},
                "sid": sid,
            }),
        )
        .await
        .expect("the ambiguous handle field is captured");

    // **A list of handles under a key nothing declares.**
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:upgrade-fixture-thing",
                "content": "keeps company with a person and a place",
                "provenance": "testimony",
                "fields": {"upgrade_fixture_company":
                           "person:upgrade-fixture-person, place:upgrade-fixture-place"},
                "sid": sid,
            }),
        )
        .await
        .expect("the handle list is captured");

    // **A message somebody decided is unreadable.**
    let unreadable = surface
        .must(
            "post_message",
            json!({"to": "assistant", "body": "set aside as unreadable", "sid": sid}),
        )
        .await
        .expect("a message is posted to be quarantined");
    surface
        .must(
            "mark_processed",
            json!({
                "message_id": unreadable["id"].as_str().expect("a message id"),
                "quarantine": "recorded as unreadable",
                "sid": sid,
            }),
        )
        .await
        .expect("the message is quarantined");

    // **An archived thing.**
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "upgrade-fixture-archived",
                   "name": "A Recorded Archive", "source": "test", "sid": sid}),
        )
        .await
        .expect("the archived thing is added");
    surface
        .must(
            "archive_entity",
            json!({"handle": "thing:upgrade-fixture-archived",
                   "reason": "recorded as no longer wanted", "sid": sid}),
        )
        .await
        .expect("the thing is archived");

    // **A bot filled to the deployed build's boot ceiling, rule last.** A charter
    // takes the boot exactly to the ceiling, the size read off the refusal of one
    // too big. The starred rule is then refused for the room it needs, the
    // charter is cut by that much, and the rule goes in. The deployed build
    // measured the rule before it had a timestamp, so the stored rule carries one
    // the check did not count.
    surface
        .must(
            "add_entity",
            json!({"kind": "bot", "handle": "upgrade-fixture-heavy",
                   "name": "A Recorded Heavy Bot", "source": "test", "sid": sid}),
        )
        .await
        .expect("the heavy bot is added");
    let refusal = |what: &'static str, verb: &'static str, args: serde_json::Value| async move {
        let body: serde_json::Value = serde_json::from_str(&surface.call(verb, args).await)
            .unwrap_or_else(|e| panic!("{what}: the answer is not JSON: {e}"));
        assert_eq!(body["status"], "blocked", "{what} was not refused: {body}");
        body
    };
    let probe: usize = 40_000;
    let too_big = refusal(
        "a charter far past the ceiling",
        "set_charter",
        json!({"bot": "upgrade-fixture-heavy", "prose": "x".repeat(probe), "sid": sid}),
    )
    .await;
    let floor = too_big["floor"]
        .as_u64()
        .expect("the refusal names the floor") as usize;
    let budget = too_big["budget"]
        .as_u64()
        .expect("the refusal names the budget") as usize;
    let at_the_ceiling = probe - (floor - budget);
    surface
        .must(
            "set_charter",
            json!({"bot": "upgrade-fixture-heavy", "prose": "x".repeat(at_the_ceiling),
                   "sid": sid}),
        )
        .await
        .expect("a charter that takes the boot exactly to the ceiling is accepted");
    let rule = json!({
        "subject": "bot:upgrade-fixture-heavy",
        "content": "the heavy bot keeps its one rule",
        "provenance": "testimony",
        "fields": {"starred": "true"},
        "sid": sid,
    });
    let no_room = refusal("a rule with no room left", "capture", rule.clone()).await;
    let needs = no_room["over"]
        .as_u64()
        .expect("the refusal names the overage") as usize;
    surface
        .must(
            "set_charter",
            json!({"bot": "upgrade-fixture-heavy",
                   "prose": "x".repeat(at_the_ceiling - needs), "sid": sid}),
        )
        .await
        .expect("a charter cut by the rule's room is accepted");
    surface
        .must("capture", rule)
        .await
        .expect("the heavy bot's rule fits now");
}
