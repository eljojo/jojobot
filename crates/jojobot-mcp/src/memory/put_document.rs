//! `put_document` — Write a markdown file onto a document, one record per section.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::documents::{live_sections, run_of};
use super::*;
use jojobot_domain::memory::document::{self, Section};
use jojobot_domain::memory::order;

/// **The widest order key a section can be minted.** The size check measures a
/// section's record with a key this long, so a key that is shorter once minted
/// never takes a record that passed over the line.
const WIDEST_ORDER_KEY: usize = 32;

/// Arguments to `put_document`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PutDocumentArgs {
    /// The document to write onto, `document:homer`. **It must already exist**:
    /// a document jojobot does not know comes back with candidates and nothing
    /// is written. Make it with `add_entity` first, kind `document`; its `name`
    /// is its title.
    pub(crate) document: String,
    /// **The whole markdown file**, as it should read back. It is split at its
    /// headings (`#` to `######`, not inside a fenced block of code) and each
    /// heading with what sits under it becomes one record on the document. A
    /// section too long for one record is cut at the last blank line that
    /// fits. Reading it back with `read_document` returns these exact bytes.
    ///
    /// A `@kind:slug` outside a code span or fence links to a thing that exists
    /// and is refused when it names nothing; inside code it is literal.
    pub(crate) markdown: String,
    /// **A section's address**, `document:homer#local-id`: write the new sections
    /// right after it. **Required when the document already has sections**, so
    /// that putting an edited file again cannot silently double the document.
    /// To change a section that is there, edit its record with `update_fact`;
    /// to take one away, archive its record.
    #[serde(default)]
    pub(crate) after: Option<String>,
    /// `testimony` (the operator said these words), `observation` (you READ
    /// them in a system of record, so `source` is required) or `inference`
    /// (you wrote or derived them; the default). One provenance covers every
    /// section of the call.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// **The system the document was read from**, for an `observation`: stored on
    /// every section as its `read_from`, the same field `capture` takes under that
    /// name. It is not where an entity came from, which is `add_entity`'s `source`.
    #[serde(default)]
    pub(crate) source: Option<String>,
    /// **What was read there**, when it has a reference: an id, a path, a line.
    /// Stored on every section as its `read_ref`, the field `capture` takes under
    /// that name.
    #[serde(default)]
    pub(crate) source_ref: Option<String>,
    /// The day the sections were made, `YYYY-MM-DD`. Defaults to the day your
    /// run is in.
    #[serde(default)]
    pub(crate) recorded_at: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Write a markdown file onto a document, one record per section.
