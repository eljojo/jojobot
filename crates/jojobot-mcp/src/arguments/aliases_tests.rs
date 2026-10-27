//! Guessed names reach the same served verb as its published spelling.

use crate::Jojobot;
use crate::harness::*;
use crate::mailboxes::testing::{counts, owning, send};
use crate::memory::testing::ensure;
use serde_json::{Value, json};

async fn served(jojobot: &Jojobot, name: &str, arguments: Value) -> Value {
    let message = serde_json::from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    }))
    .unwrap();
    let (transport, mut replies) = rmcp::transport::OneshotTransport::new(message);
    let running = rmcp::service::serve_directly(jojobot.clone(), transport, None);
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let reply = replies.recv().await.expect("the call answers");
            let wire = serde_json::to_value(reply).unwrap();
            if wire.get("id").is_some() {
                break wire;
            }
        }
    })
    .await
    .expect("an in-memory call finishes");
    running.cancel().await.unwrap();
    assert!(response.get("error").is_none(), "{response}");
    let result = &response["result"];
    if let Some(body) = result.get("structuredContent") {
        body.clone()
    } else {
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}

#[tokio::test]
async fn read_message_accepts_id_and_teaches_message_id_once() {
    let jojobot = handler();
    let sid = owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let sent = send(&jojobot, "otto", "gamma", "the requested message").await;
    let id = sent["id"].as_str().unwrap();
    let canonical = served(
        &jojobot,
        "read_message",
        json!({"message_id": id, "sid": sid}),
    )
    .await;
    assert_eq!(canonical["body"], "the requested message", "{canonical}");
    let guessed = served(&jojobot, "read_message", json!({"id": id, "sid": sid})).await;
    assert_eq!(guessed["body"], canonical["body"], "{guessed}");
    assert!(
        guessed["teaching"].to_string().contains("message_id"),
        "{guessed}"
    );
    let again = served(&jojobot, "read_message", json!({"id": id, "sid": sid})).await;
    assert_eq!(again["body"], canonical["body"], "{again}");
    assert!(
        !again["teaching"].to_string().contains("message_id"),
        "{again}"
    );
}

#[tokio::test]
async fn alias_collisions_and_unknown_names_still_refuse_without_delivery() {
    let jojobot = handler();
    let sid = owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    let sent = send(&jojobot, "otto", "gamma", "not delivered by a refused call").await;
    let id = sent["id"].as_str().unwrap();
    for arguments in [
        json!({"id": id, "message_id": id, "sid": sid}),
        json!({"id": "a different message", "message_id": id, "sid": sid}),
        json!({"id": id, "unknown_name": true, "sid": sid}),
    ] {
        let refused = served(&jojobot, "read_message", arguments).await;
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"].to_string().contains("message_id"),
            "{refused}"
        );
        assert_eq!(counts(&jojobot, "otto").await["counts"]["new"], 1);
    }
    let canonical = served(
        &jojobot,
        "read_message",
        json!({"message_id": id, "sid": sid}),
    )
    .await;
    assert_eq!(canonical["body"], "not delivered by a refused call");
}

async fn closing_alias(sent_name: &str, canonical: &str) {
    let jojobot = handler();
    let sid = owning(&jojobot, "otto").await;
    owning(&jojobot, "gamma").await;
    for name in [canonical, sent_name] {
        let sent = send(&jojobot, "otto", "gamma", "a job completed").await;
        let id = sent["id"].as_str().unwrap();
        let mut args = json!({"sid": sid});
        if canonical == "notes" {
            args["message_id"] = id.into();
            args[name] = "finished the job".into();
        } else {
            args[name] = if canonical == "message_ids" {
                json!([id])
            } else {
                json!(id)
            };
            args["notes"] = "finished the job".into();
        }
        let answer = served(&jojobot, "mark_processed", args).await;
        assert_ne!(answer["status"], "blocked", "{answer}");
        let stored = served(
            &jojobot,
            "read_message",
            json!({"message_id": id, "sid": sid}),
        )
        .await;
        assert_eq!(stored["state"], "processed", "{stored}");
        assert_eq!(stored["notes"], "finished the job", "{stored}");
        if name == sent_name {
            assert!(
                answer["teaching"].to_string().contains(canonical),
                "{answer}"
            );
        }
    }
}

