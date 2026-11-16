//! **A repair pass owed on a boot after the claims import writes rows an event
//! carries.** On a store's first boot the import takes in whatever the passes wrote.
//! On a later boot the import is already done, so a pass that runs then (one that
//! failed or was skipped before, or one a later build adds) writes through the store
//! and no event says so: the rows hold the repair and the log, which the State is
//! built from, does not.
//!
//! **Each case boots the real binary twice over one store.** The first boot imports
//! and stops in order. Then the pass is marked owed again, which is the state a pass
//! that failed, or was skipped, leaves, and the second boot runs it. The rows are
//! then read for what the pass wrote, and the log for the event that carries it.
//!
//! **What carries it is a write of the key the pass wrote**, counted as a
//! `FieldWritten` event of that key, and not a search of the log's text: the shipped
//! type declarations name keys such as `due_on` in the log whatever a pass wrote.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{Dolt, OLD_ROLE_CLAIMS_PASS, UNREADABLE_DUE_PASS, migrate};
use jojobot_adapters::testing::{Scratch, claims_events, free_port};
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

/// Every claims event the log holds, as stored.
async fn logged(pool: &sqlx::MySqlPool) -> Vec<ClaimEvent> {
    claims_events(pool)
        .await
        .into_iter()
        .map(|event| event.payload)
        .collect()
}

/// How many events write `key` to some thing.
fn writes_of(events: &[ClaimEvent], key: &str) -> usize {
    events
        .iter()
        .filter(
            |event| matches!(event, ClaimEvent::FieldWritten { key: written, .. } if written.0 == key),
        )
        .count()
}

