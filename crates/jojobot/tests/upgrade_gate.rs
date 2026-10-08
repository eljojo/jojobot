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

const FIXTURE_DIR: &str = "tests/fixtures/upgrade";
/// The fixture recorded from the build deploy 3 shipped, kept beside the first:
/// a store that build filled is what a deployed instance holds when it takes
/// this one.
const DEPLOY_3_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy3";

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **The fixture, restored into a store of this case's own.** Two cases run
/// side by side in this binary, so each takes a directory named for itself: a
/// name from the process alone would be one directory for both.
struct Restored {
    /// Removes the directory when the case is done.
    scratch: Scratch,
    state_dir: std::path::PathBuf,
    store_port: u16,
    git_ref: String,
    /// What every thing held in the old store, as `(handle, key, value)`, read
    /// before the current binary touched it.
    held_before: Vec<(String, String, String)>,
}

async fn restore_the_fixture(label: &str) -> Restored {
    restore_a_fixture(FIXTURE_DIR, label).await
}

/// **A fixture directory, restored into a store of this case's own.**
async fn restore_a_fixture(fixture_dir: &str, label: &str) -> Restored {
    let fixture_ref = format!("{fixture_dir}/ref.txt");
    let fixture_dump = format!("{fixture_dir}/doltdump.sql");
    let git_ref = std::fs::read_to_string(&fixture_ref)
        .unwrap_or_else(|e| panic!("reading {fixture_ref}: {e}"))
        .trim()
        .to_string();

    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-upgrade-gate-{}-{label}",
        std::process::id()
    ));
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
    let dump = std::fs::read_to_string(&fixture_dump)
        .unwrap_or_else(|e| panic!("reading {fixture_dump}: {e}"));
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(&statement)
            .execute(restoring.pool())
            .await
            .unwrap_or_else(|e| {
                panic!("replaying the fixture recorded at {git_ref} failed on:\n{statement}\n{e}")
            });
    }
    // **The old claim is moved to ten minutes ago, as the store of a deploy a
    // little while ago holds it.** The recording is hours old and its lease has
    // lapsed by the time this runs, and a lapsed lease proves nothing about a
    // role being read in the shape it was written in. Ten minutes is inside the
    // lease and past the renewal age: a younger moment would make the holder's
    // write renew nothing, so the claim would not move. This is the one edit made
    // to the recording, and it changes a moment, not a shape: the key is the old
    // build's own.
    sqlx::query("UPDATE field_write SET value = ? WHERE `key` = ?")
        .bind((jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string())
        .bind("role/upgrade-fixture-holder/claimed_at")
        .execute(restoring.pool())
        .await
        .expect("the recorded claim's moment is moved to now");
    // **What every thing holds, read off the old store with plain SQL before the
    // current binary touches it.** Independent of any fold in this build, so a
    // migration or a filter that changed what a thing holds cannot move this side
    // with it.
    let held_before = fields_every_thing_holds(restoring.pool()).await;
    assert!(
        held_before.len() >= 5,
        "the fixture holds things with fields, or this compares nothing: {held_before:?}"
    );
    restoring.stop().await;
    Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
    }
}

#[tokio::test]
async fn the_current_binary_boots_on_a_store_an_older_binary_filled() {
    boots_on_a_store_filled_by(FIXTURE_DIR, "boots").await;
}

/// The same proof over the store the build deploy 3 shipped filled.
#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_3_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_3_FIXTURE_DIR, "boots-deploy3").await;
}

async fn boots_on_a_store_filled_by(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
    } = restore_a_fixture(fixture_dir, label).await;

    // **The current binary boots over the restored store TWICE, and the second
    // boot changes nothing.** The first boot runs every migration and every
    // start pass the build has. A pass that is not idempotent, or a boot that
    // writes on every start, shows as a store that differs after the second
    // boot, and no read of the store would say so. The store is stopped and
    // dumped after each boot, before anything reads it, because a read through
    // the surface is itself attributed.
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let after_first = dump_store(&state_dir).await;
    let second = boot_current(&state_dir, store_port, &git_ref).await;
    second.stop().await;
    let after_second = dump_store(&state_dir).await;
    if after_first != after_second {
        let changed: Vec<String> = after_first
            .lines()
            .zip(after_second.lines())
            .filter(|(before, after)| before != after)
            .take(5)
            .map(|(before, after)| format!("- {before}\n+ {after}"))
            .collect();
        panic!(
            "the second boot of the current binary changed the store recorded at {git_ref} \
             (a boot that writes on every start, or a pass that is not idempotent). {} lines \
             before, {} after. First differences:\n{}",
            after_first.lines().count(),
            after_second.lines().count(),
            changed.join("\n")
        );
    }

    // The third boot is the one every read below goes through.
    let third = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", third.http_port))
        .await
        .expect("connecting to the current binary");
    let role_holder_file = format!("{fixture_dir}/role_holder.txt");
    let role_holder = std::fs::read_to_string(&role_holder_file)
        .unwrap_or_else(|e| panic!("reading {role_holder_file}: {e}"))
        .trim()
        .to_string();
    // **Before the gate writes anything of its own**: the steps below write to
    // the store, and a thing that has been written to is meant to hold more. The
    // first two boots wrote nothing of the gate's, and the dump above shows the
    // second changed nothing the first left.
    assert_every_thing_holds_what_it_held(&surface, &git_ref, &held_before).await;
    assert_every_recorded_record_reads_back(&surface, &git_ref, &role_holder).await;
    assert_what_a_real_store_holds_reads_back(&surface, &git_ref, &role_holder).await;
    assert_a_role_claimed_in_the_old_shape_is_held_and_moves(&surface, &git_ref, &role_holder)
        .await;
    surface.finish().await;
    third.stop().await;

    // The role is now claim-shaped, with the claim the holder's write made. A
    // fourth boot is the restart of the deployed server over it.
    assert_a_claim_shaped_lease_renews_in_place(&state_dir, store_port, &git_ref, &role_holder)
        .await;
    drop(scratch);
}

