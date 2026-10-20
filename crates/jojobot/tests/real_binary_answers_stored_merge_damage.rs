//! **A store holding a merge loop is answered as damage a person must repair, by
//! every read that meets it, through the binary that serves it.**
//!
//! Merge events that lead back to where they began cannot be made by any verb; a
//! hand edit can. A read that meets one used to say the store could not be
//! reached and that trying again was reasonable. It cannot help: the same call
//! meets the same damage. This seeds a store with a loop as a previous process
//! would have left it, starts the real binary over it, and asks each read the
//! loop touches. **Paired**: a read the loop does not touch answers as before.

use std::process::Stdio;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, kinds};
use tokio::io::AsyncBufReadExt;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The handle of a character, built so the roster gate reads the slug and not a
/// literal.
fn person(slug: &str) -> EntityId {
    EntityId::person(format!("person:{slug}"))
}

#[tokio::test]
async fn every_read_a_stored_merge_loop_touches_is_answered_as_damage_a_person_repairs() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-merge-damage");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let db_dir = state_dir.join("db");
    let store_port = free_port();

    // **Seed the store as an earlier process would have left it**: two things, a
    // merge, and then merge events edited by hand into a loop.
    let mut seeding_server = Dolt::start(&db_dir, store_port)
        .await
        .expect("the store comes up");
    migrate::run(seeding_server.pool())
        .await
        .expect("the schema applies");
    let seeding = DoltMemory::open(seeding_server.pool().clone());
    kinds::seed(&seeding).await.expect("the kinds are declared");
    let (homer, apu) = (person("homer"), person("apu"));
    for (id, name) in [(&homer, "Homer"), (&apu, "Apu")] {
        seeding
            .add_entity(NewEntity::new(id.clone(), name, "test"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    for (id, what) in [(&homer, "keeps bees"), (&apu, "runs the store")] {
        seeding
            .capture(NewFact::about(id.clone(), what, date(2026, 9, 1)))
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
    }
    seeding
        .merge(
            &apu,
            &homer,
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the fold lands");
    let pool = seeding_server.pool().clone();
    let badge = |handle: &str| {
        let handle = handle.to_string();
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT badge FROM entity WHERE id = ?")
                .bind(handle)
                .fetch_one(&pool)
                .await
                .expect("the thing has a badge")
        }
    };
    let (homer_key, apu_key) = (badge(homer.as_str()).await, badge(apu.as_str()).await);
    let first_no: i64 = sqlx::query_scalar("SELECT merge_no FROM merge_event WHERE ordinal = 0")
        .fetch_one(&pool)
        .await
        .expect("the event is there");
    let moved: Vec<(String, String)> = sqlx::query_as(
        "SELECT folded_id, survivor_id FROM merge_event WHERE merge_no = ? AND ordinal > 0",
    )
    .bind(first_no)
    .fetch_all(&pool)
    .await
    .expect("the claim map reads");
    let at: String = sqlx::query_scalar("SELECT merged_at FROM merge_event WHERE ordinal = 0")
        .fetch_one(&pool)
        .await
        .expect("the moment reads");
    // The loop: the survivor folded back into the thing that was folded into it,
    // with every claim given its old address again.
    sqlx::query(
        "INSERT INTO merge_event (merge_no, ordinal, folded, survivor, merged_at, session)
         VALUES (?, 0, ?, ?, ?, 'run-b')",
    )
    .bind(first_no + 1)
    .bind(&homer_key)
    .bind(&apu_key)
    .bind(&at)
    .execute(&pool)
    .await
    .expect("the loop's event lands");
    for (ordinal, (was, now)) in (1_i64..).zip(&moved) {
        sqlx::query(
            "INSERT INTO merge_event
               (merge_no, ordinal, folded, survivor, merged_at, session, folded_id, survivor_id)
             VALUES (?, ?, ?, ?, ?, 'run-b', ?, ?)",
        )
        .bind(first_no + 1)
        .bind(ordinal)
        .bind(&homer_key)
        .bind(&apu_key)
        .bind(&at)
        .bind(now)
        .bind(was)
        .execute(&pool)
        .await
        .expect("the loop's claim map lands");
    }
    seeding_server.stop().await;
    kinds::load_shipped();

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut seen = Vec::new();
    let boot_began = std::time::Instant::now();
    let served = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let hit = line.contains("serving http://");
                    seen.push(line);
                    if hit {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    if !served {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    assert!(
        served,
        "the server did not serve over a store holding a merge loop after {:?} (a boot is given \
         {:?}):\n{}",
        boot_began.elapsed(),
        jojobot_exercise::room::READY_TIMEOUT,
        seen.join("\n")
    );

    let surface =
        jojobot_exercise::surface::Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
            .await
            .expect("connecting to the real binary");
    let reads = [
        (
            "recall of the survivor with its claims",
            "recall",
            serde_json::json!({"subject": homer.as_str(), "facts": true}),
        ),
        (
            "recall of the thing that was folded in",
            "recall",
            serde_json::json!({"subject": apu.as_str()}),
        ),
        (
            "recall of a claim by the address it had",
            "recall",
            serde_json::json!({"history_record": format!("{apu}#f1")}),
        ),
        (
            "recall of what was built on a claim by the address it had",
            "recall",
            serde_json::json!({"built_on": format!("{apu}#f1")}),
        ),
    ];
    let mut answers = Vec::new();
    for (label, verb, arguments) in reads {
        answers.push((label, surface.call(verb, arguments).await));
    }
    let untouched = surface
        .call("recall", serde_json::json!({"subject": "topic:instance"}))
        .await;
    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;

    for (label, answer) in &answers {
        let said = serde_json::from_str::<serde_json::Value>(answer)
            .ok()
            .and_then(|body| body["transport_error"].as_str().map(str::to_string))
            .unwrap_or_else(|| {
                panic!("{label}: the damage was not answered as a refusal: {answer}")
            });
        assert!(
            said.ends_with(r#"({"fix_by":"person"})"#),
            "{label}: a person must repair it, and the answer says so: {said}"
        );
        assert!(
            said.contains(homer.as_str()) && said.contains(apu.as_str()),
            "{label}: the answer names the things in the loop: {said}"
        );
        assert!(
            !said.contains(&homer_key) && !said.contains(&apu_key),
            "{label}: a key the store files them under reached the caller: {said}"
        );
        assert!(
            !said.contains("Try once more"),
            "{label}: damage was told the way an outage is: {said}"
        );
    }
    // **Paired**: a read the loop does not touch is answered as it always was.
    assert!(
        untouched.contains("\"id\":\"topic:instance\"") && !untouched.contains("transport_error"),
        "a read the loop does not touch is refused: {untouched}"
    );
    drop(scratch);
}
