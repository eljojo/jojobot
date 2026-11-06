//! **The generation of a bot request, worked out from the State and the batch.**
//! A request's spec is the fold of its writes; a batch that leaves the fold with
//! another spec than it had makes the next generation (rule 772: one fold, the
//! one [`memory::folded_fields`] runs). The decision is made before the commit,
//! because the number is written in the same batch as the act.
//!
//! Nothing here clones the State: only the writes of the requests the batch
//! touches are read, so the cost is what those requests hold.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{ClaimBody, ClaimEvent, FieldValue, KeyId, State};
use crate::log::{ObjectId, RecordId};
use crate::memory::types::DeclaredType;
use crate::memory::{
    self, EntityId, FactId, FactStatus, KeyWrite, Provenance, Standing, WriteClass, bot_request,
};

/// **The generation one batch makes on one request.** `record` is the claim of
/// the batch that touched the request last. The number is written under it when
/// that claim is live once the batch has landed (`live`); otherwise it is written
/// under a live claim of its own, because the keys of a claim that was taken back
/// leave the thing with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub object: ObjectId,
    pub handle: EntityId,
    pub record: RecordId,
    pub live: bool,
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
        // **The spelling a batch binds is the handle of a thing it makes**; the name
        // it is created under is a display name and says nothing of the kind.
        let mut bound: BTreeMap<&ObjectId, &str> = BTreeMap::new();
        let mut revised: BTreeMap<&RecordId, &ClaimBody> = BTreeMap::new();
        for event in pending {
            match event {
                ClaimEvent::AddressBound { object, address } => {
                    bound.insert(object, address);
                }
                ClaimEvent::RecordCreated { record, claim } => {
                    created.insert(record, claim);
                }
                ClaimEvent::FieldWritten {
                    record, subject, ..
                } => touch(&mut touched, self.survivor(subject), record),
                ClaimEvent::RecordRevised { record, claim } => {
                    revised.insert(record, claim);
                    touch(&mut touched, self.survivor(&claim.home), record)
                }
                _ => {}
            }
        }
        // **A writer that numbered its own act has done it**: its batch carries
        // the write of the generation, and a second number would name the same spec
        // twice. The writers that still work the number out themselves are met here
        // until they leave the work to this function.
        let numbered: Vec<&ObjectId> = pending
            .iter()
            .filter_map(|event| match event {
                ClaimEvent::FieldWritten { subject, key, .. }
                    if key.0 == bot_request::GENERATION =>
                {
                    Some(self.survivor(subject))
                }
                _ => None,
            })
            .collect();
        touched
            .into_iter()
            .filter(|(object, _)| !numbered.contains(&object))
            .filter_map(|(object, record)| {
                let handle = self
                    .handle_of(&object)
                    .or_else(|| bound.get(&object).map(|name| EntityId(name.to_string())))?;
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
                .map(|number| {
                    let status = revised
                        .get(&record)
                        .or_else(|| created.get(&record))
                        .map(|claim| claim.status)
                        .or_else(|| self.records.get(&record).map(|held| held.claim.status));
                    Generation {
                        object,
                        handle: handle.clone(),
                        record,
                        live: status == Some(FactStatus::Active),
                        number: number.parse().expect("a generation is a number"),
                    }
                })
            })
            .collect()
    }
}

impl State {
    /// **The events that write the generations `pending` makes**, ready to join
    /// the batch. A number rides on the claim that touched the request when that
    /// claim is live once the batch has landed. When the claim was taken back, the
    /// number rides on a new live claim written beside it, whose record `fresh`
    /// names and which is dated `recorded_at`.
    pub fn generation_events(
        &self,
        pending: &[ClaimEvent],
        declared: &[DeclaredType],
        recorded_at: Date,
        mut fresh: impl FnMut() -> RecordId,
    ) -> Vec<ClaimEvent> {
        let key = KeyId(bot_request::GENERATION.to_string());
        let mut events = Vec::new();
        for generation in self.generations_after(pending, declared) {
            let record = match generation.live {
                true => generation.record.clone(),
                false => {
                    let record = fresh();
                    let held = pending
                        .iter()
                        .filter_map(|event| match event {
                            ClaimEvent::RecordCreated { claim, .. }
                                if self.survivor(&claim.home) == &generation.object =>
                            {
                                claim.label.strip_prefix('f')?.parse::<u64>().ok()
                            }
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0)
                        .max(self.highest_claim_number(&generation.object));
                    events.push(ClaimEvent::RecordCreated {
                        record: record.clone(),
                        claim: ClaimBody {
                            label: format!("f{}", held + 1),
                            home: generation.object.clone(),
                            content: format!("Set generation on @{}.", generation.handle),
                            details: None,
                            provenance: Provenance::Inference,
                            standing: Standing::default_for(Provenance::Inference),
                            status: FactStatus::Active,
                            recorded_at,
                            happened_at: None,
                            happened_through: None,
                            stale_after: None,
                            edge: None,
                            derived_from: None,
                            refs: Vec::new(),
                            stands_for: Vec::new(),
                            receipt: None,
                            app: None,
                            inserted_at: None,
                        },
                    });
                    record
                }
            };
            events.push(ClaimEvent::FieldWritten {
                record,
                ordinal: self.next_ordinal(&generation.object, &key),
                subject: generation.object,
                key: key.clone(),
                class: WriteClass::Sets,
                write: FieldValue::Set(generation.number.to_string()),
                targets: Vec::new(),
            });
        }
        events
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
