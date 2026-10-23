//! **A container can number what it holds.** A thing whose fields carry
//! `numbers_by` names the key its members are numbered by: every descendant of
//! it, however deep, that carries that key holds a positive whole number, and
//! no two hold the same one. Archived members keep theirs, and a number a
//! retraction took back is remembered as issued, so a number is never reused.
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
use super::{
    Entity, EntityId, FactStatus, FormerHandle, KeyWrite, Memory, MemoryError, resolve_handle,
};

/// The key a collection carries to say what its members are numbered by.
pub const NUMBERS_BY: &str = "numbers_by";

/// **A positive whole number in its one spelling**: digits only, no sign, no
/// leading zero. Two spellings of one number would let `07` and `7` both be
/// held. **The largest number a machine holds is no number**: the next number is
/// the largest held plus one, so a member holding it would leave no next, and
/// nothing can hold what the arithmetic cannot step past.
pub fn whole_number(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse::<u64>().ok().filter(|n| *n > 0 && *n < u64::MAX)
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
    /// **The numbers its claims held that a retraction took back**, issued by
    /// this collection: see [`Retired`]. They stay with the member, so a member
    /// that leaves takes its record out of the collection, which the guard refuses.
    pub retired: BTreeSet<u64>,
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

/// **A number a retracted claim held, and the moment of that claim's newest
/// write** (`None` when the store stamped none).
pub type TakenBack = (u64, Option<jiff::Timestamp>);

/// **What a store reads beside the fields to know which numbers a collection
/// issued**: the numbers the claims of each thing held that are no longer current,
/// by key, and for each thing that numbers its members the moment it began
/// numbering by its current key (`None` when unknown). They are separate from a
/// member's held number, so the general fold of a thing's fields is unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Retired {
    /// Per thing, per key: the numbers its taken-back claims held.
    pub taken_back: BTreeMap<EntityId, BTreeMap<String, Vec<TakenBack>>>,
    /// Per collection: when it began numbering by the key it numbers by now.
    pub founded: BTreeMap<EntityId, Option<jiff::Timestamp>>,
    /// Per member: when it began to sit in the collection it sits in now, when a
    /// store has worked that out. Absent means the versions below decide.
    pub joined: BTreeMap<EntityId, Option<jiff::Timestamp>>,
    /// Per thing: the parent each write of its own row left, oldest first, for
    /// the things on the way from a member with taken-back numbers to the root.
    pub versions: BTreeMap<EntityId, Vec<ParentVersion>>,
    /// Per survivor: the things folded into it. A claim it serves from one of them
    /// was written while that thing was a member, so the survivor's own joining
    /// does not decide it.
    pub folded: BTreeMap<EntityId, Vec<EntityId>>,
}

/// **One write of a thing's own row seen as the parent it left and when it was
/// written**: `None` for a write that kept no snapshot, whose parent is not known.
pub type ParentVersion = Option<(Option<EntityId>, Option<jiff::Timestamp>)>;

/// **When a thing began to have the parent it has now**: the moment of the first
/// write of the unbroken run of writes that left that parent. `None` when that
/// moment is not known, which keeps every number (see [`issued_here`]).
pub fn parent_run_start(versions: &[ParentVersion]) -> Option<jiff::Timestamp> {
    let Some(Some((current, _))) = versions.last() else {
        return None;
    };
    let mut start = None;
    for version in versions.iter().rev() {
        match version {
            Some((parent, at)) if parent == current => start = Some(*at),
            Some(_) => break,
            None => return None,
        }
    }
    start.flatten()
}

/// **The parent a thing's newest write left**, which is how a thing a merge folded
/// away is followed up to the collection: the listing no longer holds its row.
pub fn last_parent(versions: &[ParentVersion]) -> Option<EntityId> {
    versions.last()?.as_ref()?.0.clone()
}

