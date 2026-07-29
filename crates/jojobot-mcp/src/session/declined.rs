//! **The session context's refusals.**
//!
//! Its half of "a miss is an answer, not a failure": an id that names nothing,
//! a session that is closed, and an amend with nothing to amend all come back
//! in the guards' one shape.

use super::*;

/// An amend on a session that has not begun. Refused rather than turned into a
/// first entry.
pub(crate) fn session_nothing_to_amend() -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        // **True of both ways to get here.** A bot with no session at all has
        // nothing written yet; a bot whose last session was wrapped or swept
        // has a record that is closed and no longer amendable. Saying "not even
        // written to disk" was false for the second, and it sent a caller
        // looking for entries that are sitting right there, closed.
        "how_to_proceed": "Nothing was written. There is no OPEN session to amend: either this \
                           identity has not written anything yet — a session's record begins on \
                           its first beat — or its last session is closed, and closed is \
                           terminal both ways. Use journal to begin the next one; its first \
                           entry is what brings the record into being. To read a closed \
                           session's chronology, booting as this identity through start_here \
                           reports its state.",
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// The session context's half of "a miss is an answer, not a failure": an id
/// that names nothing, a session that is closed, and an amend with nothing to
/// amend all come back in the guards' one shape.
pub(crate) fn session_declined(e: SessionError) -> Result<CallToolResult, McpError> {
    let blocked = |attempted: &str, how: String| {
        let body = serde_json::json!({
            "status": "blocked",
            "attempted": attempted,
            "wrote": false,
            "how_to_proceed": how,
        });
        Ok(CallToolResult::success(vec![ContentBlock::text(
            body.to_string(),
        )]))
    };
    match e {
        SessionError::UnknownSession { attempted } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. jojobot holds no session with the id '{attempted}'. \
                 Ids are minted by jojobot and handed back by start_here when you boot as your \
                 identity — use the sid it gives you rather than composing one."
            ),
        ),
        // **The two ends part company here, because the way forward does.** One
        // paragraph for both used to tell the owner of a run that merely
        // stopped that their work belonged to a new session — which is advice
        // to fork the very thing they were trying to continue.
        SessionError::Closed {
            attempted,
            state: SessionState::Abandoned,
        } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' is abandoned — it stopped without \
                 being wrapped up, so it takes no write as it stands. That is not a failure and \
                 not the end of it: resume it. Call start_here with your bot name, and either \
                 take it from the offer or pass resume with its sid — it reopens where it left \
                 off and its chronology continues."
            ),
        ),
        SessionError::Closed { attempted, state } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' is {state} — its story has been told, \
                 so this end is the last word. Its chronology stands as the record of what \
                 happened. If there is more to say, it belongs to a new session: boot again (or \
                 rotate) and start_here mints one."
            ),
        ),
        SessionError::NoEntries { attempted } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Session '{attempted}' has no entries yet, so there is no \
                 most-recent one to amend — journal it instead."
            ),
        ),
        SessionError::NotABeat { attempted, session } => blocked(
            &attempted.clone(),
            format!(
                "Nothing was written. Entry '{attempted}' on session '{session}' is one the \
                 session recorded itself, and those are append-only wherever they sit. Only the \
                 most recent entry can be amended, through amend_journal."
            ),
        ),
        other => Err(session_error(other)),
    }
}

/// Map a [`SessionError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split the other two contexts make.
pub(crate) fn session_error(e: SessionError) -> McpError {
    match e {
        SessionError::InvalidId(_) | SessionError::InvalidEntry(_) => {
            McpError::invalid_params(e.to_string(), None)
        }
        // Reached only if a verb surfaces one without going through
        // `session_declined` — kept as a client error rather than a 500 for the
        // same reason the other contexts keep theirs.
        SessionError::UnknownSession { .. }
        | SessionError::Closed { .. }
        | SessionError::NoEntries { .. }
        | SessionError::NotABeat { .. } => McpError::invalid_params(e.to_string(), None),
        // **The adapter's own account does not cross.** It names pages and
        // tables, which is its business and never a caller's — logged instead,
        // where an operator debugging a real failure wants it. See
        // [`crate::boundary`].
        SessionError::Stranded { .. } | SessionError::Store(_) | SessionError::NotConfigured(_) => {
            McpError::internal_error(
                crate::boundary::store_failed("this call", &e.to_string()),
                None,
            )
        }
    }
}
