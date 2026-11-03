//! **The chart around a write, read from the state.** The same [`Lineage`] the
//! verb builds from the store one bot at a time, built here from the folds the
//! state holds, so the licence of a guarded key is judged by the same domain
//! rule over the same facts.
//!
//! A manager is read as the handle the `reports_to` value spells. A bot's
//! writers are the bots that first wrote one of its records, as the state
//! records them; a run no bot is known for names nobody.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{ObjectId, Record, RunId, State};
use crate::actor::Actor;
use crate::memory::{
    self, ChainWalk, EdgeShape, EntityId, EntityKind, FactStatus, GUARDED_KEYS, Lineage,
    REPORTS_TO, capacity, review, thread_writers, types::DeclaredType,
};

pub(super) struct Chart<'a> {
    state: &'a State,
    declared: &'a [DeclaredType],
    runs: &'a BTreeMap<RunId, EntityId>,
}

impl<'a> Chart<'a> {
    pub(super) fn new(
        state: &'a State,
        declared: &'a [DeclaredType],
        runs: &'a BTreeMap<RunId, EntityId>,
    ) -> Self {
        Self {
            state,
            declared,
            runs,
        }
    }

    /// **The one handle an identity answers to**: whichever spelling it was
    /// reached by, a thing and the thing it was merged into give one answer. A
    /// spelling nothing holds is its own.
    pub(super) fn canonical(&self, spelling: &EntityId) -> EntityId {
        self.state
            .resolve(spelling.as_str())
            .and_then(|object| self.state.handle_of(object))
            .unwrap_or_else(|| spelling.clone())
    }

    /// **The bot that wrote a record first**: the bot its envelope names, else the
    /// bot its run belongs to. A bot merged into another is the survivor, and an
    /// archived bot writes nobody's thread.
    fn writer_of(&self, record: &Record) -> Option<EntityId> {
        let named = match &record.first_writer {
            Some(Actor::Bot(bot)) => Some(bot),
            _ => record.first_run.as_ref().and_then(|run| self.runs.get(run)),
        }?;
        let object = self.state.resolve(named.as_str())?;
        let body = self.state.object(object)?;
        (!body.archived).then(|| self.canonical(named))
    }

    pub(super) fn fold(&self, handle: &EntityId) -> BTreeMap<String, String> {
        self.state
            .resolve(handle.as_str())
            .map(|object| self.state.fields(object, self.declared))
            .unwrap_or_default()
    }

    /// The bots above `start` on its `reports_to` chain, nearest first.
    fn chain_above(&self, start: &EntityId) -> Vec<EntityId> {
        let mut walk = ChainWalk::from(start);
        while let Some(at) = walk.at().cloned() {
            let held = self.fold(&at);
            walk.step(held.get(REPORTS_TO).map(String::as_str));
        }
        walk.above()
    }

    /// Whether any other standing thing reports to `subject`.
    fn has_reports(&self, subject: &EntityId) -> bool {
        self.state.standing_objects().any(|object| {
            self.state.handle_of(object).as_ref() != Some(subject)
                && memory::reports_to(&self.state.fields(object, self.declared), subject)
        })
    }

    /// **The bots that head the chart**: each manager something reports to that
    /// has no manager of its own, archived things included.
    fn heads(&self) -> Vec<EntityId> {
        let managers: BTreeSet<EntityId> = self
            .state
            .standing_objects()
            .filter_map(|object| {
                self.state
                    .fields(object, self.declared)
                    .get(REPORTS_TO)
                    .and_then(|value| memory::manager_in(value))
            })
            .collect();
        managers
            .into_iter()
            .filter(|manager| self.chain_above(manager).is_empty())
            .collect()
    }

    /// **The bots that write into `subject`**: those that kept a thought in a
    /// thread (a claim on the bot, written by the bot, drawing a connection at
    /// it, whatever became of it), then those that first wrote a live fill of
    /// it. A fill is a live record that takes a slot and sets no guarded key,
    /// now or once.
    fn writers(&self, subject: &EntityId, home: &ObjectId) -> Vec<EntityId> {
        let mut thoughts = Vec::new();
        if subject.kind() == Some(EntityKind::THREAD) {
            for (_, record) in self.state.all_records() {
                let claim = &record.claim;
                let Some(bot) = self.state.handle_of(&claim.home) else {
                    continue;
                };
                let points_at_it = claim.edge.as_ref().is_some_and(|(shape, object)| {
                    shape == EdgeShape::Connection.as_token()
                        && self.state.survivor(object) == self.state.survivor(home)
                });
                let by_itself = self.writer_of(record) == Some(self.canonical(&bot));
                if points_at_it
                    && by_itself
                    && bot.kind() == Some(EntityKind::BOT)
                    && !thoughts.contains(&bot)
                {
                    thoughts.push(bot);
                }
            }
        }
        let mut fills = Vec::new();
        for record_id in self.state.active_records(home) {
            let Some(record) = self.state.record(record_id) else {
                continue;
            };
            if record.claim.status != FactStatus::Active {
                continue;
            }
            let streams = self.state.streams_written_by(record_id);
            let sets_a_guarded_key = streams.iter().any(|(object, key)| {
                self.state.survivor(object) == self.state.survivor(home)
                    && GUARDED_KEYS.iter().any(|rule| rule.key == key.0)
            });
            let takes_no_slot = streams
                .iter()
                .any(|(_, key)| key.0 == capacity::ENGINE_RECORD || key.0 == review::REVIEW_MARK);
            if sets_a_guarded_key || takes_no_slot {
                continue;
            }
            if let Some(writer) = self.writer_of(record)
                && !fills.contains(&writer)
            {
                fills.push(writer);
            }
        }
        thread_writers::thread_writers(&thoughts, &fills)
    }

    /// **The lineage of a write about `subject`** that names `named` as manager.
    pub(super) fn lineage(
        &self,
        subject: &EntityId,
        home: &ObjectId,
        named: Option<EntityId>,
    ) -> Lineage {
        let above = self.chain_above(subject);
        let above_named = named
            .as_ref()
            .map(|manager| self.chain_above(manager))
            .unwrap_or_default();
        let has_reports = above.is_empty() && named.is_some() && self.has_reports(subject);
        let holds = memory::held_guarded_keys(&self.fold(subject));
        let heads = if memory::heads_wanted(&holds) {
            self.heads()
        } else {
            Vec::new()
        };
        let writers = self.writers(subject, home);
        let above_writers = thread_writers::above_the_writers(
            writers.iter().map(|writer| self.chain_above(writer)),
        );
        Lineage {
            creating: false,
            above,
            named,
            above_named,
            has_reports,
            holds,
            heads,
            writers,
            above_writers,
        }
    }
}