/// **The one warning a clean boot of the gate logs.** The gate starts the binary
/// with no issuer on purpose, so the boot says the endpoint is open. Any other
/// warning is a degradation.
const WARNING_THE_GATE_CAUSES: &str = "AUTH DISABLED";

/// **The warning a boot logs for the handle two things have worn**, naming the
/// field it sits under. The recording writes that handle on purpose.
fn is_the_recorded_ambiguous_handle(line: &str) -> bool {
    line.contains("FIELD VALUE LEFT AS TEXT") && line.contains("upgrade_fixture_points_at")
}

/// **Whether the recording made at `git_ref` holds the handle two things have
/// worn as plain text** under the field the gate looks for. Read off the dump
/// itself, so the answer is the recording's and not a list kept beside it: a
/// field value is `'<key>',<ordinal>,'<value>'` in the dump, and a plain handle
/// is a value that starts with a kind, where a stored id starts with the mark.
fn recording_holds_the_plain_ambiguous_handle(git_ref: &str) -> bool {
    let dir = [FIXTURE_DIR, DEPLOY_3_FIXTURE_DIR]
        .into_iter()
        .find(|dir| {
            std::fs::read_to_string(format!("{dir}/ref.txt"))
                .is_ok_and(|recorded| recorded.trim() == git_ref)
        })
        .unwrap_or_else(|| panic!("no fixture was recorded at {git_ref}"));
    let dump = std::fs::read_to_string(format!("{dir}/doltdump.sql"))
        .unwrap_or_else(|e| panic!("reading the dump in {dir}: {e}"));
    dump.split("'upgrade_fixture_points_at',")
        .skip(1)
        .any(|rest| {
            rest.split(',')
                .nth(1)
                .is_some_and(|value| value.starts_with("'thing:"))
        })
}

/// **A running copy of the current binary over a restored store.**
struct Booted {
    child: tokio::process::Child,
    http_port: u16,
}

impl Booted {
    /// Stop it, and wait until the store's own lock is released: the dump that
    /// follows needs the directory to itself.
    async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// **Boot the current binary on the store in `state_dir`** and hold it to the
/// boot's own account of itself: it reaches its serving line, and it says no
/// part of its own boot failed.
async fn boot_current(state_dir: &std::path::Path, store_port: u16, git_ref: &str) -> Booted {
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", state_dir)
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

    // **Every warning the boot logs fails the gate, except the one the gate
    // causes.** Each says a half of the boot failed or degraded and carried on:
    // the fold or the search index came up empty, a row was left as text or not
    // rekeyed, a migration failed and will retry. The server serves and every
    // read of a store that was upgraded looks fine until somebody asks the half
    // that is missing. A filter that names each message lets a new one through
    // unseen, so the filter is the level, with one named exception.
    //
    // **An error line fails it too, whatever it says.** The boot logs a pass it
    // could not finish at the error level.
    let warned: Vec<&String> = seen
        .iter()
        .filter(|line| {
            line.contains("ERROR")
                || (line.contains("WARN")
                    && !line.contains(WARNING_THE_GATE_CAUSES)
                    && !is_the_recorded_ambiguous_handle(line))
        })
        .collect();
    assert!(
        warned.is_empty(),
        "the current binary booted on a store recorded at {git_ref} and said part of its own \
         boot failed: {warned:?}"
    );
    // **The one degradation the recording holds is reported, once, every boot.** A
    // handle two things have worn cannot be lowered, so each boot says so by
    // name. A boot that says nothing has stopped seeing the row, and the gate
    // would then pass over a store it no longer exercises.
    //
    // **Only a recording that holds the handle as plain text has one to report.**
    // A build that stores every field value as a permanent id leaves nothing to
    // lower, so a store it filled holds no such row and its boot says nothing,
    // and a boot that reported one there would be inventing it. The recording
    // itself says which it is.
    let holds_it = recording_holds_the_plain_ambiguous_handle(git_ref);
    let reported = seen
        .iter()
        .filter(|line| is_the_recorded_ambiguous_handle(line))
        .count();
    assert_eq!(
        reported,
        usize::from(holds_it),
        "the boot of a store recorded at {git_ref} must report the ambiguous handle exactly when \
         the recording holds it as plain text (it does: {holds_it}): {seen:?}"
    );
    Booted { child, http_port }
}

/// **The whole store as text**, dumped by the store's own tool from the
/// directory the binary keeps it in, with no binary running. The same dump the
/// recording is made with, so two of them compare row for row.
async fn dump_store(state_dir: &std::path::Path) -> String {
    let database_dir = state_dir.join("db").join("jojobot");
    assert!(
        database_dir.join(".dolt").is_dir(),
        "the binary's database directory is not where boot_store puts it: {}",
        database_dir.display()
    );
    // The store's tool keeps its global configuration under a home directory
    // and fails where the environment names none (a build sandbox). The run's
    // own state directory holds it, as the adapter does for every store it
    // spawns.
    let dolt_home = state_dir.join("dolt-dump-home");
    std::fs::create_dir_all(&dolt_home).expect("a directory for the dump tool's configuration");
    let dump = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", &dolt_home)
        .arg("dump")
        .arg("-r")
        .arg("sql")
        .arg("-f")
        .arg("-fn")
        .arg("gate-compare.sql")
        .current_dir(&database_dir)
        .output()
        .await
        .expect("dolt dump runs");
    assert!(
        dump.status.success(),
        "dolt dump failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );
    let text = std::fs::read_to_string(database_dir.join("gate-compare.sql"))
        .expect("the dump is where dolt put it");
    let _ = std::fs::remove_file(database_dir.join("gate-compare.sql"));
    text
}

/// **A lease on a role object survives a restart, and a renewal past the renewal
/// age overwrites its moment instead of adding a write.** The store is left as
/// the previous steps made it, the moment on the role object is aged to ten
/// minutes ago with the binary stopped, and the current binary boots over it. The
/// holder is read back, a rival is refused, and the holder's next write renews:
/// the moment is new and the key still holds the writes it had.
async fn assert_a_claim_shaped_lease_renews_in_place(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    role_holder: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let database_dir = state_dir.join("db").join("jojobot");
    let aged = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    let aging = tokio::process::Command::new("dolt")
        .arg("sql")
        .arg("-q")
        .arg(format!(
            "UPDATE field_write SET value = '{aged}' WHERE `key` = 'claimed_at' AND entity = \
             (SELECT COALESCE(badge, id) FROM entity WHERE id = 'role:upgrade-fixture-holder')"
        ))
        .current_dir(&database_dir)
        .output()
        .await
        .expect("dolt sql runs");
    if !aging.status.success() {
        fail(
            "the role object's moment could not be aged",
            &String::from_utf8_lossy(&aging.stderr),
        );
    }

    let fourth = boot_current(state_dir, store_port, git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", fourth.http_port))
        .await
        .expect("connecting to the current binary");
    let read_role = || async {
        let read = surface
            .call(
                "recall",
                json!({"subject": "role:upgrade-fixture-holder", "history": "claimed_at"}),
            )
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail("the role object", &read));
        parsed["objects"][0].clone()
    };

    let before = read_role().await;
    if before["fields"]["holder"] != role_holder || before["fields"]["claimed_at"] != aged {
        fail(
            "the claim-shaped lease did not read back across a restart",
            &before.to_string(),
        );
    }
    let rival = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new",
                   "claim": "upgrade-fixture-holder"}),
        )
        .await;
    if !rival.contains("\"refused\"") {
        fail("a rival was not refused by the restarted lease", &rival);
    }

    let beat = surface
        .call(
            "journal",
            json!({"entry": "renewed after the restart", "sid": role_holder}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the restart was refused", &beat);
    }
    let after = read_role().await;
    if after["fields"]["claimed_at"] == before["fields"]["claimed_at"] {
        fail("the renewal did not move the moment", &after.to_string());
    }
    if after["history"]["count"] != before["history"]["count"] {
        fail(
            "the renewal added a write to the lease moment",
            &format!("{} then {}", before["history"], after["history"]),
        );
    }
    surface.finish().await;
    fourth.stop().await;
}

