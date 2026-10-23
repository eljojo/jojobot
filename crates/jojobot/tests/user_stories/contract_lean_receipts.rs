//! The fixed synthetic receipt set. No operator day is replayed.
use super::dsl::Story;
use serde_json::{Value, json};

async fn fixed_set() -> Vec<(&'static str, Value)> {
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
    receipts
}

#[tokio::test]
async fn lean_receipts_say_success_and_name_the_next_edit() {
    let receipts = fixed_set().await;
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
}

#[tokio::test]
async fn lean_receipts_keep_derivations_and_changed_keys_without_echoes() {
    let receipts = fixed_set().await;
    for (label, body) in &receipts {
        let text = body.to_string();
        for old in [
            "content_head",
            "content_bytes",
            "content_elided",
            "details_head",
            "details_bytes",
            "details_elided",
            "fields_count",
            "fields_elided",
            "sets_count",
            "sets_elided",
            "inserted_at",
        ] {
            assert!(
                !text.contains(&format!("\"{old}\"")),
                "{label}: echoed {old}: {body}"
            );
        }
        for old in [
            "name",
            "source",
            "type",
            "boot",
            "alternateName",
            "parent",
            "edge",
            "refs",
            "subject",
        ] {
            assert!(body.get(old).is_none(), "{label}: {old}: {body}");
        }
        if let Some(changes) = body["changed"].as_array() {
            let mut handles = std::collections::BTreeSet::new();
            for change in changes {
                assert!(
                    handles.insert(change["on"].as_str().expect("one handle per group")),
                    "{label}: repeated handle: {body}"
                );
                assert!(
                    change.get("to").is_none() && change.get("from").is_none(),
                    "{label}: echoed change: {body}"
                );
            }
        }
    }
    let f1 = &receipts[0].1;
    assert_eq!(f1["derived"]["due_on"], "2026-10-26", "{f1}");
    assert_eq!(f1.to_string().matches("2026-10-26").count(), 1, "{f1}");
    assert_eq!(
        f1["changed"][0]["keys"],
        json!(["due_on", "owner", "runs_out", "status"]),
        "{f1}"
    );
    assert_eq!(receipts[2].1["derived"]["provenance"], "inference");
    assert!(
        receipts[5].1["derived"].get("provenance").is_none(),
        "an explicit provenance is not returned: {}",
        receipts[5].1
    );
}

#[tokio::test]
async fn lean_receipts_show_generated_claims_only_when_asked() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let plain = s.call("add_entity", json!({"kind":"thing", "handle":"kettle", "name":"The Kettle", "source":"user-named", "sets":{"colour":"white"}})).await.json();
    assert!(plain.get("first_claim").is_none(), "{plain}");
    assert_eq!(plain["address"], "thing:kettle#f1", "{plain}");
    let asked = s.call("add_entity", json!({"kind":"thing", "handle":"handcart", "name":"The Handcart", "source":"user-named", "sets":{"colour":"red"}})).await.json();
    assert!(asked.get("first_claim").is_none(), "{asked}");
    let read = s
        .call("recall", json!({"subject":"thing:handcart", "facts":true}))
        .await
        .json();
    assert_eq!(
        read["objects"][0]["facts"][0]["address"], asked["address"],
        "{read}"
    );
    let edited = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "sets":{"colour":"blue"}}),
        )
        .await
        .json();
    assert!(edited.get("claim").is_none(), "{edited}");
    assert!(edited["address"].is_string(), "{edited}");
    let asked_edit = s
        .call(
            "update_entity",
            json!({"handle":"thing:handcart", "sets":{"colour":"silver"}}),
        )
        .await
        .json();
    assert!(asked_edit.get("claim").is_none(), "{asked_edit}");
    let read = s
        .call("recall", json!({"subject":"thing:handcart", "facts":true}))
        .await
        .json();
    assert!(
        read["objects"][0]["facts"]
            .as_array()
            .expect("claims")
            .iter()
            .any(|claim| claim["address"] == asked_edit["address"]),
        "{read}"
    );
    let stored = s
        .call("recall", json!({"subject":"thing:kettle", "facts":true}))
        .await
        .json();
    assert_eq!(
        stored["objects"][0]["facts"]
            .as_array()
            .expect("stored claims")
            .len(),
        2,
        "{stored}"
    );
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_keep_archived_source_warnings_on_normal_writes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:alpha", "Alpha").await;
    s.add("person:beta", "Beta").await;
    let source = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess"}),
        )
        .await
        .json();
    let active = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays go"}),
        )
        .await
        .json();
    s.call(
        "update_fact",
        json!({"address":source["address"], "status":"archived"}),
    )
    .await;
    let control = s.call("capture", json!({"subject":"person:beta", "content":"read the map", "derived_from":active["address"]})).await.json();
    assert!(control.get("warnings").is_none(), "{control}");
    let captured = s.call("capture", json!({"subject":"person:beta", "content":"read the timetable", "derived_from":source["address"]})).await.json();
    let edited = s
        .call(
            "update_fact",
            json!({"address":captured["address"], "content":"read the roster"}),
        )
        .await
        .json();
    let lean = s.call("capture", json!({"subject":"person:beta", "content":"read the ledger", "derived_from":source["address"], "token_saving":true})).await.json();
    assert_eq!(lean["id"], "person:beta", "{lean}");
    assert!(
        lean.as_object()
            .expect("receipt")
            .keys()
            .all(|key| matches!(key.as_str(), "id" | "derived")),
        "{lean}"
    );
    let missing: Vec<_> = [captured, edited]
        .into_iter()
        .filter(|body| {
            !body["warnings"].as_array().is_some_and(|lines| {
                lines.iter().any(|line| {
                    line.as_str().is_some_and(|line| {
                        line.contains(source["address"].as_str().expect("source address"))
                    })
                })
            })
        })
        .collect();
    assert!(
        missing.is_empty(),
        "archived-source warnings missing: {missing:?}"
    );
    story.finish().await;
}
