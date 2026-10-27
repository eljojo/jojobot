//! **What a record's fields point at.** The one definition of which handles a
//! field value names, which of them the store draws a link row for, how a link is
//! repointed when a thing is merged, and the order of the holders in a list of who
//! points at a name. A reader that holds records by what they name, a store that
//! draws rows and the fold that patches its lists all ask here.

use super::*;

pub mod at_write;
#[cfg(test)]
mod link_rule_tests;

/// Resolve only marked items from the value owner's stored fields. A grouped
/// reader can decline an unplaced id and leave that document to the full read.
pub fn marked_field_links(
    stored: &BTreeMap<String, String>,
    mut handle: impl FnMut(&str) -> Option<EntityId>,
) -> Option<BTreeMap<String, Vec<EntityId>>> {
    let mut links = BTreeMap::new();
    for (key, value) in stored {
        if reserved_key(key) {
            continue;
        }
        let mut named = Vec::new();
        for item in value.split(',') {
            if let Some(id) = stored_handle_id(item) {
                let target = handle(id)?;
                if !named.contains(&target) {
                    named.push(target);
                }
            }
        }
        if !named.is_empty() {
            links.insert(key.clone(), named);
        }
    }
    Some(links)
}

fn bare_reference_name(key: &str, item: &str, points_at: Option<EntityKind>) -> MemoryError {
    let kind = points_at.map_or("kind", EntityKind::as_token);
    MemoryError::InvalidFact(format!(
        "'{key}' holds a reference, but '{item}' is a bare name. \
         Write the full handle '{kind}:{item}'."
    ))
}

/// A lone name under a reference key must be the badge of an existing thing.
/// Its alphabet alone does not make it a link.
pub fn lone_reference_names<'a>(
    fields: &'a BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Vec<(&'a str, MemoryError)> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let item = value.trim();
            if !slug_shaped(item) {
                return None;
            }
            let field = declared
                .iter()
                .filter_map(|d| d.field(key))
                .find(|f| f.holds == types::ValueType::Reference)?;
            Some((item, bare_reference_name(key, item, field.points_at)))
        })
        .collect()
}

/// **The handles one field value names**, whatever its key: the value itself
/// when it is, whole, a handle, or each item of a comma-joined list that is a
/// handle (decision log 374, rule 646). Loose items beside them stay text, and a
/// handle inside a longer string is prose.
///
/// **One rule for the read and the write.** The unscoped walk and the search
/// index read links through this, and the write guard asks of every value it is
/// handed whether what it names exists. Two copies would drift, and a link the
/// walk draws would be one the guard never asked about.
pub fn handles_in_value(value: &str) -> Vec<EntityId> {
    at_write::handles_in_value(value)
}

/// **The entities a write names through its field values**, which must already
/// exist exactly as an edge's object must: the items of a declared reference
/// key, and the handles any other key's value names (see [`handles_in_value`]).
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
/// phrase does not hold the key is the floor's question ([`guard_fit`]). A bare
/// name is refused with the full handle to write, before lowering can mistake
/// it for an opaque id.
///
/// **Whatever the key**: no declaration is read, so a key any type calls a
/// reference and a key nobody declared name their handles the same way.
pub fn referenced_by(fields: &BTreeMap<String, String>) -> Vec<EntityId> {
    at_write::referenced_by(fields)
}

/// **Every reference-typed key on a fields map, with the items its value
/// holds, kept apart by key** — what [`referenced_by`] flattens for the
/// existence check, an adapter needs back in shape to resolve each item and
/// put the key back together.
///
/// Same key-matching as [`referenced_by`]: a key any declaration calls a
/// reference counts. **Unlike `referenced_by`, an item is kept whichever
/// shape it holds** — never filtered to what looks like a well-formed
/// handle. A caller lowering a value writes handles and a caller composing
/// one reads permanent ids, and neither shape passes a filter built for the
/// other: an id has no `kind:` prefix to find, and filtering it out would
/// silently compose an empty value rather than the id an adapter's own
/// resolver already knows what to do with. Malformed input is exactly what
/// that resolver's own fallback — leave it as written — exists to answer.
pub fn reference_field_values(
    fields: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Vec<(String, Vec<String>)> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let field = declared
                .iter()
                .filter_map(|d| d.field(key))
                .find(|f| f.holds == types::ValueType::Reference)?;
            let items: Vec<String> = field
                .items(value)
                .into_iter()
                .map(|item| item.trim().to_string())
                .collect();
            Some((key.clone(), items))
        })
        .collect()
}

