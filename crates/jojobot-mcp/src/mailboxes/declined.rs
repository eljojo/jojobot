//! **The mailbox context's refusals.**
//!
//! Five shapes wearing one envelope, because the way out of each differs: a
//! name that resembles a box, an id that names nothing at all, something
//! jojobot cannot read as a message — that one needs a person, not a retry —
//! a message that is somebody else's to take delivery of, and a title the
//! record cannot carry. All are successful results whose body says
//! `status: "blocked"`.

use super::*;

/// **Somebody else's message, addressed by id.**
///
/// The read side has no box argument at all, so a caller cannot open another
/// bot's box; this closed the same move made one message at a time. What the
/// refusal owes is the legitimate alternative, because a session that needs
/// something out of another box and is told only "no" will find a worse way.
/// Writing INTO that box is the sanctioned shape, and it is the shape of a
/// request rather than a taking.
pub(crate) fn not_yours(id: &MessageId, theirs: &MailboxName) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": id.as_str(),
        "wrote": false,
        "mailbox": theirs.as_str(),
        "how_to_proceed": format!(
            "Nothing was delivered and nothing moved. Message '{id}' is in '{theirs}', which is \
             not your box — and reading IS taking delivery, so opening it would move somebody \
             else's mail out of `new` and it would never look fresh to the bot it was sent to \
             again. Ids are a plain counter, so the one beside yours is somebody else's; this is \
             not a permission you can be granted. To reach that box, post_message writes into it \
             without reading it, which is the shape of a request — ask its owner for what you \
             need. Your own mail is read_mailbox, which needs no id and no name."
        ),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// One gate, because there is one way to meet a box name: by naming one that
/// must already exist. Nothing mints a box, so nothing creates one by name.
pub(crate) enum BlockedBox {
    /// A write that only **names** a box. It cannot create one.
    MustExist(&'static str),
}

/// The mailbox guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant. A successful result carrying a structured payload, not a
/// protocol error — the same shape the Memory verbs use, so one client-side
/// branch handles both contexts.
pub(crate) fn mailbox_blocked(
    attempted: &MailboxName,
    candidates: &[MailboxMatch],
    gate: BlockedBox,
) -> CallToolResult {
    let how_to_proceed = match gate {
        BlockedBox::MustExist(verb) if candidates.is_empty() => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows, and nothing \
             resembles it. {verb} cannot create one — and a new box is rarely the answer: a \
             mailbox is a channel someone must be draining, so use start_here, whose snapshot \
             names every box on the board, to pick an existing one — or tell the operator there \
             is nowhere fitting to put this. A box is \
             opened only by standing up the bot that drains it, so if '{attempted}' should \
             exist, what is missing is that identity.",
        ),
        BlockedBox::MustExist(_) => format!(
            "Nothing was written. '{attempted}' is not a mailbox jojobot knows. If one of the \
             names above is what you meant, use that — it is almost certainly a typo. \
             Otherwise: a box exists only as some bot's own, so there is no box to open here \
             without an identity to drain it. Prefer an existing box, or ask the operator.",
        ),
    };
    mailbox_blocked_body(attempted.as_str(), Some(candidates), how_to_proceed)
}

/// The mailbox blocked envelope itself, once. `None` candidates is a refusal
/// with nothing to suggest — an id nothing answers to — and the key is still
/// present and empty, because a client that branches on its shape must not have
/// to branch on whether it is there.
pub(crate) fn mailbox_blocked_body(
    attempted: &str,
    candidates: Option<&[MailboxMatch]>,
    how_to_proceed: String,
) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "candidates": candidates
            .unwrap_or_default()
            .iter()
            .map(mailbox_candidate_json)
            .collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **A title the record cannot carry**, answered as a refusal a caller can act
/// on rather than as a protocol error.
///
/// A subject is validated: it is shown as a title rather than rendered, so it
/// takes one plain line of unformatted text. Refusing that with a bare error
/// is the shape rule 68 exists to remove — a thrown error is not a value, so
/// the model on the other end gets a failure where it should get a next move,
/// and the sentence saying what to do lands in a channel nothing branches on.
///
/// **The reason comes from the validator rather than from a copy of it here.**
/// A subject can be refused for more than one fault, and a refusal that named
/// one of them would be a catalogue that goes stale the day another is added.
pub(crate) fn subject_declined(attempted: &str, said: &MailboxError) -> CallToolResult {
    mailbox_blocked_body(
        attempted,
        None,
        format!(
            "Nothing was written: {said}. A subject is one plain line of unformatted text, \
             because it is shown as a title rather than rendered. Send the message again with \
             the same body and a plain-text subject — name a tool or a field in plain words — or \
             leave `subject` off, and the opening of the body stands in as the title."
        ),
    )
}

