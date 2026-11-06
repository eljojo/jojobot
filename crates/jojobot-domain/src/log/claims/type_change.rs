//! **The claims events one declaration of a type makes.** The Dolt adapter reads
//! the declaration the State holds, and the fake reads its own State; both hand
//! it here with the new declaration, so the choice of events is made in one
//! place and the log cannot hold two readings of one declaration.

use jiff::Timestamp;

use super::ClaimEvent;
use super::import::live;
use crate::log::{Event, ObjectId};
use crate::memory::types::DeclaredType;

/// The events of declaring `declared` when the State holds `held` for its name:
/// none when the State already holds exactly this declaration, so a seed run
/// over a log that holds the shipped types leaves the log as it was, and one
/// `TypeDeclared` otherwise.
pub fn type_declaration_events(
    held: Option<&DeclaredType>,
    declared: &DeclaredType,
    at: Timestamp,
    run: Option<String>,
) -> Vec<Event<ClaimEvent>> {
    if held == Some(declared) {
        return Vec::new();
    }
    vec![live(
        ClaimEvent::TypeDeclared {
            kind: ObjectId(declared.name.clone()),
            declaration: declared.clone(),
        },
        at,
        run,
    )]
}

/// **The events of withdrawing the declaration of `kind`**, when the rows hold
/// none: one `TypeWithdrawn` if the State holds a declaration for the kind, and
/// none if it holds nothing, so a repeat of a reclaim leaves the log as it was.
pub fn type_withdrawal_events(
    kind: &ObjectId,
    held: Option<&DeclaredType>,
    at: Timestamp,
    run: Option<String>,
) -> Vec<Event<ClaimEvent>> {
    if held.is_none() {
        return Vec::new();
    }
    vec![live(
        ClaimEvent::TypeWithdrawn { kind: kind.clone() },
        at,
        run,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::{Field, ValueType};

    /// A withdrawal is made only when the State holds a declaration, and it names
    /// the kind.
    #[test]
    fn a_withdrawal_is_made_only_when_the_state_holds_a_declaration() {
        let (kind, at) = (ObjectId("pledge".into()), Timestamp::UNIX_EPOCH);
        assert!(type_withdrawal_events(&kind, None, at, None).is_empty());
        let held = declared(&["witness"]);
        let events = type_withdrawal_events(&kind, Some(&held), at, None);
        let [event] = events.as_slice() else {
            panic!("a withdrawal makes one event: {events:?}");
        };
        assert_eq!(
            event.payload,
            ClaimEvent::TypeWithdrawn { kind: kind.clone() }
        );
    }

    fn declared(keys: &[&str]) -> DeclaredType {
        DeclaredType::new(
            "pledge",
            keys.iter()
                .map(|key| Field::new(key, ValueType::Text))
                .collect(),
        )
    }

    /// **A declaration the State holds adds no event, and any other adds one**
    /// naming the type and carrying the declaration whole.
    #[test]
    fn a_declaration_makes_an_event_only_when_it_differs_from_the_one_held() {
        let at = Timestamp::UNIX_EPOCH;
        let first = declared(&["witness"]);
        let changed = declared(&["witness", "date"]);
        assert_eq!(type_declaration_events(None, &first, at, None).len(), 1);
        assert!(type_declaration_events(Some(&first), &first, at, None).is_empty());
        let events = type_declaration_events(Some(&first), &changed, at, Some("run-a".into()));
        let [event] = events.as_slice() else {
            panic!("a changed declaration makes one event: {events:?}");
        };
        assert_eq!(event.run.as_ref().map(|run| run.0.as_str()), Some("run-a"));
        assert_eq!(
            event.payload,
            ClaimEvent::TypeDeclared {
                kind: ObjectId("pledge".into()),
                declaration: changed,
            }
        );
    }
}