/// **The handle-valued fields under keys no type calls a reference**, with the
/// handles each value names (see [`handles_in_value`]) — what a store lowers to
/// permanent ids on a write, beside [`reference_field_values`], which has the
/// declared ones. A key a declaration calls a reference is left to that, so the
/// two never touch one key.
pub fn handle_field_values(
    fields: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Vec<(String, Vec<String>)> {
    let referenced: Vec<String> = reference_field_values(fields, declared)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    fields
        .iter()
        .filter(|(key, _)| !referenced.contains(key))
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

/// **How a handle under an undeclared key is stored**: the permanent id the
/// thing wears, behind the mark a stored mention wears.
///
/// An undeclared key says nothing about what it holds, so a stored id could not
/// be told from a word that happens to spell one. The mark can: no handle starts
/// with it. A declared reference key needs none, because the declaration says
/// the value is a reference.
pub fn marked_stored_handle(id: &str) -> String {
    format!("{}{id}", mention::MARK)
}

/// **The id behind a stored handle**, when the item wears the mark.
pub fn stored_handle_id(item: &str) -> Option<&str> {
    item.trim().strip_prefix(mention::MARK)
}

/// **The things one stored field write points at**, by the permanent id each
/// is stored as — the rows the link table holds for it.
///
/// A declared reference key points at every item of its value; any other key
/// points at the ids behind the mark when EVERY item of the value wears it,
/// which is the rule a read serves them by. A value that is neither points at
/// nothing. An item of a reference key that is no id and no handle — loose prose
/// under a reference key — names no thing and is left out. Ordered and
/// deduplicated, so the same write always gives the same rows.
pub fn field_link_targets(value: &str) -> Vec<String> {
    at_write::field_link_targets(value)
}

/// **The value of one stored field write with every link to `from` pointing at
/// `to` instead**, or `None` when the value links to no such thing.
///
/// A fold uses it: a field naming the folded side is a link (rule 374), and it
/// must name the survivor afterwards. An item keeps its own spelling, so a
/// marked id stays marked. **A list that already held `to` loses the repointed
/// item**, so it never names the same thing twice; the remaining items keep
/// their order. Only the items [`field_link_targets`] reads as links are
/// touched.
pub fn repoint_field_value(value: &str, from: &str, to: &str) -> Option<String> {
    at_write::repoint_field_value(value, from, to)
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
    survivor_of: impl FnMut(&EntityId) -> Result<Option<EntityId>, E>,
) -> Result<Option<String>, E> {
    at_write::forward_field_value(value, survivor_of)
}

/// **The order of the holders in a list of who points at a name, stated once.**
/// Holders come in ascending order of the key their records are filed under as
/// they are served: a thing folded into another is served as the survivor, so its
/// records stand at the survivor's key. Within a holder the records keep the order
/// the holder serves them in. Every store answers `referring_to` in this order and
/// the fold patches its lists by it, so an answer from memory and one after a
/// restart read the same.
pub fn referring_holder_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// **The holders of a list of who points at a name, once each and in that
/// order**, for a store that has found them in the order of its own index.
pub fn referring_holders(mut holders: Vec<EntityId>) -> Vec<EntityId> {
    holders.sort_by(|a, b| referring_holder_order(a.as_str(), b.as_str()));
    holders.dedup();
    holders
}

/// **Whether any field of a record names `target`**, the question
/// [`Memory::referring_to`] asks of every record.
///
/// A key some type declared a reference holds its value as items, so a list
/// names the target when ANY of its items is the handle. Every other key names
/// the handles [`handles_in_value`] reads out of it, which is what keeps a
/// handle inside a longer string, or in a list with an item that is no handle,
/// from counting as more than it is.
pub fn fields_name_target(fields: &BTreeMap<String, String>, target: &EntityId) -> bool {
    at_write::fields_name_target(fields, target)
}

/// **Every name a record's fields point at**, as [`fields_name_target`] reads
/// them: the handles [`handles_in_value`] reads out of any key's value. The one
/// definition of what a field names, so a reader that holds the records by what
/// they name and a reader that asks one record cannot come to disagree.
pub fn names_in_fields(fields: &BTreeMap<String, String>) -> Vec<String> {
    at_write::names_in_fields(fields)
}

/// **Whether a record's fields link to `target`**: whether the marks the record
/// was stored with say so ([`Fact::field_links`]).
pub fn fact_names_target(fact: &Fact, target: &EntityId) -> bool {
    fact.field_links.values().flatten().any(|id| id == target)
}

/// **Every name a record's fields link to**: the counterpart of
/// [`fact_names_target`] for a reader that holds the records by what they name.
pub fn names_in_fact(fact: &Fact) -> Vec<String> {
    fact.field_links
        .values()
        .flatten()
        .map(|id| id.to_string())
        .collect()
}
