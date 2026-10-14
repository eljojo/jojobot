//! **What the document verbs share**: which records of a document are its live
//! sections, in order, and the section each one is.
//!
//! A section is a claim on the document thing that carries an `order` field, so
//! the records that make up a document are found by the key they carry, the way
//! a loop is found by the keys that make it one. A record without it is an
//! ordinary claim about the document and is not part of its text.

use super::*;
use jojobot_domain::memory::FactId;
use jojobot_domain::memory::document::{self, Section};
use jojobot_domain::memory::order;

/// One live section of a document: the record, its order key and the section it
/// holds.
pub(crate) struct Held<'a> {
    pub(crate) order: String,
    pub(crate) section: Section,
    pub(crate) fact: &'a Fact,
    /// For a chunk that carries a section on: the local id of the record that
    /// holds that section's heading.
    pub(crate) head: Option<&'a str>,
}

/// **The live sections among a document's records, in reading order.** A section
/// that was archived or taken back is not live: it stays readable by its address
/// and is left out of the page. **A chunk whose heading is not live is not live
/// either**: archiving a section takes the whole of it out. Ties in the order
/// key, which only a hand-written key can make, fall back to the record's id so
/// the page is still the same page every time it is read.
pub(crate) fn live_sections(facts: &[Fact]) -> Vec<Held<'_>> {
    let mut held: Vec<Held<'_>> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter_map(|fact| {
            let (order, section) =
                document::from_record(&fact.content, fact.details.as_deref(), &fact.fields)?;
            Some(Held {
                order,
                section,
                fact,
                head: fact
                    .fields
                    .get(document::CONTINUES)
                    .map(String::as_str)
                    .filter(|_| facts_continue(fact)),
            })
        })
        .collect();
    held.sort_by(|a, b| (&a.order, &a.fact.id.0).cmp(&(&b.order, &b.fact.id.0)));
    let heads: std::collections::BTreeSet<&str> = held
        .iter()
        .filter(|h| h.head.is_none())
        .map(|h| h.fact.id.0.as_str())
        .collect();
    held.retain(|h| h.head.is_none_or(|head| heads.contains(head)));
    held
}

fn facts_continue(fact: &Fact) -> bool {
    fact.fields
        .get(document::CONTINUES)
        .is_some_and(|value| !value.is_empty() && value != "false")
}

