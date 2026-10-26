//! Upgrade the one store the last deployed release wrote, supplied by CI.
//! No SQL dump is committed; a cache miss records the deployed tag again.
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_exercise::surface::Surface;
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
#[path = "upgrade_gate/deaths.rs"]
mod deaths;
#[path = "upgrade_gate/deploy_8_1.rs"]
mod deploy_8_1;
#[path = "upgrade_gate/deploy_9_1.rs"]
mod deploy_9_1;
#[path = "upgrade_gate/deploy_9_2_1.rs"]
mod deploy_9_2_1;
#[path = "upgrade_gate/runs_in_moment_order.rs"]
mod runs_in_moment_order;
mod support;

fn fixture_dir() -> String {
    let path = std::env::var("UPGRADE_FIXTURE_DIR")
        .expect("CI must supply UPGRADE_FIXTURE_DIR from the last deployed tag");
    let path = std::path::PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    };
    let tag = std::fs::read_to_string(path.join("tag.txt")).expect("the deployed tag is recorded");
    eprintln!("upgrade gate source: {} at {}", tag.trim(), path.display());
    path.to_str().expect("the fixture path is UTF-8").into()
}

#[tokio::test]
async fn the_current_binary_boots_on_the_last_deployed_store() {
    boots_on_a_store_filled_by(&fixture_dir(), "last-deployed").await;
}
#[tokio::test]
async fn the_last_deployed_store_recorded_six_hours_earlier_reads_back() {
    boots_on_a_store_recorded_earlier(&fixture_dir(), "last-deployed-aged-hours", 6).await;
}
#[tokio::test]
async fn the_last_deployed_store_recorded_three_days_earlier_reads_back() {
    boots_on_a_store_recorded_earlier(&fixture_dir(), "last-deployed-aged-days", 72).await;
}

type Link = (String, String, i64, String);
const WARNING_THE_GATE_CAUSES: &str = "AUTH DISABLED";

const FOLD_HOLDER: &str = "person:upgrade-fixture-person";

const SUPPLIED_CHILD: &str = "thing:kettle";

struct Restored {
    /// Removes the directory when the case is done.
    scratch: Scratch,
    state_dir: std::path::PathBuf,
    store_port: u16,
    git_ref: String,
    /// What every thing held in the old store, as `(handle, key, value)`, read
    /// before the current binary touched it.
    held_before: Vec<(String, String, String)>,
    /// Every entity handle the old store holds, archived and folded ones
    /// included, read before the current binary touched it.
    handles: Vec<String>,
    /// **The key of the recorded holder run, when the recording build kept one.**
    /// A build that keys its runs stores only the hash of a key, so the gate sets
    /// that hash to the hash of a key of its own and holds the key here; a build
    /// that began runs without keys leaves the holder to be exchanged by its name.
    holder_key: Option<String>,
}

struct Booted {
    child: tokio::process::Child,
    /// The directory the store under this binary keeps its files in.
    state_dir: std::path::PathBuf,
    http_port: u16,
    /// Every line the binary logged up to its serving line.
    log: Vec<String>,
}

impl Booted {
    /// Stop it, and wait until the store's own lock is released: the dump that
    /// follows needs the directory to itself.
    async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        wait_for_the_store_lock_to_release(&self.state_dir).await;
    }
}

async fn restore_the_fixture(label: &str) -> Restored {
    restore_a_fixture(&fixture_dir(), label).await
}

/// **A fixture directory, restored into a store of this case's own.**
async fn restore_a_fixture(fixture_dir: &str, label: &str) -> Restored {
    restore_a_fixture_recorded_earlier(fixture_dir, label, 0).await
}

/// **Every moment a store holds, moved back by `hours`**, as if the recording had
/// been made that much earlier. The gate runs on the day it runs, so the age of
/// a recording is whatever the calendar makes it; this makes the age a thing a
/// case chooses. It reads every column of every table, so a moment a later build
/// adds is aged without anybody remembering to list it. The epoch is the store's
/// own "no moment" and stays where it is.
async fn age_the_recording(pool: &sqlx::MySqlPool, hours: i64) {
    if hours == 0 {
        return;
    }
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT CAST(table_name AS CHAR), CAST(column_name AS CHAR)
         FROM information_schema.columns
         WHERE table_schema = DATABASE() AND data_type IN ('varchar', 'text', 'char', 'longtext')",
    )
    .fetch_all(pool)
    .await
    .expect("the store's columns read");
    for (table, column) in columns {
        let held: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT DISTINCT `{column}` FROM `{table}` WHERE `{column}` LIKE '____-__-__T%Z'"
        )))
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("reading the moments in {table}.{column}: {e}"));
        let mut moments: Vec<(jiff::Timestamp, String)> = held
            .into_iter()
            .filter_map(|text| text.parse().ok().map(|moment| (moment, text)))
            .filter(|(moment, _)| *moment > jiff::Timestamp::UNIX_EPOCH)
            .collect();
        // The moments furthest from where they are going first, so a moved moment
        // never lands on one still to move.
        moments.sort();
        if hours < 0 {
            moments.reverse();
        }
        for (moment, text) in moments {
            let moved = moment
                .checked_sub(jiff::SignedDuration::from_hours(hours))
                .expect("an aged moment is in range")
                .to_string();
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE `{table}` SET `{column}` = ? WHERE `{column}` = ?"
            )))
            .bind(moved)
            .bind(text)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("aging a moment in {table}.{column}: {e}"));
        }
    }
}

/// **A fixture directory, restored into a store of this case's own, and aged**
/// by `earlier_hours` before the gate moves the moments it pins.
async fn restore_a_fixture_recorded_earlier(
    fixture_dir: &str,
    label: &str,
    earlier_hours: i64,
) -> Restored {
    let fixture_ref = format!("{fixture_dir}/ref.txt");
    let fixture_dump = format!("{fixture_dir}/doltdump.sql");
    let git_ref = std::fs::read_to_string(&fixture_ref)
        .unwrap_or_else(|e| panic!("reading {fixture_ref}: {e}"))
        .trim()
        .to_string();

    let state_dir =
        jojobot_adapters::testing::scratch_path(&format!("jojobot-upgrade-gate-{label}"));
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
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(restoring.pool())
            .await
            .unwrap_or_else(|e| {
                panic!("replaying the fixture recorded at {git_ref} failed on:\n{statement}\n{e}")
            });
    }
    // **The recording is aged before the gate pins anything.** The moments a case
    // compares against the clock are the three the edits below set, each to a
    // moment of its own choosing, so the age the recording was given changes
    // none of them; every other moment in a fixture is only ever read back, and
    // the aged cases after the boot cases are the standing proof.
    let first_migration = |pool: sqlx::MySqlPool| async move {
        let applied: String = sqlx::query_scalar("SELECT MIN(applied_at) FROM schema_migration")
            .fetch_one(&pool)
            .await
            .expect("the first migration's moment reads");
        applied
            .parse::<jiff::Timestamp>()
            .unwrap_or_else(|e| panic!("a recorded moment {applied:?} does not parse: {e}"))
    };
    let recorded_at = first_migration(restoring.pool().clone()).await;
    age_the_recording(restoring.pool(), earlier_hours).await;
    // **Stated, so an ageing that moved nothing cannot pass for one that did.**
    let aged_to = first_migration(restoring.pool().clone()).await;
    assert_eq!(
        recorded_at.duration_since(aged_to),
        jiff::SignedDuration::from_hours(earlier_hours),
        "the recording made at {git_ref} was not aged by {earlier_hours} hours"
    );
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
    // **The holder's run is moved to ten minutes ago too, so the sweep that runs
    // when the binary boots does not find it silent for a day.** A session with
    // no entry has nothing to measure but its own start, and the recording's
    // holder wrote none. The recording is a day old once the date moves on, and
    // the sweep then marks the run abandoned and refuses its write: the cases
    // below passed on the day they were recorded and failed on the next. Like the
    // claim, it is one edit to a moment and changes no shape.
    let holder_sid = std::fs::read_to_string(format!("{fixture_dir}/role_holder.txt"))
        .unwrap_or_else(|e| panic!("reading {fixture_dir}/role_holder.txt: {e}"))
        .trim()
        .to_string();
    sqlx::query("UPDATE session SET started_at = ? WHERE sid = ?")
        .bind((jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string())
        .bind(&holder_sid)
        .execute(restoring.pool())
        .await
        .expect("the holder run's moment is moved to now");
    // **A run a keyed build began keeps the hash of its key and nothing else**, so
    // the gate cannot act as it. The hash is set to the hash of a key the gate
    // minted, which changes the secret and no shape.
    let keyed: Option<String> = if has_column(restoring.pool(), "session", "key_hash").await {
        sqlx::query_scalar("SELECT key_hash FROM session WHERE sid = ?")
            .bind(&holder_sid)
            .fetch_optional(restoring.pool())
            .await
            .expect("the holder run's key reads")
            .flatten()
    } else {
        None
    };
    let holder_key = if keyed.is_some() {
        let key = jojobot_domain::session::mint_key();
        sqlx::query("UPDATE session SET key_hash = ? WHERE sid = ?")
            .bind(jojobot_domain::session::hash_key(&key))
            .bind(&holder_sid)
            .execute(restoring.pool())
            .await
            .expect("the holder run's key hash is set to a key the gate holds");
        Some(key)
    } else {
        None
    };
    // **The holder's entries are moved with its start, all by the same amount.**
    // The sweep measures a run by its newest entry, and a recording whose holder
    // wrote entries (the closing and the beats of a later build) stamps them at
    // the hour it was recorded, so moving only the start left the run reading as
    // gone quiet an hour after the recording and for ever after. The newest entry
    // lands ten minutes ago and each of the others keeps its distance from it, so
    // the chronology keeps its order and its gaps. A holder with no entry moves
    // nothing here.
    let entries: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT j.id, j.at, j.touched FROM journal_entry j
         JOIN session s ON s.id = j.session WHERE s.sid = ?",
    )
    .bind(&holder_sid)
    .fetch_all(restoring.pool())
    .await
    .expect("the holder run's entries read");
    let moment = |text: &str| -> jiff::Timestamp {
        text.parse()
            .unwrap_or_else(|e| panic!("a recorded moment {text:?} does not parse: {e}"))
    };
    let newest = entries
        .iter()
        .flat_map(|(_, at, touched)| [Some(at.as_str()), touched.as_deref()])
        .flatten()
        .map(moment)
        .max();
    if let Some(newest) = newest {
        let shift =
            (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).duration_since(newest);
        let moved = |text: &str| {
            moment(text)
                .checked_add(shift)
                .expect("a moved moment is in range")
                .to_string()
        };
        for (id, at, touched) in &entries {
            sqlx::query(
                "UPDATE journal_entry SET at = ?, touched = ?
                 WHERE id = ? AND session = (SELECT id FROM session WHERE sid = ?)",
            )
            .bind(moved(at))
            .bind(touched.as_deref().map(moved))
            .bind(id)
            .bind(&holder_sid)
            .execute(restoring.pool())
            .await
            .expect("the holder run's entry moment is moved");
        }
    }
    // **Stated here so the fixture's age cannot hide.** The sweep measures a run
    // by its newest entry, or its start when it has none; the holder must read as
    // recent whatever day the gate runs on.
    let last_beat: String = sqlx::query_scalar(
        "SELECT COALESCE(MAX(GREATEST(j.at, COALESCE(j.touched, j.at))), s.started_at)
         FROM session s LEFT JOIN journal_entry j ON j.session = s.id
         WHERE s.sid = ? GROUP BY s.id, s.started_at",
    )
    .bind(&holder_sid)
    .fetch_one(restoring.pool())
    .await
    .expect("the holder run's last moment reads");
    let last_beat: jiff::Timestamp = last_beat.parse().expect("a moment");
    assert!(
        jiff::Timestamp::now().duration_since(last_beat) < jiff::SignedDuration::from_hours(1),
        "the holder run in the fixture recorded at {git_ref} was last active at {last_beat}, \
         which the sweep reads as a run gone quiet"
    );
    // **What every thing holds, read off the old store with plain SQL before the
    // current binary touches it.** Independent of any fold in this build, so a
    // migration or a filter that changed what a thing holds cannot move this side
    // with it.
    let held_before = fields_every_thing_holds(restoring.pool()).await;
    assert_every_held_row_joined(restoring.pool(), &git_ref).await;
    assert!(
        held_before.len() >= 5,
        "the fixture holds things with fields, or this compares nothing: {held_before:?}"
    );
    let handles: Vec<String> = sqlx::query_scalar("SELECT id FROM entity ORDER BY id")
        .fetch_all(restoring.pool())
        .await
        .expect("the old store's handles read");
    restoring.stop().await;
    Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
        handles,
        holder_key,
    }
}

async fn boots_on_a_store_filled_by(fixture_dir: &str, label: &str) {
    boots_on_a_store_recorded_earlier(fixture_dir, label, 0).await;
}

async fn boots_on_a_store_recorded_earlier(fixture_dir: &str, label: &str, earlier_hours: i64) {
    let restored = restore_a_fixture_recorded_earlier(fixture_dir, label, earlier_hours).await;
    boots_on_a_restored_store(fixture_dir, restored).await;
}

