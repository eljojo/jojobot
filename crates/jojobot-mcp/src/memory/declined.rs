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
    /// A rename's destination handle. Unlike [`Relabelling`](Self::Relabelling)
    /// a handle IS moving here — that is the whole verb — so the advice must
    /// not say otherwise; unlike [`Creating`](Self::Creating) the thing is
    /// not new, it already answers to a different name.
    Renaming,
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
        Blocked::Renaming if exact => format!(
            "Nothing was renamed. The handle '{attempted}' is already taken, and that cannot be \
             forced — a handle has exactly one owner. Either this IS the entity above (nothing \
             to do — it already has this name), or it is a different thing and the destination \
             needs a more qualified slug.",
        ),
        Blocked::Renaming => format!(
            "Nothing was renamed. If '{attempted}' IS one of the entities above, renaming onto \
             it would collide with a thing that already exists there. If it is genuinely a \
             different thing that happens to share a name, re-call rename_entity with \
             override_token: \"{token}\". That token belongs to THIS refusal and lifts no other. \
             Display names are not unique and never have to be; the handle is what has to be.",
        ),
    };
    blocked_body(attempted, candidates, how_to_proceed)
}

/// The blocked envelope itself, once — so every gate's advice arrives in one
/// shape and a client branches on `status`, never on which gate fired.
pub(crate) fn blocked_body(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    how_to_proceed: impl Into<WayForward>,
) -> CallToolResult {
    let how_to_proceed = how_to_proceed.into();
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed.as_str(),
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
        // record the caller asked to take back is archived already, whether
        // by this verb or by an ordinary edit; what this call wrote is
        // nothing, because there was nothing left to write. Saying "cannot
        // be retracted" here denies a state the store is holding, and a
        // caller who believes it treats an archived record as live.
        MemoryError::AlreadyRetracted { attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "'{attempted}' is already archived — the record jojobot holds is the one you \
                 asked for. This call wrote nothing because there was nothing left to write, and \
                 a further attempt would say the same. Archiving is one-way: nothing takes a \
                 record back out of it. If that was itself a mistake, capture what is so now as \
                 a new record."
            ),
        )),
        // **Half an attribution is what is missing, and the caller has both
        // ways out** (rule 68): name where it was read, or file it as the
        // derivation it would otherwise be.
        MemoryError::UnsourcedObservation => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. The claim itself is fine — send it again with \
                 read_from naming the system, or with provenance inference."
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
        // One arm for all of them, interpolating the validator's own sentence
        // rather than restating it. Each of these faults has several causes
        // and the validators gain more; naming them here would be a catalogue
        // that goes stale on the day it is added to (rule 106).
        // **A refusal nothing in the call can reach** (rule 68). The set of
        // kinds is loaded at startup and no verb re-reads it, so the advice
        // every other malformed call gets — send it again with that fixed —
        // is advice that cannot succeed here, and a model follows it round a
        // loop with no end. This says who repairs it instead.
        MemoryError::KindsNeverLoaded { .. } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. The call is not what is wrong, and sending it                  again will not help: jojobot loaded no kinds when it started, and nothing                  a caller does re-reads them. This one needs the operator."
            ),
        )),
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
        | MemoryError::InvalidType(_) => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Nothing is missing from the store and nothing here \
                 needs the operator — the call itself is what jojobot cannot carry out. Send the \
                 same {verb} call again with that fixed."
            ),
        )),
        // **A well-formed call against a name that is not the caller's.** Not
        // a malformed declaration and not a missing one: the type is there and
        // the software owns it.
        //
        // So the way forward cannot be "send it again with that fixed" —
        // nothing the caller can put in this call reaches a shipped type, and
        // advice that implies otherwise sends a model round a loop with no
        // end. What it can do is declare under a name of its own, and the
        // sentence says so and names the verb to do it with (rule 68).
        // **The caller does not know a mechanism is here, so the sentence does
        // not name one.** No layer, no core, no composition: what they did is
        // send back text that repeats what is already there, and what they can
        // do is send only the part they are adding. A reader who has never
        // heard of any of this can act on that, which is the whole bar.
        //
        // The address is on the answer because a caller writing several things
        // needs to know which one came back.
        // **Refused because it is somebody else's, which is not the same
        // answer as absent.** A caller told a handle names nothing cannot tell
        // a typo from a thing it may not read, and would retry the first
        // forever. This says which it is and that retrying will not help.
        MemoryError::NotYours { ref attempted, .. } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was read: {e}. It is there, and it is not yours — sending this \
                 again will not change the answer. A session's chronology is its own bot's, \
                 and yours is what your own handle reaches."
            ),
        )),
        MemoryError::RepeatsShipped => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Sending this again will not change the answer. Send \
                 only the part you are adding — what you write is kept beside what is already \
                 there, and a read hands back both."
            ),
        )),
        MemoryError::ShippedType { ref name } => Ok(blocked_body(
            &EntityId(name.clone()),
            &[],
            format!(
                "Nothing was written: {e}. A shipped type cannot be extended, shrunk or replaced \
                 from here, so sending this call again will not change the answer — changing one \
                 is a change to the software. Declare a type of your own instead: call {verb} with \
                 a different name, and that type is yours to declare and redeclare as you like."
            ),
        )),
        // **The write is well formed and the record is real** — what it would
        // cost is a key the thing needs to go on being what it is. Re-sending
        // it cannot change that, so the way forward is the choice the caller
        // actually has: leave the key, or say the same thing somewhere the
        // type can still see it.
        MemoryError::BreaksFit { ref name, ref keys } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This thing answers '{name}' now, and {verb} would take \
                 {} off it, so sending this call again will not change the answer. Leave the key \
                 where it is, or write what you meant under a key of your own — adding keys is \
                 never refused, and a thing may carry anything beyond what a type asks for.",
                keys.join(", ")
            ),
        )),
        // **The key stays and the value has to change**, which is what makes
        // this a different way forward from the one above. The caller is told
        // what the key holds in the words a declaration uses, so the repair is
        // a value it can write rather than a rule it has to infer.
        MemoryError::BreaksType {
            ref name,
            ref key,
            ref wanted,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This thing is a '{name}' now, so '{key}' has to go on \
                 holding {wanted} — sending the same value again will not change the answer. \
                 Write a value that holds {wanted}, or say what you meant under a key of your \
                 own: adding keys is never refused, and only the keys a type names are held to \
                 what it declared."
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
        // **Three of `rename_entity`'s own misses**, each a caller mistake
        // and none of them a resemblance or an ordinary absence, so none of
        // them fits `blocked_result`'s gates. Answered here instead of
        // falling through to `memory_error`, which turned all three into a
        // plain protocol error — false against this verb's own argument
        // documentation, which promises a stale handle comes back "with the
        // name it moved to, rather than a bare miss."
        //
        // **Two sides of one handle** is not a resemblance: the destination
        // is fine, it names exactly what the source already answers to.
        MemoryError::NothingToRename { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was renamed: {e}. Sending this again with the same handle on both \
                 sides will not change the answer — name a destination that differs from \
                 '{attempted}'."
            ),
        )),
        // **Stale, not absent** (rule 261): the caller's evidence is real, it
        // is just out of date. The way forward is the handle it wears now,
        // never a bare miss that reads the same as a handle that never
        // existed.
        MemoryError::HandleMoved {
            ref attempted,
            ref now,
        } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was renamed: {e}. Re-call {verb} with handle: \"{now}\" — that is what \
                 '{attempted}' answers to now."
            ),
        )),
        // **Real, and with nothing stored underneath it.** Neither a create
        // (it already exists) nor an ordinary miss (it is not gone): a
        // build-supplied record has nothing for `rename_entity` or `merge`
        // to move, on either side of a fold.
        MemoryError::SuppliedHandle { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing changed: {e}. Sending this again will not change the answer — \
                 '{attempted}' is part of the software rather than something stored that \
                 {verb} could move."
            ),
        )),
        // **Two of `merge`'s own misses**, answered here for the same reason
        // `rename_entity`'s three are: neither is a resemblance or an
        // ordinary absence, and the tool's own published description
        // promises both come back `status: blocked` naming a way forward —
        // a promise the code was breaking rather than the description
        // (rule 199).
        //
        // **Two sides of one handle** is not a resemblance: the survivor is
        // fine, it names exactly what the duplicate already answers to.
        MemoryError::NothingToMerge { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Sending this again with the same handle on both \
                 sides will not change the answer — name a survivor that differs from \
                 '{attempted}'."
            ),
        )),
        // **Stale, not absent** (rule 261): the row named is real — it is a
        // forwarding row rather than a side to fold or a survivor to fold
        // into — so the way forward is the handle it forwards to, on
        // whichever side it was named.
        MemoryError::AlreadyMerged {
            ref attempted,
            ref into,
        } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Re-call {verb} naming '{into}' instead of \
                 '{attempted}' — that is where it resolves now."
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
        | MemoryError::KindsNeverLoaded { .. }
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
        | MemoryError::InvalidType(_)
        | MemoryError::ShippedType { .. }
        | MemoryError::RepeatsShipped
        | MemoryError::BreaksFit { .. }
        | MemoryError::BreaksType { .. }
        | MemoryError::UnknownFact { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::NotYours { .. }
        | MemoryError::NotRetractable { .. }
        | MemoryError::UnsourcedObservation
        | MemoryError::AlreadyRetracted { .. }
        | MemoryError::NothingToMerge { .. }
        | MemoryError::AlreadyMerged { .. }
        | MemoryError::NothingToRename { .. }
        | MemoryError::HandleMoved { .. }
        | MemoryError::SuppliedHandle { .. }
        | MemoryError::UnconfirmedPromotion
        | MemoryError::UnconfirmedSettling => McpError::invalid_params(e.to_string(), None),
        MemoryError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
        }
        // **A build-time misconfiguration, not a caller mistake.** No verb
        // produces this — it is caught at boot, before an instance ever
        // serves — so there is no way forward to hand a caller and no
        // `memory_declined` arm for it either.
        MemoryError::SuppliedRecordCollidesWithStoredRow { .. } => {
            McpError::internal_error(e.to_string(), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// **Wired to the mechanism, not merely beside it** (decision log 261,
    /// 262). Every arm of `memory_declined` funnels through `blocked_body`,
    /// so proving this one site panics on an empty way forward is proving it
    /// for all of them at once.
    #[test]
    #[should_panic(expected = "way forward")]
    fn blocked_body_cannot_be_built_with_an_empty_way_forward() {
        blocked_body(&EntityId("person:homer".into()), &[], String::new());
    }

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

    /// **The store being down is a failure, not a blocked answer** — the other
    /// half of the line this module draws, over the one read both typed verbs
    /// make before anything else.
    ///
    /// `declared_types` has exactly one fallible step, so every error reachable
    /// through it is the store. A client following the published instructions
    /// branches on `status`, and `blocked` tells it the call was ITS mistake:
    /// it fixes the type name it got right, and never retries the outage.
    ///
    /// Driven through both verbs that take `answers_type`, because the resolve
    /// is shared and a channel is chosen at each call site — one of them
    /// swallowing the error back into `Ok` is exactly the drift this pins.
    #[tokio::test]
    async fn a_store_that_cannot_list_types_is_a_failure_rather_than_the_callers_mistake() {
        let jojobot = Jojobot::new(
            Arc::new(DownMemory(
                Down::TypeRoster,
                Arc::new(InMemoryMemory::booted()),
            )),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let sid = writing_as(&jojobot);

        let recalled = jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid.clone()),
                answers_type: Some("pet".into()),
                ..recall_args("person:bart")
            }))
            .await;
        let searched = jojobot
            .search(Parameters(SearchArgs {
                sid: Some(sid),
                answers_type: Some("pet".into()),
                ..search_args()
            }))
            .await;

        for (verb, result) in [("recall", recalled), ("search", searched)] {
            let err = match result {
                Err(err) => err,
                Ok(ok) => panic!(
                    "{verb} handed an outage back as a caller-fixable answer: {}",
                    text_of(&ok)
                ),
            };
            // The adapter's own words stay inside, as everywhere else on this
            // rail — what crosses is that it was the store and to try again.
            assert!(
                !err.message.contains("the type roster cannot be read"),
                "{verb} let the adapter's own words cross: {}",
                err.message
            );
            assert!(
                err.message.contains("Try once more"),
                "{verb} left the caller without its next move: {}",
                err.message
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
}
