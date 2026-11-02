//! Request and app ledgers as events. This projection neither writes a store
//! nor re-runs admission. A request generation keeps a late receipt attached
//! to the attempt that produced it; app attempts have their own namespace.

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;

use crate::log::{ObjectId, RequestId};

/// A generation is an identity within a reusable request key, not a count of
/// historical calls. Import assigns one to the retained rows without inventing
/// generations that the old store has already removed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestKey {
    pub actor: ObjectId,
    pub id: RequestId,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub key: RequestKey,
    pub verb: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub request: Request,
    pub landed: String,
    pub receipt: Option<String>,
    pub written_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub request: Request,
    pub name: String,
    pub recorded: super::Recorded,
    pub written_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppKey {
    pub app_id: String,
    pub event_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestProjection {
    pub committed: Option<Commit>,
    pub parts: BTreeMap<String, Part>,
    pub expired: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    requests: HashMap<RequestKey, RequestProjection>,
    apps: HashMap<AppKey, AppAttempt>,
    history: Vec<LedgerEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a completed request names a part that has not committed")]
pub struct MissingPart;

impl State {
    /// Structural interpretation only. A failed completion publishes nothing;
    /// the original baseline event stays distinguishable in the history.
    pub fn apply(&mut self, event: &LedgerEvent) -> Result<(), MissingPart> {
        self.project(event)?;
        self.history.push(event.clone());
        Ok(())
    }

    fn project(&mut self, event: &LedgerEvent) -> Result<(), MissingPart> {
        match event {
            LedgerEvent::RequestPartCommitted(part) => {
                self.requests
                    .entry(part.request.key.clone())
                    .or_default()
                    .parts
                    .insert(part.name.clone(), part.clone());
            }
            LedgerEvent::RequestCommitted(commit) => {
                let held = self.requests.entry(commit.request.key.clone()).or_default();
                // A commit replayed after the fill must not take the receipt back.
                let filled = held.committed.as_ref().and_then(|old| old.receipt.clone());
                let mut commit = commit.clone();
                commit.receipt = commit.receipt.or(filled);
                held.committed = Some(commit);
            }
            LedgerEvent::RequestReceiptRecorded { key, receipt } => {
                if let Some(commit) = self
                    .requests
                    .get_mut(key)
                    .and_then(|held| held.committed.as_mut())
                {
                    commit.receipt = Some(receipt.clone());
                }
            }
            LedgerEvent::RequestCompleted {
                commit,
                required_parts,
            } => {
                if required_parts.iter().any(|part| {
                    !self
                        .requests
                        .get(&commit.request.key)
                        .is_some_and(|held| held.parts.contains_key(part))
                }) {
                    return Err(MissingPart);
                }
                self.project(&LedgerEvent::RequestCommitted(commit.clone()))?;
            }
            LedgerEvent::RequestExpired { key } => {
                if let Some(held) = self.requests.get_mut(key) {
                    held.expired = true;
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

    pub fn history(&self) -> &[LedgerEvent] {
        &self.history
    }

    pub fn request(&self, key: &RequestKey) -> Option<&RequestProjection> {
        self.requests.get(key)
    }

    /// Only the newest generation can answer a retry. An expired newest
    /// generation must not reveal an older attempt with the same public id.
    pub fn current(&self, actor: &ObjectId, id: &RequestId) -> Option<&RequestProjection> {
        self.requests
            .iter()
            .filter(|(key, _)| &key.actor == actor && &key.id == id)
            .max_by_key(|(key, _)| key.generation)
            .map(|(_, request)| request)
    }

    pub fn lookup(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Option<&Commit> {
        let held = self.current(actor, id)?;
        held.committed
            .as_ref()
            .filter(|commit| !held.expired && commit.expires_at >= at)
    }

    /// The old parts reader orders equal-time rows by part name.
    pub fn parts(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Vec<&Part> {
        let Some(held) = self.current(actor, id).filter(|held| !held.expired) else {
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