/// **Every boot-and-read-back assertion the gate makes over a restored store**,
/// for a store the caller has already restored, so a case that prepares the
/// store differently (an interrupted migration) is held to the same bar as a
/// clean boot and not to a copy of it.
async fn boots_on_a_restored_store(fixture_dir: &str, restored: Restored) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
        handles,
        holder_key,
    } = restored;

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
    // The first boot reported the recording's handle; the passes are marked, so the
    // next boots report nothing.
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
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
    let third = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    // **The boot measured the stored records and found none over the ceiling**, and
    // said so: a boot that logged nothing here would be a pass that did not run.
    assert!(
        third
            .log
            .iter()
            .any(|line| line.contains("stored records: none is over the ceiling")),
        "the boot of a store recorded at {git_ref} did not say it measured the stored records: {:?}",
        third.log
    );
    // **The boot checked the store for the damage a cut-off write leaves, found
    // none, and said so**: a boot that logged nothing here would be a check that
    // did not run. A store older than merge events holds merges with no event on
    // purpose, and the check counts those without calling them damage. The second boot's equal dump above is the
    // proof that the check wrote nothing.
    assert!(
        third.log.iter().any(|line| line.contains("storage check:")),
        "the boot of a store recorded at {git_ref} did not report the storage check: {:?}",
        third.log
    );
    assert!(
        !third
            .log
            .iter()
            .any(|line| line.contains("STORAGE CHECK NOT RUN") || line.contains("STORAGE DAMAGE")),
        "the storage check found damage or did not run on the store recorded at {git_ref}: {:?}",
        third.log
    );
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
    assert_no_stored_record_is_over_the_ceiling(&surface, &git_ref, &handles).await;
    // **The recorded holder began before keys existed**, so the name it was
    // known by is exchanged once for a key, and every call below carries the key.
    // A recording from a build that keys its runs holds a key already, so the
    // holder is not exchanged and has no role claim in the old shape to carry.
    let keyed = holder_key.is_some();
    let role_key = match holder_key {
        Some(key) => key,
        None => exchange_the_recorded_run(&surface, &git_ref, &role_holder).await,
    };
    assert_every_recorded_record_reads_back(&surface, &git_ref, &role_holder).await;
    assert_what_a_real_store_holds_reads_back(&surface, &git_ref, &role_key).await;
    if !keyed {
        assert_a_role_claimed_in_the_old_shape_is_carried_by_the_boot_and_stays_held(
            &surface,
            &git_ref,
            &role_holder,
            &role_key,
        )
        .await;
    }
    assert_the_voucher_kind_arrives_and_no_voucher_exists(&surface, &git_ref).await;
    surface.finish().await;
    third.stop().await;

    // The role is now claim-shaped, with the claim the holder's write made. A
    // fourth boot is the restart of the deployed server over it.
    if !keyed {
        assert_a_claim_shaped_lease_renews_in_place(
            &state_dir,
            store_port,
            &git_ref,
            &role_holder,
            &role_key,
        )
        .await;
    }
    drop(scratch);
}

/// **A run an older build began holds no key, and its first call with its old
/// name is exchanged once for one.** The exchange answer is a refusal that
/// carries the key and runs nothing; the same name sent again is turned away
/// and hands out nothing; and the key opens the run. Returns the key.
async fn exchange_the_recorded_run(surface: &Surface, git_ref: &str, role_holder: &str) -> String {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let first = surface
        .call(
            "journal",
            json!({"entry": "the first call with the old name", "sid": role_holder}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&first).unwrap_or_else(|_| fail("the exchange answer", &first));
    let key = match (parsed["exchanged"] == true, parsed["sid"].as_str()) {
        (true, Some(key)) if key != role_holder => key.to_string(),
        _ => fail("the old name was not exchanged for a key", &first),
    };
    let again = surface
        .call(
            "journal",
            json!({"entry": "the old name again", "sid": role_holder}),
        )
        .await;
    if again.contains("\"exchanged\":true") || !again.contains("\"status\":\"blocked\"") {
        fail("a second bare name was not turned away", &again);
    }
    let opened = surface
        .call("list_runs", json!({"sid": key, "limit": 5}))
        .await;
    if opened.contains("\"status\":\"blocked\"") || !opened.contains(role_holder) {
        fail("the key did not open the recorded run", &opened);
    }
    key
}

/// **The store of a restored fixture, brought up with no binary over it**, so
/// the gate can write rows the older build would have written, and read what the
/// current binary did to them.
async fn open_the_store(state_dir: &std::path::Path, store_port: u16) -> Dolt {
    Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the restored store comes up with no binary over it")
}

/// **Every stored record that no answer could carry whole**, found by reading
/// each thing's records back through the served surface the way a caller does,
/// one part after another. A record is over when the answer that carries it is
/// past the ceiling: a read serves a record whole or not at all, so the answer
/// is what shows it. Each is `(handle, offset of the part, characters served)`.
async fn records_over_the_ceiling(
    surface: &Surface,
    handles: &[String],
) -> Vec<(String, usize, usize)> {
    let mut over = Vec::new();
    for handle in handles {
        let mut offset = 0usize;
        loop {
            let raw = surface
                .call(
                    "recall",
                    json!({"subject": handle, "facts": true, "details": true,
                           "review_marks": true, "stood_for": true, "offset": offset}),
                )
                .await;
            let size = raw.chars().count();
            if size > jojobot_domain::text::ANSWER_CEILING {
                over.push((handle.clone(), offset, size));
            }
            let next = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|body| body["not_shown"]["offset"].as_u64());
            match next {
                Some(next) if next as usize > offset => offset = next as usize,
                _ => break,
            }
        }
    }
    over
}

/// **A store an older build filled holds no record over the ceiling.** The write
/// guard keeps new ones out; this finds the ones already held. A non-empty list
/// is not repaired here and not ignored: it goes to the PM before anything else,
/// who decides between a pointer to a page and a bounded continuation for those
/// records only.
async fn assert_no_stored_record_is_over_the_ceiling(
    surface: &Surface,
    git_ref: &str,
    handles: &[String],
) {
    assert!(
        !handles.is_empty(),
        "the recording at {git_ref} holds no entity, so this scan reads nothing"
    );
    let over = records_over_the_ceiling(surface, handles).await;
    assert!(
        over.is_empty(),
        "the store recorded at {git_ref} holds records no answer can carry whole: {over:?}. \
         Send this list to the PM before anything else; do not shorten them here"
    );
}

/// **The scan finds a record stored over the ceiling, and names only that one.**
/// One stored record is made larger than any answer straight in the store, which
/// is the only way left to make one; the scan over the served surface lists it
/// and nothing else. Without this the case above passes on a scan that reads
/// nothing.
#[tokio::test]
async fn the_scan_for_oversized_records_finds_one_stored_over_the_ceiling() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles,
        holder_key: _,
    } = restore_the_fixture("oversized").await;
    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be made oversized");
    let (badge, local, handle): (String, String, String) = sqlx::query_as(
        "SELECT f.entity, f.id, e.id FROM fact f JOIN entity e ON e.badge = f.entity
         WHERE f.status = 'active'
           AND NOT EXISTS (SELECT 1 FROM field_write w
                           WHERE w.entity = f.entity AND w.fact_id = f.id
                             AND w.`key` LIKE 'role/%')
         ORDER BY e.id, f.id LIMIT 1",
    )
    .fetch_one(editing.pool())
    .await
    .expect("the recording holds an active record");
    // **Both tables the record lives in**: reads project the newest write of a
    // claim, and the claim's own row sits beside it.
    let huge = "x".repeat(jojobot_domain::text::ANSWER_CEILING + 2_000);
    for update in [
        "UPDATE fact SET details = ? WHERE entity = ? AND id = ?",
        "UPDATE fact_write SET details = ? WHERE entity = ? AND fact_id = ?",
    ] {
        sqlx::query(update)
            .bind(&huge)
            .bind(&badge)
            .bind(&local)
            .execute(editing.pool())
            .await
            .expect("one stored record is made larger than any answer");
    }
    editing.stop().await;

    let booted = boot_current_allowing(&state_dir, store_port, &git_ref, true).await;
    // **The boot names the record**: its address, which carries the thing's handle,
    // once, as a warning.
    let named: Vec<&String> = booted
        .log
        .iter()
        .filter(|line| line.contains("STORED RECORD OVER THE CEILING"))
        .collect();
    assert_eq!(named.len(), 1, "one record, one warning: {:?}", booted.log);
    assert!(
        named[0].contains(&format!("{handle}#{local}")),
        "the warning names the record's address {handle}#{local}: {}",
        named[0]
    );
    assert!(
        !booted
            .log
            .iter()
            .any(|line| line.contains("none is over the ceiling")),
        "a store that holds one does not say it holds none: {:?}",
        booted.log
    );
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let over = records_over_the_ceiling(&surface, &handles).await;
    surface.finish().await;
    booted.stop().await;
    drop(scratch);

    assert_eq!(
        over.iter().map(|(h, ..)| h.as_str()).collect::<Vec<_>>(),
        [handle.as_str()],
        "the scan lists the one oversized record's thing and no other: {over:?}"
    );
    assert!(over[0].2 > jojobot_domain::text::ANSWER_CEILING, "{over:?}");
}

/// **The warning a boot logs for the handle two things have worn**, naming the
/// field it sits under. The recording writes that handle on purpose.
fn is_the_recorded_ambiguous_handle(line: &str) -> bool {
    is_left_as_text_under(line, "upgrade_fixture_points_at")
}

/// **Whether a boot line is the left-as-text warning for a value under `key`.**
fn is_left_as_text_under(line: &str, key: &str) -> bool {
    line.contains("FIELD VALUE LEFT AS TEXT") && line.contains(key)
}

/// **Whether the recording made at `git_ref` holds the handle two things have
/// worn as plain text** under the field the gate looks for. Read off the dump
/// itself, so the answer is the recording's and not a list kept beside it: a
/// field value is `'<key>',<ordinal>,'<value>'` in the dump, and a plain handle
/// is a value that starts with a kind, where a stored id starts with the mark.
fn recording_holds_the_plain_ambiguous_handle(git_ref: &str) -> bool {
    let dir = fixture_dir();
    let recorded = std::fs::read_to_string(format!("{dir}/ref.txt")).expect("the recorded ref");
    assert_eq!(
        recorded.trim(),
        git_ref,
        "the cache must name the same deployed build"
    );
    let dump = std::fs::read_to_string(format!("{dir}/doltdump.sql")).expect("the recorded dump");
    dump.split("'upgrade_fixture_points_at',")
        .skip(1)
        .any(|rest| {
            rest.split(',')
                .nth(1)
                .is_some_and(|value| value.starts_with("'thing:"))
        })
}

/// **The alerts sqlx logs when the machine is busy**, each by its level, its
/// target and its message together. On a busy machine a statement of a healthy
/// boot takes longer than a second, and a connection takes longer than two to
/// acquire, so these say the machine is loaded and nothing about the boot. A real
/// boot that waits over two seconds for a connection no longer fails the gate.
///
/// **Narrower than raising the thresholds for the booted binary.** A raised
/// threshold hides every slow statement and connection, whichever target logs
/// it, and needs a setting in the binary's own code for the gate to turn. This
/// reads the two lines the library writes and nothing else, so an alert from
/// another target or with another message, and every other warning, still fails
/// the gate.
fn is_a_busy_host_alert(line: &str) -> bool {
    const ALERTS: [(&str, &str); 2] = [
        (
            "sqlx::query",
            "slow statement: execution time exceeded alert threshold",
        ),
        (
            "sqlx::pool::acquire",
            "acquired connection, but time to acquire exceeded slow threshold",
        ),
    ];
    line.contains("WARN")
        && ALERTS
            .iter()
            .any(|(target, message)| line.contains(&format!("{target}: {message}")))
}

/// **The lines of a boot's log that say part of the boot failed or degraded.**
/// Every warning and every error counts, except the ones the gate itself causes
/// or expects, the ones the caller names in `tolerated_warnings` (each by a word
/// the line holds), and the alerts of a loaded machine.
fn degradations<'a>(
    seen: &'a [String],
    stored_over_the_ceiling: bool,
    tolerated_warnings: &[&str],
) -> Vec<&'a String> {
    seen.iter()
        .filter(|line| !is_a_busy_host_alert(line))
        .filter(|line| {
            let expected = line.contains(WARNING_THE_GATE_CAUSES)
                || is_the_recorded_ambiguous_handle(line)
                || deploy_9_1::is_recorded_r1_warning(line)
                || (stored_over_the_ceiling && line.contains("OVER THE CEILING"))
                || tolerated_warnings.iter().any(|named| line.contains(named));
            line.contains("ERROR") || (line.contains("WARN") && !expected)
        })
        .collect()
}

/// **The wait for the store's lock lasts as long as something holds the lock and
/// no longer.** A lock file held by an open handle keeps the wait going; the wait
/// ends once the handle is gone, and it ends at once where there is no lock file.
#[tokio::test]
async fn the_wait_for_the_store_lock_lasts_as_long_as_the_lock_is_held_and_no_longer() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-lock-wait");
    let noms = dir.join(".dolt").join("noms");
    std::fs::create_dir_all(&noms).expect("a directory for the lock");
    let held = std::fs::File::create(noms.join("LOCK")).expect("the lock file");
    held.lock().expect("the lock is taken");

    let waiting = tokio::spawn({
        let dir = dir.clone();
        async move { wait_for_the_store_lock_to_release(&dir).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!waiting.is_finished(), "the wait ended with the lock held");

    drop(held);
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("the wait ends once the lock is released")
        .expect("the wait did not fail");

    let empty = jojobot_adapters::testing::scratch_path("jojobot-lock-wait-empty");
    std::fs::create_dir_all(&empty).expect("an empty directory");
    tokio::time::timeout(
        Duration::from_millis(500),
        wait_for_the_store_lock_to_release(&empty),
    )
    .await
    .expect("with no lock file the wait ends at once");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&empty);
}

/// The files the store locks its data under `dir` with.
fn store_lock_files(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            store_lock_files(&path, found);
        } else if path.file_name().is_some_and(|name| name == "LOCK") {
            found.push(path);
        }
    }
}

