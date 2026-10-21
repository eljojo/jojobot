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
/// **Who recorded a claim when an app did**, as a reader sees it: the app, the
/// operation its grant allowed, and the moment the server took the request in.
pub(crate) fn recorded_by_json(stamp: &jojobot_domain::app::AppStamp) -> serde_json::Value {
    serde_json::json!({
        "app": stamp.app,
        "grant": stamp.grant,
        "received_at": stamp.received_at,
    })
}

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
        // **The far end of the same span, and it is meaningless without the
        // key above.** `happened_through` is never the far end of nothing —
        // see [`Fact::happened_through`] — so it earns the same present-and-
        // null treatment `happened_at` gets rather than the omit-when-empty
        // treatment `stale_after` gets below: a caller that wrote a span's
        // end must be able to read it back, through this same key, beside
        // its start.
        "happened_through": fact.happened_through.map(|day| day.to_string()),
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
        //
        // **Only the record's OWN keys.** What the record set on its thing is
        // `sets`, below, so a reader sees the two bags apart and a key under
        // `fields` is one the thing does not hold because of this claim.
        "fields": fact
            .fields
            .iter()
            .filter(|(key, _)| !fact.sets.contains(*key))
            .collect::<std::collections::BTreeMap<_, _>>(),
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
    // **What this record set on its thing**, beside `fields`. Omitted when it set
    // nothing, because a key holding nothing carries no information; a record
    // written before the bags were kept apart has none, and reads as it did.
    let set: std::collections::BTreeMap<_, _> = fact
        .fields
        .iter()
        .filter(|(key, _)| fact.sets.contains(*key))
        .collect();
    if !set.is_empty() {
        rendered["sets"] = serde_json::json!(set);
    }
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
    // **Only a claim that was settled on one carries it**, beside the
    // provenance it never changes: a settled inference reads back as an
    // inference with its receipt.
    if let Some(receipt) = &fact.receipt {
        rendered["receipt"] = settling_receipt_json(receipt);
    }
    // **Only a claim an app wrote carries it**, beside the provenance it never
    // changes. A bot that names the app in `read_from` gets no mark: the mark is
    // the server's, from the credential, and no caller can send it.
    if let Some(stamp) = &fact.app_stamp {
        rendered["recorded_by"] = recorded_by_json(stamp);
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
    }
    rendered
}

/// **The record on the wire as a LISTING serves it.**
///
/// A listing leads with each record's one-line claim, its address, provenance and
/// standing. The details are a paragraph of reasons the reader has not asked for,
/// so they are left out unless the call sent `details: true`, and the answer says
/// once how many it left out and how to ask ([`details_note`]). A record whose
/// details are left out is counted by [`left_out_of`], which the caller adds up.
pub(crate) fn listed_json(
    fact: &Fact,
    as_of: jiff::civil::Date,
    revised: Option<usize>,
    details_asked: bool,
) -> serde_json::Value {
    let mut rendered = fact_json(fact, as_of, revised);
    if !details_asked && let Some(map) = rendered.as_object_mut() {
        map.remove("details");
    }
    rendered
}

/// **How many of these records a listing without `details` leaves details out
/// of.** A record with no details leaves nothing out.
pub(crate) fn left_out_of<'a>(
    facts: impl IntoIterator<Item = &'a Fact>,
    details_asked: bool,
) -> usize {
    if details_asked {
        return 0;
    }
    facts
        .into_iter()
        .filter(|fact| fact.details.is_some())
        .count()
}

/// **The line an answer carries once when it left details out.** Eliding is never
/// silent: the count and the argument that returns them ride together, and the
/// addresses of the records it left them out of ride with them, up to
/// [`ADDRESSES_NAMED`]. The count is the whole of it.
pub(crate) fn details_note(count: usize, addresses: &[String]) -> serde_json::Value {
    let mut note = serde_json::json!({
        "count": count,
        "how_to_ask": "ask again with details: true to read them",
    });
    if !addresses.is_empty() {
        note["addresses"] = addresses
            .iter()
            .take(ADDRESSES_NAMED)
            .cloned()
            .collect::<Vec<_>>()
            .into();
    }
    note
}

