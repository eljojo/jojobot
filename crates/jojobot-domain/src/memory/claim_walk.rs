//! **Where an address went when its thing was merged away.**
//!
//! A merge gives each claim of the folded thing an address under the survivor,
//! and the merge event keeps the map. A survivor can be merged in its turn, so
//! an address from before the first merge is followed through every event that
//! moved it. **That walk is one rule, and every store applies it through this
//! module**: the store supplies the events that moved an address from where the
//! walk stands, and the walk decides which of them counts, when to stop and what
//! the answer is. A store that walked on its own would drift from the others.

use std::collections::HashSet;
use std::hash::Hash;

use super::FactId;

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
    passed: HashSet<(K, FactId)>,
    looped: bool,
}

impl<K: Clone + Eq + Hash> ClaimWalk<K> {
    pub fn new(home: K, local: FactId) -> Self {
        ClaimWalk {
            passed: HashSet::from([(home.clone(), local.clone())]),
            at: (home, local),
            moved: false,
            looped: false,
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
    ///
    /// **No count of merges ends the walk; only an address it has already stood
    /// at does.** A chain of merges has no length past which a claim is lost. A
    /// store edited by hand can hold events that lead back to where they began:
    /// the walk then stays at the last new address and says so through
    /// [`looped`](Self::looped), so a store that can refuse does.
    pub fn step(&mut self, candidates: Vec<ClaimMove<K>>) -> bool {
        let Some(newest) = candidates.into_iter().max_by_key(|m| m.order) else {
            return false;
        };
        let next = (newest.survivor, newest.survivor_id);
        if !self.passed.insert(next.clone()) {
            self.looped = true;
            return false;
        }
        self.at = next;
        self.moved = true;
        true
    }

    /// Whether the events led back to an address the walk had already stood at.
    pub fn looped(&self) -> bool {
        self.looped
    }

    /// **Where the address ended up**, or `None` when no event moved it, which is
    /// every address on a thing nobody merged and every address on a thing
    /// merged before events were kept.
    pub fn finish(self) -> Option<(K, FactId)> {
        self.moved.then_some(self.at)
    }
}

/// [`ClaimWalk`] over a store that answers at once.
pub fn follow_claim<K: Clone + Eq + Hash, E>(
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
    fn a_loop_in_the_events_ends_where_it_closes_and_says_so() {
        let events = [(1, "a", "f1", "b", "f1"), (2, "b", "f1", "a", "f1")];
        let mut walk = ClaimWalk::new("a", FactId("f1".into()));
        let mut asked = 0;
        loop {
            asked += 1;
            let (home, local) = walk.at();
            let candidates = events
                .iter()
                .filter(|(_, folded, was, _, _)| folded == home && *was == local.0)
                .map(|(order, _, _, survivor, now)| ClaimMove {
                    order: *order,
                    survivor: *survivor,
                    survivor_id: FactId((*now).into()),
                })
                .collect();
            if !walk.step(candidates) {
                break;
            }
        }
        assert!(walk.looped(), "the walk says the events led back");
        assert_eq!(asked, 2, "each address is asked once");
        assert_eq!(walk.finish(), Some(("b", FactId("f1".into()))));
    }

    #[test]
    fn a_chain_longer_than_any_fixed_bound_is_followed_to_its_end() {
        let names: Vec<String> = (0..=200).map(|n| format!("h{n}")).collect();
        let end = follow_claim::<_, ()>(names[0].clone(), FactId("f1".into()), |home, _| {
            let at = names.iter().position(|n| n == home).expect("a name");
            Ok(names
                .get(at + 1)
                .map(|next| ClaimMove {
                    order: at as u64,
                    survivor: next.clone(),
                    survivor_id: FactId("f1".into()),
                })
                .into_iter()
                .collect())
        })
        .expect("the events read");
        assert_eq!(end, Some((names[200].clone(), FactId("f1".into()))));
    }
}
