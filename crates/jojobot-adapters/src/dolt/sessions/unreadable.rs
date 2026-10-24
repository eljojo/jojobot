//! **A run whose row cannot be read is named, and the runs beside it still read**
//! (rule 130, rule 342).
//!
//! A stored moment, day or state that does not parse makes ONE record unreadable.
//! It does not make every read of a bot's runs fail: those reads feed
//! `list_runs`, the thought captures and the edit of a bot's claim, and a failure
//! there would stop every write on the bot for want of one damaged row. The run
//! comes back carrying the reason, with the fields its row still says.

use super::*;

/// **Why a run row cannot be read**, or nothing when it can. The row is parsed by
/// the decoder the full read uses, so a field it learns to parse is judged here
/// too, and the reason names the field and never the damaged cell. A row the
/// store could not hand over at all is a failed read, not a damaged run.
pub(super) fn why_unreadable(row: &sqlx::mysql::MySqlRow) -> Result<Option<String>, SessionError> {
    match super::session_from(row, &[]) {
        Ok(_) => Ok(None),
        Err(SessionError::Unreadable { what }) => Ok(Some(what)),
        Err(other) => Err(other),
    }
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

impl DoltSessions {
    /// **One run of a list read, with its name on the damage.** A row that does
    /// not parse stops the list, and the refusal says which run it stopped at, so
    /// the repair is aimed at that run and not at the bot's others.
    pub(super) async fn read_listed(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: String,
    ) -> Result<Session, SessionError> {
        self.read_in(tx, &SessionId(id.clone()))
            .await
            .map_err(|e| match e {
                SessionError::Unreadable { what } => SessionError::Unreadable {
                    what: format!("run {id}, {what}"),
                },
                other => other,
            })
    }
}
