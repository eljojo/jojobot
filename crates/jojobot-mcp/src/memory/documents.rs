//! **What the document verbs share**: which records of a document are its live
//! sections, in order, and the section each one is.
//!
//! A section is a claim on the document thing that carries an `order` field, so
//! the records that make up a document are found by the key they carry, the way
//! a loop is found by the keys that make it one. A record without it is an
//! ordinary claim about the document and is not part of its text.

use super::*;
use jojobot_domain::memory::document::{self, Section};

/// One live section of a document: the record, its order key and the section it
/// holds.
pub(crate) struct Held<'a> {
    pub(crate) order: String,
    pub(crate) section: Section,
    pub(crate) fact: &'a Fact,
}

/// **The live sections among a document's records, in reading order.** A section that
/// was archived or taken back is not live: it stays readable by its address and
/// is left out of the page. Ties in the order key, which only a hand-written key
/// can make, fall back to the record's id so the page is still the same page
/// every time it is read.
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
            })
        })
        .collect();
    held.sort_by(|a, b| (&a.order, &a.fact.id.0).cmp(&(&b.order, &b.fact.id.0)));
    held
}

#[cfg(test)]
mod tests;