/// **A message jojobot cannot read, answered in the guards' own shape.** The id
/// is real — jojobot is looking straight at the record — but it cannot be read
/// as a message, so no verb will act on it until a person repairs it. A
/// successful result carrying a structured refusal, exactly like a blocked
/// write: same `status` / `wrote` / `how_to_proceed` keys, so one client-side
/// branch handles every "jojobot declined, here is what to do" answer here.
///
/// It says what is wrong and stops. It must never hand over repair steps in
/// the store's own vocabulary: that teaches an agent a shape that is never
/// its business, and sends it to fix the message somewhere it does not live.
///
/// `reason` is returned to the calling agent, so the adapter's own account of
/// what is malformed does not go in it. The detail is logged instead; see
/// [`crate::boundary`]. What an agent needs is that retrying will not help
/// and that the message is unhandled.
pub(crate) fn mailbox_quarantined(attempted: &str, reason: &str) -> CallToolResult {
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted,
        "wrote": false,
        "reason": crate::boundary::unreadable(&format!("message {attempted}"), reason),
        "how_to_proceed": format!(
            "Nothing was written, and retrying will not help — this is not a missing message. \
             jojobot can see {attempted} but cannot read it as a message, so no verb will act \
             on it. Repairing it takes a person, and it is not something you can do from here: \
             tell the operator. Until then, treat whatever it was carrying as unhandled and say \
             so rather than reporting it delivered."
        ),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// The mailbox half of [`memory_declined`]: an id that names nothing, and the
/// quarantined card that names something jojobot cannot read. Different answers
/// — one is repairable by a better id, the other only by a person on the board
/// — in one shape.
pub(crate) fn mailbox_declined(e: MailboxError) -> Result<CallToolResult, McpError> {
    match e {
        MailboxError::UnknownMessage { attempted } => Ok(mailbox_blocked_body(
            &attempted,
            None,
            format!(
                "Nothing was written. No message jojobot holds has the id '{attempted}', in any \
                 mailbox. Ids are minted by jojobot and handed back by search, read_mailbox and \
                 post_message — use an id from one of those rather than composing one."
            ),
        )),
        MailboxError::Quarantined { attempted, reason } => {
            Ok(mailbox_quarantined(&attempted, &reason))
        }
        // **A malformed name or id is a caller mistake, so it is an answer**
        // (rule 68). The payload of these two IS the value that was refused,
        // so the caller sees what jojobot read.
        MailboxError::InvalidName(ref value) | MailboxError::InvalidMessageId(ref value) => {
            Ok(mailbox_malformed(&value.clone(), &e))
        }
        // The same answer, with nothing to put in `attempted`: what this
        // refuses is the message the call carried, not something it named.
        MailboxError::InvalidMessage(_) => Ok(mailbox_malformed("", &e)),
        other => Err(mailbox_error(other)),
    }
}

/// **A call this rail cannot carry out as written**, answered as a refusal
/// with a way forward rather than as a protocol error (rule 68). A thrown
/// error is not a value: the model on the other end gets a failure where it
/// should get a next move, and the sentence saying what to do lands in a
/// channel nothing branches on.
///
/// **The reason comes from the validator rather than from a copy of it here.**
/// Each of these faults has more than one cause and the validators gain new
/// ones; a refusal that named them would be a catalogue that goes stale on the
/// day it is added to (rule 106).
fn mailbox_malformed(attempted: &str, said: &MailboxError) -> CallToolResult {
    mailbox_blocked_body(
        attempted,
        None,
        format!(
            "Nothing was written: {said}. Nothing about this needs the operator and nothing is \
             missing from the board — the call itself is what jojobot cannot carry out. Send it \
             again with that fixed."
        ),
    )
}

/// Map a domain [`MailboxError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split [`memory_error`] makes.
pub(crate) fn mailbox_error(e: MailboxError) -> McpError {
    match e {
        // **Backstops, not the intended answer.** Every one of these is a
        // caller mistake and `mailbox_declined` answers all of them as blocked
        // results with a way forward (rule 68). They are reached only by a verb
        // that surfaces an error without going through that path, and they stay
        // client errors rather than 500s for that case.
        MailboxError::InvalidName(_)
        | MailboxError::InvalidMessageId(_)
        | MailboxError::InvalidMessage(_)
        | MailboxError::UnknownMessage { .. }
        | MailboxError::Quarantined { .. } => McpError::invalid_params(e.to_string(), None),
        // Neither of these is a caller mistake, and neither is something a
        // caller can fix by calling differently: jojobot found a card on its
        // own board that belongs to another project and refused, or a write
        // failed and could not be undone, leaving a card mid-verb. Both are
        // integrity conditions on the server side that need a person.
        MailboxError::Stranded { .. } => {
            McpError::internal_error(crate::boundary::stranded("this call", &e.to_string()), None)
        }
        MailboxError::NotConfigured(msg) => {
            McpError::internal_error(format!("mailboxes not configured: {msg}"), None)
        }
        MailboxError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;

    /// **A caller mistake never leaves this rail through the error channel**
    /// (rule 68). It comes back as a blocked answer carrying what is wrong and
    /// what to do about it.
    ///
    /// Driven through the verbs a caller actually calls. The mapper answering
    /// correctly proves nothing about whether a verb reaches it: two of these
    /// faults are raised inside the store's own write, and the verb has to
    /// route that error into the declined path for any of this to hold.
    #[tokio::test]
    async fn a_malformed_write_is_an_answer_rather_than_an_error() {
        let jojobot = mailbox_handler();
        make_box(&jojobot, "inbox").await;
        let sid = as_bot(&jojobot, "epsilon");

        // A body the record cannot carry — refused inside the domain's write.
        let empty_body = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "inbox".into(),
                sid: sid.clone(),
                body: "   ".into(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // A name that is no name — refused before the box is even looked for,
        // so it is not the resemblance gate answering.
        let bad_name = jojobot
            .post_message(Parameters(PostMessageArgs {
                mailbox: "In Box!".into(),
                sid: sid.clone(),
                body: "the shipment landed".into(),
                subject: None,
                in_reply_to: None,
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // An id that is no id.
        let bad_id = jojobot
            .mark_processed(Parameters(MarkProcessedArgs {
                message_id: "not an id!".into(),
                notes: None,
                sid: Some(sid),
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // **What each answer must carry is the validator's own sentence**, so
        // the caller learns which fault it is and what the rule is. Read from
        // the validator rather than written out here: pinning the relation
        // leaves the wording free to improve, and it is also what tells these
        // answers apart from the resemblance gate's, which would satisfy a
        // bare `wrote: false` just as well.
        let said = |e: MailboxError| e.to_string();
        for (what, result, expected) in [
            (
                "an empty body",
                &empty_body,
                said(mailbox::validate_body("   ").expect_err("an empty body is refused")),
            ),
            (
                "a malformed box name",
                &bad_name,
                said(
                    mailbox::validate_mailbox_name(&MailboxName("In Box!".into()))
                        .expect_err("a name with a space is refused"),
                ),
            ),
            (
                "a malformed message id",
                &bad_id,
                said(
                    mailbox::validate_message_id(&MessageId("not an id!".into()))
                        .expect_err("an id with a space is refused"),
                ),
            ),
        ] {
            let body = blocked(result);
            assert_eq!(body["wrote"], false, "{what} wrote something: {body}");
            let advice = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                advice.contains(&expected),
                "{what} came back without the reason it was refused for.\n  wanted: \
                 {expected}\n  got: {advice}"
            );
        }
    }

    /// A stranded write's cause and rollback account are the adapter's own
    /// words about pages and rows — they must not reach the caller, same as
    /// every other store-class failure on this rail.
    #[test]
    fn a_stranded_write_does_not_carry_the_adapters_own_words() {
        let leaky_cause = "the page for gamma has no table";
        let leaky_rollback = "the row vanished from the document";
        let err = mailbox_error(MailboxError::Stranded {
            verb: "post_message".into(),
            stranded: vec!["gamma-4".into()],
            cause: leaky_cause.into(),
            rollback: leaky_rollback.into(),
        });
        assert!(
            !err.message.contains(leaky_cause) && !err.message.contains(leaky_rollback),
            "the adapter's own words crossed: {}",
            err.message
        );
        // **Not "Try once more."** A stranded write may have half-landed, so a
        // repeat could double it — the caller needs to be told not to retry,
        // never the opposite.
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
            err.message.contains("Tell the operator") || err.message.contains("tell the operator"),
            "a caller needs the way out that is actually safe: {}",
            err.message
        );
    }
}
