//! A per-call small receipt defers explanations until they can be seen.
use super::dsl::Story;
use serde_json::{Value, json};

fn only_id_and_derivations(body: &Value) {
    assert!(body["id"].is_string(), "{body}");
    for key in body.as_object().expect("a receipt").keys() {
        assert!(matches!(key.as_str(), "id" | "derived"), "{body}");
    }
}

#[tokio::test]
async fn lean_receipts_token_saving_is_published_and_covers_the_four_single_writes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    assert_eq!(
        s.verbs_publishing("token_saving").await,
        ["add_entity", "capture", "update_entity", "update_fact"]
    );
    let made = s.call("add_entity", json!({"kind":"thing", "handle":"kettle", "name":"The Kettle", "source":"user-named", "sets":{"runs_out":"2026-10-26"}, "token_saving":true})).await.json();
    only_id_and_derivations(&made);
    assert_eq!(made["id"], "thing:kettle", "{made}");
    assert_eq!(made["derived"]["due_on"], "2026-10-26", "{made}");
    let args = json!({"subject":"thing:kettle", "content":"holds tea", "request_id":"kettle-claim", "token_saving":true});
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
            json!({"address":address, "content":"holds coffee", "token_saving":true}),
        )
        .await
        .json();
    only_id_and_derivations(&edited);
    assert_eq!(edited["id"], "thing:kettle", "{edited}");
    assert!(edited["derived"]["revision"].is_number(), "{edited}");
    let renamed = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "name":"The Silver Kettle", "token_saving":true}),
        )
        .await
        .json();
    only_id_and_derivations(&renamed);
    assert_eq!(renamed["id"], "thing:kettle", "{renamed}");
    let set = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "sets":{"colour":"white"}, "token_saving":true}),
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
    s.add("person:alpha", "Alpha").await;
    let lean = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess", "token_saving":true}),
        )
        .await
        .json();
    only_id_and_derivations(&lean);
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
    let lean = s.call("add_entity", json!({"kind":"person", "handle":"beta", "name":"Beta", "source":"user-named", "token_saving":true})).await.json();
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
