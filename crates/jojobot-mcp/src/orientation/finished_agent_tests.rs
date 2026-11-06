//! A role's pointer and the run that finished behind it.

use super::*;
use crate::harness::*;
use crate::session::WrapSessionArgs;
use jojobot_domain::memory::{FactPatch, NewFact};

async fn boot_args(jojobot: &Jojobot, args: serde_json::Value) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(serde_json::from_value(args).expect("boot args")))
            .await
            .expect("boot answered"),
    )
}

async fn wrap(jojobot: &Jojobot, sid: String) -> serde_json::Value {
    json_of(
        &jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                sid,
                story: "finished the dispatch".into(),
            }))
            .await
            .expect("wrap answered"),
    )
}

async fn stale_agent(jojobot: &Jojobot) {
    let role = EntityId("role:dev-dispatch".into());
    let record = jojobot
        .memory
        .recall(&role)
        .await
        .expect("role record")
        .remove(0);
    jojobot
        .memory
        .update_fact(
            &record.address(),
            FactPatch {
                fields: [("agent".into(), "old-agent".into())].into(),
                derived: ["agent".into()].into(),
                ..FactPatch::default()
            },
            &EntityId("bot:gamma".into()),
        )
        .await
        .expect("old boot's separate agent write");
}

#[tokio::test]
async fn p1_claim_writes_its_agent_with_its_holder() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({
            "bot":"gamma", "resume":"new", "claim":"dev-dispatch", "agent":"gamma-agent"
        }),
    )
    .await;
    assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");
    let fields = jojobot
        .memory
        .fields(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("role");
    assert_eq!(
        fields.get("agent").map(String::as_str),
        Some("gamma-agent"),
        "{fields:?}"
    );
    assert_eq!(
        fields.get("holder").map(String::as_str),
        booted["session"]["run"].as_str()
    );
    let caller = jojobot
        .caller(sid_of(&booted).as_deref())
        .expect("caller")
        .expect("run");
    let session = jojobot
        .sessions
        .read_session(&caller.card.expect("claim made a card"))
        .await
        .expect("card");
    assert_eq!(
        serde_json::to_value(session).expect("card JSON")["agent"],
        "gamma-agent"
    );
}

#[tokio::test]
async fn p1_claim_without_agent_removes_the_previous_agent() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    stale_agent(&jojobot).await;
    let claimed = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "sid":sid_of(&booted), "claim":"dev-dispatch"}),
    )
    .await;
    assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
    let fields = jojobot
        .memory
        .fields(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("role");
    assert!(
        !fields.contains_key("agent"),
        "stale agent survived: {fields:?}"
    );
}

#[tokio::test]
async fn p1_wrap_clears_the_holder_and_its_agent() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    stale_agent(&jojobot).await;
    let wrapped = wrap(&jojobot, sid_of(&booted).expect("key")).await;
    assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");
    let fields = jojobot
        .memory
        .fields(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("role");
    assert!(!fields.contains_key("holder"), "{fields:?}");
    assert!(
        !fields.contains_key("agent"),
        "stale agent survived: {fields:?}"
    );
}

#[tokio::test]
async fn p1_finished_agent_cannot_boot_or_claim_again() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    finished_agent_journey(&jojobot).await;
}

#[tokio::test]
async fn p1_every_host_is_refused_after_a_picked_up_run_wraps() {
    let jojobot = handler().quiet_after(jiff::SignedDuration::ZERO);
    make_bot(&jojobot, "gamma").await;
    let alpha = boot_args(
        &jojobot,
        serde_json::json!({
            "bot":"gamma", "resume":"new", "agent":"alpha-agent"
        }),
    )
    .await;
    let alpha_key = sid_of(&alpha).expect("alpha key");
    let caller = jojobot.identified(Some(&alpha_key)).expect("alpha caller");
    let gate = jojobot.registry.gate(caller.bot.as_str());
    {
        let held = gate.lock().await;
        jojobot
            .session_for(&held, &caller, Some("a quiet run"), None)
            .await
            .expect("run record");
    }
    let restored = boot_args(
        &jojobot,
        serde_json::json!({
            "bot":"gamma", "resume":alpha["session"]["run"], "agent":"alpha-agent"
        }),
    )
    .await;
    assert_eq!(
        restored["session"]["run"], alpha["session"]["run"],
        "{restored}"
    );
    let beta = boot_args(
        &jojobot,
        serde_json::json!({
            "bot":"gamma", "resume":alpha["session"]["run"], "agent":"beta-agent"
        }),
    )
    .await;
    assert_eq!(beta["session"]["run"], alpha["session"]["run"], "{beta}");
    let wrapped = wrap(&jojobot, sid_of(&beta).expect("beta key")).await;
    assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");
    let restarted = Jojobot::new(
        jojobot.memory.clone(),
        jojobot.search.clone(),
        jojobot.mailboxes.clone(),
        jojobot.sessions.clone(),
        jojobot.teachings.clone(),
        std::sync::Arc::new(crate::sid::SessionRegistry::new()),
    );
    restarted.registry.rebuild_from(
        &restarted
            .sessions
            .all_sessions()
            .await
            .expect("persisted runs"),
    );
    for agent in ["alpha-agent", "beta-agent"] {
        let refused = boot_args(
            &restarted,
            serde_json::json!({
                "bot":"gamma", "resume":"new", "agent":agent
            }),
        )
        .await;
        assert_eq!(refused["status"], "blocked", "{agent}: {refused}");
        let text = refused.to_string();
        assert!(
            text.contains("end the turn and do nothing") && text.contains("document:build-runtime"),
            "{text}"
        );
    }
}

