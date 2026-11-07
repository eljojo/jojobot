//! **A write made through the served stack leaves its event in the log.**
//!
//! The binary serves the journaled store, so an entity write that passes through
//! a writer job emits a claims event into that job's batch. The case adds a thing
//! through the MCP surface of the real binary and reads the event log from the
//! binary's own store: the batch holding the thing's handle is there after the
//! write and not before. A binary that served the bare store would write the row
//! and emit nothing.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_exercise::surface::Surface;
use serde_json::json;

#[tokio::test]
async fn an_entity_added_through_the_real_binary_has_its_event_in_the_log() {
    let scratch = Scratch::new("binary-logs-a-served-write");
    let http_port = free_port();
    let store_port = free_port();

    let binary = jojobot_exercise::room::server_binary().expect("the built jojobot binary");
    let mut child = tokio::process::Command::new(binary)
        .env("STATE_DIRECTORY", &scratch.0)
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
    let _serving = wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the binary {failure}"));
    let surface = Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
        .await
        .expect("connecting to the binary");
    let started = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the shipped assistant boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot hands back a session: {started}"));

    let live = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{store_port}/jojobot"))
        .expect("a pool on the binary's store");
    // A batch payload is JSON whose members are arrays of bytes; a member is the
    // JSON of one event. Count the object-created events that name the handle.
    let created_events = || async {
        let payloads: Vec<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot LIKE 'b:%'")
                .fetch_all(&live)
                .await
                .expect("the event log reads");
        payloads
            .iter()
            .filter_map(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok())
            .filter_map(|batch| batch[5].as_array().cloned())
            .flatten()
            .filter_map(|member| {
                let bytes: Vec<u8> = member
                    .as_array()?
                    .iter()
                    .filter_map(|byte| byte.as_u64().map(|byte| byte as u8))
                    .collect();
                String::from_utf8(bytes).ok()
            })
            .filter(|event| event.contains("ClaimsObjectCreated"))
            .collect::<Vec<String>>()
    };
    // The event names the object by its badge and carries the name it was added with.
    let names_it = |event: &String| event.contains(r#""name":"Ned Flanders""#);
    assert!(
        !created_events().await.iter().any(names_it),
        "an object event names the thing before it is added"
    );

    surface
        .must(
            "add_entity",
            json!({"kind": "person", "handle": "ned-flanders", "name": "Ned Flanders",
                   "source": "user-named", "sid": sid}),
        )
        .await
        .expect("the thing is added");

    let logged = created_events().await;
    live.close().await;
    surface.finish().await;
    child.start_kill().expect("the binary stops");
    child.wait().await.expect("the binary is reaped");
    assert_eq!(
        logged.iter().filter(|event| names_it(event)).count(),
        1,
        "the write left no object event for the thing it added: {logged:?}"
    );
}