/// **When a member began to sit under the collection it sits under now**: the
/// latest of the moments every thing on the way up began to have its parent. A
/// move of a thing above it into the collection is its joining too.
pub fn joined_at(
    member: &EntityId,
    collection: &EntityId,
    parent_of: impl Fn(&EntityId) -> Option<EntityId>,
    versions_of: impl Fn(&EntityId) -> Vec<ParentVersion>,
) -> Option<jiff::Timestamp> {
    let mut at = member.clone();
    let mut since: Option<jiff::Timestamp> = None;
    let mut seen: BTreeSet<EntityId> = BTreeSet::new();
    while &at != collection {
        if !seen.insert(at.clone()) {
            return None;
        }
        let run = parent_run_start(&versions_of(&at))?;
        since = Some(since.map_or(run, |held| held.max(run)));
        at = parent_of(&at)?;
    }
    since
}

/// **The whole numbers that the claims of one key held, among claims that are no
/// longer current.** `writes` is oldest first, each as the claim that carried it,
/// what it put there, whether that claim is current and the moment of the write.
/// Only the newest write of a claim counts: an earlier wording the claim was
/// edited away from never held the claim's number.
pub fn retired_numbers<'a>(
    writes: impl IntoIterator<Item = (String, Option<&'a str>, bool, Option<jiff::Timestamp>)>,
) -> Vec<TakenBack> {
    let mut newest: BTreeMap<String, (Option<&'a str>, bool, Option<jiff::Timestamp>)> =
        BTreeMap::new();
    for (claim, value, current, at) in writes {
        newest.insert(claim, (value, current, at));
    }
    newest
        .into_values()
        .filter(|(_, current, _)| !current)
        .filter_map(|(value, _, at)| Some((whole_number(value?)?, at)))
        .collect()
}

/// **When a collection began numbering by the key it numbers by now**: the moment
/// of the first write of the unbroken run of writes that holds that key. `writes`
/// is every write of `numbers_by` on the thing, oldest first, as the value, whether
/// its claim is current and its moment. `None` when the run has no stamped start,
/// which reads as "unknown" and keeps every number.
pub fn founded_at<'a>(
    writes: impl IntoIterator<Item = (Option<&'a str>, bool, Option<jiff::Timestamp>)>,
) -> Option<jiff::Timestamp> {
    let mut run: Option<(String, Option<jiff::Timestamp>)> = None;
    for (value, current, at) in writes {
        if !current {
            continue;
        }
        match value.map(str::trim) {
            None | Some("") => run = None,
            Some(held) => {
                if run.as_ref().is_none_or(|(key, _)| key != held) {
                    run = Some((held.to_string(), at));
                }
            }
        }
    }
    run?.1
}

impl Numbered {
    /// The numbers held, those that are whole numbers.
    fn held(&self) -> impl Iterator<Item = u64> + '_ {
        self.members
            .iter()
            .filter_map(|m| m.value.as_deref().and_then(whole_number))
    }

    /// **The largest number issued, held or taken back, plus one.** There is no
    /// allocator: this is a read, and a race for it is caught by the guard. A
    /// number is below the largest a machine holds ([`whole_number`]), so the
    /// step cannot overflow.
    pub fn next(&self) -> u64 {
        self.held().chain(self.retired_all()).max().unwrap_or(0) + 1
    }

    /// **Every number the collection issued that a retraction took back**, from
    /// the members that are still in it.
    pub fn retired_all(&self) -> BTreeSet<u64> {
        self.members
            .iter()
            .flat_map(|m| m.retired.iter().copied())
            .collect()
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
    below(root, entities, false)
}

fn below<'a>(root: &EntityId, entities: &'a [Entity], folded_too: bool) -> Vec<&'a Entity> {
    let mut found: Vec<&Entity> = Vec::new();
    let mut seen: BTreeSet<&EntityId> = BTreeSet::from([root]);
    let mut frontier: Vec<&EntityId> = vec![root];
    while let Some(parent) = frontier.pop() {
        for entity in entities.iter().filter(|e| {
            e.parent.as_ref() == Some(parent) && (folded_too || e.merged_into.is_none())
        }) {
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
    retired: &Retired,
) -> Option<Numbered> {
    let key = fields.get(collection)?.get(NUMBERS_BY)?.trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some(numbered_by(collection, &key, entities, fields, retired))
}

/// **A thing read as a collection numbering by `key`**, whether or not it
/// carries `numbers_by` yet: every descendant with what it holds under the key.
/// A write that would make the thing a collection is judged on this.
pub fn numbered_by(
    collection: &EntityId,
    key: &str,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
    retired: &Retired,
) -> Numbered {
    let founded = retired.founded.get(collection).copied().flatten();
    Numbered {
        collection: collection.clone(),
        key: key.to_string(),
        members: descendants(collection, entities)
            .into_iter()
            .map(|e| {
                let joined = match retired.joined.get(&e.id) {
                    Some(held) => *held,
                    None => {
                        let parent_of = |node: &EntityId| {
                            entities
                                .iter()
                                .find(|x| &x.id == node)
                                .and_then(|x| x.parent.clone())
                                .or_else(|| last_parent(retired.versions.get(node)?))
                        };
                        let versions_of = |node: &EntityId| {
                            retired.versions.get(node).cloned().unwrap_or_default()
                        };
                        // **A claim the member serves from a thing folded into it was
                        // written while that thing was a member**: the earliest of
                        // their joinings stands, and one not known keeps every number.
                        let mut since = joined_at(&e.id, collection, parent_of, versions_of);
                        for folded in retired.folded.get(&e.id).into_iter().flatten() {
                            let other = joined_at(folded, collection, parent_of, versions_of);
                            since = since.zip(other).map(|(own, other)| own.min(other));
                        }
                        since
                    }
                };
                Member {
                    handle: e.id.clone(),
                    value: fields.get(&e.id).and_then(|f| f.get(key)).cloned(),
                    retired: issued_here(retired, &e.id, key, founded.max(joined)),
                }
            })
            .collect(),
    }
}

/// **The numbers a thing's taken-back claims held under `key` that the collection
/// issued**: those written when it already numbered by the key and the thing
/// already sat in it. `since` is the later of those two moments. A claim written
/// before it is left out; a claim with no stamp, or a `since` that is not known,
/// keeps the number.
pub fn issued_here(
    retired: &Retired,
    thing: &EntityId,
    key: &str,
    since: Option<jiff::Timestamp>,
) -> BTreeSet<u64> {
    retired
        .taken_back
        .get(thing)
        .and_then(|keys| keys.get(key))
        .into_iter()
        .flatten()
        .filter(|(_, at)| match (since, at) {
            (Some(start), Some(at)) => *at >= start,
            _ => true,
        })
        .map(|(number, _)| *number)
        .collect()
}

/// **What a write does to the key a thing numbers by.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyChange {
    /// The thing numbers by this key after the write, and did not before: it
    /// becomes a collection, or numbers by another key.
    By(String),
    /// The thing numbered before the write and numbers nothing after it.
    Off,
}