/// **How many addresses a `details_left_out` line names.** The line is added to an
/// answer after its ceiling is counted, so what it names is bounded.
pub(crate) const ADDRESSES_NAMED: usize = 20;

/// **The addresses of the records whose details a listing leaves out**, for the
/// mark that carries them to the answer's line.
pub(crate) fn addresses_left_out<'a>(
    facts: impl IntoIterator<Item = &'a Fact>,
    details_asked: bool,
) -> Vec<String> {
    if details_asked {
        return Vec::new();
    }
    facts
        .into_iter()
        .filter(|fact| fact.details.is_some())
        .map(|fact| fact.address().to_string())
        .collect()
}

/// The private key beside [`OBJECT_LEFT_OUT`] that names the records it counts.
pub(crate) const OBJECT_LEFT_OUT_AT: &str = "_details_left_out_at";

/// The private key an object carries from where its records are rendered to
/// where the answer is finished, so the count of the answer is of the objects it
/// kept and not of the ones a ceiling cut.
pub(crate) const OBJECT_LEFT_OUT: &str = "_details_left_out";

/// **The same private key for prose**: an object that has a page of prose this
/// answer left out carries it, and the answer counts the kept ones once.
pub(crate) const OBJECT_PROSE_LEFT_OUT: &str = "_prose_left_out";

/// **The line an answer carries once when it left prose out.** Eliding is never
/// silent: the count and the argument that returns it ride together.
pub(crate) fn prose_note(count: usize) -> serde_json::Value {
    serde_json::json!({
        "count": count,
        "how_to_ask": "ask again with prose: true to read it",
    })
}

/// **What to do with the marks while the answer says how many details it left
/// out**: leave them, because the answer is not finished, or take them off.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marks {
    Keep,
    Take,
}

