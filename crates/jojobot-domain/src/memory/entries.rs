//! **The entries of a list: claims on it, in an order the server minted.**
//!
//! An entry is one claim on a thing of kind `list`: a few words, or a claim
//! whose edge points at a thing. It carries an order key in its own field
//! `order`, spelled as a document section's is, and the keys sort as text
//! ([`super::order`]). A caller never picks a key: it says where the entry goes
//! (the end, the front, or after a named entry) and [`bounds`] names the two keys
//! the new one falls between.
//!
//! **Which entries are open is computed from what is on the list**, never
//! stored on an entry: an entry that is archived is not open. The state each
//! open entry reads as is [`OPEN`]; later slices compute others from tick
//! records.

use super::{EntityKind, document};
use super::{Fact, FactAddress, FactStatus};

/// The word an entry reads as while nothing has acted on it.
pub const OPEN: &str = "open";

/// The word that places an entry at the front.
pub const FIRST: &str = "first";

/// **Where a new entry goes.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// After every entry there is. The default.
    End,
    /// Before every entry there is.
    First,
    /// Straight after the named entry.
    After(FactAddress),
}

/// Why a placement cannot be honoured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlaceError {
    /// `after` is neither `first` nor an address.
    #[error("'{0}' is neither the word `first` nor the address of an entry")]
    NotAPlace(String),
    /// `after` names an address that is not an open entry of this list.
    #[error("'{named}' is not an open entry of this list")]
    NotAnEntry {
        /// What the caller named.
        named: String,
        /// The addresses of the entries there are, in order.
        entries: Vec<String>,
    },
    /// No key can be minted there.
    #[error("no room is left to place the entry there: {0}")]
    NoRoom(String),
}

/// **Whether the records on a thing of this kind are placed by an order key the
/// server mints**: a document's sections and a list's entries. The one place
/// that says so, so a gate that treats the order key as layout, and not as words
/// somebody said, asks here.
pub fn places_its_records(kind: Option<EntityKind>) -> bool {
    matches!(kind, Some(EntityKind::DOCUMENT | EntityKind::LIST))
}

/// **Whether a fact is an entry of the list it sits on**: a claim on a list that
/// carries an order key. An ordinary claim about a thing is not one.
pub fn is_entry(fact: &Fact) -> bool {
    fact.home.kind() == Some(EntityKind::LIST) && fact.fields.contains_key(document::ORDER)
}

/// **The open entries of a list, in order.** A claim on the list that is
/// active and carries an order key. Ordered by that key, then by the claim's id,
/// so two entries that ever carried one key still read in a fixed order.
pub fn open_entries(facts: &[Fact]) -> Vec<&Fact> {
    let mut entries: Vec<&Fact> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter(|fact| fact.home.kind() == Some(EntityKind::LIST))
        .filter(|fact| fact.fields.contains_key(document::ORDER))
        .collect();
    entries.sort_by(|a, b| {
        (&a.fields[document::ORDER], &a.id.0).cmp(&(&b.fields[document::ORDER], &b.id.0))
    });
    entries
}

/// **The placement a caller asked for**: nothing is the end, `first` the front,
/// anything else an entry's address.
pub fn parse_placement(after: Option<&str>) -> Result<Placement, PlaceError> {
    let Some(named) = after.map(str::trim).filter(|named| !named.is_empty()) else {
        return Ok(Placement::End);
    };
    if named == FIRST {
        return Ok(Placement::First);
    }
    FactAddress::parse(named)
        .map(Placement::After)
        .map_err(|_| PlaceError::NotAPlace(named.to_string()))
}

/// **The keys a new entry falls between**, as `(lower, upper)`, from the entries
/// the list has now. `None` is the start or the end of the order.
pub fn bounds(
    entries: &[&Fact],
    placement: &Placement,
) -> Result<(Option<String>, Option<String>), PlaceError> {
    let order = |fact: &Fact| fact.fields[document::ORDER].clone();
    match placement {
        Placement::End => Ok((entries.last().map(|fact| order(fact)), None)),
        Placement::First => Ok((None, entries.first().map(|fact| order(fact)))),
        Placement::After(address) => {
            let Some(at) = entries
                .iter()
                .position(|fact| fact.home == address.home && fact.id == address.local)
            else {
                return Err(PlaceError::NotAnEntry {
                    named: format!("{}#{}", address.home.as_str(), address.local.0),
                    entries: entries
                        .iter()
                        .map(|fact| fact.address().to_string())
                        .collect(),
                });
            };
            Ok((
                Some(order(entries[at])),
                entries.get(at + 1).map(|fact| order(fact)),
            ))
        }
    }
}

