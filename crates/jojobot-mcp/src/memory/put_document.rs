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
    /// section too long for one record is cut into pieces that read back whole.
    /// Reading it back with `read_document` returns these exact bytes.
    ///
    /// A `@kind:slug` outside a code span or fence links to a thing that exists
    /// and is refused when it names nothing; inside code it is literal.
    /// Use `@<collection>/<N>` to link the member holding N; a missing number
    /// is refused with the numbers held.
    #[serde(default)]
    pub(crate) markdown: Option<String>,
    /// **Put the document back as it stood at a moment, instead of writing a
    /// file**: a day, `YYYY-MM-DD`, meaning the end of that day in your zone, or
    /// an instant. Send this OR `markdown`, never both; with this, `after` and
    /// `markdown` are not needed. A section that differs is edited back to its
    /// old wording. One archived since, or with testimony in either version,
    /// comes back as a new record that names the old one in `derived_from`, and
    /// a standing one is archived. A section made since is archived. History is
    /// never rewritten. Read the moment first with `read_document` and `as_of`.
    #[serde(default)]
    pub(crate) restore_to: Option<String>,
    /// **The operator has confirmed a restore over their own words.** A restore
    /// that would change a section whose wording is testimony is refused whole
    /// without this, naming those sections. Only meaningful with `restore_to`.
    #[serde(default)]
    pub(crate) confirmed_by_user: Option<bool>,
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
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of a put of `markdown`**, because a call sent
    /// without one cannot be repeated safely. The put is one record per section:
    /// if an answer is lost or the store stops partway, send the same call again
    /// with the same `request_id` and the sections that landed are not written
    /// again and the rest are, in the places the first call chose. It needs a
    /// `sid`, and a call that wrote nothing spends no id. A `restore_to` takes
    /// none: putting a document back to a moment twice writes nothing the second
    /// time.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
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
                       for one record is cut at the last blank line, else line end, else \
                       character that fits, never inside a fenced block. The document \
                       must already exist (add_entity, kind document). Onto a document with \
                       sections the call is refused unless it names `after`, a section's \
                       address, so an edited file put twice cannot double the text. \
                       `restore_to` (a day or an instant) instead of `markdown` puts the \
                       document back as `read_document` showed it then: a section that differs is \
                       edited back to its old wording; one archived since, or with testimony in \
                       either version, comes back as a new record that names the old one as what it \
                       derives from, and a standing one is archived; one made since is \
                       archived; over testimony it needs \
                       `confirmed_by_user`. Every \
                       section's size and links are checked before any is written. \
                       Answers with a receipt, not the text. The receipt's `changed` lists changes, or is `nothing` or `not_read`. A refusal before any section \
                       lands is an answer with status: blocked and nothing written; one after \
                       some landed also carries the addresses that landed and where the rest begins \
                       (with a `request_id`, a count). A \
                       `request_id` makes a resend of a put finish the sections that did not \
                       land."
    )]
    pub(crate) async fn put_document(
        &self,
        Parameters(args): Parameters<PutDocumentArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **Resolved before the kind is judged**: a document that is a member of a
        // numbered collection is written by its number, as it is read.
        let subject = match self.current_handle(&EntityId::person(&args.document)).await {
            Ok(subject) => subject,
            Err(e) => return memory_declined("put_document", e),
        };
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
        let markdown = match (args.markdown.as_deref(), args.restore_to.as_deref()) {
            (Some(_), Some(_)) | (None, None) => {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(
                        "put_document writes a markdown file onto a document, or puts the \
                         document back as it stood with `restore_to`: send `markdown` or \
                         `restore_to`, not both and not neither"
                            .into(),
                    ),
                );
            }
            (None, Some(asked)) => {
                let recorded_at = self
                    .dated(args.recorded_at.as_deref(), args.sid.as_deref())
                    .await?;
                return self
                    .restore_document(
                        &subject,
                        asked,
                        args.confirmed_by_user.unwrap_or(false),
                        recorded_at,
                        &caller,
                    )
                    .await;
            }
            (Some(markdown), None) => markdown,
        };
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
        // before this one wrote would mint the same keys. **The lock is on the
        // thing the handle resolves to**, so a handle it used to wear shares it.
        let _placing = match self.hold_placing(&subject).await {
            Ok(guard) => guard,
            Err(e) => return memory_declined("put_document", e),
        };
        let held = match self.memory.recall(&subject).await {
            Ok(held) => held,
            Err(e) => return memory_declined("put_document", e),
        };
        let live = live_sections(&held);
        // **The plan an earlier call under this id froze**, kept in the note of its
        // first section: where the sections go, and the count that sized their
        // cut. A call that finds it is finishing the put, and what the document
        // holds now is its own first sections, so the checks below that read the
        // document are not asked again.
        let frozen = jojobot_domain::request::recorded("chunk-0")
            .and_then(|recorded| recorded.note)
            .and_then(|note| serde_json::from_str::<serde_json::Value>(&note).ok());
        // **The section a put goes behind.** Its trailing separator, with the new
        // section's lead, is what the new heading needs to start on a line of its
        // own.
        let mut behind: Option<(document::Edges, Option<String>)> = None;
        let (lower, upper) = match (frozen.is_some(), args.after.as_deref()) {
            (true, _) => (None, None),
            (false, None) if live.is_empty() => (None, None),
            (false, None) => {
                return memory_declined(
                    "put_document",
                    MemoryError::InvalidFact(format!(
                        "{} already has {} live sections, and putting a file onto it with no \
                         `after` would write its text a second time. Two ways forward: name \
                         `after`, the address of the section the new ones follow (`after` adds \
                         and replaces nothing; read_document lists the addresses), or edit a \
                         section that is there with update_fact on its address",
                        args.document,
                        live.len()
                    )),
                );
            }
            (false, Some(named)) => {
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
                behind = Some((
                    live[end].section.edges.clone(),
                    live.get(end + 1)
                        .map(|next| next.section.edges.lead.clone()),
                ));
                (
                    Some(live[end].order.clone()),
                    live.get(end + 1).map(|h| h.order.clone()),
                )
            }
        };

        let sections = match document::split(markdown) {
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
        let ordinal = frozen
            .as_ref()
            .and_then(|plan| plan["ordinal"].as_u64())
            .map_or(held.len() + markdown.len().min(99_999), |frozen| {
                frozen as usize
            });
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
        let sections_in_all = sections.len();
        for (at, section) in sections.into_iter().enumerate() {
            let pieces = match &room {
                Some(room) => document::fit(section.clone(), &mut |piece| {
                    self.record_passes(room, &record_of(piece, &widest), ordinal)
                }),
                None => Ok(vec![section.clone()]),
            };
            let pieces = match pieces {
                Ok(pieces) => pieces,
                Err(too_big) => {
                    // Only a store that measured a room can have refused a piece.
                    let (size, room) = room
                        .as_ref()
                        .map(|room| {
                            self.record_size_and_room(
                                room,
                                &record_of(&too_big.unit, &widest),
                                ordinal,
                            )
                        })
                        .unwrap_or_default();
                    return memory_declined(
                        "put_document",
                        section_too_big(
                            &args.document,
                            (at + 1, sections_in_all),
                            &section.first_line,
                            &too_big,
                            (size, room),
                        ),
                    );
                }
            };
            chunks.extend(pieces);
        }
        let cut_for_size = chunks.iter().filter(|c| c.continues).count();
        let frozen_orders: Option<Vec<String>> = frozen.as_ref().and_then(|plan| {
            plan["orders"]
                .as_array()?
                .iter()
                .map(|order| order.as_str().map(str::to_string))
                .collect()
        });
        let orders = match frozen_orders {
            Some(orders) if orders.len() == chunks.len() => Ok(orders),
            Some(_) => {
                return Ok(crate::caller::handle_declined(
                    "put_document",
                    crate::answer::WayForward::person(
                        "This request_id began a put whose plan no longer matches this text, so \
                         the rest cannot be finished under it. Put the rest with `after` and a \
                         new request_id.",
                    ),
                ));
            }
            None => order::spread(lower.as_deref(), upper.as_deref(), chunks.len()),
        };
        let orders = match orders {
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
        let mut facts: Vec<NewFact> = chunks
            .iter()
            .zip(&orders)
            .map(|(chunk, order)| record_of(chunk, order))
            .collect();
        // **Every section's words are screened before the first is written**, the
        // way a capture screens them: a handle that names nothing, or the stored
        // mark, refuses the whole put with nothing landed.
        let words: Vec<&str> = facts
            .iter()
            .flat_map(|new| [new.content.as_str(), new.details.as_deref().unwrap_or("")])
            .collect();
        let known = match jojobot_domain::memory::mention::roster_for(&*self.memory, &words).await {
            Ok(known) => known,
            Err(e) => return memory_declined("put_document", e),
        };
        for new in facts.iter().filter(|_| frozen.is_none()) {
            if let Some(refused) = jojobot_domain::memory::mention::refusal_for_words(
                &new.content,
                new.details.as_deref(),
                &known,
            ) {
                return memory_declined("put_document", refused);
            }
        }
        // **A put behind a section is joined to both neighbours by the one seam
        // function** ([`document::lead_behind`], [`document::tail_before`]): its
        // first section starts on a line of its own after a blank line, and its
        // last section ends so the section after it does too. The seams are written
        // on the new sections only, the lead of the first and the tail of the last,
        // so the section behind it and the section after it keep the edges their
        // writers gave them and read back the same.
        if let (Some((old, next_lead)), None) = (behind, frozen.as_ref()) {
            let edges_of = |new: &NewFact| {
                new.fields
                    .get(document::EDGES)
                    .and_then(|encoded| document::Edges::decode(encoded))
            };
            if let Some(mut edges) = facts.first().and_then(edges_of) {
                edges.lead = document::lead_behind(Some(&old.tail), &edges.lead);
                facts[0]
                    .fields
                    .insert(document::EDGES.to_string(), edges.encode());
            }
            let last = facts.len() - 1;
            if let Some(mut edges) = edges_of(&facts[last]) {
                edges.tail = document::tail_before(&edges.tail, next_lead.as_deref());
                facts[last]
                    .fields
                    .insert(document::EDGES.to_string(), edges.encode());
            }
        }
        // **A resend writes the edges the first call chose.** The seams are worked
        // out from the sections around the put as they stood then, and a resend
        // finishes the put in a place where the first sections have landed, so it
        // takes the edges from the plan and works nothing out again. A plan an
        // earlier build kept holds none, and its sections keep the edges they
        // were cut with.
        if let Some(kept) = frozen
            .as_ref()
            .and_then(|plan| plan["edges"].as_array())
            .filter(|kept| kept.len() == facts.len())
        {
            for (new, edges) in facts.iter_mut().zip(kept) {
                if let Some(edges) = edges.as_str() {
                    new.fields
                        .insert(document::EDGES.to_string(), edges.to_string());
                }
            }
        }
        // **What a resend must find, kept with the first section.**
        let plan = serde_json::json!({
            "orders": orders,
            "ordinal": ordinal,
            "edges": facts
                .iter()
                .map(|new| new.fields.get(document::EDGES).cloned())
                .collect::<Vec<_>>(),
        })
        .to_string();

        // **The document as it stands before the first section is written**,
        // for the receipt's account of which sections the write added.
        let before = self.as_it_stands(&subject).await;
        let mut landed: Vec<FactAddress> = Vec::new();
        let mut landed_bytes = 0;
        let mut fold_behind = None;
        // The record that holds the heading of the section being written: the link
        // every chunk that carries it on points at.
        let mut heading_record: Option<String> = None;
        for (at, (mut new, chunk)) in facts.into_iter().zip(&chunks).enumerate() {
            if chunk.continues {
                new.fields.insert(
                    document::CONTINUES.to_string(),
                    heading_record
                        .clone()
                        .expect("a chunk that carries a section on comes after its heading"),
                );
            }
            // **Each section is a part, written once under a `request_id`**, and
            // the first keeps the plan.
            let written = match jojobot_domain::request::part(
                &format!("chunk-{at}"),
                (at == 0).then(|| plan.clone()),
                self.memory.capture(new),
            )
            .await
            {
                jojobot_domain::request::Part::Ran(written) => written,
                // An earlier call wrote this section. Its address is in `landed`,
                // and the link the next section of the same heading needs is its
                // record.
                jojobot_domain::request::Part::Recorded(recorded) => {
                    let Some(address) = serde_json::from_str::<serde_json::Value>(&recorded.landed)
                        .ok()
                        .and_then(|landed| landed.get("address")?.as_str().map(str::to_string))
                        .and_then(|address| FactAddress::parse(&address).ok())
                    else {
                        return Ok(crate::caller::handle_declined(
                            "put_document",
                            crate::answer::WayForward::person(
                                "A section was written under this request_id, but its address \
                                 cannot be read back, so the rest cannot be finished.",
                            ),
                        ));
                    };
                    if !chunk.continues {
                        heading_record = Some(address.local.0.clone());
                    }
                    landed.push(address);
                    landed_bytes += document::join(std::slice::from_ref(chunk)).len();
                    continue;
                }
            };
            let refused = match written {
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
            // **With an id the put is not finished, and the same call finishes
            // it**: the answer says so, and `after` is not needed.
            if jojobot_domain::request::carries_an_id() && !landed.is_empty() {
                let cause = match &refused {
                    Ok(answered) => crate::stopped::Stopped::of_answer(answered),
                    Err(error) => crate::stopped::Stopped::of_error(error),
                };
                return Ok(crate::caller::handle_declined(
                    "put_document",
                    cause.unfinished(
                        &format!(
                            "{} of {} sections landed and stay.",
                            landed.len(),
                            chunks.len()
                        ),
                        "Send the same call again with the same request_id and it writes the \
                         rest, in the places the first call chose.",
                    ),
                ));
            }
            return stopped_midway(refused, &landed, chunks.len(), landed_bytes);
        }
        jojobot_domain::request::finished();

        self.beat("put_document", subject.as_str(), args.sid.as_deref())
            .await;
        let mut body = serde_json::json!({
            "document": subject.as_str(),
            "sections": landed.len(),
            "first": landed.first().map(ToString::to_string),
            "last": landed.last().map(ToString::to_string),
            "addresses": landed.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "bytes": markdown.len(),
        });
        if cut_for_size > 0 {
            body["cut_for_size"] = cut_for_size.into();
        }
        let resumed = crate::request::note_resumed(&mut body);
        let changed = match (fold_behind, resumed) {
            (None, false) => self.changed_on(&before, &subject).await,
            _ => crate::changed::Changed::not_read(),
        };
        crate::changed::note_changed(&mut body, changed);
        if let Some(behind) = fold_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        crate::answer::note_postcondition(
            &mut body,
            format!(
                "{} written on {}, in order, at the `addresses` listed. read_document returns the \
                 file; update_fact on a section's address edits that section alone, and with \
                 `after` moves it.",
                crate::answer::counted(landed.len(), "section", "sections"),
                subject
            ),
        );
        json_result(&body)
    }
}

/// **The refusal for a section no cut gets under a record**, built here for every
/// path that reaches [`document::TooBig`]. It names the section by its place and
/// its heading, says how big the piece in the way is against what one record on the
/// document can carry, and gives the two ways through. The cause is told apart: a
/// fenced block is never cut, while a piece with no room even for its first
/// character means the document's own name, aliases and keys take the room.
///
/// `at` is the section's place, counted from 1, and the number of sections in
/// the put; `sizes` is the piece as a record reads it and the room for one.
fn section_too_big(
    document: &str,
    at: (usize, usize),
    heading: &str,
    too_big: &document::TooBig,
    sizes: (usize, usize),
) -> MemoryError {
    let (place, of) = at;
    let (size, room) = sizes;
    let name = if heading.trim().is_empty() {
        format!("section {place} of {of} (the text before the first heading)")
    } else {
        format!("section {place} of {of} ('{}')", heading.trim())
    };
    MemoryError::InvalidFact(if too_big.fenced {
        format!(
            "{name} holds a fenced block that does not fit in one record: as a record reads \
             it, the block is {size} characters, and one record on {document} can carry \
             {room}. A fenced block is never cut. Two ways through: split the block into \
             several fenced blocks, each with its own opening and closing fence and each \
             under {room} characters; or keep the code as a page outside the store and put a \
             line in the section that says where it is"
        )
    } else {
        format!(
            "{name} cannot be stored: not one character of it fits beside the document's own \
             name, aliases and keys. A record on {document} would be {size} characters, and \
             one can carry {room}. Two ways through: shorten the name or the aliases of \
             {document} with update_entity; or take keys off it with update_fact and \
             clear_fields"
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(too_big: &document::TooBig, heading: &str) -> String {
        match section_too_big("document:homer", (3, 7), heading, too_big, (4_000, 900)) {
            MemoryError::InvalidFact(said) => said,
            other => panic!("an invalid fact is what a caller is told: {other:?}"),
        }
    }

    fn unit(fenced: bool) -> document::TooBig {
        let section = document::split("# Unit\n\nbody\n")
            .expect("splits")
            .remove(0);
        document::TooBig {
            unit: Box::new(section),
            fenced,
        }
    }

    /// **One builder serves both causes, and each says its own.** A piece with no
    /// room even for its first character is the document's own name, aliases and
    /// keys, so the ways through shrink those; a fenced block is the section's own
    /// text, so the ways through change the block. Each names the section by place
    /// and heading and gives the size against the room.
    #[test]
    fn the_refusal_names_the_cause_the_section_and_the_sizes() {
        let name_and_keys = refusal(&unit(false), "## Setup");
        assert!(name_and_keys.contains("section 3 of 7"), "{name_and_keys}");
        assert!(name_and_keys.contains("## Setup"), "{name_and_keys}");
        assert!(
            name_and_keys.contains("4000") && name_and_keys.contains("900"),
            "{name_and_keys}"
        );
        assert!(name_and_keys.contains("update_entity"), "{name_and_keys}");
        assert!(name_and_keys.contains("clear_fields"), "{name_and_keys}");
        assert!(!name_and_keys.contains("fenced"), "{name_and_keys}");

        let block = refusal(&unit(true), "## Setup");
        assert!(block.contains("section 3 of 7"), "{block}");
        assert!(block.contains("4000") && block.contains("900"), "{block}");
        assert!(block.contains("split"), "{block}");
        assert!(block.contains("outside"), "{block}");
        assert!(!block.contains("update_entity"), "{block}");
    }

    /// A section with no heading of its own (the text before the first heading)
    /// is named by its place and says so, instead of an empty pair of quotes.
    #[test]
    fn a_section_with_no_heading_is_named_by_its_place() {
        let said = refusal(&unit(true), "  ");
        assert!(said.contains("section 3 of 7"), "{said}");
        assert!(said.contains("before the first heading"), "{said}");
        assert!(!said.contains("''"), "{said}");
    }
}
