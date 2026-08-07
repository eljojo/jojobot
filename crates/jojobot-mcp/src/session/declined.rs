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
        // True of both ways to get here: a bot with no session at all has
        // nothing written yet; a bot whose last session was wrapped or swept
        // has a record that is closed and no longer amendable. Never say "not
        // even written to disk" — that sends a caller looking for entries
        // that are sitting right there, closed.
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
        // The two ends part company here, because the way forward does: the
        // message for an abandoned run must never tell its owner their work
        // belongs to a new session — that is advice to fork the very thing
        // they were trying to continue.
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
        // **A malformed id or entry is a caller mistake, so it is an answer**
        // (rule 68). The validator's own sentence says which fault it is and
        // what the rule is, and it is carried rather than restated: a focus
        // alone has three ways to be refused and the validators gain more, so
        // naming them here would be a catalogue that goes stale (rule 106).
        SessionError::InvalidId(_) | SessionError::InvalidEntry(_) => blocked(
            "",
            format!(
                "Nothing was written: {e}. Nothing about this needs the operator and no session \
                 is missing — the call itself is what jojobot cannot carry out. Send it again \
                 with that fixed."
            ),
        ),
        other => Err(session_error(other)),
    }
}

/// Map a [`SessionError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split the other two contexts make.
pub(crate) fn session_error(e: SessionError) -> McpError {
    match e {
        // **Backstops, not the intended answer.** Every one of these is a
        // caller mistake and `session_declined` answers all of them as blocked
        // results with a way forward (rule 68). They are reached only by a verb
        // that surfaces an error without going through that path, and they stay
        // client errors rather than 500s for that case.
        SessionError::InvalidId(_)
        | SessionError::InvalidEntry(_)
        | SessionError::UnknownSession { .. }
        | SessionError::Closed { .. }
        | SessionError::NoEntries { .. }
        | SessionError::NotABeat { .. } => McpError::invalid_params(e.to_string(), None),
        // **The adapter's own account does not cross.** It names pages and
        // tables, which is its business and never a caller's — logged instead,
        // where an operator debugging a real failure wants it. See
        // [`crate::boundary`].
        SessionError::Store(_) | SessionError::NotConfigured(_) => McpError::internal_error(
            crate::boundary::store_failed("this call", &e.to_string()),
            None,
        ),
        SessionError::Stranded { .. } => {
            McpError::internal_error(crate::boundary::stranded("this call", &e.to_string()), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

    /// **A caller mistake never leaves this rail through the error channel**
    /// (rule 68). It comes back as a blocked answer carrying what is wrong and
    /// what to do about it.
    ///
    /// The two faults reach the surface by different routes, and only one of
    /// them is the fall-through the mapper is blamed for. An empty entry is
    /// refused by the append and handed to the declined path; a focus the
    /// record cannot carry is refused by the call that OPENS the session,
    /// which sends its error straight to the mapper. Driving both through
    /// `journal` is what tells them apart — asking the mapper directly would
    /// pass on a build where neither is wired to anything.
    #[tokio::test]
    async fn a_malformed_beat_is_an_answer_rather_than_an_error() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        // Refused by the append, on a session that already exists.
        let started = booted(&jojobot, "gamma").await;
        jojobot
            .journal(Parameters(JournalArgs {
                entry: "set out to read the box".into(),
                focus: None,
                sid: started.clone(),
            }))
            .await
            .expect("the first beat lands");
        let empty_entry = jojobot
            .journal(Parameters(JournalArgs {
                entry: "   ".into(),
                focus: None,
                sid: started,
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // Refused while the session is being opened, before any entry exists.
        // A second identity, because a bot with a run in flight is offered it
        // back rather than handed a fresh handle.
        make_bot(&jojobot, "delta").await;
        let fresh = booted(&jojobot, "delta").await;
        let bad_focus = jojobot
            .journal(Parameters(JournalArgs {
                entry: "set out to read the box".into(),
                focus: Some("reading `the box`".into()),
                sid: fresh,
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        let said = |e: SessionError| e.to_string();
        for (what, result, expected) in [
            (
                "an empty entry",
                &empty_entry,
                said(
                    jojobot_domain::session::validate_entry("   ")
                        .expect_err("an empty entry is refused"),
                ),
            ),
            (
                "a focus the record cannot carry",
                &bad_focus,
                said(
                    jojobot_domain::session::validate_focus("reading `the box`")
                        .expect_err("a focus with a backtick is refused"),
                ),
            ),
        ] {
            let body = blocked(result);
            assert_eq!(body["wrote"], false, "{what} wrote something: {body}");
            let advice = body["how_to_proceed"].as_str().expect("advice");
            // The validator's own sentence, read from the validator rather
            // than written out here: it says which fault it is and what the
            // rule is, and pinning the relation leaves the wording free.
            assert!(
                advice.contains(&expected),
                "{what} came back without the reason it was refused for.\n  wanted: \
                 {expected}\n  got: {advice}"
            );
        }
    }

    /// A stranded write must never be told to retry — it may have
    /// half-landed, and a repeat could double whatever did.
    #[test]
    fn a_stranded_write_does_not_invite_a_retry() {
        let leaky_cause = "the page for gamma has no table";
        let leaky_rollback = "the row vanished from the document";
        let err = session_error(SessionError::Stranded {
            verb: "journal".into(),
            stranded: vec!["gamma-4".into()],
            cause: leaky_cause.into(),
            rollback: leaky_rollback.into(),
        });
        assert!(
            !err.message.contains(leaky_cause) && !err.message.contains(leaky_rollback),
            "the adapter's own words crossed: {}",
            err.message
        );
        assert!(
            !err.message.contains("Try once more"),
            "a stranded write must not invite a retry: {}",
            err.message
        );
        assert!(
            err.message.contains("Do not try again"),
            "…and must say so plainly: {}",
            err.message
        );
        assert!(
            err.message.contains("Tell the operator"),
            "a caller needs the way out that is actually safe: {}",
            err.message
        );
    }
}
