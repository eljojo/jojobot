//! **A server acting out a day continues after the stamps its store already
//! holds.**
//!
//! A stated clock counts from its day's midnight in every process. A process
//! started over a store that an earlier process filled would stamp its first
//! writes before that process's last ones, and a reader that orders by stamp
//! would put the new mail ahead of the old. These cases start the real binary on
//! the stated day, over a store seeded by another process, and read the stamp of
//! a write it makes.

use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jiff::Timestamp;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewSession, Sessions, Sid};
use tokio::io::AsyncBufReadExt;

const DAY: &str = "2026-06-01";

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **The stamp of a journal beat written by the real binary acting out
/// [`DAY`]**, over a store whose only row, when `seeded_at` is given, is a run
/// begun at that moment by an earlier process.
async fn stamp_of_a_beat_after(seeded_at: Option<Timestamp>) -> Timestamp {
    // One directory per call: two seeded cases run side by side in this process.
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-continued-clock-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let store_port = free_port();

    if let Some(started_at) = seeded_at {
        // The exact path `wiring::resolve_store_dir` derives from `STATE_DIRECTORY`.
        let mut seeding = Dolt::start(&state_dir.join("db"), store_port)
            .await
            .expect("the store comes up");
        migrate::run(seeding.pool())
            .await
            .expect("the schema applies");
        DoltSessions::open(seeding.pool().clone())
            .begin(NewSession {
                bot: EntityId("bot:marge".into()),
                sid: Sid("hmr1".into()),
                focus: "working".into(),
                started_at,
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the earlier process's run lands");
        seeding.stop().await;
    }

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("JOJOBOT_TODAY", DAY)
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let served = tokio::time::timeout(Duration::from_secs(60), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains("serving http://") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(served, "the binary never reached its serving line");

    let surface =
        jojobot_exercise::surface::Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
            .await
            .expect("connecting to the real binary");
    let booted: serde_json::Value = serde_json::from_str(
        &surface
            .call("start_here", serde_json::json!({"bot": "assistant"}))
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = booted
        .pointer("/session/sid")
        .and_then(|sid| sid.as_str())
        .expect("a boot with nothing to resume hands back a sid")
        .to_string();
    let beat: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "journal",
                serde_json::json!({"sid": sid, "entry": "found the door"}),
            )
            .await,
    )
    .expect("the journal answers in JSON");
    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;

    beat.pointer("/entry/at")
        .and_then(|at| at.as_str())
        .unwrap_or_else(|| panic!("the receipt carries the beat's stamp: {beat}"))
        .parse()
        .expect("the stamp is an instant")
}

/// **The positive.** The earlier process left a run begun at eleven at night on
/// the stated day. A beat the new process writes is stamped after it, though the
/// new process has run for a minute and not for twenty-three hours.
#[tokio::test]
async fn a_beat_written_over_a_store_another_process_filled_is_stamped_after_what_it_holds() {
    let seeded: Timestamp = format!("{DAY}T23:00:00.5Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(Some(seeded)).await;

    assert!(
        stamp > seeded,
        "the beat is stamped {stamp}, which is not after the run the store already held, {seeded}"
    );
}

/// **A stamp after the stated day counts as well.** The earlier process left a
/// run begun at one in the morning on the day after [`DAY`]. A beat the new
/// process writes is stamped after it, though the stated day is over a day behind
/// it.
#[tokio::test]
async fn a_beat_written_over_a_store_holding_a_stamp_on_the_next_day_is_stamped_after_it() {
    let seeded: Timestamp = "2026-06-02T01:00:00.5Z".parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(Some(seeded)).await;

    assert!(
        stamp > seeded,
        "the beat is stamped {stamp}, which is not after the run the store already held, {seeded}"
    );
}

/// **The control.** With nothing stamped in the store, the clock starts at its
/// day's midnight as it always did. A clock that began late for every store
/// would pass the case above for the wrong reason.
#[tokio::test]
async fn a_beat_written_over_a_store_with_no_stamp_on_the_day_is_stamped_at_midnight() {
    let midnight: Timestamp = format!("{DAY}T00:00:00Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(None).await;

    assert!(
        stamp >= midnight && stamp.duration_since(midnight).as_secs() < 600,
        "the beat is stamped {stamp}, which is not within ten minutes of {midnight}"
    );
}
