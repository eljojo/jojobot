//! Receipts for the four ordinary memory writes, before accounting and replay storage.

use rmcp::model::{CallToolRequestParams, CallToolResponse, ContentBlock};
use serde_json::{Map, Value, json};

tokio::task_local! {
    pub(crate) static TOKEN_SAVING: Option<crate::settings::TokenSaving>;
    pub(crate) static SINGLE_WRITE: Option<Call>;
}

pub(crate) fn single_write() -> bool {
    SINGLE_WRITE.try_with(Option::is_some).unwrap_or(false)
}

pub(crate) fn derived_value(key: &str, value: &Value) -> bool {
    SINGLE_WRITE
        .try_with(|call| {
            call.as_ref()
                .is_some_and(|call| call.sent(key) != Some(value))
        })
        .unwrap_or(false)
}

pub(crate) fn saving() -> bool {
    TOKEN_SAVING
        .try_with(|setting| setting.as_ref().is_some_and(|setting| setting.on))
        .unwrap_or(false)
}

/// Apply receipt mode before accounting and notices; lean leaves notices pending.
/// An unreadable setting keeps normal mode and says why.
pub(crate) fn compact(answered: &mut CallToolResponse) -> bool {
    if !TOKEN_SAVING.try_with(Option::is_some).unwrap_or(false) {
        return false;
    }
    let CallToolResponse::Complete(result) = answered else {
        return false;
    };
    if result.is_error == Some(true) {
        return false;
    }
    let Some(block) = result.content.first_mut() else {
        return false;
    };
    let Some(text) = block.as_text() else {
        return false;
    };
    let Ok(mut body) = serde_json::from_str::<Value>(&text.text) else {
        return false;
    };
    if body["status"] == "blocked" {
        return false;
    }
    let Some(id) = body.get("id").cloned() else {
        return false;
    };
    if !saving() {
        TOKEN_SAVING.with(|setting| {
            if let Some(setting) = setting {
                setting.explain(&mut body);
            }
        });
        *block = ContentBlock::text(body.to_string());
        return false;
    }
    let mut derived = body
        .get("derived")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(revision) = body.get("revision") {
        derived.insert("revision".into(), revision.clone());
    }
    let mut lean = json!({"id":id});
    if !derived.is_empty() {
        lean["derived"] = derived.into();
    }
    *block = ContentBlock::text(lean.to_string());
    true
}

#[derive(Clone)]
pub(crate) struct Call {
    verb: String,
    args: Map<String, Value>,
}

impl Call {
    pub(crate) fn from_request(request: &CallToolRequestParams) -> Option<Self> {
        if request
            .arguments
            .as_ref()
            .is_some_and(|args| args.contains_key("entries") || args.contains_key("ticks"))
        {
            return None;
        }
        matches!(
            request.name.as_ref(),
            "add_entity" | "capture" | "update_fact" | "update_entity"
        )
        .then(|| Self {
            verb: request.name.to_string(),
            args: request.arguments.clone().unwrap_or_default(),
        })
    }

    pub(crate) fn finish(&self, answered: &mut CallToolResponse) {
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        if result.is_error == Some(true) {
            return;
        }
        let Some(block) = result.content.first_mut() else {
            return;
        };
        let Some(text) = block.as_text() else {
            return;
        };
        let Ok(mut body) = serde_json::from_str::<Value>(&text.text) else {
            return;
        };
        if body["status"] == "blocked" {
            return;
        }
        let Some(id) = body.get("id").or_else(|| body.get("subject")).cloned() else {
            return;
        };
        let address = body
            .get("address")
            .or_else(|| body.pointer("/first_claim/address"))
            .or_else(|| body.pointer("/claim/address"))
            .cloned();
        body["success"] = true.into();
        body["id"] = id;
        if let Some(address) = address {
            body["address"] = address;
        }
        self.keep_new_information(&mut body);
        for key in ["first_claim", "claim"] {
            if let Some(claim) = body.as_object_mut().and_then(|body| body.remove(key))
                && body.get("how_to_read").is_none()
                && let Some(how) = claim.get("how_to_read")
            {
                body["how_to_read"] = how.clone();
            }
        }
        *block = ContentBlock::text(body.to_string());
    }

