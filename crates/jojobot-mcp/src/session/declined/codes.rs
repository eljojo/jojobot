use jojobot_domain::session::SessionError;

crate::answer::lane_refusal_codes! {
session_refusal_code, SESSION_REFUSAL_CODES, SessionError;
    Indeterminate,
    InvalidId(..),
    InvalidEntry(..),
    UnknownSession { .. },
    Closed { .. },
    NotWrapped { .. },
    NoEntries { .. },
    NotABeat { .. },
    Store(..),
    Unreadable { .. },
    Refused(..),
    Conflict,
    KindsNeverLoaded,
}