/// **The key for a new entry placed there.**
pub fn mint(entries: &[&Fact], placement: &Placement) -> Result<String, PlaceError> {
    let (lower, upper) = bounds(entries, placement)?;
    super::order::between(lower.as_deref(), upper.as_deref())
        .map_err(|e| PlaceError::NoRoom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{EntityId, FactId};

    fn entry(local: &str, order: &str, status: FactStatus) -> Fact {
        // A handle's kind is read from the loaded set, which another case in
        // this process may have unloaded.
        crate::memory::kinds::load_shipped();
        let home = EntityId("list:packing".into());
        Fact {
            id: FactId(local.into()),
            home: home.clone(),
            subject: home,
            content: format!("entry {local}"),
            details: None,
            provenance: crate::memory::Provenance::Testimony,
            standing: crate::memory::Standing::Open,
            status,
            recorded_at: jiff::civil::date(2026, 10, 16),
            happened_at: None,
            happened_through: None,
            inserted_at: None,
            stale_after: None,
            receipt: None,
            edge: None,
            fields: [(document::ORDER.to_string(), order.to_string())]
                .into_iter()
                .collect(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            sets: Default::default(),
        }
    }

    /// **Open entries read in key order, whatever order they were written in,
    /// and an archived entry or a claim with no key is not one.**
    #[test]
    fn open_entries_read_in_key_order_and_leave_out_the_archived_and_the_keyless() {
        let mut keyless = entry("f4", "5", FactStatus::Active);
        keyless.fields.remove(document::ORDER);
        let facts = vec![
            entry("f1", "8", FactStatus::Active),
            entry("f2", "4", FactStatus::Active),
            entry("f3", "6", FactStatus::Archived),
            keyless,
        ];
        let ids: Vec<&str> = open_entries(&facts)
            .iter()
            .map(|f| f.id.0.as_str())
            .collect();
        assert_eq!(ids, ["f2", "f1"]);
    }

    /// **Each placement lands between the keys it should, and a key minted there
    /// sorts there.** The end follows the last entry, the front precedes the first
    /// and `after` sits between an entry and the one that follows it.
    #[test]
    fn a_placement_mints_a_key_between_its_neighbours() {
        let facts = vec![
            entry("f1", "4", FactStatus::Active),
            entry("f2", "8", FactStatus::Active),
        ];
        let open = open_entries(&facts);
        let end = mint(&open, &Placement::End).unwrap();
        assert!(end.as_str() > "8", "{end}");
        let first = mint(&open, &Placement::First).unwrap();
        assert!(first.as_str() < "4", "{first}");
        let after_first = mint(
            &open,
            &Placement::After(FactAddress::new(
                EntityId("list:packing".into()),
                FactId("f1".into()),
            )),
        )
        .unwrap();
        assert!(
            after_first.as_str() > "4" && after_first.as_str() < "8",
            "{after_first}"
        );
        let after_last = mint(
            &open,
            &Placement::After(FactAddress::new(
                EntityId("list:packing".into()),
                FactId("f2".into()),
            )),
        )
        .unwrap();
        assert!(after_last.as_str() > "8", "{after_last}");
        assert_eq!(
            mint(&[], &Placement::End).unwrap(),
            order_of_an_empty_list()
        );
    }

    fn order_of_an_empty_list() -> String {
        crate::memory::order::between(None, None).unwrap()
    }

    /// **`after` must name an open entry of this list, and the refusal lists the
    /// entries there are.**
    #[test]
    fn after_naming_something_else_is_refused_with_the_entries() {
        let facts = vec![entry("f1", "4", FactStatus::Active)];
        let open = open_entries(&facts);
        let miss = Placement::After(FactAddress::new(
            EntityId("list:packing".into()),
            FactId("f9".into()),
        ));
        match bounds(&open, &miss) {
            Err(PlaceError::NotAnEntry { entries, .. }) => {
                assert_eq!(entries, ["list:packing#f1"]);
            }
            other => panic!("expected a refusal naming the entries: {other:?}"),
        }
        assert_eq!(parse_placement(None), Ok(Placement::End));
        assert_eq!(parse_placement(Some("first")), Ok(Placement::First));
        assert!(matches!(
            parse_placement(Some("nonsense")),
            Err(PlaceError::NotAPlace(_))
        ));
    }

    /// **Another session changing only an entry's order key is no rewrite of what
    /// was said**, exactly as for a document's section: the key says where the
    /// entry sits and never what it says. The same patch on a claim on anything
    /// else is a rewrite of testimony and is refused, which is what makes the
    /// first answer mean something.
    #[test]
    fn a_change_of_only_the_order_key_of_an_entry_is_not_a_rewrite_of_testimony() {
        let patch = || crate::memory::FactPatch {
            fields: [(document::ORDER.to_string(), "9".to_string())]
                .into_iter()
                .collect(),
            session: Some("another".to_string()),
            ..Default::default()
        };
        let on_a_list = entry("f1", "4", FactStatus::Active);
        let settled = crate::memory::settle_rewrite(
            &on_a_list.address(),
            &on_a_list,
            patch(),
            Some("writer"),
            &[],
        );
        assert!(settled.is_ok(), "{settled:?}");

        let mut elsewhere = entry("f1", "4", FactStatus::Active);
        elsewhere.home = EntityId("thing:gamma".into());
        elsewhere.subject = elsewhere.home.clone();
        let refused = crate::memory::settle_rewrite(
            &elsewhere.address(),
            &elsewhere,
            patch(),
            Some("writer"),
            &[],
        );
        assert!(
            matches!(
                refused,
                Err(crate::memory::MemoryError::TestimonyRewritten { .. })
            ),
            "{refused:?}"
        );
    }
}