/// **Every value a thing holds in the old store**, as `(handle, key, value)`:
/// the newest write of each key among the writes of records that still stand.
///
/// **A value stored as permanent ids is read here as the handles those ids answer
/// to**, off the same old store: each id is replaced, in place, by the handle of
/// the entity wearing it. What the upgraded binary serves is then
/// compared with that, position by position, so a list served in another order
/// or naming other things is a difference and not a match.
async fn fields_every_thing_holds(pool: &sqlx::MySqlPool) -> Vec<(String, String, String)> {
    let badges: std::collections::BTreeMap<String, String> =
        sqlx::query_as::<_, (Option<String>, String)>("SELECT badge, id FROM entity")
            .fetch_all(pool)
            .await
            .expect("the old store's badges read")
            .into_iter()
            .filter_map(|(badge, id)| badge.map(|badge| (badge, id)))
            .collect();
    let held: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT e.id, w.`key`, w.value
         FROM field_write w
         JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id AND f.status = 'active'
         JOIN entity e ON e.badge = w.entity
         WHERE w.value IS NOT NULL
           -- The two keys jojobot writes about a record and never folds onto a thing.
           AND w.`key` NOT IN ('merged_from', 'retracts')
           -- A key declared to describe its record is the record's own and the thing
           -- never holds it.
           AND w.`key` NOT IN (SELECT key_name FROM type_field WHERE folds = 'describes')
           AND w.ordinal = (
             SELECT MAX(w2.ordinal)
             FROM field_write w2
             JOIN fact f2 ON f2.entity = w2.entity AND f2.id = w2.fact_id AND f2.status = 'active'
             WHERE w2.entity = w.entity AND w2.`key` = w.`key`)
         ORDER BY e.id, w.`key`",
    )
    .fetch_all(pool)
    .await
    .expect("the old store's folded fields read");
    held.into_iter()
        .map(|(handle, key, value)| (handle, key, with_badges_as_handles(&value, &badges)))
        .collect()
}

