//! **What a write changed** — the one `changed` block every write verb's
//! receipt carries (rule 133, rule 180, card 1333).
//!
//! A caller that writes knows what it sent. It does not know what the write
//! did: which names, keys, claims or states moved, and from what. The server
//! holds both sides at the moment of the write, so the receipt says it, and a
//! caller never reads the thing again to find out.
//!
//! **One shape on every write verb, built here and nowhere else** (rule 498).
//! `changed` is a list of entries, each `{on, what, key?, from?, to?,
//! bytes_before?, bytes_after?}`, or the word `nothing` when the call changed
//! nothing. An entry with no `from` names something that was not there before;
//! one with no `to` names something that is gone. Text — a claim's words, its
//! details, a page's prose — is named by its size and never by its words,
//! because the caller wrote those words and an echo teaches it nothing.
//!
//! **The limit: the block is a before-read and an after-read, not a lock.** The
//! thing is read just before the write and just after it. A write by another
//! session to the same thing between the two reads can appear in this call's
//! block, and a write this call made can appear in theirs. Only a document
//! section's placing holds a per-thing lock (`Jojobot::hold_placing`); no other
//! write holds one across its reads, and widening that lock to every write is a
//! design question of its own rather than part of the receipt.

use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{Entity, Fact};

/// **How many entries one receipt lists.** A merge moves every claim of the
/// thing it folds, so a list with no bound would grow with the thing. Past
/// this the list is cut and the receipt says how many it left out.
const CHANGES_SHOWN: usize = 20;

/// **The longest value an entry shows as itself.** A longer one is named by
/// its size, as text is.
const VALUE_SHOWN_BYTES: usize = 240;

/// What changed on a message or a run: its state.
pub(crate) const STATE: &str = "state";

/// What changed on a message: the notes it was retired with.
pub(crate) const NOTES: &str = "notes";

/// What changed on a run: a chronology entry.
pub(crate) const ENTRY: &str = "entry";

/// What changed on a run: its focus.
pub(crate) const FOCUS: &str = "focus";

/// What changed on a message: the message that replaced it.
pub(crate) const SUPERSEDED_BY: &str = "superseded_by";

/// The part of a bot a box is, when a creation names the box it opened.
pub(crate) const MAILBOX: &str = "mailbox";

/// What changed on a claim or an entry: its words.
pub(crate) const WORDS: &str = "words";

/// The word `changed` holds when the call changed nothing.
pub(crate) const NOTHING: &str = "nothing";

/// The word `changed` holds when the server could not read what the call
/// changed.
pub(crate) const NOT_READ: &str = "not_read";

/// **One thing the call changed.**
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct Change {
    /// The handle, the claim address or the message id the change is on.
    on: String,
    /// One word for what changed on it.
    what: &'static str,
    /// The key, when `what` is a key on a thing or a field of a claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_before: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_after: Option<usize>,
}

impl Change {
    fn on(on: impl Into<String>, what: &'static str) -> Self {
        Self {
            on: on.into(),
            what,
            key: None,
            from: None,
            to: None,
            bytes_before: None,
            bytes_after: None,
        }
    }

    fn keyed(mut self, key: &str) -> Self {
        self.key = Some(key.to_string());
        self
    }

    /// **A value on each side**, either of them absent. A value too long to
    /// show is named by its size.
    fn values(mut self, from: Option<serde_json::Value>, to: Option<serde_json::Value>) -> Self {
        let long = |v: &Option<serde_json::Value>| {
            v.as_ref().is_some_and(|v| shown_len(v) > VALUE_SHOWN_BYTES)
        };
        if long(&from) || long(&to) {
            self.bytes_before = from.as_ref().map(shown_len);
            self.bytes_after = to.as_ref().map(shown_len);
        } else {
            self.from = from;
            self.to = to;
        }
        self
    }

    /// **Text, by its size on each side.** Zero is a real size: text that was
    /// empty before.
    fn sized(mut self, before: Option<&str>, after: Option<&str>) -> Self {
        self.bytes_before = before.map(str::len);
        self.bytes_after = after.map(str::len);
        self
    }
}

fn shown_len(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => s.len(),
        other => other.to_string().len(),
    }
}

/// **Everything one call changed**, in the order a reader takes it: the thing
/// first, then its keys, then its claims.
#[derive(Debug, Default)]
pub(crate) struct Changed {
    changes: Vec<Change>,
    /// Set when what the call changed could not be read.
    unread: bool,
}

