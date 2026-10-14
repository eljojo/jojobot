//! **Where an address went when its thing was merged away.**
//!
//! A merge gives each claim of the folded thing an address under the survivor,
//! and the merge event keeps the map. A survivor can be merged in its turn, so
//! an address from before the first merge is followed through every event that
//! moved it. **That walk is one rule, and every store applies it through this
//! module**: the store supplies the events that moved an address from where the
//! walk stands, and the walk decides which of them counts, when to stop and what
//! the answer is. A store that walked on its own would drift from the others.

use super::FactId;

/// A merge cannot fold a thing into itself, so the chain of merges ends. The
/// bound is for a store that has been edited by hand.
pub const MOST_MERGES_FOLLOWED: usize = 64;

/// **One event that moved the claim the walk stands at**: the key it went to and
/// the id it was given there. `order` is the event's place among all merges, so
/// that the later of two events can be told from the earlier one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimMove<K> {
    pub order: u64,
    pub survivor: K,
    pub survivor_id: FactId,
}

/// **The walk itself, free of any store.** A caller asks the store for the
/// events that moved [`at`](Self::at), hands them to [`step`](Self::step), and
/// asks again while `step` says the address moved on. A synchronous store calls
/// [`follow_claim`] instead and does not write the loop.
#[derive(Debug, Clone)]
pub struct ClaimWalk<K> {
    at: (K, FactId),
    moved: bool,
    steps: usize,
}

impl<K> ClaimWalk<K> {
    pub fn new(home: K, local: FactId) -> Self {
        ClaimWalk {
            at: (home, local),
            moved: false,
            steps: 0,
        }
    }

    /// The address the walk stands at: the one to ask the store about.
    pub fn at(&self) -> (&K, &FactId) {
        (&self.at.0, &self.at.1)
    }

    /// **Take the events that moved the address the walk stands at.** The newest
    /// event wins: a thing is merged once, and a store that holds two events for
    /// one address holds the later merge's account of where it went. Returns
    /// whether the walk moved and should be asked again.
    pub fn step(&mut self, candidates: Vec<ClaimMove<K>>) -> bool {
        let Some(newest) = candidates.into_iter().max_by_key(|m| m.order) else {
            return false;
        };
        self.at = (newest.survivor, newest.survivor_id);
        self.moved = true;
        self.steps += 1;
        self.steps < MOST_MERGES_FOLLOWED
    }

    /// **Where the address ended up**, or `None` when no event moved it, which is
    /// every address on a thing nobody merged and every address on a thing
    /// merged before events were kept.
    pub fn finish(self) -> Option<(K, FactId)> {
        self.moved.then_some(self.at)
    }
}

/// [`ClaimWalk`] over a store that answers at once.
pub fn follow_claim<K, E>(
    home: K,
    local: FactId,
    mut moved_from: impl FnMut(&K, &FactId) -> Result<Vec<ClaimMove<K>>, E>,
) -> Result<Option<(K, FactId)>, E> {
    let mut walk = ClaimWalk::new(home, local);
    loop {
        let (home, local) = walk.at();
        let candidates = moved_from(home, local)?;
        if !walk.step(candidates) {
            return Ok(walk.finish());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Events as `(order, folded, folded_id, survivor, survivor_id)`.
    type Event = (u64, &'static str, &'static str, &'static str, &'static str);

    fn walk(events: &[Event], home: &'static str, local: &str) -> Option<(&'static str, String)> {
        follow_claim::<_, ()>(home, FactId(local.into()), |home, local| {
            Ok(events
                .iter()
                .filter(|(_, folded, was, _, _)| folded == home && *was == local.as_str())
                .map(|(order, _, _, survivor, now)| ClaimMove {
                    order: *order,
                    survivor: *survivor,
                    survivor_id: FactId((*now).into()),
                })
                .collect())
        })
        .expect("the events read")
        .map(|(home, local)| (home, local.0))
    }

    #[test]
    fn an_address_no_event_moved_does_not_move() {
        let events = [(1, "a", "f1", "b", "f3")];
        assert_eq!(walk(&events, "a", "f9"), None);
        assert_eq!(walk(&events, "b", "f3"), None);
    }

    #[test]
    fn an_address_is_followed_through_every_merge_that_moved_it() {
        let events = [(1, "a", "f1", "b", "f3"), (2, "b", "f3", "c", "f7")];
        assert_eq!(walk(&events, "a", "f1"), Some(("c", "f7".into())));
        assert_eq!(walk(&events, "b", "f3"), Some(("c", "f7".into())));
    }

    #[test]
    fn the_newest_event_for_an_address_decides_where_it_went() {
        let events = [(1, "a", "f1", "b", "f3"), (2, "a", "f1", "c", "f4")];
        assert_eq!(walk(&events, "a", "f1"), Some(("c", "f4".into())));
        let reversed = [events[1], events[0]];
        assert_eq!(
            walk(&reversed, "a", "f1"),
            Some(("c", "f4".into())),
            "the order the store lists them in does not decide it"
        );
    }

    #[test]
    fn a_loop_in_the_events_ends_at_the_bound() {
        let events = [(1, "a", "f1", "b", "f1"), (2, "b", "f1", "a", "f1")];
        let mut asked = 0;
        let end = follow_claim::<_, ()>("a", FactId("f1".into()), |home, local| {
            asked += 1;
            Ok(events
                .iter()
                .filter(|(_, folded, was, _, _)| folded == home && *was == local.as_str())
                .map(|(order, _, _, survivor, now)| ClaimMove {
                    order: *order,
                    survivor: *survivor,
                    survivor_id: FactId((*now).into()),
                })
                .collect())
        })
        .expect("the events read");
        assert_eq!(asked, MOST_MERGES_FOLLOWED);
        assert!(end.is_some(), "an address that moved is reported as moved");
    }
}
