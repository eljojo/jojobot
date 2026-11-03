//! **What a write to claims needs to know before it is made**, answered from the
//! state alone. Each lookup is a map read: [`State::apply`] keeps the maps, so no
//! lookup scans and nothing here reads a store.
//!
//! - [`State::next_ordinal`]: the place a new write of a key takes in the stream
//!   of an object.
//! - [`State::highest_claim_number`]: the number the next claim of a home counts
//!   up from. This module supplies the number; the rule that uses it is the
//!   admission's.
//! - [`State::highest_merge_number`]: the number of merges applied.
//! - [`State::numbered_member`]: the member a container's number belongs to.
//! - [`State::number_issued`]: whether a claim ever held a number, taken back
//!   or not.
//! - [`State::claim_at`]: the claim an address names.
//!
//! **Each index equals a recomputation from the final state.** The tests hold
//! that against a generated run of events.

use std::collections::{BTreeMap, BTreeSet};

use super::{ClaimBody, FieldValue, KeyId, MovedRecord, ObjectId, RecordId, State};
use crate::memory::{
    self,
    numbered::{NUMBERS_BY, whole_number},
};

/// The claim an address names, and the object it is homed on now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub record: RecordId,
    pub home: ObjectId,
}

/// A numbered member: the container, the key it numbers by, and the number.
type Seat = (ObjectId, String, u64);

/// **The maps [`State::apply`] keeps.** Every key is an object that is not
/// folded into another: a merge moves what a folded object held to its survivor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Indexes {
    /// The highest ordinal written in each stream.
    top_ordinal: BTreeMap<(ObjectId, KeyId), u64>,
    /// Per home, how many claims hold each claim number. A count, so a claim
    /// that leaves a home gives its number back.
    claim_numbers: BTreeMap<ObjectId, BTreeMap<u64, u32>>,
    merges: u64,
    /// (home, label) to the claim that holds it.
    labels: BTreeMap<(ObjectId, String), RecordId>,
    /// The objects whose parent is the key.
    children: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    /// The streams each claim wrote to: a revision changes what they fold to.
    written_by: BTreeMap<RecordId, BTreeSet<(ObjectId, KeyId)>>,
    /// Each seat to the members that hold it; two only if the writes disagree.
    seats: BTreeMap<Seat, BTreeSet<ObjectId>>,
    /// The seats each member holds, to take them back when the member changes.
    held_seats: BTreeMap<ObjectId, BTreeSet<Seat>>,
    /// Every seat a claim ever held, whatever became of the claim. Nothing is
    /// removed: a number issued once is never issued again.
    issued: BTreeSet<Seat>,
}

impl Indexes {
    /// A merge moved a stream: `highest` is the top ordinal it has now.
    pub(super) fn moved_stream(
        &mut self,
        folded: &(ObjectId, KeyId),
        survivor: &(ObjectId, KeyId),
        highest: Option<u64>,
    ) {
        self.top_ordinal.remove(folded);
        match highest {
            Some(top) => self.top_ordinal.insert(survivor.clone(), top),
            None => self.top_ordinal.remove(survivor),
        };
    }
}

/// The number a label counts as: `f12` is 12. Any other label counts as none.
pub(super) fn claim_number(label: &str) -> Option<u64> {
    whole_number(label.strip_prefix('f')?)
}

impl State {
    /// **The objects that stand**: none folded into another.
    pub(super) fn standing_objects(&self) -> impl Iterator<Item = &ObjectId> {
        self.objects
            .keys()
            .filter(|object| !self.redirects.contains_key(*object))
    }

    /// Every claim the state holds, with its identity.
    pub(super) fn all_records(&self) -> impl Iterator<Item = (&RecordId, &super::Record)> {
        self.records.iter()
    }

