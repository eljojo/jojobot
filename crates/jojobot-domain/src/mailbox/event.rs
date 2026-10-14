//! **How a message event is made** (rule 498). Every writer that changes a
//! message's state — the store, the fake and the fold image — calls
//! [`next_event`], so the number, the notes and the attribution of an event are
//! decided at one site. A verb that changes state and forgets to call it has no
//! event to forget to number.
//!
//! Reading a stored event back is the store's own decoding and is not here.

use super::{Acting, MailboxName, MessageEvent, MessageEventKind, MessageState, normalize_notes};

/// **What the writer of a change knows**, before the change is numbered.
#[derive(Debug, Clone, Copy)]
pub struct Change<'a> {
    kind: MessageEventKind,
    acting: Acting<'a>,
    by_box: &'a MailboxName,
    prior_state: MessageState,
    notes: Option<&'a str>,
    reason: Option<&'a str>,
}

impl<'a> Change<'a> {
    /// A change of `kind` made at `acting` through `by_box`, from `prior_state`.
    pub fn new(
        kind: MessageEventKind,
        acting: Acting<'a>,
        by_box: &'a MailboxName,
        prior_state: MessageState,
    ) -> Self {
        Change {
            kind,
            acting,
            by_box,
            prior_state,
            notes: None,
            reason: None,
        }
    }

    /// The notes the call sent, as it sent them. They are normalised when the
    /// event is made, and `None` stays `None`.
    pub fn with_notes(mut self, notes: Option<&'a str>) -> Self {
        self.notes = notes;
        self
    }

    /// The reason a quarantine gave, as the writer keeps it.
    pub fn with_reason(mut self, reason: &'a str) -> Self {
        self.reason = Some(reason);
        self
    }
}

/// **The next event of a message**: the change, numbered one past the highest of
/// `held_ordinals`, the ordinals of the events the message already has.
pub fn next_event(
    held_ordinals: impl IntoIterator<Item = usize>,
    change: Change<'_>,
) -> MessageEvent {
    MessageEvent {
        ordinal: held_ordinals.into_iter().max().unwrap_or(0) + 1,
        kind: change.kind,
        at: change.acting.at,
        by_session: change.acting.session.map(str::to_string),
        by_box: Some(change.by_box.clone()),
        prior_state: change.prior_state,
        notes: normalize_notes(change.notes),
        reason: change.reason.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
    }

    fn inbox() -> MailboxName {
        MailboxName("inbox".into())
    }

    /// **The number is one past the highest the message holds**, whatever order
    /// the held ones come in and whatever gaps they leave, and one for a message
    /// that holds none.
    #[test]
    fn an_event_is_numbered_one_past_the_highest_held() {
        let by = inbox();
        let change = Change::new(
            MessageEventKind::Read,
            Acting::by("run-a", at(1)),
            &by,
            MessageState::New,
        );
        assert_eq!(next_event([], change).ordinal, 1);
        assert_eq!(next_event([1], change).ordinal, 2);
        assert_eq!(next_event([5, 1, 2], change).ordinal, 6);
    }

    /// **The event says who made the change, when, through which box and from
    /// which state**, and an unnamed run is recorded as none.
    #[test]
    fn an_event_carries_the_run_the_moment_the_box_and_the_prior_state() {
        let by = inbox();
        let named = next_event(
            [],
            Change::new(
                MessageEventKind::Processed,
                Acting::by("run-a", at(7)),
                &by,
                MessageState::Read,
            ),
        );
        assert_eq!(named.kind, MessageEventKind::Processed);
        assert_eq!(named.by_session.as_deref(), Some("run-a"));
        assert_eq!(named.at, at(7));
        assert_eq!(named.by_box, Some(inbox()));
        assert_eq!(named.prior_state, MessageState::Read);
        let unnamed = next_event(
            [],
            Change::new(
                MessageEventKind::Read,
                Acting::unnamed(at(8)),
                &by,
                MessageState::New,
            ),
        );
        assert_eq!(unnamed.by_session, None);
    }

    /// **Notes are normalised here and a call that sent none records none.** A
    /// blank note is none, and a note with padding is trimmed.
    #[test]
    fn an_events_notes_are_the_normalised_notes_the_call_sent() {
        let by = inbox();
        let made = |notes: Option<&str>| {
            next_event(
                [],
                Change::new(
                    MessageEventKind::Processed,
                    Acting::by("run-a", at(1)),
                    &by,
                    MessageState::Read,
                )
                .with_notes(notes),
            )
            .notes
        };
        assert_eq!(made(Some("  done  ")).as_deref(), Some("done"));
        assert_eq!(made(Some("   ")), None);
        assert_eq!(made(None), None);
    }

    /// **A quarantine's reason rides on its event, and no other change has one.**
    #[test]
    fn an_events_reason_is_the_one_a_quarantine_gave() {
        let by = inbox();
        let change = Change::new(
            MessageEventKind::Quarantined,
            Acting::by("run-a", at(1)),
            &by,
            MessageState::Read,
        );
        assert_eq!(
            next_event([1], change.with_reason("not trusted"))
                .reason
                .as_deref(),
            Some("not trusted")
        );
        assert_eq!(next_event([1], change).reason, None);
    }
}
