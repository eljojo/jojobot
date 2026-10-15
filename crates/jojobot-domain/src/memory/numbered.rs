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
/// things are not members: a merge appends an event and moves no row, and the
/// survivor serves what the folded thing held, so the folded thing's own number
/// is held by the survivor or by nobody ([`refuses_numbering`] asks which). A
/// cycle in the pointers ends the walk rather than looping.
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

/// **One change a write makes to the members of a numbered collection.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The thing holds `value` under the collection's key after the write. A
    /// thing that is not a member yet becomes one. `renumbers` is true when the
    /// writer named the number, so the number the member held before may go.
    Set {
        handle: EntityId,
        value: Option<String>,
        renumbers: bool,
    },
    /// The thing is not a member after the write.
    Leave(EntityId),
}

/// **What a write does to the things it touches**, in terms of what each holds
/// once the write has landed. Every write path that changes what a collection or
/// a member holds describes itself this way and asks [`Reach::refuses`]: one
/// question, however the number came to change.
#[derive(Debug, Clone, Default)]
pub struct Change {
    /// The things the write leaves where they are, holding these fields as
    /// served.
    pub holds: Vec<(EntityId, BTreeMap<String, String>)>,
    /// The things the write stops being members of anything: a merged-away
    /// thing, whose number is held on only if the survivor now serves it.
    pub leaves: Vec<EntityId>,
    /// The things the write moves to another parent, with what each holds: the
    /// collections they sit in change with them. A reparent moves the thing and
    /// everything under it; a merge moves what was under the duplicate to the
    /// survivor.
    pub moved: Vec<(EntityId, BTreeMap<String, String>)>,
    /// Whether the writer named the number itself (a capture or an edit that
    /// sets the key), which may give up the number the member held.
    pub renumbers: bool,
}

impl Change {
    /// Every thing the change names.
    pub fn touched(&self) -> impl Iterator<Item = &EntityId> {
        self.holds
            .iter()
            .chain(self.moved.iter())
            .map(|(handle, _)| handle)
            .chain(self.leaves.iter())
    }
}

/// **The collections a write touches, as they stand before it**, and the ones a
/// moved thing joins. Read by the store inside the transaction of the write (or
/// by the fake over its rows), then asked once.
#[derive(Debug, Clone, Default)]
pub struct Reach {
    /// Every numbered collection above a touched thing, and above the new parent.
    pub scopes: Vec<Numbered>,
    /// The collections at or above the new parent of a thing that moves.
    pub chain: BTreeSet<EntityId>,
}

impl Numbered {
    /// **The edits a change makes to this collection.** `joins` says whether the
    /// things that move end up inside it.
    fn edits_for(&self, change: &Change, joins: bool) -> Vec<Edit> {
        let is_member = |h: &EntityId| self.members.iter().any(|m| &m.handle == h);
        let mut edits = Vec::new();
        for handle in &change.leaves {
            if is_member(handle) {
                edits.push(Edit::Leave(handle.clone()));
            }
        }
        for (handle, fields) in &change.holds {
            if is_member(handle) {
                edits.push(Edit::Set {
                    handle: handle.clone(),
                    value: fields.get(&self.key).cloned(),
                    renumbers: change.renumbers,
                });
            }
        }
        for (handle, fields) in &change.moved {
            if joins {
                edits.push(Edit::Set {
                    handle: handle.clone(),
                    value: fields.get(&self.key).cloned(),
                    renumbers: false,
                });
            } else if is_member(handle) {
                edits.push(Edit::Leave(handle.clone()));
            }
        }
        edits
    }
}

impl Reach {
    /// **Refuse a write that would leave a member with a bad number or another
    /// member's, or leave a number nobody holds.** There is no override (rule
    /// 472).
    pub fn refuses(&self, change: &Change) -> Option<MemoryError> {
        // A collection inside the moved subtree moves with its members and
        // changes nothing for them.
        let moved: BTreeSet<&EntityId> = change.moved.iter().map(|(h, _)| h).collect();
        for scope in &self.scopes {
            if moved.contains(&scope.collection) {
                continue;
            }
            let edits = scope.edits_for(change, self.chain.contains(&scope.collection));
            if let Some(refusal) = refuses_numbering(scope, &edits) {
                return Some(refusal);
            }
        }
        None
    }
}