/// **Wait until the store's own lock is released**, which is the moment the
/// store's process is gone: the directory is then the next step's to use. The
/// store holds an advisory lock on a `LOCK` file of its data directory for as long
/// as it runs, so a lock that cannot be taken is a store still running. The wait
/// ends at once when nothing holds one, and it is bounded: a store that still
/// holds its lock ten seconds after it was told to stop fails the case.
async fn wait_for_the_store_lock_to_release(dir: &std::path::Path) {
    let give_up = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let mut files = Vec::new();
        store_lock_files(dir, &mut files);
        let held = files
            .iter()
            .filter(|file| {
                std::fs::File::open(file).is_ok_and(|open| {
                    matches!(open.try_lock(), Err(std::fs::TryLockError::WouldBlock))
                })
            })
            .count();
        if held == 0 {
            return;
        }
        assert!(
            std::time::Instant::now() < give_up,
            "{held} lock file(s) under {dir:?} are still held ten seconds after the store was told to stop"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// **Boot the current binary on the store in `state_dir`** and hold it to the
/// boot's own account of itself: it reaches its serving line, and it says no
/// part of its own boot failed.
async fn boot_current(state_dir: &std::path::Path, store_port: u16, git_ref: &str) -> Booted {
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(git_ref));
    boot_current_expecting(state_dir, store_port, git_ref, Some(reports), &[]).await
}

/// **The same boot, for a store that holds a record over the ceiling on purpose.**
/// The boot names each one in a warning, which is its job, so that warning is the
/// one more the gate does not count as a degradation.
async fn boot_current_allowing(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    stored_over_the_ceiling: bool,
) -> Booted {
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(git_ref));
    boot_current_with(
        state_dir,
        store_port,
        git_ref,
        stored_over_the_ceiling,
        Some(reports),
        &[],
    )
    .await
}

/// **The same boot, told how many times it must report the recording's
/// ambiguous handle (`None` leaves the count to the caller), and which other
/// warnings it may log, each named by a word the line holds.** A first boot
/// reports the recording's handle when the recording holds it.
async fn boot_current_expecting(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    ambiguous_reports: Option<usize>,
    tolerated_warnings: &[&str],
) -> Booted {
    boot_current_with(
        state_dir,
        store_port,
        git_ref,
        false,
        ambiguous_reports,
        tolerated_warnings,
    )
    .await
}

/// The boot every other one is a case of.
async fn boot_current_with(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    stored_over_the_ceiling: bool,
    ambiguous_reports: Option<usize>,
    tolerated_warnings: &[&str],
) -> Booted {
    let (marked_before, expected_r1) =
        deploy_9_1::recorded_r1_before_boot(state_dir, store_port).await;
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let mut child = tokio::process::Command::new(&binary)
        .env("JOJOBOT_LINK_RULE", deploy_9_1::link_rule_word())
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
        .expect("the current binary runs");

    let serving = match jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    {
        Ok(serving) => serving,
        Err(failure) => panic!(
            "the current binary {failure} (a boot is given {:?}) on a store recorded at \
             {git_ref} — this is the upgrade class this gate exists to catch.",
            jojobot_exercise::room::READY_TIMEOUT
        ),
    };
    let http_port = serving.port();
    let mut seen = serving.seen;
    let mut lines = serving.stdout;
    // **The release notes are sent after the serving line**, in a task of the
    // binary's own, and write each bot's ledger. A boot is not finished for this
    // gate until that pass says it is: a case that stops the binary earlier
    // leaves the ledgers to the next boot, which then writes rows it did not
    // before. The lines it logs are held to the same account as the rest.
    let passed = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            let done = line.contains("release notes pass finished");
            seen.push(line);
            if done {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        passed,
        "the current binary never finished sending the release notes on a store recorded at \
         {git_ref}: {seen:?}"
    );
    // **Keep reading.** The binary logs to the pipe for as long as it runs, and
    // a pipe nobody reads fills, after which the binary blocks inside a write
    // and every call to it hangs. The helper reads stderr; stdout is read here.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    // **Every error and unexpected warning fails the gate.** Known R1 identities
    // are allowed only at the counts the restored store predicts; missing,
    // duplicate and repeated reports fail too. Other exceptions are explicit in
    // degradations, including the gate's auth warning and busy-host alerts.
    let warned = degradations(&seen, stored_over_the_ceiling, tolerated_warnings);
    assert!(
        warned.is_empty(),
        "the current binary booted on a store recorded at {git_ref} and said part of its own \
         boot failed: {warned:?}"
    );
    assert!(
        deploy_9_1::recorded_r1_counts_match(&seen, &expected_r1),
        "recorded at {git_ref}: expected R1 reports {expected_r1:?}: {seen:?}"
    );
    // An ambiguous plain handle is reported on the first marking boot only.
    // A settled marker prevents reinterpretation, even when spelling survives.
    let reported = seen
        .iter()
        .filter(|line| is_the_recorded_ambiguous_handle(line))
        .count();
    if let Some(ambiguous_reports) = ambiguous_reports {
        let ambiguous_reports = if marked_before { 0 } else { ambiguous_reports };
        assert_eq!(
            reported, ambiguous_reports,
            "the boot of a store recorded at {git_ref} must report the ambiguous handle \
             {ambiguous_reports} time(s): {seen:?}"
        );
    }
    Booted {
        child,
        state_dir: state_dir.to_path_buf(),
        http_port,
        log: seen,
    }
}

/// **Where the store's own tool keeps its configuration for a run.** The tool
/// keeps its global configuration under a home directory and fails where the
/// environment names none (a build sandbox). The run's own state directory holds
/// it, as the adapter does for every store it spawns. Every call of the tool in
/// this file names it.
fn dolt_home_in(state_dir: &std::path::Path) -> std::path::PathBuf {
    let home = state_dir.join("dolt-home");
    std::fs::create_dir_all(&home).expect("a directory for the store tool's configuration");
    home
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
    let dump = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", dolt_home_in(state_dir))
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
/// holder is read back, and the holder's next write renews:
/// the moment is new and the key still holds the writes it had.
async fn assert_a_claim_shaped_lease_renews_in_place(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    role_holder: &str,
    role_key: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let database_dir = state_dir.join("db").join("jojobot");
    let aged = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    let aging = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", dolt_home_in(state_dir))
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

    let fourth = boot_current_expecting(state_dir, store_port, git_ref, Some(0), &[]).await;
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
    let beat = surface
        .call(
            "journal",
            json!({"entry": "renewed after the restart", "sid": role_key}),
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
    // **The lease is still the holder's, shown by a rival**: another run of the
    // owning bot claims the role, finds the renewed lease held, and is told which
    // run it took the role from. A boot that carried the holder and lost the lease
    // would take it from nobody.
    let rival = surface
        .call(
            "start_here",
            json!({
                "bot": "assistant",
                "claim": "upgrade-fixture-holder",
                "resume": "new",
                "brief": true,
            }),
        )
        .await;
    let rival: serde_json::Value =
        serde_json::from_str(&rival).unwrap_or_else(|_| fail("the rival's boot", &rival));
    if rival["session"]["claim"]["status"] != "taken"
        || rival["session"]["claim"]["took_over_from"] != role_holder
    {
        fail(
            "a rival taking the renewed lease from the holder",
            &rival.to_string(),
        );
    }
    surface.finish().await;
    fourth.stop().await;
}

/// **Whether the old store's table has the column.** An older recording lacks the
/// columns a later build added, and a query that names one fails there instead of
/// answering for the shape the store has.
async fn has_column(pool: &sqlx::MySqlPool, table: &str, column: &str) -> bool {
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("the columns of the old store read");
    found > 0
}

/// **Every value a thing holds in the old store**, as `(handle, key, value)`:
/// the newest write of each key among the writes of records that still stand,
/// leaving out the rows of the class `stays`, which never reach the thing.
///
/// **A value stored as permanent ids is read here as the handles those ids answer
/// to**, off the same old store: each id is replaced, in place, by the handle of
/// the entity wearing it. What the upgraded binary serves is then
/// compared with that, position by position, so a list served in another order
/// or naming other things is a difference and not a match.
async fn fields_every_thing_holds(pool: &sqlx::MySqlPool) -> Vec<(String, String, String)> {
    let rows: Vec<(Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT badge, id, merged_into FROM entity")
            .fetch_all(pool)
            .await
            .expect("the old store's badges read");
    // **A folded thing answers as the thing it was folded into**, and a chain of
    // folds answers as the last: a value that names it is served as the survivor's
    // handle, so that is the handle it is compared as.
    let folded_into: std::collections::BTreeMap<&str, &str> = rows
        .iter()
        .filter_map(|(_, id, into)| into.as_deref().map(|into| (id.as_str(), into)))
        .collect();
    let answers_as = |id: &str| -> String {
        let mut now = id;
        for _ in 0..16 {
            match folded_into.get(now) {
                Some(next) => now = next,
                None => break,
            }
        }
        now.to_string()
    };
    let badges: std::collections::BTreeMap<String, String> = rows
        .iter()
        .filter_map(|(badge, id, _)| badge.as_ref().map(|badge| (badge.clone(), answers_as(id))))
        .collect();
    // A recording from before rows had a class holds none to leave out.
    let (outer, inner) = if has_column(pool, "field_write", "write_class").await {
        (
            "AND w.write_class <> 'stays'",
            "AND w2.write_class <> 'stays'",
        )
    } else {
        ("", "")
    };
    let held: Vec<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT e.id, w.`key`, w.value
         FROM field_write w
         JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id AND f.status = 'active'
         JOIN entity e ON e.badge = w.entity
         -- A folded handle is no thing: it answers by naming its survivor, and what
         -- it held is read as the survivor's.
         WHERE e.merged_into IS NULL
           AND w.value IS NOT NULL
           -- A key written under `fields` stays on its record and never reaches the
           -- thing, so a thing does not hold it; rows written before the class
           -- existed are `own` and do reach it.
           {outer}
           -- The two keys jojobot writes about a record and never folds onto a thing.
           AND w.`key` NOT IN ('merged_from', 'retracts')
           -- A key declared to describe its record is the record's own and the thing
           -- never holds it.
           AND w.`key` NOT IN (SELECT key_name FROM type_field WHERE folds = 'describes')
           AND w.ordinal = (
             SELECT MAX(w2.ordinal)
             FROM field_write w2
             JOIN fact f2 ON f2.entity = w2.entity AND f2.id = w2.fact_id AND f2.status = 'active'
             WHERE w2.entity = w.entity AND w2.`key` = w.`key`
               {inner})
         ORDER BY e.id, w.`key`"
    )))
    .fetch_all(pool)
    .await
    .expect("the old store's folded fields read");
    held.into_iter()
        .map(|(handle, key, value)| (handle, key, with_badges_as_handles(&value, &badges)))
        .collect()
}

/// **The rows `fields_every_thing_holds` cannot place under a handle**, as
/// `(entity, key)`: the same newest value of a key on a record that still
/// stands, whose entity column no entity wears as its badge. That is a field
/// written in a store that never badged, keyed by a bare handle. The join to
/// `entity` leaves such a row out without a word, so the oracle would compare
/// less than the fixture holds and still pass; this names what it left out.
async fn held_rows_that_do_not_join(pool: &sqlx::MySqlPool) -> Vec<(String, String)> {
    let (outer, inner) = if has_column(pool, "field_write", "write_class").await {
        (
            "AND w.write_class <> 'stays'",
            "AND w2.write_class <> 'stays'",
        )
    } else {
        ("", "")
    };
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT w.entity, w.`key`
         FROM field_write w
         JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id AND f.status = 'active'
         LEFT JOIN entity e ON e.badge = w.entity
         WHERE e.id IS NULL
           AND w.value IS NOT NULL
           {outer}
           AND w.`key` NOT IN ('merged_from', 'retracts')
           AND w.`key` NOT IN (SELECT key_name FROM type_field WHERE folds = 'describes')
           AND w.ordinal = (
             SELECT MAX(w2.ordinal)
             FROM field_write w2
             JOIN fact f2 ON f2.entity = w2.entity AND f2.id = w2.fact_id AND f2.status = 'active'
             WHERE w2.entity = w.entity AND w2.`key` = w.`key`
               {inner})
         ORDER BY w.entity, w.`key`"
    )))
    .fetch_all(pool)
    .await
    .expect("the old store's unjoined fields read")
}

/// **The oracle compares everything the fixture holds, or the gate fails.** A
/// row it could not join is not compared, and a comparison that skipped rows
/// says nothing about them.
async fn assert_every_held_row_joined(pool: &sqlx::MySqlPool, git_ref: &str) {
    let unjoined = held_rows_that_do_not_join(pool).await;
    assert!(
        unjoined.is_empty(),
        "recorded at {git_ref}: {} held values are unjoined to any entity, so the upgrade \
         comparison would skip them: {unjoined:?}",
        unjoined.len()
    );
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
        // **The one change the boot makes on purpose**: a role's holder and moment
        // move off the bot onto the role object, which the role cases hold.
        if key.starts_with("role/") {
            continue;
        }
        let read = surface
            .call("recall", json!({"subject": handle, "keys": [key]}))
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&read).unwrap_or_else(|_| {
            panic!("recorded at {git_ref}: {handle} did not read back: {read}")
        });
        // **The thing read, found by its handle**: a thread or a topic that holds
        // decisions answers them first, so the first object is not always the thing.
        let thing = parsed["objects"]
            .as_array()
            .and_then(|all| all.iter().find(|object| object["id"] == handle.as_str()))
            .unwrap_or(&parsed["objects"][0]);
        let now = thing["fields"][key].as_str().unwrap_or_else(|| {
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

/// **A fixture whose rows all join leaves nothing out of the comparison.** Every
/// recording comes from a build that badges, so every held value belongs to an
/// entity that wears the badge it is keyed by.
#[tokio::test]
async fn every_recorded_held_value_joins_to_an_entity() {
    let restored = restore_a_fixture(&fixture_dir(), "joins-last-deployed").await;
    assert!(
        !restored.held_before.is_empty(),
        "the comparison is over something at {}",
        restored.git_ref
    );
}

/// **A held value keyed by a bare handle fails the gate.** One thing in the
/// restored store loses its badge, which is what a store that never badged
/// looks like to the join: its field rows are still keyed by the old badge and
/// no entity wears it. The oracle must name them and stop, not compare the rest.
#[tokio::test]
#[should_panic(expected = "unjoined")]
async fn a_held_value_the_oracle_cannot_join_fails_the_gate() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-upgrade-gate-unjoined");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let git_ref = std::fs::read_to_string(format!("{}/ref.txt", fixture_dir()))
        .expect("the fixture's ref")
        .trim()
        .to_string();
    let store = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("a fresh store comes up to receive the fixture");
    let dump = std::fs::read_to_string(format!("{}/doltdump.sql", fixture_dir()))
        .expect("the fixture's dump");
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(store.pool())
            .await
            .expect("the fixture replays");
    }
    let (holder, _, _) = fields_every_thing_holds(store.pool())
        .await
        .into_iter()
        .next()
        .expect("the fixture holds a field");
    sqlx::query("UPDATE entity SET badge = NULL WHERE id = ?")
        .bind(&holder)
        .execute(store.pool())
        .await
        .expect("one thing loses its badge");
    assert_every_held_row_joined(store.pool(), &git_ref).await;
}

