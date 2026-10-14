//! **A request id that a write spent is still spent after the server restarts.**
//!
//! The ledger is a table in the store, so it outlives the process. These cases
//! start the real binary over a real store, send a write under an id, stop the
//! binary, start it again over the same store, and send the write again.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::testing::free_port;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **One run of the real binary over the store in `state_dir`**, booted as the
/// assistant. Hands back the connection and the run's key; the binary stops
/// when the handle is dropped.
async fn running(
    state_dir: &std::path::Path,
    store_port: u16,
) -> (
    tokio::process::Child,
    jojobot_exercise::surface::Surface,
    String,
) {
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let served = tokio::time::timeout(Duration::from_secs(90), async {
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
    let mut booted = parse(
        &surface
            .call("start_here", json!({"bot": "assistant", "brief": true}))
            .await,
    );
    if booted
        .pointer("/session/sid")
        .and_then(Value::as_str)
        .is_none()
    {
        // An earlier run of the assistant is on the board: this boot starts its own.
        booted = parse(
            &surface
                .call(
                    "start_here",
                    json!({"bot": "assistant", "brief": true, "resume": "new"}),
                )
                .await,
        );
    }
    let sid = booted
        .pointer("/session/sid")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a boot hands back a sid: {booted}"))
        .to_string();
    (child, surface, sid)
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("an answer in JSON ({e}): {text}"))
}

async fn stop(mut child: tokio::process::Child, surface: jojobot_exercise::surface::Surface) {
    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;
}

#[tokio::test]
async fn a_write_sent_again_after_a_restart_returns_the_first_receipt_and_stores_nothing() {
    let state_dir =
        std::env::temp_dir().join(format!("jojobot-request-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let store_port = free_port();

    // ── the first process: the thing is made and the claim is written ───────
    let (child, surface, sid) = running(&state_dir, store_port).await;
    let made = parse(
        &surface
            .call(
                "add_entity",
                json!({
                    "kind": "person", "handle": "milhouse", "name": "Milhouse",
                    "source": "user-named", "sid": sid,
                }),
            )
            .await,
    );
    assert_ne!(made["status"], "blocked", "{made}");
    let write = |sid: &str| {
        json!({
            "subject": "person:milhouse", "content": "wears glasses",
            "provenance": "testimony", "request_id": "r1", "sid": sid,
        })
    };
    let first = parse(&surface.call("capture", write(&sid)).await);
    assert_ne!(first["status"], "blocked", "{first}");
    assert!(first.get("repeat").is_none(), "{first}");
    stop(child, surface).await;

    // ── the second process, over the same store ─────────────────────────────
    let (child, surface, sid) = running(&state_dir, store_port).await;
    let again = parse(&surface.call("capture", write(&sid)).await);
    assert_eq!(again["repeat"], true, "the id is still spent: {again}");
    assert_eq!(
        again["address"], first["address"],
        "the first receipt comes back: {again}"
    );
    let held = parse(
        &surface
            .call(
                "recall",
                json!({"subject": "person:milhouse", "facts": true, "sid": sid}),
            )
            .await,
    );
    assert_eq!(
        held["objects"][0]["facts"].as_array().map(Vec::len),
        Some(1),
        "one claim stands: {held}"
    );
    stop(child, surface).await;
}

/// **A process that stopped between the commit and the store of the receipt.**
/// The row is there and the receipt is not. The repeat says the write landed, and
/// gives what the write returned, and stores nothing.
#[tokio::test]
async fn a_repeat_of_a_write_whose_receipt_was_never_kept_says_it_landed_and_gives_its_address() {
    let state_dir =
        std::env::temp_dir().join(format!("jojobot-request-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let store_port = free_port();

    let (child, surface, sid) = running(&state_dir, store_port).await;
    surface
        .call(
            "add_entity",
            json!({
                "kind": "person", "handle": "milhouse", "name": "Milhouse",
                "source": "user-named", "sid": sid,
            }),
        )
        .await;
    let write = |sid: &str| {
        json!({
            "subject": "person:milhouse", "content": "wears glasses",
            "provenance": "testimony", "request_id": "r1", "sid": sid,
        })
    };
    let first = parse(&surface.call("capture", write(&sid)).await);
    stop(child, surface).await;

    // The receipt is taken out of the stored row, as a stop in that window leaves it.
    let mut store = jojobot_adapters::dolt::Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the store comes up");
    let taken = sqlx::query("UPDATE request_receipt SET receipt = NULL")
        .execute(store.pool())
        .await
        .expect("the row is readable")
        .rows_affected();
    assert_eq!(taken, 1, "one row held a receipt");
    store.stop().await;

    let (child, surface, sid) = running(&state_dir, store_port).await;
    let again = parse(&surface.call("capture", write(&sid)).await);
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["status"], "landed", "{again}");
    assert_eq!(again["receipt_kept"], false, "{again}");
    assert_eq!(
        again["landed"]["address"], first["address"],
        "the answer gives the address the write returned: {again}"
    );
    let held = parse(
        &surface
            .call(
                "recall",
                json!({"subject": "person:milhouse", "facts": true, "sid": sid}),
            )
            .await,
    );
    assert_eq!(
        held["objects"][0]["facts"].as_array().map(Vec::len),
        Some(1),
        "one claim stands: {held}"
    );
    stop(child, surface).await;
}