async fn finished_agent_journey(jojobot: &Jojobot) {
    let booted = boot_args(
        jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "agent":"gamma-agent"}),
    )
    .await;
    let wrapped = wrap(jojobot, sid_of(&booted).expect("key")).await;
    let jojobot = Jojobot::new(
        jojobot.memory.clone(),
        jojobot.search.clone(),
        jojobot.mailboxes.clone(),
        jojobot.sessions.clone(),
        jojobot.teachings.clone(),
        std::sync::Arc::new(crate::sid::SessionRegistry::new()),
    );
    jojobot.registry.rebuild_from(
        &jojobot
            .sessions
            .all_sessions()
            .await
            .expect("persisted runs"),
    );
    let refused = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "resume":"new", "agent":"gamma-agent", "claim":"dev-dispatch"})).await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    let text = refused.to_string();
    assert!(
        text.contains("end the turn and do nothing") && text.contains("document:build-runtime"),
        "{refused}"
    );
    assert!(
        text.contains(wrapped["session"]["id"].as_str().expect("run")),
        "{refused}"
    );
    assert!(
        jojobot
            .memory
            .fields(&EntityId("role:dev-dispatch".into()))
            .await
            .is_err(),
        "refused boot claimed a role"
    );
}

#[tokio::test]
async fn p1_finished_agent_journey_over_dolt() {
    let scratch = jojobot_adapters::testing::Scratch::new("finished-agent");
    let mut store = jojobot_adapters::testing::start_unhurried(
        &scratch.0,
        jojobot_adapters::testing::free_port(),
    )
    .await
    .expect("the real store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&store, "finished_agent")
        .await
        .expect("the schema");
    let sessions = std::sync::Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(pool));
    let (jojobot, _) = crate::session::testing::with_sessions_port_and_memory(sessions);
    make_bot(&jojobot, "gamma").await;
    finished_agent_journey(&jojobot).await;
    store.stop().await;
}

#[tokio::test]
async fn p1_agent_is_recorded_before_a_roleless_boot_returns() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "agent":"gamma-agent"}),
    )
    .await;
    let runs = jojobot
        .sessions
        .sessions_of(&EntityId("bot:gamma".into()))
        .await
        .expect("runs");
    let run = runs
        .iter()
        .find(|run| run.sid.as_ref().map(|sid| sid.as_str()) == booted["session"]["run"].as_str())
        .expect("agent boot already persisted its run");
    assert_eq!(
        serde_json::to_value(run).expect("run JSON")["agent"],
        "gamma-agent"
    );
}

#[tokio::test]
async fn p1_agent_is_not_silently_dropped_beside_a_fetch() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "resume":"new"})).await;
    let refused = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "sid":sid_of(&booted), "agent":"gamma-agent", "skill":"asking"})).await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(refused.to_string().contains("agent"), "{refused}");
}

#[tokio::test]
async fn p1_claim_replaces_a_separate_legacy_agent_in_one_revision() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    separate_agent(&jojobot).await;
    let role = EntityId("role:dev-dispatch".into());
    let record = jojobot
        .memory
        .recall(&role)
        .await
        .expect("records")
        .into_iter()
        .find(|fact| fact.fields.contains_key("holder"))
        .expect("claim record")
        .address();
    let before = jojobot
        .memory
        .claim_history(&record)
        .await
        .expect("history")
        .len();
    let claimed = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "sid":sid_of(&booted), "claim":"dev-dispatch", "agent":"gamma-agent"})).await;
    assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
    let fields = jojobot.memory.fields(&role).await.expect("role");
    assert_eq!(
        fields.get("agent").map(String::as_str),
        Some("gamma-agent"),
        "{fields:?}"
    );
    assert_eq!(
        jojobot
            .memory
            .claim_history(&record)
            .await
            .expect("history")
            .len(),
        before + 1
    );
}

#[tokio::test]
async fn p1_unwrapped_restore_and_wrap_code_correction_stay_available() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "agent":"gamma-agent"}),
    )
    .await;
    let key = sid_of(&booted).expect("key");
    let caller = jojobot.identified(Some(&key)).expect("caller");
    let gate = jojobot.registry.gate(caller.bot.as_str());
    let card = {
        let held = gate.lock().await;
        jojobot
            .session_for(&held, &caller, Some("paused"), None)
            .await
            .expect("materialized run")
    };
    jojobot
        .sessions
        .close(&card, SessionState::Abandoned)
        .await
        .expect("stopped without wrapping");
    let restored = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "agent":"gamma-agent", "resume":booted["session"]["run"]})).await;
    assert_eq!(
        restored["session"]["run"], booted["session"]["run"],
        "{restored}"
    );
    let wrapped = wrap(&jojobot, sid_of(&restored).expect("restored key")).await;
    let correction = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "agent":"gamma-agent", "resume":wrapped["wrap_code"]}),
    )
    .await;
    assert!(sid_of(&correction).is_some(), "{correction}");
    assert_eq!(
        correction["session"]["run"], booted["session"]["run"],
        "{correction}"
    );
}