/// **A stored value with each item that is a badge replaced by the handle of the
/// entity wearing it.** An item is one piece of a comma list, and it is a badge
/// when it is one an entity wears, with or without the `@#` mark: a field a type
/// declared a reference holds the bare id, any other key holds the marked one.
/// Separators and every other item are untouched. A badge nothing wears is left
/// as it is, so the comparison against it fails loudly.
fn with_badges_as_handles(
    value: &str,
    badges: &std::collections::BTreeMap<String, String>,
) -> String {
    value
        .split(',')
        .map(|piece| {
            let item = piece.trim();
            let bare = item.strip_prefix("@#").unwrap_or(item);
            match badges.get(bare) {
                Some(handle) => piece.replacen(item, handle, 1),
                None => piece.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// **The upgrade changed nothing a thing holds.** Each value the old store held
/// is still held, under the same key, after the current binary has booted on it.
/// A value that names another thing was stored as that thing's permanent id and
/// is served as the handle the thing answers to; it is compared as that handle,
/// item by item and in order.
async fn assert_every_thing_holds_what_it_held(
    surface: &Surface,
    git_ref: &str,
    held_before: &[(String, String, String)],
) {
    eprintln!(
        "upgrade gate: comparing {} values the store recorded at {git_ref} held before the upgrade",
        held_before.len()
    );
    for (handle, key, was) in held_before {
        let read = surface.call("recall", json!({"subject": handle})).await;
        let parsed: serde_json::Value = serde_json::from_str(&read).unwrap_or_else(|_| {
            panic!("recorded at {git_ref}: {handle} did not read back: {read}")
        });
        let now = parsed["objects"][0]["fields"][key]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade \
                     and holds nothing under it after: {read}"
                )
            });
        assert_eq!(
            now, was,
            "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade (ids \
             read as the handles they answered to) and holds '{now}' after"
        );
    }
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

/// **A role claimed before roles were objects is held by the current binary,
/// and its holder's next write past the renewal age moves it onto the object.**
/// Release 1 reads both shapes and writes the new one, so the store a deploy
/// leaves behind must still refuse a rival while the old lease is fresh, then show
/// the claim on the role object once its holder writes, with a rival still refused
/// after. Without the
/// both-shapes read, every role in the store would read as empty here and the
/// rival would be seated.
async fn assert_a_role_claimed_in_the_old_shape_is_held_and_moves(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let rival = |label: &'static str| async move {
        let read = surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new",
                       "claim": "upgrade-fixture-holder"}),
            )
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail(label, &read));
        parsed["session"]["claim"].clone()
    };

    let before = rival("a rival's claim on a role held in the old shape").await;
    if before["status"] != "refused" || before["holder"] != role_holder {
        fail(
            "a role held in the old shape did not refuse a rival, naming its holder",
            &before.to_string(),
        );
    }

    // The holder writes. That renews the claim, and the renewal lands on the role
    // object, which did not exist.
    let beat = surface
        .call(
            "journal",
            json!({"entry": "worked on after the upgrade", "sid": role_holder}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the upgrade was refused", &beat);
    }
    let read = surface
        .call("recall", json!({"kind": "role", "parent": "bot:assistant"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role objects", &read));
    let object = &parsed["objects"][0];
    if object["id"] != "role:upgrade-fixture-holder"
        || object["fields"]["holder"] != role_holder
        || object["fields"]["claimed_at"].as_str().is_none()
    {
        fail(
            "the holder's write did not move its claim onto the role object",
            &read,
        );
    }

    let after = rival("a rival's claim after the claim moved").await;
    if after["status"] != "refused" || after["holder"] != role_holder {
        fail(
            "the moved claim did not still refuse a rival, naming its holder",
            &after.to_string(),
        );
    }
}

/// **What a store that has run for a while holds beyond the plain records, read
/// back through the served surface.** Each of these is a row the deployed build
/// wrote: a type named for a shipped kind, a handle two things have worn, a list
/// of handles under a key nobody declared, a message somebody decided is
/// unreadable, an archived thing, and a bot filled to the deployed build's boot
/// ceiling. The recording is the primary source; what each read answers here was
/// read off the upgraded store, not written from what it ought to say.
async fn assert_what_a_real_store_holds_reads_back(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let json_of = |what: &str, read: &str| -> serde_json::Value {
        serde_json::from_str(read).unwrap_or_else(|_| fail(what, read))
    };

    // A type a caller named `topic` sits beside the shipped kind of that name.
    let read = surface.call("start_here", json!({"brief": true})).await;
    let boot = json_of("the vocabulary", &read);
    let vocabulary = &boot["snapshot"]["vocabulary"];
    if !vocabulary["types"]
        .as_array()
        .is_some_and(|types| types.contains(&json!({"type": "topic"})))
        || !vocabulary["kinds"]
            .as_array()
            .is_some_and(|kinds| kinds.contains(&json!({"kind": "topic"})))
    {
        fail("a type named topic beside the topic kind", &read);
    }

    // The handle two things have worn names the later one, and the thing it was
    // taken from is found at the handle it was renamed to. The field that holds
    // the handle reads back as the text it was written as: the boot could not
    // lower it, and said so.
    for (handle, name) in [
        ("thing:upgrade-fixture-namesake", "A Later Heir"),
        ("thing:upgrade-fixture-successor", "A Recorded Namesake"),
    ] {
        let read = surface.call("recall", json!({"subject": handle})).await;
        let parsed = json_of(handle, &read);
        if parsed["objects"][0]["id"] != handle || parsed["objects"][0]["name"] != name {
            fail(handle, &read);
        }
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let fields = json_of("the thing's fields", &read)["objects"][0]["fields"].clone();
    if fields["upgrade_fixture_points_at"] != "thing:upgrade-fixture-namesake" {
        fail("the ambiguous handle left as the text written", &read);
    }
    // A list of handles under a key nobody declared reads back as written.
    if fields["upgrade_fixture_company"]
        != "person:upgrade-fixture-person, place:upgrade-fixture-place"
    {
        fail("the list of handles", &read);
    }

    // A quarantined message is counted apart and opens for nobody, and the
    // refusal carries the reason it was set aside with.
    let read = surface
        .call(
            "read_mailbox",
            json!({"counts_only": true, "sid": role_holder}),
        )
        .await;
    let counted = json_of("the mailbox counts", &read);
    let quarantined = &counted["quarantined"];
    let id = match (
        quarantined["count"].as_u64(),
        quarantined["ids"][0].as_str(),
    ) {
        (Some(1), Some(id)) => id.to_string(),
        _ => fail("one quarantined message", &read),
    };
    let read = surface
        .call(
            "read_message",
            json!({"message_id": id, "sid": role_holder}),
        )
        .await;
    if json_of("the quarantined message", &read)["status"] != "blocked"
        || !read.contains("recorded as unreadable")
    {
        fail("the quarantined message", &read);
    }

    // An archived thing is read by its handle, says why, and is left out of the
    // default listing, which counts what it left out.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-archived"}),
        )
        .await;
    if json_of("the archived thing", &read)["objects"][0]["archived"]["reason"]
        != "recorded as no longer wanted"
    {
        fail("the archived thing", &read);
    }
    let read = surface.call("recall", json!({"kind": "thing"})).await;
    let listed = json_of("the things", &read);
    if listed["archived_excluded"].as_u64() != Some(1)
        || listed["objects"].as_array().is_none_or(|all| {
            all.iter()
                .any(|o| o["id"] == "thing:upgrade-fixture-archived")
        })
    {
        fail("the archived thing left out of the listing", &read);
    }

    // A bot filled to the deployed build's ceiling, whose rule the deployed
    // build let in without the room for its timestamp, still boots and serves its
    // whole charter and its rule.
    let read = surface
        .call(
            "start_here",
            json!({"bot": "upgrade-fixture-heavy", "brief": true}),
        )
        .await;
    let heavy = json_of("the heavy bot's boot", &read);
    let charter = heavy["identity"]["charter"].as_str().unwrap_or("");
    if heavy["identity"]["bot"]["id"] != "bot:upgrade-fixture-heavy"
        || heavy["identity"]["charter_elided"] != false
        || charter.is_empty()
        || !charter.chars().all(|c| c == 'x')
        || !read.contains("the heavy bot keeps its one rule")
        || heavy["session"]["sid"].as_str().is_none()
    {
        fail(
            "the heavy bot's boot",
            &read.chars().take(400).collect::<String>(),
        );
    }
    // A new starred rule on it is refused, and the refusal says how much room it
    // held back for the new record's timestamp.
    let sid = heavy["session"]["sid"].as_str().unwrap_or_default();
    let read = surface
        .call(
            "capture",
            json!({
                "subject": "bot:upgrade-fixture-heavy",
                "content": "one more rule", "provenance": "testimony",
                "fields": {"starred": "true"}, "sid": sid,
            }),
        )
        .await;
    let refused = json_of("a new rule on the heavy bot", &read);
    let kept = jiff::Timestamp::new(0, 123_456_789)
        .expect("a moment")
        .to_string()
        .len()
        - jiff::Timestamp::new(0, 0)
            .expect("a moment")
            .to_string()
            .len();
    if refused["status"] != "blocked" || refused["stamp_margin"] != kept {
        fail("a new rule refused naming the stamp margin", &read);
    }
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
        // **The sender is the bot's handle**, not an id and not nothing.
        if !read.contains("\"sender\":\"bot:assistant\"") {
            fail(&format!("the sender of the {state} mail message"), &read);
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
    // **The starred rule rides the boot** of the identity it was written on.
    if !booted["identity"]["rules"].as_array().is_some_and(|rules| {
        rules
            .iter()
            .any(|rule| rule["content"] == "the recorder keeps its recording rule")
    }) {
        fail(
            "the starred rule on the assistant's boot",
            &booted.to_string(),
        );
    }
    assert_the_newer_shapes_read_back(surface, git_ref).await;
    // **What this build newly interprets, read from records the older build
    // wrote as free text, and held on the next write.**
    assert_the_project_keys_and_the_due_day_read_back_and_bind(surface, git_ref, sid).await;
}

/// **A project, work filed under it and a due day the older build stored, all
/// written by the older build.**
///
/// Each is read back as written. Then two writes go through the served surface:
/// a status the project lists lands, and a status it does not list is refused.
/// The refusal is what shows the build's own `project` keys hold the next write.
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
    let thing = fields_of(&read, "the thing with a stored due day");
    if thing["due_on"] != "2026-12-01" {
        fail("the stored due day", &read);
    }

    // **An edit that sets another key leaves the stored due day where it was**,
    // and is not refused because of it. The record is testimony an older build
    // wrote, so the key is one that only describes the record: a value field on
    // it is the operator's, and the gate's legacy case holds that refusal.
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
                .find(|f| f["content"] == "a day kept under decide_by")
        })
        .and_then(|fact| fact["address"].as_str())
        .unwrap_or_else(|| fail("the record carrying the stored due day", &read))
        .to_string();
    let landed = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"purpose": "a note on the day"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("an edit beside the stored due day", &landed);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the thing after the edit", &read));
    if parsed["objects"][0]["fields"]["due_on"] != "2026-12-01"
        || !read.contains("a note on the day")
    {
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
            "the project's own keys holding a status it does not list",
            &refused,
        );
    }
}