    fn sent(&self, key: &str) -> Option<&Value> {
        self.args
            .get("sets")
            .and_then(|bag| bag.get(key))
            .or_else(|| self.args.get("fields").and_then(|bag| bag.get(key)))
            .or_else(|| self.args.get(key))
    }

    fn keep_new_information(&self, body: &mut Value) {
        let mut derived = body
            .as_object_mut()
            .and_then(|body| body.remove("derived"))
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        for key in ["first_claim", "claim"] {
            if let Some(values) = body[key]["derived"].as_object() {
                derived.extend(values.clone());
            }
        }
        for (argument, returned) in [
            ("name", "name"),
            ("aliases", "alternateName"),
            ("source", "source"),
            ("crm", "crm"),
            ("parent", "parent"),
        ] {
            if let (Some(sent), Some(stored)) = (self.args.get(argument), body.get(returned))
                && sent != stored
            {
                derived.insert(argument.into(), stored.clone());
            }
        }
        if self.verb == "capture" {
            for key in ["provenance", "standing", "recorded_at"] {
                if !self.args.contains_key(key)
                    && let Some(value) = body.get(key)
                {
                    derived.insert(key.into(), value.clone());
                }
            }
        }
        if let Some(differences) = body.get("delta").and_then(Value::as_array) {
            let mut reasons = Vec::new();
            for difference in differences {
                if let (Some(key), Some(stored)) =
                    (difference["field"].as_str(), difference.get("stored"))
                {
                    derived.insert(key.into(), stored.clone());
                }
                if let Some(reason) = difference.get("because").filter(|v| !v.is_null()) {
                    reasons.push(reason.clone());
                }
            }
            if !reasons.is_empty() {
                warnings(body, reasons);
            }
        }
        if let Some(changes) = body.get("changed").and_then(Value::as_array) {
            let mut grouped: std::collections::BTreeMap<String, Value> =
                std::collections::BTreeMap::new();
            for change in changes {
                let Some(on) = change["on"].as_str() else {
                    continue;
                };
                let group = grouped
                    .entry(on.into())
                    .or_insert_with(|| json!({"on": on}));
                if let Some(key) = change["key"].as_str() {
                    let keys = group
                        .as_object_mut()
                        .expect("a change group")
                        .entry("keys")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .expect("keys are a list");
                    if !keys.iter().any(|held| held == key) {
                        keys.push(key.into());
                    }
                }
                if !matches!(change["what"].as_str(), Some("key" | "field")) {
                    let what = group
                        .as_object_mut()
                        .expect("a change group")
                        .entry("what")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .expect("changes are a list");
                    if !what.contains(&change["what"]) {
                        what.push(change["what"].clone());
                    }
                }
            }
            body["changed"] = grouped.into_values().collect::<Vec<_>>().into();
        }
        // A changed value the call did not supply is the result of
        // the write's arithmetic or normalisation, rather than an echo.
        derived.retain(|key, value| self.sent(key) != Some(value));
        if derived.contains_key("due_on") {
            body["falls_due"] = true.into();
        }
        trim_record(body);
        for key in ["first_claim", "claim"] {
            if let Some(claim) = body.get_mut(key) {
                trim_record(claim);
            }
        }
        if !derived.is_empty() {
            body["derived"] = derived.into();
        }
    }
}

fn trim_record(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };
    for key in [
        "name",
        "alternateName",
        "alternateName_elided",
        "alternateName_count",
        "type",
        "source",
        "crm",
        "boot",
        "parent",
        "subject",
        "content",
        "details",
        "content_head",
        "details_head",
        "content_bytes",
        "details_bytes",
        "content_elided",
        "details_elided",
        "fields",
        "sets",
        "fields_count",
        "sets_count",
        "fields_elided",
        "sets_elided",
        "inserted_at",
        "provenance",
        "standing",
        "status",
        "recorded_at",
        "happened_at",
        "happened_through",
        "stale_after",
        "derived_from",
        "refs",
        "stands_for",
        "edge",
        "receipt",
        "postcondition",
        "delta",
        "delta_note",
    ] {
        map.remove(key);
    }
}

