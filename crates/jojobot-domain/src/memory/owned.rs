//! **What the binary supplies, and what it owns** (rule 234).
//!
//! A capability ships data by DECLARING it here — a value and where it goes —
//! and by doing nothing else. No table, no column, no verb, no change to how
//! that capability reads or writes. **A feature must not know that shipped data
//! exists**, so nothing above the layer that answers reads has a word for it.
//!
//! # The software's half is not stored
//!
//! A [`Provision`] lives in the build. A read resolves it into the answer and a
//! write is kept from storing it back, and the store holds only what the
//! operator wrote.
//!
//! **That is what makes upgrading free and reclaiming unnecessary.** A later
//! build improves the value and every instance reads the new one; a build that
//! stops supplying one stops supplying it, and the operator's half was never
//! touched. Nothing marks it, because nothing of it is there to mark.
//!
//! # …except where the store has to enforce it
//!
//! **A kind is the exception, and the reason is enforcement rather than
//! storage.** The store refuses a write that would drop a thing below its
//! kind's required keys, and it decides that by reading the declarations it
//! holds. A kind whose keys lived only in the binary is a floor the store
//! cannot stand on.
//!
//! So a kind is **materialized**: written into the store, marked with an
//! [`Origin`] of [`Shipped`](Origin::Shipped), and **reconciled rather than
//! upserted** — what the store holds as the software's and this build no longer
//! ships is taken back by [`reclaimed`]. **The mark is read, never listed**
//! (rule 106): nothing here holds the names of the rows to protect.
//!
//! # Reclaiming is not a delete verb
//!
//! Rule 60 says the served surface has no delete. This is not that rule's
//! subject: reclaiming happens at startup, no caller can reach it, and what it
//! removes is a row the software wrote about itself. Nothing the operator said
//! is reachable from here — [`reclaimed`] never names a row a caller owns, and
//! the store refuses to remove one even when handed its name.

use super::types::Origin;
use std::collections::BTreeMap;

use super::{Entity, EntityId, MemoryError};

