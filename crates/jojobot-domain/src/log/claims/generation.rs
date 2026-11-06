//! **The generation of a bot request, worked out from the State and the batch.**
//! A request's spec is the fold of its writes; a batch that leaves the fold with
//! another spec than it had makes the next generation (rule 772: one fold, the
//! one [`memory::folded_fields`] runs). The decision is made before the commit,
//! because the number is written in the same batch as the act.
//!
//! Nothing here clones the State: only the writes of the requests the batch
//! touches are read, so the cost is what those requests hold.

use std::collections::BTreeMap;

use super::{ClaimBody, ClaimEvent, FieldValue, State};
use crate::log::{ObjectId, RecordId};
use crate::memory::types::DeclaredType;
use crate::memory::{self, EntityId, FactId, KeyWrite, bot_request};

/// **The generation one batch makes on one request.** `record` is the claim of
/// the batch that touched the request last: the number is written under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub object: ObjectId,
    pub record: RecordId,
    pub number: u64,
}

impl State {
    /// **The generations `pending` makes**, one for each bot request whose folded
    /// spec the batch changes, in the order the requests are first touched. The
    /// writes read are the request's held ones plus the key writes and the
    /// status changes of `pending`; a clear of a key not held, a resend and a
    /// batch that touches no spec key make none. A batch that touches two
    /// requests makes one for each.
    pub fn generations_after(
        &self,
        pending: &[ClaimEvent],
        declared: &[DeclaredType],
    ) -> Vec<Generation> {
        let mut touched: Vec<(ObjectId, RecordId)> = Vec::new();
        let mut created: BTreeMap<&RecordId, &ClaimBody> = BTreeMap::new();
        let mut named: BTreeMap<&ObjectId, &str> = BTreeMap::new();
        for event in pending {
            match event {
                ClaimEvent::ObjectCreated { object, body } => {
                    named.insert(object, &body.name);
                }
                ClaimEvent::RecordCreated { record, claim } => {
                    created.insert(record, claim);
                }
                ClaimEvent::FieldWritten {
                    record, subject, ..
                } => touch(&mut touched, self.survivor(subject), record),
                ClaimEvent::RecordRevised { record, claim } => {
                    touch(&mut touched, self.survivor(&claim.home), record)
                }
                _ => {}
            }
        }
        touched
            .into_iter()
            .filter_map(|(object, record)| {
                let handle = self
                    .handle_of(&object)
                    .or_else(|| named.get(&object).map(|name| EntityId(name.to_string())))?;
                if !bot_request::is_request(&handle) {
                    return None;
                }
                let before = self.key_writes(&object);
                let mut after = before.clone();
                for event in pending {
                    match event {
                        ClaimEvent::RecordCreated { record, claim } => {
                            created.insert(record, claim);
                        }
                        ClaimEvent::RecordRevised { claim, .. }
                            if self.survivor(&claim.home) == &object =>
                        {
                            for write in after.iter_mut().filter(|w| w.fact.0 == claim.label) {
                                write.status = claim.status;
                            }
                        }
                        ClaimEvent::FieldWritten {
                            record,
                            subject,
                            key,
                            ordinal,
                            class,
                            write,
                            ..
                        } if self.survivor(subject) == &object
                            && memory::reaches_the_thing(*class) =>
                        {
                            let claim = created
                                .get(record)
                                .copied()
                                .or_else(|| self.records.get(record).map(|held| &held.claim))?;
                            after.push(KeyWrite {
                                key: key.0.clone(),
                                ordinal: *ordinal,
                                value: match write {
                                    FieldValue::Set(value) => Some(value.clone()),
                                    FieldValue::Clear => None,
                                },
                                fact: FactId(claim.label.clone()),
                                status: claim.status,
                                provenance: claim.provenance,
                                standing: claim.standing,
                                note: claim.details.clone(),
                                recorded_by: claim.app.clone(),
                                writer: None,
                                run: None,
                                written_at: None,
                            });
                        }
                        _ => {}
                    }
                }
                let (held, left) = (
                    memory::folded_fields(&before, declared),
                    memory::folded_fields(&after, declared),
                );
                let folded = |fields: &BTreeMap<String, String>| {
                    fields
                        .get(bot_request::GENERATION)
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .unwrap_or(0)
                };
                let highest = bot_request::highest_number(
                    after
                        .iter()
                        .filter(|write| write.key == bot_request::GENERATION)
                        .map(|write| write.value.as_deref()),
                );
                bot_request::next_generation_withdrawing(
                    &handle,
                    &bot_request::spec_of(&held),
                    &bot_request::spec_of(&left),
                    folded(&held),
                    folded(&left),
                    highest,
                )
                .map(|number| Generation {
                    object,
                    record,
                    number: number.parse().expect("a generation is a number"),
                })
            })
            .collect()
    }
}

/// Note that `record` touched `object`: first touch fixes the order, the last
/// touch names the record.
fn touch(touched: &mut Vec<(ObjectId, RecordId)>, object: &ObjectId, record: &RecordId) {
    match touched.iter_mut().find(|(held, _)| held == object) {
        Some(entry) => entry.1 = record.clone(),
        None => touched.push((object.clone(), record.clone())),
    }
}
