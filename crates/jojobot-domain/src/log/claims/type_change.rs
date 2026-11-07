//! **The claims events one declaration of a type makes.** The Dolt adapter reads
//! the declaration the State holds, and the fake reads its own State; both hand
//! it here with the new declaration, so the choice of events is made in one
//! place and the log cannot hold two readings of one declaration.

use jiff::Timestamp;

use super::ClaimEvent;
use super::import::live;
use crate::log::{Event, ObjectId};
use crate::memory::types::{DeclaredType, Field, KindKeysPlan, Origin};

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

/// **The declaration a kind holds once a declaration of its keys has landed**, as the
/// store's rows hold it: the keys the plan writes, then the keys it keeps, each with
/// the origin that owns it. `held` is every key the kind holds before, with its
/// field; the plan's kept keys take their field from it. With no plan the kind is
/// as it was. A kind with no key holds no declaration, which is none here.
/// The type takes the origin of its first key, as the rows read back.
pub fn effective_kind_declaration(
    token: &str,
    plan: Option<&KindKeysPlan>,
    held: &[(Field, Origin)],
) -> Option<DeclaredType> {
    let rows: Vec<(Field, Origin)> = match plan {
        None => held.to_vec(),
        Some(plan) => plan
            .rows
            .iter()
            .filter_map(|row| match &row.written {
                Some(field) => Some((field.clone(), row.origin)),
                None => held.iter().find(|(field, _)| field.key == row.key).cloned(),
            })
            .collect(),
    };
    let origin = rows.first()?.1;
    Some(DeclaredType {
        name: token.to_string(),
        fields: rows.into_iter().map(|(field, _)| field).collect(),
        origin,
        must: None,
    })
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

    /// **A kind's declaration is the keys the plan leaves it, kept ones included**,
    /// in the plan's order and with the origin of the first.
    #[test]
    fn a_kind_declaration_holds_the_written_and_the_kept_keys() {
        use crate::memory::types::{HeldKindKey, PlannedKindKey, plan_kind_keys};

        let witness = Field::new("witness", ValueType::Text);
        let date = Field::new("date", ValueType::Text);
        let held = vec![(witness.clone(), Origin::Shipped)];
        let plan = plan_kind_keys(
            "pledge",
            Origin::Declared,
            std::slice::from_ref(&date),
            &[HeldKindKey {
                key: "witness".into(),
                origin: Origin::Shipped,
            }],
        )
        .expect("a caller may add a key");
        let _: &[PlannedKindKey] = &plan.rows;
        let effective =
            effective_kind_declaration("pledge", Some(&plan), &held).expect("the kind holds keys");
        let keys: Vec<&str> = effective.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["witness", "date"]);
        assert_eq!(effective.origin, Origin::Shipped);
        assert!(effective_kind_declaration("pledge", None, &[]).is_none());
        assert_eq!(
            effective_kind_declaration("pledge", None, &held).map(|d| d.fields),
            Some(vec![witness])
        );
    }

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
