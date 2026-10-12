//! `read_document` — Read a document back as the markdown it was put as.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::documents::live_sections;
use super::*;
use jojobot_domain::memory::document;
use jojobot_domain::text::ANSWER_CEILING;

/// Arguments to `read_document`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadDocumentArgs {
    /// The document, `document:homer`.
    pub(crate) document: String,
    /// **The section to start from, counting from 0**, when a document is too
    /// long for one answer. The answer names the next one in `next_offset`.
    /// Leave it off to start at the top.
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Read a document as one markdown page, or as many parts as it takes.
#[tool_router(router = read_document_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Read a document as markdown: its live sections joined in order, byte \
                       for byte as they were put, as one markdown string. Archived sections are left \
                       out and counted. A document longer than one answer comes back in parts: \
                       the sections from `offset` that fit, with `next_offset` and a \
                       not_shown block naming the call that reads the rest. The document's \
                       own name, one_liner, stale_after and resource ride beside the text, \
                       never inside it. A document that does not exist is a blocked result \
                       with candidates."
    )]
    pub(crate) async fn read_document(
        &self,
        Parameters(args): Parameters<ReadDocumentArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let _ = caller;
        let subject = EntityId::person(&args.document);
        if subject.kind() != Some(EntityKind::DOCUMENT) {
            return memory_declined(
                "read_document",
                MemoryError::InvalidQuery(format!(
                    "'{}' is not a document: read_document reads a `document:homer` thing",
                    args.document
                )),
            );
        }
        let facts = match self.memory.recall(&subject).await {
            Ok(facts) => facts,
            Err(e) => return memory_declined("read_document", e),
        };
        let thing = match self.memory.scan_entity(&subject).await {
            Ok(Some(scanned)) => scanned,
            Ok(None) => {
                return memory_declined(
                    "read_document",
                    MemoryError::UnknownEntity {
                        attempted: subject.to_string(),
                        nearest: Vec::new(),
                    },
                );
            }
            Err(e) => return memory_declined("read_document", e),
        };
        let live = live_sections(&facts);
        let archived = facts
            .iter()
            .filter(|fact| fact.status != FactStatus::Active)
            .filter(|fact| fact.fields.contains_key(document::ORDER))
            .count();
        let offset = args.offset.unwrap_or(0).min(live.len());

        let mut body = serde_json::json!({
            "document": subject.as_str(),
            "name": thing.entity.as_ref().map(|entity| entity.name.clone()),
            "sections": live.len(),
            "archived_sections_left_out": (archived > 0).then_some(archived),
        });
        let fields: serde_json::Map<String, serde_json::Value> = thing
            .fields
            .iter()
            .filter(|(key, _)| ["one_liner", "stale_after", "resource"].contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), serde_json::Value::from(value.as_str())))
            .collect();
        if !fields.is_empty() {
            body["fields"] = fields.into();
        }
        if offset > 0 {
            body["offset"] = offset.into();
        }

        // What is spent before any text: the answer without it, the pointer to
        // what follows, and the status bar that joins the answer after the verb
        // returns.
        let mut bare = body.clone();
        bare["markdown"] = "".into();
        bare["next_offset"] = usize::MAX.into();
        let spent = crate::answer::served_len(&bare)
            + crate::answer::widest_not_shown("sections")
            + crate::answer::STATUS_BAR_ROOM;
        let mut room = ANSWER_CEILING.saturating_sub(spent);
        let mut page = String::new();
        let mut taken = 0;
        for held in &live[offset..] {
            let text = document::join(std::slice::from_ref(&held.section));
            // Counted as the JSON string it rides in, with its escapes.
            let cost = serde_json::Value::from(text.as_str())
                .to_string()
                .chars()
                .count()
                - 2;
            // One section always ships: a read that returned nothing would never
            // advance.
            if taken > 0 && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            page.push_str(&text);
            taken += 1;
        }
        body["markdown"] = page.into();
        let end = offset + taken;
        if end < live.len() {
            body["next_offset"] = end.into();
            body["not_shown"] = crate::answer::not_shown(live.len() - end, end, "sections");
        }
        json_result(&body)
    }
}
