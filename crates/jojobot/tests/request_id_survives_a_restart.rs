//! **A request id that a write spent is still spent after the server restarts.**
//!
//! The ledger is a table in the store, so it outlives the process. These cases
//! start the real binary over a real store, send a write under an id, stop the
//! binary, start it again over the same store, and send the write again.

use std::process::Stdio;

use jojobot_adapters::testing::free_port;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;

use jojobot_adapters::testing::Scratch;

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
    let boot_began = std::time::Instant::now();
    let served = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains("serving http://") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        served,
        "the binary never reached its serving line after {:?} (a boot is given {:?})",
        boot_began.elapsed(),
        jojobot_exercise::room::READY_TIMEOUT
    );
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
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-request-restart");
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
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-request-crash");
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

/// **A write sent again under its id, after the first committed, answers as a
/// repeat before any guard of the verb runs — on the real binary and the real
/// store.** Each second call here would be refused by a guard if it ran: a
/// creation of a handle that exists, an archive of a thing already archived, a
/// restore of one not archived, a merge of a thing already folded, a retraction of
/// a claim already taken back. The id is looked up once, at the dispatch, ahead of
/// every verb, so none of the guards is asked.
///
/// **Paired with each first call**, which landed and carries no `repeat`: a
/// dispatch that answered every second call as a repeat without the first having
/// written would pass the repeats alone.
#[tokio::test]
async fn a_write_sent_again_under_its_id_answers_as_a_repeat_before_its_guards_run() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-request-order");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let (child, surface, sid) = running(&state_dir, free_port()).await;

    let send = |tool: &'static str, mut args: Value| {
        let surface = &surface;
        let sid = sid.clone();
        async move {
            args["sid"] = sid.into();
            parse(&surface.call(tool, args).await)
        }
    };
    for (kind, slug, name) in [
        ("person", "krusty", "Krusty"),
        ("person", "skinner", "Skinner"),
        ("person", "apu", "Apu"),
    ] {
        let made = send(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named"}),
        )
        .await;
        assert_ne!(made["status"], "blocked", "{made}");
    }
    let claim = send(
        "capture",
        json!({"subject": "person:apu", "content": "runs the store", "provenance": "testimony"}),
    )
    .await["address"]
        .as_str()
        .expect("a claim address")
        .to_string();

    let writes: Vec<(&'static str, Value)> = vec![
        (
            "add_entity",
            json!({"kind": "person", "handle": "lisa", "name": "Lisa",
                   "source": "user-named", "request_id": "order-create"}),
        ),
        (
            "archive_entity",
            json!({"handle": "person:apu", "reason": "filed twice", "request_id": "order-archive"}),
        ),
        (
            "archive_entity",
            json!({"handle": "person:apu", "reason": "back again", "restore": true,
                   "request_id": "order-restore"}),
        ),
        (
            "merge_entities",
            json!({"duplicate": "person:skinner", "survivor": "person:krusty",
                   "request_id": "order-merge"}),
        ),
        (
            "retract",
            json!({"address": claim, "reason": "mistaken", "request_id": "order-retract"}),
        ),
    ];
    for (tool, args) in writes {
        let first = send(tool, args.clone()).await;
        assert_ne!(first["status"], "blocked", "{tool} landed first: {first}");
        assert!(first.get("repeat").is_none(), "{tool}: {first}");
        let again = send(tool, args).await;
        assert_eq!(
            again["repeat"], true,
            "{tool} sent again is a repeat, not a refusal by its own guard: {again}"
        );
    }
    stop(child, surface).await;
}
