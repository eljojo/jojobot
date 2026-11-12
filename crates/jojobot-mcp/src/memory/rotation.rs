//! **What a capture receipt says when a key on a thing comes back to an earlier
//! person.** Two people taking turns at a chore is a list with tick back, one
//! entry per turn; a key rewritten each sitting cannot say whose go is next. One
//! change of person is ordinary, so the signal is a person handle written to a
//! key that held it in an earlier claim and does not hold it now (A, then B,
//! then A). A plain value never counts: a status word cycles the same way.
//! Nothing is stored and the line is the receipt's `hint` (rules 456, 498).

use jojobot_domain::memory::{Fact, FactStatus};

use super::*;

/// The sentence a capture receipt adds when a key has come back to an earlier person.
const ROTATION_LINE: &str = "This key has come back to an earlier person, which reads as \
    people taking turns. A list on this thing keeps turns: one entry per turn, ticked with \
    `after` to say whose go is next.";

impl Jojobot {
    /// The rotation sentence for a claim just written, or none. None when the claim sets
    /// no key to a person who held it before, or when the thing could not be read: the
    /// write landed, so a sentence that cannot be worked out is left off and logged.
    pub(crate) async fn rotation_line(&self, written: &Fact) -> Option<&'static str> {
        let held = self
            .memory
            .recall(&written.subject)
            .await
            .inspect_err(|error| {
                tracing::warn!(%error, subject = %written.subject, "a receipt could not read the claims of its thing");
            })
            .ok()?;
        written
            .sets
            .iter()
            .any(|key| comes_back(&held, written, key))
            .then_some(ROTATION_LINE)
    }
}

/// Whether `written` sets `key` to a person that an earlier claim on the same thing set it
/// to, while the newest earlier claim set it to someone else.
fn comes_back(held: &[Fact], written: &Fact, key: &str) -> bool {
    let Some(value) = written.fields.get(key) else {
        return false;
    };
    if EntityId(value.clone()).kind() != Some(EntityKind::PERSON) {
        return false;
    }
    let mut earlier: Vec<(u64, &str)> = held
        .iter()
        .filter(|claim| {
            claim.id != written.id
                && claim.subject == written.subject
                && claim.status == FactStatus::Active
                && claim.sets.contains(key)
        })
        .filter_map(|claim| Some((claim.id.ordinal()?, claim.fields.get(key)?.as_str())))
        .collect();
    earlier.sort_by_key(|(ordinal, _)| *ordinal);
    let Some((_, current)) = earlier.last() else {
        return false;
    };
    current != value && earlier.iter().any(|(_, said)| said == value)
}
