//! **A claim must fit one ordinary answer.**
//!
//! A read of a thing serves each of its records whole or not at all, and every
//! answer has a ceiling. A record that cannot fit one answer even alone is
//! served over the ceiling, which is the one thing the ceiling exists to
//! prevent. So the write that would make such a record is refused, with the
//! ways to say it, instead of the read being cut and the words being lost.
//!
//! The question is asked of the same arithmetic the read uses
//! ([`record_room`]), on a probe of the answer a read of the thing would serve
//! once the write has landed: the thing's own row with its folded fields, the
//! envelope, the first-read teaching, and the record rendered at its widest.

use super::*;
use crate::memory::recall::record_room;
use crate::orientation::floor::{prospective_fact, widest_stamp};
use jojobot_domain::memory::{
    EntityPatch, Fact, FactAddress, FactPatch, FactStatus, NewFact, Provenance, Standing,
};

/// A thing's folded fields, one value per key.
pub(crate) type Fold = std::collections::BTreeMap<String, String>;

/// **The widest revision count a record can wear.** A record written twice or
/// more carries a note naming how many times, and the note grows with the
/// count's digits; the probe wears five so an edit later never moves a record
/// that fit into one that does not.
const WIDEST_REVISION_COUNT: usize = 99_999;

/// **The last day a record can be read on.** A record with a stay-good day
/// reads as stale once the day passes, and the stale note makes it longer; the
/// probe is read on a day past every day so the longest rendering is the one
/// measured.
fn last_day() -> jiff::civil::Date {
    jiff::civil::date(9999, 12, 31)
}

/// **A record's size as a read serves it, and the room one part has for it.**
struct Measured {
    size: usize,
    room: usize,
    /// The thing's own row and the room one answer has for it, **when the row
    /// alone does not fit**: then no record on the thing can, and the cause is
    /// the row.
    row_over: Option<(usize, usize)>,
}

impl Jojobot {
    /// **The refusal for a record too big to be read back whole**, or `None`.
    ///
    /// `fact` is the record as the write would leave it. `creating` is the
    /// thing when the same call makes it. `before` is the record as it stands
    /// when the write is an edit: an edit that does not make the record
    /// bigger is never refused, so a record already over the ceiling stays
    /// repairable by every write that does not grow it.
    ///
    /// A thing or a store this cannot measure is not a thing known to be over,
    /// so it passes.
    pub(crate) async fn refuses_a_claim_too_big_to_read(
        &self,
        fact: &Fact,
        creating: Option<&Entity>,
        before: Option<&Fact>,
    ) -> Option<MemoryError> {
        let after = self.measured_claim(fact, creating).await?;
        if after.size <= after.room {
            return None;
        }
        if let Some(before) = before
            && let Some(was) = self.measured_claim(before, creating).await
            && after.size <= was.size
        {
            return None;
        }
        // **The cause is named.** A row already over leaves no room for any record,
        // and telling the caller to shorten the record would send it the wrong way.
        if let Some((row, row_room)) = after.row_over {
            return Some(MemoryError::ThingTooLarge {
                subject: fact.home.to_string(),
                size: row,
                room: row_room,
            });
        }
        Some(MemoryError::ClaimTooLarge {
            subject: fact.home.to_string(),
            size: after.size,
            room: after.room,
        })
    }

    /// **The same check for a record that is about to be created**: the
    /// prospective record is built the way a store builds it, stamped as the
    /// widest the store can stamp it.
    pub(crate) async fn refuses_a_new_claim_too_big_to_read(
        &self,
        new: &NewFact,
        creating: Option<&Entity>,
    ) -> Option<MemoryError> {
        let held = match creating {
            Some(_) => 0,
            None => self.memory.recall(&new.subject).await.ok()?.len(),
        };
        let fact = prospective_fact(new, held + 1, Some(widest_stamp(self.clock().now())));
        self.refuses_a_claim_too_big_to_read(&fact, creating, None)
            .await
    }

    /// **The same check for an edit**, asked of the record as the edit would
    /// leave it and compared with the record as it stands.
    pub(crate) async fn refuses_an_edit_too_big_to_read(
        &self,
        address: &FactAddress,
        patch: &FactPatch,
    ) -> Option<MemoryError> {
        let held = self.memory.recall(&address.home).await.ok()?;
        let was = held.iter().find(|fact| fact.id == address.local)?;
        let mut edited = was.clone();
        jojobot_domain::memory::apply_fact_patch(&mut edited, patch).ok()?;
        self.refuses_a_claim_too_big_to_read(&edited, None, Some(was))
            .await
    }

    async fn measured_claim(&self, fact: &Fact, creating: Option<&Entity>) -> Option<Measured> {
        // Stamped as the widest the store can stamp it, whatever the record
        // carries now: a stamp prints from 20 to 30 characters, and a record
        // that fits only with a short one would stop fitting on the next edit.
        let mut stamped = fact.clone();
        stamped.inserted_at = Some(widest_stamp(self.clock().now()));
        // The same reasoning for the three words an edit can change: a record
        // measured in its longest spelling of each does not grow when an edit
        // archives it, settles it or restates how it is known, so a record that
        // fits stays editable and a record already over stays repairable.
        stamped.status = FactStatus::Archived;
        stamped.standing = Standing::Settled;
        stamped.provenance = Provenance::Observation;
        let fact = &stamped;
        let (entity, mut folded) = self.thing_as_read(&fact.home, creating).await?;
        // The describing keys stay on their claim and never reach the thing.
        let describing = crate::seed::describing_keys();
        for (key, value) in &fact.fields {
            if !describing.contains(key) {
                folded.insert(key.clone(), value.clone());
            }
        }
        let record = crate::memory::wire::fact_json(fact, last_day(), Some(WIDEST_REVISION_COUNT));
        let (skeleton, rest, widest) = read_of_a_thing(&entity, &folded);
        let row_room = record_room(rest, 0, widest);
        Some(Measured {
            size: record.to_string().chars().count() + 1,
            room: record_room(rest, skeleton, widest),
            row_over: (skeleton > row_room).then_some((skeleton, row_room)),
        })
    }

