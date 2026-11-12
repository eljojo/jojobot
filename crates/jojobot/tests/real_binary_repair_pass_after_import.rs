//! **A repair pass that runs on a boot after the claims import is read by the
//! served stack.** The boot runs the repair passes before the import and starts
//! journaling after it, so a pass that runs on a boot whose import is already done
//! writes rows and emits no event. The claims State holds no event for such a write.
//! Today the fold's image serves the read and `before_the_import` refreshes it from
//! the store after the passes, so the cases pass; they pin what a stack that serves
//! the read from the State has to keep, and go red the day it does not carry the stamp.
//!
//! The pass here is the due-day pass: it stamps the day a thing became late when its
//! schedule cannot be read. The store is made as an old build left it (rows, no log, a
//! thing whose schedule cannot be read, no stamp). The first case boots it with the
//! pass owed, so the pass runs before the import and the import carries its stamp. The
//! second boots it twice: the first boot finds the pass finished and imports the thing
//! unstamped, the marker is taken off, and the second boot runs the pass over an
//! import that is already done. Each case reads the stamp from the rows and then
//! through the served surface.

use std::process::Stdio;

use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{Dolt, UNREADABLE_DUE_PASS, migrate};
use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

const LATE: &str = "work:sigma";

/// **The store an old build left**: a project and one work item whose schedule cannot be
/// read, written through the store's own port (rows, no log), and the due-day pass
/// marked finished when `finished` says so.
async fn an_old_store(scratch: &Scratch, finished: bool) {
    let mut store = Dolt::start(&scratch.0.join("db"), free_port())
        .await
        .expect("the store comes up with no binary over it");
    migrate::run(store.pool()).await.expect("the schema");
    let memory = DoltMemory::open(store.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    for (handle, parent) in [("project:atlas", None), (LATE, Some("project:atlas"))] {
        memory
            .add_entity(NewEntity {
                parent: parent.map(|parent| EntityId(parent.into())),
                ..NewEntity::new(EntityId(handle.into()), handle, "user-named")
            })
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let mut fact = NewFact::about(
        EntityId(LATE.into()),
        "recorded as it was",
        date(2026, 9, 1),
    );
    fact.provenance = Provenance::Inference;
    fact.fields = [
        ("cadence_days", "often"),
        ("advances_from", "due_date"),
        ("counts_from", "2026-06-01"),
        // A status word that finishes nothing: the pass stamps work that is still open.
        ("status", "next"),
    ]
    .iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    if finished {
        sqlx::query("INSERT IGNORE INTO store_pass (name, finished_at) VALUES (?, ?)")
            .bind(UNREADABLE_DUE_PASS)
            .bind("2026-10-01T00:00:00Z")
            .execute(store.pool())
            .await
            .expect("the pass is marked finished");
    }
    store.stop().await;
}

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
    /// The thing as the served surface reads it.
    async fn recalled(&self) -> Value {
        self.surface
            .must("recall", json!({"subject": LATE, "sid": self.sid}))
            .await
            .expect("the thing is recalled")
    }

    /// The stamps the rows hold for the day the thing became late.
    async fn stamps_in_the_rows(&self) -> Vec<Option<String>> {
        sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'due_on'")
            .fetch_all(&self.live)
            .await
            .expect("the field writes read")
    }

    async fn stop(mut self) {
        self.live.close().await;
        self.surface.finish().await;
        // An orderly stop: the store lets go of its port before the next boot asks for it,
        // so the second boot's rows are read from the store that boot runs.
        let pid = self.child.id().expect("the binary runs");
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success(), "the binary was sent the stop signal");
        tokio::time::timeout(std::time::Duration::from_secs(60), self.child.wait())
            .await
            .expect("the binary stops within a minute")
            .expect("the binary is reaped");
    }
}

/// **A pass that runs before the import is carried by it** (the control): the store is
/// owed the pass, the one boot runs it and then imports, and the served surface reads the
/// stamp the rows hold.
#[tokio::test]
async fn a_stamp_written_before_the_import_is_served() {
    let scratch = Scratch::new("repair-pass-before-import");
    an_old_store(&scratch, false).await;
    let boot = serve(&scratch, free_port()).await;
    let rows = boot.stamps_in_the_rows().await;
    let recalled = boot.recalled().await;
    boot.stop().await;

    assert_eq!(
        rows.len(),
        1,
        "the pass stamped the thing in the rows: {rows:?}"
    );
    assert!(
        recalled["objects"][0]["fields"]["due_on"].is_string(),
        "the served thing holds the stamp the rows hold: {recalled}"
    );
}

/// **A pass that runs on a boot after the import is read by the served stack.** The first
/// boot finds the pass finished and imports the thing unstamped; the marker is taken off
/// while it runs, and the second boot runs the pass over an import that is already done.
/// The rows must hold the stamp (so the pass ran) and the served surface must too.
#[tokio::test]
async fn a_stamp_written_after_the_import_is_served() {
    let scratch = Scratch::new("repair-pass-after-import");
    an_old_store(&scratch, true).await;
    let store_port = free_port();
    let first = serve(&scratch, store_port).await;
    assert!(
        first.stamps_in_the_rows().await.is_empty(),
        "the first boot ran no pass: it was marked finished"
    );
    assert!(
        first.recalled().await["objects"][0]["fields"]["due_on"].is_null(),
        "the thing is imported without a stamp"
    );
    sqlx::query("DELETE FROM store_pass WHERE name = ?")
        .bind(UNREADABLE_DUE_PASS)
        .execute(&first.live)
        .await
        .expect("the pass is owed again");
    first.stop().await;

    let second = serve(&scratch, store_port).await;
    let rows = second.stamps_in_the_rows().await;
    let recalled = second.recalled().await;
    second.stop().await;

    assert_eq!(
        rows.len(),
        1,
        "the second boot ran the pass and stamped the thing in the rows: {rows:?}"
    );
    assert!(
        recalled["objects"][0]["fields"]["due_on"].is_string(),
        "the served thing holds the stamp the rows hold: {recalled}"
    );
}
