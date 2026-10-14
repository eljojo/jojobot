//! **Where something went when its thing was merged away.**
//!
//! A merge gives each claim of the folded thing an address under the survivor,
//! and the merge event keeps the map. A survivor can be merged in its turn, so
//! an address from before the first merge is followed through every event that
//! moved it. **That walk is one rule, and every store applies it through this
//! module**: the store supplies the events that moved an address from where the
//! walk stands, and the walk decides which of them counts, when to stop and what
//! the answer is. A store that walked on its own would drift from the others.
//!
//! **Two questions, two walks.** [`ClaimWalk`] answers where a CLAIM ADDRESS went.
//! [`ThingWalk`] answers where a THING went: the survivor a handle, a pointer, a
//! parent or a manager ended at. [`FoldedInto`] asks the other way, for every
//! thing that was folded into one. No count of merges ends any of them; only a
//! repeat does, and a repeat is a [`MergeLoop`] every caller turns into the same
//! error.

use std::collections::HashSet;
use std::hash::Hash;

use super::FactId;

/// **Merge events that lead back to where they began.** A merge cannot fold a
/// thing into itself, so none made here closes a loop; a store edited by hand
/// can. A walk that meets one stops and says so, because ending at any address on
/// the loop would serve a claim, a pointer or a manager at a place nobody put it.
/// Every store turns this one value into its one error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the merge events in the store lead back to where they began")]
pub struct MergeLoop;

impl From<MergeLoop> for super::MemoryError {
    fn from(_: MergeLoop) -> Self {
        super::MemoryError::Store(MergeLoop.to_string())
    }
}

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
    /// the walk then stops and [`finish`](Self::finish) answers [`MergeLoop`].
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

    /// **Where the address ended up**, or `None` when no event moved it, which is
    /// every address on a thing nobody merged and every address on a thing
    /// merged before events were kept. [`MergeLoop`] when the events led back to
    /// an address the walk had already stood at.
    pub fn finish(self) -> Result<Option<(K, FactId)>, MergeLoop> {
        if self.looped {
            return Err(MergeLoop);
        }
        Ok(self.moved.then_some(self.at))
    }
}

/// [`ClaimWalk`] over a store that answers at once.
pub fn follow_claim<K: Clone + Eq + Hash, E: From<MergeLoop>>(
    home: K,
    local: FactId,
    mut moved_from: impl FnMut(&K, &FactId) -> Result<Vec<ClaimMove<K>>, E>,
) -> Result<Option<(K, FactId)>, E> {
    let mut walk = ClaimWalk::new(home, local);
    loop {
        let (home, local) = walk.at();
        let candidates = moved_from(home, local)?;
        if !walk.step(candidates) {
            return Ok(walk.finish()?);
        }
    }
}

/// **The walk of a THING to the survivor it ended at**, free of any store. A
/// handle, a pointer, a parent or a manager that names a folded thing is served
/// as the survivor, through a survivor that was folded in its turn. The caller
/// asks the store for the survivor of [`at`](Self::at) and hands it to
/// [`step`](Self::step).
#[derive(Debug, Clone)]
pub struct ThingWalk<K> {
    at: K,
    passed: HashSet<K>,
    looped: bool,
}

impl<K: Clone + Eq + Hash> ThingWalk<K> {
    pub fn new(start: K) -> Self {
        ThingWalk {
            passed: HashSet::from([start.clone()]),
            at: start,
            looped: false,
        }
    }

    /// The thing the walk stands at: the one to ask the store about.
    pub fn at(&self) -> &K {
        &self.at
    }

    /// **Take the survivor of the thing the walk stands at**, `None` when nothing
    /// folded it. Returns whether the walk moved and should be asked again.
    /// Only a thing it has already stood at ends it early, as a [`MergeLoop`].
    pub fn step(&mut self, survivor: Option<K>) -> bool {
        let Some(next) = survivor else {
            return false;
        };
        if !self.passed.insert(next.clone()) {
            self.looped = true;
            return false;
        }
        self.at = next;
        true
    }

    /// The thing the walk ended at: the start when nothing folded it.
    /// [`MergeLoop`] when the events led back to a thing already passed.
    pub fn finish(self) -> Result<K, MergeLoop> {
        if self.looped {
            return Err(MergeLoop);
        }
        Ok(self.at)
    }
}

/// [`ThingWalk`] over a store that answers at once.
pub fn follow_thing<K: Clone + Eq + Hash, E: From<MergeLoop>>(
    start: K,
    mut survivor_of: impl FnMut(&K) -> Result<Option<K>, E>,
) -> Result<K, E> {
    let mut walk = ThingWalk::new(start);
    loop {
        let survivor = survivor_of(walk.at())?;
        if !walk.step(survivor) {
            return Ok(walk.finish()?);
        }
    }
}

/// **Every thing folded into one thing**, directly or through things folded into
/// those, found by asking the store what was folded into each in turn. Each thing
/// counts once and the start never counts, so a loop in the events ends at the
/// repeat. The caller asks for [`pending`](Self::pending) things until none is
/// left, and hands what it found for each to [`reach`](Self::reach).
#[derive(Debug, Clone)]
pub struct FoldedInto<K> {
    start: K,
    reached: HashSet<K>,
    found: Vec<K>,
    pending: Vec<K>,
}

impl<K: Clone + Eq + Hash> FoldedInto<K> {
    pub fn new(start: K) -> Self {
        FoldedInto {
            reached: HashSet::from([start.clone()]),
            pending: vec![start.clone()],
            found: Vec::new(),
            start,
        }
    }

    /// The next thing to ask the store about, or `None` when every one was asked.
    pub fn pending(&mut self) -> Option<K> {
        self.pending.pop()
    }

