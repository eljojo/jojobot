//! **The one place an adapter's words stop and a caller's begin.**
//!
//! An adapter is allowed to know it writes to pages and tables — that is its
//! whole job. A caller is not: jojobot's storage is never an agent's business,
//! and an agent that learns the shape of it will act on that shape.
//!
//! The layer between them is here, and it had a hole. Every store-class
//! failure on the mail and session rails carried the adapter's own sentence
//! straight through into a protocol error body, a `reason` field, or — worst —
//! the note a boot splices into the first thing a session ever reads. Six
//! reachable paths, one cause: nothing scrubbed, because nothing was asked to.
//!
//! # Why this replaces rather than rewrites
//!
//! The obvious fix is a list of words to strip. That is the sweep the cut
//! removed, rebuilt by the side door, and it fails the same way: it only ever
//! catches the phrasings somebody thought of, and the seventh sentence is
//! written next week by someone who has not read the list.
//!
//! So the adapter's text is not edited for the caller — it is **not sent to
//! the caller at all**. It goes to the log, where an operator debugging a real
//! failure wants it and where no agent reads it. What crosses the boundary is
//! a sentence written for the reader on the other side, which cannot leak
//! because it contains nothing to leak.

/// **A store failure, told to the caller and to the log separately.**
///
/// `detail` is the adapter's own account — logged, never returned. `verb` says
/// what the caller was trying to do, so the sentence they get is about their
/// call rather than about jojobot's insides.
///
/// The sentence is deliberately identical for every store failure. A caller's
/// options do not vary with which page could not be found: nothing was
/// written that they can rely on, retrying may work, and if it keeps happening
/// a person has to look. Anything more specific would be describing the store.
pub(crate) fn store_failed(verb: &str, detail: &str) -> String {
    tracing::error!(verb, detail, "store failure reached the boundary");
    format!(
        "{verb} could not be completed — jojobot's own storage failed, which is not something \
         your call did wrong and not something you can fix by calling differently. Nothing was \
         written that you should rely on. Try once more; if it fails again, tell the operator, \
         because it needs a person."
    )
}

/// **A record jojobot can see and cannot read**, told the same way.
///
/// The adapter's reason — which field was edited past parsing, and on what —
/// is exactly the detail an operator repairing it needs and exactly the detail
/// an agent must not be handed. Logged, not returned.
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

    /// **The words an adapter may use and a caller may not.**
    ///
    /// Asserted against the sentences that CROSS, not against a list of
    /// sentences somebody remembered to check — that is the difference between
    /// this and the sweep the cut removed. There are two functions here, so
    /// there are two things to check, and a third would have to be added
    /// deliberately.
    #[test]
    fn nothing_crossing_the_boundary_names_the_store() {
        let leaky = "the page for gamma has no table, and the row vanished from the document";
        for said in [
            store_failed("post_message", leaky),
            unreadable("message gamma-4", leaky),
        ] {
            let lowered = said.to_lowercase();
            for furniture in [
                "page",
                "table",
                "row",
                "cell",
                "column",
                "document",
                "fence",
                "markdown",
                "outline",
                "collection",
            ] {
                assert!(
                    !lowered.contains(furniture),
                    "a caller has no {furniture:?}: {said}"
                );
            }
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
