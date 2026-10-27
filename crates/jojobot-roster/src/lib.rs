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
pub const ORDINARY_FILE: &str = "crates/jojobot-roster/src/ordinary.rs";

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
