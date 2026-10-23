//! Receipts for the four ordinary memory writes, before accounting and replay storage.

use rmcp::model::{CallToolRequestParams, CallToolResponse, ContentBlock};
use serde_json::Value;

pub(crate) struct Call;

impl Call {
    pub(crate) fn from_request(request: &CallToolRequestParams) -> Option<Self> {
        matches!(
            request.name.as_ref(),
            "add_entity" | "capture" | "update_fact" | "update_entity"
        )
        .then_some(Self)
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
        *block = ContentBlock::text(body.to_string());
    }
}
