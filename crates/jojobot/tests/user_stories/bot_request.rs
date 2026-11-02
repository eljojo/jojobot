//! **A manager asks for a bot, and only the app bound to the request reports on
//! it.** A manager writes one record naming the bot, what should run and the app
//! that watches it; a bot that is not above the target is refused, and the
//! refusal names who may. The bound app finds its request by listing the
//! requests bound to it, writes what it observed, and no bot writes that.

use jojobot_domain::app::operations::OperationSpec;
use serde_json::json;

use super::dsl::Story;

const APP: &str = "beta";

fn status_keys() -> Vec<String> {
    [
        "observed_state",
        "observed_generation",
        "runtime_ref",
        "last_result",
    ]
    .map(String::from)
    .to_vec()
}

fn spec(target: &str, machine: &str) -> serde_json::Value {
    json!({
        "target_bot": target, "desired": "running", "machine": machine,
        "model": "model-a", "bound_app": APP,
    })
}

fn creating(spec: serde_json::Value) -> serde_json::Value {
    json!({"kind": "bot-request", "handle": "alpha", "name": "Homer runs",
           "source": "user-named", "sets": spec})
}

fn bound_to_the_app() -> serde_json::Value {
    json!({"kind": "bot-request", "fields": [{"key": "bound_app", "value": APP}]})
}

#[tokio::test]
async fn a_manager_asks_for_a_bot_and_only_its_app_reports_on_it() {
    let (story, token) = Story::begin_serving_an_app(
        "bot:marge",
        APP,
        vec![
            OperationSpec {
                operation: "recall".into(),
                kinds: vec![],
                keys: vec![],
            },
            OperationSpec {
                operation: "capture".into(),
                kinds: vec!["bot-request".into()],
                keys: status_keys(),
            },
        ],
    )
    .await;
    let marge = story.session().await;
    marge
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "homer", "name": "Homer",
                   "source": "user-named", "sets": {"reports_to": "bot:marge"}}),
        )
        .await;
    marge.add("bot:bart", "Bart").await;
    marge.add("machine:theta", "Theta").await;
    let homer = story.as_bot("bot:homer").await;
    let bart = story.as_bot("bot:bart").await;

    // ── 1. a bot that is not above the target is refused, and told who may ──
    bart.refused("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await
        .says("\"wrote\":false")
        .says("bot:marge");

    // ── 2. an unknown bot or machine in the spec is refused at the write ────
    marge
        .refused("add_entity", creating(spec("bot:omega", "machine:theta")))
        .await
        .says("\"wrote\":false");
    marge
        .refused("add_entity", creating(spec("bot:homer", "machine:omega")))
        .await
        .says("\"wrote\":false");

    // ── 2b. a target with no manager on the chart has no writer, whoever asks ─
    marge
        .refused("add_entity", creating(spec("bot:marge", "machine:theta")))
        .await
        .says("\"wrote\":false")
        .says("KeyNotYours");

    // ── 3. the manager above the target asks for the bot ───────────────────
    marge
        .call("add_entity", creating(spec("bot:homer", "machine:theta")))
        .await;

    // ── 4. the bound app lists the requests bound to it and finds it ───────
    let (status, listed) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(status, 200, "{listed}");
    let found = &listed["objects"][0];
    assert_eq!(found["handle"], "bot-request:alpha", "{listed}");
    assert_eq!(found["fields"]["desired"], "running", "{listed}");
    assert_eq!(found["fields"]["generation"], "1", "{listed}");

    // ── 5. a spec edit is the manager's alone, and it raises the generation ─
    bart.refused(
        "capture",
        json!({"subject": "bot-request:alpha", "content": "stop it",
               "sets": {"desired": "stopped"}}),
    )
    .await
    .says("\"wrote\":false")
    .says("bot:marge");
    homer
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "stop me",
                   "sets": {"desired": "stopped"}}),
        )
        .await
        .says("\"wrote\":false");
    marge
        .call(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Stop the bot.",
                   "sets": {"desired": "stopped"}}),
        )
        .await;
    let (_, edited) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(
        edited["objects"][0]["fields"]["desired"], "stopped",
        "{edited}"
    );
    assert_eq!(
        edited["objects"][0]["fields"]["generation"], "2",
        "{edited}"
    );

    // ── 6. no bot sends the generation, and the target never moves ─────────
    marge
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Skip ahead.",
                   "sets": {"generation": "9"}}),
        )
        .await
        .says("\"wrote\":false");
    marge
        .refused(
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Another bot.",
                   "sets": {"target_bot": "bot:bart"}}),
        )
        .await
        .says("\"wrote\":false");

    // ── 7. only the bound app writes status; no bot does, whoever it is ─────
    let status_of = json!({"subject": "bot-request:alpha", "content": "Seen.",
                           "sets": {"observed_state": "stopped", "observed_generation": "2"}});
    marge
        .refused("capture", status_of.clone())
        .await
        .says("\"wrote\":false");
    homer
        .refused("capture", status_of.clone())
        .await
        .says("\"wrote\":false");
    let (written, body) = story.app_calls(&token, "capture", status_of).await;
    assert_eq!(written, 200, "{body}");
    let (_, seen) = story.app_calls(&token, "recall", bound_to_the_app()).await;
    assert_eq!(
        seen["objects"][0]["fields"]["observed_generation"], "2",
        "{seen}"
    );
    assert_eq!(seen["objects"][0]["fields"]["generation"], "2", "{seen}");

    // ── 8. the app writes no spec, and anyone adds a note ──────────────────
    let (refused, body) = story
        .app_calls(
            &token,
            "capture",
            json!({"subject": "bot-request:alpha", "content": "Run it.",
                   "sets": {"desired": "running"}}),
        )
        .await;
    assert_ne!(refused, 200, "{body}");
    bart.call(
        "capture",
        json!({"subject": "bot-request:alpha", "content": "A note from a bystander."}),
    )
    .await;
    story.finish().await;
}

/// **A manager sends a `reports_to` beside the target, and the target's own
/// chain still judges the request.** Marge manages homer and not lisa. She names
/// homer as the manager and lisa as the target, so the request is asked for a
/// bot nobody above her authorised. Bart manages lisa and makes the same write.
#[tokio::test]
async fn naming_a_manager_beside_the_target_does_not_change_who_judges_it() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge.add("bot:bart", "Bart").await;
    marge.add("machine:theta", "Theta").await;
    let bart = story.as_bot("bot:bart").await;
    for (by, handle, name, boss) in [
        (&marge, "homer", "Homer", "bot:marge"),
        (&bart, "lisa", "Lisa", "bot:bart"),
    ] {
        by.call(
            "add_entity",
            json!({"kind": "bot", "handle": handle, "name": name,
                   "source": "user-named", "sets": {"reports_to": boss}}),
        )
        .await;
    }
    let mut asked = spec("bot:lisa", "machine:theta");
    asked["reports_to"] = json!("bot:homer");

    marge
        .refused("add_entity", creating(asked.clone()))
        .await
        .says("\"wrote\":false")
        .says("bot:bart");
    bart.call("add_entity", creating(asked)).await;
    story.finish().await;
}
