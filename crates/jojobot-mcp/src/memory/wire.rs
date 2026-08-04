//! **The response vocabulary** — one record, one spelling.
//!
//! Rendered by hand rather than derived, so `capture`, `recall`, `update_fact`
//! and `search` cannot drift into three renderings of one fact, and so the
//! schema.org-flavoured names live in exactly one place.

use super::*;

/// A fact on the wire: the whole row plus the **address** — the handle a caller
/// needs to edit it. Reads return it with every fact precisely so that update is
/// usable without a second lookup.
///
/// Rendered by hand rather than derived, so `capture`, `recall`, `update_fact`
/// and `search` cannot drift into three spellings of one record — and so the
/// response vocabulary (schema.org names, § Vocabulary) lives in exactly one
/// place. **Input grammar is unaffected:** ids and kind tokens stay lowercase
/// `kind:slug` on the way in.
pub(crate) fn fact_json(fact: &Fact) -> serde_json::Value {
    serde_json::json!({
        "address": fact.address().to_string(),
        "subject": fact.subject.as_str(),
        "content": fact.content,
        "details": fact.details,
        "provenance": fact.provenance.as_token(),
        // **Always present, beside its twin.** The two answer different
        // questions — who backs this, and how sure is anyone — and a reader
        // deciding what to trust needs both or it is back to inferring one
        // from the other, which is the bug this field exists to end.
        "standing": fact.standing.as_token(),
        "status": fact.status.as_token(),
        "date": fact.date.to_string(),
        "edge": fact.edge.as_ref().map(edge_json),
        // **Absent-as-null, never an omitted key.** Most facts are not events,
        // and a reader must not have to branch on whether the field is there to
        // learn that this one is not one.
        "event": fact.event.as_ref().map(event_json),
        // Same rule: most claims are not derived from another claim, and a
        // reader must not have to branch on a missing key to learn that.
        "derived_from": fact.derived_from.as_ref().map(|a| a.to_string()),
    })
}

/// An event record on the wire — the type NAME, the bag as the caller wrote it,
/// and the entities it touches.
///
/// **Rendered as its parts, never as the stored line.** The payload's text form
/// is how the store keeps it, and handing that back would teach a caller a
/// grammar it has no business knowing and no reason to parse.
pub(crate) fn event_json(event: &Event) -> serde_json::Value {
    serde_json::json!({
        "type": event.kind,
        "metadata": event.metadata,
        // Links whose meaning is deliberately unrecorded — see
        // `EdgeShape::Connection` for why these are not `about`.
        "refs": event.refs.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
    })
}

/// A handle the reader can act on **and** understand: the id, the kind, and the
/// display name when the store knows one.
///
/// `name` is null for a handle that resolves to nothing — the orphan case. It is
/// left null rather than filled with the handle: an unresolvable subject is a
/// real condition, and hiding it behind a plausible string is how it went
/// unnoticed for a milestone.
pub(crate) fn entity_ref_json(reference: &EntityRef) -> serde_json::Value {
    serde_json::json!({
        "id": reference.id.as_str(),
        "type": reference.kind.map(type_name),
        "name": reference.name,
        // Same key an entity hit uses for the same idea — the asker who typed a
        // nickname has to see it here, or the hit answers a question they did
        // not ask under a name they do not recognize.
        "alternateName": reference.aliases,
    })
}

/// An edge on the wire. `type` carries schema.org's word for the shape —
/// `memberOf`, `attendee` — where the input token is `membership`, `attendance`.
pub(crate) fn edge_json(edge: &Edge) -> serde_json::Value {
    serde_json::json!({
        "type": edge.shape.as_name(),
        "object": edge.object.as_str(),
    })
}

/// An entity on the wire. `type` is the schema.org-flavored **name** for its
/// kind (`Person`, `CreativeWork`, `Organization`); the lowercase kind token
/// stays the input grammar and the handle's prefix.
pub(crate) fn entity_json(entity: &Entity) -> serde_json::Value {
    serde_json::json!({
        "id": entity.id.as_str(),
        "type": type_name(entity.kind),
        "name": entity.name,
        // schema.org's word for the same idea, and SKOS's split: one preferred
        // label, any number of alternate ones.
        "alternateName": entity.aliases,
        "source": entity.source,
        "crm": entity.crm,
        "boot": entity.boot.as_token(),
    })
}

/// One of the guard's candidates on the wire.
pub(crate) fn candidate_json(candidate: &EntityMatch) -> serde_json::Value {
    serde_json::json!({
        "handle": candidate.handle.as_str(),
        "type": type_name(candidate.kind),
        "name": candidate.name,
        "source": candidate.source,
        "reason": candidate.reason,
    })
}

/// The schema.org-flavored type name for an entity kind — **names only**, no
/// `@context`, no CURIEs, no JSON-LD: the recognition benefit is the word, which
/// models know from pretraining, not the machinery.
///
/// `Project` is jojobot's own personal-goal sense (trips, big rocks, builds),
/// deliberately NOT schema.org's Organization-subtype meaning.
pub(crate) fn type_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Person => "Person",
        EntityKind::Place => "Place",
        EntityKind::Event => "Event",
        EntityKind::Work => "CreativeWork",
        EntityKind::Thing => "Product",
        EntityKind::Org => "Organization",
        EntityKind::Topic => "Topic",
        EntityKind::Project => "Project",
        // schema.org has no bot; `SoftwareApplication` is its nearest word for
        // a non-human actor, and it is the one a model already knows.
        EntityKind::Bot => "SoftwareApplication",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The response vocabulary, whole.** Every kind renders its schema.org
    /// name — and the table is walked from `EntityKind::ALL`, so a ninth kind
    /// cannot arrive without someone deciding what it is called on the wire.
    ///
    /// The other half is the input grammar, which is **unchanged**: the names
    /// are output only, and a capitalized kind is still not a kind token.
    #[test]
    fn every_kind_renders_its_schema_org_name_and_none_is_an_input_token() {
        let table = [
            (EntityKind::Person, "person", "Person"),
            (EntityKind::Place, "place", "Place"),
            (EntityKind::Event, "event", "Event"),
            (EntityKind::Work, "work", "CreativeWork"),
            (EntityKind::Thing, "thing", "Product"),
            (EntityKind::Org, "org", "Organization"),
            (EntityKind::Topic, "topic", "Topic"),
            (EntityKind::Project, "project", "Project"),
            (EntityKind::Bot, "bot", "SoftwareApplication"),
        ];
        assert_eq!(
            table.len(),
            EntityKind::ALL.len(),
            "every kind must be named here"
        );
        for (kind, token, name) in table {
            assert_eq!(kind.as_token(), token, "the input token stays lowercase");
            assert_eq!(type_name(kind), name);
            // The response name is a name, not a token: input grammar unchanged.
            if name != token {
                assert!(parse_kind(name).is_err(), "{name} must not parse as a kind");
            }
        }
        assert!(
            parse_kind("Person").is_err(),
            "a capitalized kind stays rejected"
        );
    }
}