/// **The shapes the first fixture was missing, each read back as the older
/// build wrote it.** A claim read out of a system of record, a handle under a
/// key nothing declares between two bots, the instance's own zone, and a piece
/// of work at `done`.
async fn assert_the_newer_shapes_read_back(surface: &Surface, git_ref: &str) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let read_of = |read: &str, what: &str| -> serde_json::Value {
        serde_json::from_str(read).unwrap_or_else(|_| fail(what, read))
    };

    // The claim read out of a system of record keeps its provenance and the
    // system it names.
    let read = surface
        .call(
            "recall",
            json!({"subject": "place:upgrade-fixture-place", "facts": true}),
        )
        .await;
    let parsed = read_of(&read, "the recorded observation");
    let observed = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"] == "the recorded place opens at nine")
        })
        .unwrap_or_else(|| fail("the recorded observation", &read));
    if observed["provenance"] != "observation"
        || observed["fields"]["read_from"] != "the recorded place's page"
    {
        fail("the observation's provenance or the system it names", &read);
    }

    // The handle under an undeclared key is served as the handle.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant"}))
        .await;
    let parsed = read_of(&read, "the reports_to field");
    if parsed["objects"][0]["fields"]["reports_to"] != "bot:upgrade-fixture-lead" {
        fail("the handle under reports_to", &read);
    }
    // **A handle the older build kept as the text it was sent** is served as
    // the handle.
    if parsed["objects"][0]["fields"]["pairs_with"] != "bot:upgrade-fixture-lead" {
        fail("the handle under a key nothing declares", &read);
    }

    // The instance's own record keeps the zone it works in.
    let read = surface
        .call("recall", json!({"subject": "topic:instance"}))
        .await;
    let parsed = read_of(&read, "the instance record");
    if parsed["objects"][0]["fields"]["timezone"] != "America/New_York" {
        fail("the instance's zone", &read);
    }

    // The finished piece of work is still finished.
    let read = surface
        .call("recall", json!({"subject": "work:upgrade-fixture-prior"}))
        .await;
    let parsed = read_of(&read, "the finished work");
    if parsed["objects"][0]["fields"]["status"] != "done" {
        fail("the finished work's status", &read);
    }
}

