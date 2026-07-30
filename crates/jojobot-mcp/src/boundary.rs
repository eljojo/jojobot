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

/// A write that failed and could not be undone: part of it may remain, so
/// retrying is not a safe next move. A genuinely different class from
/// [`store_failed`] — that one is a clean failure, where nothing was written
/// and a retry is the reasonable answer — so it earns its own function
/// rather than a branch inside that one.
pub(crate) fn stranded(verb: &str, detail: &str) -> String {
    tracing::error!(verb, detail, "stranded write reached the boundary");
    format!(
        "{verb} may not have finished, and jojobot could not undo the part that did. Do not \
         try again — a repeat could double whatever landed. Tell the operator: this needs a \
         person to look at what is actually there."
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
            stranded("post_message", leaky),
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

        // **A stranded write is not told to retry** — the reason it is a
        // separate function rather than a branch of `store_failed`, whose
        // sentence says the opposite.
        let stranded_said = stranded("post_message", "the page vanished");
        assert!(stranded_said.contains("post_message"), "{stranded_said}");
        assert!(
            !stranded_said.contains("Try once more"),
            "a repeat could double whatever landed: {stranded_said}"
        );
        assert!(
            stranded_said.contains("Do not try again"),
            "…and it must say so plainly: {stranded_said}"
        );
        assert!(
            stranded_said.contains("Tell the operator"),
            "the caller must learn only a person can look: {stranded_said}"
        );
    }
}
