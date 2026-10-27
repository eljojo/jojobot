//! The calling bot chooses lean receipts through its held setting.
use super::dsl::Story;
use serde_json::{Value, json};

fn only_id_and_derivations(body: &Value) {
    assert!(body["id"].is_string(), "{body}");
    for key in body.as_object().expect("a receipt").keys() {
        assert!(matches!(key.as_str(), "id" | "derived"), "{body}");
    }
}

#[tokio::test]
async fn lean_receipts_derivations_survive_the_changed_count_limit() {
    let mut failures = Vec::new();
    for saving in [false, true] {
        let story = Story::begin("bot:otto").await;
        let s = story.session().await;
        if saving {
            s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
        }
        let short = s.call("add_entity", json!({"kind":"thing", "handle":"brass-bell", "name":"Brass bell", "source":"user-named", "sets":{"runs_out":"2026-10-26"}})).await.json();
        assert_eq!(short["derived"]["due_on"], "2026-10-26", "{short}");
        let mut sets = serde_json::Map::new();
        for n in 0..20 {
            sets.insert(format!("a{n:02}"), "short".into());
        }
        sets.insert("runs_out".into(), "2026-10-26".into());
        let many = s.call("add_entity", json!({"kind":"thing", "handle":"kettle", "name":"The Kettle", "source":"user-named", "sets":sets})).await.json();
        if many["derived"]["due_on"] != "2026-10-26" {
            failures.push(format!("saving={saving}: {many}"));
        }
        assert!(many["derived"].get("a00").is_none(), "{many}");
        assert!(many["derived"].get("runs_out").is_none(), "{many}");
        if saving {
            only_id_and_derivations(&many);
        }
        story.finish().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn lean_receipts_derivations_survive_an_elided_old_list_value() {
    let mut failures = Vec::new();
    for saving in [false, true] {
        let story = Story::begin("bot:otto").await;
        let s = story.session().await;
        s.call(
            "declare_type",
            json!({"name":"thing", "fields":[
                {"key":"tags", "holds":"list:text"},
                {"key":"short_tags", "holds":"list:text"}
            ]}),
        )
        .await;
        s.add("thing:kettle", "The Kettle").await;
        let long_item = "b".repeat(250);
        let old = format!("alpha, {long_item}");
        let long = s
            .call(
                "capture",
                json!({"subject":"thing:kettle", "content":"Long list", "sets":{"tags":old}}),
            )
            .await
            .json();
        let short = s.call("capture", json!({"subject":"thing:kettle", "content":"Short list", "sets":{"short_tags":"alpha, beta"}})).await.json();
        if saving {
            s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
        }
        let control = s
            .call(
                "update_fact",
                json!({"address":short["address"], "sets_remove":{"short_tags":"beta"}}),
            )
            .await
            .json();
        assert_eq!(control["derived"]["short_tags"], "alpha", "{control}");
        let changed = s
            .call(
                "update_fact",
                json!({"address":long["address"], "sets_remove":{"tags":long_item}}),
            )
            .await
            .json();
        if changed["derived"]["tags"] != "alpha" {
            failures.push(format!("saving={saving}: {changed}"));
        }
        if saving {
            only_id_and_derivations(&changed);
        }
        let read = s
            .call("recall", json!({"subject":"thing:kettle"}))
            .await
            .json();
        assert_eq!(read["objects"][0]["fields"]["tags"], "alpha", "{read}");
        story.finish().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn lean_receipts_token_saving_setting_covers_the_four_single_writes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let made = s.call("add_entity", json!({"kind":"thing", "handle":"kettle", "name":"The Kettle", "source":"user-named", "sets":{"runs_out":"2026-10-26"}})).await.json();
    only_id_and_derivations(&made);
    assert_eq!(made["id"], "thing:kettle", "{made}");
    assert_eq!(made["derived"]["due_on"], "2026-10-26", "{made}");
    let args =
        json!({"subject":"thing:kettle", "content":"holds tea", "request_id":"kettle-claim"});
    let captured = s.call("capture", args.clone()).await.json();
    only_id_and_derivations(&captured);
    assert_eq!(captured["id"], "thing:kettle", "{captured}");
    assert_eq!(captured["derived"]["provenance"], "inference", "{captured}");
    let repeated = s.call("capture", args).await.json();
    assert_eq!(repeated, captured, "{repeated}");
    let stored = s
        .call("recall", json!({"subject":"thing:kettle", "facts":true}))
        .await
        .json();
    let address = stored["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|claim| claim["content"] == "holds tea")
        .expect("written claim")["address"]
        .clone();
    let edited = s
        .call(
            "update_fact",
            json!({"address":address, "content":"holds coffee"}),
        )
        .await
        .json();
    only_id_and_derivations(&edited);
    assert_eq!(edited["id"], "thing:kettle", "{edited}");
    assert!(edited["derived"]["revision"].is_number(), "{edited}");
    let renamed = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "name":"The Silver Kettle"}),
        )
        .await
        .json();
    only_id_and_derivations(&renamed);
    assert_eq!(renamed["id"], "thing:kettle", "{renamed}");
    let set = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "sets":{"colour":"white"}}),
        )
        .await
        .json();
    only_id_and_derivations(&set);
    assert_eq!(set["id"], "thing:kettle", "{set}");
    s.call("recall", json!({"subject":"thing:kettle", "facts":true}))
        .await
        .says("holds coffee")
        .says("The Silver Kettle")
        .says("white");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_token_saving_defers_teaching_until_a_normal_receipt() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let setting = story.session().await;
    s.add("person:alpha", "Alpha").await;
    setting.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let lean = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess"}),
        )
        .await
        .json();
    only_id_and_derivations(&lean);
    setting.call("capture", json!({"subject":"bot:otto", "content":"Use normal receipts", "sets":{"setting_token_saving":"off"}})).await;
    let normal = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays go"}),
        )
        .await
        .json();
    assert!(normal["how_to_read"].is_string(), "{normal}");
    assert!(
        normal["teaching"]
            .as_array()
            .is_some_and(|lines| !lines.is_empty()),
        "{normal}"
    );
    let next = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays cards"}),
        )
        .await
        .json();
    assert!(next.get("how_to_read").is_none(), "{next}");
    assert!(next.get("teaching").is_none(), "{next}");
    assert!(next["address"].is_string(), "{next}");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_token_saving_leaves_unseen_session_notices_for_the_next_answer() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let setting = story.session().await;
    s.add("person:alpha", "Alpha").await;
    let first = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess"}),
        )
        .await
        .json();
    s.call("recall", json!({"subject":"person:alpha", "facts":true}))
        .await;
    let other = story.as_bot("bot:assistant").await;
    other
        .call(
            "update_fact",
            json!({"address":first["address"], "content":"plays go", "provenance":"inference"}),
        )
        .await;
    other.post("otto", "Chess", "The game changed.").await;
    setting.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let lean = s
        .call(
            "add_entity",
            json!({"kind":"person", "handle":"beta", "name":"Beta", "source":"user-named"}),
        )
        .await
        .json();
    only_id_and_derivations(&lean);
    let next = s
        .call("recall", json!({"subject":"person:beta"}))
        .await
        .json();
    assert_eq!(next["status_bar"]["mail_waiting"], 1, "{next}");
    assert!(
        next["held_changed"]
            .as_array()
            .is_some_and(
                |notices| notices.iter().any(|notice| notice["claims_edited"]
                    .as_array()
                    .is_some_and(|addresses| addresses.contains(&first["address"])))
            ),
        "{next}"
    );
    let again = s
        .call("recall", json!({"subject":"person:beta"}))
        .await
        .json();
    assert!(again.get("held_changed").is_none(), "{again}");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_token_saving_setting_defaults_off_and_belongs_to_the_caller() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let other = story.as_bot("bot:assistant").await;
    let unset = s
        .call(
            "capture",
            json!({"subject":"bot:otto", "content":"Read the map"}),
        )
        .await
        .json();
    assert_eq!(unset["success"], true, "{unset}");
    assert!(unset["address"].is_string(), "{unset}");
    other.call("capture", json!({"subject":"bot:assistant", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let ordinary = s
        .call(
            "capture",
            json!({"subject":"bot:assistant", "content":"Read the ledger"}),
        )
        .await
        .json();
    assert_eq!(ordinary["success"], true, "{ordinary}");
    assert!(ordinary["address"].is_string(), "{ordinary}");
    let lean = other
        .call(
            "capture",
            json!({"subject":"bot:otto", "content":"Read the timetable"}),
        )
        .await
        .json();
    only_id_and_derivations(&lean);
    assert_eq!(lean["id"], "bot:otto", "{lean}");
    other.call("capture", json!({"subject":"bot:assistant", "content":"Use normal receipts", "sets":{"setting_token_saving":"off"}})).await;
    let off = other
        .call(
            "capture",
            json!({"subject":"bot:otto", "content":"Read the roster"}),
        )
        .await
        .json();
    assert_eq!(off["success"], true, "{off}");
    assert!(off["address"].is_string(), "{off}");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_token_saving_old_argument_is_unpublished_and_refused_on_all_four_writes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "The Kettle").await;
    let captured = s
        .call(
            "capture",
            json!({"subject":"thing:kettle", "content":"Holds tea"}),
        )
        .await
        .json();
    let mut failures = Vec::new();
    let published = s.verbs_publishing("token_saving").await;
    if !published.is_empty() {
        failures.push(format!("published: {published:?}"));
    }
    for (verb, args) in [
        (
            "add_entity",
            json!({"kind":"thing", "handle":"brass-bell", "name":"Brass bell", "source":"user-named", "token_saving":true}),
        ),
        (
            "capture",
            json!({"subject":"thing:kettle", "content":"Holds coffee", "token_saving":true}),
        ),
        (
            "update_entity",
            json!({"handle":"thing:kettle", "name":"The Silver Kettle", "token_saving":true}),
        ),
        (
            "update_fact",
            json!({"address":captured["address"], "content":"Holds coffee", "token_saving":true}),
        ),
    ] {
        let body = s.attempt(verb, args).await;
        if body["status"] != "blocked"
            || !body["how_to_proceed"]
                .as_str()
                .is_some_and(|text| text.contains("token_saving"))
        {
            failures.push(format!("{verb}: {body}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    story.finish().await;
}