/// **A pair written before a write had a moment of its own merges after the
/// upgrade, from each build the gate proves an upgrade from.**
///
/// Each fixture holds two things that each wrote `status` once, recorded unmerged
/// by a build whose writes carry no stamp. The current binary boots over the
/// store, which runs the migration that adds the stamp, and merges them through
/// the served verb. Both writes own ordinal 1 of the key, so the fold lands only
/// if the merge renumbers them, and with no stamp on either the survivor's write
/// is placed last: it is the one the survivor holds, and the duplicate's comes
/// first in the key's history. Each recording is checked to hold a `field_write`
/// table that has no stamp column, so the case cannot pass over stamped rows.
#[tokio::test]
async fn a_pair_written_before_the_stamp_merges_after_the_upgrade() {
    for (dir, label) in [
        (FIXTURE_DIR, "merge-first"),
        (DEPLOY_3_FIXTURE_DIR, "merge-deploy3"),
    ] {
        let Restored {
            scratch,
            state_dir,
            store_port,
            git_ref,
            ..
        } = restore_a_fixture(dir, label).await;
        let recorded = std::fs::read_to_string(format!("{dir}/doltdump.sql"))
            .unwrap_or_else(|e| panic!("reading the recording in {dir}: {e}"));
        let field_write_table = recorded
            .split("CREATE TABLE `field_write`")
            .nth(1)
            .and_then(|rest| rest.split(");").next())
            .unwrap_or_else(|| panic!("the recording at {git_ref} holds no field_write table"));
        assert!(
            !field_write_table.contains("written_at"),
            "the recording at {git_ref} already stamps its field writes, so it proves nothing \
             about rows that cannot be stamped: {field_write_table}"
        );
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
            .await
            .expect("connecting to the current binary");
        let fail =
            |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };

        let boot = surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await;
        let boot: serde_json::Value =
            serde_json::from_str(&boot).unwrap_or_else(|_| fail("booting to merge", &boot));
        let sid = boot["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| fail("booting to merge", &boot.to_string()))
            .to_string();

        let merged = surface
            .call(
                "merge_entities",
                json!({"duplicate": "thing:blue-kite", "survivor": "thing:red-kite",
                       "reason": "recorded as one kite", "sid": sid}),
            )
            .await;
        let merged_answer: serde_json::Value = serde_json::from_str(&merged)
            .unwrap_or_else(|_| fail("the merge answered something that is not json", &merged));
        if merged_answer["merged"] != "thing:blue-kite"
            || merged_answer["now_resolves_to"] != "thing:red-kite"
        {
            fail("the merge of the pair did not land", &merged);
        }

        let read = surface
            .call(
                "recall",
                json!({"subject": "thing:red-kite", "history": "status"}),
            )
            .await;
        let answered: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail("reading the survivor", &read));
        if answered["objects"][0]["fields"]["status"] != "now" {
            fail("the survivor does not hold its own write of status", &read);
        }
        let each: Vec<&str> = answered["objects"][0]["history"]["writes"]
            .as_array()
            .unwrap_or_else(|| fail("the writes behind status", &read))
            .iter()
            .map(|write| write["value"].as_str().unwrap_or(""))
            .collect();
        if each != ["done", "now"] {
            fail(
                "the duplicate's write is not placed before the survivor's",
                &read,
            );
        }

        surface.finish().await;
        booted.stop().await;
        drop(scratch);
    }
}