/// **Say once, off the answer as it stands, how many records it left their
/// details out of, and how many objects it left their prose out of.** The one
/// home for the counts, the lines and the marks, for every shape an answer
/// takes: `objects` with the ones they reached (`connected`) and `built_on`,
/// `results`, and `records`. A count above zero sets `details_left_out`
/// (`prose_left_out`); none removes it. `Marks::Take` also takes the marks off,
/// once the answer is finished; `Marks::Keep` leaves them for a later pass, so a
/// ceiling that cuts objects is counted after it, on what it kept.
pub(crate) fn say_details_left_out(body: &mut serde_json::Value, marks: Marks) {
    fn visit(
        object: &mut serde_json::Value,
        take: bool,
        named: &mut Vec<String>,
    ) -> (usize, usize) {
        let Some(map) = object.as_object_mut() else {
            return (0, 0);
        };
        let mut count = |key: &str| {
            if take {
                map.remove(key)
            } else {
                map.get(key).cloned()
            }
            .and_then(|count| count.as_u64())
            .unwrap_or(0) as usize
        };
        let own = (count(OBJECT_LEFT_OUT), count(OBJECT_PROSE_LEFT_OUT));
        let at = if take {
            map.remove(OBJECT_LEFT_OUT_AT)
        } else {
            map.get(OBJECT_LEFT_OUT_AT).cloned()
        };
        named.extend(
            at.as_ref()
                .and_then(|at| at.as_array())
                .into_iter()
                .flatten()
                .filter_map(|address| address.as_str().map(str::to_string)),
        );
        let reached = map
            .get_mut("connected")
            .and_then(|connected| connected.as_array_mut())
            .map_or((0, 0), |all| {
                all.iter_mut()
                    .map(|one| visit(one, take, named))
                    .fold((0, 0), |(d, p), (a, b)| (d + a, p + b))
            });
        (own.0 + reached.0, own.1 + reached.1)
    }
    let take = marks == Marks::Take;
    let mut named: Vec<String> = Vec::new();
    let (mut left_out, mut prose_left_out) = (0, 0);
    let mut add = |(details, prose): (usize, usize)| {
        left_out += details;
        prose_left_out += prose;
    };
    for shape in ["objects", "results", "records"] {
        if let Some(all) = body.get_mut(shape).and_then(|all| all.as_array_mut()) {
            for one in all.iter_mut() {
                add(visit(one, take, &mut named));
            }
        }
    }
    if let Some(built_on) = body.get_mut("built_on") {
        add(visit(built_on, take, &mut named));
    }
    if left_out > 0 {
        body["details_left_out"] = details_note(left_out, &named);
    } else if let Some(map) = body.as_object_mut() {
        map.remove("details_left_out");
    }
    if prose_left_out > 0 {
        body["prose_left_out"] = prose_note(prose_left_out);
    } else if let Some(map) = body.as_object_mut() {
        map.remove("prose_left_out");
    }
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
        // **Each count is the keys held in that bag, counted on its own.** A key is
        // a setting when the claim holds it and names it in `sets`, and an own field
        // when it holds it and does not. Neither count is the other taken away from
        // the total, so a `sets` naming a key the claim no longer holds can neither
        // panic here nor count a key that is not there.
        let set_count = fact
            .fields
            .keys()
            .filter(|key| fact.sets.contains(*key))
            .count();
        let own_count = fact
            .fields
            .keys()
            .filter(|key| !fact.sets.contains(*key))
            .count();
        fields.insert("fields_count".into(), own_count.into());
        // The same count for what the record set on its thing, only when it set
        // something, as `sets` itself is.
        if set_count > 0 {
            fields.insert("sets".into(), serde_json::Value::Null);
            fields.insert("sets_elided".into(), true.into());
            fields.insert("sets_count".into(), set_count.into());
        }
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
    let mut wire = serde_json::json!({
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
            "how_to_restore": "archive_entity with restore: true, this handle and a reason",
        })),
    });
    // **The names of what was folded into this one, apart from the aliases a
    // caller wrote.** A merge carries no name onto the survivor, so without this
    // a reader would lose "this was also known as X" without a word. Left out
    // when nothing was folded in.
    if !entity.folded_names.is_empty()
        && let Some(object) = wire.as_object_mut()
    {
        object.insert("foldedNames".into(), entity.folded_names.clone().into());
    }
    wire
}

/// **Take the aliases out of a write's receipt when keeping them would take the
/// receipt past the ceiling.** The receipt of a creation or an edit echoes the
/// row it leaves, and an edit may leave a row that is still over one answer (it
/// shrinks it in steps), so the echo cannot always fit. Called last, on the body
/// as it will be served, so it measures the answer and not a part of it. A body
/// that fits keeps its aliases whole.
///
/// **Eliding is never silent**: the aliases become null, a marker and their count
/// replace them, and `how_to_read` names the read that returns them.
pub(crate) fn elide_aliases_past_the_ceiling(body: &mut serde_json::Value) {
    let served = body.to_string().chars().count() + crate::answer::STATUS_BAR_ROOM;
    if served <= jojobot_domain::text::ANSWER_CEILING {
        return;
    }
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    let count = fields
        .get("alternateName")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    let what = if fields.contains_key("foldedNames") {
        "the aliases and the names folded into this thing"
    } else {
        "the aliases"
    };
    let how = format!(
        "{what} do not fit this answer. recall {} to read them.",
        fields
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the thing")
    );
    fields.insert("alternateName".into(), serde_json::Value::Null);
    fields.insert("alternateName_elided".into(), true.into());
    fields.insert("alternateName_count".into(), count.into());
    // The names of what was folded in are a list of names like the aliases, and
    // go with them, so the receipt is never kept over one answer by a list the
    // elision left whole.
    if let Some(folded) = fields
        .get("foldedNames")
        .and_then(serde_json::Value::as_array)
    {
        let folded = folded.len();
        fields.insert("foldedNames".into(), serde_json::Value::Null);
        fields.insert("foldedNames_elided".into(), true.into());
        fields.insert("foldedNames_count".into(), folded.into());
    }
    fields.insert("how_to_read".into(), how.into());
}

