//! **The three windows prose states, written from their constants.**
//!
//! A model reads that a role is held for some minutes, that a stopped run is
//! offered back for some days and that a code is good for some minutes. Each is
//! a constant in the build, and a sentence that states the number by hand says
//! the old number the day the constant moves.
//!
//! So prose carries a token where the number goes, and [`fill`] puts the number
//! in: `{{lease}}`, `{{offer}}` and `{{code}}`. A description is filled when the
//! router is built, the essay when it is read, and a refusal where it is made.
//! The words around a token are the author's; the number is the constant's.

use jojobot_domain::session::{LEASE_FRESHNESS, LEASE_RENEWS_AFTER, OFFER_ABANDONED_WITHIN};

/// The token for how long a held role stays held without a write.
pub(crate) const LEASE: &str = "{{lease}}";
/// The token for how old a lease's last renewal must be before a write renews it again.
pub(crate) const RENEWAL: &str = "{{renewal}}";
/// The token for how long a stopped run is offered back.
pub(crate) const OFFER: &str = "{{offer}}";
/// The token for how long a code the operator was sent stays good.
pub(crate) const CODE: &str = "{{code}}";

/// `45 minutes`: the lease, whole minutes.
pub(crate) fn lease() -> String {
    counted(LEASE_FRESHNESS.as_secs() / 60, "minute")
}

/// `4 minutes 30 seconds`: how old the last renewal must be for a write to
/// renew the lease again.
pub(crate) fn renewal() -> String {
    let secs = LEASE_RENEWS_AFTER.as_secs();
    match (secs / 60, secs % 60) {
        (0, s) => counted(s, "second"),
        (m, 0) => counted(m, "minute"),
        (m, s) => format!("{} {}", counted(m, "minute"), counted(s, "second")),
    }
}

/// `7 days`: how long a stopped run is offered back.
pub(crate) fn offer() -> String {
    counted(OFFER_ABANDONED_WITHIN.as_hours() / 24, "day")
}

/// `10 minutes`: how long a code is good for.
pub(crate) fn code() -> String {
    counted(jojobot_domain::challenge::WINDOW.as_secs() / 60, "minute")
}

/// A count and its unit, plural unless the count is one.
fn counted(count: i64, unit: &str) -> String {
    match count {
        1 => format!("1 {unit}"),
        n => format!("{n} {unit}s"),
    }
}

/// **Prose with its tokens replaced by the constants' values.** Text with no
/// token comes back as it was.
pub(crate) fn fill(text: &str) -> String {
    text.replace(LEASE, &lease())
        .replace(RENEWAL, &renewal())
        .replace(OFFER, &offer())
        .replace(CODE, &code())
}

/// Whether any token is still in `text`: a token that reached a reader is a
/// number that was never filled.
#[cfg(test)]
pub(crate) fn unfilled(text: &str) -> bool {
    text.contains("{{lease}}")
        || text.contains("{{renewal}}")
        || text.contains("{{offer}}")
        || text.contains("{{code}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The values, pinned by literal: the constants are declared in the domain
    /// and what they say is stated here once, so a changed constant is a changed
    /// line in this case and a reader decides.
    #[test]
    fn the_windows_read_as_the_constants_say() {
        assert_eq!(lease(), "45 minutes");
        assert_eq!(renewal(), "4 minutes 30 seconds");
        assert_eq!(offer(), "7 days");
        assert_eq!(code(), "10 minutes");
    }

    #[test]
    fn fill_replaces_every_token_and_leaves_other_text_alone() {
        let text =
            "held {{lease}}, offered {{offer}}, good {{code}}, and {{lease}} again; {not a token}";
        assert_eq!(
            fill(text),
            "held 45 minutes, offered 7 days, good 10 minutes, and 45 minutes again; {not a token}",
        );
        assert!(!unfilled(&fill(text)));
        assert!(unfilled(text));
        assert_eq!(fill("no token here"), "no token here");
    }

    #[test]
    fn a_count_of_one_is_singular() {
        assert_eq!(counted(1, "minute"), "1 minute");
        assert_eq!(counted(0, "day"), "0 days");
    }
}
