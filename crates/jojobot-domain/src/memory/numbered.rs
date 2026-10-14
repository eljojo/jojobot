//! **A container can number what it holds.** A thing whose fields carry
//! `numbers_by` names the key its members are numbered by: every descendant of
//! it, however deep, that carries that key holds a positive whole number, and
//! no two hold the same one. Archived members keep theirs, so a number is never
//! reused.
//!
//! Nothing here knows what is being numbered. The engine reads a key a caller
//! named and a handle that ends in a number; it holds no word for rules,
//! episodes or issues.
//!
//! **The rules are pure and take what the store read.** A store reads the
//! entities and the folded fields of the things in question inside the
//! transaction that makes the write, hands them here, and refuses on the
//! answer, so the check and the write commit together. The fake does the same
//! over its own rows.

use std::collections::{BTreeMap, BTreeSet};

use super::guard::{EntityMatch, MatchReason};
use super::{Entity, EntityId, FormerHandle, MemoryError};

/// The key a collection carries to say what its members are numbered by.
pub const NUMBERS_BY: &str = "numbers_by";

/// **A positive whole number in its one spelling**: digits only, no sign, no
/// leading zero. Two spellings of one number would let `07` and `7` both be
/// held.
pub fn whole_number(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse::<u64>().ok().filter(|n| *n > 0)
}

/// **`<collection>/<N>` taken apart**: the handle of the collection and the
/// number. `None` for a handle that does not end in a slash and a whole number,
/// so every ordinary handle passes through untouched.
pub fn numbered_handle(id: &EntityId) -> Option<(EntityId, u64)> {
    let (collection, number) = id.as_str().rsplit_once('/')?;
    if number.trim() != number || !collection.contains(':') {
        return None;
    }
    Some((EntityId(collection.to_string()), whole_number(number)?))
}

/// One descendant of a collection and what it holds under the collection's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The member's handle.
    pub handle: EntityId,
    /// The value it holds under the key, as written; `None` when it holds none.
    pub value: Option<String>,
}

/// **A numbered collection as a write sees it**: the collection, the key, and
/// every descendant, archived ones included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbered {
    /// The collection's handle.
    pub collection: EntityId,
    /// The key its members are numbered by.
    pub key: String,
    /// Every descendant of the collection.
    pub members: Vec<Member>,
}

impl Numbered {
    /// The numbers held, those that are whole numbers.
    fn held(&self) -> impl Iterator<Item = u64> + '_ {
        self.members
            .iter()
            .filter_map(|m| m.value.as_deref().and_then(whole_number))
    }

    /// **The largest number held, archived members included, plus one.** There
    /// is no allocator: this is a read, and a race for it is caught by the guard.
    pub fn next(&self) -> u64 {
        self.held().max().unwrap_or(0) + 1
    }

    /// The member holding `number`, if one does.
    pub fn holder_of(&self, number: u64) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| m.value.as_deref().and_then(whole_number) == Some(number))
    }

    /// The handle a member holding `number` answers to through this collection.
    pub fn handle_for(&self, number: u64) -> EntityId {
        EntityId(format!("{}/{number}", self.collection))
    }

    /// **The handles of the members that hold a number**, in number order, as
    /// `<collection>/<N>` — what a miss offers instead of a guess.
    pub fn handles(&self) -> Vec<EntityId> {
        let mut numbers: Vec<u64> = self.held().collect();
        numbers.sort_unstable();
        numbers.dedup();
        numbers.into_iter().map(|n| self.handle_for(n)).collect()
    }
}

