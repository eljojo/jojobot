//! **A thread an older build wrote, before any build held a `decision` kind**,
//! boots unchanged on the current binary and takes its first decision.

use super::*;

/// A thread the recorded build wrote, before any build held a `decision` kind.
const RECORDED_THREAD: &str = "thread:upgrade-fixture-line";

/// **Rows for a thread with one record and no child**, written below the surface
/// into the recorded schema, as a build that did not know decisions would have
/// left it.
async fn plant_a_thread_with_no_decisions(pool: &sqlx::MySqlPool) {
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                             archived_reason, archived_at)
         VALUES (?, 'thread', 'A Recorded Line', 'test', NULL, NULL, 'on-demand', '', 'line01',
                 NULL, NULL, NULL)",
    )
    .bind(RECORDED_THREAD)
    .execute(pool)
    .await
    .expect("the thread lands");
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, recorded_at,
                           edge_shape, edge_object, event_kind, derived_from, derived_from_id,
                           inserted_at, stale_after, happened_at, happened_through)
         VALUES ('line01', 'f1', 'a finding the line gathered', NULL, 'testimony', 'settled',
                 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .execute(pool)
    .await
    .expect("the thread's record lands");
}

/// **A store with threads and no decisions boots unchanged, and its first
/// decision then leads its thread.** The build reconciles the shipped kinds at
/// every start, so the kind needs no migration. Before a decision exists the
/// thread answers alone and carries none of the keys a decision adds; after one
/// is made under it, the same read puts the decision first.
#[tokio::test]
async fn a_store_with_threads_and_no_decisions_boots_unchanged_and_takes_its_first_decision() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "thread-decisions").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_thread_with_no_decisions(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let parse = |what: &str, body: &str| -> serde_json::Value {
        serde_json::from_str(body).unwrap_or_else(|_| fail(what, body))
    };
    let handles = |answer: &serde_json::Value| -> Vec<String> {
        answer["objects"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter_map(|o| o["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let before = parse(
        "the thread before any decision",
        &surface
            .call("recall", json!({"subject": RECORDED_THREAD}))
            .await,
    );
    if handles(&before) != [RECORDED_THREAD] {
        fail("the thread answering alone", &before.to_string());
    }
    for added in ["decisions_archived", "decisions_not_shown"] {
        if before.get(added).is_some() {
            fail(
                "the thread carrying nothing a decision adds",
                &before.to_string(),
            );
        }
    }

    let boot = parse(
        "booting to write",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    let made = surface
        .call(
            "add_entity",
            json!({"kind": "decision", "handle": "upgrade-fixture-choice", "name": "A choice",
                   "source": "user-named", "parent": RECORDED_THREAD, "sid": sid}),
        )
        .await;
    if made.contains("\"status\":\"blocked\"") || made.contains("no kind is named") {
        fail("making a decision under the recorded thread", &made);
    }
    let said = surface
        .call(
            "capture",
            json!({"subject": "decision:upgrade-fixture-choice", "content": "the line goes on",
                   "provenance": "testimony", "sid": sid}),
        )
        .await;
    if said.contains("\"status\":\"blocked\"") {
        fail("writing the decision", &said);
    }
    let after = parse(
        "the thread after its first decision",
        &surface
            .call("recall", json!({"subject": RECORDED_THREAD}))
            .await,
    );
    booted.stop().await;
    assert_eq!(
        handles(&after),
        ["decision:upgrade-fixture-choice", RECORDED_THREAD],
        "recorded at {git_ref}: the decision leads the thread it was made under: {after}"
    );
    drop(scratch);
}