impl Changed {
    /// **What the server could not read.** Said as such, never as `nothing`:
    /// a write that landed and a write that changed nothing read alike to a
    /// caller who is told only that the list is empty.
    pub(crate) fn not_read() -> Self {
        Self {
            changes: Vec::new(),
            unread: true,
        }
    }

    /// One thing created: a message, a session entry.
    pub(crate) fn created(on: impl Into<String>) -> Self {
        Self {
            changes: vec![Change::on(on, "created")],
            unread: false,
        }
    }

    /// **One thing created beside the thing the call is about**, named by what
    /// it is for: the box a new bot owns. `key` says which part of the call's
    /// subject it is.
    pub(crate) fn created_for(on: impl Into<String>, key: &str) -> Self {
        Self {
            changes: vec![Change::on(on, "created").keyed(key)],
            unread: false,
        }
    }

    /// One value moved, or nothing when the two sides are equal.
    pub(crate) fn value(
        on: impl Into<String>,
        what: &'static str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Self {
        let mut changed = Self::default();
        if from != to {
            changed
                .changes
                .push(Change::on(on, what).values(from.map(Into::into), to.map(Into::into)));
        }
        changed
    }

    /// **Text, named by its size on each side** and never by its words, or
    /// nothing when the two sides are the same text. `key` names the part of
    /// `on` the text sits in, such as the entry a run's words belong to. A side
    /// that is `None` is text that was not there.
    pub(crate) fn text(
        on: impl Into<String>,
        what: &'static str,
        key: Option<&str>,
        before: Option<&str>,
        after: Option<&str>,
    ) -> Self {
        let mut changed = Self::default();
        if before != after {
            let change = Change::on(on, what).sized(before, after);
            changed.changes.push(match key {
                Some(key) => change.keyed(key),
                None => change,
            });
        }
        changed
    }

    /// **What one call changed in a type's declaration**, from the declaration
    /// as it stood before the call (`None` when the name was new) and as it
    /// stands after. A new type is one `created` entry. A redeclaration is one
    /// `key` entry for each key added, removed or changed, each side a short
    /// token for what the key holds; the same declaration again is nothing.
    pub(crate) fn of_type(before: Option<&DeclaredType>, after: &DeclaredType) -> Self {
        let on = after.name.as_str();
        let Some(before) = before else {
            return Self::created(on);
        };
        let tokens = |declared: &DeclaredType| -> std::collections::BTreeMap<String, String> {
            declared
                .fields
                .iter()
                .map(|field| (field.key.clone(), field_token(field)))
                .collect()
        };
        let (was, is) = (tokens(before), tokens(after));
        let keys: std::collections::BTreeSet<&String> = was.keys().chain(is.keys()).collect();
        let mut changed = Self::default();
        for key in keys {
            let (from, to) = (was.get(key), is.get(key));
            if from != to {
                changed
                    .changes
                    .push(Change::on(on, "key").keyed(key).values(
                        from.map(|t| t.as_str().into()),
                        to.map(|t| t.as_str().into()),
                    ));
            }
        }
        changed
    }

    /// **Both lists, in order.** What could not be read stays unread.
    pub(crate) fn and(mut self, other: Changed) -> Self {
        self.changes.extend(other.changes);
        self.unread |= other.unread;
        self
    }

