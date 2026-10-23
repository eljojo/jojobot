//! Receipts for the four ordinary memory writes, before accounting and replay storage.

use rmcp::model::{CallToolRequestParams, CallToolResponse, ContentBlock};
use serde_json::{Map, Value, json};

pub(crate) struct Call {
    verb: String,
    args: Map<String, Value>,
}

impl Call {
    pub(crate) fn from_request(request: &CallToolRequestParams) -> Option<Self> {
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
            .get("derived")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
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
                    // A changed value the call did not supply is the result of
                    // the write's arithmetic or normalisation, rather than an echo.
                    if let Some(value) = change.get("to")
                        && self.sent(key) != Some(value)
                    {
                        derived.insert(key.into(), value.clone());
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
        if derived.contains_key("due_on") {
            body["falls_due"] = true.into();
        }
        if self.args.get("borrow") == Some(&Value::Bool(true))
            && let Some(line) = body.get("postcondition").cloned()
        {
            warnings(body, [line]);
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
