//! **Memory's refusals** — the guard's answers, and the line between a refusal
//! and a failure.
//!
//! A blocked result is a SUCCESS whose body says `status: "blocked"`,
//! `wrote: false`: the caller named something that resembles what exists, or
//! named something that is not there, and nothing was written. A plain error is
//! a malformed call or the store itself failing. Callers branch on `status`;
//! they should never have to parse a failure.

use super::*;

/// Which gate stopped a write — because the way out of each one is different,
/// and one copy-pasted paragraph telling a rename to "pick a more qualified
/// slug" is worse than no advice at all.
pub(crate) enum Blocked {
    /// A creation: the handle is being minted here, so an exact collision is
    /// unforgivable and the token covers only a shared *name*.
    Creating,
    /// A relabel — a change to a name or an alias. No handle is moving, so
    /// nothing here is unforgivable.
    Relabelling,
    /// A write that only **names** an entity (a capture's subject, an edge's
    /// object). It cannot create one, so there is no token to hand back and no
    /// `override_token` on the verb.
    MustExist(&'static str),
}

/// The write guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant.
///
/// A **successful** result carrying a structured payload, not a protocol error.
/// The guard doing its job is an answer the caller has to act on — jojobot
/// detects, the AI decides — and dressing it as an exception made a working
/// feature read like a broken server: clients that retry on error retry it, and
/// clients that unwrap on error handle it exactly wrong. `status` and `wrote`
/// are what stop it reading as a completed write.
pub(crate) fn blocked_result(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    gate: Blocked,
) -> CallToolResult {
    let exact = candidates
        .iter()
        .any(|c| c.reason == guard::MatchReason::ExactHandle);
    // **The token this refusal mints, and the only thing that lifts it** (rule
    // 75). It rides the advice rather than sitting in a field of its own,
    // because a secret a caller has to be told separately about is one nobody
    // uses — the sentence that says what to do names the thing to do it with
    // (rule 68). An exact collision mints none: there is nothing to lift.
    let token = guard::override_token(attempted, candidates);
    let how_to_proceed = match gate {
        Blocked::Creating if exact => format!(
            "Nothing was written. The handle '{attempted}' is already taken, and that cannot be \
             forced — a handle has exactly one owner. Either this IS the entity above (use its \
             handle and carry on), or it is a different one and needs a more qualified slug.",
        ),
        Blocked::Creating => format!(
            "Nothing was written. If '{attempted}' IS one of the entities above, use that handle \
             instead. If it is genuinely a different one that happens to share a name, re-call \
             add_entity with override_token: \"{token}\". That token belongs to THIS refusal and \
             lifts no other. Display names are not unique and never have to be; the handle is \
             what has to be.",
        ),
        // Says "name" rather than "rename": this gate fires on an alias write
        // too, and telling a caller nothing was renamed when they renamed
        // nothing sends them looking for a rename they never made.
        Blocked::Relabelling => format!(
            "Nothing was written, and the handle '{attempted}' is unaffected either way — this \
             only moves the names it answers to. Either pick a name or alias that isn't already \
             worn, or re-call update_entity with override_token: \"{token}\" if this entity really \
             does share a name with one above: names are not unique, handles are.",
        ),
        // The candidate list is often empty here — this gate fires on any
        // unrecognized handle, not only a near miss — so the advice must not
        // point at "the handles above" when there are none.
        Blocked::MustExist(verb) if candidates.is_empty() => format!(
            "Nothing was written. '{attempted}' is not an entity jojobot knows, and nothing \
             resembles it. {verb} cannot create an entity: call add_entity to create \
             '{attempted}' first, then re-call {verb}.",
        ),
        Blocked::MustExist(verb) => format!(
            "Nothing was written. '{attempted}' is not an entity jojobot knows. If one of the \
             handles above is what you meant, use that. Otherwise {verb} cannot create it for \
             you — call add_entity to create '{attempted}' first, then re-call {verb}.",
        ),
    };
    blocked_body(attempted, candidates, how_to_proceed)
}

/// The blocked envelope itself, once — so every gate's advice arrives in one
/// shape and a client branches on `status`, never on which gate fired.
pub(crate) fn blocked_body(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    how_to_proceed: String,
) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A miss and a block speak one shape.** An id, handle or address that names
/// nothing is not a malformed call and not a server failure: it is jojobot
/// declining because what the caller named is not there — the same answer the
/// resemblance and existence gates give — so it comes back as a *successful*
/// result whose body says `status: blocked`, `wrote: false`, with whatever is
/// nearby and what to do next.
///
/// Two shapes for one idea meant a client had to branch twice to learn the same
/// thing, and the error half read as a broken server: clients that retry on
/// error retry it, and clients that unwrap on error handle it exactly wrong.
///
/// Everything that is genuinely a caller mistake (a malformed address, an
/// unknown kind token) or genuinely a failure (the store is down) stays an
/// error. `Ok` here is the refusal; `Err` is still an error.
pub(crate) fn memory_declined(
    verb: &'static str,
    e: MemoryError,
) -> Result<CallToolResult, McpError> {
    match e {
        MemoryError::UnknownEntity { attempted, nearest } => Ok(blocked_result(
            &EntityId(attempted),
            &nearest,
            Blocked::MustExist(verb),
        )),
        // A fact miss has no entity candidates — its near misses are the live
        // addresses in the same doc, which is what makes it repairable.
        MemoryError::UnknownFact { attempted, nearest } => {
            let live = if nearest.is_empty() {
                "That entity holds no facts at all yet, so there is nothing here to edit — \
                 capture one first."
                    .to_string()
            } else {
                format!(
                    "The addresses that do exist here are: {}.",
                    nearest.join(", ")
                )
            };
            Ok(blocked_body(
                &EntityId(attempted.clone()),
                &[],
                format!(
                    "Nothing was written. '{attempted}' addresses no fact jojobot holds, and this \
                     verb never creates one. {live} Recall the entity if none of them is what you \
                     meant — every fact comes back carrying the address that edits it."
                ),
            ))
        }
        // **A refusal, not a failure**: the row is there and the caller named
        // it correctly — jojobot is declining to do this to THAT row. The
        // domain already wrote the sentence that says which of the three
        // reasons it is and what to do instead, so it is carried through
        // rather than re-worded here, where it would drift from the rule it
        // describes.
        // **Already done is not the same answer as cannot be done.** The
        // record the caller asked to take back is taken back; what this call
        // wrote is nothing, because there was nothing left to write. Saying
        // "cannot be retracted" here denies a state the store is holding, and
        // a caller who believes it treats a retracted record as live.
        MemoryError::AlreadyRetracted { attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "'{attempted}' is already retracted — the record jojobot holds is the one you \
                 asked for. This call wrote nothing because there was nothing left to write, and \
                 a further attempt would say the same. Retraction is one-way: nothing takes a \
                 record back out of it. If the retraction was itself a mistake, capture what is \
                 so now as a new record."
            ),
        )),
        MemoryError::NotRetractable { attempted, why } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!("Nothing was written. '{attempted}' cannot be retracted: {why}."),
        )),
        // **A malformed argument is a caller mistake, so it is an answer**
        // (rule 68). A thrown error is not a value: the model on the other end
        // gets a failure where it should get a next move, and the sentence
        // saying what to do lands in a channel nothing branches on.
        //
        // One arm for all six, interpolating the validator's own sentence
        // rather than restating it. Each of these faults has several causes
        // and the validators gain more; naming them here would be a catalogue
        // that goes stale on the day it is added to (rule 106).
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_) => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Nothing is missing from the store and nothing here \
                 needs the operator — the call itself is what jojobot cannot carry out. Send the \
                 same {verb} call again with that fixed."
            ),
        )),
        // **A different refusal, so a different way forward.** These two are
        // not malformed calls: the arguments are well-formed and jojobot is
        // declining to bless a claim the operator has not blessed. Telling a
        // caller to fix the call would invite them to set the confirmation
        // flag themselves, which is the one thing the gate exists to stop.
        MemoryError::UnconfirmedPromotion | MemoryError::UnconfirmedSettling => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This is not a malformed call and re-sending it will \
                 not change the answer — what is missing is the operator's word. Ask, and re-call \
                 {verb} with confirmed_by_user only once they have actually said so."
            ),
        )),
        other => Err(memory_error(other)),
    }
}

