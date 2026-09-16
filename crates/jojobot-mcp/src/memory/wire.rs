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
/// **The day is handed in, never read here.** Whether a claim has passed the
/// day its writer said it stays good is a question about a day, and which day
/// that is belongs to the RUN asking: two runs in two zones disagree about
/// today for the same stored claim, and both are right. The renderer reads no
/// clock, exactly as the domain reads none.
/// **`revised` is the total writes behind this claim, when the caller checked
/// — `None` where nobody did.** Not opt-in on its own: a caller that read no
/// count says nothing about whether one exists, rather than saying there is
/// none — the same reason `None` and `Some(1)` are two different answers
/// below.
pub(crate) fn fact_json(
    fact: &Fact,
    as_of: jiff::civil::Date,
    revised: Option<usize>,
) -> serde_json::Value {
    let mut rendered = serde_json::json!({
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
        "recorded_at": fact.recorded_at.to_string(),
        // **A THIRD question, and absent is its ordinary answer.** `date` is
        // about the claim and this is about the thing the claim is about. A
        // claim that says nothing here is complete: nobody gave a day, and a
        // day nobody gave is not something jojobot invents.
        //
        // **Present as `null`, not omitted**, unlike `stale_after` below —
        // `unprompted` and `bikes` (user stories) pin the literal
        // `"happened_at":null` on the wire, so a consumer here does read the
        // bare key rather than only its value.
        "happened_at": fact.happened_at.map(|day| day.to_string()),
        // **The other clock, and it is not the one above.** `date` says when
        // the claim is true OF; this says when jojobot took the record in. They
        // disagree on every backfill and on every booking, and a reader
        // deciding how old a claim is needs the second one — with `null`
        // meaning a record from before the store kept it, never "just now".
        "inserted_at": fact.inserted_at.map(|at| at.to_string()),
        "edge": fact.edge.as_ref().map(edge_json),
        // **The record's fields, flat on the record.** They are not a
        // sub-object about some other kind of thing: they are what this record
        // says, so they read at the same level as its content and its date.
        //
        // **Always present, empty when the record carries none.** A reader must
        // not have to branch on whether the key is there to learn that a record
        // has no fields.
        "fields": fact.fields,
        // Links whose meaning is deliberately unrecorded — see
        // `EdgeShape::Connection` for why these are not `about`.
        "refs": fact.refs.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
        // Same rule: most claims are not derived from another claim, and a
        // reader must not have to branch on a missing key to learn that.
        // **Present as `null`, not omitted** — `challenge` and `unsourced`
        // (user stories) pin the literal `"derived_from":null` on the wire.
        "derived_from": fact.derived_from.as_ref().map(|a| a.to_string()),
        // The synthesis mark: which claims this record stands for. Always
        // present, empty when the record carries no mark — same convention
        // as `refs`, because most records are not a synthesis of others.
        "stands_for": fact.stands_for.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
    });
    // **A key that holds nothing carries no information, so it is omitted
    // rather than spelled out as its own name plus `null`** — a field earns
    // its place (rule 300). `stale_after` is the one field of its shape with
    // no consumer reading its bare presence: nothing in the build or the
    // stories distinguishes "absent" from "null" for it, unlike `edge`,
    // `derived_from` and `happened_at` above, which real user stories pin as
    // present-and-null and so are left untouched.
    if let Some(stale_after) = fact.stale_after {
        rendered["stale_after"] = serde_json::json!(stale_after.to_string());
    }
    // **Only when the day has passed, and only when somebody set one.** A key
    // that said `false` on every ordinary claim would spend a reader's
    // attention saying nothing, and absence must not read as staleness.
    if fact.is_stale(as_of) {
        rendered["stale"] = serde_json::json!(true);
        rendered["stale_note"] = serde_json::json!(
            "this reading is past the day its writer said it stays good. It is not false — it is \
             unverified, and NOTHING IS COMING TO CHECK IT: no sweep and no reminder. Confirm it \
             before acting on it"
        );
    }
    // **Only when there is more than one, and only when the count was read at
    // all.** A claim written once is silent here, exactly as an unstale claim
    // is silent on `stale` — so the field a reader learns to watch for is the
    // one that fires, never a `false`/`1` nobody needs.
    if let Some(total) = revised
        && total > 1
    {
        rendered["revised"] = serde_json::json!(true);
        rendered["revision_count"] = serde_json::json!(total);
        rendered["revision_note"] = serde_json::json!(format!(
            "this claim has been written {total} times. The record above is the newest; recall \
             the subject with history_record: \"{}\" to read the earlier wording, oldest first.",
            fact.address()
        ));
    }
    rendered
}

