//! **The rules of a bot that hold a seat**, answered from the state alone.
//!
//! A starred rule rides the bot's boot, and the bot has only so many seats
//! ([`memory::seats`]). The admission of a new star asks how many seats are in
//! use and which rules hold them. This is that one question: a narrow read of
//! the records, not a second representation of a claim.
//!
//! A record is a rule that can hold a seat when it is active, draws no
//! `connection` edge (that is a thought), carries `starred: true`, and is no
//! bookkeeping record of jojobot's own (a review mark, an engine record, a
//! settings record). The shapes the store recognised by the words of a record
//! written before those labels existed are not read here: the import (e) labels
//! them.

use super::{FieldValue, ObjectId, RecordId, State};
use crate::memory::{self, EdgeShape, FactStatus, review};

/// A rule that holds a seat, as the refusal of a full set of seats names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarredRule {
    pub record: RecordId,
    /// The claim's address on the bot, `f` and its number.
    pub label: String,
    pub content: String,
}

/// A record a thing holds, as a refusal of a full thing names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRecord {
    pub record: RecordId,
    pub label: String,
    pub content: String,
}

impl State {
    /// **The live records of a thing**, in the order of their labels: the active
    /// claims homed on it that take a slot under a record ceiling. The review
    /// mark and a record jojobot wrote about an act of its own take none.
    pub fn live_records(&self, home: &ObjectId) -> Vec<HeldRecord> {
        let home = self.survivor(home);
        let mut live: Vec<HeldRecord> = self
            .records
            .iter()
            .filter(|(record, held)| {
                held.claim.status == FactStatus::Active
                    && self.survivor(&held.claim.home) == home
                    && !self.writes_a_key(
                        home,
                        record,
                        &[review::REVIEW_MARK, memory::capacity::ENGINE_RECORD],
                    )
            })
            .map(|(record, held)| HeldRecord {
                record: record.clone(),
                label: held.claim.label.clone(),
                content: held.claim.content.clone(),
            })
            .collect();
        live.sort_by_key(|held| {
            (
                memory::numbered::whole_number(held.label.trim_start_matches('f')),
                held.label.clone(),
            )
        });
        live
    }

    /// **When a claim was last touched**: the newest moment the store took in a
    /// write of the claim. A claim with no recorded write moment is never quiet.
    pub fn last_touched(&self, record: &RecordId) -> Option<jiff::Timestamp> {
        self.streams
            .values()
            .flatten()
            .filter(|write| &write.record == record)
            .filter_map(|write| write.written_at)
            .max()
    }

    /// Whether the record wrote any of the keys on the thing.
    fn writes_a_key(&self, home: &ObjectId, record: &RecordId, keys: &[&str]) -> bool {
        self.streams.iter().any(|((subject, key), stream)| {
            subject == home
                && keys.contains(&key.0.as_str())
                && stream.iter().any(|write| &write.record == record)
        })
    }

    /// **The starred rules of a bot, oldest first**: the order a boot seats them
    /// in. A record kept without a moment is older than every stamped one, and the
    /// number of the label breaks a tie.
    pub fn starred_rules(&self, bot: &ObjectId) -> Vec<StarredRule> {
        let bot = self.survivor(bot);
        let mut starred: Vec<(&RecordId, &super::Record)> = self
            .records
            .iter()
            .filter(|(_, held)| {
                held.claim.status == FactStatus::Active
                    && self.survivor(&held.claim.home) == bot
                    && held
                        .claim
                        .edge
                        .as_ref()
                        .is_none_or(|(shape, _)| *shape != EdgeShape::Connection.to_string())
            })
            .filter(|(record, _)| self.is_starred_and_plain(bot, record))
            .collect();
        starred.sort_by_key(|(_, held)| {
            (
                held.claim.inserted_at,
                memory::numbered::whole_number(held.claim.label.trim_start_matches('f')),
                held.claim.label.clone(),
            )
        });
        starred
            .into_iter()
            .map(|(record, held)| StarredRule {
                record: record.clone(),
                label: held.claim.label.clone(),
                content: held.claim.content.clone(),
            })
            .collect()
    }

    /// Whether the record's own writes on the bot say `starred: true` and say
    /// nothing a bookkeeping record says.
    fn is_starred_and_plain(&self, bot: &ObjectId, record: &RecordId) -> bool {
        let mut starred = false;
        for ((subject, key), stream) in &self.streams {
            if subject != bot {
                continue;
            }
            for write in stream.iter().filter(|write| &write.record == record) {
                if matches!(
                    key.0.as_str(),
                    review::REVIEW_MARK | review::SETTINGS_RECORD | memory::capacity::ENGINE_RECORD
                ) {
                    return false;
                }
                if key.0 == "starred" {
                    starred = matches!(&write.write, FieldValue::Set(value) if value == "true");
                }
            }
        }
        starred
    }
}

#[cfg(test)]
mod tests;
