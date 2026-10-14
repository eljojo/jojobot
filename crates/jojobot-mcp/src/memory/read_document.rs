//! `read_document` — Read a document back as the markdown it was put as.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::documents::{live_sections, section_text};
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
    /// **Read the document as it stood then**: a day, `YYYY-MM-DD`, meaning the
    /// end of that day in your zone, or an instant such as
    /// `2026-08-03T12:00:00Z`. Each section reads in the wording, place and
    /// archive state it had at that moment; a section made later is absent and
    /// one archived later is there. The answer names the moment it used. Leave it
    /// off to read the document as it stands.
    #[serde(default)]
    pub(crate) as_of: Option<String>,
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
                       never inside it. `as_of` (a day or an instant) reads the document as \
                       it stood then: each section in the wording, place and archive state it \
                       had, a section made later absent, and the answer names the moment it \
                       used. A document that does not exist is a blocked result with \
                       candidates."
    )]
    pub(crate) async fn read_document(
        &self,
        Parameters(args): Parameters<ReadDocumentArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
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
        let as_of = match args.as_of.as_deref() {
            None => None,
            Some(asked) => {
                let moment = match self.moment_of(asked, caller.as_ref()).await {
                    Ok(moment) => moment,
                    Err(e) => return memory_declined("read_document", e),
                };
                match self.facts_as_of(&subject, moment).await {
                    Ok(then) => Some((asked.trim().to_string(), then)),
                    Err(e) => return memory_declined("read_document", e),
                }
            }
        };
        let facts = match &as_of {
            Some((_, then)) => then.facts.clone(),
            None => match self.memory.recall(&subject).await {
                Ok(facts) => facts,
                Err(e) => return memory_declined("read_document", e),
            },
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
        // Every record that is a section and is not live: archived on its own, or
        // the chunk of a section whose heading was.
        let archived = facts
            .iter()
            .filter(|fact| fact.fields.contains_key(document::ORDER))
            .count()
            - live.len();
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
        if let Some((asked, then)) = &as_of {
            body["as_of"] = serde_json::json!({
                "asked": asked,
                "moment": then.moment.to_string(),
            });
            if !then.unplaced.is_empty() {
                // **Named, never vouched for**: these sections rest on a write
                // the store kept before it stamped its writes.
                body["as_of"]["unplaced"] = then
                    .unplaced
                    .iter()
                    .map(|id| serde_json::Value::from(id.0.as_str()))
                    .collect::<Vec<_>>()
                    .into();
            }
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
            let text = section_text(held);
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