/// **The statements of a dump, split at the semicolons that end one.** A `CREATE
/// TABLE` spans many lines, so the dump is read as a stream rather than by line.
/// A semicolon inside a single-quoted value ends nothing: the reader follows the
/// quotes, a backslash that escapes the next character and a doubled quote.
fn split_sql_statements(dump: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_value = false;
    let mut chars = dump.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if in_value {
            match c {
                '\\' => current.extend(chars.next()),
                '\'' if chars.peek() == Some(&'\'') => current.extend(chars.next()),
                '\'' => in_value = false,
                _ => {}
            }
        } else if c == '\'' {
            in_value = true;
        } else if c == ';' {
            statements.push(std::mem::take(&mut current).trim().to_string());
        }
    }
    let rest = current.trim();
    if !rest.is_empty() {
        statements.push(format!("{rest};"));
    }
    statements.retain(|statement| statement != ";");
    statements
}

/// **The splitter keeps a semicolon that sits inside a value.** A document's
/// sections carry the edges field `;;\n\n;\n`, so a recording of the deploy 6.1
/// build holds semicolons inside strings, and a split on every semicolon cuts a
/// statement in the middle of a value. The same text holds an escaped quote and a
/// doubled one, which end no string either.
#[test]
fn the_statement_splitter_keeps_a_semicolon_inside_a_value() {
    let dump = "INSERT INTO `t` VALUES ('a;b','it\\'s; here','it''s; there');\n\
                DROP TABLE IF EXISTS `u`;\n";
    assert_eq!(
        split_sql_statements(dump),
        [
            "INSERT INTO `t` VALUES ('a;b','it\\'s; here','it''s; there');".to_string(),
            "DROP TABLE IF EXISTS `u`;".to_string(),
        ]
    );
}

