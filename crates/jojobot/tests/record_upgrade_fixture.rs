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

use jojobot_adapters::testing::Scratch;

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
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the old binary runs");

    let serving = match jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    {
        Ok(serving) => serving,
        Err(failure) => panic!(
            "the old binary at {git_ref} {failure} (a boot is given {:?})",
            jojobot_exercise::room::READY_TIMEOUT
        ),
    };
    // **Keep reading.** The binary logs to the pipe for as long as it runs, and
    // a pipe nobody reads fills, after which the binary blocks inside a write
    // and every call to it hangs.
    let mut lines = serving.stdout;
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

    // **No default directory.** The gate keeps one fixture per build it proves an
    // upgrade from, and each deploy adds a directory of its own. A default would
    // either record over a fixture the gate still reads or create a directory
    // the gate does not read.
    let named_dir = std::env::var("UPGRADE_FIXTURE_DIR")
        .ok()
        .filter(|dir| !dir.is_empty())
        .expect("UPGRADE_FIXTURE_DIR names the fixture directory to record into");
    let fixture_dir = std::path::Path::new(&named_dir);
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

/// **A creation, with the token of a near-miss screen read out of its refusal.**
/// A build that screens a creation for a resembling name turns the call back and
/// names the token that lifts it; the token is sent with a reason that tells the
/// two apart, as a caller has to. A build that does not screen it lands the first
/// call, so one recorder serves both.
async fn add_screened(surface: &Surface, args: serde_json::Value) -> anyhow::Result<()> {
    let first = surface.call("add_entity", args.clone()).await;
    if !first.contains("\"status\":\"blocked\"") {
        return Ok(());
    }
    let refusal: serde_json::Value = serde_json::from_str(&first)?;
    let advice = refusal["how_to_proceed"].as_str().unwrap_or_default();
    let token = advice
        .split("override_token: \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or_else(|| anyhow::anyhow!("the creation was refused without a token: {first}"))?;
    let mut again = args;
    again["override_token"] = token.into();
    again["override_reason"] = "recorded as a different thing beside the first".into();
    surface.must("add_entity", again).await.map(|_| ())
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
    // **Pointers at the duplicate, written before it is folded.** The deployed
    // build keeps no merge event, so what it leaves naming the folded thing is
    // all a later build has to go by: an edge, a reference of a named kind, a
    // handle under a key nobody declares, and a ref of the claim.
    surface
        .must(
            "declare_type",
            json!({
                "name": "upgrade-fixture-keeper",
                "fields": [{"key": "keeps", "holds": "reference:thing"}],
                "sid": sid,
            }),
        )
        .await
        .expect("the keeper type is declared");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "keeps an eye on the twin",
                "provenance": "testimony",
                "shape": "about",
                "object": "thing:recorded-twin",
                "sid": sid,
            }),
        )
        .await
        .expect("the edge at the duplicate is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "watches the twin",
                "provenance": "testimony",
                "sets": {"upgrade_fixture_watches": "thing:recorded-twin"},
                "sid": sid,
            }),
        )
        .await
        .expect("the handle under an undeclared key is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "holds the twin",
                "provenance": "testimony",
                "sets": {"keeps": "thing:recorded-twin"},
                "sid": sid,
            }),
        )
        .await
        .expect("the reference at the duplicate is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "mentions the twin among its refs",
                "provenance": "testimony",
                "refs": ["thing:recorded-twin"],
                "sid": sid,
            }),
        )
        .await
        .expect("the ref at the duplicate is captured");
    // **Pointers at records the build supplies**, which have no row: an edge, a ref
    // and a parent. A boot must read each as naming something, not as a stale
    // handle, and a boot that read one as unresolved would say so as an error.
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "keeps an eye on the colleagues view",
                "provenance": "testimony",
                "shape": "about",
                "object": "view:colleagues",
                "sid": sid,
            }),
        )
        .await
        .expect("the edge at a supplied view is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "mentions the loops view among its refs",
                "provenance": "testimony",
                "refs": ["view:loops"],
                "sid": sid,
            }),
        )
        .await
        .expect("the ref at a supplied view is captured");
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "kettle", "name": "A Recorded Kettle",
                   "source": "test", "parent": "view:ready", "sid": sid}),
        )
        .await
        .expect("a thing under a supplied view is added");
    // **A reference-typed field that names a record the build supplies.** The
    // record has no row and no badge, so the field holds its handle as it stands.
    surface
        .must(
            "declare_type",
            json!({
                "name": "upgrade-fixture-viewer",
                "fields": [{"key": "looks_at", "holds": "reference"}],
                "sid": sid,
            }),
        )
        .await
        .expect("the viewer type is declared");
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "looks at the colleagues view",
                "provenance": "testimony",
                "sets": {"looks_at": "view:colleagues"},
                "sid": sid,
            }),
        )
        .await
        .expect("the reference at a supplied view is captured");
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

    // **Pointers written after the fold, by the handle the duplicate still
    // answers to.** The deployed build may follow the handle to the survivor or
    // keep the folded thing's key; the recording keeps whichever it did, and the
    // gate reads it back before and after the boot. An answer that refuses is
    // printed and recorded as nothing.
    for (field, content) in [
        (
            "upgrade_fixture_watches_after",
            "watches the twin after the fold",
        ),
        ("keeps", "holds the twin after the fold"),
    ] {
        let answer = surface
            .call(
                "capture",
                json!({
                    "subject": "person:upgrade-fixture-person",
                    "content": content,
                    "provenance": "testimony",
                    "sets": {field: "thing:recorded-twin"},
                    "sid": sid,
                }),
            )
            .await;
        eprintln!("a field at the folded handle, after the fold ({field}): {answer}");
    }
    let answer = surface
        .call(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "keeps an eye on the twin after the fold",
                "provenance": "testimony",
                "shape": "about",
                "object": "thing:recorded-twin",
                "sid": sid,
            }),
        )
        .await;
    eprintln!("an edge at the folded handle, after the fold: {answer}");

    // **A fold whose survivor is renamed afterwards.** The folded row names the
    // handle the survivor wore at the fold, and the rename moves no other row.
    for (slug, name) in [
        ("recorded-spare", "A Second Spare Copy"),
        ("recorded-heir", "A Recorded Heir"),
    ] {
        add_screened(
            surface,
            json!({"kind": "thing", "handle": slug, "name": name,
                   "source": "test", "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("add_entity thing:{slug}: {e:#}"));
    }
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "watches the second twin",
                "provenance": "testimony",
                "sets": {"upgrade_fixture_watches_two": "thing:recorded-spare"},
                "sid": sid,
            }),
        )
        .await
        .expect("the handle at the second duplicate is captured");
    surface
        .must(
            "merge_entities",
            json!({
                "duplicate": "thing:recorded-spare",
                "survivor": "thing:recorded-heir",
                "reason": "recorded as one more thing",
                "sid": sid,
            }),
        )
        .await
        .expect("the second duplicate is merged");
    // **The deployed build screens the rename against the survivor's own folded
    // names**, so the first call is turned back and names the token that lifts it.
    // The token is read out of the refusal, as a caller has to.
    let turned_back = surface
        .call(
            "rename_entity",
            json!({"handle": "thing:recorded-heir", "to": "thing:jukebox", "sid": sid}),
        )
        .await;
    // **A build that does not screen the rename has already made it.**
    if turned_back.contains("\"status\":\"blocked\"") {
        let turned_back: serde_json::Value =
            serde_json::from_str(&turned_back).expect("the refusal is JSON");
        let advice = turned_back["how_to_proceed"].as_str().unwrap_or_default();
        let token = advice
            .split("override_token: \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| {
                panic!("the rename was not turned back with a token: {turned_back}")
            });
        surface
            .must(
                "rename_entity",
                json!({"handle": "thing:recorded-heir", "to": "thing:jukebox",
                       "override_token": token, "sid": sid}),
            )
            .await
            .expect("the second survivor is renamed after the fold");
    }
    // A child made under the first duplicate once it is folded.
    let answer = surface
        .call(
            "add_entity",
            json!({"kind": "thing", "handle": "the-fern", "name": "A Recorded Fern",
                   "source": "test", "parent": "thing:recorded-twin", "sid": sid}),
        )
        .await;
    eprintln!("a child under the folded handle, after the fold: {answer}");
    // **The handle the rename freed is taken by a new thing.** The second
    // duplicate still names that handle, now worn by another thing and once worn
    // by the survivor.
    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "recorded-heir", "name": "A Newcomer",
                   "source": "test", "sid": sid}),
        )
        .await
        .expect("the handle the rename freed is taken by a new thing");
    // An edge drawn at the second duplicate once its survivor wears a new handle.
    let answer = surface
        .call(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "keeps an eye on the second twin",
                "provenance": "testimony",
                "shape": "about",
                "object": "thing:recorded-spare",
                "sid": sid,
            }),
        )
        .await;
    eprintln!("an edge at the second folded handle, after the rename: {answer}");

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
                    "sets": {"status": status},
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
        add_screened(
            surface,
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
                "sets": {"status": "now",
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
                "sets": {"status": "done"},
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
                "sets": {"status": "review",
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
                "sets": {"decide_by": "2026-12-01"},
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
    // **Written by the assistant itself, in the recording run.** Deploy 9.1 lets
    // only the bots already above a bot in the chart place it, and the assistant
    // has none, so the assistant alone may write who it reports to; an earlier
    // build lets anyone, so this writer records on every build.
    surface
        .must(
            "capture",
            json!({
                "subject": "bot:assistant",
                "content": "the assistant reports to the recorded lead",
                "provenance": "testimony",
                "sets": {"reports_to": "bot:upgrade-fixture-lead"},
                "sid": sid,
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
                "sets": {"pairs_with": "bot:upgrade-fixture-lead"},
                "sid": sid,
            }),
        )
        .await
        .expect("the undeclared handle field is captured");

    // **The instance's own record, with the zone it works in.**
    // **A build that seeds the instance record has it already**, and refuses the
    // creation by its exact handle; an older build has none and makes it here.
    let instance = surface
        .call(
            "add_entity",
            json!({"kind": "topic", "handle": "instance", "name": "This instance",
                   "source": "test", "sid": sid}),
        )
        .await;
    assert!(
        !instance.contains("\"status\":\"blocked\"") || instance.contains("exact-handle"),
        "the instance topic was refused for a reason other than being there: {instance}"
    );
    surface
        .must(
            "capture",
            json!({
                "subject": "topic:instance",
                "content": "the instance works in one zone",
                "provenance": "testimony",
                "sets": {"timezone": "America/New_York"},
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
    seed_what_deploy_6_introduced(surface, &sid).await;
    seed_what_a_walk_and_a_labelled_type_meet_on_the_head(surface, &sid).await;

    // **A key written under `sets`, which a build that has the argument stores
    // in a class of its own.** The gate reads it back on the thing and reads the
    // class in the store, so a recording from a build without the argument cannot
    // serve this case: that build refuses the argument, and the recording fails
    // here rather than recording a store that lacks the row.
    surface
        .must(
            "capture",
            json!({
                "subject": "person:upgrade-fixture-person",
                "content": "a property of the thing, set by this claim",
                "provenance": "testimony",
                "sets": {"upgrade_fixture_setting": "kept-under-sets"},
                "sid": sid,
            }),
        )
        .await
        .expect("the key written under sets is captured");

    // **A bot's creation under a `request_id` writes in two commits**, the entity
    // and then its box, and a build that keeps the parts of such a write holds one
    // row for each, so a store it filled has rows in that table.
    add_screened(
        surface,
        json!({"kind": "bot", "handle": "upgrade-fixture-parted",
               "name": "A Recorded Parted Bot", "source": "test",
               "request_id": "upgrade-fixture-parted-bot", "sid": sid}),
    )
    .await
    .expect("the bot created under a request id is added");

    // **A run another sitting takes over by its name**, which is the one act that
    // sets the moment a pick-up happened. The run is quiet for the sweep only on a
    // stated day, so the first boot states today and the second states a day two
    // days on: it sweeps the run and offers it back (a swept run is offered for
    // seven days), and the third boot picks it up by its name.
    add_screened(
        surface,
        json!({"kind": "bot", "handle": "upgrade-fixture-taken",
               "name": "A Recorded Taken Bot", "source": "test", "sid": sid}),
    )
    .await
    .expect("the bot whose run is taken over is added");
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date();
    let first_sitting = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-taken", "brief": true, "today": today.to_string()}),
        )
        .await
        .expect("the first sitting boots");
    let first_sid = first_sitting["session"]["sid"]
        .as_str()
        .expect("a first boot carries a session id")
        .to_string();
    surface
        .must(
            "journal",
            json!({"entry": "began a run another sitting takes over", "sid": first_sid}),
        )
        .await
        .expect("the first sitting writes, so its run has a card");
    let two_days_on = today.saturating_add(jiff::Span::new().days(2)).to_string();
    let offer = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-taken", "brief": true, "today": two_days_on}),
        )
        .await
        .expect("the second sitting is offered the run");
    let offered_run = offer["session"]["choices"][0]["run"]
        .as_str()
        .unwrap_or_else(|| panic!("the offer names the run to pick up: {offer}"))
        .to_string();
    let offer_code = offer["session"]["offer_code"]
        .as_str()
        .unwrap_or_else(|| panic!("the offer carries a code: {offer}"))
        .to_string();
    let taken = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-taken", "brief": true, "resume": offered_run,
                   "offer_code": offer_code, "today": two_days_on}),
        )
        .await
        .expect("the second sitting picks the run up by its name");
    assert_eq!(
        taken["session"]["resumed"], true,
        "the run was not picked up: {taken}"
    );

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
    // **The story differs from the beat above on purpose.** A wrap whose story
    // is already in the chronology adds no entry, so the same words would record
    // a wrapped run with no closing entry in it. The run's focus is still the
    // label the claim set, so the closing entry carries it beside the story.
    surface
        .must(
            "wrap_session",
            json!({"sid": sid, "story": "closed the recording run after it seeded the store"}),
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
    let held_sid = held["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();

    // **A wrapped run that is its bot's newest, so its wrap window stays open.**
    // The first run is `assistant`'s and a newer `assistant` run follows it, which
    // ends its window. This bot has one run and wraps it, so the store holds a
    // window that is still open for the gate to read after the upgrade. The
    // second run, still open, writes the bot in: the first is wrapped and cannot.
    surface
        .must(
            "add_entity",
            json!({"kind": "bot", "handle": "upgrade-fixture-wrapper",
                   "name": "A Recorded Wrapper", "source": "test", "sid": held_sid}),
        )
        .await
        .expect("the second bot is added");
    let wrapper = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-wrapper", "brief": true}),
        )
        .await
        .expect("the second bot boots");
    let wrapper_sid = wrapper["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();
    surface
        .must(
            "journal",
            json!({"entry": "did one thing before wrapping", "sid": wrapper_sid}),
        )
        .await
        .expect("the second bot journals a beat");
    surface
        .must(
            "wrap_session",
            json!({"sid": wrapper_sid, "story": "wrapped the only run this bot had"}),
        )
        .await
        .expect("the second bot's run wraps");

    // **A run with a long history.** One bot, one run, a hundred and twenty
    // beats and a hundred and fifty reads, so the store holds hundreds of the
    // deployed build's per-write rows for a run. The gate boots over it.
    surface
        .must(
            "add_entity",
            json!({"kind": "bot", "handle": "upgrade-fixture-chatterer",
                   "name": "A Recorded Chatterer", "source": "test", "sid": held_sid}),
        )
        .await
        .expect("the chatterer is added");
    let chatterer = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-chatterer", "brief": true}),
        )
        .await
        .expect("the chatterer boots");
    let chatter_sid = chatterer["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();
    for beat in 0..120 {
        surface
            .must(
                "journal",
                json!({"entry": format!("beat {beat} of a long run"),
                       "focus": format!("beat {beat}"), "sid": chatter_sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("beat {beat}: {e:#}"));
    }
    for _ in 0..150 {
        surface
            .must(
                "recall",
                json!({"subject": "person:upgrade-fixture-person", "facts": true,
                       "sid": chatter_sid}),
            )
            .await
            .expect("a read of the long run");
    }
    // **The run reports two compactions**, each setting back what it had been served,
    // with reads between them, so the store holds two dated changes holding a total
    // each and a run served again since the last.
    for reads in [20, 5] {
        surface
            .must("start_here", json!({"compacted": true, "sid": chatter_sid}))
            .await
            .expect("the long run reports a compaction");
        for _ in 0..reads {
            surface
                .must(
                    "recall",
                    json!({"subject": "person:upgrade-fixture-person", "facts": true,
                           "sid": chatter_sid}),
                )
                .await
                .expect("a read after a compaction");
        }
    }
    surface
        .must(
            "wrap_session",
            json!({"sid": chatter_sid, "story": "talked for a long time"}),
        )
        .await
        .expect("the long run wraps");

    // **The holder is recorded by the run's public name where a build has one.**
    // A build that keeps the key only as a hash has no name to find the run by
    // but the one it hands back as `run`; an older build's sid was the name.
    held["session"]["run"]
        .as_str()
        .map_or(held_sid, str::to_string)
}

/// **Two shapes the deployed build lets a caller write and a later build reads
/// differently**, seeded through the recording build's own surface.
///
/// A dependency list rewritten over several records: a piece of work's
/// `depends_on` is written three times, each a whole value, so the older records
/// still carry items the newest no longer holds. And a type a caller declared
/// with a key that a later build ships as a describing key (`subject`), which the
/// recording build accepts as an ordinary key.
async fn seed_what_a_walk_and_a_labelled_type_meet_on_the_head(surface: &Surface, sid: &str) {
    for (slug, name) in [
        ("gamma", "A Recorded Second Task"),
        ("delta", "A Recorded Third Task"),
        ("epsilon", "A Recorded Fourth Task"),
    ] {
        add_screened(
            surface,
            json!({"kind": "work", "handle": slug, "name": name, "source": "test",
                   "parent": "project:upgrade-fixture-project", "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("add_entity work:{slug}: {e:#}"));
    }
    for (content, depends_on) in [
        (
            "the fourth task waits on three pieces of work",
            "work:upgrade-fixture-prior, work:gamma, work:delta",
        ),
        (
            "the fourth task waits on two pieces of work",
            "work:gamma, work:delta",
        ),
        ("the fourth task waits on one piece of work", "work:delta"),
    ] {
        surface
            .must(
                "capture",
                json!({
                    "subject": "work:epsilon",
                    "content": content,
                    "provenance": "testimony",
                    "sets": {"depends_on": depends_on},
                    "sid": sid,
                }),
            )
            .await
            .expect("the dependency list is rewritten");
    }
    // **A shape only older builds accept.** Deploy 9.1 refuses a type that
    // declares one of the keys describing a record (`subject` among them), so a
    // store that build fills never holds one; the deploy 8.1 fixture is where the
    // upgrade over it is proved. Any other refusal is still a failed seed.
    let declared = surface
        .call(
            "declare_type",
            json!({
                "name": "upgrade-fixture-labelled",
                "fields": [{"key": "subject", "holds": "text"}],
                "sid": sid,
            }),
        )
        .await;
    let answer: serde_json::Value =
        serde_json::from_str(&declared).expect("declare_type answers JSON");
    if answer["status"] == "blocked" {
        let why = answer["how_to_proceed"].as_str().unwrap_or_default();
        assert!(
            why.contains("keys that describe a record"),
            "a type naming the key subject is declared, or refused as a describing key: {declared}"
        );
        eprintln!("this build refuses a type declaring `subject`; the fixture holds none");
    } else {
        // **An older build accepts it, and the fixture must then hold it**: the
        // answer lists the types as the store reads them back, so a declare that
        // answered without error and stored nothing is a failed seed.
        assert!(
            answer["types"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|known| known["name"] == "upgrade-fixture-labelled"),
            "the type naming the key subject is declared and read back: {declared}"
        );
    }
}

/// **What the build deploy 6 shipped writes and earlier builds do not**, seeded
/// through the recording build's own surface: a numbered collection, a view that
/// takes inputs, merges that leave an event, documents, and runs with a trail of
/// entries and a wrap window. The gate reads each back after the migrations of
/// the build that follows have run.
async fn seed_what_deploy_6_introduced(surface: &Surface, sid: &str) {
    // **A numbered collection.** The collection numbers its members by `number`;
    // one member is archived and keeps its number, so the next number skips it.
    add_screened(
        surface,
        json!({"kind": "topic", "handle": "upgrade-fixture-ordinances",
                   "name": "Recorded ordinances", "source": "test", "sid": sid}),
    )
    .await
    .expect("the collection is added");
    surface
        .must(
            "capture",
            json!({"subject": "topic:upgrade-fixture-ordinances",
                   "content": "numbers its members by number", "provenance": "testimony",
                   "sets": {"numbers_by": "number"}, "sid": sid}),
        )
        .await
        .expect("the collection numbers its members");
    for (slug, name, number) in [
        ("upgrade-fixture-ordinance-curfew", "Recorded curfew", "1"),
        ("upgrade-fixture-ordinance-parking", "Recorded parking", "2"),
        ("upgrade-fixture-ordinance-signs", "Recorded signs", "4"),
    ] {
        add_screened(
            surface,
            json!({"kind": "decision", "handle": slug, "name": name, "source": "test",
                       "parent": "topic:upgrade-fixture-ordinances",
                       "sets": {"number": number}, "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("add_entity decision:{slug}: {e:#}"));
    }
    surface
        .must(
            "archive_entity",
            json!({"handle": "decision:upgrade-fixture-ordinance-signs",
                   "reason": "recorded as repealed", "sid": sid}),
        )
        .await
        .expect("a member is archived and keeps its number");

    // **A view that takes an input**, saved as a record with the input typed.
    add_screened(
        surface,
        json!({"kind": "view", "handle": "upgrade-fixture-members-of",
                   "name": "Recorded members of", "source": "test",
                   "sets": {"inputs": "collection: kind topic", "parent": "$collection"},
                   "sid": sid}),
    )
    .await
    .expect("the view with an input is added");

    // **A chain of merges, each leaving an event with its claim map.** The first
    // folds into the second and the second into the third, so a claim filed under
    // the first is served as the last one's.
    for (slug, name) in [
        ("upgrade-fixture-fold-a", "A Recorded Fold A"),
        ("upgrade-fixture-fold-b", "A Recorded Fold B"),
        ("upgrade-fixture-fold-c", "A Recorded Fold C"),
    ] {
        add_screened(
            surface,
            json!({"kind": "thing", "handle": slug, "name": name, "source": "test",
                       "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("add_entity thing:{slug}: {e:#}"));
        surface
            .must(
                "capture",
                json!({"subject": format!("thing:{slug}"),
                       "content": format!("the claim filed under {slug}"),
                       "provenance": "testimony", "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("capture on thing:{slug}: {e:#}"));
    }
    for (duplicate, survivor) in [
        (
            "thing:upgrade-fixture-fold-a",
            "thing:upgrade-fixture-fold-b",
        ),
        (
            "thing:upgrade-fixture-fold-b",
            "thing:upgrade-fixture-fold-c",
        ),
    ] {
        surface
            .must(
                "merge_entities",
                json!({"duplicate": duplicate, "survivor": survivor,
                       "reason": "recorded as a chain", "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("merge {duplicate} into {survivor}: {e:#}"));
    }

    // **Documents**: one put, one section edited, and a second folded into it.
    for (slug, name, markdown) in [
        (
            "upgrade-fixture-manual",
            "A Recorded Manual",
            "# Recorded manual\n\nRead this first.\n\n## Opening\n\nSwitch the fryer on.\n",
        ),
        (
            "upgrade-fixture-appendix",
            "A Recorded Appendix",
            "# Recorded appendix\n\nThe last word.\n",
        ),
    ] {
        add_screened(
            surface,
            json!({"kind": "document", "handle": slug, "name": name, "source": "test",
                       "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("add_entity document:{slug}: {e:#}"));
        surface
            .must(
                "put_document",
                json!({"document": format!("document:{slug}"), "markdown": markdown,
                       "sid": sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("put_document document:{slug}: {e:#}"));
    }
    surface
        .must(
            "merge_entities",
            json!({"duplicate": "document:upgrade-fixture-appendix",
                   "survivor": "document:upgrade-fixture-manual",
                   "reason": "recorded as one document", "sid": sid}),
        )
        .await
        .expect("the appendix is folded into the manual");

    // **Runs with a trail.** A bot boots, writes entries, amends the newest, wraps
    // (which hands back a code), is reopened by that code for one last change and
    // wraps again. The trail is the run's entries, its wrap window and the key the
    // reopening replaced.
    add_screened(
        surface,
        json!({"kind": "bot", "handle": "upgrade-fixture-trail",
                   "name": "A Recorded Trail", "source": "test", "sid": sid}),
    )
    .await
    .expect("the trail bot is added");
    let trail = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-trail", "brief": true}),
        )
        .await
        .expect("the trail bot boots");
    let trail_sid = trail["session"]["sid"]
        .as_str()
        .expect("a fresh boot carries a session id")
        .to_string();
    for beat in 0..3 {
        surface
            .must(
                "journal",
                json!({"entry": format!("trail beat {beat}"),
                       "focus": format!("trail focus {beat}"), "sid": trail_sid}),
            )
            .await
            .unwrap_or_else(|e| panic!("trail beat {beat}: {e:#}"));
    }
    surface
        .must(
            "amend_journal",
            json!({"entry": "trail beat 2, amended", "sid": trail_sid}),
        )
        .await
        .expect("the newest entry is amended");
    let wrapped = surface
        .must(
            "wrap_session",
            json!({"sid": trail_sid, "story": "walked a trail and wrapped it"}),
        )
        .await
        .expect("the trail run wraps");
    let wrap_code = wrapped["wrap_code"]
        .as_str()
        .expect("the first wrap hands back a code")
        .to_string();
    let reopened = surface
        .must(
            "start_here",
            json!({"bot": "upgrade-fixture-trail", "resume": wrap_code, "brief": true}),
        )
        .await
        .expect("the wrap code reopens the run");
    let reopened_sid = reopened["session"]["sid"]
        .as_str()
        .expect("the reopening hands the run's key back")
        .to_string();
    surface
        .must(
            "journal",
            json!({"entry": "trail beat after the reopening", "sid": reopened_sid}),
        )
        .await
        .expect("the reopened run takes one last change");
    surface
        .must(
            "wrap_session",
            json!({"sid": reopened_sid, "story": "walked the trail again and wrapped it"}),
        )
        .await
        .expect("the reopened run wraps a second time");
}

/// **Whether a `declare_type` answer leaves the recording where it meant to be.**
/// A declaration that landed does. A refusal does only when it is the one a
/// build that already ships the type gives, which is the one outcome that means
/// there is no row to record. Anything else does not: a refusal for another
/// reason, a transport failure, or an answer that is not JSON at all.
fn a_declaration_reached_its_starting_state(answer: &str) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(answer) else {
        return false;
    };
    if !body["transport_error"].is_null() {
        return false;
    }
    body["status"] != "blocked"
        || body["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("is a type that ships with the software"))
}

/// **What a store that has run for months holds and the plain records above do
/// not**: a type a caller named after a shipped kind, a handle in a field that
/// two things have worn, a list of handles under a key nobody declared, a message somebody decided is unreadable, an archived thing, and a
/// bot filled to the deployed build's boot ceiling. Each is written through the
/// deployed build's own surface, so each is a row that build really wrote.
async fn seed_what_a_real_store_holds_beyond_the_plain_records(surface: &Surface, sid: &str) {
    // **A type named for a shipped kind.** The deployed build lets a caller
    // declare it; a later build ships a kind of the same name and has to boot.
    //
    // **A build that already ships the type refuses it, and that refusal is the
    // only one taken here.** The gap exists in a store only if the recording
    // build let the caller declare the name: a later build ships `topic` as a
    // type too, so on it there is no such row to record. Any other refusal is
    // still a recording that did not reach its starting state.
    let declared = surface
        .call(
            "declare_type",
            json!({
                "name": "topic",
                "fields": [{"key": "upgrade_fixture_mood", "holds": "text"}],
                "sid": sid,
            }),
        )
        .await;
    assert!(
        a_declaration_reached_its_starting_state(&declared),
        "a type named topic is declared, or the build already ships it: {declared}"
    );

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
                "sets": {"upgrade_fixture_points_at": "thing:upgrade-fixture-namesake"},
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
                "sets": {"upgrade_fixture_company":
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

/// **The predicate takes a landed declaration and the shipped-type refusal, and
/// nothing else.** Each of the others is a recording that did not reach its
/// starting state, and each is paired with the two outcomes that do, so a
/// predicate that refused everything would not pass either.
#[test]
fn only_a_landed_declaration_or_the_shipped_type_refusal_reaches_the_starting_state() {
    let landed = json!({"name": "topic", "status": "declared"}).to_string();
    let shipped = json!({
        "status": "blocked",
        "how_to_proceed": "'topic' is a type that ships with the software, so it is not declared again",
    })
    .to_string();
    assert!(a_declaration_reached_its_starting_state(&landed));
    assert!(a_declaration_reached_its_starting_state(&shipped));

    let other_refusal = json!({
        "status": "blocked",
        "how_to_proceed": "the name is not a valid type name",
    })
    .to_string();
    let transport = json!({"transport_error": "connection reset by peer"}).to_string();
    assert!(!a_declaration_reached_its_starting_state(&other_refusal));
    assert!(!a_declaration_reached_its_starting_state(&transport));
    assert!(!a_declaration_reached_its_starting_state("not json at all"));
}
