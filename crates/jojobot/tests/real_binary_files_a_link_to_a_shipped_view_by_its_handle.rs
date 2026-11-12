//! **A field that names a view the software ships is filed and served by the
//! view's handle.** The real binary serves the journaled store, so a write goes
//! through the claims State and its row projection. A shipped view has no row and
//! no badge; the rows file it by its handle, as they file a claim's home and an
//! edge to it. The first case captures a reference-typed field naming a shipped
//! view and reads the value and the link row from the binary's own store, then
//! recalls the thing. The second puts back the shape the last release stored,
//! the view's supply identity in the value and in its link row, restarts the
//! binary over it and recalls the thing again.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

struct Served {
    child: tokio::process::Child,
    surface: Surface,
    sid: String,
    live: sqlx::MySqlPool,
}

async fn serve(scratch: &Scratch, store_port: u16) -> Served {
    let http_port = free_port();
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
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await
        .expect("the shipped assistant boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot hands back a session: {started}"))
        .to_string();
    let live = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{store_port}/jojobot"))
        .expect("a pool on the binary's store");
    Served {
        child,
        surface,
        sid,
        live,
    }
}

impl Served {
    async fn recalled(&self) -> Value {
        self.surface
            .must(
                "recall",
                json!({"subject": "person:homer", "sid": self.sid}),
            )
            .await
            .expect("the person is recalled")
    }

    /// The value and the link row the store holds for the field.
    async fn rows(&self) -> (Vec<Option<String>>, Vec<String>) {
        let values = sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'looks_at'")
            .fetch_all(&self.live)
            .await
            .expect("the field writes read");
        let targets = sqlx::query_scalar("SELECT target FROM field_link WHERE `key` = 'looks_at'")
            .fetch_all(&self.live)
            .await
            .expect("the link rows read");
        (values, targets)
    }

    async fn stop(mut self) {
        self.live.close().await;
        self.surface.finish().await;
        self.child.start_kill().expect("the binary stops");
        self.child.wait().await.expect("the binary is reaped");
    }
}

/// The person, the type that makes `looks_at` a reference, and the capture that
/// names a shipped view.
async fn name_a_shipped_view(served: &Served) {
    let sid = &served.sid;
    served
        .surface
        .must(
            "add_entity",
            json!({"kind": "person", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sid": sid}),
        )
        .await
        .expect("the person is added");
    served
        .surface
        .must(
            "declare_type",
            json!({"name": "viewer", "fields": [{"key": "looks_at", "holds": "reference"}],
                   "sid": sid}),
        )
        .await
        .expect("the type is declared");
    served
        .surface
        .must(
            "capture",
            json!({"subject": "person:homer", "content": "looks at the colleagues view",
                   "provenance": "testimony", "sets": {"looks_at": "view:colleagues"},
                   "sid": sid}),
        )
        .await
        .expect("the link to a shipped view is captured");
}

#[tokio::test]
async fn a_link_to_a_shipped_view_is_filed_and_served_by_its_handle() {
    let scratch = Scratch::new("binary-link-to-a-shipped-view");
    let served = serve(&scratch, free_port()).await;
    name_a_shipped_view(&served).await;

    let recalled = served.recalled().await;
    let (values, targets) = served.rows().await;
    served.stop().await;

    assert_eq!(
        recalled["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "the served value: {recalled}"
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

/// **A link filed by the view's handle survives a log that is rebuilt from the rows.**
/// The write leaves the handle in the rows and the supply identity in the State.
/// With the log removed, the next boot imports the rows into a new State; the
/// link must still name the view, in the value served and in who points at the view.
#[tokio::test]
async fn a_link_filed_by_a_shipped_views_handle_survives_a_log_rebuilt_from_the_rows() {
    let scratch = Scratch::new("binary-link-rebuilt-from-rows");
    let store_port = free_port();
    let first = serve(&scratch, store_port).await;
    name_a_shipped_view(&first).await;
    sqlx::query("DELETE FROM event_log WHERE slot <> 'validity'")
        .execute(&first.live)
        .await
        .expect("the log is removed");
    first.stop().await;

    let second = serve(&scratch, store_port).await;
    let recalled = second.recalled().await;
    let view = second
        .surface
        .must(
            "recall",
            json!({"subject": "view:colleagues", "sid": second.sid}),
        )
        .await
        .expect("the view is recalled");
    second.stop().await;
    assert_eq!(
        recalled["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "the served value after the log is rebuilt: {recalled}"
    );
    assert_eq!(
        view["objects"][0]["held"]["count"], 1,
        "the thing that looks at the view is among who points at it: {view}"
    );
}

/// **A value the last release stored with the view's supply identity is served as
/// the view's handle after a restart.** The rows are put back in the recorded
/// shape (the identity behind the mark in the value, bare in the link row) and
/// the log keeps what the State held, so the restart reads exactly what a store
/// the last release recorded holds.
#[tokio::test]
async fn a_value_stored_with_a_shipped_views_identity_is_served_as_its_handle_after_a_restart() {
    let scratch = Scratch::new("binary-stored-identity-of-a-shipped-view");
    let store_port = free_port();
    let first = serve(&scratch, store_port).await;
    name_a_shipped_view(&first).await;
    for sql in [
        "UPDATE field_write SET value = '@#supplied/view:colleagues' WHERE `key` = 'looks_at'",
        "UPDATE field_link SET target = 'supplied/view:colleagues' WHERE `key` = 'looks_at'",
    ] {
        sqlx::query(sql)
            .execute(&first.live)
            .await
            .expect("the recorded shape is put back");
    }
    first.stop().await;

    let second = serve(&scratch, store_port).await;
    let recalled = second.recalled().await;
    second.stop().await;
    assert_eq!(
        recalled["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "the served value after the restart: {recalled}"
    );
}