/// **A role claimed before roles were objects is carried onto its object by the
/// first boot, and stays held.** The recording keeps the claim on the bot. The
/// boot writes the holder and the moment onto a role object and takes the keys
/// off, so the role object names the holder before and after the holder's next
/// write, and the bot holds no key of the old shape. Without the pass, every
/// role in the store would read as empty here.
async fn assert_a_role_claimed_in_the_old_shape_is_carried_by_the_boot_and_stays_held(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
    role_key: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let the_role_object = || async {
        let read = surface
            .call("recall", json!({"kind": "role", "parent": "bot:assistant"}))
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail("the role objects", &read));
        parsed["objects"]
            .as_array()
            .and_then(|all| {
                all.iter()
                    .find(|o| o["id"] == "role:upgrade-fixture-holder")
            })
            .cloned()
            .unwrap_or_else(|| fail("the role object the boot made", &read))
    };

    let carried = the_role_object().await;
    if carried["fields"]["holder"] != role_holder
        || carried["fields"]["claimed_at"].as_str().is_none()
    {
        fail(
            "the boot carrying the old claim onto the role object",
            &carried.to_string(),
        );
    }
    // The holder writes. That renews the claim on the role object.
    let beat = surface
        .call(
            "journal",
            json!({"entry": "worked on after the upgrade", "sid": role_key}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the upgrade was refused", &beat);
    }
    let renewed = the_role_object().await;
    // **An equal moment is right here**: a renewal inside the renewal age keeps the
    // moment the lease already holds, and the aged renewal further on is the check
    // that a renewal moves it.
    if renewed["fields"]["holder"] != role_holder {
        fail(
            "the holder's write keeping the claim on the role object",
            &renewed.to_string(),
        );
    }
    if renewed["fields"]["claimed_at"].as_str() < carried["fields"]["claimed_at"].as_str() {
        fail(
            "the holder's write moving the claim's moment forward",
            &renewed.to_string(),
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
    role_key: &str,
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
        || boot["snapshot"]["entities"]["by_kind"]["topic"]
            .as_u64()
            .is_none_or(|held| held == 0)
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
            json!({"counts_only": true, "sid": role_key}),
        )
        .await;
    let counted = json_of("the mailbox counts", &read);
    // No message in the recording was replaced, so the count is left out.
    if counted["counts"].get("superseded").is_some() {
        fail("no replaced message in the recorded box", &read);
    }
    let quarantined = &counted["quarantined"];
    let id = match (
        quarantined["count"].as_u64(),
        quarantined["ids"][0].as_str(),
    ) {
        (Some(1), Some(id)) => id.to_string(),
        _ => fail("one quarantined message", &read),
    };
    let read = surface
        .call("read_message", json!({"message_id": id, "sid": role_key}))
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
        // A claim written before the bags were kept apart was an own field that
        // reached, and it reads as it always did: under `fields`, with no `sets`.
        || !landed["sets"].is_null()
    {
        fail("the recorded fact's provenance, edge or fields", &read);
    }

    // **A reference-typed value, a claim worked out from another, and a
    // merge** — the three shapes an upgrade of the storage has to carry and
    // an older recording never held.
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

    // The role-claim fields: the boot carried them onto the role object, and the
    // bot holds none of the old keys.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role-claim fields", &read));
    if parsed["objects"][0]["fields"]
        .as_object()
        .is_some_and(|fields| fields.keys().any(|key| key.starts_with("role/")))
    {
        fail("the bot holding no key of the old shape", &read);
    }
    let read = surface
        .call("recall", json!({"subject": "role:upgrade-fixture-holder"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role-claim fields", &read));
    // **The holder is the session the recording booted as, recorded beside
    // the dump** — a check that any non-empty value passes would pass on a
    // field the upgrade had rewritten to garbage.
    if role_holder.is_empty() || parsed["objects"][0]["fields"]["holder"] != role_holder {
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
        // **Mail written before a message could replace another replaces
        // nothing and is replaced by nothing**, and says neither. The keys are
        // left out of a message they do not apply to.
        if read.contains("\"supersedes\"") || read.contains("\"superseded_by\"") {
            fail(&format!("the {state} mail message replaces nothing"), &read);
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
    // The boot that makes the offer carries it; the answer to the offer, which
    // the boot above is, leaves the rules out.
    let offering = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let offering: serde_json::Value = serde_json::from_str(&offering)
        .unwrap_or_else(|_| fail("the assistant's boot that makes the offer", &offering));
    if !offering["identity"]["rules"]
        .as_array()
        .is_some_and(|rules| {
            rules
                .iter()
                .any(|rule| rule["content"] == "the recorder keeps its recording rule")
        })
    {
        fail(
            "the starred rule on the assistant's boot",
            &offering.to_string(),
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

    // The next write lands whatever its word, and the answer names the project's
    // own list, which the older build wrote and this one reads unchanged.
    let landed = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "waiting on a review",
                   "provenance": "testimony", "sets": {"status": "waiting"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("a status the project lists", &landed);
    }
    let unlisted = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "an unlisted status",
                   "provenance": "testimony", "sets": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if unlisted.contains("\"status\":\"blocked\"")
        || !unlisted.contains("status_word_note")
        || !unlisted.contains("review")
    {
        fail(
            "an unlisted status landing with the project's columns named",
            &unlisted,
        );
    }
    let unlisted = surface
        .call(
            "capture",
            json!({"subject": "project:upgrade-fixture-project", "content": "an unlisted status",
                   "provenance": "testimony", "sets": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if unlisted.contains("\"status\":\"blocked\"") || !unlisted.contains("status_word_note") {
        fail(
            "the project's own keys holding a status it does not list",
            &unlisted,
        );
    }
    // **The columns the older build wrote are where they were** after both writes.
    let read = surface
        .call(
            "recall",
            json!({"subject": "project:upgrade-fixture-project"}),
        )
        .await;
    let project_after = fields_of(&read, "the project after the writes");
    if project_after["columns"] != project["columns"] {
        fail("the recorded columns after the writes", &read);
    }
}

/// **The shapes an older fixture was missing, each read back as the older
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

/// **An instance record somebody made by hand keeps what it holds across the
/// boots that seed the record.** The build makes the record when none exists, so
/// the case is the one an existing instance is in: the recording holds one with
/// the zone on it, the operator is written onto it through the served surface,
/// and the next boot reads both unchanged. A seed that remade, renamed or
/// refused over the record would drop a key or log a warning, and the boot's own
/// warning check fails the gate on the second.
#[tokio::test]
async fn a_hand_made_instance_record_keeps_its_operator_and_zone_across_a_boot() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("instance-record").await;
    let mut editing = open_the_store(&state_dir, store_port).await;
    let changed = sqlx::query(
        "UPDATE entity SET source = 'test', name = 'A hand-made instance' WHERE id = 'topic:instance'",
    )
    .execute(editing.pool()).await.expect("the existing instance record is made by hand");
    assert_eq!(
        changed.rows_affected(),
        1,
        "the cached store holds the instance record"
    );
    editing.stop().await;
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot is json");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    for (what, tool, args) in [
        (
            "making the operator",
            "add_entity",
            json!({"kind": "person", "handle": "milhouse", "name": "Milhouse",
                   "source": "user-named", "sid": sid}),
        ),
        (
            "naming the operator on the record",
            "capture",
            json!({"subject": "topic:instance", "content": "who the operator is",
                   "provenance": "testimony", "sid": sid,
                   "sets": {"operator": "person:milhouse"}}),
        ),
    ] {
        let answer = surface.call(tool, args).await;
        if answer.contains("\"status\":\"blocked\"") {
            fail(what, &answer);
        }
    }
    surface.finish().await;
    first.stop().await;

    // The first boot reported the recording's ambiguous handle; this one is the
    // second and reports none.
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call("recall", json!({"subject": "topic:instance"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the instance record", &read));
    let record = &parsed["objects"][0];
    if record["fields"]["timezone"] != "America/New_York"
        || record["fields"]["operator"] != "person:milhouse"
    {
        fail("the zone and the operator both being read", &read);
    }
    if record["source"] != "test" || record["name"] != "A hand-made instance" {
        fail("the record being the one made by hand", &read);
    }
    surface.finish().await;
    second.stop().await;
    drop(scratch);
}

/// **A store upgraded into the voucher kind holds the kind and no voucher.**
/// The kind arrives with the boot's own seed, and nothing writes a voucher on
/// the way: the gate is open until a later release issues the first one.
async fn assert_the_voucher_kind_arrives_and_no_voucher_exists(surface: &Surface, git_ref: &str) {
    let read = surface
        .call("list_entities", json!({"kind": "voucher"}))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&read).unwrap_or_else(|_| {
        panic!("recorded at {git_ref}: the voucher listing did not read back: {read}")
    });
    assert_eq!(
        parsed["count"], 0,
        "recorded at {git_ref}: a store upgraded into the voucher kind holds a voucher: {read}"
    );
}

/// **After the upgrade, `field_link` holds the links the writes imply, and
/// nothing else.**
///
/// Raw marked items, read without the production parser, imply
/// the expected rows. Served values follow merges; raw targets keep folded keys.
/// Historical rows' ordinals count; a cleared value implies no remaining link.
#[tokio::test]
async fn field_link_holds_the_links_the_writes_imply_and_nothing_else() {
    use std::io::Write;
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "links").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    booted.stop().await;
    let mut reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let expected = links_the_stored_fields_imply(reading.pool()).await;
    let held = links_field_link_holds(reading.pool()).await;
    assert!(!expected.is_empty(), "recording {git_ref} implies no link");
    check_field_link_rows(&expected, &held)
        .unwrap_or_else(|e| panic!("recorded at {git_ref}: {e}"));
    // The same check must reject missing rows, extra targets and wrong ordinals.
    // Each real-table mutation runs inside a transaction and is rolled back.
    // The last row's next ordinal cannot exist for that source and key.
    let (entity, key, ordinal, target) = expected.last().unwrap();
    for break_kind in ["missing", "wrong", "ordinal"] {
        let mut tx = reading.pool().begin().await.unwrap();
        let statement = match break_kind {
            "missing" => {
                "DELETE FROM field_link WHERE entity = ? AND `key` = ? AND ordinal = ? AND target = ?"
            }
            _ => "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, ?, ?, ?)",
        };
        let changed = sqlx::query(sqlx::AssertSqlSafe(statement))
            .bind(entity)
            .bind(key)
            .bind(*ordinal + i64::from(break_kind == "ordinal"))
            .bind(if break_kind == "wrong" {
                "@not-a-store-key"
            } else {
                target.as_str()
            })
            .execute(&mut *tx)
            .await
            .expect("the raw link row is sabotaged");
        assert_eq!(changed.rows_affected(), 1);
        let broken = links_field_link_holds(&mut *tx).await;
        let refusal = check_field_link_rows(&expected, &broken)
            .expect_err("the oracle must reject the sabotaged raw row");
        // Direct stderr leaves a CI receipt even when this test passes.
        writeln!(
            std::io::stderr(),
            "raw-row sabotage {break_kind}: RED: {refusal}"
        )
        .unwrap();
        tx.rollback()
            .await
            .expect("the raw-row sabotage is rolled back");
    }
    check_field_link_rows(&expected, &links_field_link_holds(reading.pool()).await).unwrap();
    reading.stop().await;
    drop(scratch);
}

fn check_field_link_rows(
    expected: &std::collections::BTreeSet<Link>,
    held: &std::collections::BTreeSet<Link>,
) -> Result<(), String> {
    if expected == held {
        return Ok(());
    }
    let missing: Vec<_> = expected.difference(held).collect();
    let extra: Vec<_> = held.difference(expected).collect();
    Err(format!(
        "field_link disagrees with raw field_write: missing {missing:?}; extra {extra:?}"
    ))
}

/// Under On, each marked comma item links, whatever its key or neighboring text.
async fn links_the_stored_fields_imply(pool: &sqlx::MySqlPool) -> std::collections::BTreeSet<Link> {
    let writes: Vec<Link> = sqlx::query_as(
        "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .expect("the raw field writes read");
    links_the_raw_writes_imply(writes)
}

fn links_the_raw_writes_imply(writes: Vec<Link>) -> std::collections::BTreeSet<Link> {
    let mut links = std::collections::BTreeSet::new();
    for (entity, key, ordinal, value) in writes {
        for item in value.split(',') {
            if let Some(target) = item.trim().strip_prefix("@#").filter(|id| !id.is_empty()) {
                links.insert((entity.clone(), key.clone(), ordinal, target.to_string()));
            }
        }
    }
    links
}

#[test]
fn the_raw_link_oracle_reads_only_marks_in_each_item() {
    let row =
        |key: &str, ordinal, value: &str| ("held001".into(), key.into(), ordinal, value.into());
    let writes = vec![
        row("looks_at", 1, "view:colleagues"),
        row("looks_at", 2, "@#red001, kitchen, @#red002, @#red001, @#"),
        row("links", 3, "@#view:colleagues"),
        row("links", 4, "red003"),
        row("links", 5, "@#red004"),
    ];
    assert_eq!(
        links_the_raw_writes_imply(writes),
        std::collections::BTreeSet::from([
            row("looks_at", 2, "red001"),
            row("looks_at", 2, "red002"),
            row("links", 3, "view:colleagues"),
            row("links", 5, "red004"),
        ])
    );
}

async fn links_field_link_holds<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::MySql>,
) -> std::collections::BTreeSet<Link> {
    sqlx::query_as::<_, Link>(
        "SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4",
    )
    .fetch_all(executor)
    .await
    .expect("field_link reads")
    .into_iter()
    .collect()
}

/// **What a real store's pool logs when a statement runs past the alert
/// threshold, and four other lines logged beside it.** The slow statement is a
/// real one: `SLEEP` against the store, through the pool the binary builds its
/// own the same way, so the line is the one sqlx writes and not one invented
/// for the case. Every line carries `label`, so a sibling case's log in the
/// store's one global sink is not read as this case's.
async fn lines_logged_around_a_slow_statement(label: &str) -> Vec<String> {
    let state_dir =
        jojobot_adapters::testing::scratch_path(&format!("jojobot-upgrade-gate-{label}"));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    // `free_port` installs the store's log sink, the process's one global
    // subscriber, before the store starts.
    let store = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("a fresh store comes up");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT SLEEP(1.3) /* {label} */"
    )))
    .execute(store.pool())
    .await
    .expect("the slow statement runs");
    tracing::warn!("{label} a boot step degraded");
    tracing::warn!(target: "sqlx::query", "{label} an alert of another kind");
    tracing::warn!(
        target: "jojobot_adapters::dolt",
        "{label} slow statement: execution time exceeded alert threshold"
    );
    tracing::error!("{label} a pass could not finish");
    jojobot_adapters::testing::store_log()
        .lines()
        .filter(|line| line.contains(label))
        .map(str::to_string)
        .collect()
}

/// **The slow-statement alert is not a failed boot.** On a busy machine a
/// statement of the boot runs past sqlx's alert threshold and sqlx says so at
/// warning level. That says nothing about the boot, so the gate lets exactly
/// that line through: by its target and its message.
#[tokio::test]
async fn a_slow_statement_alert_is_not_a_degradation() {
    let logged = lines_logged_around_a_slow_statement("slow-alert-alone").await;
    let alert = logged
        .iter()
        .find(|line| line.contains("sqlx::query: slow statement"))
        .unwrap_or_else(|| {
            panic!("sqlx logged no slow-statement alert, so this proves nothing: {logged:?}")
        });
    assert!(
        degradations(std::slice::from_ref(alert), false, &[]).is_empty(),
        "the slow-statement alert counted as a degraded boot: {alert}"
    );
}

/// **Every other warning and every error still fails the gate**, beside the
/// alert it lets through. A warning from the same target with another message,
/// the alert's own message from another target, a warning from anywhere else
/// and an error are each a degradation, and the alert in the same log is not.
#[tokio::test]
async fn any_other_warning_or_error_is_still_a_degradation_beside_the_alert() {
    let logged = lines_logged_around_a_slow_statement("slow-alert-beside").await;
    let failing = degradations(&logged, false, &[]);
    assert_eq!(
        failing.len(),
        4,
        "the four other lines fail, and only they: {logged:?}"
    );
    assert!(
        failing
            .iter()
            .all(|line| !line.contains("sqlx::query: slow statement")),
        "the alert was counted as a degradation: {failing:?}"
    );
    for said in [
        "a boot step degraded",
        "an alert of another kind",
        "execution time exceeded alert threshold",
        "a pass could not finish",
    ] {
        assert!(
            failing.iter().any(|line| line.contains(said)),
            "{said:?} did not fail the gate: {failing:?}"
        );
    }
}

/// **What a real pool logs when a connection takes longer to acquire than its
/// slow-acquire threshold, and three other lines logged beside it.** The pool is
/// opened onto a fresh store with a threshold no acquire can meet, so the line is
/// the one sqlx writes and not one invented for the case. The real line carries
/// no label, so the threshold is the case's own mark: a sibling case's pool
/// keeps the default and is not read as this case's. The three others carry
/// `label`.
async fn lines_logged_around_a_slow_acquire(label: &str, threshold: Duration) -> Vec<String> {
    let state_dir =
        jojobot_adapters::testing::scratch_path(&format!("jojobot-upgrade-gate-{label}"));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let port = free_port();
    let _store = Dolt::start(&state_dir.join("db"), port)
        .await
        .expect("a fresh store comes up");
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_slow_threshold(threshold)
        .connect(&format!("mysql://root@127.0.0.1:{port}"))
        .await
        .expect("a pool onto the store");
    drop(pool.acquire().await.expect("a connection is acquired"));
    tracing::warn!(target: "sqlx::pool::acquire", "{label} an alert of another message");
    tracing::warn!(
        target: "sqlx::query",
        "acquired connection, but time to acquire exceeded slow threshold {label}"
    );
    tracing::warn!("{label} a boot step degraded");
    let mark = format!("slow_acquire_threshold_secs={:?}", threshold.as_secs_f64());
    jojobot_adapters::testing::store_log()
        .lines()
        .filter(|line| line.contains(label) || line.contains(&mark))
        .map(str::to_string)
        .collect()
}

/// **The slow-acquire alert is not a failed boot.** On a busy machine a
/// connection of the boot takes longer to acquire than sqlx's threshold and sqlx
/// says so at warning level. That says nothing about the boot, so the gate lets
/// exactly that line through: by its target and its message.
#[tokio::test]
async fn a_slow_acquire_alert_is_not_a_degradation() {
    let logged =
        lines_logged_around_a_slow_acquire("slow-acquire-alone", Duration::from_micros(7)).await;
    let alert = logged
        .iter()
        .find(|line| line.contains("sqlx::pool::acquire: acquired connection"))
        .unwrap_or_else(|| {
            panic!("sqlx logged no slow-acquire alert, so this proves nothing: {logged:?}")
        });
    assert!(
        degradations(std::slice::from_ref(alert), false, &[]).is_empty(),
        "the slow-acquire alert counted as a degraded boot: {alert}"
    );
}

/// **Every other warning still fails the gate**, beside the alert it lets
/// through. A warning from the same target with another message, the alert's own
/// message from another target and a warning from anywhere else are each a
/// degradation, and the alert in the same log is not.
#[tokio::test]
async fn any_other_warning_is_still_a_degradation_beside_the_acquire_alert() {
    let logged =
        lines_logged_around_a_slow_acquire("slow-acquire-beside", Duration::from_micros(9)).await;
    assert!(
        logged
            .iter()
            .any(|line| line.contains("sqlx::pool::acquire: acquired connection")),
        "sqlx logged no slow-acquire alert, so this proves nothing: {logged:?}"
    );
    let failing = degradations(&logged, false, &[]);
    assert_eq!(
        failing.len(),
        3,
        "the three other lines fail, and only they: {logged:?}"
    );
    assert!(
        failing
            .iter()
            .all(|line| !line.contains("sqlx::pool::acquire: acquired connection")),
        "the alert was counted as a degradation: {failing:?}"
    );
    for said in [
        "an alert of another message",
        "exceeded slow threshold slow-acquire-beside",
        "a boot step degraded",
    ] {
        assert!(
            failing.iter().any(|line| line.contains(said)),
            "{said:?} did not fail the gate: {failing:?}"
        );
    }
}

/// The claim of the holder with these words, as a recall serves it.
fn the_claim_saying<'a>(read: &'a serde_json::Value, words: &str) -> &'a serde_json::Value {
    read["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["content"] == words))
        .unwrap_or_else(|| panic!("the holder has no claim saying {words:?}: {read}"))
}

/// **A pointer at a record the build supplies is valid, and the boot says nothing
/// about it.** The recording holds an edge, a ref and a parent at shipped views,
/// which have no row of their own. A boot that read one as a stale handle would
/// log an error naming it, and `boot_current` holds every boot to no error line.
/// The pointers also read back as they were written, so a boot that dropped them
/// to stay quiet is not a pass.
#[tokio::test]
async fn a_pointer_at_a_record_the_build_supplies_boots_quiet_and_reads_back() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "supplied-pointers").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": FOLD_HOLDER, "facts": true}))
            .await,
    )
    .expect("the holder reads");
    let child: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": SUPPLIED_CHILD}))
            .await,
    )
    .expect("the child reads");
    booted.stop().await;

    assert_eq!(
        the_claim_saying(&read, "keeps an eye on the colleagues view")["edge"]["object"],
        "view:colleagues",
        "recorded at {git_ref}: {read}"
    );
    assert_eq!(
        the_claim_saying(&read, "mentions the loops view among its refs")["refs"][0],
        "view:loops",
        "recorded at {git_ref}: {read}"
    );
    assert_eq!(
        child["objects"][0]["parent"], "view:ready",
        "recorded at {git_ref}: {child}"
    );
    drop(scratch);
}

/// **A reference-typed field that names a record the build supplies is valid, and
/// a boot that owes the reference pass says nothing about it.** The recording
/// holds such a field, on a key declared to hold a reference to anything. The
/// deployed build had already run the pass, so its marker is dropped first: a
/// store that has not run it is one that last booted on a build before the pass.
/// The record has no row to resolve through, and a boot that read the value as a
/// stale handle would log an error naming it. The field reads back as the record.
#[tokio::test]
async fn a_reference_field_naming_a_supplied_record_boots_quiet_and_reads_back() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "supplied-reference").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_write WHERE `key` = 'looks_at'")
        .fetch_one(store.pool())
        .await
        .expect("the field writes count");
    let dropped = sqlx::query("DELETE FROM store_pass WHERE name = 'reference field lowering'")
        .execute(store.pool())
        .await
        .expect("the pass is owed again")
        .rows_affected();
    store.stop().await;
    assert_eq!(
        (held, dropped),
        (1, 1),
        "recorded at {git_ref}: the recording holds the field, and the pass had run"
    );

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": FOLD_HOLDER}))
            .await,
    )
    .expect("the holder reads");
    booted.stop().await;
    assert_eq!(
        read["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "recorded at {git_ref}: {read}"
    );
    drop(scratch);
}

/// **The first boot over a store the deployed build filled says nothing is wrong,
/// and the second boot logs the same.** The store's pass markers are set, so the
/// passes that already ran take their settled branch, and the passes this build
/// adds run once. `boot_current` holds every boot to no warning and no error
/// line; this case names the property and boots until it has seen it three times.
#[tokio::test]
async fn the_boots_over_a_store_the_deployed_build_filled_log_no_warning_and_no_error() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "no-noise").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let marked: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the pass markers read");
    store.stop().await;
    assert!(
        marked.iter().any(|name| name == "field value lowering")
            && marked.iter().any(|name| name == "reference field lowering"),
        "the recording holds the markers the deployed build's own boot set: {marked:?}"
    );
    let mut dumps = Vec::new();
    for boot in 1..=3 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let log = booted.log.clone();
        booted.stop().await;
        let noisy = degradations(&log, false, &[]);
        assert!(
            noisy.is_empty(),
            "boot {boot} over the store recorded at {git_ref} logged: {noisy:?}"
        );
        dumps.push(dump_store(&state_dir).await);
    }
    // **And a boot that has nothing left to do writes nothing**: the second and
    // the third leave the store as the first left it.
    assert!(
        dumps[1] == dumps[0] && dumps[2] == dumps[1],
        "a later boot over the store recorded at {git_ref} changed it: {} lines after the \
         first boot, {} after the second, {} after the third",
        dumps[0].lines().count(),
        dumps[1].lines().count(),
        dumps[2].lines().count()
    );
    drop(scratch);
}

