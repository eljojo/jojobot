mod support;

use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use jojobot_query_api::{Question, ViewOutcome};
use std::sync::Arc;
use support::ui_board::{seed, set_on};

async fn recall_board(memory: Arc<dyn Memory>, offset: Option<usize>) -> serde_json::Value {
    let idp = support::TestIdp::new();
    let (_, endpoints) =
        support::spawn_idp(idp.token_for(support::ui_board::READER, support::ui_board::CLIENT_ID))
            .await;
    let board = support::ui_board::seeded_board_over(memory).await;
    let (addr, cancel, _) =
        support::ui_board::spawn_jojobot_over(endpoints, &[support::ui_board::READER], &idp, board)
            .await;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("http://{addr}/mcp");
    let (session, _) = support::open_with(&http, &url, "2025-06-18").await;
    let mut args = serde_json::json!({"view":"board","inputs":{"project":"project:atlas"}});
    if let Some(offset) = offset {
        args["offset"] = offset.into();
        args["limit"] = 1.into();
        args["order"] = serde_json::json!({"key":"pitch"});
    }
    let mut request = http.post(url).header("content-type", "application/json").header("accept", "application/json, text/event-stream").json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"recall","arguments":args}}));
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let wire = request.send().await.unwrap().text().await.unwrap();
    let body = support::tool_body(&support::event_stream_result(&wire));
    cancel.cancel();
    body
}

#[tokio::test]
async fn view_read_path_windowed_board_keeps_the_full_count_and_status_words() {
    let (memory, _) = fixture().await;
    // A numeric order lets the graph read only the requested window.
    put(&*memory, "work:sigma", &[("pitch", "1")]).await;
    put(&*memory, "work:tau", &[("pitch", "2")]).await;
    let mut seen = std::collections::BTreeSet::new();
    for offset in [1, 0] {
        let body = recall_board(memory.clone(), Some(offset)).await;
        let objects = body["objects"].as_array().unwrap();
        assert_eq!(objects.len(), 1, "offset {offset}: {body}");
        assert_eq!(body["board"]["done"], 1, "offset {offset}: {body}");
        assert!(
            body["board"]["status_words"]
                .as_array()
                .unwrap()
                .iter()
                .any(|word| word == "unlisted"),
            "offset {offset}: {body}"
        );
        let object = &objects[0];
        assert_eq!(object["finished"], object["id"] == "work:sigma", "{object}");
        seen.insert(object["id"].as_str().unwrap().to_string());
    }
    assert_eq!(seen, ["work:sigma".into(), "work:tau".into()].into());
}

#[tokio::test]
async fn view_read_path_invalid_default_substitution_is_refused_with_its_form() {
    let memory = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    memory
        .add_entity(NewEntity::new(
            EntityId("view:alpha".into()),
            "Alpha",
            "the fixture roster",
        ))
        .await
        .unwrap();
    put(
        &*memory,
        "view:alpha",
        &[
            ("selects", "work"),
            ("inputs", "q: text? = a\"b"),
            ("where", "name = $q"),
        ],
    )
    .await;
    let queries = jojobot_mcp::memory::view_run::handle(
        memory,
        Arc::new(jojobot_domain::session::testing::InMemorySessions::default()),
    );
    let answer = queries
        .ask(
            &jojobot_domain::actor::Actor::Operator,
            named("view:alpha", &[]),
        )
        .await
        .unwrap_or_else(|_| panic!("saved view refusal"));
    let view = answer.view.unwrap();
    assert_eq!(view.inputs.len(), 1);
    assert!(matches!(view.outcome, ViewOutcome::Refused(_)));
}

async fn fixture() -> (Arc<dyn Memory>, jojobot_query_api::QueryHandle) {
    let supplied = jojobot_mcp::provisions();
    let memory: Arc<dyn Memory> = Arc::new(jojobot_adapters::provisioned::Provisioned::new(
        jojobot_domain::memory::testing::InMemoryMemory::booted().knowing(supplied.clone()),
        supplied,
    ));
    seed(memory.as_ref()).await;
    for (handle, parent, fields) in [
        (
            "project:atlas",
            None,
            vec![(
                "columns",
                "someday, next, now, waiting, done, review, queued",
            )],
        ),
        (
            "work:sigma",
            Some("project:atlas"),
            vec![("status", "done")],
        ),
        (
            "work:tau",
            Some("project:atlas"),
            vec![("status", "unlisted"), ("depends_on", "work:sigma")],
        ),
    ] {
        let mut entity = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
        entity.parent = parent.map(|p| EntityId(p.into()));
        memory.add_entity(entity).await.unwrap();
        put(&*memory, handle, &fields).await;
    }
    let queries = jojobot_mcp::memory::view_run::handle(
        memory.clone(),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::default()),
    );
    (memory, queries)
}