    /// **What one call changed on one thing**, from the thing as it stood
    /// before the call and as it stands after. `None` on a side is a thing
    /// that was not there.
    pub(crate) fn of_thing(before: Option<&DocScan>, after: Option<&DocScan>) -> Self {
        let mut changes = Vec::new();
        let (before_entity, after_entity) = (
            before.and_then(|scan| scan.entity.as_ref()),
            after.and_then(|scan| scan.entity.as_ref()),
        );
        // **A write landed and its thing cannot be found.** An empty list would
        // read as a call that changed nothing.
        let Some(after_entity) = after_entity else {
            return Self::not_read();
        };
        let on = after_entity.id.as_str();
        match before_entity {
            None => changes.push(Change::on(on, "created")),
            Some(before_entity) => entity_changes(before_entity, after_entity, &mut changes),
        }
        // **A side that was not there holds nothing**: no prose, no keys and
        // no claims, so everything the call made shows as made.
        let no_fields = std::collections::BTreeMap::new();
        let fields = |scan: Option<&DocScan>| {
            scan.map(|s| s.fields.clone())
                .unwrap_or_else(|| no_fields.clone())
        };
        if prose(before) != prose(after) {
            changes.push(Change::on(on, "prose").sized(Some(prose(before)), Some(prose(after))));
        }
        let (before_fields, after_fields) = (fields(before), fields(after));
        let keys: std::collections::BTreeSet<&String> =
            before_fields.keys().chain(after_fields.keys()).collect();
        for key in keys {
            let (was, is) = (before_fields.get(key), after_fields.get(key));
            if was != is {
                changes.push(Change::on(on, "key").keyed(key).values(
                    was.map(|v| v.as_str().into()),
                    is.map(|v| v.as_str().into()),
                ));
            }
        }
        let (before_facts, after_facts) = (facts(before), facts(after));
        let was: std::collections::BTreeMap<&str, &Fact> =
            before_facts.iter().map(|f| (f.id.as_str(), f)).collect();
        let mut claims: Vec<&Fact> = after_facts.iter().collect();
        claims.sort_by_key(|fact| fact.address().to_string());
        for fact in claims {
            match was.get(fact.id.as_str()) {
                None => changes.push(Change::on(fact.address().to_string(), "created")),
                Some(old) => claim_changes(old, fact, &mut changes),
            }
        }
        let still: std::collections::BTreeSet<&str> =
            after_facts.iter().map(|f| f.id.as_str()).collect();
        for fact in before_facts {
            if !still.contains(fact.id.as_str()) {
                changes.push(Change::on(fact.address().to_string(), "moved"));
            }
        }
        Self {
            changes,
            unread: false,
        }
    }
}

/// **A key of a type as one short token**: what it holds, how its writes fold,
/// then `required` and the closed set when it has them. Two keys with the same
/// token declare the same thing.
fn field_token(field: &Field) -> String {
    let mut token = format!("{}, {}", field.holds_token(), field.folds.as_token());
    if field.required {
        token.push_str(", required");
    }
    if let Some(values) = &field.one_of {
        token.push_str(", one_of ");
        token.push_str(&values.join("|"));
    }
    token
}

fn prose(scan: Option<&DocScan>) -> &str {
    scan.map_or("", |s| s.prose.as_str())
}

fn facts(scan: Option<&DocScan>) -> &[Fact] {
    scan.map_or(&[], |s| s.facts.as_slice())
}

fn entity_changes(before: &Entity, after: &Entity, changes: &mut Vec<Change>) {
    let on = after.id.as_str();
    let mut value =
        |what: &'static str, was: Option<serde_json::Value>, is: Option<serde_json::Value>| {
            if was != is {
                changes.push(Change::on(on, what).values(was, is));
            }
        };
    value(
        "handle",
        Some(before.id.as_str().into()),
        Some(after.id.as_str().into()),
    );
    value(
        "name",
        Some(before.name.as_str().into()),
        Some(after.name.as_str().into()),
    );
    value(
        "aliases",
        Some(before.aliases.clone().into()),
        Some(after.aliases.clone().into()),
    );
    value(
        "source",
        Some(before.source.as_str().into()),
        Some(after.source.as_str().into()),
    );
    value(
        "crm",
        before.crm.as_deref().map(Into::into),
        after.crm.as_deref().map(Into::into),
    );
    value(
        "parent",
        before.parent.as_ref().map(|p| p.as_str().into()),
        after.parent.as_ref().map(|p| p.as_str().into()),
    );
    value(
        "boot",
        Some(before.boot.as_token().into()),
        Some(after.boot.as_token().into()),
    );
    let standing = |e: &Entity| {
        if e.archived.is_some() {
            "archived"
        } else {
            "active"
        }
    };
    value(
        "status",
        Some(standing(before).into()),
        Some(standing(after).into()),
    );
    value(
        "merged_into",
        before.merged_into.as_ref().map(|m| m.as_str().into()),
        after.merged_into.as_ref().map(|m| m.as_str().into()),
    );
}

