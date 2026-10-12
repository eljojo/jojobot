//! **What the document verbs share**: which records of a document are its live
//! sections, in order, and the section each one is.
//!
//! A section is a claim on the document thing that carries an `order` field, so
//! the records that make up a document are found by the key they carry, the way
//! a loop is found by the keys that make it one. A record without it is an
//! ordinary claim about the document and is not part of its text.

use super::*;
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

impl Jojobot {
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
        let target = FactAddress::parse(after)?;
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
