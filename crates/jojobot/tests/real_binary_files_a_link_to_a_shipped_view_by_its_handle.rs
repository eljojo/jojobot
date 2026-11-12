//! **A field that names a view the software ships is filed and served by the
//! view's handle.** The real binary serves the journaled store, so the write goes
//! through the claims State and its row projection. A shipped view has no row and
//! no badge; the rows file it by its handle, as they file a claim's home and an
//! edge to it. The case captures a reference-typed field naming a shipped view,
//! then reads the value and the link row from the binary's own store, and recalls
//! the thing through the served surface.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_exercise::surface::Surface;
use serde_json::json;

#[tokio::test]
async fn a_link_to_a_shipped_view_is_filed_and_served_by_its_handle() {
    let scratch = Scratch::new("binary-link-to-a-shipped-view");
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
        .unwrap_or_else(|| panic!("the boot hands back a session: {started}"))
        .to_string();

    surface
        .must(
            "add_entity",
            json!({"kind": "person", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sid": sid}),
        )
        .await
        .expect("the person is added");
    surface
        .must(
            "declare_type",
            json!({"name": "viewer", "fields": [{"key": "looks_at", "holds": "reference"}],
                   "sid": sid}),
        )
        .await
        .expect("the type is declared");
    surface
        .must(
            "capture",
            json!({"subject": "person:homer", "content": "looks at the colleagues view",
                   "provenance": "testimony", "sets": {"looks_at": "view:colleagues"},
                   "sid": sid}),
        )
        .await
        .expect("the link to a shipped view is captured");

    let served = surface
        .must("recall", json!({"subject": "person:homer", "sid": sid}))
        .await
        .expect("the person is recalled");
    let live = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{store_port}/jojobot"))
        .expect("a pool on the binary's store");
    let values: Vec<Option<String>> =
        sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'looks_at'")
            .fetch_all(&live)
            .await
            .expect("the field writes read");
    let targets: Vec<String> =
        sqlx::query_scalar("SELECT target FROM field_link WHERE `key` = 'looks_at'")
            .fetch_all(&live)
            .await
            .expect("the link rows read");
    live.close().await;
    surface.finish().await;
    child.start_kill().expect("the binary stops");
    child.wait().await.expect("the binary is reaped");

    assert_eq!(
        served["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "the served value: {served}"
    );
    assert_eq!(
        targets,
        vec!["view:colleagues".to_string()],
        "the link row files the view by its handle; the values are {values:?}"
    );
    assert!(
        values
            .iter()
            .flatten()
            .all(|value| !value.contains("supplied/")),
        "the stored value holds the handle, not the supply identity: {values:?}"
    );
}