/// **A run with hundreds of the deployed build's per-write rows boots inside the
/// time a boot is given.** The recording holds one run with a long history. The
/// case states the size of it from the store, boots, and holds the boot to the
/// time the gate gives a boot (the serving line has to appear) and to half of it,
/// so a boot that grows with a run's history moves this case before it moves the
/// deadline.
#[tokio::test]
async fn a_boot_over_a_run_with_a_long_history_stays_inside_the_time_a_boot_is_given() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "long-history").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let (longest_run, rows): (String, i64) = sqlx::query_as(
        "SELECT session, COUNT(*) AS n FROM session_write GROUP BY session ORDER BY n DESC LIMIT 1",
    )
    .fetch_one(store.pool())
    .await
    .expect("the longest run reads");
    store.stop().await;
    assert!(
        rows >= 200,
        "the recording holds a run with a long history, or this proves nothing: {longest_run} \
         holds {rows} rows"
    );

    let began = std::time::Instant::now();
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let took = began.elapsed();
    booted.stop().await;
    assert!(
        took < jojobot_exercise::room::READY_TIMEOUT / 2,
        "the boot over a run of {rows} per-write rows took {took:?}, over half of the {:?} a boot \
         is given",
        jojobot_exercise::room::READY_TIMEOUT
    );
    drop(scratch);
}

const CROWDED_THREAD: &str = "thread:upgrade-fixture-crowded";
const OLD_VIEW: &str = "view:upgrade-fixture-view";
const OLD_VIEW_RED: &str = "thing:upgrade-fixture-red";
const OLD_VIEW_BLUE: &str = "thing:upgrade-fixture-blue";
const LINK_TABLE_REBUILT: &str = "LINK TABLE DISAGREED";
const SHIPPED_HANDLE_WITH_A_ROW: &str = "view:loops";
const SHIPPED_HANDLE_WITHOUT_ONE: &str = "view:ready";
const RETIRED_HANDLE: &str = "view:upgrade-fixture-retired";
const HAND_REMOVED_HANDLE: &str = "view:upgrade-fixture-hand-removed";
const DRAWING_HANDLE: &str = "thing:upgrade-fixture-drawing";

/// **A stored rhythm that counts days reads on the head exactly as it did.**
/// The recording holds no rhythm, so the case writes one into the
/// restored store: seven days, counted from the day it fell due, basis the 1st
/// of August. The head's cadence type and its calendar rules add keys; they do
/// not move a stored rhythm, so it falls due on the 8th and not the 7th, and
/// the fields it holds are the ones written, with no `rrule` and no anchor.
#[tokio::test]
async fn a_stored_cadence_days_rhythm_reads_the_same_on_the_head() {
    use jojobot_adapters::dolt::memory::DoltMemory;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "stored-rhythm").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool().clone();
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let kettle = EntityId("thing:kettle".into());
    let descale = EntityId("rhythm:descale".into());
    // The recording may already hold the thing the rhythm hangs off.
    let held = memory.list_entities(None).await.expect("the entities read");
    if !held.iter().any(|entity| entity.id == kettle) {
        memory
            .add_entity(NewEntity::new(kettle.clone(), "The Kettle", "test"))
            .await
            .expect("the kettle is written")
            .written()
            .expect("nothing collides");
    }
    memory
        .add_entity(NewEntity {
            parent: Some(kettle),
            ..NewEntity::new(descale.clone(), "Descale", "test")
        })
        .await
        .expect("the rhythm is written")
        .written()
        .expect("nothing collides");
    let schedule = [
        ("cadence_days", "7"),
        ("advances_from", "due_date"),
        ("counts_from", "2026-08-01"),
        ("last_check_in", "2026-08-01"),
        ("name", "Descale"),
    ];
    memory
        .capture(NewFact {
            fields: schedule
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            sets: schedule.iter().map(|(key, _)| key.to_string()).collect(),
            ..NewFact::about(
                descale,
                "descale the kettle weekly",
                "2026-08-01".parse().expect("a day"),
            )
        })
        .await
        .expect("the schedule is written");
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let owed = |day: &'static str| {
        surface.call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": day}}),
        )
    };
    let on_the_7th = owed("2026-08-07").await;
    let on_the_8th = owed("2026-08-08").await;
    let held = surface
        .call("recall", json!({"subject": "rhythm:descale"}))
        .await;
    booted.stop().await;
    assert!(
        !on_the_7th.contains("rhythm:descale"),
        "recorded at {git_ref}: the day before it falls due it is not owed: {on_the_7th}"
    );
    assert!(
        on_the_8th.contains("rhythm:descale"),
        "recorded at {git_ref}: a week after the basis it is owed: {on_the_8th}"
    );
    assert!(
        held.contains("\"cadence_days\":\"7\"")
            && !held.contains("rrule")
            && !held.contains("anchored_on"),
        "recorded at {git_ref}: the stored rhythm holds only what was written: {held}"
    );
    drop(scratch);
}

/// **A stored rhythm of the shape a live instance holds checks in on the head as
/// it did before.** The read above only reads the schedule; this case RUNS the
/// check-in. Two loops are written into the restored store the way the deployed
/// build kept them: a number of days (7 and 30), counted from the day of the
/// check-in, with the basis and the last check-in on 12 October and the outcome
/// `ran`, and no `anchored_on` and no `rrule`. Each falls due a cycle after the
/// basis and not before. A check-in on the 18th lands, moves the basis to the
/// 18th, puts the next due day a cycle after it, and writes no calendar anchor.
#[tokio::test]
async fn a_stored_cadence_days_rhythm_checks_in_on_the_head_as_it_did_before() {
    use jojobot_adapters::dolt::memory::DoltMemory;
    use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "stored-rhythm-check-in").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool().clone();
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    // (the loop, the thing it hangs off, the cycle in days, and the days the
    // case asks about: the last day it is not owed, the day it falls due, and the
    // same two after the check-in on the 18th.)
    let loops: [(&str, &str, &str, &str, [&str; 4]); 2] = [
        (
            "rhythm:descale",
            "Descale",
            "thing:kettle",
            "7",
            ["2026-10-18", "2026-10-19", "2026-10-24", "2026-10-25"],
        ),
        (
            "rhythm:swap-the-air-filter",
            "Swap the air filter",
            "thing:the-air-filter",
            "30",
            ["2026-11-10", "2026-11-11", "2026-11-16", "2026-11-17"],
        ),
    ];
    // The recording may already hold a thing a rhythm hangs off.
    let held = memory.list_entities(None).await.expect("the entities read");
    for (handle, name, parent, cadence, _) in &loops {
        let parent = EntityId((*parent).into());
        if !held.iter().any(|entity| entity.id == parent) {
            memory
                .add_entity(NewEntity::new(parent.clone(), "A thing", "test"))
                .await
                .expect("the thing is written")
                .written()
                .expect("nothing collides");
        }
        memory
            .add_entity(NewEntity {
                parent: Some(parent),
                ..NewEntity::new(EntityId((*handle).into()), *name, "test")
            })
            .await
            .expect("the rhythm is written")
            .written()
            .expect("nothing collides");
        let schedule = [
            ("cadence_days", *cadence),
            ("advances_from", "check_in_date"),
            ("counts_from", "2026-10-12"),
            ("last_check_in", "2026-10-12"),
            ("outcome", "ran"),
            ("name", *name),
        ];
        memory
            .capture(NewFact {
                fields: schedule
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                sets: schedule.iter().map(|(key, _)| key.to_string()).collect(),
                ..NewFact::about(
                    EntityId((*handle).into()),
                    "the loop as the deployed build kept it",
                    "2026-10-12".parse().expect("a day"),
                )
            })
            .await
            .expect("the schedule is written");
    }
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let owed = |day: &'static str| {
        surface.call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": day}}),
        )
    };
    let started = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await
        .expect("the shipped identity boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot handed back no handle: {started}"))
        .to_string();

    let mut failures = Vec::new();
    for &(handle, _, _, cadence, [last_clear, due, last_clear_after, due_after]) in &loops {
        // Before any check-in: not owed the day before the cycle ends, owed on it.
        let before = owed(last_clear).await;
        let on_the_day = owed(due).await;
        if before.contains(handle) || !on_the_day.contains(handle) {
            failures.push(format!(
                "{handle} ({cadence} days) before the check-in: owed on {last_clear}? {}; \
                 owed on {due}? {}",
                before.contains(handle),
                on_the_day.contains(handle)
            ));
        }
        // The check-in on the 18th, the way a bot makes one.
        let made = surface
            .must(
                "capture",
                json!({"subject": handle, "content": "did it", "provenance": "testimony",
                       "recorded_at": "2026-10-18", "check_in": "ran", "sid": sid}),
            )
            .await;
        if let Err(refused) = made {
            failures.push(format!("{handle}: the check-in was refused: {refused}"));
            continue;
        }
        let read = surface
            .must("recall", json!({"subject": handle, "sid": sid}))
            .await
            .expect("the rhythm reads");
        let fields = &read["objects"][0]["fields"];
        for (key, expected) in [
            ("counts_from", "2026-10-18"),
            ("last_check_in", "2026-10-18"),
            ("outcome", "ran"),
            ("advances_from", "check_in_date"),
            ("cadence_days", cadence),
        ] {
            if fields[key] != expected {
                failures.push(format!("{handle}: {key} is {} not {expected}", fields[key]));
            }
        }
        if !fields["anchored_on"].is_null() || !fields["rrule"].is_null() {
            failures.push(format!(
                "{handle}: a days cadence gained a calendar key: {fields}"
            ));
        }
        // After it: a cycle on from the 18th, not before.
        let before_next = owed(last_clear_after).await;
        let next = owed(due_after).await;
        if before_next.contains(handle) || !next.contains(handle) {
            failures.push(format!(
                "{handle} after the check-in: owed on {last_clear_after}? {}; owed on {due_after}? {}",
                before_next.contains(handle),
                next.contains(handle)
            ));
        }
    }
    booted.stop().await;
    assert!(
        failures.is_empty(),
        "recorded at {git_ref}: a stored days rhythm does not check in as it did: {failures:#?}"
    );
    drop(scratch);
}

/// **A store the build before the switch filled keeps every key on the build
/// that switched.** Before the switch a key written as an own field wore the class
/// `own` and reached its thing. The switch keeps no migration: it reads that class
/// as it always did, and writes the new class `stays` only for a write made after
/// it. A boot that read `own` as a key to leave behind would empty every thing the
/// old build filled.
///
/// The recordings predate the class column, so a first boot adds it and gives each
/// row the class `legacy`. The case then rewrites those rows to `own`, which is
/// what the build before the switch wrote for an own field, and boots again.
#[tokio::test]
async fn rows_classed_own_still_reach_their_things_on_the_last_deployed_store() {
    rows_classed_own_before_the_switch_still_reach_their_things_after_it_over(
        &fixture_dir(),
        "own-class-deploy3",
    )
    .await;
}

async fn rows_classed_own_before_the_switch_still_reach_their_things_after_it_over(
    fixture_dir: &str,
    label: &str,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
        ..
    } = restore_a_fixture(fixture_dir, label).await;

    boot_current(&state_dir, store_port, &git_ref)
        .await
        .stop()
        .await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let rewritten = sqlx::query("UPDATE field_write SET write_class = 'own'")
        .execute(store.pool())
        .await
        .expect("the rows are classed as the build before the switch classed them")
        .rows_affected();
    assert!(
        rewritten as usize >= held_before.len(),
        "recorded at {git_ref}: {rewritten} rows were classed own and the store holds {} \
             values, so the case would leave some in another class",
        held_before.len()
    );
    store.stop().await;

    // The first boot reported the recording's handle and marked the pass, so
    // this one reports nothing.
    let booted = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    assert_every_thing_holds_what_it_held(&surface, &git_ref, &held_before).await;
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let classes: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT write_class FROM field_write ORDER BY 1")
            .fetch_all(store.pool())
            .await
            .expect("the classes read");
    assert_eq!(
        classes,
        ["own"],
        "recorded at {git_ref}: a boot rewrote the class of a row it did not write"
    );
    store.stop().await;
    drop(scratch);
}

/// **Rows for a thread with five active records**, written below the surface into
/// the recorded schema, as a build that let a thread grow without a limit left it.
async fn plant_a_thread_over_the_default_ceiling(pool: &sqlx::MySqlPool) {
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                             archived_reason, archived_at)
         VALUES (?, 'thread', 'A Crowded Line', 'test', NULL, NULL, 'on-demand', '', 'crowd1',
                 NULL, NULL, NULL)",
    )
    .bind(CROWDED_THREAD)
    .execute(pool)
    .await
    .expect("the thread lands");
    for n in 1..=5 {
        sqlx::query(
            "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                               recorded_at, edge_shape, edge_object, event_kind, derived_from,
                               derived_from_id, inserted_at, stale_after, happened_at,
                               happened_through)
             VALUES ('crowd1', ?, ?, NULL, 'inference', 'open', 'active', '2026-10-08', NULL,
                     NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        )
        .bind(format!("f{n}"))
        .bind(format!("crowded record number {n}"))
        .execute(pool)
        .await
        .expect("the thread's record lands");
        sqlx::query(
            "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                    standing, status, recorded_at, edge_shape, edge_object,
                                    derived_from, derived_from_id, inserted_at, stale_after,
                                    written_at, happened_at, happened_through, session)
             VALUES ('crowd1', ?, 1, ?, NULL, 'inference', 'open', 'active', '2026-10-08', NULL,
                     NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL, '2026-10-08T14:02:10Z',
                     NULL, NULL, NULL)",
        )
        .bind(format!("f{n}"))
        .bind(format!("crowded record number {n}"))
        .execute(pool)
        .await
        .expect("the record's first write lands");
    }
}

