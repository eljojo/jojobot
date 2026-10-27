//! Fictional fixture names and their source classifier, shared by the local gate and CI.

mod cast;
mod ordinary;
mod scanner;

pub use cast::ROSTER;
pub use ordinary::{ORDINANCES, ORDINARY};
pub use scanner::{
    bare_slugs_in, handles_in, is_listed_slug, off_roster_handles_in, two_part_handles_in,
    unlisted_slugs_under_a_declared_kind_in, violations_in,
};

pub const ROSTER_FILE: &str = "crates/jojobot-roster/src/cast.rs";
pub const LEGACY_ROSTER_FILE: &str = "crates/jojobot-domain/tests/fixture_roster.rs";
pub const ORDINARY_FILE: &str = "crates/jojobot-roster/src/ordinary.rs";

/// Messages written before the handle gate existed are outside its audit.
pub const COMMIT_HANDLE_CHECK_BOUNDARY: &str = "99681a26";

/// A domain test holds this copy equal to EntityKind::ALL.
pub const KINDS: &[&str] = &[
    "person",
    "project",
    "place",
    "event",
    "work",
    "thing",
    "org",
    "topic",
    "bot",
    "pet",
    "rhythm",
    "promise",
    "machine",
    "view",
    "session",
    "thread",
    "decision",
    "role",
    "voucher",
    "challenge",
    "document",
    "list",
    "usage",
    "trip",
];

/// Every explicitly listed handle, for the CI audits of the catalog.
pub fn listed() -> impl Iterator<Item = &'static str> {
    ROSTER.iter().copied().chain(ordinary())
}

/// Ordinary exceptions name nobody and remain subject to the unused-entry audit.
pub fn ordinary() -> Vec<&'static str> {
    ORDINARY.iter().chain(ORDINANCES).copied().collect()
}

pub fn is_listed(handle: &str) -> bool {
    listed().any(|entry| entry == handle)
}

/// The same refusal appears in the local gate and the CI source check.
pub fn refusal(violations: &[String]) -> String {
    format!(
        "handles outside the fictional roster — if the name is openly fictional, \
         add it to ROSTER (a name from the shows) or ORDINARY (a handle that names nobody) in a conscious diff; if it names anything real from the \
         operator's life, it must not enter this repo at all:\n{}",
        violations.join("\n")
    )
}

/// The constructor refusal is shared with the CI gate.
pub fn bare_refusal(bare: &[String]) -> String {
    format!(
        "these hand a bare slug to a handle constructor, so the name never appears as \
         `kind:slug` and the roster gate above has never seen it — write the handle \
         whole:\n{}",
        bare.join("\n")
    )
}

/// Declared kinds still require a slug from the same catalog.
pub fn declared_refusal(refused: &[String]) -> String {
    format!(
        "these hand a slug that is on neither ROSTER nor ORDINARY to a handle constructor \
         under a kind the call does not write out. If the name is openly fictional, add a \
         handle with it to ROSTER (a name from the shows) or ORDINARY (a handle that names \
         nobody) in a conscious diff; otherwise use a slug that is already there:\n{}",
        refused.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_has_a_boundary_and_an_unlisted_one_is_refused() {
        let unlisted = ["person", ":", "not-on-the-roster"].concat();
        assert_eq!(handles_in(&unlisted), vec![unlisted.clone()]);
        assert!(handles_in(&format!("url{unlisted}")).is_empty());
        let commits = vec![("abc".into(), format!("{unlisted} person:homer"))];
        assert_eq!(
            off_roster_handles_in(&commits),
            vec![format!("abc: {unlisted}")]
        );
    }
}
