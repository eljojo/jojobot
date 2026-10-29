//! **This file is a contract, not a story.**
//!
//! The fixed synthetic receipt set. No operator day is replayed.
use super::dsl::Story;
use serde_json::{Value, json};

async fn fixed_set(lean: bool) -> Vec<(&'static str, Value)> {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-23").await;
    let s = story.session_on("2026-10-23", None).await;
    s.add("bot:gamma", "Gamma").await;
    s.add("project:atlas", "Atlas").await;
    s.add("person:milhouse", "Milhouse").await;
    if lean {
        s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    }
    let mut receipts: Vec<(&str, Value)> = Vec::new();
    let made = s
        .call(
            "add_entity",
            json!({
                "kind":"work", "handle":"work:sigma",
                "name":"Sigma",
                "source":"user-named", "parent":"project:atlas",
                "sets":{"owner":"bot:gamma", "status":"next", "runs_out":"2026-10-26"}
            }),
        )
        .await
        .json();
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
    let read = s
        .call("recall", json!({"subject":"person:alpha", "facts":true}))
        .await
        .json();
    let address = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|claim| claim["content"] == "plays chess")
        .expect("written claim")["address"]
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
            json!({"handle":"person:alpha", "name":"Delta", "aliases":["Zeta"]}),
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
    for lean in [false, true] {
        let receipts = fixed_set(lean).await;
        for (label, body) in &receipts {
            assert!(body["id"].is_string(), "{label}: {body}");
            if lean {
                assert!(
                    body.get("success").is_none() && body.get("address").is_none(),
                    "{label}: {body}"
                );
            } else {
                assert_eq!(body["success"], true, "{label}: {body}");
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
    }
}

#[tokio::test]
async fn lean_receipts_keep_derivations_and_changed_keys_without_echoes() {
    for lean in [false, true] {
        let receipts = fixed_set(lean).await;
        for (label, body) in &receipts {
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
                    !body.to_string().contains(&format!("\"{old}\"")),
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
        }
        if lean {
            for (label, body) in &receipts {
                assert!(
                    body.as_object()
                        .expect("receipt")
                        .keys()
                        .all(|key| matches!(key.as_str(), "id" | "derived")),
                    "{label}: {body}"
                );
            }
        }
        for (label, body) in &receipts {
            if let Some(changes) = body["changed"].as_array() {
                let mut handles = std::collections::BTreeSet::new();
                for change in changes {
                    assert!(
                        handles.insert(change["on"].as_str().expect("one handle per group")),
                        "{label}: {body}"
                    );
                    assert!(
                        change.get("to").is_none() && change.get("from").is_none(),
                        "{label}: {body}"
                    );
                }
            }
        }
        let f1 = &receipts[0].1;
        assert_eq!(f1["derived"]["due_on"], "2026-10-26", "{f1}");
        assert_eq!(receipts[2].1["derived"]["provenance"], "inference");
        assert!(
            receipts[5].1["derived"].get("provenance").is_none(),
            "an explicit provenance is not derived: {}",
            receipts[5].1
        );
        assert_eq!(f1.to_string().matches("2026-10-26").count(), 1, "{f1}");
        if !lean {
            assert_eq!(
                f1["changed"][0]["keys"],
                json!(["due_on", "owner", "runs_out", "status"]),
                "{f1}"
            );
        }
    }
}

#[tokio::test]
async fn lean_receipts_sets_only_edits_do_not_repeat_claim_labels() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let captured = s
        .call(
            "capture",
            json!({"subject":"thing:handcart", "content":"the wheel squeaks",
                "provenance":"inference", "sets":{"colour":"red"}}),
        )
        .await
        .json();
    let address = captured["address"].as_str().expect("claim address");
    let edited = s
        .call(
            "update_fact",
            json!({"address":address, "sets":{"colour":"blue"}}),
        )
        .await
        .json();
    for key in ["provenance", "standing"] {
        assert!(edited.get(key).is_none(), "{key}: {edited}");
    }
    let read = s.recall("thing:handcart").await;
    assert_eq!(read.json()["objects"][0]["fields"]["colour"], "blue");
    let claim = read.claim(address).json();
    assert_eq!(claim["content"], "the wheel squeaks");
    assert_eq!(claim["provenance"], "inference");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_details_only_edits_do_not_repeat_claim_labels() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let captured = s
        .call(
            "capture",
            json!({"subject":"thing:handcart", "content":"the wheel squeaks",
                "provenance":"inference"}),
        )
        .await
        .json();
    let address = captured["address"].as_str().expect("claim address");
    for (resend_words, details) in [(false, "checked empty"), (true, "checked with a load")] {
        let mut args = json!({"address":address, "details":details});
        if resend_words {
            args["content"] = "the wheel squeaks".into();
        }
        let edited = s.call("update_fact", args).await.json();
        for key in ["provenance", "standing"] {
            assert!(edited.get(key).is_none(), "{key}: {edited}");
        }
        let read = s.recall_with_details("thing:handcart").await;
        let claim = read.claim(address).json();
        assert_eq!(claim["content"], "the wheel squeaks");
        assert_eq!(claim["details"], details);
        assert_eq!(claim["provenance"], "inference");
    }
    story.finish().await;
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
async fn lean_receipts_review_derivations_name_mentions_without_echoing_refs() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("person:alpha", "Alpha"),
        ("person:beta", "Beta"),
        ("person:milhouse", "Milhouse"),
        ("person:nelson", "Nelson"),
    ] {
        s.add(handle, name).await;
    }
    let made = s.call("add_entity", json!({"kind":"thing", "handle":"kettle", "name":"The Kettle of @person:milhouse", "source":"user-named", "sets":{"colour":"white"}})).await.json();
    let captured = s.call("capture", json!({"subject":"person:alpha", "content":"plays with @person:milhouse beside `@person:nelson`", "refs":["person:beta"]})).await.json();
    s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let edited = s.call("update_fact", json!({"address":captured["address"], "content":"plays chess", "details":"with @person:nelson"})).await.json();
    let set = s
        .call(
            "update_entity",
            json!({"handle":"thing:kettle", "sets":{"colour":"blue"}}),
        )
        .await
        .json();
    for (body, expected) in [
        (&made, "person:milhouse"),
        (&captured, "person:milhouse"),
        (&edited, "person:nelson"),
        (&set, "thing:kettle"),
    ] {
        assert_eq!(body["derived"]["mentions"], json!([expected]), "{body}");
        assert!(body.get("refs").is_none(), "{body}");
        assert!(
            body.get("first_claim").is_none() && body.get("claim").is_none(),
            "{body}"
        );
    }
    let stored = s
        .call(
            "recall",
            json!({"subject":"person:alpha", "facts":true, "details":true}),
        )
        .await
        .json();
    assert_eq!(
        stored["objects"][0]["facts"][0]["refs"],
        json!(["person:beta"]),
        "{stored}"
    );
    s.call(
        "recall",
        json!({"subject":"person:alpha", "follow":{"direction":"out"}}),
    )
    .await
    .says("\"mention\":true")
    .says("\"ref\":true");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_review_derivations_warn_only_at_or_over_capacity() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("thing:jukebox", "The Jukebox").await;
    s.call(
        "capture",
        json!({"subject":"bot:gamma", "content":"room for two", "sets":{"thought_capacity":"2"}}),
    )
    .await;
    let gamma = story.as_bot("bot:gamma").await;
    let mut receipts = Vec::new();
    for (words, borrow) in [
        ("needs a needle", false),
        ("needs a record", false),
        ("needs a battery", true),
    ] {
        receipts.push(gamma.call("capture", json!({"subject":"bot:gamma", "content":words, "shape":"connection", "object":"thing:jukebox", "borrow":borrow})).await.json());
    }
    s.add("thread:milhouse-moves", "Milhouse moves").await;
    let mut records = Vec::new();
    for words in [
        "the movers sent a quote",
        "the movers sent a time",
        "the movers confirmed the day",
    ] {
        records.push(
            s.call(
                "capture",
                json!({"subject":"thread:milhouse-moves", "content":words}),
            )
            .await
            .json(),
        );
    }
    assert!(receipts[0].get("warnings").is_none(), "{}", receipts[0]);
    assert!(records[0].get("warnings").is_none(), "{}", records[0]);
    for body in [&receipts[1], &receipts[2], &records[2]] {
        assert!(
            body["warnings"]
                .as_array()
                .is_some_and(|lines| !lines.is_empty()),
            "{body}"
        );
        assert!(body.get("postcondition").is_none(), "{body}");
    }
    assert!(
        !receipts[2]["warnings"]
            .to_string()
            .contains("accounts now stand"),
        "{}",
        receipts[2]
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
    s.call("capture", json!({"subject":"bot:otto", "content":"Use lean receipts", "sets":{"setting_token_saving":"on"}})).await;
    let lean = s.call("capture", json!({"subject":"person:beta", "content":"read the ledger", "derived_from":source["address"]})).await.json();
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