/// **After the upgrade, `field_link` holds the links the writes imply, and
/// nothing else.**
///
/// The expected set comes from the served surface and not from the start pass
/// that fills the table: every record's fields are read as the surface serves
/// them, and a value that is one handle, or a list in which every item is a
/// handle, is a link from the thing to the thing it names. The table is read
/// from the store directly, with the binary stopped, and each end is turned
/// back into a handle through the entity table. The two sets must be equal and
/// must not be empty. A pass that leaves a link out, or leaves one behind that
/// no write implies, fails here, and so does a gate whose expected set came out
/// of the same code as the table.
#[tokio::test]
async fn field_link_holds_the_links_the_writes_imply_and_nothing_else() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
    } = restore_the_fixture("links").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let mut expected = links_the_served_fields_imply(&surface, &git_ref).await;
    surface.finish().await;
    booted.stop().await;
    // **The handle two things have worn is served as a field and links nothing.**
    // The boot could not say which of the two it meant, so it left the value as
    // text. The served fields imply the link, the table must not hold it, and
    // taking it out of the expected set makes the comparison below fail if it does.
    let ambiguous = (
        "thing:upgrade-fixture-thing".to_string(),
        "upgrade_fixture_points_at".to_string(),
        "thing:upgrade-fixture-namesake".to_string(),
    );
    assert!(
        expected.remove(&ambiguous),
        "the recording's ambiguous handle is not served as a field: {expected:?}"
    );

    let reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let held = links_field_link_holds(reading.pool()).await;
    let mut reading = reading;
    reading.stop().await;
    drop(scratch);

    assert!(
        !expected.is_empty(),
        "the recording at {git_ref} implies no link at all, so this case proves nothing"
    );
    let missing: Vec<_> = expected.difference(&held).collect();
    let extra: Vec<_> = held.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "field_link disagrees with the writes recorded at {git_ref}. Implied by the served \
         fields and absent from the table: {missing:?}. In the table and implied by no \
         write: {extra:?}"
    );
}