/// **A store whose threads already hold more than three records boots cleanly and
/// reads whole.** The default ceiling needs no migration and archives nothing:
/// the thread is read and edited as it was, a capture onto it is refused with the
/// routes, and it takes records again once archiving brings it back under.
#[tokio::test]
async fn a_thread_a_deployed_build_left_over_the_default_ceiling_reads_and_edits_as_before() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "thread-over-ceiling").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_thread_over_the_default_ceiling(store.pool()).await;
    store.stop().await;

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

    // ── it reads whole ──────────────────────────────────────────────────────
    let read = surface
        .call(
            "recall",
            json!({"subject": CROWDED_THREAD, "facts": true, "status": "active", "sid": sid}),
        )
        .await;
    for n in 1..=5 {
        if !read.contains(&format!("crowded record number {n}")) {
            fail("the thread reading all five records", &read);
        }
    }

    // ── it edits as before ──────────────────────────────────────────────────
    let edited = surface
        .call(
            "update_fact",
            json!({"address": format!("{CROWDED_THREAD}#f1"),
                   "content": "crowded record number 1, reworded", "provenance": "inference",
                   "sid": sid}),
        )
        .await;
    if edited.contains("\"status\":\"blocked\"") {
        fail("editing a record over the ceiling", &edited);
    }

    // ── a capture is refused with the routes, and writes nothing ────────────
    let refused = parse(
        "a capture onto the crowded thread",
        &surface
            .call(
                "capture",
                json!({"subject": CROWDED_THREAD, "content": "one more record", "sid": sid}),
            )
            .await,
    );
    if refused["status"] != "blocked" || refused["live"] != 5 || refused["routes"].is_null() {
        fail(
            "the capture refused with its count and routes",
            &refused.to_string(),
        );
    }

    // ── archiving back under the ceiling lets it take a record again ────────
    for n in 1..=3 {
        let archived = surface
            .call(
                "update_fact",
                json!({"address": format!("{CROWDED_THREAD}#f{n}"), "status": "archived",
                       "details": "no longer earns its slot", "sid": sid}),
            )
            .await;
        if archived.contains("\"status\":\"blocked\"") {
            fail("archiving a record over the ceiling", &archived);
        }
    }
    let landed = surface
        .call(
            "capture",
            json!({"subject": CROWDED_THREAD, "content": "one more record", "sid": sid}),
        )
        .await;
    booted.stop().await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("a capture once the thread is back under", &landed);
    }
    drop(scratch);
}

/// **A view the way a build before the expression stored it**: a record on the
/// view whose own fields name a key and a value, and no `where` anywhere. Written
/// below the surface into the recorded schema, with the two things it separates.
async fn plant_a_view_with_a_filter_record(pool: &sqlx::MySqlPool) {
    for (id, kind, name, badge) in [
        (OLD_VIEW, "view", "A Recorded View", "view01"),
        (OLD_VIEW_RED, "thing", "A Red Thing", "red001"),
        (OLD_VIEW_BLUE, "thing", "A Blue Thing", "blue01"),
    ] {
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                                 merged_into, archived_reason, archived_at)
             VALUES (?, ?, ?, 'test', NULL, NULL, 'on-demand', '', ?, NULL, NULL, NULL)",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .bind(badge)
        .execute(pool)
        .await
        .expect("the entity lands");
        for fact in ["f1", "f2"] {
            if fact == "f2" && kind != "view" {
                continue;
            }
            sqlx::query(
                "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                                   recorded_at, edge_shape, edge_object, event_kind, derived_from,
                                   derived_from_id, inserted_at, stale_after, happened_at,
                                   happened_through)
                 VALUES (?, ?, 'a recorded claim', NULL, 'testimony', 'settled', 'active',
                         '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                         NULL, NULL)",
            )
            .bind(badge)
            .bind(fact)
            .execute(pool)
            .await
            .expect("the record lands");
            sqlx::query(
                "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                        standing, status, recorded_at, edge_shape, edge_object,
                                        derived_from, derived_from_id, inserted_at, stale_after,
                                        written_at, happened_at, happened_through, session)
                 VALUES (?, ?, 1, 'a recorded claim', NULL, 'testimony', 'settled', 'active',
                         '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                         '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
            )
            .bind(badge)
            .bind(fact)
            .execute(pool)
            .await
            .expect("the record's first write lands");
        }
    }
    for (badge, key, value, fact) in [
        ("view01", "selects", "thing", "f1"),
        ("view01", "key", "color", "f2"),
        ("view01", "value", "red", "f2"),
        ("red001", "color", "red", "f1"),
        ("blue01", "color", "blue", "f1"),
    ] {
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(badge)
        .bind(key)
        .bind(value)
        .bind(fact)
        .execute(pool)
        .await
        .expect("the field lands");
    }
}

/// **A view stored before the expression existed answers as it did.** Its
/// filter is a record whose own fields name a key and a value, and the current
/// binary reads it with no `where` on the view: the red thing is in, the blue one
/// is out, and a second boot over the same store answers the same.
#[tokio::test]
async fn a_view_stored_with_a_filter_record_answers_as_before_on_the_current_binary() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "old-view").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_view_with_a_filter_record(store.pool()).await;
    store.stop().await;

    for boot in 0..2 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
            .await
            .expect("connecting to the current binary");
        let answer = surface
            .call("recall", json!({"view": "upgrade-fixture-view"}))
            .await;
        booted.stop().await;
        let answer: serde_json::Value = serde_json::from_str(&answer).unwrap_or_else(|_| {
            panic!("recorded at {git_ref}: boot {boot}: the view did not answer: {answer}")
        });
        let handles: Vec<&str> = answer["objects"]
            .as_array()
            .map(|all| all.iter().filter_map(|o| o["id"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            handles,
            [OLD_VIEW_RED],
            "recorded at {git_ref}: boot {boot}: the stored filter record still separates red from \
             blue: {answer}"
        );
    }
    drop(scratch);
}

/// **Three claims an older build wrote, each naming a thing by its stored mark**:
/// in plain words, inside an inline code span and inside a fenced block. Before
/// text in code was literal (rule 450), a handle written in a span was resolved
/// like any other, so the row holds the mark there.
///
/// **The column shape is copied, not guessed.** Each insert uses the column list
/// of the `fact` and `fact_write` rows in the recordings (for instance entity
/// `gwd5wp`, claim `f3`, in the `upgrade` dump) and the values
/// `plant_a_view_with_a_filter_record` gives a claim. **Only the content is new.**
/// Neither recording holds a mark in a claim's words, so these three rows are an
/// invention of the shape; the recordings hold the mark only as a field value.
/// The host is the blue thing and the mark is the red thing's badge.
async fn plant_claims_naming_a_thing_by_its_stored_mark(pool: &sqlx::MySqlPool) {
    for (fact, content) in [
        ("f3", "see @#red001 for the plain case"),
        ("f4", "the form is `@#red001` in a span"),
        ("f5", "the form in a block:\n```\n@#red001\n```\nends"),
    ] {
        sqlx::query(
            "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                               recorded_at, edge_shape, edge_object, event_kind, derived_from,
                               derived_from_id, inserted_at, stale_after, happened_at,
                               happened_through)
             VALUES ('blue01', ?, ?, NULL, 'testimony', 'settled', 'active',
                     '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                     NULL, NULL)",
        )
        .bind(fact)
        .bind(content)
        .execute(pool)
        .await
        .expect("the claim lands");
        sqlx::query(
            "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                    standing, status, recorded_at, edge_shape, edge_object,
                                    derived_from, derived_from_id, inserted_at, stale_after,
                                    written_at, happened_at, happened_through, session)
             VALUES ('blue01', ?, 1, ?, NULL, 'testimony', 'settled', 'active',
                     '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                     '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        )
        .bind(fact)
        .bind(content)
        .execute(pool)
        .await
        .expect("the claim's first write lands");
    }
}

/// **A stored mark inside code reads as the handle it answers to, as one outside
/// code does.** The mark is jojobot's own, so the reader resolves it wherever it
/// sits (rule 450 makes a WRITTEN handle in code literal, and nothing rewrites
/// what an older build stored). The boot holds to the gate's bar: no warning, no
/// error line. The plain claim is the positive the two in code depend on.
#[tokio::test]
async fn a_stored_mark_inside_code_reads_as_the_handle_on_the_current_binary() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "mark-in-code").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_view_with_a_filter_record(store.pool()).await;
    plant_claims_naming_a_thing_by_its_stored_mark(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call("recall", json!({"subject": OLD_VIEW_BLUE, "facts": true}))
        .await;
    booted.stop().await;
    let answer: serde_json::Value = serde_json::from_str(&answer)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: the thing did not answer: {answer}"));
    let content_of = |fact: &str| -> String {
        answer["objects"][0]["facts"]
            .as_array()
            .and_then(|all| {
                all.iter().find(|claim| {
                    claim["address"]
                        .as_str()
                        .is_some_and(|address| address.ends_with(&format!("#{fact}")))
                })
            })
            .and_then(|claim| claim["content"].as_str())
            .unwrap_or_else(|| panic!("recorded at {git_ref}: no claim {fact}: {answer}"))
            .to_string()
    };
    for (fact, shape) in [("f3", ""), ("f4", "`"), ("f5", "```")] {
        let content = content_of(fact);
        assert!(
            content.contains(OLD_VIEW_RED),
            "recorded at {git_ref}: claim {fact} reads the mark as the handle: {content}"
        );
        assert!(
            !content.contains("red001"),
            "recorded at {git_ref}: claim {fact} still shows the badge: {content}"
        );
        assert!(
            content.contains(shape),
            "recorded at {git_ref}: claim {fact} keeps its code around the handle: {content}"
        );
    }
    drop(scratch);
}

/// **A link table lost after the lowering finished is rebuilt at the next
/// boot.** The lowering is marked and reads no field write, but the table is a
/// derived index: a boot compares it with the writes cheaply and rebuilds it,
/// with a warning, when they disagree. The pair of the case above, where a
/// healthy store logs no rebuild.
async fn a_lost_link_table_is_rebuilt(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles: _,
        holder_key: _,
    } = restore_a_fixture(fixture_dir, label).await;
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    let first = boot_current_expecting(&state_dir, store_port, &git_ref, Some(reports), &[]).await;
    let first_rebuilt = first
        .log
        .iter()
        .any(|line| line.contains(LINK_TABLE_REBUILT));
    first.stop().await;
    assert!(
        !first_rebuilt,
        "a store that had never been booted by this build and held a consistent table logged a \
         rebuild"
    );

    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be edited");
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_link")
        .fetch_one(editing.pool())
        .await
        .expect("the link rows count");
    assert!(
        held > 0,
        "the recording implies links, or this proves nothing"
    );
    sqlx::query("DELETE FROM field_link")
        .execute(editing.pool())
        .await
        .expect("the link table is emptied");
    editing.stop().await;

    let second = boot_current_expecting(
        &state_dir,
        store_port,
        &git_ref,
        None,
        &[LINK_TABLE_REBUILT],
    )
    .await;
    let rebuilt = second
        .log
        .iter()
        .filter(|line| line.contains(LINK_TABLE_REBUILT))
        .count();
    second.stop().await;
    assert_eq!(rebuilt, 1, "the boot reports the rebuild once");
    let reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let restored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_link")
        .fetch_one(reading.pool())
        .await
        .expect("the link rows count");
    let mut reading = reading;
    reading.stop().await;
    assert_eq!(
        restored, held,
        "the rebuilt table holds the links the writes imply"
    );
    drop(scratch);
}

#[tokio::test]
async fn a_lost_link_table_is_rebuilt_at_the_next_boot_on_the_last_deployed_store() {
    a_lost_link_table_is_rebuilt(&fixture_dir(), "rebuilt-deploy3").await;
}

/// **One claim, and the one field it set, filed under `key`.** For a stored thing
/// the key is its badge; for a handle no row holds it is the handle's own text,
/// which is how a write on a record the build supplies is filed. The column lists
/// are the ones `plant_a_view_with_a_filter_record` copies from the recordings.
async fn plant_a_claim_filed_under(
    pool: &sqlx::MySqlPool,
    key: &str,
    content: &str,
    field: (&str, &str),
) {
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, event_kind, derived_from,
                           derived_from_id, inserted_at, stale_after, happened_at,
                           happened_through)
         VALUES (?, 'f1', ?, NULL, 'testimony', 'settled', 'active',
                 '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                 NULL, NULL)",
    )
    .bind(key)
    .bind(content)
    .execute(pool)
    .await
    .expect("the record lands");
    sqlx::query(
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                standing, status, recorded_at, edge_shape, edge_object,
                                derived_from, derived_from_id, inserted_at, stale_after,
                                written_at, happened_at, happened_through, session)
         VALUES (?, 'f1', 1, ?, NULL, 'testimony', 'settled', 'active',
                 '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .bind(key)
    .bind(content)
    .execute(pool)
    .await
    .expect("the record's first write lands");
    // **The shape `capture` with `sets` writes**: the class `sets`, which is what
    // makes the key a property of the thing, and the moment of the write.
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES (?, ?, 1, ?, 'f1', '2026-10-08T14:02:10Z', 'sets')",
    )
    .bind(key)
    .bind(field.0)
    .bind(field.1)
    .execute(pool)
    .await
    .expect("the field lands");
}

/// **What `recall` answers for one handle, with its claims**, or the answer's own
/// words when it is not an object.
async fn recalled_with_claims(booted: &Booted, handle: &str) -> serde_json::Value {
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call("recall", json!({"subject": handle, "facts": true}))
        .await;
    serde_json::from_str(&answer).unwrap_or_else(|_| panic!("recall of {handle} said: {answer}"))
}