fn claim_changes(before: &Fact, after: &Fact, changes: &mut Vec<Change>) {
    let on = after.address().to_string();
    if before.content != after.content {
        changes.push(Change::on(&on, WORDS).sized(Some(&before.content), Some(&after.content)));
    }
    if before.details != after.details {
        changes.push(
            Change::on(&on, "details").sized(before.details.as_deref(), after.details.as_deref()),
        );
    }
    let mut value =
        |what: &'static str, was: Option<serde_json::Value>, is: Option<serde_json::Value>| {
            if was != is {
                changes.push(Change::on(&on, what).values(was, is));
            }
        };
    value(
        "status",
        Some(before.status.as_token().into()),
        Some(after.status.as_token().into()),
    );
    value(
        "provenance",
        Some(before.provenance.as_token().into()),
        Some(after.provenance.as_token().into()),
    );
    value(
        "standing",
        Some(before.standing.as_token().into()),
        Some(after.standing.as_token().into()),
    );
    let day = |d: Option<jiff::civil::Date>| d.map(|d| d.to_string().into());
    value(
        "recorded_at",
        day(Some(before.recorded_at)),
        day(Some(after.recorded_at)),
    );
    value(
        "happened_at",
        day(before.happened_at),
        day(after.happened_at),
    );
    value(
        "happened_through",
        day(before.happened_through),
        day(after.happened_through),
    );
    value(
        "stale_after",
        day(before.stale_after),
        day(after.stale_after),
    );
    let edge = |f: &Fact| {
        f.edge
            .as_ref()
            .map(|e| serde_json::json!({"shape": e.shape.as_token(), "object": e.object.as_str()}))
    };
    value("edge", edge(before), edge(after));
    let handles = |ids: &[jojobot_domain::memory::EntityId]| {
        serde_json::Value::from(ids.iter().map(|id| id.as_str()).collect::<Vec<_>>())
    };
    value(
        "refs",
        Some(handles(&before.refs)),
        Some(handles(&after.refs)),
    );
    value(
        "derived_from",
        before.derived_from.as_ref().map(|a| a.to_string().into()),
        after.derived_from.as_ref().map(|a| a.to_string().into()),
    );
    let addresses = |a: &[jojobot_domain::memory::FactAddress]| {
        serde_json::Value::from(a.iter().map(ToString::to_string).collect::<Vec<_>>())
    };
    value(
        "stands_for",
        Some(addresses(&before.stands_for)),
        Some(addresses(&after.stands_for)),
    );
    let keys: std::collections::BTreeSet<&String> =
        before.fields.keys().chain(after.fields.keys()).collect();
    for key in keys {
        let (was, is) = (before.fields.get(key), after.fields.get(key));
        if was != is {
            changes.push(Change::on(&on, "field").keyed(key).values(
                was.map(|v| v.as_str().into()),
                is.map(|v| v.as_str().into()),
            ));
        }
    }
}

/// **Put the block on a receipt.** The list, or `nothing`, or `not_read` with a
/// line saying why; a list past [`CHANGES_SHOWN`] is cut and says how many it
/// left out.
pub(crate) fn note_changed(body: &mut serde_json::Value, changed: Changed) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    if changed.unread {
        fields.insert("changed".into(), NOT_READ.into());
        fields.insert(
            "changed_note".into(),
            "the write landed, and what it changed could not be read back; recall what it \
             names to see where it stands"
                .into(),
        );
        return;
    }
    if changed.changes.is_empty() {
        fields.insert("changed".into(), NOTHING.into());
        return;
    }
    let total = changed.changes.len();
    let shown: Vec<serde_json::Value> = changed
        .changes
        .into_iter()
        .take(CHANGES_SHOWN)
        .map(|change| serde_json::to_value(change).unwrap_or_default())
        .collect();
    fields.insert("changed".into(), shown.into());
    if total > CHANGES_SHOWN {
        fields.insert(
            "changed_left_out".into(),
            serde_json::json!({
                "count": total - CHANGES_SHOWN,
                "how_to_read": "recall each thing this call wrote on, with facts: true, to read \
                                where the rest stand",
            }),
        );
    }
}

impl crate::Jojobot {
    /// **A thing as it stands, for one side of a `changed` block.** `Ok(None)`
    /// is a thing that is not there; `Err` is a read that failed, which makes
    /// the block `not_read` rather than a guess.
    pub(crate) async fn as_it_stands(
        &self,
        id: &jojobot_domain::memory::EntityId,
    ) -> Result<Option<DocScan>, ()> {
        self.memory.scan_entity(id).await.map_err(|_| ())
    }