/// Keep independent warnings when a write has more than one consequence.
pub(crate) fn warnings(body: &mut Value, lines: impl IntoIterator<Item = Value>) {
    let held = body
        .as_object_mut()
        .expect("a receipt object")
        .entry("warnings")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("warnings are a list");
    held.extend(lines);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TEST_SID;
    use crate::memory::testing::{Down, capture_args, capture_ok, ensure, healthy_and_down};
    use rmcp::model::{ClientCapabilities, ClientInfo, Implementation, ProtocolVersion};

    async fn served(handler: &crate::Jojobot, verb: &str, mut args: Value) -> Value {
        args["sid"] = TEST_SID.into();
        let request = serde_json::from_value(json!({
            "jsonrpc":"2.0", "id":1, "method":"tools/call",
            "params":{"name":verb, "arguments":args}
        }))
        .expect("a tool request");
        let (transport, mut replies) =
            rmcp::transport::OneshotTransport::<rmcp::RoleServer>::new(request);
        let mut info = ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("receipt-case", "0.0.1"),
        );
        info.protocol_version = ProtocolVersion::V_2024_11_05;
        let server = rmcp::service::serve_directly(handler.clone(), transport, Some(info));
        let reply = replies.recv().await.expect("a tool response");
        server.waiting().await.expect("the request ends");
        let reply = serde_json::to_value(reply).expect("a JSON response");
        let body: Value = serde_json::from_str(
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_else(|| panic!("an answer: {reply}")),
        )
        .expect("an answer body");
        assert_ne!(body["status"], "blocked", "{body}");
        body
    }

    async fn counter_derivation(lean: bool) {
        let handler = crate::harness::handler();
        ensure(&handler, "person:homer").await;
        served(
            &handler,
            "declare_type",
            json!({
                "name":"donut-counter", "fields":[{"key":"donuts", "holds":"number", "folds":"sum"}]
            }),
        )
        .await;
        served(
            &handler,
            "capture",
            json!({
                "subject":"person:homer", "content":"ate seven donuts", "sets":{"donuts":"7"}
            }),
        )
        .await;
        let second = served(
            &handler,
            "capture",
            json!({
                "subject":"person:homer", "content":"ate two donuts", "sets":{"donuts":"2"}
            }),
        )
        .await;
        if lean {
            capture_ok(
                &handler,
                crate::memory::capture::CaptureArgs {
                    sets: Some([("setting_token_saving".into(), "on".into())].into()),
                    ..capture_args("bot:otto", "Use lean receipts")
                },
            )
            .await;
        }
        let receipt = served(
            &handler,
            "update_fact",
            json!({
                "address":second["address"], "sets":{"donuts":"3"}
            }),
        )
        .await;
        let recalled = served(&handler, "recall", json!({"subject":"person:homer"})).await;
        assert_eq!(
            recalled["objects"][0]["fields"]["donuts"], "12",
            "{recalled}"
        );
        assert_eq!(receipt["derived"]["donuts"], "12", "{receipt}");
        if lean {
            assert!(
                receipt
                    .as_object()
                    .expect("a receipt")
                    .keys()
                    .all(|key| matches!(key.as_str(), "id" | "derived")),
                "{receipt}"
            );
        } else {
            assert_eq!(receipt["success"], true, "{receipt}");
        }
    }

    #[tokio::test]
    async fn normal_receipts_keep_a_counter_total_when_a_claim_echoes_the_input() {
        counter_derivation(false).await;
    }

    #[tokio::test]
    async fn lean_receipts_keep_a_counter_total_when_a_claim_echoes_the_input() {
        counter_derivation(true).await;
    }

    #[tokio::test]
    async fn lean_receipts_replay_a_faulted_setting_with_a_readable_control() {
        use crate::memory::testing::{DownMemory, InMemoryMemory};
        use jojobot_domain::request::testing::InMemoryRequests;
        use std::sync::Arc;

        let ledger = InMemoryRequests::new();
        let memory = Arc::new(InMemoryMemory::seeded().with_requests(ledger.clone()));
        let healthy = crate::harness::handler_over(memory.clone()).with_requests(ledger);
        ensure(&healthy, "person:alpha").await;
        capture_ok(
            &healthy,
            crate::memory::capture::CaptureArgs {
                sets: Some([("setting_token_saving".into(), "on".into())].into()),
                ..capture_args("bot:otto", "Use lean receipts")
            },
        )
        .await;
        let mut blind = healthy.clone();
        blind.memory = Arc::new(DownMemory(Down::ImageFields, memory));
        for (readable, handler) in [(true, &healthy), (false, &blind)] {
            let args = json!({
                "request_id":format!("setting-replay-{readable}"),
                "subject":"person:alpha", "content":format!("Reads the map, readable={readable}")
            });
            let first = served(handler, "capture", args.clone()).await;
            let replayed = served(handler, "capture", args).await;
            assert_eq!(replayed["id"], "person:alpha", "{replayed}");
            if readable {
                assert_eq!(first, replayed, "a lean replay preserves its receipt");
                assert!(
                    replayed
                        .as_object()
                        .expect("a receipt")
                        .keys()
                        .all(|key| matches!(key.as_str(), "id" | "derived")),
                    "{replayed}"
                );
            } else {
                for receipt in [&first, &replayed] {
                    assert_eq!(receipt["success"], true, "{receipt}");
                    assert_eq!(receipt["token_saving"]["mode"], "normal", "{receipt}");
                    assert_eq!(receipt["token_saving"]["setting_read"], false, "{receipt}");
                }
                assert_eq!(replayed["repeat"], true, "{replayed}");
            }
        }
        let read = served(
            &healthy,
            "recall",
            json!({"subject":"person:alpha", "facts":true}),
        )
        .await;
        assert_eq!(
            read["objects"][0]["facts"].as_array().map(Vec::len),
            Some(2),
            "each request id writes once: {read}"
        );
    }

    #[tokio::test]
    async fn lean_receipts_explain_an_unreadable_setting_over_the_served_tools() {
        let (healthy, blind) = healthy_and_down(Down::ImageFields);
        ensure(&healthy, "person:alpha").await;
        capture_ok(
            &healthy,
            crate::memory::capture::CaptureArgs {
                sets: Some([("setting_token_saving".into(), "on".into())].into()),
                ..capture_args("bot:otto", "Use lean receipts")
            },
        )
        .await;
        for (readable, handler) in [(true, healthy), (false, blind)] {
            let request = serde_json::from_value(json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":"capture", "arguments":{
                    "subject":"person:alpha", "content":"Reads the map", "sid":TEST_SID
                }}
            }))
            .expect("a tool request");
            let (transport, mut replies) =
                rmcp::transport::OneshotTransport::<rmcp::RoleServer>::new(request);
            let mut info = ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("receipt-case", "0.0.1"),
            );
            info.protocol_version = ProtocolVersion::V_2024_11_05;
            let server = rmcp::service::serve_directly(handler, transport, Some(info));
            let reply = replies.recv().await.expect("a tool response");
            server.waiting().await.expect("the request ends");
            let reply = serde_json::to_value(reply).expect("a JSON response");
            let body: Value = serde_json::from_str(
                reply["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap_or_else(|| panic!("a receipt: {reply}")),
            )
            .expect("a receipt body");
            assert_eq!(body["id"], "person:alpha", "{body}");
            if readable {
                assert!(
                    body.as_object()
                        .expect("a receipt")
                        .keys()
                        .all(|key| matches!(key.as_str(), "id" | "derived")),
                    "{body}"
                );
            } else {
                assert_eq!(body["success"], true, "{body}");
                assert_eq!(body["token_saving"]["mode"], "normal", "{body}");
                assert_eq!(body["token_saving"]["setting_read"], false, "{body}");
                let note = body["token_saving"]["note"]
                    .as_str()
                    .expect("an explanation");
                assert!(!note.is_empty() && !note.contains('\n') && !note.contains("  "));
            }
        }
    }
}
