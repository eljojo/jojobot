//! **The state of the work a list entry points at, read beside the entry.**
//!
//! Position is not readiness: a list never skips a blocked entry or promotes a
//! ready one, and ticking an entry says the list item is done and not that the
//! work is. So the entry shows the work's own state next to it, read from the
//! work each time and stored nowhere. The verdict is [`ready::verdicts_of`], the
//! one question the ready view asks, so the two cannot part.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::memory::ready;
use jojobot_domain::memory::readiness::Blocker;

/// **One piece of work as a linked entry shows it.**
pub(crate) struct Linked {
    handle: EntityId,
    status: Option<String>,
    owner: Option<String>,
    verdict: ready::Verdict,
    /// Whether its status means done, archived or not. The verdict never says
    /// finished for archived work, so this is read from the status itself.
    status_finished: bool,
}

impl Linked {
    /// **Whether the work is still to be done**: neither finished nor archived.
    pub(crate) fn is_open(&self) -> bool {
        !self
            .verdict
            .blockers
            .iter()
            .any(|blocker| matches!(blocker, Blocker::Finished | Blocker::Archived))
    }

    /// **Whether the work left the bundle**: archived and never finished. An
    /// archived piece of work whose status means done still counts as done; one
    /// that did not finish is no longer part of what the list is about, as a
    /// dropped entry is not.
    pub(crate) fn left_the_bundle(&self) -> bool {
        !self.status_finished
            && self
                .verdict
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, Blocker::Archived))
    }

    /// **Whether the work is finished**: its status means done, by the shipped
    /// word or a word the instance maps to it, archived or not.
    pub(crate) fn is_finished(&self) -> bool {
        self.status_finished
    }

    /// **The block a list read carries on the entry**: the work, its status and
    /// owner, and the verdict as the ready view renders it.
    pub(crate) fn as_read(&self) -> serde_json::Value {
        let mut block = serde_json::json!({ "handle": self.handle.to_string() });
        if let Some(status) = &self.status {
            block["status"] = status.as_str().into();
        }
        if let Some(owner) = &self.owner {
            block["owner"] = owner.as_str().into();
        }
        ready::render(&mut block, &self.verdict);
        block
    }

    /// **The block a tick's receipt carries**: the work, its status, and whether
    /// it is still open. The tick changed none of it.
    pub(crate) fn as_receipt(&self) -> serde_json::Value {
        let mut block = serde_json::json!({
            "handle": self.handle.to_string(),
            "work_open": self.is_open(),
        });
        if let Some(status) = &self.status {
            block["status"] = status.as_str().into();
        }
        block
    }
}

/// **How far along a bundle is**, worked out at each read from the work its
/// entries link and stored nowhere. The one function that counts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Progress {
    done: usize,
    of: usize,
}

impl Progress {
    /// **Done out of all**: of the links, those that are a piece of work or a
    /// project are the whole, and those among them that are finished are done. A
    /// link to anything else (a person, a place) counts for nothing, and neither
    /// does work that was archived unfinished. `None` when no link is left, so a
    /// list of plain entries says nothing about progress.
    pub(crate) fn of(
        links: &BTreeSet<EntityId>,
        linked: &BTreeMap<EntityId, Linked>,
    ) -> Option<Progress> {
        let work: Vec<&Linked> = links
            .iter()
            .filter_map(|link| linked.get(link))
            .filter(|work| !work.left_the_bundle())
            .collect();
        (!work.is_empty()).then(|| Progress {
            done: work.iter().filter(|work| work.is_finished()).count(),
            of: work.len(),
        })
    }

    /// **The block a list read carries**: the two numbers and the words that say
    /// them, `done` once every piece is.
    pub(crate) fn as_read(&self) -> serde_json::Value {
        serde_json::json!({
            "done": self.done,
            "of": self.of,
            "reads": if self.done == self.of {
                "done".to_string()
            } else {
                format!("{} of {} done", self.done, self.of)
            },
        })
    }
}

impl Jojobot {
    /// **The state of each piece of work among `handles`**, by handle. A handle
    /// that is not a piece of work or a project (a person, a place) has no state
    /// and is left out. **A read that fails is an error**, never a handle left
    /// out: a list that dropped the work it could not read would count the rest
    /// as the whole and read further along than it is.
    pub(crate) async fn linked_state(
        &self,
        handles: &BTreeSet<EntityId>,
    ) -> Result<BTreeMap<EntityId, Linked>, MemoryError> {
        let mut found = Vec::new();
        for kind in [EntityKind::WORK, EntityKind::PROJECT] {
            if !handles.iter().any(|handle| handle.kind() == Some(kind)) {
                continue;
            }
            let all = self.memory.list_entities(Some(kind)).await?;
            for entity in all.into_iter().filter(|e| handles.contains(&e.id)) {
                let fields = self.memory.fields(&entity.id).await?;
                found.push((entity, fields));
            }
        }
        let asked: Vec<ready::Asked<'_>> = found
            .iter()
            .map(|(entity, fields)| ready::Asked {
                kind: entity.kind.as_token(),
                fields,
                archived: entity.archived.is_some(),
            })
            .collect();
        let verdicts = ready::verdicts_of(self.memory.as_ref(), &asked).await?;
        let instance = self.instance_fields().await?;
        Ok(found
            .iter()
            .zip(verdicts)
            .map(|((entity, fields), verdict)| {
                (
                    entity.id.clone(),
                    Linked {
                        handle: entity.id.clone(),
                        status: fields.get("status").cloned(),
                        owner: fields.get("owner").cloned(),
                        verdict,
                        status_finished: jojobot_domain::memory::kinds::is_finished(
                            entity.kind.as_token(),
                            fields,
                            &instance,
                        ),
                    },
                )
            })
            .collect())
    }
}
