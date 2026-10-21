//! **What a claim says of the run whose role it took.** The run was found in a
//! state, and the sentence says what happened to it in that state: an active
//! holder is abandoned, a holder that wrapped stays wrapped with only its role
//! released, and a holder the sweep had already abandoned is not abandoned again.

use jojobot_domain::session::SessionState;

/// The note of a claim that took `run`'s role, found in state `was`.
pub(crate) fn of(run: &str, was: SessionState) -> String {
    match was {
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
        let active = of("r1", SessionState::Active);
        let abandoned = of("r1", SessionState::Abandoned);
        let wrapped = of("r1", SessionState::Wrapped);
        assert!(active.contains("is now abandoned"), "{active}");
        assert!(!abandoned.contains("is now abandoned"), "{abandoned}");
        assert!(abandoned.contains("already been abandoned"), "{abandoned}");
        assert!(!wrapped.contains("abandoned"), "{wrapped}");
        for note in [&active, &abandoned, &wrapped] {
            assert!(note.contains("r1"), "{note}");
        }
    }
}
