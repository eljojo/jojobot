//! **Moving an entry of a list: where it goes, read against the order the list
//! reads now.**
//!
//! A list that rotates does not read in key order once its entries have taken
//! turns, so a move is placed against what a reader sees, and writes the three
//! fields that make the place hold ([`entries::move_entry`]). The server mints
//! all three; a caller sends only where the entry goes.

use std::collections::BTreeMap;

use super::*;
use jojobot_domain::memory::entries::{self, PlaceError, Placement};

impl Jojobot {
    /// **Whether this address is a record of a list.** A move of one is placed by
    /// [`Self::plan_entry_move`]; a move of a document's section is not.
    pub(crate) async fn is_on_a_list(&self, address: &FactAddress) -> bool {
        match self.served_address(address).await {
            Ok(served) => served.home.kind() == Some(EntityKind::LIST),
            Err(_) => false,
        }
    }

    /// **Plan the move of the entry at `address` to where `after` says**: `first`,
    /// `last`, or the address of the open entry it should follow. The answer is the
    /// record fields the move writes, or the refusal that names the way forward.
    pub(crate) async fn plan_entry_move(
        &self,
        address: &FactAddress,
        after: &str,
    ) -> Result<BTreeMap<String, String>, MemoryError> {
        let entry = self.served_address(address).await?;
        let placement = match entries::parse_placement(Some(after)) {
            // An address from before a merge is followed to where its entry is now.
            Ok(Placement::After(named)) => Placement::After(self.served_address(&named).await?),
            Ok(other) => other,
            Err(e) => return Err(refused(&entry.home, e)),
        };
        let facts = self.memory.recall(&entry.home).await?;
        let setting = entries::Setting::of(&self.memory.fields(&entry.home).await?);
        let moved = entries::move_entry(&facts, setting, &entry, &placement)
            .map_err(|e| refused(&entry.home, e))?;
        Ok([
            (document_order().to_string(), moved.order),
            (entries::HELD_TURN.to_string(), moved.held_turn.to_string()),
            (
                entries::HELD_SINCE.to_string(),
                moved.held_since.to_string(),
            ),
        ]
        .into())
    }
}

fn document_order() -> &'static str {
    jojobot_domain::memory::document::ORDER
}

/// **A move that cannot be honoured, in words that say what to send instead.**
fn refused(list: &EntityId, why: PlaceError) -> MemoryError {
    let way = match &why {
        PlaceError::NotAPlace(_) => format!(
            "`after` moves an entry of {list}: send `{}`, `{}`, or the address of the open \
             entry it should follow",
            entries::FIRST,
            entries::LAST
        ),
        PlaceError::NotAnEntry { entries, .. } => {
            if entries.is_empty() {
                format!("{list} has no open entries")
            } else {
                format!("Its open entries are {}", entries.join(", "))
            }
        }
        PlaceError::NotMovable { .. } => {
            "Only an open entry moves. Send `undone` on it to open it again, then move it"
                .to_string()
        }
        PlaceError::FollowsItself(_) => "Name another open entry, or `first` or `last`".to_string(),
        PlaceError::NotAPlaceToFollow { .. } => {
            "Name an open entry to follow, or `first` or `last`".to_string()
        }
        PlaceError::NoRoom(_) => "Move it to `first` or `last` instead".to_string(),
    };
    MemoryError::InvalidFact(format!("{why}. {way}"))
}
