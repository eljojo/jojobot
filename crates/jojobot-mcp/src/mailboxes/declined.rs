//! **The mailbox context's refusals.**
//!
//! Four shapes wearing one envelope, because the way out of each differs: a
//! name that resembles a box, an id that names nothing at all, something
//! jojobot cannot read as a message — that one needs a person, not a retry —
//! and a message that is somebody else's to take delivery of. All are
//! successful results whose body says `status: "blocked"`.

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
        other => Err(mailbox_error(other)),
    }
}

/// Map a domain [`MailboxError`] to an MCP error, splitting client mistakes from
/// server-side failures — the same split [`memory_error`] makes.
pub(crate) fn mailbox_error(e: MailboxError) -> McpError {
    match e {
        MailboxError::InvalidName(_)
        | MailboxError::InvalidMessageId(_)
        | MailboxError::InvalidMessage(_)
        | MailboxError::UnknownMessage { .. }
        // Reached only if a verb other than mark_processed ever surfaces one;
        // that verb renders it as a structured result instead.
        | MailboxError::Quarantined { .. } => McpError::invalid_params(e.to_string(), None),
        // Neither of these is a caller mistake, and neither is something a
        // caller can fix by calling differently: jojobot found a card on its
        // own board that belongs to another project and refused, or a write
        // failed and could not be undone, leaving a card mid-verb. Both are
        // integrity conditions on the server side that need a person.
        MailboxError::Stranded { .. } => {
            McpError::internal_error(e.to_string(), None)
        }
        MailboxError::NotConfigured(msg) => {
            McpError::internal_error(format!("mailboxes not configured: {msg}"), None)
        }
        MailboxError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
        }
    }
}