#[tokio::test]
async fn mark_processed_accepts_message() {
    closing_alias("message", "message_id").await;
}

#[tokio::test]
async fn mark_processed_accepts_ids() {
    closing_alias("ids", "message_ids").await;
}

#[tokio::test]
async fn mark_processed_accepts_note() {
    closing_alias("note", "notes").await;
}

#[tokio::test]
async fn recall_accepts_handle() {
    let jojobot = handler();
    ensure(&jojobot, "thing:chain").await;
    for name in ["subject", "handle"] {
        let answer = served(
            &jojobot,
            "recall",
            json!({name: "thing:chain", "sid": TEST_SID}),
        )
        .await;
        assert_eq!(answer["objects"][0]["id"], "thing:chain", "{answer}");
        if name == "handle" {
            assert!(
                answer["teaching"].to_string().contains("subject"),
                "{answer}"
            );
        }
    }
}

async fn document_alias(name: &str) {
    let jojobot = handler();
    ensure(&jojobot, "document:chain").await;
    let markdown = "# Chain\nKeep it oiled.\n";
    let written = served(
        &jojobot,
        "put_document",
        json!({"document": "document:chain", "markdown": markdown, "sid": TEST_SID}),
    )
    .await;
    assert_ne!(written["status"], "blocked", "{written}");
    for spelling in ["document", name] {
        let answer = served(
            &jojobot,
            "read_document",
            json!({spelling: "document:chain", "sid": TEST_SID}),
        )
        .await;
        assert_eq!(answer["markdown"], markdown, "{answer}");
        if spelling == name {
            assert!(
                answer["teaching"].to_string().contains("document"),
                "{answer}"
            );
        }
    }
}

#[tokio::test]
async fn read_document_accepts_handle() {
    document_alias("handle").await;
}

#[tokio::test]
async fn read_document_accepts_subject() {
    document_alias("subject").await;
}

#[tokio::test]
async fn two_aliases_for_one_argument_are_refused_with_both_names() {
    let jojobot = handler();
    ensure(&jojobot, "document:chain").await;
    let canonical = served(
        &jojobot,
        "read_document",
        json!({"document": "document:chain", "sid": TEST_SID}),
    )
    .await;
    assert_eq!(canonical["document"], "document:chain", "{canonical}");
    let refused = served(
        &jojobot,
        "read_document",
        json!({"handle": "document:chain", "subject": "document:wheel", "sid": TEST_SID}),
    )
    .await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    let advice = refused["how_to_proceed"].as_str().unwrap();
    assert!(advice.contains("handle"), "{advice}");
    assert!(advice.contains("subject"), "{advice}");
}

#[tokio::test]
async fn add_entity_accepts_slug() {
    for name in ["handle", "slug"] {
        let jojobot = handler();
        let slug = "chain";
        let answer = served(&jojobot, "add_entity", json!({"kind": "thing", name: slug, "name": "Chain", "source": "user-named", "sid": TEST_SID})).await;
        assert_ne!(answer["status"], "blocked", "{answer}");
        let stored = served(
            &jojobot,
            "recall",
            json!({"subject": format!("thing:{slug}"), "sid": TEST_SID}),
        )
        .await;
        assert_eq!(
            stored["objects"][0]["id"],
            format!("thing:{slug}"),
            "{stored}"
        );
        if name == "slug" {
            assert!(
                answer["teaching"].to_string().contains("handle"),
                "{answer}"
            );
        }
    }
}

#[tokio::test]
async fn set_charter_accepts_charter() {
    let jojobot = handler();
    owning(&jojobot, "gamma").await;
    for (name, text) in [
        ("prose", "Keeps the plan."),
        ("charter", "Keeps the current plan."),
    ] {
        let answer = served(
            &jojobot,
            "set_charter",
            json!({"bot": "gamma", name: text, "sid": TEST_SID}),
        )
        .await;
        assert_ne!(answer["status"], "blocked", "{answer}");
        let stored = served(
            &jojobot,
            "recall",
            json!({"subject": "bot:gamma", "prose": true, "sid": TEST_SID}),
        )
        .await;
        assert_eq!(stored["objects"][0]["prose"], text, "{stored}");
        if name == "charter" {
            assert!(answer["teaching"].to_string().contains("prose"), "{answer}");
        }
    }
}