    /// The streams a claim wrote to.
    pub(super) fn streams_written_by(&self, record: &RecordId) -> Vec<(ObjectId, KeyId)> {
        self.indexes
            .written_by
            .get(record)
            .map(|streams| streams.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// **The handle an object answers to now**: a spelling of the form
    /// `kind:slug` bound to it.
    pub(super) fn handle_of(&self, object: &ObjectId) -> Option<crate::memory::EntityId> {
        let object = self.survivor(object);
        self.addresses
            .iter()
            .find(|(address, bound)| {
                self.survivor(bound) == object && address.contains(':') && !address.contains('#')
            })
            .map(|(address, _)| crate::memory::EntityId(address.clone()))
    }

    /// **The ordinal a new write of `key` on `object` takes**: one past the
    /// highest in its stream, and one for a stream nothing was written to. The
    /// object is followed to its merge survivor.
    pub fn next_ordinal(&self, object: &ObjectId, key: &KeyId) -> u64 {
        self.indexes
            .top_ordinal
            .get(&(self.survivor(object).clone(), key.clone()))
            .map_or(1, |top| top.saturating_add(1))
    }

    /// **The highest claim number any claim homed on `home` holds**, archived and
    /// taken-back claims included; zero for a home with none. A label `fN` holds
    /// N; any other label holds none.
    pub fn highest_claim_number(&self, home: &ObjectId) -> u64 {
        self.indexes
            .claim_numbers
            .get(self.survivor(home))
            .and_then(|held| held.keys().next_back().copied())
            .unwrap_or(0)
    }

    /// **The number of the latest merge**: merges are numbered from one in the
    /// order they were applied, so this is how many have been.
    pub fn highest_merge_number(&self) -> u64 {
        self.indexes.merges
    }

    /// **The member of `container` that holds `number` under `key`.** A container
    /// numbers its members by the key its `numbers_by` field holds; its members
    /// are every object under it by parent, however deep, that holds a whole
    /// number under that key. An archived member keeps its number.
    pub fn numbered_member(
        &self,
        container: &ObjectId,
        key: &str,
        number: u64,
    ) -> Option<&ObjectId> {
        self.indexes
            .seats
            .get(&(self.survivor(container).clone(), key.to_string(), number))?
            .iter()
            .next()
    }

    /// **Whether a claim ever held `number` under `key` in `container`**, one
    /// that stands, one taken back, or one on a member since archived. A
    /// claim counts by its newest write of the key: an earlier wording it was
    /// edited away from never held the number.
    pub fn number_issued(&self, container: &ObjectId, key: &str, number: u64) -> bool {
        self.indexes
            .issued
            .contains(&(self.survivor(container).clone(), key.to_string(), number))
    }

    /// **The containers above `member` that number their members**, nearest
    /// first, each with the key it numbers by.
    pub(super) fn numbering_scopes(&self, member: &ObjectId) -> Vec<(ObjectId, String)> {
        self.ancestors(member)
            .into_iter()
            .filter_map(|container| {
                let key = self.numbers_by(&container)?;
                Some((container, key))
            })
            .collect()
    }

    /// **The claim an address names**: a spelling of the object, a `#`, and the
    /// label the claim is served under. The spelling may be one the object
    /// answers to now or one it gave up, and a merged object's follows it to its
    /// survivor.
    pub fn claim_at(&self, address: &str) -> Option<Located> {
        let (spelling, label) = address.rsplit_once('#')?;
        let object = match self.resolve(spelling) {
            Some(object) => object,
            None => self.survivor(self.retired_addresses.get(spelling)?),
        };
        let record = self
            .indexes
            .labels
            .get(&(object.clone(), label.to_string()))?;
        let home = self.survivor(&self.records.get(record)?.claim.home).clone();
        Some(Located {
            record: record.clone(),
            home,
        })
    }

    // The hooks `apply` calls. Each keeps the maps above with the change.

    pub(super) fn index_object(
        &mut self,
        object: &ObjectId,
        before: Option<Option<ObjectId>>,
        parent: &Option<ObjectId>,
    ) {
        if before.as_ref() == Some(parent) {
            return;
        }
        if let Some(Some(old)) = &before {
            let key = self.survivor(old).clone();
            if let Some(held) = self.indexes.children.get_mut(&key) {
                held.remove(object);
            }
        }
        if let Some(new) = parent {
            let key = self.survivor(new).clone();
            self.indexes
                .children
                .entry(key)
                .or_default()
                .insert(object.clone());
        }
        self.refresh_subtree(object);
    }

    pub(super) fn index_record(
        &mut self,
        record: &RecordId,
        before: Option<&ClaimBody>,
        claim: &ClaimBody,
    ) {
        if let Some(before) = before {
            self.unindex_claim(record, before);
        }
        self.index_claim(record, claim);
        // The claim's status or standing decides what its writes fold to.
        let wrote: Vec<(ObjectId, KeyId)> = self
            .indexes
            .written_by
            .get(record)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        for (subject, key) in wrote {
            self.refresh_written(&subject, &key);
        }
    }

    fn index_claim(&mut self, record: &RecordId, claim: &ClaimBody) {
        let home = self.survivor(&claim.home).clone();
        if let Some(number) = claim_number(&claim.label) {
            *self
                .indexes
                .claim_numbers
                .entry(home.clone())
                .or_default()
                .entry(number)
                .or_insert(0) += 1;
        }
        self.indexes
            .labels
            .insert((home, claim.label.clone()), record.clone());
    }

    fn unindex_claim(&mut self, record: &RecordId, claim: &ClaimBody) {
        let home = self.survivor(&claim.home).clone();
        if let Some(number) = claim_number(&claim.label)
            && let Some(held) = self.indexes.claim_numbers.get_mut(&home)
            && let Some(count) = held.get_mut(&number)
        {
            *count -= 1;
            if *count == 0 {
                held.remove(&number);
            }
        }
        let at = (home, claim.label.clone());
        if self.indexes.labels.get(&at) == Some(record) {
            self.indexes.labels.remove(&at);
        }
    }

    pub(super) fn index_write(
        &mut self,
        record: &RecordId,
        subject: &ObjectId,
        key: &KeyId,
        ordinal: u64,
    ) {
        let stream = (subject.clone(), key.clone());
        let top = self.indexes.top_ordinal.entry(stream.clone()).or_default();
        *top = (*top).max(ordinal);
        self.indexes
            .written_by
            .entry(record.clone())
            .or_default()
            .insert(stream);
        self.refresh_written(subject, key);
    }

    /// Before a merge changes anything: the claims it moves give up the number
    /// and label they hold under the home they are on.
    pub(super) fn index_before_merge(&mut self, records: &[MovedRecord]) {
        for moved in records {
            if let Some(held) = self.records.get(&moved.record) {
                let claim = held.claim.clone();
                self.unindex_claim(&moved.record, &claim);
            }
        }
    }

    /// After a merge changed the state: what the folded object held moves to its
    /// survivor, the moved claims take the labels they are served under, and
    /// every seat that can have changed is read again.
    pub(super) fn index_after_merge(
        &mut self,
        folded: &ObjectId,
        survivor: &ObjectId,
        records: &[MovedRecord],
    ) {
        if let Some(numbers) = self.indexes.claim_numbers.remove(folded) {
            let into = self
                .indexes
                .claim_numbers
                .entry(survivor.clone())
                .or_default();
            for (number, count) in numbers {
                *into.entry(number).or_insert(0) += count;
            }
        }
        let labels: Vec<(ObjectId, String)> = self
            .indexes
            .labels
            .range((folded.clone(), String::new())..)
            .map(|(at, _)| at.clone())
            .take_while(|(home, _)| home == folded)
            .collect();
        for at in labels {
            if let Some(record) = self.indexes.labels.remove(&at) {
                self.indexes.labels.insert((survivor.clone(), at.1), record);
            }
        }
        for moved in records {
            if let Some(held) = self.records.get(&moved.record) {
                let claim = held.claim.clone();
                self.index_claim(&moved.record, &claim);
            }
        }
        if let Some(children) = self.indexes.children.remove(folded) {
            self.indexes
                .children
                .entry(survivor.clone())
                .or_default()
                .extend(children);
        }
        self.indexes.merges += 1;
        self.release_seats(folded);
        self.refresh_subtree(survivor);
    }

    // Seats: who holds which number under which container.

    /// What the fold gives one key of an object. The declared types are not in
    /// this state, so a key counts as an ordinary one: the newest write wins.
    fn held_value(&self, subject: &ObjectId, key: &str) -> Option<String> {
        let writes = self.key_writes_of(subject, &KeyId(key.to_string()));
        memory::folded_fields(&writes, &[]).remove(key)
    }

    /// The key an object numbers its members by, if it numbers them.
    pub(super) fn numbers_by(&self, object: &ObjectId) -> Option<String> {
        let key = self.held_value(object, NUMBERS_BY)?;
        let key = key.trim();
        (!key.is_empty()).then(|| key.to_string())
    }

    /// **Every object under `root` by parent**, archived ones included, none
    /// folded into another, the root left out.
    pub(super) fn members_below(&self, root: &ObjectId) -> Vec<ObjectId> {
        self.subtree(root)
            .into_iter()
            .skip(1)
            .filter(|object| !self.redirects.contains_key(object))
            .collect()
    }

    /// **The numbers a claim once held under `key` in `container` that nobody
    /// holds now**: issued, and taken back.
    pub(super) fn issued_unheld(&self, container: &ObjectId, key: &str) -> BTreeSet<u64> {
        let container = self.survivor(container);
        self.indexes
            .issued
            .iter()
            .filter(|(held_in, held_key, number)| {
                held_in == container
                    && held_key == key
                    && self
                        .indexes
                        .seats
                        .get(&(held_in.clone(), held_key.clone(), *number))
                        .is_none_or(|members| members.is_empty())
            })
            .map(|(_, _, number)| *number)
            .collect()
    }

    /// The objects above one by parent, nearest first, merged objects followed
    /// to their survivors. A parent that loops back ends the walk.
    fn ancestors(&self, object: &ObjectId) -> Vec<ObjectId> {
        let start = self.survivor(object).clone();
        let mut out: Vec<ObjectId> = Vec::new();
        let mut at = start.clone();
        while let Some(parent) = self.objects.get(&at).and_then(|body| body.parent.as_ref()) {
            let parent = self.survivor(parent).clone();
            if parent == start || out.contains(&parent) {
                break;
            }
            out.push(parent.clone());
            at = parent;
        }
        out
    }

    /// The object and everything under it by parent.
    fn subtree(&self, root: &ObjectId) -> Vec<ObjectId> {
        let root = self.survivor(root).clone();
        let mut seen = BTreeSet::from([root.clone()]);
        let mut order = vec![root];
        let mut next = 0;
        while let Some(node) = order.get(next).cloned() {
            next += 1;
            for child in self.indexes.children.get(&node).into_iter().flatten() {
                // A folded object's children moved to its survivor.
                if self.survivor(child) == child && seen.insert(child.clone()) {
                    order.push(child.clone());
                }
            }
        }
        order
    }

    /// A write of `key` on `subject` changed what the subject holds.
    fn refresh_written(&mut self, subject: &ObjectId, key: &KeyId) {
        if key.0 == NUMBERS_BY {
            self.refresh_subtree(subject);
        } else {
            self.refresh_member(subject);
        }
    }

    fn refresh_subtree(&mut self, root: &ObjectId) {
        for member in self.subtree(root) {
            self.refresh_member(&member);
        }
    }

    /// Read again every seat one object holds, under every container above it.
    fn refresh_member(&mut self, member: &ObjectId) {
        let member = self.survivor(member).clone();
        self.release_seats(&member);
        let mut held: BTreeSet<Seat> = BTreeSet::new();
        for container in self.ancestors(&member) {
            let Some(key) = self.numbers_by(&container) else {
                continue;
            };
            if let Some(number) = self
                .held_value(&member, &key)
                .as_deref()
                .and_then(whole_number)
            {
                held.insert((container, key, number));
            }
        }
        for container in self.ancestors(&member) {
            let Some(key) = self.numbers_by(&container) else {
                continue;
            };
            let mut newest: BTreeMap<&RecordId, &FieldValue> = BTreeMap::new();
            for write in self
                .streams
                .get(&(member.clone(), KeyId(key.clone())))
                .into_iter()
                .flatten()
                .filter(|write| memory::reaches_the_thing(write.class))
            {
                newest.insert(&write.record, &write.write);
            }
            let issued: Vec<Seat> = newest
                .into_values()
                .filter_map(|value| match value {
                    FieldValue::Set(value) => whole_number(value),
                    FieldValue::Clear => None,
                })
                .map(|number| (container.clone(), key.clone(), number))
                .collect();
            self.indexes.issued.extend(issued);
        }
        for seat in &held {
            self.indexes
                .seats
                .entry(seat.clone())
                .or_default()
                .insert(member.clone());
        }
        if !held.is_empty() {
            self.indexes.held_seats.insert(member, held);
        }
    }

    fn release_seats(&mut self, member: &ObjectId) {
        for seat in self.indexes.held_seats.remove(member).into_iter().flatten() {
            if let Some(members) = self.indexes.seats.get_mut(&seat) {
                members.remove(member);
                if members.is_empty() {
                    self.indexes.seats.remove(&seat);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