/// **Refuse the edits that would leave one collection with a bad number, with
/// two holders of a number, or with a number nobody holds.**
///
/// Asked in this order: a value that is no number; a number two members hold
/// after the write, naming the one the write did not touch; a number held before
/// and held by nobody after. The last is how a number keeps naming one member
/// for ever: a member that left the collection, was merged into a thing outside
/// it, lost the claim that gave it the number or had the key taken off all free
/// a number, and `next` could hand it to a different member. A member that
/// *replaces* its number with another through a write that names it is the one
/// way a number goes, and it is the writer's.
pub fn refuses_numbering(before: &Numbered, edits: &[Edit]) -> Option<MemoryError> {
    let mut after = before.members.clone();
    let mut touched: BTreeSet<&EntityId> = BTreeSet::new();
    for edit in edits {
        match edit {
            Edit::Leave(handle) => after.retain(|m| &m.handle != handle),
            Edit::Set { handle, value, .. } => {
                touched.insert(handle);
                match after.iter_mut().find(|m| &m.handle == handle) {
                    Some(member) => member.value = value.clone(),
                    None => after.push(Member {
                        handle: handle.clone(),
                        value: value.clone(),
                    }),
                }
            }
        }
    }
    let held_before = |handle: &EntityId| {
        before
            .members
            .iter()
            .find(|m| &m.handle == handle)
            .and_then(|m| m.value.as_deref())
    };
    // A value that is no number, in a member the write set to it. A member that
    // already held it, and is only being read through a merge, is not refused for
    // what it held before.
    for edit in edits {
        if let Edit::Set {
            handle,
            value: Some(raw),
            ..
        } = edit
            && whole_number(raw).is_none()
            && held_before(handle) != Some(raw.as_str())
        {
            return Some(MemoryError::NotANumber {
                collection: before.collection.to_string(),
                key: before.key.clone(),
                value: raw.clone(),
            });
        }
    }
    let now = Numbered {
        collection: before.collection.clone(),
        key: before.key.clone(),
        members: after,
    };
    for handle in &touched {
        let Some(number) = now
            .members
            .iter()
            .find(|m| &m.handle == *handle)
            .and_then(|m| m.value.as_deref())
            .and_then(whole_number)
        else {
            continue;
        };
        let rivals: Vec<&Member> = now
            .members
            .iter()
            .filter(|m| {
                &m.handle != *handle && m.value.as_deref().and_then(whole_number) == Some(number)
            })
            .collect();
        if let Some(holder) = rivals
            .iter()
            .find(|m| !touched.contains(&m.handle))
            .or(rivals.first())
        {
            return Some(MemoryError::NumberTaken {
                collection: before.collection.to_string(),
                key: before.key.clone(),
                number,
                holder: holder.handle.to_string(),
                next: now.next(),
            });
        }
    }
    for member in &before.members {
        let Some(number) = member.value.as_deref().and_then(whole_number) else {
            continue;
        };
        if now.holder_of(number).is_some() {
            continue;
        }
        let replaced = edits.iter().any(|edit| {
            matches!(
                edit,
                Edit::Set { handle, value: Some(_), renumbers: true } if handle == &member.handle
            )
        });
        if !replaced {
            return Some(MemoryError::NumberFreed {
                collection: before.collection.to_string(),
                key: before.key.clone(),
                number,
                holder: member.handle.to_string(),
            });
        }
    }
    None
}

/// **The collections a change touches**, over a roster and what each thing
/// holds: the ones above any thing it holds or releases, and for a move the ones
/// at or above the new parent. The fake reads its rows this way; a store that
/// reads a transaction builds the same [`Reach`] from its own queries.
pub fn reach_of(
    change: &Change,
    under: Option<&EntityId>,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
) -> Reach {
    let mut reach = Reach::default();
    let add = |scope: Numbered, reach: &mut Reach| {
        if !reach
            .scopes
            .iter()
            .any(|s| s.collection == scope.collection)
        {
            reach.scopes.push(scope);
        }
    };
    for handle in change.touched() {
        for scope in scopes_of(handle, entities, fields) {
            add(scope, &mut reach);
        }
    }
    if let Some(parent) = under {
        let mut above = scopes_of(parent, entities, fields);
        if let Some(own) = collection_of(parent, entities, fields) {
            above.insert(0, own);
        }
        for scope in above {
            reach.chain.insert(scope.collection.clone());
            add(scope, &mut reach);
        }
    }
    reach
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
                    told_apart_by: None,
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

    /// **A numbered handle is a handle to read, write to and point at, and it
    /// is never a handle to make.** A thing created or renamed to one would
    /// shadow the member the number names, because a direct match wins.
    #[test]
    fn a_numbered_handle_is_a_handle_to_take_and_never_one_to_make() {
        use crate::memory::testing::InMemoryMemory;
        use crate::memory::{validate_entity, validate_new_handle, validate_subject};
        // A booted store is what loads the kinds a handle's kind is asked of.
        let _booted = InMemoryMemory::booted();
        for taken in [
            "topic:contract-ordinances/2",
            "topic:contract-ordinances/2/3",
            "topic:contract-ordinances/12",
        ] {
            assert!(validate_subject(&handle(taken)).is_ok(), "{taken}");
            assert!(validate_new_handle(&handle(taken)).is_err(), "{taken}");
            assert!(
                validate_entity(&handle(taken), "Name", &[], "test", None, None).is_err(),
                "{taken} is not a handle to make"
            );
        }
        for refused in [
            "topic:contract-ordinances/0",
            "topic:contract-ordinances/07",
            "topic:contract-ordinances/",
            "topic:contract-ordinances//2",
            "topic:contract-ordinances/two",
            "topic:/2",
            "topic:contract-ordinances/2/",
        ] {
            assert!(validate_subject(&handle(refused)).is_err(), "{refused}");
        }
        assert!(validate_new_handle(&handle("topic:contract-ordinances")).is_ok());
        assert!(
            validate_entity(
                &handle("topic:contract-ordinances"),
                "Name",
                &[],
                "test",
                None,
                Some(&handle("topic:contract-ordinances/2")),
            )
            .is_ok(),
            "a numbered handle may be a parent"
        );
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