/// **The positions of a section and of every chunk that carries it on**, in
/// reading order. A position that names a chunk stands for the section it
/// carries on.
pub(crate) fn run_of(live: &[Held<'_>], at: usize) -> Vec<usize> {
    let head = match live[at].head {
        Some(head) => live.iter().position(|h| h.fact.id.0 == head).unwrap_or(at),
        None => at,
    };
    let id = live[head].fact.id.0.as_str();
    (0..live.len())
        .filter(|n| *n == head || live[*n].head == Some(id))
        .collect()
}

/// **Where a moved section goes**: the order key for the section itself and for
/// each chunk that carries it on, which move with it.
pub(crate) struct MovePlan {
    pub(crate) head_order: String,
    pub(crate) rest: Vec<(FactAddress, String)>,
}

/// **A document's records as they stood at one moment**, rebuilt so the same
/// reading code that serves today's page serves that one.
pub(crate) struct AsOf {
    /// The records that existed then, each carrying the wording, the archive
    /// state and the section keys it held then. A record made later is absent.
    pub(crate) facts: Vec<Fact>,
    /// The instant the answer is about.
    pub(crate) moment: jiff::Timestamp,
    /// The records whose state at that moment rests on a write the store kept
    /// with no moment of its own, and that the read therefore placed before
    /// every stamped one. It names them rather than vouching for them.
    pub(crate) unplaced: Vec<FactId>,
}

/// **The keys a section is read back from, besides its words.** What a section
/// held under them then is what places it and shapes its edges.
const SECTION_KEYS: [&str; 4] = [
    document::ORDER,
    document::LEVEL,
    document::EDGES,
    document::CONTINUES,
];

impl Jojobot {
    /// **The instant a caller's `as_of` names.** A day means the end of that
    /// day, in the zone the asking run works in, else the instance's: the
    /// document as it stood when that day was over. An instant is taken as given.
    pub(crate) async fn moment_of(
        &self,
        asked: &str,
        caller: Option<&crate::caller::Caller>,
    ) -> Result<jiff::Timestamp, MemoryError> {
        let asked = asked.trim();
        if let Ok(day) = asked.parse::<jiff::civil::Date>() {
            let zone = match caller.and_then(crate::caller::Caller::zone) {
                Some(zone) => zone,
                None => self.unzoned_frame().await,
            };
            return day
                .at(23, 59, 59, 999_999_999)
                .to_zoned(zone)
                .map(|end| end.timestamp())
                .map_err(|why| {
                    MemoryError::InvalidQuery(format!(
                        "as_of '{asked}' names a day this zone cannot place: {why}"
                    ))
                });
        }
        asked.parse::<jiff::Timestamp>().map_err(|_| {
            MemoryError::InvalidQuery(format!(
                "as_of is a day, YYYY-MM-DD, or an instant such as 2026-08-03T12:00:00Z, and \
                 '{asked}' is neither"
            ))
        })
    }

    /// **Rebuild a document's records as they stood at `moment`.** Each record
    /// takes its newest write at or before the moment, and each section key its
    /// newest write at or before it. A record with no write by then did not exist
    /// and is left out; a record whose write by then was an archive is present
    /// and archived. A write with no moment is older than every write with one.
    pub(crate) async fn facts_as_of(
        &self,
        subject: &EntityId,
        moment: jiff::Timestamp,
    ) -> Result<AsOf, MemoryError> {
        let facts = self.memory.recall(subject).await?;
        let chains = self.memory.claim_histories(subject).await?;
        let trails = self.memory.key_trails(subject).await?;
        let by_then =
            |written_at: Option<jiff::Timestamp>| written_at.is_none_or(|at| at <= moment);
        let mut kept = Vec::with_capacity(facts.len());
        let mut unplaced = Vec::new();
        for mut fact in facts {
            let Some(write) = chains
                .get(&fact.id)
                .and_then(|chain| chain.iter().rev().find(|write| by_then(write.written_at)))
            else {
                continue;
            };
            if write.written_at.is_none() {
                unplaced.push(fact.id.clone());
            }
            fact.content = write.content.clone();
            fact.details = write.details.clone();
            fact.status = write.status;
            for key in SECTION_KEYS {
                let held = trails
                    .record_writes
                    .iter()
                    .filter(|w| w.fact == fact.id && w.key == key && by_then(w.written_at))
                    .max_by_key(|w| w.ordinal)
                    .and_then(|w| w.value.clone());
                match held {
                    Some(value) => {
                        fact.fields.insert(key.to_string(), value);
                    }
                    None => {
                        fact.fields.remove(key);
                    }
                }
            }
            kept.push(fact);
        }
        Ok(AsOf {
            facts: kept,
            moment,
            unplaced,
        })
    }

    /// **Where an address a caller holds reads now**: moved by the merge walk when
    /// a merge moved its claim, and homed under the handle its thing answers to.
    async fn served_address(&self, address: &FactAddress) -> Result<FactAddress, MemoryError> {
        let address = self
            .memory
            .moved_address(address)
            .await?
            .unwrap_or_else(|| address.clone());
        Ok(FactAddress::new(
            self.current_handle(&address.home).await?,
            address.local,
        ))
    }

    /// **Plan the move of the section at `address` behind the section `after`
    /// names.** Both have to be live sections of the same document. A section
    /// moves with the chunks that carry it on, and lands behind the target's own
    /// chunks, so a section is never cut in two by a move. A chunk is not moved
    /// on its own: the section it carries on is.
    pub(crate) async fn plan_move(
        &self,
        address: &FactAddress,
        after: &str,
    ) -> Result<MovePlan, MemoryError> {
        let refused = |what: String| {
            MemoryError::InvalidFact(format!(
                "`after` moves a section of a document to the place behind another section of \
                 the same document. {what}"
            ))
        };
        // **Both addresses are read where their claims are now**: under the
        // handle the document answers to today, and past a merge. A caller holds
        // an address from before a rename or a merge, and the sections below
        // are listed under the handle they wear now.
        let address = &self.served_address(address).await?;
        let target = self.served_address(&FactAddress::parse(after)?).await?;
        let facts = self.memory.recall(&address.home).await?;
        let live = live_sections(&facts);
        let is = |held: &Held<'_>, wanted: &FactAddress| {
            held.fact.home == wanted.home && held.fact.id == wanted.local
        };
        let Some(at) = live.iter().position(|held| is(held, address)) else {
            return Err(refused(format!(
                "{address} is not a live section: it is no part of a document, or it was \
                 archived. Edit an ordinary claim without `after`"
            )));
        };
        if live[at].head.is_some() {
            return Err(refused(format!(
                "{address} carries on the section before it. Move that section: this one \
                 moves with it"
            )));
        }
        let run = run_of(&live, at);
        let Some(target_at) = live.iter().position(|held| is(held, &target)) else {
            return Err(refused(format!(
                "{after} is not a live section of {}. Name the address of the section it \
                 should follow",
                address.home
            )));
        };
        if run.contains(&target_at) {
            return Err(refused("A section cannot follow itself.".to_string()));
        }
        let target_end = *run_of(&live, target_at)
            .last()
            .expect("a run holds its section");
        let remaining: Vec<&Held<'_>> = live
            .iter()
            .enumerate()
            .filter(|(n, _)| !run.contains(n))
            .map(|(_, held)| held)
            .collect();
        let behind = remaining
            .iter()
            .position(|held| is(held, &live[target_end].fact.address()))
            .expect("the target's last chunk is among the sections that stay");
        let orders = order::spread(
            Some(&remaining[behind].order),
            remaining.get(behind + 1).map(|held| held.order.as_str()),
            run.len(),
        )
        .map_err(|e| refused(format!("No room is left to place it there: {e}")))?;
        let mut orders = orders.into_iter();
        Ok(MovePlan {
            head_order: orders.next().expect("one order for the section itself"),
            // The chunks in the order they read now, which after a move that
            // stopped partway is not the order the heading sits in.
            rest: run
                .iter()
                .filter(|n| **n != at)
                .map(|n| live[*n].fact.address())
                .zip(orders)
                .collect(),
        })
    }
}

/// **A refusal that came after some writes had landed**, carrying what landed.
/// A refusal that is an answer gains the keys; one that is an error gains them
/// in its data, and its stock words, which say nothing was written, are replaced
/// by `words` because here they would be false.
pub(crate) fn stopped_with(
    refused: Result<CallToolResult, McpError>,
    carried: serde_json::Value,
    words: String,
) -> Result<CallToolResult, McpError> {
    match refused {
        Ok(refused) => {
            let mut body = refused
                .content
                .first()
                .and_then(|block| block.as_text())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text.text).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            body.as_object_mut()
                .expect("a refusal body is an object")
                .extend(carried.as_object().cloned().unwrap_or_default());
            crate::answer::json_result(&body)
        }
        Err(error) => {
            let mut data = error.data.unwrap_or_else(|| serde_json::json!({}));
            if let (Some(data), Some(carried)) = (data.as_object_mut(), carried.as_object()) {
                data.extend(carried.clone());
            }
            Err(McpError::new(error.code, words, Some(data)))
        }
    }
}

#[cfg(test)]
mod tests;