    /// **Take what was folded into the thing just asked about**, and answer which
    /// of it the walk had not reached before. A thing already reached, and the
    /// start, are left out.
    pub fn reach(&mut self, folded: impl IntoIterator<Item = K>) -> Vec<K> {
        let mut new = Vec::new();
        for key in folded {
            if key != self.start && self.reached.insert(key.clone()) {
                self.found.push(key.clone());
                self.pending.push(key.clone());
                new.push(key);
            }
        }
        new
    }

    /// Everything reached, in the order it was found.
    pub fn finish(self) -> Vec<K> {
        self.found
    }
}

/// [`FoldedInto`] over a store that answers at once.
pub fn folded_into_walk<K: Clone + Eq + Hash>(
    survivor: K,
    mut folded_into: impl FnMut(&K) -> Vec<K>,
) -> Vec<K> {
    let mut walk = FoldedInto::new(survivor);
    while let Some(at) = walk.pending() {
        let folded = folded_into(&at);
        walk.reach(folded);
    }
    walk.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Events as `(order, folded, folded_id, survivor, survivor_id)`.
    type Event = (u64, &'static str, &'static str, &'static str, &'static str);

    fn moves<'a>(
        events: &'a [Event],
        home: &'a &'static str,
        local: &'a FactId,
    ) -> Vec<ClaimMove<&'static str>> {
        events
            .iter()
            .filter(|(_, folded, was, _, _)| folded == home && *was == local.as_str())
            .map(|(order, _, _, survivor, now)| ClaimMove {
                order: *order,
                survivor: *survivor,
                survivor_id: FactId((*now).into()),
            })
            .collect()
    }

    fn walk(events: &[Event], home: &'static str, local: &str) -> Option<(&'static str, String)> {
        follow_claim::<_, MergeLoop>(home, FactId(local.into()), |home, local| {
            Ok(moves(events, home, local))
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
    fn a_loop_in_the_events_is_a_merge_loop_after_each_address_is_asked_once() {
        let events = [(1, "a", "f1", "b", "f1"), (2, "b", "f1", "a", "f1")];
        let mut asked = 0;
        let end = follow_claim::<_, MergeLoop>("a", FactId("f1".into()), |home, local| {
            asked += 1;
            Ok(moves(&events, home, local))
        });
        assert_eq!(end, Err(MergeLoop), "the loop is reported, not answered");
        assert_eq!(asked, 2, "each address is asked once");
    }

    #[test]
    fn a_chain_longer_than_any_fixed_bound_is_followed_to_its_end() {
        let names: Vec<String> = (0..=200).map(|n| format!("h{n}")).collect();
        let end = follow_claim::<_, MergeLoop>(names[0].clone(), FactId("f1".into()), |home, _| {
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

    fn survivors(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&&'static str) -> Result<Option<&'static str>, MergeLoop> {
        move |thing| {
            Ok(pairs
                .iter()
                .find(|(folded, _)| folded == thing)
                .map(|(_, survivor)| *survivor))
        }
    }

    #[test]
    fn a_thing_nobody_folded_ends_where_it_started() {
        assert_eq!(follow_thing("a", survivors(&[("x", "y")])), Ok("a"));
    }

    #[test]
    fn a_thing_is_followed_through_every_survivor_that_was_folded_in_its_turn() {
        let chain: &'static [(&str, &str)] = &[("a", "b"), ("b", "c"), ("c", "d")];
        assert_eq!(follow_thing("a", survivors(chain)), Ok("d"));
        assert_eq!(follow_thing("c", survivors(chain)), Ok("d"));
    }

    #[test]
    fn a_chain_of_things_longer_than_any_fixed_bound_is_followed_to_its_end() {
        let names: Vec<String> = (0..=200).map(|n| format!("h{n}")).collect();
        let end = follow_thing::<_, MergeLoop>(names[0].clone(), |thing| {
            let at = names.iter().position(|n| n == thing).expect("a name");
            Ok(names.get(at + 1).cloned())
        });
        assert_eq!(end, Ok(names[200].clone()));
    }

    #[test]
    fn things_that_lead_back_to_where_they_began_are_a_merge_loop() {
        let looped: &'static [(&str, &str)] = &[("a", "b"), ("b", "c"), ("c", "a")];
        assert_eq!(follow_thing("a", survivors(looped)), Err(MergeLoop));
        let into_itself: &'static [(&str, &str)] = &[("a", "a")];
        assert_eq!(follow_thing("a", survivors(into_itself)), Err(MergeLoop));
    }

    fn folded_in(
        pairs: &'static [(&'static str, &'static str)],
        survivor: &'static str,
    ) -> Vec<&'static str> {
        folded_into_walk(survivor, |at| {
            pairs
                .iter()
                .filter(|(_, into)| into == at)
                .map(|(folded, _)| *folded)
                .collect()
        })
    }

    #[test]
    fn everything_folded_into_a_thing_is_found_through_what_was_folded_into_those() {
        let pairs: &'static [(&str, &str)] = &[("b", "a"), ("c", "b"), ("d", "b"), ("x", "y")];
        let mut found = folded_in(pairs, "a");
        found.sort();
        assert_eq!(found, ["b", "c", "d"]);
        assert!(
            folded_in(pairs, "c").is_empty(),
            "nothing was folded into c"
        );
    }

    #[test]
    fn a_loop_among_the_folded_things_ends_and_never_counts_the_start() {
        let pairs: &'static [(&str, &str)] = &[("b", "a"), ("a", "b")];
        assert_eq!(folded_in(pairs, "a"), ["b"]);
    }
}