/// The lines of a boot's log that name `handle`.
fn log_lines_naming<'a>(booted: &'a Booted, handle: &str) -> Vec<&'a String> {
    booted
        .log
        .iter()
        .filter(|line| line.contains(handle))
        .collect()
}

/// **A boot on a store that holds a row at a handle the build now supplies does
/// not refuse.** The stored row answers, its claims read back through `recall`,
/// and the boot names the handle in one line that is not a warning. Paired with a
/// handle the build supplies and the store holds no row at, which the boot does
/// not name.
#[tokio::test]
async fn a_store_holding_a_row_at_a_handle_the_build_supplies_boots_and_the_row_answers() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "row-at-supplied").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                             merged_into, archived_reason, archived_at)
         VALUES (?, 'view', 'Stored Loops', 'test', NULL, NULL, 'on-demand', '', 'lpsrow',
                 NULL, NULL, NULL)",
    )
    .bind(SHIPPED_HANDLE_WITH_A_ROW)
    .execute(store.pool())
    .await
    .expect("the stored row lands");
    plant_a_claim_filed_under(
        store.pool(),
        "lpsrow",
        "a claim the operator wrote on their own view",
        ("answer", "kept"),
    )
    .await;
    store.stop().await;

    for boot in 0..2 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let answer = recalled_with_claims(&booted, SHIPPED_HANDLE_WITH_A_ROW).await;
        let object = &answer["objects"][0];
        assert_eq!(
            object["name"], "Stored Loops",
            "recorded at {git_ref}: boot {boot}: the stored row answers at a shipped handle: \
             {answer}"
        );
        assert_eq!(
            object["fields"]["answer"], "kept",
            "recorded at {git_ref}: boot {boot}: its field reads back: {answer}"
        );
        assert!(
            object["facts"].to_string().contains("on their own view"),
            "recorded at {git_ref}: boot {boot}: its claim reads back: {answer}"
        );
        assert_eq!(
            log_lines_naming(&booted, SHIPPED_HANDLE_WITH_A_ROW).len(),
            1,
            "recorded at {git_ref}: boot {boot}: the boot names the handle once: {:?}",
            booted.log
        );
        assert!(
            log_lines_naming(&booted, SHIPPED_HANDLE_WITHOUT_ONE).is_empty(),
            "recorded at {git_ref}: boot {boot}: a supplied handle with no row is not named"
        );
        booted.stop().await;
    }
    drop(scratch);
}

/// **What an earlier boot of a build that supplied `handle` left in the store**:
/// the record of supplied handles holds it. A store restored from before the
/// record existed has no table yet, so the schema runs first, as the boot's own
/// run would.
async fn record_as_supplied(pool: &sqlx::MySqlPool, handle: &str) {
    jojobot_adapters::dolt::migrate::run(pool)
        .await
        .expect("the schema runs");
    sqlx::query("INSERT INTO supplied_handle (handle) VALUES (?)")
        .bind(handle)
        .execute(pool)
        .await
        .expect("the handle is recorded");
}

/// **What the operator wrote on a handle the build stopped supplying reads back
/// as an ordinary stored thing.** The writes sit filed under the handle's own
/// text with no row. The first boot gives the handle a row, takes the kind from
/// the handle and the name from its slug, and files the claim and the field under
/// the row's badge, so `recall` answers as for any thing and the next boot has
/// nothing left to do. Paired with a handle the build still supplies, which keeps
/// its writes where they are, is given no row, and is not named.
#[tokio::test]
async fn what_the_operator_wrote_on_a_handle_the_build_stopped_supplying_reads_back_as_a_thing() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "retired-handle").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_claim_filed_under(
        store.pool(),
        RETIRED_HANDLE,
        "a claim written while the build supplied this view",
        ("answer", "kept"),
    )
    .await;
    record_as_supplied(store.pool(), RETIRED_HANDLE).await;
    // The pair: an overlay on a handle the build still supplies, filed the same way.
    plant_a_claim_filed_under(
        store.pool(),
        SHIPPED_HANDLE_WITHOUT_ONE,
        "a claim on a view the build still supplies",
        ("answer", "still on top"),
    )
    .await;
    // The second pair: writes under a handle no build supplied.
    plant_a_claim_filed_under(
        store.pool(),
        HAND_REMOVED_HANDLE,
        "a claim under a handle nobody supplied",
        ("answer", "left"),
    )
    .await;
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let retired = recalled_with_claims(&first, RETIRED_HANDLE).await;
    let object = &retired["objects"][0];
    assert_eq!(
        object["id"], RETIRED_HANDLE,
        "recorded at {git_ref}: the handle answers as a thing: {retired}"
    );
    assert_eq!(
        object["name"], "upgrade-fixture-retired",
        "recorded at {git_ref}: the name is the handle's slug: {retired}"
    );
    assert_eq!(
        object["fields"]["answer"], "kept",
        "recorded at {git_ref}: the field reads back: {retired}"
    );
    assert!(
        object["facts"]
            .to_string()
            .contains("while the build supplied"),
        "recorded at {git_ref}: the claim reads back: {retired}"
    );
    let shipped = recalled_with_claims(&first, SHIPPED_HANDLE_WITHOUT_ONE).await;
    assert_eq!(
        shipped["objects"][0]["fields"]["answer"], "still on top",
        "recorded at {git_ref}: a handle still supplied keeps its overlay: {shipped}"
    );
    assert_eq!(
        log_lines_naming(&first, RETIRED_HANDLE).len(),
        1,
        "recorded at {git_ref}: the first boot names the handle it gave a row: {:?}",
        first.log
    );
    assert!(
        log_lines_naming(&first, SHIPPED_HANDLE_WITHOUT_ONE).is_empty(),
        "recorded at {git_ref}: a handle still supplied is not named"
    );
    // **Quiet is no warning and no error.** The storage check reports the claim
    // filed under the handle no build supplied at info level, with its address,
    // so the information reaches the log and the boot stays quiet.
    let named = log_lines_naming(&first, HAND_REMOVED_HANDLE);
    assert!(
        named
            .iter()
            .all(|line| !line.contains("WARN") && !line.contains("ERROR")),
        "recorded at {git_ref}: a handle no build supplied is named above info: {named:?}"
    );
    assert!(
        named.iter().any(|line| line.contains("storage check:")),
        "recorded at {git_ref}: the storage check reports the handle: {:?}",
        first.log
    );
    // The recording itself holds no handle to adopt: the one line that names a
    // handle is the planted one's.
    assert_eq!(
        first
            .log
            .iter()
            .filter(|line| line.contains("handle="))
            .count(),
        1,
        "recorded at {git_ref}: the boot gave a row to the planted handle and to no other: {:?}",
        first.log
    );
    first.stop().await;

    // The store, read with no binary over it.
    let mut store = open_the_store(&state_dir, store_port).await;
    let (kind, badge): (String, Option<String>) =
        sqlx::query_as("SELECT kind, badge FROM entity WHERE id = ?")
            .bind(RETIRED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("the handle has a row now");
    assert_eq!(
        kind, "view",
        "recorded at {git_ref}: the kind is the handle's"
    );
    let badge = badge.expect("recorded at {git_ref}: the row wears a badge like any other");
    let filed_under_the_badge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ?")
            .bind(&badge)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    let filed_under_the_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ?")
            .bind(RETIRED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        (filed_under_the_badge, filed_under_the_handle),
        (1, 0),
        "recorded at {git_ref}: the claim moved onto the badge"
    );
    let rows_at_the_supplied_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
            .bind(SHIPPED_HANDLE_WITHOUT_ONE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        rows_at_the_supplied_handle, 0,
        "recorded at {git_ref}: a handle the build supplies is given no row"
    );
    let rows_at_the_hand_removed_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
            .bind(HAND_REMOVED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        rows_at_the_hand_removed_handle, 0,
        "recorded at {git_ref}: a handle no build supplied is given no row"
    );
    // The boot recorded what the build supplies, so a later build that stops
    // supplying one finds it there.
    let recorded: Vec<String> = sqlx::query_scalar("SELECT handle FROM supplied_handle")
        .fetch_all(store.pool())
        .await
        .expect("the record reads");
    for shipped in [SHIPPED_HANDLE_WITH_A_ROW, SHIPPED_HANDLE_WITHOUT_ONE] {
        assert!(
            recorded.iter().any(|handle| handle == shipped),
            "recorded at {git_ref}: the boot did not record {shipped}: {recorded:?}"
        );
    }
    store.stop().await;

    let second = boot_current(&state_dir, store_port, &git_ref).await;
    assert!(
        log_lines_naming(&second, RETIRED_HANDLE).is_empty(),
        "recorded at {git_ref}: the second boot has nothing left to give a row: {:?}",
        second.log
    );
    let again = recalled_with_claims(&second, RETIRED_HANDLE).await;
    assert_eq!(
        again["objects"][0]["fields"]["answer"], "kept",
        "recorded at {git_ref}: the thing still answers on the second boot: {again}"
    );
    second.stop().await;
    drop(scratch);
}

/// **Everything that names a handle the build stopped supplying follows the
/// handle's new row.** While the build supplied the handle, another thing's
/// claims drew an edge at it, a field value named it, and marks tied claims to
/// its claims, all filed under the handle's own text because a supplied record
/// has no badge. The first boot gives the handle a row and moves each of those
/// onto the row's badge, so none rests on a name that a rename frees. A walk
/// out of the drawing thing reaches the adopted thing, the field serves its
/// handle, and the stored columns hold the badge. The gate also fails the
/// case if the adopting boot logs a warning or an error.
#[tokio::test]
async fn everything_naming_a_handle_the_build_stopped_supplying_follows_the_row_the_boot_gave_it() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "retired-edge").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    record_as_supplied(store.pool(), RETIRED_HANDLE).await;
    plant_a_claim_filed_under(
        store.pool(),
        RETIRED_HANDLE,
        "a claim written while the build supplied this view",
        ("answer", "kept"),
    )
    .await;
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                             merged_into, archived_reason, archived_at)
         VALUES (?, 'thing', 'Drawing', 'test', NULL, NULL, 'on-demand', '', 'drwrow',
                 NULL, NULL, NULL)",
    )
    .bind(DRAWING_HANDLE)
    .execute(store.pool())
    .await
    .expect("the drawing thing lands");
    plant_a_claim_filed_under(
        store.pool(),
        "drwrow",
        "a claim that draws an edge at the retired view",
        ("drawn", "yes"),
    )
    .await;
    // The edge, as the deployed build filed it: the handle's own text.
    for table in ["fact", "fact_write"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET edge_shape = 'about', edge_object = ? WHERE entity = 'drwrow'"
        )))
        .bind(RETIRED_HANDLE)
        .execute(store.pool())
        .await
        .expect("the edge lands");
    }
    // The other columns that name the handle, as the deployed build filed them:
    // a field on the drawing thing naming the retired view (value behind the
    // stored-handle mark, and its link row), a field on the retired view naming
    // the drawing thing (and its link row, keyed by the view's own text), and a
    // mark that ties a claim of each to a claim of the other.
    for statement in [
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES ('drwrow', 'watches', 1, '@#view:upgrade-fixture-retired', 'f1',
                 '2026-10-08T14:02:10Z', 'sets')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
         VALUES ('drwrow', 'watches', 1, 'view:upgrade-fixture-retired')",
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES ('view:upgrade-fixture-retired', 'holds', 1, '@#drwrow', 'f1',
                 '2026-10-08T14:02:10Z', 'sets')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
         VALUES ('view:upgrade-fixture-retired', 'holds', 1, 'drwrow')",
        "INSERT INTO fact_stands_for (fact_home, fact_id, ordinal, source_home, source_id)
         VALUES ('drwrow', 'f1', 1, 'view:upgrade-fixture-retired', 'f1')",
    ] {
        sqlx::query(statement)
            .execute(store.pool())
            .await
            .expect("the column lands");
    }
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call(
            "recall",
            json!({"subject": DRAWING_HANDLE, "facts": true, "follow": {"depth": 1}}),
        )
        .await;
    let walked: serde_json::Value =
        serde_json::from_str(&answer).unwrap_or_else(|_| panic!("recall said: {answer}"));
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .and_then(|connected| connected.iter().find(|o| o["id"] == RETIRED_HANDLE))
        .unwrap_or_else(|| panic!("recorded at {git_ref}: the walk does not reach it: {walked}"));
    assert_eq!(
        reached["fields"]["answer"], "kept",
        "recorded at {git_ref}: the adopted thing carries the write: {reached}"
    );
    assert_eq!(
        walked["objects"][0]["fields"]["watches"], RETIRED_HANDLE,
        "recorded at {git_ref}: the field naming the adopted thing serves its handle: {walked}"
    );
    first.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(RETIRED_HANDLE)
        .fetch_one(store.pool())
        .await
        .expect("the handle has a row now");
    let edge: String = sqlx::query_scalar("SELECT edge_object FROM fact WHERE entity = 'drwrow'")
        .fetch_one(store.pool())
        .await
        .expect("the edge reads");
    assert_eq!(
        edge, badge,
        "recorded at {git_ref}: the edge still holds the handle's text, not the badge"
    );
    for (table, column) in [
        ("field_link", "entity"),
        ("field_link", "target"),
        ("fact_stands_for", "source_home"),
    ] {
        let held: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE {column} = ?"
        )))
        .bind(RETIRED_HANDLE)
        .fetch_one(store.pool())
        .await
        .expect("counted");
        assert_eq!(
            held, 0,
            "recorded at {git_ref}: {table}.{column} still holds the handle's text"
        );
    }
    let value: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = 'drwrow' AND `key` = 'watches'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the field value reads");
    assert_eq!(
        value,
        format!("@#{badge}"),
        "recorded at {git_ref}: the field value still holds the handle's text"
    );
    store.stop().await;
    drop(scratch);
}
