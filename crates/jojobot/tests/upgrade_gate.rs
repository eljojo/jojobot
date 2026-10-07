//! **The upgrade proof: a store an older binary filled, read by this one.**
//!
//! `real_binary_serves_a_store_with_existing_rows` proves restart-safety —
//! the binary under test seeds the store itself, so the rows it meets were
//! written by the SAME schema and the SAME parsing it is about to run. That
//! is not the outage class this catches: startup parsed a stored row before
//! loading what it needed to parse it, and the incident happened between
//! two different binaries, not two runs of one.
//!
//! This loads a store dumped from a binary built at an older ref —
//! `crates/jojobot/tests/fixtures/upgrade/doltdump.sql`, refreshed by
//! `make refresh-upgrade-fixture`, never by hand — into a fresh disposable
//! store, boots the CURRENT binary on it, and asserts both that startup
//! succeeds and that every record the recording wrote reads back correctly
//! through the served surface.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::testing::free_port;
use jojobot_exercise::surface::Surface;
use serde_json::json;

const FIXTURE_DUMP: &str = "tests/fixtures/upgrade/doltdump.sql";
const FIXTURE_REF: &str = "tests/fixtures/upgrade/ref.txt";
/// The session id the recording's own boot was handed, which is who the
/// recorded role claim names as its holder.
const FIXTURE_ROLE_HOLDER: &str = "tests/fixtures/upgrade/role_holder.txt";

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn the_current_binary_boots_on_a_store_an_older_binary_filled() {
    let git_ref = std::fs::read_to_string(FIXTURE_REF)
        .unwrap_or_else(|e| panic!("reading {FIXTURE_REF}: {e}"))
        .trim()
        .to_string();

    let state_dir =
        std::env::temp_dir().join(format!("jojobot-upgrade-gate-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let db_dir = state_dir.join("db");
    let store_port = free_port();

    // **Restore through the same layout `Dolt::start` itself produces** —
    // an empty store brought up the ordinary way, then the fixture's own
    // dump replayed over the live connection, so the directory this test's
    // binary meets is built by the exact code every other suite already
    // trusts to build it, not a hand-rolled copy of that layout.
    let mut restoring = Dolt::start(&db_dir, store_port)
        .await
        .expect("a fresh store comes up to receive the fixture");
    let dump = std::fs::read_to_string(FIXTURE_DUMP)
        .unwrap_or_else(|e| panic!("reading {FIXTURE_DUMP}: {e}"));
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(&statement)
            .execute(restoring.pool())
            .await
            .unwrap_or_else(|e| {
                panic!("replaying the fixture recorded at {git_ref} failed on:\n{statement}\n{e}")
            });
    }
    restoring.stop().await;

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
        .expect("the current binary runs");

    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdout));
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

    assert!(
        served,
        "the current binary did not reach its own serving line on a store recorded at \
         {git_ref} — this is the upgrade class this gate exists to catch. What it said \
         instead:\n{}",
        seen.join("\n")
    );

    // **Keep reading.** The binary logs to the pipe for as long as it runs, and
    // a pipe nobody reads fills, after which the binary blocks inside a write
    // and every call to it hangs.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let stderr = child.stderr.take().expect("stderr was piped");
    tokio::spawn(async move {
        let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stderr));
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    // **The boot's own warnings fail the gate.** Each says a half of the boot
    // failed and carried on — the fold or the search index came up empty, or
    // the kind seed stopped part way — so the server serves and every read of
    // a store that was upgraded looks fine until somebody asks the half that
    // is missing.
    //
    // **An error line fails it too, whatever it says.** The boot logs a pass it
    // could not finish at the error level, and a filter that names each
    // message lets a new one through unseen.
    let warned: Vec<&String> = seen
        .iter()
        .filter(|line| {
            line.contains("FOLD EMPTY")
                || line.contains("SEARCH INDEX EMPTY")
                || line.contains("KINDS NOT LOADED")
                || line.contains("ERROR")
        })
        .collect();
    assert!(
        warned.is_empty(),
        "the current binary booted on a store recorded at {git_ref} and said part of its own \
         boot failed: {warned:?}"
    );

    let surface = Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
        .await
        .expect("connecting to the current binary");
    let role_holder = std::fs::read_to_string(FIXTURE_ROLE_HOLDER)
        .unwrap_or_else(|e| panic!("reading {FIXTURE_ROLE_HOLDER}: {e}"))
        .trim()
        .to_string();
    assert_every_recorded_record_reads_back(&surface, &git_ref, &role_holder).await;
    surface.finish().await;

    let _ = child.start_kill();
    let _ = child.wait().await;
    drop(scratch);
}

