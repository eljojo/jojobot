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
use jojobot_adapters::claims_log::codec;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{Dolt, OLD_ROLE_CLAIMS_PASS, UNREADABLE_DUE_PASS, migrate};
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

/// Every claims event the log holds, as stored.
async fn logged(pool: &sqlx::MySqlPool) -> Vec<ClaimEvent> {
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(pool)
    .await
    .expect("the log is readable");
    let mut events = Vec::new();
    for payload in payloads {
        let batch: serde_json::Value = serde_json::from_slice(&payload).expect("a stored batch");
        for member in batch[5].as_array().expect("a batch holds its members") {
            let bytes: Vec<u8> = member
                .as_array()
                .expect("a member is bytes")
                .iter()
                .map(|byte| byte.as_u64().expect("a byte") as u8)
                .collect();
            if let Some(event) = codec::decode_own(&bytes).expect("every stored event reads") {
                events.push(event.payload);
            }
        }
    }
    events
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
fn boot(state: &Path, port: u16) {
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