/// **One boot of the real binary over the store, which serves and stops in order.**
/// Gives back the lines it logged up to its serving line.
fn boot(state: &Path, port: u16) -> Vec<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", state)
        .env("JOJOBOT_STORE_PORT", port.to_string())
        .env("JOJOBOT_BIND", "127.0.0.1:0")
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");
    let mut log = BufReader::new(child.stdout.take().expect("stdout was piped"));
    let (said, lines) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        while matches!(log.read_line(&mut line), Ok(n) if n > 0) {
            if said.send(line.trim_end().to_string()).is_err() {
                return;
            }
            line.clear();
        }
    });
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    let served = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) => {
                let served = line.contains("serving http://");
                seen.push(line);
                if served {
                    break true;
                }
            }
            Err(_) => break false,
        }
    };
    if !served {
        let _ = child.kill();
        let _ = child.wait();
        panic!("a boot never served:\n{}", seen.join("\n"));
    }
    let stopped = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success(), "the stop signal was not delivered");
    let ended = Instant::now() + Duration::from_secs(60);
    let status = loop {
        match child.try_wait().expect("the child can be polled") {
            Some(status) => break status,
            None if Instant::now() > ended => {
                let _ = child.kill();
                panic!("a boot did not stop in order");
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    assert_eq!(status.code(), Some(0), "the stop was not orderly");
    seen
}

/// **A store as an earlier build left it**, seeded through the bare store, with the
/// named pass marked finished so the first boot imports what it would repair as it is.
async fn seeded(path: &Path, port: u16, pass: &str, plant: impl AsyncFnOnce(&DoltMemory)) {
    let mut seeding = Dolt::start(path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let rows = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&rows)
        .await
        .expect("the kinds are seeded");
    plant(&rows).await;
    sqlx::query("INSERT INTO store_pass (name, finished_at) VALUES (?, '2026-09-01T00:00:00Z')")
        .bind(pass)
        .execute(seeding.pool())
        .await
        .expect("the pass is marked finished");
    seeding.stop().await;
}

/// **The pass is owed again after the import**, as one that failed or was skipped on
/// the import's boot would be, and the count of events that write `key` at that point.
async fn owed_again(path: &Path, port: u16, pass: &str, key: &str) -> usize {
    let mut store = Dolt::start(path, port).await.expect("the store reopens");
    let carried = writes_of(&logged(store.pool()).await, key);
    sqlx::query("DELETE FROM store_pass WHERE name = ?")
        .bind(pass)
        .execute(store.pool())
        .await
        .expect("the pass is owed");
    store.stop().await;
    carried
}

async fn thing(rows: &DoltMemory, handle: &str, parent: Option<&str>) -> EntityId {
    let id = EntityId(handle.into());
    rows.add_entity(NewEntity {
        parent: parent.map(|p| EntityId(p.into())),
        ..NewEntity::new(id.clone(), handle, "user-named")
    })
    .await
    .expect("add ok")
    .written()
    .expect("not blocked");
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_due_day_stamped_by_a_pass_owed_after_the_import_is_carried_by_an_event() {
    let state = jojobot_adapters::testing::scratch_path("jojobot-due-pass-after-import");
    let path = state.join("db");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(state.clone());
    let port = free_port();

    // A work item whose schedule cannot be read and that holds no due day.
    seeded(&path, port, UNREADABLE_DUE_PASS, async |rows| {
        thing(rows, "project:atlas", None).await;
        let sigma = thing(rows, "work:sigma", Some("project:atlas")).await;
        rows.capture(memory::state(NewFact {
            fields: [
                ("cadence_days", "often"),
                ("advances_from", "due_date"),
                ("counts_from", "2026-06-01"),
                ("status", "closed"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            ..NewFact::about(sigma, "recorded as it was", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok");
    })
    .await;
    boot(&state, port);
    let carried_before = owed_again(&path, port, UNREADABLE_DUE_PASS, "due_on").await;
    assert_eq!(
        carried_before, 0,
        "the import carried no due day: the item was unstamped when it ran"
    );
    boot(&state, port);

    let mut store = Dolt::start(&path, port).await.expect("the store reopens");
    let in_rows: Vec<Option<String>> =
        sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'due_on'")
            .fetch_all(store.pool())
            .await
            .expect("the rows read");
    let events = logged(store.pool()).await;
    store.stop().await;
    assert!(
        !in_rows.is_empty(),
        "the pass wrote no stamp to the rows, so the case holds no repair to carry"
    );
    assert!(
        writes_of(&events, "due_on") > 0,
        "the rows hold the stamp {in_rows:?} that the pass wrote after the import, and no event \
         writes a due day ({} events in the log), so the State serves the thing without it",
        events.len()
    );
    drop(scratch);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_role_claim_moved_by_a_pass_owed_after_the_import_is_carried_by_an_event() {
    let state = jojobot_adapters::testing::scratch_path("jojobot-role-pass-after-import");
    let path = state.join("db");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(state.clone());
    let port = free_port();

    // A bot that holds a role the way an earlier build kept it: two keys on the bot.
    seeded(&path, port, OLD_ROLE_CLAIMS_PASS, async |rows| {
        let sigma = thing(rows, "bot:sigma", None).await;
        rows.capture(memory::state(NewFact {
            fields: [
                ("role/dev-alpha/holder", "session-alpha"),
                ("role/dev-alpha/claimed_at", "2026-09-01T00:00:00Z"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            ..NewFact::about(sigma, "held a role", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok");
    })
    .await;
    boot(&state, port);
    let carried_before = owed_again(&path, port, OLD_ROLE_CLAIMS_PASS, "claimed_at").await;
    assert_eq!(
        carried_before, 0,
        "the import carried no claim moment on a role object: the claim was on the bot"
    );
    boot(&state, port);

    let mut store = Dolt::start(&path, port).await.expect("the store reopens");
    let on_the_role: Vec<Option<String>> =
        sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'claimed_at'")
            .fetch_all(store.pool())
            .await
            .expect("the rows read");
    let events = logged(store.pool()).await;
    store.stop().await;
    assert!(
        !on_the_role.is_empty(),
        "the pass moved no claim onto a role object, so the case holds no repair to carry"
    );
    assert!(
        writes_of(&events, "claimed_at") > 0,
        "the rows hold the claim {on_the_role:?} that the pass moved after the import, and no \
         event writes a claim moment ({} events in the log), so the State holds no such role",
        events.len()
    );
    drop(scratch);
}

/// The stored names of the one-time passes that rewrite claims rows of the linking
/// store. Written out here: nothing outside this process declares the spelling, so a
/// rename in the code must fail a case and not follow silently.
const FIELD_MARKS_PASS: &str = "field value links marked";
const FORWARDED_LINKS_PASS: &str = "forwarded field links repaired";

/// The permanent key a thing is stored under: its badge, else its handle.
async fn key_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing is stored")
}

/// The value of the one `spouse` write the case holds.
async fn the_spouse_value(path: &Path, port: u16) -> String {
    let mut store = Dolt::start(path, port).await.expect("the store reopens");
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'spouse'")
            .fetch_one(store.pool())
            .await
            .expect("the write reads");
    store.stop().await;
    value.expect("the write holds a value")
}

/// **A link row no mark implies is left alone on a boot after the import.** The
/// link table is checked on every boot, and a disagreement reruns the reconcile,
/// which marks the write by SQL. After the import the log is the claims' truth and a
/// rewrite of `field_write.value` that no event carries makes the rows and the log
/// disagree, with nothing to reconcile them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_link_row_with_no_mark_is_not_marked_by_a_boot_after_the_import() {
    let state = jojobot_adapters::testing::scratch_path("jojobot-link-row-after-import");
    let path = state.join("db");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(state.clone());
    let port = free_port();

    seeded(&path, port, FIELD_MARKS_PASS, async |rows| {
        let homer = thing(rows, "person:homer", None).await;
        thing(rows, "person:marge", None).await;
        rows.capture(memory::state(NewFact {
            fields: [("spouse", "person:marge")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..NewFact::about(homer, "married", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok");
    })
    .await;
    boot(&state, port);

    // The write holds plain text, and a link row that no mark implies stands for it:
    // what a store written while the rule was off holds.
    let mut store = Dolt::start(&path, port).await.expect("the store reopens");
    sqlx::query("UPDATE field_write SET value = 'person:marge' WHERE `key` = 'spouse'")
        .execute(store.pool())
        .await
        .expect("the write is plain text");
    let marge = key_of(store.pool(), "person:marge").await;
    sqlx::query("DELETE FROM field_link WHERE `key` = 'spouse'")
        .execute(store.pool())
        .await
        .expect("the link rows are cleared");
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target) \
         SELECT entity, `key`, ordinal, ? FROM field_write WHERE `key` = 'spouse'",
    )
    .bind(&marge)
    .execute(store.pool())
    .await
    .expect("the link row is planted");
    store.stop().await;

    boot(&state, port);

    assert_eq!(
        the_spouse_value(&path, port).await,
        "person:marge",
        "a boot after the import marked a write by SQL, and no event carries it"
    );
    drop(scratch);
}

/// **A forward that became placeable is not repointed by a boot after the import**,
/// and the boot names the pass owed at error level, which the upgrade gate fails on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_forward_made_placeable_is_not_repointed_by_a_boot_after_the_import() {
    let state = jojobot_adapters::testing::scratch_path("jojobot-forward-after-import");
    let path = state.join("db");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(state.clone());
    let port = free_port();

    seeded(&path, port, FORWARDED_LINKS_PASS, async |rows| {
        let homer = thing(rows, "person:homer", None).await;
        let marge = thing(rows, "person:marge", None).await;
        let lisa = thing(rows, "person:lisa", None).await;
        rows.capture(memory::state(NewFact {
            fields: [("spouse", "person:marge")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..NewFact::about(homer, "married", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok");
        rows.merge(
            &marge,
            &lisa,
            Some("the same person"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the merge lands");
    })
    .await;
    boot(&state, port);

    // The write names the folded thing, as a fold of an older build left it, and the
    // pass is owed again.
    let mut store = Dolt::start(&path, port).await.expect("the store reopens");
    let marge = key_of(store.pool(), "person:marge").await;
    let on_the_husk = jojobot_domain::memory::marked_stored_handle(&marge);
    sqlx::query("UPDATE field_write SET value = ? WHERE `key` = 'spouse'")
        .bind(&on_the_husk)
        .execute(store.pool())
        .await
        .expect("the write names the folded thing");
    sqlx::query("UPDATE field_link SET target = ? WHERE `key` = 'spouse'")
        .bind(&marge)
        .execute(store.pool())
        .await
        .expect("the link names the folded thing");
    sqlx::query("DELETE FROM store_pass WHERE name = ?")
        .bind(FORWARDED_LINKS_PASS)
        .execute(store.pool())
        .await
        .expect("the pass is owed");
    store.stop().await;

    let lines = boot(&state, port);

    assert_eq!(
        the_spouse_value(&path, port).await,
        on_the_husk,
        "a boot after the import repointed a link by SQL, and no event carries it"
    );
    assert!(
        lines.iter().any(|line| line.contains("ERROR")
            && line.contains("CLAIMS-ROW PASS OWED AFTER THE IMPORT")
            && line.contains(FORWARDED_LINKS_PASS)),
        "the boot names the pass owed at error level, which the upgrade gate fails on: {lines:#?}"
    );
    drop(scratch);
}