/// **The receipt for a record somebody just wrote**: the same record with the
/// prose and the keys they sent replaced by what they do not have.
///
/// **What the read-back proved is untouched.** The store is still read before
/// a write is called a success, so a claim that did not survive storage is an
/// error rather than a success with mangled bytes. The proof happens
/// server-side; shipping the evidence to the author of the claim is what stops
/// here.
///
/// **Everything jojobot decided stays.** The address, without which the record
/// cannot be edited. The subject as it was qualified. The date, the provenance
/// and the standing — a caller that named none of those learns here what was
/// recorded, which is not an echo but the only way it finds out.
pub(crate) fn fact_receipt_json(fact: &Fact, as_of: jiff::civil::Date) -> serde_json::Value {
    const HOW: &str = "you wrote this claim. recall the subject to read it back, with its \
                       records and their addresses.";
    // **A write does not check its own claim's history.** The count belongs to
    // reading a claim, not to writing one — the caller that just wrote it
    // already knows whether it was a correction, and recall is where the
    // signal lives.
    let mut body = fact_json(fact, as_of, None);
    elide_prose(&mut body, "content", &fact.content, HOW);
    if let Some(details) = &fact.details {
        elide_prose(&mut body, "details", details, HOW);
    }
    if let Some(fields) = body.as_object_mut() {
        // **A count rather than the keys.** The keys and their values are what
        // the caller sent; how many landed is what says the write took them.
        fields.insert("fields".into(), serde_json::Value::Null);
        fields.insert("fields_elided".into(), true.into());
        fields.insert("fields_count".into(), fact.fields.len().into());
    }
    body
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

/// **A type declaration on the wire**: its name and its keys, in the order it
/// declared them.
///
/// The order is carried rather than sorted, because it is what the declaration
/// said, and a hit names the keys a record holds and lacks in that same order.
pub(crate) fn declared_type_json(declared: &DeclaredType) -> serde_json::Value {
    serde_json::json!({
        "name": declared.name,
        // **Where it came from, on every type that is served.** A type the
        // software ships is closed: a caller cannot replace it, and without
        // this the only way to learn that is to declare over it and read the
        // refusal — which makes a closed type something a caller trips over
        // rather than something it can see.
        "origin": declared.origin.as_token(),
        "fields": declared
            .fields
            .iter()
            // The whole declaration as one token, narrowing included — the same
            // spelling `declare_type` takes, so what a caller reads back is
            // what it would send to say the same thing again.
            //
            // **How the key folds is on every key, not only on the counters.**
            // Stated rather than left off when it is the default: a reader who
            // has to infer newest-wins from a missing token cannot tell it from
            // a build that does not have folds at all.
            //
            // **The set a key is narrowed to, when it has one.** Null rather
            // than an empty list for a key nobody narrowed: a reader that had
            // to tell "no set" from "a set with nothing in it" would be reading
            // a difference that cannot exist, because a set with no values is
            // refused at the declaration.
            //
            // **Whether the key is required, stated on every key**, for the
            // same reason the fold is. Optional is the default, so absence and
            // `false` would mean one thing — and a reader that had to infer it
            // from a missing key could not tell an optional key from a build
            // that does not have required keys at all. The narrowed set is the
            // other spelling on purpose: there, null and an empty list would
            // mean two different things, and only one of them can exist.
            .map(|f| serde_json::json!({
                "key": f.key,
                "holds": f.holds_token(),
                "folds": f.folds.as_token(),
                "required": f.required,
                "one_of": f.one_of,
            }))
            .collect::<Vec<_>>(),
    })
}

/// **How a thing answers a type, on the wire.**
///
/// `complete` is stated rather than left to be worked out from `lacking` being
/// empty: what a caller branches on is whether the thing is whole, and making
/// them derive it invites two callers to derive it differently.
///
/// `lacking` names the keys, because the caller's next move is to fill them or
/// to ignore them, and both need the names. `mistyped` carries what the type
/// said and what the record actually holds, so a reader sees the mistake
/// rather than being told one happened. Neither is a reason the record was
/// withheld: it is here.
pub(crate) fn answers_json(found: &jojobot_domain::memory::types::Match) -> serde_json::Value {
    serde_json::json!({
        "complete": found.complete(),
        "held": found.held,
        "lacking": found.lacking,
        "mistyped": found
            .mistyped
            .iter()
            .map(|m| serde_json::json!({
                "key": m.key,
                // **What the key wanted, from the declaration's own
                // spelling.** One function answers this for every narrowing
                // there is, and the refusal a write gets uses the same one: a
                // reader told `reference` learns nothing about what is wrong
                // with a perfectly good handle of another kind, and a reader
                // told `text` about a key narrowed to a set is looking at text.
                "declared": m.wanted(),
                "value": m.value,
            }))
            .collect::<Vec<_>>(),
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
        // **What this one sits under**, null for a root — which most entities
        // are. It is rendered rather than left out because a caller that named
        // a parent has no other way to see the pointer landed, and a rhythm
        // cannot be read at all without knowing whose loop it is.
        "parent": entity.parent.as_ref().map(|p| p.as_str()),
        // **Null while active; whole once archived.** The direct door — asking
        // for this handle — serves everything, including why and when: the
        // broad door excludes it instead of rendering half an answer.
        "archived": entity.archived.as_ref().map(|a| serde_json::json!({
            "reason": a.reason,
            "at": a.at.to_string(),
        })),
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
    match kind.as_token() {
        "person" => "Person",
        "place" => "Place",
        "event" => "Event",
        "work" => "CreativeWork",
        "thing" => "Product",
        "org" => "Organization",
        "topic" => "Topic",
        "project" => "Project",
        // schema.org has no bot; `SoftwareApplication` is its nearest word for
        // a non-human actor, and it is the one a model already knows.
        "bot" => "SoftwareApplication",
        // schema.org has no animal either, and its nearest available word is
        // `Product` — which states the one thing this kind exists to deny. So
        // the word here is the plain one every model already knows.
        "pet" => "Pet",
        // **A kind the software never heard of has no word waiting for it.**
        // The set of kinds is data, so a store may hold one this build has no
        // opinion about, and the honest answer is the caller's own token
        // rather than a schema.org word picked for being valid: the comments
        // above already choose the plain word twice over a nearest-fit that
        // would state something false, and this is the same choice for a case
        // that could not arise before.
        //
        // Title case because every word in this map is title case, and a
        // lowercase token beside `Person` reads as an accident rather than as
        // a name. Only the first letter is touched — a rule for every shape a
        // token might take would be a guess about tokens nobody has written.
        other => title_cased(other),
    }
}

/// The token with its first character upper-cased, held for the life of the
/// process beside the kind it names.
fn title_cased(token: &str) -> &'static str {
    let mut characters = token.chars();
    let titled = match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    };
    jojobot_domain::memory::kinds::intern(&titled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The response vocabulary, whole.** Every kind renders its schema.org
    /// name — and the table is walked from `EntityKind::ALL`, so a new kind
    /// cannot arrive without someone deciding what it is called on the wire.
    ///
    /// The other half is the input grammar, which is **unchanged**: the names
    /// are output only, and a capitalized kind is still not a kind token.
    #[test]
    fn every_kind_renders_its_schema_org_name_and_none_is_an_input_token() {
        let table = [
            (EntityKind::PERSON, "person", "Person"),
            (EntityKind::PLACE, "place", "Place"),
            (EntityKind::EVENT, "event", "Event"),
            (EntityKind::WORK, "work", "CreativeWork"),
            (EntityKind::THING, "thing", "Product"),
            (EntityKind::ORG, "org", "Organization"),
            (EntityKind::TOPIC, "topic", "Topic"),
            (EntityKind::PROJECT, "project", "Project"),
            (EntityKind::BOT, "bot", "SoftwareApplication"),
            (EntityKind::PET, "pet", "Pet"),
            (EntityKind::RHYTHM, "rhythm", "Rhythm"),
            (EntityKind::MACHINE, "machine", "Machine"),
            (EntityKind::VIEW, "view", "View"),
            (EntityKind::SESSION, "session", "Session"),
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

    /// 🚨 **A badge never reaches a caller**, on the wire or through serde.
    ///
    /// The handle is the only public name (rule 205): a caller sends handles,
    /// reads handles, and has no word for the name a row keeps underneath. A
    /// badge that leaked would become something a client stored and sent back,
    /// and the one thing it must never be is addressable.
    ///
    /// **Both doors in one case.** `entity_json` is hand-built and could simply
    /// omit it; `Entity` also derives `Serialize`, and a field added without
    /// `skip` would ride out through any path that serialises the struct.
    ///
    /// **Each negative is paired with the positive it depends on.** Asserting
    /// only the badge's absence passes identically if serialisation broke and
    /// produced nothing at all — it cannot tell *correctly withheld* from
    /// *nothing came out*. Asserting first that the handle and name ARE there
    /// closes that gap: an answer empty enough to hide the badge is now also
    /// empty enough to fail the positive.
    #[test]
    fn an_entitys_badge_reaches_no_caller() {
        let entity = Entity {
            id: EntityId::person("person:alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: Vec::new(),
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: Some("zzzzzz".into()),
            archived: None,
        };
        let served = entity_json(&entity).to_string();
        assert!(
            served.contains("person:alpha") && served.contains("Alpha"),
            "the wire answer carries the entity it is about: {served}",
        );
        assert!(
            !served.contains("zzzzzz") && !served.contains("badge"),
            "the wire answer carries the badge: {served}",
        );
        let serialised = serde_json::to_string(&entity).expect("an entity serialises");
        assert!(
            serialised.contains("person:alpha") && serialised.contains("Alpha"),
            "serde carries the entity it is about: {serialised}",
        );
        assert!(
            !serialised.contains("zzzzzz") && !serialised.contains("badge"),
            "serde carries the badge out: {serialised}",
        );
    }
}
