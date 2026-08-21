//! **A row the binary owns** — what that means, and how a boot makes the
//! store hold exactly the set this build ships (rule 234).
//!
//! The software writes rows of its own: the kinds today, and whatever a later
//! build ships beside them. Those rows are not the operator's. A caller cannot
//! change one, the software changes them on every boot, and the set is
//! **reconciled rather than upserted** — a row an older build shipped and this
//! one does not is taken back.
//!
//! # Why an owned row has to be marked
//!
//! Reconciling has to tell *the operator wrote this* from *an older build
//! shipped this and the new one dropped it*. The two need opposite treatment
//! and in the store they are the same row. So the row carries the answer: an
//! [`Origin`] of [`Shipped`](Origin::Shipped) is the mark, and it is one column
//! rather than a version history, because the question a reconcile asks is
//! *whose row is this* and not *which build wrote it*.
//!
//! **The mark is read, never listed** (rule 106). Nothing here holds the names
//! of the rows to protect: the protection reads the column, so a row added to
//! the build is protected by being written, and a list nobody updated cannot
//! go stale.
//!
//! # Who the software writes as
//!
//! An owned row is written by **the build**, and the [`Origin`] on the row IS
//! that identity. There is no bot and no session behind it: these rows are the
//! software's vocabulary rather than claims about the operator's life, so
//! nothing about them is attributable to a caller.
//!
//! That is what makes "prevent unintended changes while forcing intended ones"
//! a check on the WRITER rather than a property of the row. The origin is an
//! argument only in-process code can supply — no served verb exposes it — so a
//! caller writes as a caller by construction, and the store refuses a caller's
//! declaration over an owned row by reading the column.
//!
//! # Reclaiming is not a delete verb
//!
//! Rule 60 says the served surface has no delete. This is not that rule's
//! subject: reclaiming happens at startup, no caller can reach it, and what it
//! removes is a row the software wrote about itself. Nothing the operator said
//! is reachable from here — [`reclaimed`] never names a row a caller owns, and
//! the store refuses to remove one even when handed its name.

use super::types::Origin;

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