/// **Which owned rows this build no longer ships**, out of what the store
/// holds.
///
/// The general half of the mechanism, and it is general because it knows
/// nothing about what the rows are: a registry hands over what it holds with
/// each row's origin, and the set of names this build ships. What comes back
/// is the names to reclaim.
///
/// **A caller's row is never in the answer**, whatever this build ships. That
/// is the composition property: the owned half and the operator's own half sit
/// in one table, and reconciling one leaves the other exactly as it was.
///
/// **A row this build ships is never in the answer either**, so a set that
/// grew is written by the declarations and a set that shrank is closed by
/// this.
pub fn reclaimed<'a, H, S>(held: H, ships: S) -> Vec<String>
where
    H: IntoIterator<Item = (&'a str, Origin)>,
    S: IntoIterator<Item = &'a str>,
{
    let ships: Vec<&str> = ships.into_iter().collect();
    held.into_iter()
        .filter(|(_, origin)| *origin == Origin::Shipped)
        .map(|(name, _)| name)
        .filter(|name| !ships.contains(name))
        .map(str::to_string)
        .collect()
}

/// **What a provision puts into the store's answers.**
///
/// Two shapes, and the difference is whether the store holds a row at all. A
/// capability names one; nothing here branches on which capability it is (rule
/// 106).
#[derive(Debug, Clone)]
pub enum Supplies {
    /// **Text at the human half of a row the store already holds.** Resolved
    /// on top of what the operator wrote, which narrows it.
    Prose(String),
    /// **A whole record the store holds nothing of** — the row itself and the
    /// keys it carries.
    ///
    /// It is how the software ships a THING rather than a paragraph. The
    /// record answers every read a stored one answers, so the code that uses
    /// it cannot tell which half supplied it — which is the whole point, and
    /// what keeps a capability from forking over where its data came from.
    Record {
        entity: Entity,
        fields: BTreeMap<String, String>,
    },
}

/// **A value this build supplies at an address in the store.**
///
/// The whole of what a capability declares in order to ship data. No table, no
/// column, no verb: a value, and where it goes.
///
/// **It is not stored.** The build carries it and a read resolves it in, which
/// is why nothing marks it and nothing reconciles it — a build that stops
/// supplying one stops supplying it, and what the operator wrote was never
/// touched.
#[derive(Debug, Clone)]
pub struct Provision {
    /// The address it answers at.
    pub at: EntityId,
    /// What it puts there.
    pub supplies: Supplies,
}

impl Provision {
    /// The prose this build ships for an entity the store already holds.
    pub fn prose(at: EntityId, value: impl Into<String>) -> Self {
        Provision {
            at,
            supplies: Supplies::Prose(value.into()),
        }
    }

    /// **A whole record this build ships**, addressed by its own handle.
    pub fn record(entity: Entity, fields: BTreeMap<String, String>) -> Self {
        Provision {
            at: entity.id.clone(),
            supplies: Supplies::Record { entity, fields },
        }
    }
}

/// **What this build supplies, and the two questions asked of it.**
///
/// Held by the layer that answers reads, handed in from the build. An empty set
/// is the ordinary case for every row in the store.
#[derive(Debug, Clone, Default)]
pub struct Provisions(Vec<Provision>);

impl Provisions {
    pub fn new(provisions: Vec<Provision>) -> Self {
        Provisions(provisions)
    }

    /// The prose the build supplies for this entity, if any.
    pub fn prose_for(&self, entity: &EntityId) -> Option<&str> {
        self.0.iter().find_map(|p| match &p.supplies {
            Supplies::Prose(value) if &p.at == entity => Some(value.trim()),
            _ => None,
        })
    }

    /// Every whole record this build supplies.
    pub fn records(&self) -> impl Iterator<Item = (&Entity, &BTreeMap<String, String>)> {
        self.0.iter().filter_map(|p| match &p.supplies {
            Supplies::Record { entity, fields } => Some((entity, fields)),
            Supplies::Prose(_) => None,
        })
    }

    /// The record the build supplies under this handle, if any.
    pub fn record_for(&self, entity: &EntityId) -> Option<(&Entity, &BTreeMap<String, String>)> {
        self.records().find(|(held, _)| &held.id == entity)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// **What the build supplies, then what the operator wrote.**
///
/// The build's half comes first because the second narrows the first: a reader
/// meeting the exception before the rule has to hold it in the air until the
/// rule arrives.
///
/// The divider is text rather than nothing, because two blocks of prose with a
/// blank line between them do not tell a reader which of the two a new build
/// could change under them.
pub fn extended(shipped: &str, own: &str) -> String {
    let own = own.trim();
    if own.is_empty() {
        return shipped.trim().to_string();
    }
    format!("{}\n\n---\n\n{}\n\n{own}", shipped.trim(), THE_LAYER_BELOW)
}

/// **What the second half is**, said in the answer rather than left to be
/// inferred.
const THE_LAYER_BELOW: &str = "**Above is what this build ships, and it moves when the software \
     does. Below is what this instance has written for itself: it narrows what \
     is above and never repeals it.**";

/// **May this text be stored at an address the build supplies?**
///
/// **The read-modify-write trap, closed underneath.** A caller reads a resolved
/// value, adds a line and sends the whole thing back. Storing it would write
/// the build's own words into the instance's half, where they stop moving when
/// the software does — a shipped default turned into a frozen customisation by
/// a caller doing the most ordinary thing there is.
///
/// **Refused, never trimmed** (rule 68). Cutting the text down to the half we
/// wanted would store something the caller did not write, and they would never
/// learn which half was kept. The caller does not know this mechanism exists,
/// so the refusal has to be actionable by somebody who has never heard of it:
/// it says the text repeats what the software already says, and to send only
/// what is being added.
pub fn guard_extension(shipped: &str, incoming: &str) -> Result<(), MemoryError> {
    if carries(incoming, shipped) {
        return Err(MemoryError::RepeatsShipped);
    }
    Ok(())
}

/// **Does this text carry that one**, whitespace aside.
///
/// Compared on the non-empty lines rather than byte for byte: a caller that
/// reflowed what it read, or re-indented it, has still sent the same words
/// back, and a byte comparison would wave that through.
fn carries(text: &str, shipped: &str) -> bool {
    let lines = |s: &str| -> Vec<String> {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    };
    let needle = lines(shipped);
    if needle.is_empty() {
        return false;
    }
    let hay = lines(text);
    hay.windows(needle.len().max(1))
        .any(|window| window == needle.as_slice())
        || needle.iter().all(|line| hay.contains(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The three answers a reconcile has to keep apart**, in one call: a row
    /// that left the build's set goes, a row still in it stays, and a row the
    /// caller wrote stays whatever the build ships.
    ///
    /// All three, because each covers how the others pass on a build that is
    /// wrong. Only the first would pass on a build that reclaims everything;
    /// only the second and third on a build that reclaims nothing.
    #[test]
    fn an_owned_row_that_left_the_set_is_reclaimed_and_no_other_row_is() {
        let held = vec![
            ("person", Origin::Shipped),
            ("zeta", Origin::Shipped),
            ("eta", Origin::Declared),
        ];
        assert_eq!(
            reclaimed(held, ["person"]),
            vec!["zeta".to_string()],
            "the row that left the build's set, and only it",
        );
    }

    /// **A caller's row is never reclaimed, even under a name the build once
    /// shipped.** The origin decides, and nothing else does: a caller who
    /// declares under a retired shipped name owns what they declared.
    #[test]
    fn a_callers_row_survives_a_name_the_build_dropped() {
        assert!(
            reclaimed(vec![("zeta", Origin::Declared)], ["person"]).is_empty(),
            "the column says whose the row is, and this one is not the build's",
        );
    }

    /// **A build that ships nothing reclaims every owned row and no other.**
    /// The edge the loop would get wrong by treating an empty set as "keep
    /// everything".
    #[test]
    fn a_build_that_ships_nothing_reclaims_only_what_it_owns() {
        let held = vec![("zeta", Origin::Shipped), ("eta", Origin::Declared)];
        assert_eq!(reclaimed(held, []), vec!["zeta".to_string()]);
    }
}
