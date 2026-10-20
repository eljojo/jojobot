//! **What a write that landed lets the thing answer, said on the receipt.**
//!
//! The read side works out what a thing lacks against a type in many places
//! (`answers_type`, `unreadable_because`). The write side said nothing, so a
//! caller learned that a half-built thing could not answer only by asking the
//! question and getting less back. This block tells the writer at the moment it
//! can still act: add these keys, and jojobot can answer this.
//!
//! **One answer to "what does this thing lack" (rule 51).** The loop half asks
//! the loop carrier why it cannot say when the thing falls due. The type half
//! asks each declared type how the thing matches it. Nothing here keeps a
//! second list of keys.
//!
//! **Framed as a capability gained, never a shortfall, and nothing is refused.**
//! A thing that holds every key it could, or matches nothing, gets no block at
//! all: silence is the answer for most writes.

use super::*;
use jojobot_domain::attention::{self, Carrier, Due, Rhythms};
use jojobot_domain::memory::kinds;
use jojobot_domain::memory::types::DeclaredType;
use std::collections::BTreeMap;

/// One thing a caller could write next, and what it would let jojobot answer.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Step {
    /// The declared type this step completes, when it is a type's step.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) type_name: Option<String>,
    /// The keys to write, by name.
    pub(crate) add: Vec<String>,
    /// What holding them lets jojobot answer. One line.
    pub(crate) unlocks: String,
}

/// **The steps that make a loop say when it is next due**, read off what the
/// thing holds through the loop carrier.
///
/// A thing that holds none of a schedule is not a loop and gets none. The
/// schedule keys come as one step because a policy without a cadence is refused
/// when written, and a cadence alone is a whole schedule. A check-in is its own step because jojobot
/// works out the day the next cycle counts from, and the caller does not write it.
pub(crate) fn loop_steps(fields: &BTreeMap<String, String>) -> Vec<Step> {
    if Rhythms.interface().matched_by(fields).is_none() {
        return Vec::new();
    }
    let holds = |key: &str| fields.get(key).is_some_and(|held| !held.trim().is_empty());
    let mut steps = Vec::new();
    let mut needs_check_in = !holds(attention::LAST_CHECK_IN);
    let mut next_due = false;
    match Rhythms.due(fields) {
        Due::NotYetOpened => {
            needs_check_in = true;
            next_due = true;
        }
        Due::Unreadable => {
            let lacks = Rhythms
                .unreadable_because(fields)
                .map(|why| why.lacks)
                .unwrap_or_default();
            // A loop that lacks a cadence is offered both shapes; the policy
            // defaults, so it is never a step of its own.
            let schedule: Vec<String> = if lacks.contains(&attention::CADENCE_DAYS) {
                vec![
                    attention::CADENCE_DAYS.to_string(),
                    attention::RRULE.to_string(),
                ]
            } else {
                Vec::new()
            };
            if !schedule.is_empty() {
                steps.push(Step {
                    type_name: None,
                    unlocks: format!(
                        "Add {} and this loop can say when it is next due.",
                        schedule.join(" or ")
                    ),
                    add: schedule,
                });
            }
            if lacks.contains(&attention::COUNTS_FROM) {
                needs_check_in = true;
                next_due = true;
            }
        }
        Due::On(_) | Due::Never => {}
    }
    if needs_check_in {
        steps.push(Step {
            type_name: None,
            add: vec![attention::LAST_CHECK_IN.to_string()],
            unlocks: if next_due {
                "Record a check-in and this loop can say when it is next due and be offered \
                 when it is late."
                    .to_string()
            } else {
                "Record a check-in and this loop can say when it was last looked at.".to_string()
            },
        });
    }
    steps
}

/// **The steps that make a thing answer a declared type it already partly
/// answers.** A type that matches nothing the thing holds gets no step.
///
/// Two sorts of type are not a shape for a caller to complete: one whose every
/// key jojobot works out itself, and one named like a kind other than the
/// thing's own, which is that kind's keys read as a type. **The type named like
/// the thing's own kind is its kind's shape, and only the keys the kind
/// requires are offered**: an optional key of a kind is welcome and never earns
/// a line. **A key a step before it already names is left out**, so one key is
/// named once.
pub(crate) fn type_steps(
    kind: &str,
    fields: &BTreeMap<String, String>,
    types: &[DeclaredType],
    named: &[String],
) -> Vec<Step> {
    types
        .iter()
        .filter(|declared| declared.name == kind || !kinds::known(&declared.name))
        .filter(|declared| {
            !declared
                .fields
                .iter()
                .all(|field| attention::COMPUTED_KEYS.contains(&field.key.as_str()))
        })
        .filter_map(|declared| {
            let matched = declared.matched_by(fields)?;
            let of_kind = declared.name == kind;
            let add: Vec<String> = matched
                .lacking
                .into_iter()
                .filter(|key| !of_kind || declared.field(key).is_some_and(|field| field.required))
                .filter(|key| !named.contains(key))
                .collect();
            if add.is_empty() {
                return None;
            }
            Some(Step {
                type_name: Some(declared.name.clone()),
                unlocks: format!(
                    "Add {} and a question for the type {} (answers_type, fits_type) finds this \
                     thing whole.",
                    add.join(", "),
                    declared.name
                ),
                add,
            })
        })
        .collect()
}

impl Jojobot {
    /// **Put the `unlocks` block on the receipt of a write that landed**, when
    /// there is something to say. A thing that lacks nothing it could answer
    /// with, or matches nothing, adds nothing at all.
    ///
    /// **The type steps ride once per thing per session**, on the teaching
    /// ledger, and the ledger is spent only by a call that has a type step to
    /// say. A loop's step repeats until it is taken, because the loop is still
    /// unable to answer. A read that fails adds nothing, silently: this is
    /// bookkeeping riding on somebody else's write and must never fail it.
    pub(crate) async fn note_unlocks(
        &self,
        subject: &EntityId,
        caller: &Caller,
        body: &mut serde_json::Value,
    ) {
        let Ok(fields) = self.memory.fields(subject).await else {
            return;
        };
        let mut steps = loop_steps(&fields);
        if let Ok(types) = self.memory.declared_types().await {
            let named: Vec<String> = steps.iter().flat_map(|step| step.add.clone()).collect();
            let of_types = type_steps(subject.kind_token(), &fields, &types, &named);
            if !of_types.is_empty()
                && self
                    .first_contact(&format!("type-unlocks:{subject}"), Some(caller))
                    .await
            {
                steps.extend(of_types);
            }
        }
        if steps.is_empty() {
            return;
        }
        let Ok(records) = self.memory.recall(subject).await else {
            return;
        };
        let records = records
            .iter()
            .filter(|fact| fact.status == FactStatus::Active)
            .count();
        if let Some(object) = body.as_object_mut() {
            object.insert(
                "unlocks".into(),
                serde_json::json!({
                    "holds": { "keys": fields.len(), "records": records },
                    "steps": steps,
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests;
