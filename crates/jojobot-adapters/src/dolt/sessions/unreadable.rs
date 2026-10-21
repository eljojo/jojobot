//! **A run whose row cannot be read is named, and the runs beside it still read**
//! (rule 130, rule 342).
//!
//! A stored moment, day or state that does not parse makes ONE record unreadable.
//! It does not make every read of a bot's runs fail: those reads feed
//! `list_runs`, the thought captures and the edit of a bot's claim, and a failure
//! there would stop every write on the bot for want of one damaged row. The run
//! comes back carrying the reason, with the fields its row still says.

use super::*;

/// **Why a run row cannot be read**, or nothing when it can: its start, its state
/// and its stated day, each as the full read of the row would parse them.
pub(super) fn why_unreadable(row: &sqlx::mysql::MySqlRow) -> Result<Option<String>, SessionError> {
    let started: String = row.try_get("started_at").map_err(store)?;
    if started.parse::<Timestamp>().is_err() {
        return Ok(Some("its start time is no time".to_string()));
    }
    let state: String = row.try_get("state").map_err(store)?;
    if SessionState::from_token(&state).is_none() {
        return Ok(Some("it sits in no state jojobot recognizes".to_string()));
    }
    let stated: Option<String> = row.try_get("started_on").map_err(store)?;
    if stated.is_some_and(|cell| cell.parse::<jiff::civil::Date>().is_err()) {
        return Ok(Some("its stated day is no day".to_string()));
    }
    Ok(None)
}

/// **The summary of a run that cannot be read**: its name, its bot, what it was
/// working on and what it was served, which are text and a number, with the
/// reason and no moment it could be trusted for. A run that cannot be read is not
/// live, so it is always abandoned, whatever state its row holds: a reader that
/// treats an active run as running (a bot's `working_on`) must not take it for one.
pub(super) fn unreadable_summary(
    row: &sqlx::mysql::MySqlRow,
    why: String,
) -> Result<jojobot_domain::session::SessionSummary, SessionError> {
    let id: String = row.try_get("id").map_err(store)?;
    tracing::error!(session = %id, why = %why, "a run cannot be read and is named as unreadable");
    Ok(jojobot_domain::session::SessionSummary {
        sid: readable_sid(row.try_get("sid").map_err(store)?, &id),
        id: SessionId(id),
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at: Timestamp::UNIX_EPOCH,
        state: SessionState::Abandoned,
        entry_count: 0,
        last_beat: Timestamp::UNIX_EPOCH,
        served_chars: row.try_get::<i64, _>("served_chars").map_err(store)? as u64,
        started_on: None,
        compactions: 0,
        served_set_back: 0,
        unreadable: Some(why),
    })
}