    /// **The thing as a read serves it: its row and its folded fields**, in one
    /// targeted read of that thing. A listing of every entity to find one would
    /// make every write cost the size of the store.
    async fn thing_as_read(
        &self,
        id: &EntityId,
        creating: Option<&Entity>,
    ) -> Option<(Entity, Fold)> {
        if let Some(entity) = creating {
            return Some((entity.clone(), Fold::new()));
        }
        let scanned = self.memory.scan_entity(id).await.ok()??;
        Some((scanned.entity?, scanned.fields))
    }

    /// **The refusal for a thing whose own row cannot be read back whole**, or
    /// `None`. The row is its name, its aliases and its folded fields; a read of
    /// the thing serves all of them whatever else it was asked for, so a row
    /// that does not fit one answer leaves no room for any record on it.
    ///
    /// `before` is the thing as it stands when the write is an edit. As with a
    /// record, an edit that does not make the row bigger is never refused.
    pub(crate) fn refuses_a_thing_too_big_to_read(
        after: (&Entity, &Fold),
        before: Option<(&Entity, &Fold)>,
    ) -> Option<MemoryError> {
        let (skeleton, rest, widest) = read_of_a_thing(after.0, after.1);
        let room = record_room(rest, 0, widest);
        if skeleton <= room {
            return None;
        }
        if let Some(before) = before {
            let (was, ..) = read_of_a_thing(before.0, before.1);
            if skeleton <= was {
                return None;
            }
        }
        Some(MemoryError::ThingTooLarge {
            subject: after.0.id.to_string(),
            size: skeleton,
            room,
        })
    }

    /// **The same check for a thing about to be made.** It holds no keys yet.
    pub(crate) fn refuses_a_new_thing_too_big_to_read(creating: &Entity) -> Option<MemoryError> {
        Self::refuses_a_thing_too_big_to_read((creating, &Fold::new()), None)
    }

    /// **The same check for a relabelling**: the thing as the new name and
    /// aliases would leave it, against the thing as it stands.
    pub(crate) async fn refuses_a_relabelling_too_big_to_read(
        &self,
        handle: &EntityId,
        patch: &EntityPatch,
    ) -> Option<MemoryError> {
        if patch.name.is_none() && patch.aliases.is_none() {
            return None;
        }
        let (current, fold) = self.thing_as_read(handle, None).await?;
        let mut after = current.clone();
        if let Some(name) = &patch.name {
            after.name = name.clone();
        }
        if let Some(aliases) = &patch.aliases {
            after.aliases = aliases.clone();
        }
        Self::refuses_a_thing_too_big_to_read((&after, &fold), Some((&current, &fold)))
    }

    /// **The same check for a merge.** The survivor takes the duplicate's name
    /// and aliases as aliases and the duplicate's keys under its own, so the
    /// row is measured with both folded in, and the account the merge writes
    /// is measured as a record on the survivor.
    pub(crate) async fn refuses_a_merge_too_big_to_read(
        &self,
        duplicate: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
    ) -> Option<MemoryError> {
        let (kept, was) = self.thing_as_read(survivor, None).await?;
        let (folded, folded_fields) = self.thing_as_read(duplicate, None).await?;
        let mut after = kept.clone();
        for label in std::iter::once(&folded.name).chain(folded.aliases.iter()) {
            if !after.aliases.contains(label) && *label != after.name {
                after.aliases.push(label.clone());
            }
        }
        let mut now = was.clone();
        for (key, value) in folded_fields {
            now.entry(key).or_insert(value);
        }
        if let Some(refused) =
            Self::refuses_a_thing_too_big_to_read((&after, &now), Some((&kept, &was)))
        {
            return Some(refused);
        }
        let account =
            jojobot_domain::memory::merge_account(duplicate, survivor, reason, date).ok()?;
        self.refuses_a_new_claim_too_big_to_read(&account, None)
            .await
    }
}

/// **A read of one thing with facts asked for, before any fact is added**: the
/// size of the thing's own row, the size of the rest of the answer, and the size
/// of the pointer that names what comes after a part. The same three numbers
/// `fit_recall` spends against the ceiling.
fn read_of_a_thing(entity: &Entity, folded: &Fold) -> (usize, usize, usize) {
    let mut object = crate::memory::wire::entity_json(entity);
    object["fields"] = folded
        .iter()
        .map(|(key, value)| (key.clone(), serde_json::Value::from(value.as_str())))
        .collect::<serde_json::Map<_, _>>()
        .into();
    object["as_of"] = "9999-12-31".into();
    object["facts"] = serde_json::json!([]);
    let skeleton = object.to_string().chars().count() + 1;
    let envelope = serde_json::json!({
        "count": 1,
        "objects": [],
        "built_on": null,
        "record_history_unreached": null,
        "values": null,
        "overdue_as_of": null,
        "overdue_excluded": null,
        "near_day": null,
        "near_within_days": null,
        "near_clock": null,
        "near_unplaced": null,
        "withheld": 0,
        "archived_excluded": 0,
        "merged_excluded": 0,
        "teaching": [crate::teaching::CLAIMS_TEACHING],
    });
    let rest = envelope.to_string().chars().count() + crate::answer::STATUS_BAR_ROOM;
    let widest = crate::answer::not_shown(usize::MAX, usize::MAX, "objects")
        .to_string()
        .chars()
        .count();
    (skeleton, rest, widest)
}