/// **The change a write makes to the key a thing numbers by, when it makes
/// one**: the `numbers_by` the thing holds after the write, against the one it
/// holds now. A write that sends the key it already has changes nothing, and a
/// write on a thing that never numbered and still does not changes nothing.
pub fn numbering_change(
    now: Option<&BTreeMap<String, String>>,
    after: &BTreeMap<String, String>,
) -> Option<KeyChange> {
    let key = |fields: &BTreeMap<String, String>| {
        fields
            .get(NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let held = now.and_then(key);
    let wanted = key(after);
    match (held, wanted) {
        (Some(held), Some(wanted)) if held == wanted => None,
        (_, Some(wanted)) => Some(KeyChange::By(wanted)),
        (Some(_), None) => Some(KeyChange::Off),
        (None, None) => None,
    }
}

/// **Every numbered collection a thing sits under**, nearest first: the
/// ancestors that carry `numbers_by`, each with its members. The thing itself
/// is not its own collection.
pub fn scopes_of(
    subject: &EntityId,
    entities: &[Entity],
    fields: &BTreeMap<EntityId, BTreeMap<String, String>>,
    retired: &Retired,
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
        if let Some(scope) = collection_of(&ancestor, entities, fields, retired) {
            scopes.push(scope);
        }
        at = entities
            .iter()
            .find(|e| e.id == ancestor)
            .and_then(|e| e.parent.clone());
    }
    scopes
}

/// **One change a write makes to the members of a numbered collection.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The thing holds `value` under the collection's key after the write. A
    /// thing that is not a member yet becomes one.
    Set {
        handle: EntityId,
        value: Option<String>,
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
}

impl Change {
    /// **Whether a thing the change leaves in place holds a `numbers_by`**, so a
    /// store that numbers nothing yet still has to ask.
    pub fn sets_numbering(&self) -> bool {
        self.founds()
            .any(|(_, fields)| fields.get(NUMBERS_BY).is_some_and(|v| !v.trim().is_empty()))
    }

    /// **The things whose own `numbers_by` this change judges.** A merge reads
    /// its survivor through the fold and is judged by its own path, so a change
    /// that releases a thing founds nothing.
    pub fn founds(&self) -> impl Iterator<Item = (&EntityId, &BTreeMap<String, String>)> {
        self.holds
            .iter()
            .filter(|_| self.leaves.is_empty())
            .map(|(handle, fields)| (handle, fields))
    }

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
    /// The things the write makes a collection, or numbers by another key.
    pub founded: Vec<Founding>,
}

/// **A thing the write makes a collection**, moves to another key or stops
/// numbering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Founding {
    /// The collection as it numbers now, when it numbers already.
    pub old: Option<Numbered>,
    /// The collection as the write would have it: the thing, the new key and
    /// what every descendant holds under that key. `None` when the write takes
    /// the key off.
    pub new: Option<Numbered>,
}