/// Map a domain [`MemoryError`] to an MCP error, splitting client mistakes
/// (invalid params) from server-side failures.
pub(crate) fn memory_error(e: MemoryError) -> McpError {
    match e {
        // **Backstops, not the intended answer.** Every one of these is a
        // caller mistake and `memory_declined` answers all of them as blocked
        // results with a way forward (rule 68). They are reached only by a verb
        // that surfaces an error without going through that path, and they stay
        // client errors rather than 500s for that case.
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
        | MemoryError::UnknownFact { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::NotRetractable { .. }
        | MemoryError::AlreadyRetracted { .. }
        | MemoryError::UnconfirmedPromotion
        | MemoryError::UnconfirmedSettling => McpError::invalid_params(e.to_string(), None),
        MemoryError::NotConfigured(msg) => {
            McpError::internal_error(format!("memory not configured: {msg}"), None)
        }
        MemoryError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
        }
        MemoryError::Stranded { .. } => {
            McpError::internal_error(crate::boundary::stranded("this call", &e.to_string()), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// **A caller mistake never leaves this rail through the error channel**
    /// (rule 68). It comes back as a blocked answer carrying what is wrong and
    /// what to do about it.
    ///
    /// Driven through the verbs a caller calls, because these faults arrive by
    /// two different routes and only one of them is the fall-through the
    /// mapper is blamed for. `capture`, `add_entity` and `search` never reach
    /// the declined path at all: they hand the domain's error straight to the
    /// mapper, so an arm added there does nothing for them until the call site
    /// is routed too. Asking the mapper directly would pass on a build where
    /// none of them is wired to anything.
    #[tokio::test]
    async fn a_malformed_memory_write_is_an_answer_rather_than_an_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("person", "person:alpha", "Alpha")))
            .await
            .expect("add ok");

        // Refused inside the domain's own write.
        let empty_claim = jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("person:alpha", "   ")
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        let bad_entity = jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(sid.clone()),
                ..add_args("person", "person:beta", "   ")
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // Refused by the query's own validation, before the index is read.
        let empty_query = jojobot
            .search(Parameters(SearchArgs {
                sid: Some(sid),
                ..search_args()
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        for (what, result) in [
            ("an empty claim", &empty_claim),
            ("an entity with no name", &bad_entity),
            ("a search that narrows nothing", &empty_query),
        ] {
            let body = blocked(result);
            assert_eq!(body["wrote"], false, "{what} wrote something: {body}");
            assert!(
                body["how_to_proceed"]
                    .as_str()
                    .is_some_and(|advice| !advice.is_empty()),
                "{what} came back with no way forward: {body}"
            );
        }
    }

    /// A store failure's own account must not reach the caller — the same
    /// invariant the mailbox and session rails hold, through the same
    /// function.
    #[test]
    fn a_store_failure_does_not_carry_the_adapters_own_words() {
        let leaky = "the page for gamma has no table, and the row vanished from the document";
        let err = memory_error(MemoryError::Store(leaky.into()));
        assert!(
            !err.message.contains(leaky),
            "the adapter's own words crossed: {}",
            err.message
        );
        assert!(
            err.message.contains("Try once more"),
            "a caller needs its next move: {}",
            err.message
        );
    }

    /// A stranded write must never be told to retry — it may have
    /// half-landed, and a repeat could double whatever did. A genuinely
    /// different class from a clean `Store` failure, so it must not share
    /// that failure's "Try once more" advice.
    #[test]
    fn a_stranded_write_does_not_invite_a_retry() {
        let leaky_cause = "the page for gamma has no table";
        let leaky_rollback = "the row vanished from the document";
        let err = memory_error(MemoryError::Stranded {
            verb: "capture".into(),
            stranded: vec!["person:alpha#f4".into()],
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
