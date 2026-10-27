use jojobot_domain::mailbox::MailboxError;

crate::answer::lane_refusal_codes! {
mailbox_refusal_code, MAILBOXES_REFUSAL_CODES, MailboxError;
    Indeterminate,
    InvalidName(..),
    InvalidMessageId(..),
    InvalidMessage(..),
    UnknownMessage { .. },
    Quarantined { .. },
    QuarantinedOnPurpose { .. },
    NotYourMessage { .. },
    AlreadyProcessed { .. },
    CannotSupersede(..),
    Store(..),
    Refused(..),
    Conflict,
    OwnerHasMultipleBoxes { .. },
    NameTaken { .. },
    KindsNeverLoaded,
}
