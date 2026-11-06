//! **The claims events one change of an entity's row makes.** The Dolt adapter
//! reads the row back inside its transaction and the fake reads its own record;
//! both hand the row here, so the choice of events is made in one place and the
//! log cannot hold two readings of one change.

use jiff::{Timestamp, civil::Date};

use super::ClaimEvent;
use super::import::{EntityRow, live, object_body};
use crate::log::Event;

/// What a writer did to the row, and so which events follow.
pub enum EntityChange<'a> {
    /// The row did not exist before: the object is created and its handle bound.
    Created,
    /// The row was rewritten under the same handle.
    Revised,
    /// The row now answers to a new handle: the old one is retired on `day`, the
    /// day the rename writes to the former-handle row, and the new one bound.
    Renamed { from: &'a str, day: Date },
}

/// The events of `row` as it stands after `change`, in the order they apply.
pub fn entity_change_events(
    row: &EntityRow,
    change: EntityChange<'_>,
    at: Timestamp,
    run: Option<String>,
) -> Vec<Event<ClaimEvent>> {
    let key = row.key.clone();
    let body = object_body(row);
    let event = |payload| live(payload, at, run.clone());
    let bound = || ClaimEvent::AddressBound {
        object: key.clone(),
        address: row.handle.clone(),
    };
    match change {
        EntityChange::Created => vec![
            event(ClaimEvent::ObjectCreated {
                object: key.clone(),
                body,
            }),
            event(bound()),
        ],
        EntityChange::Revised => vec![event(ClaimEvent::ObjectRevised { object: key, body })],
        EntityChange::Renamed { from, day } => vec![
            event(ClaimEvent::ObjectRevised {
                object: key.clone(),
                body,
            }),
            event(ClaimEvent::AddressRetired {
                object: key.clone(),
                address: from.to_string(),
                day,
            }),
            event(bound()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::ObjectId;

    fn row() -> EntityRow {
        EntityRow {
            key: ObjectId("k1".into()),
            handle: "person:homer".into(),
            kind: "thing".into(),
            name: "New Name".into(),
            source: "user-named".into(),
            boot: None,
            parent: None,
            aliases: Vec::new(),
            crm: None,
            prose: String::new(),
            badge: Some("k1".into()),
            archived: None,
            merged_into: None,
            taken_in: None,
        }
    }

    fn shapes(events: &[Event<ClaimEvent>]) -> Vec<&'static str> {
        events
            .iter()
            .map(|event| match &event.payload {
                ClaimEvent::ObjectCreated { .. } => "created",
                ClaimEvent::ObjectRevised { .. } => "revised",
                ClaimEvent::AddressBound { .. } => "bound",
                ClaimEvent::AddressRetired { .. } => "retired",
                _ => "other",
            })
            .collect()
    }

    /// **Each change makes its own events in the order they apply**, and a rename
    /// retires the old spelling on the day it was given.
    #[test]
    fn a_change_makes_its_events_and_a_rename_carries_its_day() {
        let at = Timestamp::UNIX_EPOCH;
        let made = |change| entity_change_events(&row(), change, at, None);
        assert_eq!(shapes(&made(EntityChange::Created)), ["created", "bound"]);
        assert_eq!(shapes(&made(EntityChange::Revised)), ["revised"]);
        let day = jiff::civil::date(2026, 3, 9);
        let renamed = made(EntityChange::Renamed {
            from: "person:marge",
            day,
        });
        assert_eq!(shapes(&renamed), ["revised", "retired", "bound"]);
        let ClaimEvent::AddressRetired {
            address, day: on, ..
        } = &renamed[1].payload
        else {
            panic!("the second event retires the old spelling");
        };
        assert_eq!((address.as_str(), *on), ("person:marge", day));
    }
}