/// **Every descendant of `root`, however deep**, by the parent pointers. Merged
/// things are not members: a fold moved what they held to the survivor. A cycle
/// in the pointers ends the walk rather than looping.
pub fn descendants<'a>(root: &EntityId, entities: &'a [Entity]) -> Vec<&'a Entity> {
    let mut found: Vec<&Entity> = Vec::new();
    let mut seen: BTreeSet<&EntityId> = BTreeSet::from([root]);
    let mut frontier: Vec<&EntityId> = vec![root];
    while let Some(parent) = frontier.pop() {
        for entity in entities
            .iter()
            .filter(|e| e.parent.as_ref() == Some(parent) && e.merged_into.is_none())
        {
            if seen.insert(&entity.id) {
                found.push(entity);
                frontier.push(&entity.id);
            }
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// **The numbered collection a thing is, if it is one**: the key it carries and
/// every descendant with what it holds under that key.
pub fn collection_of(
    collection: &EntityId,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Option<Numbered> {
    let key = fields.get(collection)?.get(NUMBERS_BY)?.trim().to_string();
    if key.is_empty() {
        return None;
    }
    let members = descendants(collection, entities)
        .into_iter()
        .map(|e| Member {
            handle: e.id.clone(),
            value: fields.get(&e.id).and_then(|f| f.get(&key)).cloned(),
        })
        .collect();
    Some(Numbered {
        collection: collection.clone(),
        key,
        members,
    })
}

/// **Every numbered collection a thing sits under**, nearest first: the
/// ancestors that carry `numbers_by`, each with its members. The thing itself
/// is not its own collection.
pub fn scopes_of(
    subject: &EntityId,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Vec<Numbered> {
    let mut scopes = Vec::new();
    let mut seen: BTreeSet<EntityId> = BTreeSet::from([subject.clone()]);
    let mut at = entities
        .iter()
        .find(|e| &e.id == subject)
        .and_then(|e| e.parent.clone());
    while let Some(ancestor) = at {
        if !seen.insert(ancestor.clone()) {
            break;
        }
        if let Some(scope) = collection_of(&ancestor, entities, fields) {
            scopes.push(scope);
        }
        at = entities
            .iter()
            .find(|e| e.id == ancestor)
            .and_then(|e| e.parent.clone());
    }
    scopes
}

/// **The member of `collection` holding `number`**, if the collection is
/// numbered and one does.
pub fn member_numbered(
    collection: &EntityId,
    number: u64,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Option<EntityId> {
    collection_of(collection, entities, fields)?
        .holder_of(number)
        .map(|m| m.handle.clone())
}

/// **Refuse a write that would leave a member with a bad number or another
/// member's.** `after` is the subject's folded fields as the write leaves them;
/// `scopes` are the collections it sits under ([`scopes_of`]). A subject that
/// holds no number under a collection's key is not asked anything: a member
/// that carries no number is allowed. There is no override (rule 472).
pub fn refuses_numbering(
    subject: &EntityId,
    after: &BTreeMap<String, String>,
    scopes: &[Numbered],
) -> Option<MemoryError> {
    for scope in scopes {
        let Some(raw) = after.get(&scope.key) else {
            continue;
        };
        let Some(number) = whole_number(raw) else {
            return Some(MemoryError::NotANumber {
                collection: scope.collection.to_string(),
                key: scope.key.clone(),
                value: raw.clone(),
            });
        };
        if let Some(holder) = scope.members.iter().find(|m| {
            &m.handle != subject && m.value.as_deref().and_then(whole_number) == Some(number)
        }) {
            return Some(MemoryError::NumberTaken {
                collection: scope.collection.to_string(),
                key: scope.key.clone(),
                number,
                holder: holder.handle.to_string(),
                next: scope.next(),
            });
        }
    }
    None
}

/// The day every derived former handle carries. **No rename happened**: the
/// entry exists because a member holds a number, and moves when the number does.
pub const NUMBERED_DAY: jiff::civil::Date = jiff::civil::date(1970, 1, 1);

/// **Every `<collection>/<N>` handle, as a former handle**: each numbered
/// member answers to the handle its number makes, through the same second tier
/// a renamed thing answers to its old name through, so every place that
/// resolves a handle resolves these with no code of its own (rule 498). They
/// are derived on every read and stored nowhere, so a number that moves takes
/// its handle with it, and a pointer, which holds a permanent id, never holds
/// one.
pub fn derived_formers(
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Vec<FormerHandle> {
    let mut derived = Vec::new();
    for collection in entities {
        let Some(scope) = collection_of(&collection.id, entities, fields) else {
            continue;
        };
        for handle in scope.handles() {
            let Some((_, number)) = numbered_handle(&handle) else {
                continue;
            };
            let Some(holder) = scope.holder_of(number) else {
                continue;
            };
            let Some(badge) = entities
                .iter()
                .find(|e| e.id == holder.handle)
                .and_then(|e| e.badge.clone())
            else {
                continue;
            };
            derived.push(FormerHandle {
                former: handle,
                badge,
                changed_at: NUMBERED_DAY,
            });
        }
    }
    derived
}

/// **What a miss on `<collection>/<N>` offers**: the numbers the collection
/// holds, as handles, each with the member's name. `None` when the handle is not
/// a numbered one or its collection is not a numbered collection, so the
/// ordinary near-miss screen answers instead.
pub fn nearest_numbers(
    asked: &EntityId,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Option<Vec<EntityMatch>> {
    let (collection, _) = numbered_handle(asked)?;
    let scope = collection_of(&collection, entities, fields)?;
    Some(
        scope
            .handles()
            .into_iter()
            .filter_map(|handle| {
                let (_, number) = numbered_handle(&handle)?;
                let holder = scope.holder_of(number)?;
                let member = entities.iter().find(|e| e.id == holder.handle)?;
                Some(EntityMatch {
                    handle,
                    kind: member.kind,
                    name: member.name.clone(),
                    source: member.source.clone(),
                    reason: MatchReason::HeldNumber,
                })
            })
            .collect(),
    )
}

/// What a read of a numbered collection adds: the key and the number the next
/// member takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbering {
    /// The key the members are numbered by.
    pub key: String,
    /// The largest number held, archived included, plus one.
    pub next_number: u64,
}

impl From<&Numbered> for Numbering {
    fn from(scope: &Numbered) -> Self {
        Numbering {
            key: scope.key.clone(),
            next_number: scope.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(raw: &str) -> EntityId {
        EntityId(raw.to_string())
    }

    #[test]
    fn a_number_has_one_spelling() {
        assert_eq!(whole_number("7"), Some(7));
        assert_eq!(whole_number(" 12 "), Some(12));
        for refused in ["", "0", "07", "-3", "+3", "2.5", "1e3", "seven", "1 2"] {
            assert_eq!(whole_number(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_handle_ending_in_a_number_is_taken_apart_and_no_other_is() {
        assert_eq!(
            numbered_handle(&handle("topic:contract-ordinances/32")),
            Some((handle("topic:contract-ordinances"), 32))
        );
        for plain in [
            "topic:contract-ordinances",
            "topic:contract-ordinances/",
            "topic:contract-ordinances/0",
            "topic:contract-ordinances/07",
            "topic:contract-ordinances/two",
            "topic:contract-ordinances/ 2",
            "/2",
        ] {
            assert_eq!(numbered_handle(&handle(plain)), None, "{plain}");
        }
    }
}