/// **A naive split, safe for what this fixture actually contains**: this
/// crate's own recorded content never carries a semicolon inside a string,
/// so splitting the whole file on `;` is enough to recover each statement —
/// a `CREATE TABLE` spans many lines and must not be split by line first. A
/// dump that ever needed a semicolon inside a value would need a real SQL
/// tokenizer here instead.
fn split_sql_statements(dump: &str) -> Vec<String> {
    dump.split(';')
        .map(str::trim)
        .filter(|stmt| !stmt.is_empty())
        .map(|stmt| format!("{stmt};"))
        .collect()
}

/// **Every category the recording wrote, read back through the served
/// surface** — the second half of the proof. Startup succeeding is not
/// enough on its own: a stricter parse can let the server come up and still
/// fail the one row it was made stricter about the moment something asks
/// for it.
async fn assert_every_recorded_record_reads_back(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };

    // Entities of several kinds.
    for (kind, slug) in [
        ("person", "upgrade-fixture-person"),
        ("place", "upgrade-fixture-place"),
        ("thing", "upgrade-fixture-thing"),
    ] {
        let handle = format!("{kind}:{slug}");
        let read = surface
            .call("recall", json!({"subject": handle, "facts": true}))
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail(&handle, &read));
        if parsed["objects"][0]["id"] != handle {
            fail(&handle, &read);
        }
    }

    // The fact with fields, an edge, and a stated provenance.
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded fact", &read));
    let facts = parsed["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| fail("the recorded fact's own list", &read));
    let landed = facts
        .iter()
        .find(|f| f["content"] == "lives at the recorded place");
    let Some(landed) = landed else {
        fail("the recorded fact", &read);
    };
    if landed["provenance"] != "testimony"
        || landed["edge"]["object"] != "place:upgrade-fixture-place"
        || landed["fields"]["since"] != "2026-01-01"
    {
        fail("the recorded fact's provenance, edge or fields", &read);
    }

    // **A reference-typed value, a claim worked out from another, and a
    // merge** — the three shapes an upgrade of the storage has to carry and
    // the first recording never held.
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded claims", &read));
    let facts = parsed["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| fail("the recorded claims' own list", &read));
    let lived = facts
        .iter()
        .find(|f| f["content"] == "lives at the recorded place")
        .unwrap_or_else(|| fail("the claim a later one is worked out from", &read));
    let visited = facts
        .iter()
        .find(|f| f["content"] == "visited the recorded place")
        .unwrap_or_else(|| fail("the claim that holds a reference", &read));
    if visited["fields"]["venue"] != "place:upgrade-fixture-place" {
        fail("the reference-typed value", &read);
    }
    if visited["derived_from"].is_null() || visited["derived_from"] != lived["address"] {
        fail("the derived_from pointer", &read);
    }
    // **The survivor holds the duplicate's claim and the account of the
    // merge**, and the duplicate's own handle still resolves, as a status that
    // names the survivor.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    if !read.contains("the twin carried a note") || !read.contains("recorded as one thing") {
        fail("the merge", &read);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:recorded-twin", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the merged handle", &read));
    // **Served as a status naming the survivor, never as an empty thing.**
    if parsed["objects"][0]["id"] != "thing:recorded-twin"
        || parsed["objects"][0]["status"] != "merged"
        || parsed["objects"][0]["merged_into"] != "thing:upgrade-fixture-thing"
        || read.contains("the twin carried a note")
    {
        fail("the merged handle", &read);
    }

    // The declared type is still in the instance's own vocabulary.
    let read = surface.call("start_here", json!({"brief": true})).await;
    if !read.contains("upgrade-fixture-type") {
        fail("the declared type", &read);
    }
    // The thought: an active, connection-edged claim on the bot's own
    // handle, still readable as one of its own thoughts.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant", "facts": true}))
        .await;
    if !read.contains("a recorded thought, live in the room") {
        fail("the recorded thought", &read);
    }

    // **An anonymous boot of the upgraded store is within the ceiling.** The
    // snapshot is what grows with a store, and a boot that sizes its essay as
    // text overshoots as soon as the snapshot leaves it no spare characters.
    let anonymous = surface.call("start_here", json!({})).await;
    let ceiling = jojobot_domain::text::BOOT_ANSWER.budget;
    if anonymous.chars().count() > ceiling {
        fail(
            "an anonymous boot within the ceiling",
            &format!("{} characters against {ceiling}", anonymous.chars().count()),
        );
    }

    // The role-claim fields, on the bot's own folded fields.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role-claim fields", &read));
    // **The holder is the session the recording booted as, recorded beside
    // the dump** — a check that any non-empty value passes would pass on a
    // field the upgrade had rewritten to garbage.
    if role_holder.is_empty()
        || parsed["objects"][0]["fields"]["role/upgrade-fixture-holder/holder"] != role_holder
    {
        fail("the role-claim fields", &read);
    }

    // Mail in each of its three states — read through search, since a
    // message this binary did not send is invisible to list_sent, and
    // read_mailbox drains only the caller's own live box.
    for (needle, state) in [
        ("left new", "new"),
        ("will be read", "read"),
        ("will be processed", "processed"),
    ] {
        let read = surface
            .call(
                "search",
                json!({"query": needle, "include_mail": true, "limit": 5}),
            )
            .await;
        if !read.contains(needle) || !read.contains(state) {
            fail(&format!("the {state} mail message"), &read);
        }
    }

    // The wrapped session — `list_runs` is the direct read on a bot's own
    // runs and their state, attributed to the caller's own sid.
    let booted = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await;
    let booted: serde_json::Value =
        serde_json::from_str(&booted).unwrap_or_else(|_| fail("booting to verify", &booted));
    let sid = booted["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to verify", &booted.to_string()));
    let read = surface
        .call("list_runs", json!({"sid": sid, "limit": 30}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the wrapped session", &read));
    let runs = parsed["runs"]
        .as_array()
        .unwrap_or_else(|| fail("the wrapped session's own runs list", &read));
    // The recording wrapped its own run and left a second one open, so the
    // list holds both states.
    if !runs.iter().any(|run| run["state"] == "wrapped") {
        fail("the wrapped session's own state", &read);
    }
    // **What this build newly interprets, read from records the older build
    // wrote as free text, and held on the next write.**
    assert_the_project_keys_and_the_due_day_read_back_and_bind(surface, git_ref, sid).await;
}

/// **A project, work filed under it, a hand-written due day and a caller's type
/// named `project`, all written by the older build.**
///
/// Each is read back as written. Then two writes go through the served surface:
/// a status the project lists lands, and a status it does not list is refused.
/// The refusal is what shows the build's own `project` keys stand where the
/// caller's type of that name stood, because a type holding only a `budget` key
/// refuses no status.
async fn assert_the_project_keys_and_the_due_day_read_back_and_bind(
    surface: &Surface,
    git_ref: &str,
    sid: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let fields_of = |read: &str, what: &str| -> serde_json::Value {
        let parsed: serde_json::Value =
            serde_json::from_str(read).unwrap_or_else(|_| fail(what, read));
        if parsed["objects"][0]["id"].is_null() {
            fail(what, read);
        }
        parsed["objects"][0]["fields"].clone()
    };

    let read = surface
        .call(
            "recall",
            json!({"subject": "project:upgrade-fixture-project"}),
        )
        .await;
    let project = fields_of(&read, "the recorded project");
    if project["status"] != "now"
        || !project["columns"]
            .as_str()
            .is_some_and(|columns| columns.contains("review") && columns.contains("someday"))
    {
        fail("the recorded project's status and columns", &read);
    }

    let read = surface
        .call("recall", json!({"subject": "work:upgrade-fixture-task"}))
        .await;
    let task = fields_of(&read, "the recorded task");
    if task["status"] != "review"
        || task["owner"] != "person:upgrade-fixture-person"
        || task["depends_on"] != "work:upgrade-fixture-prior"
    {
        fail("the recorded task's status, owner and dependency", &read);
    }

    let read = surface
        .call("recall", json!({"subject": "thing:upgrade-fixture-thing"}))
        .await;
    let thing = fields_of(&read, "the thing with a hand-written due day");
    if thing["due_on"] != "2026-12-01" {
        fail("the hand-written due day", &read);
    }

    // **An edit that sets another key leaves the hand-written due day where it
    // was**, and is not refused because of it.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the thing's own records", &read));
    let address = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"] == "a day written by hand")
        })
        .and_then(|fact| fact["address"].as_str())
        .unwrap_or_else(|| fail("the record carrying the hand-written due day", &read))
        .to_string();
    let landed = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"colour": "red"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("an edit beside the hand-written due day", &landed);
    }
    let read = surface
        .call("recall", json!({"subject": "thing:upgrade-fixture-thing"}))
        .await;
    let after = fields_of(&read, "the thing after an edit beside its due day");
    if after["due_on"] != "2026-12-01" || after["colour"] != "red" {
        fail("the due day beside an edit that set another key", &read);
    }

    // The next write is held to the project's own list.
    let landed = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "waiting on a review",
                   "provenance": "testimony", "fields": {"status": "waiting"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("a status the project lists", &landed);
    }
    let refused = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "an unlisted status",
                   "provenance": "testimony", "fields": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"") || !refused.contains("review") {
        fail("a refusal naming the project's own columns", &refused);
    }
    let refused = surface
        .call(
            "capture",
            json!({"subject": "project:upgrade-fixture-project", "content": "an unlisted status",
                   "provenance": "testimony", "fields": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"") {
        fail(
            "the build's own keys where the caller's `project` type stood",
            &refused,
        );
    }
}
