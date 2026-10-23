//! The fixed synthetic receipt set. No operator day is replayed.
use super::dsl::Story;
use serde_json::{Value, json};

#[tokio::test]
async fn lean_receipts_say_success_and_name_the_next_edit() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-23").await;
    let s = story.session_on("2026-10-23", None).await;
    s.add("bot:pm", "PM").await;
    s.add("project:right-fact-right-place", "Right fact right place")
        .await;
    s.add("person:milhouse", "Milhouse").await;
    let mut receipts: Vec<(&str, Value)> = Vec::new();
    let made = s.call("add_entity", json!({
        "kind":"work", "handle":"work:revisit-release-field-storage",
        "name":"Revisit next week: should a refused query like \"no thing among the 6 holds release\" change how jojobot stores records?",
        "source":"user-named", "parent":"project:right-fact-right-place",
        "sets":{"owner":"bot:pm", "status":"next", "runs_out":"2026-10-26"}
    })).await.json();
    receipts.push(("add_entity f1", made));
    receipts.push((
        "add_entity plain",
        s.call(
            "add_entity",
            json!({"kind":"person", "handle":"alpha", "name":"Alpha", "source":"user-named"}),
        )
        .await
        .json(),
    ));
    let plain = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess"}),
        )
        .await
        .json();
    let address = plain["address"]
        .as_str()
        .expect("claim address")
        .to_string();
    receipts.push(("capture plain", plain));
    for (label, args) in [
        (
            "capture sets",
            json!({"subject":"person:alpha", "content":"prefers the morning", "sets":{"time":"morning"}}),
        ),
        (
            "capture edge",
            json!({"subject":"person:alpha", "content":"knows Milhouse", "shape":"connection", "object":"person:milhouse"}),
        ),
        (
            "capture fields",
            json!({"subject":"person:alpha", "content":"read the timetable", "provenance":"observation", "fields":{"read_from":"timetable", "purpose":"note"}}),
        ),
        (
            "update_fact content",
            json!({"address":address, "content":"plays go"}),
        ),
        (
            "update_fact sets",
            json!({"address":address, "sets":{"time":"evening"}}),
        ),
        (
            "update_fact archive",
            json!({"address":address, "status":"archived", "details":"superseded"}),
        ),
        (
            "update_entity name aliases",
            json!({"handle":"person:alpha", "name":"Alpha Gamma", "aliases":["Gamma"]}),
        ),
        (
            "update_entity sets",
            json!({"handle":"person:alpha", "sets":{"time":"afternoon"}}),
        ),
    ] {
        let verb = label.split_whitespace().next().expect("verb");
        receipts.push((label, s.call(verb, args).await.json()));
    }
    for (label, body) in &receipts {
        assert_eq!(body["success"], true, "{label}: {body}");
        assert!(body["id"].is_string(), "{label}: {body}");
        if label.starts_with("capture")
            || label.starts_with("update_fact")
            || *label == "add_entity f1"
            || *label == "update_entity sets"
        {
            assert!(
                body["address"]
                    .as_str()
                    .is_some_and(|address| address.contains("#f")),
                "{label}: {body}"
            );
        }
    }
    let refused = s
        .refused(
            "capture",
            json!({"subject":"person:beta", "content":"plays chess"}),
        )
        .await
        .json();
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(refused.get("success").is_none(), "{refused}");
    story.finish().await;
}
