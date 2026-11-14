//! **The entries of history a thing holds, and the same entries rebuilt from the log.**
//! Every entry the State keeps of a write names the position of the event that made it
//! (rule 802). This module gathers a thing's entries, and rebuilds entries from events
//! fetched by those positions with the interpreter the State itself runs, so an entry
//! rebuilt from the log is the one the State holds and there is no second reading of an
//! event.

use jiff::Timestamp;

use super::{
    ClaimEvent, Event, FieldValue, KeyId, ObjectId, ObjectVersion, ObservedWrite, Position,
    RecordId, Revision, RunId, State, Write,
};
use crate::actor::Actor;
use crate::memory::WriteClass;

/// **One write of a key**, with the thing and the key it was written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyWriteEntry {
    pub subject: ObjectId,
    pub key: KeyId,
    pub record: RecordId,
    pub ordinal: u64,
    pub class: WriteClass,
    pub write: FieldValue,
    pub targets: Vec<ObjectId>,
    pub writer: Option<Actor>,
    pub run: Option<RunId>,
    pub written_at: Option<Timestamp>,
    pub position: Position,
}

impl KeyWriteEntry {
    fn of(subject: &ObjectId, key: &KeyId, write: &Write) -> Self {
        Self {
            subject: subject.clone(),
            key: key.clone(),
            record: write.record.clone(),
            ordinal: write.ordinal,
            class: write.class,
            write: write.write.clone(),
            targets: write.targets.clone(),
            writer: write.writer.clone(),
            run: write.run.clone(),
            written_at: write.written_at,
            position: write.position,
        }
    }
}

/// **Every entry of history that concerns a set of things**, each list in the order of
/// the log. A version's `seq` is the position: the State counts versions as it applies
/// them, and a State built from some of the events counts differently, but the order of
/// the log is the same.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryEntries {
    pub versions: Vec<ObjectVersion>,
    pub observed: Vec<(ObjectId, ObservedWrite)>,
    pub revisions: Vec<(RecordId, Revision)>,
    pub writes: Vec<KeyWriteEntry>,
    /// **The merges that changed these entries**: each thing folded into another, with the
    /// position of the event that folded it. A merge renumbers a key's writes and moves a
    /// folded thing's writes under its survivor, so the entries are rebuilt with them.
    pub merges: Vec<(ObjectId, Position)>,
}

impl HistoryEntries {
    /// **The log position of every entry**, in order, once each.
    pub fn positions(&self) -> Vec<Position> {
        let mut found: Vec<Position> = self
            .versions
            .iter()
            .map(|version| version.position)
            .chain(self.observed.iter().map(|(_, write)| write.position))
            .chain(self.revisions.iter().map(|(_, revision)| revision.position))
            .chain(self.writes.iter().map(|write| write.position))
            .chain(self.merges.iter().map(|(_, position)| *position))
            .collect();
        found.sort();
        found.dedup();
        found
    }

    fn ordered(mut self) -> Self {
        self.versions.sort_by_key(|version| version.position);
        self.observed.sort_by_key(|(_, write)| write.position);
        self.revisions
            .sort_by_key(|(_, revision)| revision.position);
        self.writes.sort_by_key(|write| write.position);
        self.merges.sort_by_key(|(_, position)| *position);
        self
    }
}

impl State {
    /// **The entries of history a thing holds, and of everything folded into it.**
    pub fn history_entries_of(&self, subject: &ObjectId) -> HistoryEntries {
        let root = self.survivor(subject);
        let objects: Vec<&ObjectId> = std::iter::once(root)
            .chain(self.folded_under.get(root).into_iter().flatten())
            .collect();
        self.collect_entries(|object| objects.contains(&object))
    }

    /// **The entries of history that events fetched from the log make**, each at the
    /// position it was fetched by. The events are applied to a State of their own by the
    /// one interpreter, and everything that State holds is the answer.
    pub fn history_entries_from<'a>(
        events: impl IntoIterator<Item = (Position, &'a Event<ClaimEvent>)>,
    ) -> HistoryEntries {
        let mut events: Vec<_> = events.into_iter().collect();
        events.sort_by_key(|(position, _)| *position);
        let mut scratch = State::default();
        for (position, event) in events {
            scratch.apply_at(event, position);
        }
        scratch.collect_entries(|_| true)
    }

    fn collect_entries(&self, accept: impl Fn(&ObjectId) -> bool) -> HistoryEntries {
        let mut entries = HistoryEntries::default();
        for (object, versions) in &self.versions {
            if accept(object) {
                entries
                    .versions
                    .extend(versions.iter().cloned().map(|mut version| {
                        version.seq = version.position.0;
                        version
                    }));
            }
        }
        for (object, writes) in &self.observed {
            if accept(object) {
                entries
                    .observed
                    .extend(writes.iter().map(|write| (object.clone(), write.clone())));
            }
        }
        for (record, held) in &self.records {
            if accept(&held.claim.home) {
                entries.revisions.extend(
                    held.history
                        .iter()
                        .map(|revision| (record.clone(), revision.clone())),
                );
            }
        }
        for ((subject, key), stream) in &self.streams {
            if accept(subject) {
                entries.writes.extend(
                    stream
                        .iter()
                        .map(|write| KeyWriteEntry::of(subject, key, write)),
                );
            }
        }
        for (folded, position) in &self.merge_positions {
            if accept(folded) {
                entries.merges.push((folded.clone(), *position));
            }
        }
        entries.ordered()
    }
}