/// One of the guard's candidates on the wire.
pub(crate) fn candidate_json(candidate: &EntityMatch) -> serde_json::Value {
    let mut body = serde_json::json!({
        "handle": candidate.handle.as_str(),
        "type": type_name(candidate.kind),
        "name": candidate.name,
        "source": candidate.source,
        "reason": candidate.reason,
    });
    // **Beside the name, so the list alone tells the candidates apart.** Absent
    // for a candidate nobody said how to tell apart, rather than rendered empty.
    if let (Some(told), Some(body)) = (&candidate.told_apart_by, body.as_object_mut()) {
        body.insert(
            jojobot_domain::memory::guard::TOLD_APART_BY.into(),
            told.as_str().into(),
        );
    }
    body
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
        // schema.org has nothing for an ongoing situation either — its own
        // `Thread` sense is a conversation, not a line in somebody's life —
        // and the same choice already made for `rhythm`/`machine` applies
        // here: the plain word over a nearest-fit that would state something
        // false.
        "thread" => "Thread",
        // schema.org's `Decision` is no kind of thing, so the plain word stands
        // here as it does for `thread`.
        "decision" => "Decision",
        // A seat held by one session at a time has no schema.org word, and the
        // plain one is the same choice as for `thread` and `rhythm`.
        "role" => "Role",
        // A grant to do one gated act has no schema.org word either; the plain
        // one, as for `role`.
        "voucher" => "Voucher",
        // A code waiting to be presented has no schema.org word either.
        "challenge" => "Challenge",
        // schema.org's word for a file a person reads, held whole or in parts.
        "document" => "DigitalDocument",
        "list" => "ItemList",
        // schema.org's own word for a journey with two ends.
        "trip" => "Trip",
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

/// **A receipt as the wire spells it**: one key naming the shape. Written out
/// here rather than serialized from the domain type, so what a caller reads does
/// not move with how a store keeps it.
pub(crate) fn settling_receipt_json(
    receipt: &jojobot_domain::memory::Receipt,
) -> serde_json::Value {
    match receipt {
        jojobot_domain::memory::Receipt::Check(check) => serde_json::json!({ "check": check }),
        jojobot_domain::memory::Receipt::Confirmed { on, at } => {
            serde_json::json!({ "confirmed": { "on": on.to_string(), "at": at } })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🚨 **A receipt never takes the number of settings from the number of
    /// keys.** A claim whose `sets` names keys it no longer holds, which an
    /// edit that cleared a setting once answered with, panicked the server on a
    /// subtraction. The counts are the keys held in each bag, by membership, so
    /// the answer is the same whatever a stale `sets` names.
    ///
    /// **Both halves**: the keys the claim holds are counted where they belong, and
    /// the keys it does not hold are counted nowhere.
    #[tokio::test]
    async fn a_receipt_counts_the_bags_by_membership_when_sets_names_a_key_that_is_gone() {
        use crate::harness::handler_over;
        use crate::memory::testing::{capture_args, capture_ok};
        let memory = std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
        let jojobot = handler_over(memory.clone());
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("mood".to_string(), "x".to_string())]
                        .into_iter()
                        .collect(),
                ),
                sets: Some(
                    [("hat".to_string(), "red".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "at the party")
            },
        )
        .await;
        let mut fact =
            jojobot_domain::memory::Memory::recall(&*memory, &EntityId::person("person:alpha"))
                .await
                .expect("recall")
                .pop()
                .expect("the claim");
        assert_eq!(fact.sets.len(), 1, "{fact:?}");
        // The state a stale answer had: a setting gone from the fields and still
        // named in `sets`, and a key that was never there.
        fact.fields.remove("hat");
        fact.sets.insert("ghost".to_string());
        let day = jiff::civil::date(2026, 10, 11);
        let receipt = fact_receipt_json(&fact, day);
        assert_eq!(receipt["fields_count"], 1, "{receipt}");
        assert!(receipt.get("sets_count").is_none(), "{receipt}");
        // …and one that does hold a setting counts it.
        fact.fields.insert("hat".to_string(), "red".to_string());
        let held = fact_receipt_json(&fact, day);
        assert_eq!(held["fields_count"], 1, "{held}");
        assert_eq!(held["sets_count"], 1, "{held}");
    }

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
            (EntityKind::PROMISE, "promise", "Promise"),
            (EntityKind::MACHINE, "machine", "Machine"),
            (EntityKind::VIEW, "view", "View"),
            (EntityKind::SESSION, "session", "Session"),
            (EntityKind::USAGE, "usage", "Usage"),
            (EntityKind::THREAD, "thread", "Thread"),
            (EntityKind::DECISION, "decision", "Decision"),
            (EntityKind::ROLE, "role", "Role"),
            (EntityKind::VOUCHER, "voucher", "Voucher"),
            (EntityKind::CHALLENGE, "challenge", "Challenge"),
            (EntityKind::DOCUMENT, "document", "DigitalDocument"),
            (EntityKind::LIST, "list", "ItemList"),
            (EntityKind::TRIP, "trip", "Trip"),
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

    /// **A survivor's read names what was folded into it apart from its aliases,
    /// and says nothing when nothing was.** The key is `foldedNames`; the aliases a
    /// caller wrote stay under `alternateName`.
    #[test]
    fn a_survivor_names_what_was_folded_in_apart_from_its_aliases() {
        let entity = |folded: &[&str]| Entity {
            folded_names: folded.iter().map(|name| name.to_string()).collect(),
            id: EntityId::person("person:alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        };
        let folded = entity_json(&entity(&["Beta", "Bee"]));
        assert_eq!(folded["foldedNames"], serde_json::json!(["Beta", "Bee"]));
        assert_eq!(folded["alternateName"], serde_json::json!(["Al"]));
        let alone = entity_json(&entity(&[]));
        assert!(
            alone.get("foldedNames").is_none(),
            "a thing nothing was folded into carries no such key: {alone}"
        );
    }

    /// **A receipt keeps its aliases until they would take it past the ceiling.**
    /// The same entity is served twice: with a few aliases it is left whole, and
    /// with enough to pass one answer the aliases are replaced by a marker, their
    /// count and the read that returns them. The rest of the receipt stays.
    #[test]
    fn a_receipt_elides_its_aliases_only_when_they_would_pass_the_ceiling() {
        let entity = |count: usize| Entity {
            folded_names: Vec::new(),
            id: EntityId::person("person:alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: (0..count)
                .map(|i| format!("{i} {}", "x".repeat(150)))
                .collect(),
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        };

        let mut small = entity_json(&entity(3));
        let before = small.clone();
        elide_aliases_past_the_ceiling(&mut small);
        assert_eq!(small, before, "a receipt that fits is left whole");
        assert_eq!(small["alternateName"].as_array().map(Vec::len), Some(3));

        let mut large = entity_json(&entity(300));
        elide_aliases_past_the_ceiling(&mut large);
        assert_eq!(large["alternateName"], serde_json::Value::Null, "{large}");
        assert_eq!(large["alternateName_elided"], true, "{large}");
        assert_eq!(large["alternateName_count"], 300, "{large}");
        let how = large["how_to_read"].as_str().unwrap_or_default();
        assert!(
            how.contains("recall") && how.contains("person:alpha"),
            "{large}"
        );
        assert_eq!(large["id"], "person:alpha", "the rest stays: {large}");
        assert!(
            large.to_string().chars().count() <= jojobot_domain::text::ANSWER_CEILING,
            "the elided receipt fits one answer"
        );
    }

    /// **The names of what was folded in are elided with the aliases when they
    /// are what takes a receipt past the ceiling.** A survivor can hold a long
    /// list of them, and a receipt that left them whole would be over one answer.
    /// The same marker, count and route as the aliases carry. **Paired**: a
    /// receipt whose folded names fit is left whole.
    #[test]
    fn a_receipt_elides_the_folded_names_with_the_aliases_past_the_ceiling() {
        let entity = |folded: usize| Entity {
            folded_names: (0..folded)
                .map(|i| format!("{i} {}", "y".repeat(150)))
                .collect(),
            id: EntityId::person("person:alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        };

        let mut small = entity_json(&entity(3));
        let before = small.clone();
        elide_aliases_past_the_ceiling(&mut small);
        assert_eq!(small, before, "a receipt that fits is left whole");

        let mut large = entity_json(&entity(300));
        elide_aliases_past_the_ceiling(&mut large);
        assert_eq!(large["foldedNames"], serde_json::Value::Null, "{large}");
        assert_eq!(large["foldedNames_elided"], true, "{large}");
        assert_eq!(large["foldedNames_count"], 300, "{large}");
        let how = large["how_to_read"].as_str().unwrap_or_default();
        assert!(
            how.contains("recall") && how.contains("person:alpha"),
            "{large}"
        );
        assert!(
            large.to_string().chars().count() <= jojobot_domain::text::ANSWER_CEILING,
            "the elided receipt fits one answer"
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
            folded_names: Vec::new(),
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

    /// 🚨 **A span's far end reaches the caller, beside its start.**
    ///
    /// `happened_through` is a real, stored, domain-correct field — migrated
    /// into its own column, round-tripped through the store, covered by the
    /// domain's own contract tests — and until now this renderer never put
    /// it on the wire at all: a caller could write the end of a span and
    /// never read it back, through any verb. Proven the same way
    /// `happened_at` already is above it: both ends present when both were
    /// given.
    #[test]
    fn a_facts_happened_through_reaches_the_caller_beside_happened_at() {
        let fact = Fact {
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            reaching: Default::default(),
            id: jojobot_domain::memory::FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "A festival that ran more than one day".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 4, 20),
            happened_at: Some(jiff::civil::date(2026, 4, 18)),
            happened_through: Some(jiff::civil::date(2026, 4, 20)),
            inserted_at: None,
            stale_after: None,
            receipt: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
        };
        let served = fact_json(&fact, jiff::civil::date(2026, 4, 20), None).to_string();
        assert!(
            served.contains("\"happened_at\":\"2026-04-18\""),
            "the span's start did not reach the caller: {served}",
        );
        assert!(
            served.contains("\"happened_through\":\"2026-04-20\""),
            "the span's far end did not reach the caller: {served}",
        );
    }
    /// **A claim an app wrote says so on the wire, as `recorded_by` beside its
    /// provenance, and a claim no app wrote has no such key.** The key is a
    /// top-level one and never a field, so a bot's own `fields` cannot spell it.
    #[test]
    fn a_claim_an_app_wrote_is_rendered_with_recorded_by_and_others_are_not() {
        let mut fact = Fact {
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            reaching: Default::default(),
            id: jojobot_domain::memory::FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "Deploy event: commit abc1234".into(),
            details: None,
            provenance: Provenance::Observation,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 10, 18),
            happened_at: None,
            happened_through: None,
            inserted_at: None,
            stale_after: None,
            receipt: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
        };
        let day = jiff::civil::date(2026, 10, 18);
        assert!(
            fact_json(&fact, day, None).get("recorded_by").is_none(),
            "a claim no app wrote carries no mark"
        );
        fact.app_stamp = Some(jojobot_domain::app::AppStamp {
            app: "springfield-ci".into(),
            grant: "record_deploy_event".into(),
            received_at: "2026-10-18T12:00:00Z".into(),
        });
        let served = fact_json(&fact, day, None);
        assert_eq!(served["recorded_by"]["app"], "springfield-ci");
        assert_eq!(served["recorded_by"]["grant"], "record_deploy_event");
        assert_eq!(served["recorded_by"]["received_at"], "2026-10-18T12:00:00Z");
        assert!(served["fields"].get("recorded_by").is_none());
    }
}