async fn separate_agent(jojobot: &Jojobot) {
    jojobot
        .memory
        .capture(NewFact {
            fields: [("agent".into(), "old-agent".into())].into(),
            sets: ["agent".into()].into(),
            ..NewFact::about(
                EntityId("role:dev-dispatch".into()),
                "the old boot recorded its agent",
                jiff::civil::date(2026, 7, 1),
            )
        })
        .await
        .expect("legacy agent capture")
        .written()
        .expect("capture lands");
}

#[tokio::test]
async fn p1_no_agent_claim_clears_a_separate_legacy_agent_fact() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    separate_agent(&jojobot).await;
    let claimed = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "sid":sid_of(&booted), "claim":"dev-dispatch"}),
    )
    .await;
    assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
    let fields = jojobot
        .memory
        .fields(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("role");
    assert!(
        !fields.contains_key("agent"),
        "separate stale agent survived: {fields:?}"
    );
}

#[tokio::test]
async fn p1_wrap_clears_a_separate_legacy_agent_fact() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    separate_agent(&jojobot).await;
    let wrapped = wrap(&jojobot, sid_of(&booted).expect("key")).await;
    assert_eq!(wrapped["roles"][0]["released"], true, "{wrapped}");
    let fields = jojobot
        .memory
        .fields(&EntityId("role:dev-dispatch".into()))
        .await
        .expect("role");
    assert!(!fields.contains_key("holder"), "{fields:?}");
    assert!(
        !fields.contains_key("agent"),
        "separate stale agent survived: {fields:?}"
    );
}

#[test]
fn p1_domain_role_pointer_value_table() {
    jojobot_domain::session::role_write::testing::p1_role_pointer_value_table();
}

#[tokio::test]
async fn p1_shared_store_claim_pointer() {
    jojobot_domain::memory::testing::contract::p1_claim_replaces_or_clears_its_agent(
        &jojobot_domain::memory::testing::InMemoryMemory::booted(),
    )
    .await;
}

#[tokio::test]
async fn p1_shared_store_release_pointer() {
    jojobot_domain::memory::testing::contract::p1_release_clears_its_agent(
        &jojobot_domain::memory::testing::InMemoryMemory::booted(),
    )
    .await;
}

#[tokio::test]
async fn p1_wrapped_sid_tells_the_caller_to_end_its_turn() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "resume":"new"})).await;
    let sid = sid_of(&booted).expect("key");
    wrap(&jojobot, sid.clone()).await;
    let refused = jojobot.caller(Some(&sid)).expect_err("wrapped key");
    let text = json_of(&refused).to_string();
    assert!(
        text.contains("end the turn and do nothing") && text.contains("document:build-runtime"),
        "{text}"
    );
}

#[tokio::test]
async fn p1_wrapped_sid_cannot_boot_a_new_run() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let booted = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "resume":"new"})).await;
    let sid = sid_of(&booted).expect("key");
    wrap(&jojobot, sid.clone()).await;
    let refused = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "sid":sid, "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(
        refused.to_string().contains("end the turn and do nothing"),
        "{refused}"
    );
}

async fn wrapped_after_pickup() -> (Jojobot, String, String) {
    let jojobot = handler().quiet_after(jiff::SignedDuration::ZERO);
    make_bot(&jojobot, "gamma").await;
    let first = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    let first_key = sid_of(&first).expect("first host key");
    let run = first["session"]["run"]
        .as_str()
        .expect("public run")
        .to_string();
    let picked_up = boot_args(&jojobot, serde_json::json!({"bot":"gamma", "resume":run})).await;
    assert_eq!(picked_up["session"]["run"], run, "{picked_up}");
    let wrapped = wrap(&jojobot, sid_of(&picked_up).expect("new host key")).await;
    assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");
    (jojobot, first_key, run)
}

#[tokio::test]
async fn p1_previous_host_sid_gets_finished_words_after_wrap() {
    let (jojobot, key, run) = wrapped_after_pickup().await;
    let refused = jojobot
        .caller(Some(&key))
        .expect_err("earlier key is retired");
    let text = json_of(&refused).to_string();
    assert!(
        text.contains("end the turn and do nothing")
            && text.contains("document:build-runtime")
            && text.contains(&run),
        "{text}"
    );
}

#[tokio::test]
async fn p1_previous_host_sid_cannot_boot_after_wrap() {
    let (jojobot, key, _) = wrapped_after_pickup().await;
    let refused = boot_args(
        &jojobot,
        serde_json::json!({"bot":"gamma", "sid":key, "resume":"new", "claim":"dev-dispatch"}),
    )
    .await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(
        refused.to_string().contains("end the turn and do nothing"),
        "{refused}"
    );
}
