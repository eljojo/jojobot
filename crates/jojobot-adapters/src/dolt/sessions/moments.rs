//! **How a stored moment, day or state is read**, in one place for the full read
//! of a run and the cheap summary of it. A cell that does not parse is damage a
//! person repairs ([`SessionError::Unreadable`]), never a failed read to retry.

use super::*;

/// **The three moments of a chronology row**: when it was written, the day it
/// happened on, and when it was touched. The one parse of them, so the full read
/// and the cheap summary read judge an entry by the same code.
pub(super) fn entry_moments(
    row: &sqlx::mysql::MySqlRow,
) -> Result<(Timestamp, Option<jiff::civil::Date>, Option<Timestamp>), SessionError> {
    let at: String = row.try_get("at").map_err(store)?;
    let touched: Option<String> = row.try_get("touched").map_err(store)?;
    let on: Option<String> = row.try_get("happened_on").map_err(store)?;
    Ok((
        instant_of(&at, "one of its beats' time")?,
        day_of(on, "one of its beats' day")?,
        touched
            .as_deref()
            .map(|raw| instant_of(raw, "one of its beats' touch time"))
            .transpose()?,
    ))
}

/// **The columns of a run's own row that the full read parses.** The cheap
/// summary reads the same list and parses it with the same decoder, so a field
/// the full read learns to parse is judged by the summary too.
pub(super) const RUN_COLUMNS: &str = "id, sid, key_hash, bot, focus, started_at, state, timezone, \
    started_on, served_chars, stated_day, wrap_window, charter_told_at, retired_key_hash, \
    retired_at, retired_by, taken_at, displaced_by";

/// A state token this build does not know: damage, and a person's.
pub(super) fn no_such_state() -> SessionError {
    SessionError::Unreadable {
        what: "it sits in no state jojobot recognizes".into(),
    }
}

/// Parse a stored instant. A cell that is no instant is a record jojobot
/// cannot read rather than a value to guess at.
pub(super) fn instant(raw: &str) -> Result<Timestamp, SessionError> {
    instant_of(raw, "a stored moment")
}

/// [`instant`] naming the field, so the refusal says which cell. The damaged
/// text goes to the log and never to a caller.
pub(super) fn instant_of(raw: &str, what: &str) -> Result<Timestamp, SessionError> {
    raw.parse().map_err(|_| {
        tracing::error!(cell = %raw, field = what, "a session row carries a timestamp that is no timestamp");
        SessionError::Unreadable {
            what: format!("{what} is no time"),
        }
    })
}

/// **A stated day as the store holds it**, or nothing when the run stated none.
///
/// A cell that is no date is a store fault a person repairs, exactly as a
/// timestamp that is no timestamp is: it is not a run without a frame, it is a
/// run whose frame jojobot cannot read, and answering it on the clock would
/// quietly serve the wrong one.
pub(super) fn day(raw: Option<String>) -> Result<Option<jiff::civil::Date>, SessionError> {
    day_of(raw, "a stored day")
}

/// [`day`] naming the field, so the refusal says which cell.
pub(super) fn day_of(
    raw: Option<String>,
    what: &str,
) -> Result<Option<jiff::civil::Date>, SessionError> {
    raw.map(|cell| {
        cell.parse().map_err(|_| {
            tracing::error!(cell = %cell, field = what, "a session row carries a day that is no day");
            SessionError::Unreadable {
                what: format!("{what} is no day"),
            }
        })
    })
    .transpose()
}

/// One `session_write` row as the domain's record. A record an older build
/// appended has no kind and no moment, and reads as unrecorded.
pub(super) fn change_from_row(row: &sqlx::mysql::MySqlRow) -> Result<SessionChange, SessionError> {
    let changed = row
        .try_get::<Option<String>, _>("changed")
        .map_err(store)?
        .map(|token| {
            ChangeKind::from_token(&token).ok_or_else(|| {
                SessionError::Store(
                    "a stored session change is of a kind this build does not know".into(),
                )
            })
        })
        .transpose()?;
    Ok(SessionChange {
        ordinal: row.try_get::<i64, _>("ordinal").map_err(store)?.max(0) as usize,
        changed,
        value: row.try_get("value").map_err(store)?,
        written_at: row
            .try_get::<Option<String>, _>("written_at")
            .map_err(store)?
            .map(|at| instant_of(&at, "one of its changes' time"))
            .transpose()?,
    })
}
