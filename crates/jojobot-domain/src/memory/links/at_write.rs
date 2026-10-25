//! **The link decision of rule 646, ahead of its switch.** Whether a field value
//! links is decided from the value alone, at the write, and stored as the mark
//! on the permanent id; no declaration decides it. These are the functions the
//! switch ([`super::link_rule`]) selects when it is [`super::LinkRule::On`]; the
//! same-named functions one level up answer today's rule until then, and the
//! log-only summary asks these what the rule would do.

use super::*;

/// **The handles one field value names**, whatever its key: the value itself
/// when it is, whole, a handle, or each item of a comma-joined list that is a
/// handle (decision log 355, 374, 646). Loose items beside them stay text, and
/// a handle inside a longer string is prose.
///
/// **The write guard asks whether these handles exist.** The write stores the
/// link decision as marked permanent ids. Later readers use the resulting
/// field-link metadata, never this parser on served text: an unmarked handle
/// spelling may be an older decision to leave text unlinked. A type declared
/// afterwards neither makes nor removes a link.
pub fn handles_in_value(value: &str) -> Vec<EntityId> {
    let whole = EntityId(value.trim().to_string());
    if validate_subject(&whole).is_ok() {
        return vec![whole];
    }
    value
        .split(',')
        .map(|item| EntityId(item.trim().to_string()))
        .filter(|id| validate_subject(id).is_ok())
        .collect()
}

/// **The entities a write names through its field values**, which must already
/// exist exactly as an edge's object must: the handles any key's value names
/// (see [`handles_in_value`]).
///
/// A reference is a walkable link, so a value naming nothing leaves the same
/// hole an edge into a missing entity leaves. This is rule 3 — everything a
/// write NAMES must already exist — reached through a key rather than through
/// an edge, which is why it is checked against what the write puts there rather
/// than against the thing's whole state. Checking every reference the thing
/// carries would make a link that went missing a wall in front of every later
/// repair.
///
/// **Only a value that is a well-formed handle is named.** A reference key
/// holding a phrase points at nothing and claims to point at nothing; asking
/// whether it exists would refuse loose prose over a link nobody drew. That the
/// phrase does not hold the key is the floor's question ([`guard_fit`]).
pub fn referenced_by(fields: &BTreeMap<String, String>) -> Vec<EntityId> {
    fields
        .values()
        .flat_map(|value| handles_in_value(value))
        .collect()
}

/// **The handles each field value names**, kept apart by key — what a store
/// lowers to permanent ids on a write (see [`handles_in_value`]), whatever the
/// key and whether or not any type declares it.
pub fn handle_field_values(fields: &BTreeMap<String, String>) -> Vec<(String, Vec<String>)> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let handles = handles_in_value(value);
            (!handles.is_empty()).then(|| {
                (
                    key.clone(),
                    handles.iter().map(|id| id.to_string()).collect(),
                )
            })
        })
        .collect()
}

/// **How a handle in a field value is stored**: the permanent id the thing
/// wears, behind the mark a stored mention wears.
///
/// A value says nothing about what it holds, so a stored id could not be told
/// from a word that happens to spell one. The mark can: no handle starts with
/// it. Every handle a field value names is stored so, under every key; the mark
/// is the whole of the decision that the item is a link.
pub fn marked_stored_handle(id: &str) -> String {
    format!("{}{id}", mention::MARK)
}

/// **The id behind a stored handle**, when the item wears the mark.
pub fn stored_handle_id(item: &str) -> Option<&str> {
    item.trim().strip_prefix(mention::MARK)
}

/// **The things one stored field write points at**, by the permanent id each
/// item that wears the mark is stored as — the rows the link table holds for it.
///
/// The mark was put on at the write and is the only thing read here: no
/// declaration, now or then, decides it. Ordered and deduplicated, so the same
/// write always gives the same rows.
pub fn field_link_targets(value: &str) -> Vec<String> {
    let mut targets: Vec<String> = value
        .split(',')
        .filter_map(stored_handle_id)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

/// **The value of one stored field write with every link to `from` pointing at
/// `to` instead**, or `None` when the value links to no such thing.
///
/// A fold uses it: a field naming the folded side is a link (rule 374), and it
/// must name the survivor afterwards. An item keeps its own spelling, so loose
/// text beside a link is left as it was written.
/// **A list that already held `to` loses the repointed item**, so it never
/// names the same thing twice; the remaining items keep their order.
pub fn repoint_field_value(value: &str, from: &str, to: &str) -> Option<String> {
    if !field_link_targets(value).iter().any(|id| id == from) {
        return None;
    }
    let repointed: Vec<(Option<String>, String)> = value
        .split(',')
        .map(|item| {
            let item = item.trim().to_string();
            match stored_handle_id(&item) {
                Some(id) if id == from => (Some(to.to_string()), marked_stored_handle(to)),
                Some(id) => (Some(id.to_string()), item),
                None => (None, item),
            }
        })
        .collect();
    // Loose text is never merged with other loose text; only links name a thing.
    let mut seen: Vec<&str> = Vec::new();
    let mut kept: Vec<&str> = Vec::new();
    for (id, item) in &repointed {
        if let Some(id) = id {
            if seen.contains(&id.as_str()) {
                continue;
            }
            seen.push(id);
        }
        kept.push(item);
    }
    Some(kept.join(", "))
}

/// **The value of one stored field write with every link in it served as the
/// thing it became**, or `None` when no link in it names a folded thing.
///
/// Each link is followed to the end of the merges by the one thing walk, so a
/// survivor folded in its turn is followed, and a loop among the merges is the
/// walk's [`claim_walk::MergeLoop`] and not an answer. The value is rewritten by
/// the rule [`repoint_field_value`] gives a merge: an item keeps its own
/// spelling and a list never names the same thing twice. `survivor_of` is asked
/// for the survivor of one thing and answers `None` when nothing folded it.
pub fn forward_field_value<E: From<claim_walk::MergeLoop>>(
    value: &str,
    mut survivor_of: impl FnMut(&EntityId) -> Result<Option<EntityId>, E>,
) -> Result<Option<String>, E> {
    let mut forwarded = value.to_string();
    let mut changed = false;
    for target in field_link_targets(value) {
        let end = claim_walk::follow_thing(EntityId(target.clone()), &mut survivor_of)?;
        if end.as_str() == target {
            continue;
        }
        if let Some(now) = repoint_field_value(&forwarded, &target, end.as_str()) {
            forwarded = now;
            changed = true;
        }
    }
    Ok(changed.then_some(forwarded))
}

/// **Whether any field of a record names `target`**, the question
/// [`Memory::referring_to`] asks of every record: whether one of the handles
/// [`handles_in_value`] reads out of a field is it, which is what keeps a handle
/// inside a longer string from counting as more than it is.
pub fn fields_name_target(fields: &BTreeMap<String, String>, target: &EntityId) -> bool {
    names_in_fields(fields)
        .iter()
        .any(|named| named == target.as_str())
}

/// **Every name a record's fields point at**, as [`fields_name_target`] reads
/// them: the handles [`handles_in_value`] reads out of each value. The one
/// definition of what a field names, so a reader that holds the records by what
/// they name and a reader that asks one record cannot come to disagree.
pub fn names_in_fields(fields: &BTreeMap<String, String>) -> Vec<String> {
    fields
        .values()
        .flat_map(|value| handles_in_value(value))
        .map(|named| named.0)
        .collect()
}
