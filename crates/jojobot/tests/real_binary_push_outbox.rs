//! The configured binary delivers operator mail through the real Dolt outbox.
//! The stand-in records the publish before answering late, so seeing the HTTP
//! request alone cannot stand in for the outbox row reaching its final state.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_exercise::surface::Surface;
use serde_json::json;

mod support;

#[tokio::test]
async fn the_real_binary_pushes_operator_mail_and_marks_its_outbox_sent_after_a_late_answer() {
    let scratch = Scratch::new("binary-push-outbox");
    let http_port = free_port();
    let store_port = free_port();
    let ntfy = support::FakeNtfy::start().await;
    ntfy.answer_late(Duration::from_millis(1500));

    let binary = jojobot_exercise::room::server_binary().expect("the built jojobot binary");
    let mut child = tokio::process::Command::new(binary)
        .env("STATE_DIRECTORY", &scratch.0)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("JOJOBOT_NTFY_URL", &ntfy.url)
        .env("JOJOBOT_NTFY_TOPIC", "phone")
        .env_remove("JOJOBOT_NTFY_TOKEN")
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
    .unwrap_or_else(|failure| panic!("the configured binary {failure}"));
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
    surface
        .must(
            "add_entity",
            json!({"kind": "person", "handle": "ned-flanders", "name": "Ned Flanders",
                   "source": "user-named", "sid": sid}),
        )
        .await
        .expect("the operator is added");
    surface
        .must(
            "capture",
            json!({"subject": "topic:instance", "content": "who the operator is",
                   "provenance": "testimony", "sets": {"operator": "person:ned-flanders"},
                   "sid": sid}),
        )
        .await
        .expect("the instance names its operator");
    let posted = surface
        .must(
            "post_message",
            json!({"to": "person:ned-flanders", "subject": "the lease",
                   "body": "it is signed", "sid": sid}),
        )
        .await
        .expect("the operator's mail lands");
    let id = posted["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the post hands back its id: {posted}"));

    let live = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{store_port}/jojobot"))
        .expect("a pool on the binary's store");
    // The runner writes `sent` after the late answer. Stopping at the first
    // publish would race that write and leave a pending row behind.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let rows: Vec<(String, String)> =
                sqlx::query_as("SELECT message_id, state FROM push_outbox")
                    .fetch_all(&live)
                    .await
                    .expect("the live outbox reads");
            if rows == [(id.to_string(), "sent".to_string())] {
                break;
            }
            assert!(
                rows.is_empty() || rows == [(id.to_string(), "pending".to_string())],
                "only this post can be pending before it is sent: {rows:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the post's Dolt outbox row never ended sent after the late answer");
    let published = ntfy.published();
    assert_eq!(published.len(), 1, "one push for the post: {published:?}");
    assert_eq!(published[0].body["topic"], "phone");
    assert_eq!(published[0].body["title"], "the lease");
    assert!(
        published[0].body["message"]
            .as_str()
            .expect("the push carries text")
            .contains("it is signed"),
        "the posted body reaches the phone: {published:?}"
    );
    live.close().await;
    surface.finish().await;
    child.start_kill().expect("the binary stops");
    child.wait().await.expect("the binary is reaped");
}
