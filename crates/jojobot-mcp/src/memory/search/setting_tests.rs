//! The caller's setting selects the answer shape, without a per-call switch.

use super::*;

#[tokio::test]
async fn search_lean_setting_unavailable_image_serves_normal_and_says_why() {
    let mut jojobot = indexed(vec![hit(fact("damper result", None))]);
    let sid = as_bot(&jojobot, "otto");
    set_mode(&jojobot, &sid, "bot:otto", "on").await;
    jojobot.memory = Arc::new(jojobot_adapters::fold::Folded::new(jojobot.memory.clone()));
    let body = answer(&jojobot, "damper", Some(sid)).await;
    assert_eq!(body["results"].as_array().unwrap().len(), 1, "{body}");
    assert!(body["matching"].is_object(), "{body}");
    assert_eq!(body["token_saving"]["mode"], "normal", "{body}");
    assert_eq!(body["token_saving"]["setting_read"], false, "{body}");
    assert!(body["token_saving"]["note"].is_string(), "{body}");
}

pub(super) async fn set_mode(jojobot: &Jojobot, _sid: &str, bot: &str, value: &str) {
    ensure(jojobot, bot).await;
    // Seed the setting without teaching the searching session through a write receipt.
    jojobot
        .memory
        .capture(jojobot_domain::memory::testing::contract::state(
            jojobot_domain::memory::NewFact {
                fields: [("setting_token_saving".into(), value.into())].into(),
                ..jojobot_domain::memory::NewFact::about(
                    EntityId(bot.into()),
                    "Token saving for this bot",
                    jiff::civil::date(2026, 7, 1),
                )
            },
        ))
        .await
        .unwrap()
        .written()
        .unwrap();
}

#[tokio::test]
async fn search_lean_setting_uses_the_image_through_the_served_memory_wrappers() {
    use jojobot_adapters::{counted::Counted, fold::Folded, search::IndexedMemory};
    use jojobot_domain::memory::mention::Mentioning;
    let mut jojobot = indexed(vec![hit(fact("damper result", None))]);
    let sid = as_bot(&jojobot, "otto");
    set_mode(&jojobot, &sid, "bot:otto", "on").await;
    let stored = jojobot.memory.clone();
    let counted = Arc::new(Counted::new(stored.clone()));
    let folded = Arc::new(Folded::new(counted.clone()));
    folded.rebuild().await.unwrap();
    jojobot.memory = Arc::new(IndexedMemory::new(Arc::new(Mentioning::new(folded))).unwrap());
    // A write below the fold distinguishes the held value from a store read.
    let mut writer = indexed(Vec::new());
    writer.memory = stored;
    let other = as_bot(&writer, "otto");
    set_mode(&writer, &other, "bot:otto", "off").await;
    counted.reset();
    let lean = answer(&jojobot, "damper", Some(sid)).await;
    assert_eq!(lean.as_object().unwrap().len(), 1, "{lean}");
    assert_eq!(lean["results"].as_array().unwrap().len(), 1, "{lean}");
    assert_eq!(counted.calls(), 0, "{:?}", counted.seen());
}

#[tokio::test]
async fn search_lean_setting_on_selects_results_only_and_off_keeps_unspent_notes() {
    let jojobot = indexed(vec![hit(fact("damper result", None))]);
    let sid = as_bot(&jojobot, "otto");
    set_mode(&jojobot, &sid, "bot:otto", "on").await;
    let lean = answer(&jojobot, "damper", Some(sid.clone())).await;
    assert_eq!(lean.as_object().unwrap().len(), 1, "{lean}");
    assert_eq!(lean["results"].as_array().unwrap().len(), 1, "{lean}");
    set_mode(&jojobot, &sid, "bot:otto", "off").await;
    let normal = answer(&jojobot, "damper", Some(sid)).await;
    assert!(normal["matching"].is_object(), "{normal}");
    assert!(
        normal["mail"]["note"].is_string(),
        "the lean call spent no note: {normal}"
    );
    assert_eq!(normal["results"], lean["results"]);
}

#[tokio::test]
async fn search_lean_setting_belongs_to_the_calling_bot_and_defaults_off() {
    let jojobot = indexed(vec![hit(fact("damper result", None))]);
    let otto = as_bot(&jojobot, "otto");
    let gamma = as_bot(&jojobot, "gamma");
    set_mode(&jojobot, &gamma, "bot:gamma", "on").await;
    let normal = answer(&jojobot, "damper", Some(otto)).await;
    assert!(normal["matching"].is_object(), "{normal}");
    let anonymous = answer(&jojobot, "damper", None).await;
    assert!(anonymous["matching"].is_object(), "{anonymous}");
    let lean = answer(&jojobot, "damper", Some(gamma)).await;
    assert_eq!(lean.as_object().unwrap().len(), 1, "{lean}");
}

#[test]
fn search_lean_setting_replaces_the_published_argument_and_refuses_it() {
    let jojobot = handler();
    let tools = Jojobot::tool_router().list_all();
    let search = tools.iter().find(|tool| tool.name == "search").unwrap();
    assert!(
        search.input_schema["properties"]
            .get("token_saving")
            .is_none()
    );
    let request = rmcp::model::CallToolRequestParams::new("search".to_string()).with_arguments(
        serde_json::json!({"query": "damper", "token_saving": true})
            .as_object()
            .unwrap()
            .clone(),
    );
    let refused = json_of(
        &jojobot
            .unimplemented_arguments(&request)
            .expect("the old argument is unknown"),
    );
    assert_eq!(refused["status"], "blocked");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .unwrap()
            .contains("token_saving")
    );
}
