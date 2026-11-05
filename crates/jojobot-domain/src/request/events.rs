//! Request and app ledgers as events. This projection neither writes a store
//! nor re-runs admission. A request generation keeps a late receipt attached
//! to the attempt that produced it; app attempts have their own namespace.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::log::{ObjectId, RequestId};

/// A generation is an identity within a reusable request key, not a count of
/// historical calls. Import assigns one to the retained rows without inventing
/// generations that the old store has already removed.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct RequestKey {
    pub actor: ObjectId,
    pub id: RequestId,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub key: RequestKey,
    pub verb: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Commit {
    pub request: Request,
    pub landed: String,
    pub receipt: Option<String>,
    pub written_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Part {
    pub request: Request,
    pub name: String,
    pub recorded: super::Recorded,
    pub written_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct AppKey {
    pub app_id: String,
    pub event_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppAttempt {
    pub key: AppKey,
    pub digest: String,
    pub landed: String,
    pub written_at: Timestamp,
}

/// Complete decided values. `required_parts` is frozen by the completing
/// operation; replay checks that structure, never today's admission rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerEvent {
    RequestPartCommitted(Part),
    RequestCommitted(Commit),
    RequestReceiptRecorded {
        key: RequestKey,
        receipt: String,
    },
    RequestCompleted {
        commit: Commit,
        required_parts: Vec<String>,
    },
    RequestExpired {
        key: RequestKey,
    },
    AppAttemptCommitted(AppAttempt),
    /// A retained row is evidence of its current state, not an invented
    /// sequence of original commit, receipt-fill and completion events.
    Imported {
        row: LegacyRow,
        generation: u64,
    },
}

/// The complete columns of 0089, 0100 and 0104. Identity resolution precedes
/// this pure mapping. Source times remain source times, including expired rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyRow {
    RequestReceipt {
        bot_key: ObjectId,
        request_id: RequestId,
        verb: String,
        args_digest: String,
        landed: String,
        receipt: Option<String>,
        written_at: Timestamp,
    },
    RequestPart {
        bot_key: ObjectId,
        request_id: RequestId,
        part: String,
        verb: String,
        args_digest: String,
        landed: String,
        note: Option<String>,
        written_at: Timestamp,
    },
    AppWrite {
        app_id: String,
        event_key: String,
        digest: String,
        landed: String,
        written_at: Timestamp,
    },
}

impl LegacyRow {
    fn projection(&self, generation: u64) -> LedgerEvent {
        match self {
            Self::RequestReceipt {
                bot_key,
                request_id,
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
            } => LedgerEvent::RequestCommitted(Commit {
                request: Request {
                    key: RequestKey {
                        actor: bot_key.clone(),
                        id: request_id.clone(),
                        generation,
                    },
                    verb: verb.clone(),
                    digest: args_digest.clone(),
                },
                landed: landed.clone(),
                receipt: receipt.clone(),
                written_at: *written_at,
                expires_at: *written_at + super::KEPT_FOR,
            }),
            Self::RequestPart {
                bot_key,
                request_id,
                part,
                verb,
                args_digest,
                landed,
                note,
                written_at,
            } => LedgerEvent::RequestPartCommitted(Part {
                request: Request {
                    key: RequestKey {
                        actor: bot_key.clone(),
                        id: request_id.clone(),
                        generation,
                    },
                    verb: verb.clone(),
                    digest: args_digest.clone(),
                },
                name: part.clone(),
                recorded: super::Recorded {
                    landed: landed.clone(),
                    note: note.clone(),
                },
                written_at: *written_at,
                expires_at: *written_at + super::KEPT_FOR,
            }),
            Self::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            } => LedgerEvent::AppAttemptCommitted(AppAttempt {
                key: AppKey {
                    app_id: app_id.clone(),
                    event_key: event_key.clone(),
                },
                digest: digest.clone(),
                landed: landed.clone(),
                written_at: *written_at,
            }),
        }
    }

    pub fn into_event(self, generation: u64) -> LedgerEvent {
        LedgerEvent::Imported {
            row: self,
            generation,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RequestProjection {
    pub committed: Option<Commit>,
    pub parts: BTreeMap<String, Part>,
}

impl RequestProjection {
    /// The last instant any row of this generation is kept to. A generation with
    /// no row has none.
    fn horizon(&self) -> Option<Timestamp> {
        self.committed
            .iter()
            .map(|commit| commit.expires_at)
            .chain(self.parts.values().map(|part| part.expires_at))
            .max()
    }
}

/// The ledgers as projections. **The log is the history, so the state keeps no
/// copy of the events it applied.** A request is held under its `(actor, id)`,
/// with its generations beneath it, so a lookup is one keyed read however many
/// requests are held. **A generation whose rows have all expired is evicted**,
/// and the instant that decides it is one an event carries, never the clock at
/// apply time, so a replay holds the same state the live run held.
///
/// **A snapshot carries the eviction index with the requests**, so a state
/// restored from one goes on evicting what the live state would have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct State {
    requests: snapshot_map::SnapshotMap<(ObjectId, RequestId), BTreeMap<u64, RequestProjection>>,
    /// Each held generation under the instant its last row expires, so eviction
    /// reads the oldest and stops at the first that is still kept.
    horizons: BTreeSet<(Timestamp, ObjectId, RequestId, u64)>,
    apps: snapshot_map::SnapshotMap<AppKey, AppAttempt>,
}

mod snapshot_map;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a completed request names a part that has not committed")]
pub struct MissingPart;

impl LedgerEvent {
    /// The instant this event carries for eviction. A receipt and an expiry carry
    /// none.
    fn carried_instant(&self) -> Option<Timestamp> {
        match self {
            Self::RequestPartCommitted(part) => Some(part.written_at),
            Self::RequestCommitted(commit) | Self::RequestCompleted { commit, .. } => {
                Some(commit.written_at)
            }
            Self::AppAttemptCommitted(attempt) => Some(attempt.written_at),
            Self::Imported { row, .. } => Some(match row {
                LegacyRow::RequestReceipt { written_at, .. }
                | LegacyRow::RequestPart { written_at, .. }
                | LegacyRow::AppWrite { written_at, .. } => *written_at,
            }),
            Self::RequestReceiptRecorded { .. } | Self::RequestExpired { .. } => None,
        }
    }
}

impl State {
    /// Structural interpretation only. A failed completion changes nothing.
    pub fn apply(&mut self, event: &LedgerEvent) -> Result<(), MissingPart> {
        self.project(event)?;
        if let Some(instant) = event.carried_instant() {
            self.evict_before(instant);
        }
        Ok(())
    }

    /// Change one generation, keeping the horizon index in step with it.
    fn edit(&mut self, key: &RequestKey, change: impl FnOnce(&mut RequestProjection)) {
        let held = self
            .requests
            .entry((key.actor.clone(), key.id.clone()))
            .or_default()
            .entry(key.generation)
            .or_default();
        let before = held.horizon();
        change(held);
        let after = held.horizon();
        if before != after {
            if let Some(at) = before {
                self.horizons
                    .remove(&(at, key.actor.clone(), key.id.clone(), key.generation));
            }
            if let Some(at) = after {
                self.horizons
                    .insert((at, key.actor.clone(), key.id.clone(), key.generation));
            }
        }
    }

    fn remove_generation(&mut self, actor: &ObjectId, id: &RequestId, generation: u64) {
        let slot = (actor.clone(), id.clone());
        let Some(generations) = self.requests.get_mut(&slot) else {
            return;
        };
        if let Some(gone) = generations.remove(&generation)
            && let Some(at) = gone.horizon()
        {
            self.horizons
                .remove(&(at, actor.clone(), id.clone(), generation));
        }
        if generations.is_empty() {
            self.requests.remove(&slot);
        }
    }

    /// Drop every generation whose last row expired before `instant`.
    fn evict_before(&mut self, instant: Timestamp) {
        while let Some((at, actor, id, generation)) = self.horizons.first().cloned() {
            if at >= instant {
                break;
            }
            self.remove_generation(&actor, &id, generation);
        }
    }

    fn project(&mut self, event: &LedgerEvent) -> Result<(), MissingPart> {
        match event {
            LedgerEvent::RequestPartCommitted(part) => {
                self.edit(&part.request.key, |held| {
                    held.parts.insert(part.name.clone(), part.clone());
                });
            }
            LedgerEvent::RequestCommitted(commit) => {
                self.edit(&commit.request.key, |held| {
                    // A commit replayed after the fill must not take the receipt back.
                    let filled = held.committed.as_ref().and_then(|old| old.receipt.clone());
                    let mut commit = commit.clone();
                    commit.receipt = commit.receipt.or(filled);
                    held.committed = Some(commit);
                });
            }
            LedgerEvent::RequestReceiptRecorded { key, receipt } => {
                // A receipt addresses a generation that is held, or nothing.
                let held = self
                    .requests
                    .get(&(key.actor.clone(), key.id.clone()))
                    .is_some_and(|generations| {
                        generations
                            .get(&key.generation)
                            .is_some_and(|held| held.committed.is_some())
                    });
                if held {
                    self.edit(key, |held| {
                        if let Some(commit) = held.committed.as_mut() {
                            commit.receipt = Some(receipt.clone());
                        }
                    });
                }
            }
            LedgerEvent::RequestCompleted {
                commit,
                required_parts,
            } => {
                if required_parts.iter().any(|part| {
                    !self
                        .request(&commit.request.key)
                        .is_some_and(|held| held.parts.contains_key(part))
                }) {
                    return Err(MissingPart);
                }
                self.project(&LedgerEvent::RequestCommitted(commit.clone()))?;
            }
            LedgerEvent::RequestExpired { key } => {
                // **Expiring a generation hides the request**, and the older
                // generations of the id go with it: none of them may answer a
                // retry in its place.
                let older: Vec<u64> = self
                    .requests
                    .get(&(key.actor.clone(), key.id.clone()))
                    .map(|generations| {
                        generations
                            .range(..=key.generation)
                            .map(|(g, _)| *g)
                            .collect()
                    })
                    .unwrap_or_default();
                for generation in older {
                    self.remove_generation(&key.actor, &key.id, generation);
                }
            }
            LedgerEvent::AppAttemptCommitted(attempt) => {
                self.apps.insert(attempt.key.clone(), attempt.clone());
            }
            LedgerEvent::Imported { row, generation } => {
                self.project(&row.projection(*generation))?
            }
        }
        Ok(())
    }

    pub fn replay<'a>(
        events: impl IntoIterator<Item = &'a LedgerEvent>,
    ) -> Result<Self, MissingPart> {
        let mut state = Self::default();
        for event in events {
            state.apply(event)?;
        }
        Ok(state)
    }

    pub fn request(&self, key: &RequestKey) -> Option<&RequestProjection> {
        self.requests
            .get(&(key.actor.clone(), key.id.clone()))?
            .get(&key.generation)
    }

    /// Only the newest generation can answer a retry.
    pub fn current(&self, actor: &ObjectId, id: &RequestId) -> Option<&RequestProjection> {
        self.requests
            .get(&(actor.clone(), id.clone()))?
            .last_key_value()
            .map(|(_, request)| request)
    }

    /// **The generation a write under this id belongs to.** The newest
    /// generation while one of its rows is still held at `at`; the next number
    /// once every row of it has aged out; 1 for an id with no generation held. A
    /// reused id so starts a generation of its own, and a late receipt of the
    /// old attempt cannot attach to it.
    pub fn generation_for(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> u64 {
        let Some((generation, held)) = self
            .requests
            .get(&(actor.clone(), id.clone()))
            .and_then(|generations| generations.last_key_value())
        else {
            return 1;
        };
        if held.horizon().is_some_and(|horizon| horizon >= at) {
            *generation
        } else {
            generation + 1
        }
    }

    pub fn lookup(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Option<&Commit> {
        self.current(actor, id)?
            .committed
            .as_ref()
            .filter(|commit| commit.expires_at >= at)
    }

    /// The old parts reader orders equal-time rows by part name.
    pub fn parts(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Vec<&Part> {
        let Some(held) = self.current(actor, id) else {
            return Vec::new();
        };
        let mut parts: Vec<_> = held
            .parts
            .values()
            .filter(|part| part.expires_at >= at)
            .collect();
        parts.sort_by(|a, b| (a.written_at, &a.name).cmp(&(b.written_at, &b.name)));
        parts
    }

    pub fn app(&self, key: &AppKey) -> Option<&AppAttempt> {
        self.apps.get(key)
    }
}

#[cfg(test)]
mod tests;