impl Founding {
    /// **Refuse a collection whose members already break its promise.** Asked
    /// in this order: a key change, or the key taken off, frees every number
    /// held under the old key; then the members are asked for one whole number
    /// each and no number twice. A member that holds nothing under the key is
    /// welcome. **Which member a refusal names does not depend on the order a
    /// store walks them in**: the lowest handle.
    fn refuses(&self) -> Option<MemoryError> {
        if let Some(old) = &self.old
            && let Some((member, number)) = old
                .members
                .iter()
                .filter_map(|m| Some((m, whole_number(m.value.as_deref()?)?)))
                .min_by(|a, b| a.0.handle.cmp(&b.0.handle))
        {
            return Some(MemoryError::NumberFreed {
                collection: old.collection.to_string(),
                key: old.key.clone(),
                number,
                holder: member.handle.to_string(),
                taken_back: false,
            });
        }
        let new = self.new.as_ref()?;
        let held = |m: &Member| m.value.as_deref().and_then(whole_number);
        let mut unfit: Vec<String> = Vec::new();
        for member in &new.members {
            let Some(raw) = member.value.as_deref() else {
                continue;
            };
            let twice = held(member).is_some_and(|number| {
                new.members
                    .iter()
                    .any(|other| other.handle != member.handle && held(other) == Some(number))
            });
            if held(member).is_none() || twice {
                unfit.push(format!("{} ({raw})", member.handle));
            }
        }
        if unfit.is_empty() {
            return None;
        }
        unfit.sort();
        Some(MemoryError::MembersBreakNumbering {
            collection: new.collection.to_string(),
            key: new.key.clone(),
            members: unfit,
        })
    }
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
                });
            }
        }
        for (handle, fields) in &change.moved {
            if joins {
                edits.push(Edit::Set {
                    handle: handle.clone(),
                    value: fields.get(&self.key).cloned(),
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
        for founding in &self.founded {
            if let Some(refusal) = founding.refuses() {
                return Some(refusal);
            }
        }
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
/// for ever: **a member keeps the number it holds**, so a member that left the
/// collection, was merged into a thing outside it, had the key taken off or was
/// given a different number frees a number, and `next` could hand it to a
/// different member. A write that sends the number the member already holds
/// changes nothing and lands. A member that holds no number may be given one.
/// **A retraction never asks this**: it says the record should never have been
/// written, so a number given by mistake can always be taken back. **What it
/// takes back is the claim and not the issue**: the collection remembers the
/// number as issued ([`Numbered::retired_all`]), and a later write that asks a member
/// to take it is refused. "Never reused" holds for a member that is retired,
/// which is archived and keeps its number, and for a number a retraction took
/// from a claim.
pub fn refuses_numbering(before: &Numbered, edits: &[Edit]) -> Option<MemoryError> {
    let mut after = before.members.clone();
    let mut touched: BTreeSet<&EntityId> = BTreeSet::new();
    for edit in edits {
        match edit {
            Edit::Leave(handle) => after.retain(|m| &m.handle != handle),
            Edit::Set { handle, value } => {
                touched.insert(handle);
                match after.iter_mut().find(|m| &m.handle == handle) {
                    Some(member) => member.value = value.clone(),
                    None => after.push(Member {
                        handle: handle.clone(),
                        value: value.clone(),
                        retired: BTreeSet::new(),
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
    // **A number issued and taken back is issued no more.** A member that takes
    // a number it did not hold, which a retraction took from a claim, is refused.
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
        let held_already = before
            .members
            .iter()
            .find(|m| &m.handle == *handle)
            .and_then(|m| m.value.as_deref())
            .and_then(whole_number)
            == Some(number);
        if before.retired_all().contains(&number) && !held_already {
            return Some(MemoryError::NumberRetired {
                collection: before.collection.to_string(),
                key: before.key.clone(),
                number,
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
        return Some(MemoryError::NumberFreed {
            collection: before.collection.to_string(),
            key: before.key.clone(),
            number,
            holder: member.handle.to_string(),
            taken_back: false,
        });
    }
    // **A member that leaves takes the record of the numbers its claims held out
    // of the collection**, so a number the collection issued could be issued
    // again. A merge into a member that stays serves that record, and so lets it
    // go; any other way out is refused, and archiving keeps the member.
    let kept = edits.iter().any(|edit| {
        matches!(edit, Edit::Set { handle, .. }
            if before.members.iter().any(|m| &m.handle == handle))
    });
    if !kept {
        let still = now.retired_all();
        for edit in edits {
            let Edit::Leave(handle) = edit else {
                continue;
            };
            let Some(member) = before.members.iter().find(|m| &m.handle == handle) else {
                continue;
            };
            if let Some(number) = member.retired.iter().find(|n| !still.contains(n)) {
                return Some(MemoryError::NumberFreed {
                    collection: before.collection.to_string(),
                    key: before.key.clone(),
                    number: *number,
                    holder: member.handle.to_string(),
                    taken_back: true,
                });
            }
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
    retired: &Retired,
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
        for scope in scopes_of(handle, entities, fields, retired) {
            add(scope, &mut reach);
        }
    }
    for (handle, after) in change.founds() {
        let new = match numbering_change(fields.get(handle), after) {
            Some(KeyChange::By(key)) => Some(numbered_by(handle, &key, entities, fields, retired)),
            Some(KeyChange::Off) => None,
            None => continue,
        };
        reach.founded.push(Founding {
            old: collection_of(handle, entities, fields, retired),
            new,
        });
    }
    if let Some(parent) = under {
        let mut above = scopes_of(parent, entities, fields, retired);
        if let Some(own) = collection_of(parent, entities, fields, retired) {
            above.insert(0, own);
        }
        for scope in above {
            reach.chain.insert(scope.collection.clone());
            add(scope, &mut reach);
        }
    }
    reach
}

/// **What one thing's claims held that are no longer current, by key, and when
/// it began numbering by its key**, from the writes a store keeps for it
/// ([`KeyWrite`], any key): see [`retired_numbers`] and [`founded_at`]. The thing's
/// entry in the result holds both when there is anything to say.
pub fn retired_of_writes(thing: &EntityId, writes: &[KeyWrite]) -> Retired {
    let mut by_key: BTreeMap<&str, Vec<&KeyWrite>> = BTreeMap::new();
    for write in writes {
        by_key.entry(write.key.as_str()).or_default().push(write);
    }
    let mut out = Retired::default();
    for (key, mut writes) in by_key {
        writes.sort_by_key(|w| w.ordinal);
        if key == NUMBERS_BY {
            let start = founded_at(writes.iter().map(|w| {
                (
                    w.value.as_deref(),
                    w.status == FactStatus::Active,
                    w.written_at,
                )
            }));
            out.founded.insert(thing.clone(), start);
        }
        let numbers = retired_numbers(writes.iter().map(|w| {
            (
                w.fact.0.clone(),
                w.value.as_deref(),
                w.status == FactStatus::Active,
                w.written_at,
            )
        }));
        if !numbers.is_empty() {
            out.taken_back
                .entry(thing.clone())
                .or_default()
                .insert(key.to_string(), numbers);
        }
    }
    out
}

/// **The numbers the claims of one member held under `key` that are no longer
/// current**, read from the key's every write ([`Memory::history`]) so the answer
/// does not depend on the member still holding a number.
pub async fn retired_in_history<M: Memory + ?Sized>(
    store: &M,
    member: &EntityId,
    key: &str,
) -> Result<Vec<TakenBack>, MemoryError> {
    let writes = store.history(member, key).await?;
    Ok(retired_numbers(writes.iter().map(|w| {
        (
            w.fact.to_string(),
            w.value.as_deref(),
            w.status == FactStatus::Active,
            w.written_at,
        )
    })))
}

/// **What a collection's members' taken-back claims held, and when it began
/// numbering**, read through the trait: the default read of a collection stands
/// on it.
pub async fn retired_read<M: Memory + ?Sized>(
    store: &M,
    collection: &EntityId,
    all: &[Entity],
    key: &str,
) -> Result<Retired, MemoryError> {
    let mut retired = Retired::default();
    retired.founded.insert(
        collection.clone(),
        founded_in_history(store, collection).await?,
    );
    for member in descendants(collection, all) {
        let numbers = retired_in_history(store, &member.id, key).await?;
        if !numbers.is_empty() {
            retired.taken_back.insert(
                member.id.clone(),
                BTreeMap::from([(key.to_string(), numbers)]),
            );
            versions_along(store, &member.id, collection, all, &mut retired).await;
        }
    }
    Ok(retired)
}

/// **The versions of every thing from `from` up to the collection**, each as the
/// parent it left and when, and the things folded into any of them (their
/// versions are part of the history of what they were folded into, marked by the
/// thing that wrote them). A store that keeps no history of a thing leaves it
/// empty, which reads as "not known".
async fn versions_along<M: Memory + ?Sized>(
    store: &M,
    from: &EntityId,
    collection: &EntityId,
    all: &[Entity],
    into: &mut Retired,
) {
    let mut starts = vec![from.clone()];
    let mut seen: BTreeSet<EntityId> = BTreeSet::new();
    while let Some(start) = starts.pop() {
        let mut at = start;
        while &at != collection && seen.insert(at.clone()) {
            let held = store.entity_history(&at).await.unwrap_or_default();
            let mut by_thing: BTreeMap<EntityId, Vec<ParentVersion>> = BTreeMap::new();
            for version in &held {
                by_thing.entry(version.thing.clone()).or_default().push(
                    version
                        .snapshot
                        .as_ref()
                        .map(|snapshot| (snapshot.parent.clone(), version.written_at)),
                );
            }
            into.versions
                .insert(at.clone(), by_thing.remove(&at).unwrap_or_default());
            for (folded, versions) in by_thing {
                starts.extend(last_parent(&versions));
                into.folded
                    .entry(at.clone())
                    .or_default()
                    .push(folded.clone());
                into.versions.insert(folded, versions);
            }
            let Some(parent) = all
                .iter()
                .find(|e| e.id == at)
                .and_then(|e| e.parent.clone())
                .or_else(|| last_parent(&into.versions[&at]))
            else {
                break;
            };
            at = parent;
        }
    }
}

/// **When the collection began numbering by the key it numbers by now**, read from
/// the key's every write ([`Memory::history`]): see [`founded_at`].
pub async fn founded_in_history<M: Memory + ?Sized>(
    store: &M,
    collection: &EntityId,
) -> Result<Option<jiff::Timestamp>, MemoryError> {
    let writes = store.history(collection, NUMBERS_BY).await?;
    Ok(founded_at(writes.iter().map(|w| {
        (
            w.value.as_deref(),
            w.status == FactStatus::Active,
            w.written_at,
        )
    })))
}

/// **What a collection numbering says of a write that would free a number.** A
/// member that holds `number` is kept in the collection by it; a member that only
/// carries a number a retraction took back is kept by the record of it.
pub fn freed_words(
    collection: &str,
    key: &str,
    number: &u64,
    holder: &str,
    taken_back: &bool,
) -> String {
    if *taken_back {
        format!(
            "'{collection}' numbers its members by '{key}', and this write would take {holder} \
             out, which carries {number}: a number the collection issued, whose claim a \
             retraction took back"
        )
    } else {
        format!(
            "'{collection}' numbers its members by '{key}', and this write would leave {holder} \
             without {number}"
        )
    }
}

/// **What a numbered collection adds to a read of it**, over the reads of the
/// trait: the default of [`Memory::numbering_of`].
pub async fn numbering_of<M: Memory + ?Sized>(
    store: &M,
    entity: &EntityId,
) -> Result<Option<Numbering>, MemoryError> {
    super::validate_subject(entity)?;
    let all = store.list_entities(None).await?;
    let former = store.former_handles().await?;
    let Some(resolved) = resolve_handle(entity, &all, &former) else {
        return Err(MemoryError::UnknownEntity {
            attempted: entity.to_string(),
            nearest: super::guard::screen(entity, &[], &all),
        });
    };
    Ok(store
        .numbered_scope(&resolved.id, &all)
        .await?
        .as_ref()
        .map(Numbering::from))
}

/// **The numbers a collection holds, as handles, for a miss on
/// `<collection>/<N>`**: one match for each number held, each with the member's
/// name. Empty when the handle is no numbered handle or its collection numbers
/// nothing, so the ordinary near-miss screen answers.
pub async fn numbers_held<M: Memory + ?Sized>(
    store: &M,
    asked: &EntityId,
) -> Result<Vec<EntityMatch>, MemoryError> {
    let Some((collection, number)) = numbered_handle(asked) else {
        return Ok(Vec::new());
    };
    let all = store.list_entities(None).await?;
    let former = store.former_handles().await?;
    let Some(resolved) = resolve_handle(&collection, &all, &former) else {
        return Ok(Vec::new());
    };
    let Some(scope) = store.numbered_scope(&resolved.id, &all).await? else {
        return Ok(Vec::new());
    };
    let mut fields: BTreeMap<EntityId, BTreeMap<String, String>> = BTreeMap::new();
    fields.insert(
        resolved.id.clone(),
        BTreeMap::from([(NUMBERS_BY.to_string(), scope.key.clone())]),
    );
    for member in &scope.members {
        if let Some(value) = &member.value {
            fields.insert(
                member.handle.clone(),
                BTreeMap::from([(scope.key.clone(), value.clone())]),
            );
        }
    }
    let asked = EntityId(format!("{}/{number}", resolved.id));
    Ok(nearest_numbers(&asked, &all, &fields).unwrap_or_default())
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
        let Some(scope) = collection_of(&collection.id, entities, fields, &Retired::default())
        else {
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
    let scope = collection_of(&collection, entities, fields, &Retired::default())?;
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
        assert_eq!(whole_number("18446744073709551614"), Some(u64::MAX - 1));
        for refused in [
            "",
            "0",
            "07",
            "-3",
            "+3",
            "2.5",
            "1e3",
            "seven",
            "1 2",
            "18446744073709551615",
            "18446744073709551616",
        ] {
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