/// **A role field an older build left on a bot that does not own the role does
/// not stop the owner.** The recording holds a role in the old shape on the
/// assistant. A second bot is named its owner through `claims_role`, and its
/// claim at the door is taken: the old field sits on another bot and is never
/// read as its lease. The assistant, now a non-owner, is refused with the owner
/// named, whatever its old field says. The owner then archives the role
/// object its claim made, and a stranger's archive of it is refused.
#[tokio::test]
async fn an_old_role_field_on_a_bot_that_is_not_the_owner_does_not_stop_the_owner() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("stray-role").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let parse = |what: &str, body: &str| -> serde_json::Value {
        serde_json::from_str(body).unwrap_or_else(|_| fail(what, body))
    };

    // The assistant's boot is the identity that writes.
    let boot = parse(
        "booting to write",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    let made = surface
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "alpha", "name": "Alpha",
                   "source": "user-named", "sid": sid}),
        )
        .await;
    if made.contains("\"status\":\"blocked\"") {
        fail("making the owner", &made);
    }
    let named = surface
        .call(
            "capture",
            json!({"subject": "bot:alpha", "content": "alpha runs the role the recording held",
                   "provenance": "testimony", "sid": sid,
                   "fields": {"claims_role": "upgrade-fixture-holder"}}),
        )
        .await;
    if named.contains("\"status\":\"blocked\"") {
        fail("naming the owner", &named);
    }

    let owner = parse(
        "the owner's claim",
        &surface
            .call(
                "start_here",
                json!({"bot": "alpha", "brief": true, "claim": "upgrade-fixture-holder"}),
            )
            .await,
    );
    if owner["session"]["claim"]["status"] != "taken" {
        fail(
            "the owner's claim being taken over an old field on another bot",
            &owner.to_string(),
        );
    }

    let stray = parse(
        "the old holder's claim",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new",
                       "claim": "upgrade-fixture-holder"}),
            )
            .await,
    );
    let claim = &stray["session"]["claim"];
    if claim["status"] != "refused" || claim["owner"] != "bot:alpha" {
        fail(
            "the old holder being refused with the owner named",
            &stray.to_string(),
        );
    }

    // **The role object the owner's claim made is the owner's to archive, and a
    // stranger's archive of it is refused naming the owner.** Read over the real
    // store, where the object's parent comes back as a handle.
    let stranger = parse(
        "the stranger's archive",
        &surface
            .call(
                "archive_entity",
                json!({"handle": "role:upgrade-fixture-holder", "reason": "i want it",
                       "sid": sid}),
            )
            .await,
    );
    if stranger["status"] != "blocked"
        || !stranger["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("bot:alpha"))
    {
        fail(
            "a stranger's archive of a role object being refused, naming its owner",
            &stranger.to_string(),
        );
    }
    let owner_sid = owner["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("the owner's handle", &owner.to_string()))
        .to_string();
    let archived = parse(
        "the owner's archive",
        &surface
            .call(
                "archive_entity",
                json!({"handle": "role:upgrade-fixture-holder", "reason": "made by mistake",
                       "sid": owner_sid}),
            )
            .await,
    );
    if archived["archived"]["reason"] != "made by mistake" {
        fail(
            "the owner's archive of its role object",
            &archived.to_string(),
        );
    }
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// **Testimony an older build recorded has no session of its own, and this
/// build refuses to rewrite its words or its value fields in place.** The
/// refusal names the route, the route works, and an edit of a key that only
/// describes the record still lands. The claim is one the recording wrote as
/// testimony, so the case is about stored rows and not about a claim this build
/// wrote a moment ago.
#[tokio::test]
async fn testimony_an_older_build_recorded_is_corrected_by_the_route_and_not_in_place() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("legacy-testimony").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    let boot = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await;
    let boot: serde_json::Value =
        serde_json::from_str(&boot).unwrap_or_else(|_| fail("booting to write", &boot));
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();

    const WORDS: &str = "lives at the recorded place";
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded claims", &read));
    let recorded = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|f| f["content"] == WORDS))
        .unwrap_or_else(|| fail("the recorded testimony", &read));
    if recorded["provenance"] != "testimony" {
        fail("the recorded claim being testimony", &read);
    }
    let address = recorded["address"]
        .as_str()
        .unwrap_or_else(|| fail("the recorded claim's address", &read))
        .to_string();

    // (1) A rewrite of the words in place is refused, names the route, and
    // leaves the words as they were.
    let refused = surface
        .call(
            "update_fact",
            json!({"address": address, "content": "lives somewhere else",
                   "provenance": "testimony", "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"")
        || !refused.contains("derived_from")
        || !refused.contains("archived")
    {
        fail("a refusal that names the route", &refused);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains(WORDS) || read.contains("lives somewhere else") {
        fail("the refused rewrite leaving the words alone", &read);
    }

    // (3) **A value field on it is the operator's, as its words are.** An edit
    // that sets one, with the words left alone, is refused with the same route
    // and changes nothing. A key that only describes the record is anybody's, and
    // lands.
    let refused = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"colour": "red"}, "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"")
        || !refused.contains("derived_from")
        || !refused.contains("archived")
    {
        fail("a refusal of a value field that names the route", &refused);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person"}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the person's fields", &read));
    if !parsed["objects"][0]["fields"]["colour"].is_null() {
        fail("the refused field staying off the person", &read);
    }
    let landed = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"purpose": "a note on where"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("an edit of a key that only describes the record", &landed);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains("a note on where") {
        fail("the describing key the edit set", &read);
    }

    // (2) Archive the claim, then write the corrected one derived from it.
    let archived = surface
        .call(
            "update_fact",
            json!({"address": address, "status": "archived",
                   "details": "the operator corrected it", "sid": sid}),
        )
        .await;
    if archived.contains("\"status\":\"blocked\"") {
        fail("archiving the recorded claim", &archived);
    }
    let corrected = surface
        .call(
            "capture",
            json!({"subject": "person:upgrade-fixture-person",
                   "content": "lives somewhere else", "provenance": "testimony",
                   "derived_from": address, "sid": sid}),
        )
        .await;
    if corrected.contains("\"status\":\"blocked\"") {
        fail(
            "the corrected claim derived from the archived one",
            &corrected,
        );
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "history_record": address}),
        )
        .await;
    if !read.contains(WORDS) || !read.contains("archived") {
        fail("the archived original staying readable", &read);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains("lives somewhere else") {
        fail("the corrected claim being served", &read);
    }

    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// A link as the surface names it: the thing, the key, the thing it points at.
type Link = (String, String, String);

/// **Every link the served fields imply.** Each kind in the snapshot is browsed
/// with its records, and a field value that is one handle of a thing in the
/// store, or a comma-separated list of them, is a link.
async fn links_the_served_fields_imply(
    surface: &Surface,
    git_ref: &str,
) -> std::collections::BTreeSet<Link> {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let snapshot = surface.call("start_here", json!({})).await;
    let parsed: serde_json::Value =
        serde_json::from_str(&snapshot).unwrap_or_else(|_| fail("the snapshot", &snapshot));
    let kinds: Vec<String> = parsed["snapshot"]["entities"]["by_kind"]
        .as_object()
        .unwrap_or_else(|| fail("the snapshot's kinds", &snapshot))
        .keys()
        .cloned()
        .collect();
    assert!(!kinds.is_empty(), "the snapshot lists no kind: {snapshot}");

    let mut things = Vec::new();
    for kind in &kinds {
        let read = surface
            .call("recall", json!({"kind": kind, "facts": true}))
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&read)
            .unwrap_or_else(|_| fail(&format!("the {kind} records"), &read));
        for object in parsed["objects"].as_array().into_iter().flatten() {
            things.push(object.clone());
        }
    }
    let known: std::collections::BTreeSet<String> = things
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect();

    let mut links = std::collections::BTreeSet::new();
    for object in &things {
        let Some(subject) = object["id"].as_str() else {
            continue;
        };
        for fact in object["facts"].as_array().into_iter().flatten() {
            for (key, value) in fact["fields"].as_object().into_iter().flatten() {
                let Some(value) = value.as_str() else {
                    continue;
                };
                let items: Vec<&str> = value.split(',').map(str::trim).collect();
                if !items.iter().all(|item| known.contains(*item)) {
                    continue;
                }
                for item in items {
                    links.insert((subject.to_string(), key.clone(), item.to_string()));
                }
            }
        }
    }
    links
}

/// **Every link the table holds**, with both ends turned back into handles
/// through the entity table, and the write's ordinal dropped: the same link
/// written twice is one link.
async fn links_field_link_holds(pool: &sqlx::MySqlPool) -> std::collections::BTreeSet<Link> {
    let handles: std::collections::BTreeMap<String, String> =
        sqlx::query_as::<_, (String, Option<String>)>("SELECT id, badge FROM entity")
            .fetch_all(pool)
            .await
            .expect("the entity table reads")
            .into_iter()
            .map(|(id, badge)| {
                (
                    badge
                        .filter(|b| !b.is_empty())
                        .unwrap_or_else(|| id.clone()),
                    id,
                )
            })
            .collect();
    let named = |stored: String| handles.get(&stored).cloned().unwrap_or(stored);
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT entity, `key`, target FROM field_link ORDER BY 1, 2, 3",
    )
    .fetch_all(pool)
    .await
    .expect("field_link reads")
    .into_iter()
    .map(|(entity, key, target)| (named(entity), key, named(target)))
    .collect()
}
