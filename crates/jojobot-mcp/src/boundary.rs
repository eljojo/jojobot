//! Where an adapter's words stop and a caller's words begin: jojobot's
//! storage is never an agent's business. The adapter's text is logged,
//! never returned; what crosses is written for the caller only.

/// A store failure. `detail` is logged, never returned; the sentence is
/// the same for every failure, because the caller's next move never changes.
pub(crate) fn store_failed(verb: &str, detail: &str) -> String {
    tracing::error!(verb, detail, "store failure reached the boundary");
    format!(
        "{verb} could not be completed — jojobot's own storage failed, which is not something \
         your call did wrong and not something you can fix by calling differently. Nothing was \
         written that you should rely on. Try once more; if it fails again, tell the operator, \
         because it needs a person."
    )
}

/// A record jojobot cannot read, told the same way: logged, not returned.
pub(crate) fn unreadable(what: &str, detail: &str) -> String {
    tracing::error!(what, detail, "unreadable record reached the boundary");
    format!(
        "{what} exists but cannot be read as a record. Something about how it is stored was \
         changed by hand, and only a person can put it back."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What crosses must not contain the adapter's own account.
    #[test]
    fn nothing_crossing_the_boundary_names_the_store() {
        let leaky = "the page for gamma has no table, and the row vanished from the document";
        for said in [
            store_failed("post_message", leaky),
            unreadable("message gamma-4", leaky),
        ] {
            assert!(
                !said.contains(leaky),
                "the adapter's own words crossed: {said}"
            );
        }
    }

    /// **And it still tells the caller what to do**, which is the half a
    /// scrubber loses: stripping the store's words out of the store's sentence
    /// leaves a sentence about nothing.
    #[test]
    fn what_crosses_is_still_an_answer() {
        let said = store_failed("post_message", "the page vanished");
        assert!(said.contains("post_message"), "{said}");
        assert!(
            said.contains("Try once more"),
            "a caller needs its next move: {said}"
        );
        assert!(
            said.contains("tell the operator"),
            "…and the way out when it repeats: {said}"
        );

        let unread = unreadable("message gamma-4", "the state cell is not a state");
        assert!(unread.contains("message gamma-4"), "{unread}");
        assert!(
            unread.contains("only a person"),
            "the caller must learn a retry is pointless: {unread}"
        );
    }
}