#[tool_router(router = put_document_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Write a whole markdown file onto a document: it is split at its \
                       headings and every heading with what sits under it is stored as one \
                       claim on the document, in order, each with its own address, history and \
                       provenance. read_document returns the file byte for byte; update_fact \
                       edits one section and leaves the others untouched. A section too long \
                       for one record is cut at the last blank line that fits. The document \
                       must already exist (add_entity, kind document). Onto a document with \
                       sections the call is refused unless it names `after`, a section's \
                       address, so an edited file put twice cannot double the text. Every \
                       section is checked before any is written; a write that stops midway \
                       answers with the addresses that landed and where the rest begins. \
                       Answers with a receipt, not the text. A refusal before any section \
                       lands is an answer with status: blocked and nothing written; one after \
                       some landed also carries what landed and where the rest begins."
    )]
    pub(crate) async fn put_document(
        &self,
        Parameters(args): Parameters<PutDocumentArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let subject = EntityId::person(&args.document);
        if subject.kind() != Some(EntityKind::DOCUMENT) {
            return memory_declined(
                "put_document",
                MemoryError::InvalidFact(format!(
                    "'{}' is not a document: put_document writes onto a `document:homer` thing, \
                     made with add_entity (kind document)",
                    args.document
                )),
            );
        }
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let recorded_at = self
            .dated(args.recorded_at.as_deref(), args.sid.as_deref())
            .await?;
        let mut read_fields = std::collections::BTreeMap::new();
        if let Some(read_from) = args.source.as_deref().filter(|v| !v.trim().is_empty()) {
            read_fields.insert(
                jojobot_domain::memory::READ_FROM.to_string(),
                read_from.into(),
            );
        }
        if let Some(read_ref) = args.source_ref.as_deref().filter(|v| !v.trim().is_empty()) {
            read_fields.insert(
                jojobot_domain::memory::READ_REF.to_string(),
                read_ref.into(),
            );
        }
        // **Held from the read of the neighbours to the last write**: the keys are
        // minted from that read, and a second put on the same document that read
        // before this one wrote would mint the same keys.
        let _placing = self.registry.hold_placing(&subject).await;
        let held = match self.memory.recall(&subject).await {
            Ok(held) => held,
            Err(e) => return memory_declined("put_document", e),
        };
        let live = live_sections(&held);
        let (lower, upper) = match args.after.as_deref() {
            None if live.is_empty() => (None, None),
            None => {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(format!(
                        "{} already has {} live sections, and putting a file onto it with no \
                         `after` would write its text a second time. Two ways forward: name \
                         `after`, the address of the section the new ones follow, or edit a \
                         section that is there with update_fact on its address",
                        args.document,
                        live.len()
                    )),
                );
            }
            Some(named) => {
                let address = FactAddress::parse(named).map_err(memory_error)?;
                let Some(at) = live
                    .iter()
                    .position(|h| h.fact.home == address.home && h.fact.id == address.local)
                else {
                    return memory_declined(
                        "put_document",
                        MemoryError::InvalidFact(format!(
                            "'{named}' is not a live section of {}: `after` names the address \
                             of one of its sections. {}",
                            args.document,
                            if live.is_empty() {
                                "It has none yet, so leave `after` off".to_string()
                            } else {
                                format!("Its sections start at {}", live[0].fact.address())
                            }
                        )),
                    );
                };
                // The new text goes after the section and after every chunk that
                // carries it on, never between the two halves of one section.
                let end = *run_of(&live, at).last().expect("a run holds its section");
                (
                    Some(live[end].order.clone()),
                    live.get(end + 1).map(|h| h.order.clone()),
                )
            }
        };

        let sections = match document::split(&args.markdown) {
            Ok(sections) if sections.is_empty() => {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(
                        "the markdown is empty: there is nothing to put".into(),
                    ),
                );
            }
            Ok(sections) => sections,
            Err(e) => {
                return memory_declined("put_document", MemoryError::InvalidFact(e.to_string()));
            }
        };

        let room = self.room_for_records(&subject).await;
        let ordinal = held.len() + args.markdown.len().min(99_999);
        let widest = "z".repeat(WIDEST_ORDER_KEY);
        let record_of = |section: &Section, order: &str| {
            let mut new = NewFact::about(subject.clone(), section.first_line.clone(), recorded_at);
            new.details = (!section.body.is_empty()).then(|| section.body.clone());
            new.provenance = provenance;
            new.fields = document::record_fields(section, order);
            // The link to the heading's record is that record's id, which is not
            // known until it is written; the widest id stands in for it here.
            if section.continues {
                new.fields
                    .insert(document::CONTINUES.to_string(), "f99999".to_string());
            }
            new.fields.extend(read_fields.clone());
            new.session = Some(caller.sid.as_str().to_string());
            new
        };
        let mut chunks: Vec<Section> = Vec::new();
        for section in sections {
            let pieces = match &room {
                Some(room) => document::fit(section.clone(), &mut |piece| {
                    self.record_passes(room, &record_of(piece, &widest), ordinal)
                }),
                None => Ok(vec![section.clone()]),
            };
            let Ok(pieces) = pieces else {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(format!(
                        "no part of the section '{}' fits in a record on {}: the document's \
                         own name, aliases and keys leave no room, so shorten them \
                         with update_entity",
                        section.first_line, args.document
                    )),
                );
            };
            chunks.extend(pieces);
        }
        let cut_for_size = chunks.iter().filter(|c| c.continues).count();
        let orders = match order::spread(lower.as_deref(), upper.as_deref(), chunks.len()) {
            Ok(orders) => orders,
            Err(e) => {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(format!(
                        "no room is left to place the new sections there: {e}"
                    )),
                );
            }
        };
        let facts: Vec<NewFact> = chunks
            .iter()
            .zip(&orders)
            .map(|(chunk, order)| record_of(chunk, order))
            .collect();
        // **Every section's words are screened before the first is written**, the
        // way a capture screens them: a handle that names nothing, or the stored
        // mark, refuses the whole put with nothing landed.
        let known = match self.memory.list_entities(None).await {
            Ok(known) => known,
            Err(e) => return memory_declined("put_document", e),
        };
        for new in &facts {
            let words = [new.content.as_str(), new.details.as_deref().unwrap_or("")];
            if let Some(refused) = jojobot_domain::memory::mention::refusal_for_text(&words, &known)
            {
                return memory_declined("put_document", refused);
            }
        }

        let mut landed: Vec<FactAddress> = Vec::new();
        let mut landed_bytes = 0;
        let mut fold_behind = None;
        // The record that holds the heading of the section being written: the link
        // every chunk that carries it on points at.
        let mut heading_record: Option<String> = None;
        for (mut new, chunk) in facts.into_iter().zip(&chunks) {
            if chunk.continues {
                new.fields.insert(
                    document::CONTINUES.to_string(),
                    heading_record
                        .clone()
                        .expect("a chunk that carries a section on comes after its heading"),
                );
            }
            let refused = match self.memory.capture(new).await {
                Ok(Guarded::Written(fact)) => {
                    if !chunk.continues {
                        heading_record = Some(fact.id.0.clone());
                    }
                    landed.push(fact.address());
                    landed_bytes += document::join(std::slice::from_ref(chunk)).len();
                    continue;
                }
                Err(MemoryError::FoldBehind {
                    landed: Landed::Fact(fact),
                    behind,
                    ..
                }) => {
                    if !chunk.continues {
                        heading_record = Some(fact.id.0.clone());
                    }
                    landed.push(fact.address());
                    landed_bytes += document::join(std::slice::from_ref(chunk)).len();
                    fold_behind = Some(behind);
                    continue;
                }
                Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                }) => Ok(blocked_result(
                    &attempted,
                    &candidates,
                    Blocked::MustExist("put_document"),
                )),
                Err(e) => memory_declined("put_document", e),
            };
            return stopped_midway(refused, &landed, chunks.len(), landed_bytes);
        }

        self.beat("put_document", subject.as_str(), args.sid.as_deref())
            .await;
        let mut body = serde_json::json!({
            "document": subject.as_str(),
            "sections": landed.len(),
            "first": landed.first().map(ToString::to_string),
            "last": landed.last().map(ToString::to_string),
            "bytes": args.markdown.len(),
        });
        if cut_for_size > 0 {
            body["cut_for_size"] = cut_for_size.into();
        }
        if let Some(behind) = fold_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        crate::answer::note_postcondition(
            &mut body,
            format!(
                "{} written on {}, in order. read_document returns the file; update_fact on a \
                 section's address edits that section alone.",
                crate::answer::counted(landed.len(), "section", "sections"),
                subject
            ),
        );
        json_result(&body)
    }
}

/// **A refusal that came after some sections had landed**, carrying what landed
/// and where the rest of the text begins. Nothing is taken back behind the
/// caller: the sections that are there stay, and `after` the last of them is
/// where a second call writes the rest.
///
/// A store that failed answers as an error whose stock words say nothing was
/// written, which is false here, so those words are replaced.
fn stopped_midway(
    refused: Result<CallToolResult, McpError>,
    landed: &[FactAddress],
    total: usize,
    landed_bytes: usize,
) -> Result<CallToolResult, McpError> {
    if landed.is_empty() {
        return refused;
    }
    let to_finish = format!(
        "{} of {} sections landed and stay. Put the markdown from byte {landed_bytes} on, with \
         after: {}",
        landed.len(),
        total,
        landed.last().map(ToString::to_string).unwrap_or_default()
    );
    let carried = serde_json::json!({
        "landed": landed.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "not_written": total - landed.len(),
        "rest_begins_at_byte": landed_bytes,
        "to_finish": to_finish,
    });
    super::documents::stopped_with(
        refused,
        carried,
        format!("the store stopped partway: {to_finish}"),
    )
}