async fn put(memory: &dyn Memory, handle: &str, fields: &[(&str, &str)]) {
    let mut fact = NewFact::about(
        EntityId(handle.into()),
        "the view's values",
        jiff::civil::Date::constant(2026, 3, 10),
    );
    fact.fields = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    memory.capture(set_on(fact)).await.unwrap();
}

fn named(view: &str, inputs: &[(&str, &str)]) -> Question {
    Question::named_view(
        EntityId(view.into()),
        inputs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        jojobot_domain::clock::Clock::stating(jiff::civil::Date::constant(2026, 3, 10)),
    )
}

#[tokio::test]
async fn view_read_path_board_keeps_empty_and_unlisted_columns_and_counts_done() {
    let (memory, queries) = fixture().await;
    let answer = queries
        .ask(
            &jojobot_domain::actor::Actor::Operator,
            named("view:board", &[("project", "project:atlas")]),
        )
        .await
        .unwrap_or_else(|_| panic!("board read"));
    let board = answer
        .view
        .expect("the view's answer")
        .board
        .expect("board annotations");
    assert_eq!(
        board.status_words,
        [
            "someday", "next", "now", "waiting", "done", "review", "queued", "unlisted"
        ]
    );
    assert_eq!(board.done, 1);
    assert_eq!(board.finished, [EntityId("work:sigma".into())].into());
    assert_eq!(answer.selected.objects.len(), 2);
    let body = recall_board(memory, None).await;
    assert_eq!(body["objects"].as_array().unwrap().len(), 2, "{body}");
    assert_eq!(body["board"]["done"], 1, "{body}");
    assert!(
        body["board"]["status_words"]
            .as_array()
            .unwrap()
            .iter()
            .any(|word| word == "unlisted")
    );
    for object in body["objects"].as_array().unwrap() {
        assert_eq!(object["finished"], object["id"] == "work:sigma", "{object}");
    }
}

#[tokio::test]
async fn view_read_path_inputs_defaults_overrides_days_and_readiness() {
    let (memory, queries) = fixture().await;
    let mut rhythm = NewEntity::new(
        EntityId("rhythm:descale".into()),
        "Descale the machine",
        "the fixture roster",
    );
    rhythm.parent = Some(EntityId("person:alpha".into()));
    memory.add_entity(rhythm).await.unwrap();
    put(
        &*memory,
        "rhythm:descale",
        &[
            ("cadence_days", "7"),
            ("advances_from", "due_date"),
            ("counts_from", "2026-03-10"),
        ],
    )
    .await;
    let needs = queries
        .ask(
            &jojobot_domain::actor::Actor::Operator,
            named("view:board", &[]),
        )
        .await
        .unwrap_or_else(|_| panic!("input state"));
    assert!(matches!(
        needs.view.unwrap().outcome,
        ViewOutcome::NeedsInputs
    ));
    for (inputs, expected) in [
        (vec![], vec!["work:tau"]),
        (vec![("project", "project:atlas")], vec!["work:tau"]),
    ] {
        let answer = queries
            .ask(
                &jojobot_domain::actor::Actor::Operator,
                named("view:ready", &inputs),
            )
            .await
            .unwrap_or_else(|_| panic!("ready read"));
        assert!(matches!(
            answer.view.unwrap().outcome,
            ViewOutcome::Answered
        ));
        assert_eq!(
            answer
                .selected
                .objects
                .iter()
                .map(|o| o.entity.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
    for (day, refused) in [("2026-03-11", false), ("not-a-day", true)] {
        let answer = queries
            .ask(
                &jojobot_domain::actor::Actor::Operator,
                named("view:due", &[("by", day)]),
            )
            .await
            .unwrap_or_else(|_| panic!("day read"));
        assert_eq!(
            matches!(answer.view.unwrap().outcome, ViewOutcome::Refused(_)),
            refused
        );
    }
    for (inputs, includes) in [(vec![], true), (vec![("by", "2026-03-11")], false)] {
        let answer = queries
            .ask(
                &jojobot_domain::actor::Actor::Operator,
                named("view:due", &inputs),
            )
            .await
            .unwrap_or_else(|_| panic!("due read"));
        assert_eq!(
            answer
                .selected
                .objects
                .iter()
                .any(|o| o.entity.id.as_str() == "rhythm:descale"),
            includes
        );
    }
    let invalid = queries
        .ask(
            &jojobot_domain::actor::Actor::Operator,
            named("view:board", &[("project", "person:alpha")]),
        )
        .await
        .unwrap_or_else(|_| panic!("refusal"));
    assert!(matches!(
        invalid.view.unwrap().outcome,
        ViewOutcome::Refused(_)
    ));
}
