//! **What a claim says of the run whose role it took.** The run was found in a
//! state, and the sentence says what happened to it in that state: an active
//! holder is abandoned, a holder that wrapped stays wrapped with only its role
//! released, and a holder the sweep had already abandoned is not abandoned again.

use jojobot_domain::session::SessionState;

/// The note of a claim that took `run`'s role, found in state `was`. A holder
/// that lost the role but could not be closed (`left_open`) is still active, and
/// the note says so instead of saying it is abandoned.
pub(crate) fn of(run: &str, was: SessionState, left_open: bool) -> String {
    match was {
        SessionState::Active if left_open => format!(
            "run {run} of this bot held the role and has lost it, and its lease is yours. It \
             could not be marked displaced, so it is still active and does not know."
        ),
        SessionState::Wrapped => format!(
            "run {run} of this bot had wrapped without releasing the role, and its lease is yours."
        ),
        SessionState::Abandoned => format!(
            "run {run} of this bot had already been abandoned and still held the role, and its \
             lease is yours. It can still be resumed, and resuming it does not take the role \
             back."
        ),
        SessionState::Active => format!(
            "run {run} of this bot held the role and is now abandoned, and its lease is yours. \
             It can still be resumed, and resuming it does not take the role back."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Each state the holder was found in gets its own sentence**, and only an
    /// active holder is said to be abandoned now.
    #[test]
    fn each_state_the_holder_was_found_in_has_its_own_sentence() {
        let active = of("r1", SessionState::Active, false);
        let abandoned = of("r1", SessionState::Abandoned, false);
        let wrapped = of("r1", SessionState::Wrapped, false);
        assert!(active.contains("is now abandoned"), "{active}");
        assert!(!abandoned.contains("is now abandoned"), "{abandoned}");
        assert!(abandoned.contains("already been abandoned"), "{abandoned}");
        assert!(!wrapped.contains("abandoned"), "{wrapped}");
        let left_open = of("r1", SessionState::Active, true);
        assert!(!left_open.contains("is now abandoned"), "{left_open}");
        assert!(left_open.contains("still active"), "{left_open}");
        for note in [&active, &abandoned, &wrapped, &left_open] {
            assert!(note.contains("r1"), "{note}");
        }
    }
}