    /// **What a call changed on one thing**, given its state before the call.
    pub(crate) async fn changed_on(
        &self,
        before: &Result<Option<DocScan>, ()>,
        after: &jojobot_domain::memory::EntityId,
    ) -> Changed {
        match (before, self.as_it_stands(after).await) {
            (Ok(before), Ok(after)) => Changed::of_thing(before.as_ref(), after.as_ref()),
            _ => Changed::not_read(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Jojobot;
    use crate::harness::*;
    use crate::memory::set_charter::SetCharterArgs;
    use crate::memory::testing::*;
    use crate::memory::update_entity::UpdateEntityArgs;
    use crate::memory::update_fact::UpdateFactArgs;
    use jojobot_domain::memory::Memory;
    use rmcp::handler::server::wrapper::Parameters;
    use std::sync::Arc;

    /// **The verbs that only read.** Every other tool is a write and must
    /// carry `changed`, so a tool added later is a write until it is named
    /// here. A verb listed here either reads only or reports what it changed
    /// in its own fields.
    const READS: &[&str] = &[
        "list_entities",
        "list_runs",
        "list_sent",
        "ping",
        "read_document",
        // Delivery: each message in `messages` carries its `state` (now `read`)
        // and `seen_before`, which tells a move from new to read from a re-delivery.
        "read_mailbox",
        // Delivery: the message's `state` (now `read`) and `seen_before`; a
        // history read sets `taken` to false and moves nothing.
        "read_message",
        "recall",
        "search",
        // Boot: `session.swept` lists the abandoned runs it closed,
        // `identity.owned_mailbox.healed` says a missing box was opened, and
        // `session.claim.status` says whether the role was taken.
        "start_here",
    ];

    /// **One successful call of every write verb**, in an order where each
    /// call has what it needs, with the answer each gave.
    async fn every_write(
        jojobot: &Jojobot,
    ) -> std::collections::BTreeMap<String, serde_json::Value> {
        use crate::mailboxes::mark_processed::MarkProcessedArgs;
        use crate::mailboxes::post_message::PostMessageArgs;
        use crate::memory::archive_entity::ArchiveEntityArgs;
        use crate::memory::declare_type::{DeclareTypeArgs, FieldArgs};
        use crate::memory::merge_entities::MergeArgs;
        use crate::memory::put_document::PutDocumentArgs;
        use crate::memory::rename_entity::RenameEntityArgs;
        use crate::memory::retract::RetractArgs;
        use crate::session::amend_journal::AmendJournalArgs;
        use crate::session::journal::JournalArgs;
        use crate::session::wrap_session::WrapSessionArgs;

        let sid = writing_as(jojobot);
        let mut answers = std::collections::BTreeMap::new();
        let mut keep = |verb: &str, result: rmcp::model::CallToolResult| {
            let body = json_of(&result);
            assert_ne!(body["status"], "blocked", "{verb} was refused: {body}");
            answers.insert(verb.to_string(), body);
        };

        keep(
            "add_entity",
            jojobot
                .add_entity(Parameters(add_args("person", "milhouse", "Milhouse")))
                .await
                .expect("add_entity answers"),
        );
        make_bot(jojobot, "gamma").await;
        let captured = capture_ok(jojobot, capture_args("person:milhouse", "likes comics")).await;
        let address = captured["address"]
            .as_str()
            .expect("an address")
            .to_string();
        let mut capture = capture_args("person:milhouse", "plays the oboe");
        capture.sid = Some(sid.clone());
        keep(
            "capture",
            jojobot
                .capture(Parameters(capture))
                .await
                .expect("capture answers"),
        );
        keep(
            "update_fact",
            jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("likes comic books".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update_fact answers"),
        );
        keep(
            "update_entity",
            jojobot
                .update_entity(Parameters(renaming(
                    "person:milhouse",
                    "Milhouse Van Houten",
                )))
                .await
                .expect("update_entity answers"),
        );
        ensure(jojobot, "thing:red-bike").await;
        keep(
            "rename_entity",
            jojobot
                .rename_entity(Parameters(RenameEntityArgs {
                    handle: "thing:red-bike".into(),
                    to: "thing:road-bike".into(),
                    parent: None,
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("rename_entity answers"),
        );
        keep(
            "archive_entity",
            jojobot
                .archive_entity(Parameters(ArchiveEntityArgs {
                    request_id: None,
                    handle: "thing:road-bike".into(),
                    reason: "sold".into(),
                    restore: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("archive_entity answers"),
        );
        keep(
            "set_charter",
            jojobot
                .set_charter(Parameters(SetCharterArgs {
                    request_id: None,
                    bot: "gamma".into(),
                    prose: "Holds the plan.".into(),
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("set_charter answers"),
        );
        keep(
            "declare_type",
            jojobot
                .declare_type(Parameters(DeclareTypeArgs {
                    request_id: None,
                    name: "rota".into(),
                    fields: vec![FieldArgs {
                        key: "starts".into(),
                        holds: None,
                        folds: None,
                        required: false,
                        one_of: None,
                    }],
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("declare_type answers"),
        );
        ensure(jojobot, "person:homer").await;
        ensure(jojobot, "person:lisa").await;
        keep(
            "merge_entities",
            jojobot
                .merge_entities(Parameters(MergeArgs {
                    request_id: None,
                    duplicate: "person:homer".into(),
                    survivor: "person:lisa".into(),
                    reason: Some("one person".into()),
                    recorded_at: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("merge_entities answers"),
        );
        keep(
            "retract",
            jojobot
                .retract(Parameters(RetractArgs {
                    request_id: None,
                    address: address.clone(),
                    reason: Some("never said".into()),
                    recorded_at: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("retract answers"),
        );
        ensure(jojobot, "document:krusty").await;
        keep(
            "put_document",
            jojobot
                .put_document(Parameters(PutDocumentArgs {
                    document: "document:krusty".into(),
                    markdown: Some("# One\n\nThe first part.\n".into()),
                    restore_to: None,
                    confirmed_by_user: None,
                    after: None,
                    provenance: None,
                    source: None,
                    source_ref: None,
                    recorded_at: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("put_document answers"),
        );
        let posted = jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "gamma".into(),
                body: "the plan is ready".into(),
                sid: sid.clone(),
                subject: None,
                in_reply_to: None,
                supersedes: None,
            }))
            .await
            .expect("post_message answers");
        let message = json_of(&posted)["id"].as_str().expect("an id").to_string();
        keep("post_message", posted);
        keep(
            "mark_processed",
            jojobot
                .mark_processed(Parameters(MarkProcessedArgs {
                    request_id: None,
                    message_id: message,
                    notes: Some("read it".into()),
                    sid: Some(as_bot(jojobot, "gamma")),
                    quarantine: None,
                }))
                .await
                .expect("mark_processed answers"),
        );
        make_bot(jojobot, "delta").await;
        let run = booted(jojobot, "delta").await;
        keep(
            "journal",
            jojobot
                .journal(Parameters(JournalArgs {
                    working_on: None,
                    entry: "Read the plan.".into(),
                    focus: Some("the plan".into()),
                    sid: run.clone(),
                }))
                .await
                .expect("journal answers"),
        );
        keep(
            "amend_journal",
            jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "Read the whole plan.".into(),
                    sid: run.clone(),
                }))
                .await
                .expect("amend_journal answers"),
        );
        keep(
            "wrap_session",
            jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "Read the plan and stopped.".into(),
                    sid: run,
                }))
                .await
                .expect("wrap_session answers"),
        );
        answers
    }

    /// 🚨 **Every write verb's receipt carries `changed`**, a list or the word
    /// `nothing`. The verbs are read off the served surface, so a write verb
    /// added later with no case here, or with no block, fails this.
    #[tokio::test]
    async fn every_write_verb_answers_with_what_it_changed() {
        let jojobot = handler();
        let answers = every_write(&jojobot).await;
        let tools = crate::Jojobot::tool_router().list_all();
        let writes: Vec<&str> = tools
            .iter()
            .map(|t| t.name.as_ref())
            .filter(|name| !READS.contains(name))
            .collect();
        assert!(
            writes.len() >= 16,
            "the served surface has its write verbs: {writes:?}"
        );
        for verb in writes {
            // **A caller finds the block where it reads about the verb**: the
            // description names it, once.
            let description = tools
                .iter()
                .find(|t| t.name.as_ref() == verb)
                .and_then(|t| t.description.as_deref())
                .unwrap_or_default();
            assert_eq!(
                description.matches("`changed`").count(),
                1,
                "{verb}'s description names the block once: {description}",
            );
            let body = answers
                .get(verb)
                .unwrap_or_else(|| panic!("{verb} writes and has no case in every_write"));
            let changed = &body["changed"];
            assert!(
                changed.as_array().is_some_and(|list| !list.is_empty())
                    || changed == crate::changed::NOTHING,
                "{verb} answers without what it changed: {body}",
            );
        }
    }

    fn renaming(handle: &str, name: &str) -> UpdateEntityArgs {
        UpdateEntityArgs {
            reviewed: None,
            request_id: None,
            handle: handle.into(),
            name: Some(name.into()),
            aliases: None,
            source: None,
            crm: None,
            sets: None,
            override_token: None,
            override_reason: None,
            sid: Some(TEST_SID.into()),
        }
    }

    /// **A new display name is named, with what it was and what it is**, and
    /// the same call again changes nothing and says so.
    #[tokio::test]
    async fn a_new_display_name_is_named_and_the_same_name_again_is_nothing() {
        let jojobot = handler();
        writing_as(&jojobot);
        ensure(&jojobot, "person:milhouse").await;

        let renamed = json_of(
            &jojobot
                .update_entity(Parameters(renaming(
                    "person:milhouse",
                    "Milhouse Van Houten",
                )))
                .await
                .expect("update_entity answers"),
        );
        let changed = renamed["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {renamed}"));
        assert_eq!(changed.len(), 1, "{renamed}");
        assert_eq!(changed[0]["on"], "person:milhouse", "{renamed}");
        assert_eq!(changed[0]["what"], "name", "{renamed}");
        assert_eq!(changed[0]["from"], "milhouse", "{renamed}");
        assert_eq!(changed[0]["to"], "Milhouse Van Houten", "{renamed}");

        let again = json_of(
            &jojobot
                .update_entity(Parameters(renaming(
                    "person:milhouse",
                    "Milhouse Van Houten",
                )))
                .await
                .expect("update_entity answers"),
        );
        assert_eq!(again["changed"], "nothing", "{again}");
    }

    /// **A receipt carries the block the way a verb puts it there.**
    fn receipt(changed: crate::changed::Changed) -> serde_json::Value {
        let mut body = serde_json::json!({});
        crate::changed::note_changed(&mut body, changed);
        body
    }

    /// 🚨 **A write that landed and whose thing cannot be found afterwards is
    /// `not_read`**, never `nothing`: an empty list reads as a call that
    /// changed nothing. Two scans that differ are a list, which is the positive
    /// the refusal to guess is held against.
    #[tokio::test]
    async fn a_thing_missing_after_the_write_is_not_read_and_two_scans_that_differ_are_a_list() {
        use crate::changed::{Changed, NOT_READ};
        let jojobot = handler();
        writing_as(&jojobot);
        ensure(&jojobot, "person:lisa").await;
        let id = jojobot_domain::memory::EntityId("person:lisa".into());
        let before = jojobot
            .as_it_stands(&id)
            .await
            .expect("the read answers")
            .expect("the thing is there");
        jojobot
            .update_entity(Parameters(renaming("person:lisa", "Lisa Simpson")))
            .await
            .expect("update_entity answers");
        let after = jojobot
            .as_it_stands(&id)
            .await
            .expect("the read answers")
            .expect("the thing is there");

        for (side, changed) in [
            ("no scan on either side", Changed::of_thing(None, None)),
            (
                "a thing before and none after",
                Changed::of_thing(Some(&before), None),
            ),
        ] {
            let body = receipt(changed);
            assert_eq!(body["changed"], NOT_READ, "{side}: {body}");
            assert!(body.get("changed_note").is_some(), "{side}: {body}");
        }

        let body = receipt(Changed::of_thing(Some(&before), Some(&after)));
        let list = body["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {body}"));
        assert!(list.iter().any(|c| c["what"] == "name"), "{body}");
        assert!(body.get("changed_note").is_none(), "{body}");

        let body = receipt(Changed::of_thing(None, Some(&after)));
        assert_eq!(body["changed"][0]["what"], "created", "{body}");
    }

    /// **A list is cut at `CHANGES_SHOWN`, and says how many it left out.** Exactly
    /// that many entries ship whole, with no left-out block.
    #[test]
    fn a_list_past_the_cut_ships_the_cut_and_counts_the_rest() {
        use crate::changed::Changed;
        let moved = |count: usize| {
            (0..count).fold(Changed::default(), |all, n| {
                all.and(Changed::value(
                    "person:lisa",
                    "name",
                    Some("before"),
                    Some(&format!("after {n}")),
                ))
            })
        };

        let at_the_cut = receipt(moved(20));
        assert_eq!(at_the_cut["changed"].as_array().map(Vec::len), Some(20));
        assert!(at_the_cut.get("changed_left_out").is_none(), "{at_the_cut}");

        let past = receipt(moved(27));
        assert_eq!(past["changed"].as_array().map(Vec::len), Some(20));
        assert_eq!(past["changed_left_out"]["count"], 7, "{past}");
        assert!(
            past["changed_left_out"]["how_to_read"].is_string(),
            "{past}"
        );
        assert_eq!(past["changed"][19]["to"], "after 19", "{past}");
    }

    /// **A value past `VALUE_SHOWN_BYTES` is named by its size on each side**
    /// and shows no words; one of exactly that size shows as itself.
    #[test]
    fn a_value_past_the_shown_size_is_named_by_bytes_and_one_at_it_is_shown() {
        use crate::changed::Changed;
        let at_the_limit = "x".repeat(240);
        let past = "y".repeat(241);

        let shown = receipt(Changed::value(
            "person:lisa",
            "name",
            Some("short"),
            Some(&at_the_limit),
        ));
        assert_eq!(shown["changed"][0]["from"], "short", "{shown}");
        assert_eq!(shown["changed"][0]["to"], at_the_limit, "{shown}");
        assert!(shown["changed"][0].get("bytes_after").is_none(), "{shown}");

        let sized = receipt(Changed::value(
            "person:lisa",
            "name",
            Some("short"),
            Some(&past),
        ));
        let entry = &sized["changed"][0];
        assert_eq!(entry["bytes_before"], 5, "{sized}");
        assert_eq!(entry["bytes_after"], 241, "{sized}");
        assert!(
            entry.get("from").is_none() && entry.get("to").is_none(),
            "{sized}"
        );
    }

    /// 🚨 **A before-read that fails makes the answer `not_read`**, and the
    /// write still lands: the caller is told it could not be read, not that
    /// nothing changed. The same call over a store that reads is the list.
    #[tokio::test]
    async fn a_before_read_that_fails_is_not_read_and_the_rename_still_lands() {
        use crate::changed::NOT_READ;
        use crate::session::scripted::Scripted;
        use jojobot_domain::memory::testing::InMemoryMemory;
        let inner = Arc::new(InMemoryMemory::seeded());
        let scripted = Arc::new(Scripted::over(inner.clone()));
        let jojobot = Jojobot::new(
            scripted.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        writing_as(&jojobot);
        ensure(&jojobot, "person:lisa").await;
        let id = jojobot_domain::memory::EntityId("person:lisa".into());

        // The claim room reads the thing first; the before-read is the second scan.
        scripted.script(|s| s.scan_entity_fails_on = Some(2));
        let blind = json_of(
            &jojobot
                .update_entity(Parameters(renaming("person:lisa", "Lisa Simpson")))
                .await
                .expect("update_entity answers"),
        );
        assert_eq!(blind["changed"], NOT_READ, "{blind}");
        assert!(blind.get("changed_note").is_some(), "{blind}");
        let stored = inner
            .list_entities(None)
            .await
            .expect("the inner store lists")
            .into_iter()
            .find(|e| e.id == id)
            .expect("the thing is there");
        assert_eq!(stored.name, "Lisa Simpson", "the rename landed");

        let reading = json_of(
            &jojobot
                .update_entity(Parameters(renaming("person:lisa", "Lisa Marie Simpson")))
                .await
                .expect("update_entity answers"),
        );
        assert_eq!(reading["changed"][0]["what"], "name", "{reading}");
        assert_eq!(reading["changed"][0]["from"], "Lisa Simpson", "{reading}");
    }

    /// **A charter written twice in the same words changes nothing the second
    /// time**, and the first write names the prose by its size alone: the
    /// caller wrote the words and is not handed them back.
    #[tokio::test]
    async fn the_same_charter_again_is_nothing_and_a_new_one_is_named_by_size() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let charter = |prose: &str| SetCharterArgs {
            request_id: None,
            bot: "gamma".into(),
            prose: prose.into(),
            sid: Some(TEST_SID.into()),
        };

        let first = json_of(
            &jojobot
                .set_charter(Parameters(charter("Holds the plan.")))
                .await
                .expect("set_charter answers"),
        );
        let changed = first["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {first}"));
        let prose = changed
            .iter()
            .find(|c| c["what"] == "prose")
            .unwrap_or_else(|| panic!("the prose is named: {first}"));
        assert_eq!(prose["on"], "bot:gamma", "{first}");
        assert_eq!(prose["bytes_after"], "Holds the plan.".len(), "{first}");
        assert!(prose.get("to").is_none(), "no words come back: {first}");

        let again = json_of(
            &jojobot
                .set_charter(Parameters(charter("Holds the plan.")))
                .await
                .expect("set_charter answers"),
        );
        assert_eq!(again["changed"], "nothing", "{again}");
    }
}
