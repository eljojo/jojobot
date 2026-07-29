//! **The mailbox response vocabulary** — one record, one spelling.
//!
//! Rendered by hand for the reason the fact renderer is: `post_message`,
//! `read_mailbox` and `mark_processed` must not drift into three spellings of
//! one message. The receipt renderer is here too, and it is where the rule that
//! eliding is never silent is actually enforced.

use super::*;

/// **The owner named does not exist.** Reported with what Memory's own screen
/// found, so a typo comes back with the handle it probably meant.
pub(crate) fn unknown_owner(attempted: &EntityId, candidates: &[EntityMatch]) -> CallToolResult {
    let nearest: Vec<&str> = candidates.iter().map(|c| c.handle.as_str()).collect();
    let how = if nearest.is_empty() {
        format!(
            "Nothing was created. There is no entity '{attempted}', and a mailbox is created FOR \
             somebody — so jojobot has nobody to file this box under. Create the owner with \
             add_entity first, then create the box."
        )
    } else {
        format!(
            "Nothing was created. There is no entity '{attempted}'. Did you mean {}? A mailbox is \
             created FOR somebody, so the owner has to exist first — confirm which one it is, or \
             add_entity the new one and then create the box.",
            nearest.join(", ")
        )
    };
    blocked_body(attempted, candidates, how)
}

/// A mailbox on the wire: its name, what is in it per state, and what is in it
/// that could not be read — a caller must see "N unreadable" rather than
/// nothing, because a quarantined card is invisible to every other verb.
pub(crate) fn mailbox_json(mailbox: &Mailbox) -> serde_json::Value {
    serde_json::json!({
        "name": mailbox.name.as_str(),
        "counts": {
            "new": mailbox.counts.new,
            "read": mailbox.counts.read,
            "processed": mailbox.counts.processed,
            "total": mailbox.counts.total(),
        },
        "quarantined": quarantined_json(mailbox),
    })
}

/// What is on a box that jojobot cannot read as a message.
///
/// **Rendered apart from the counts, because it is scoped differently.** Counts
/// are a queue and belong to whoever drains it; something unreadable is a fault
/// no verb can act on, and the caller who most needs to see it is a sender —
/// somebody who does not drain this box, and who would otherwise read the
/// silence as "my message was never sent".
///
/// **`ids`, not `card_ids`.** The old spelling was retired vocabulary from when
/// mail lived on a task board, and it shipped on every mailbox payload
/// including a boot — so a fresh session's first read of jojobot taught it that
/// messages are cards, which is both wrong and not its business. What the field
/// holds is the ids a person needs in order to repair these by hand, and that
/// is now what it is called.
pub(crate) fn quarantined_json(mailbox: &Mailbox) -> serde_json::Value {
    serde_json::json!({
        "count": mailbox.quarantined.len(),
        "ids": mailbox.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
    })
}

/// A message on the wire. Rendered by hand rather than derived, so
/// `post_message`, `read_mailbox` and `mark_processed` cannot drift into three
/// spellings of one record — the same rule the fact renderer follows.
pub(crate) fn message_json(message: &Message) -> serde_json::Value {
    serde_json::json!({
        "id": message.id.as_str(),
        "mailbox": message.mailbox.as_str(),
        "sender": message.sender,
        "sent_at": message.sent_at.to_string(),
        // Null for every message posted before there was a field for one, and
        // for every one posted without it since. Absent-as-null rather than an
        // omitted key: a reader must not have to branch on whether it is there.
        "subject": message.subject,
        "body": message.body,
        "state": message.state.as_token(),
        "notes": message.notes,
        // Null for a message that answers nothing, which is most of them. A
        // link, never a status: it says these two are one exchange and nothing
        // about whether either has been handled.
        "in_reply_to": message.in_reply_to.as_ref().map(|id| id.as_str()),
    })
}

/// A message **without its body shipped back** — the whole record otherwise,
/// plus enough of the body to recognize which message this is.
///
/// **Eliding is never silent.** `body_elided` is always present and always
/// true here, `body_bytes` is the exact size of what is stored, and
/// `how_to_read` names the verb that hands the body over. A reader that has to
/// infer from a missing key whether a body was withheld or empty is a reader
/// that will eventually infer wrong.
///
/// The write is still verified: the store's read-back invariant means a body
/// that did not survive storage is an error rather than a success with mangled
/// bytes, so what the full echo used to prove is proven server-side. What the
/// echo added was shipping a 4-8 KB report back to the one caller who wrote it.
pub(crate) fn message_receipt_json(message: &Message, how_to_read: &str) -> serde_json::Value {
    let mut body = message_json(message);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("body".into(), serde_json::Value::Null);
        obj.insert("body_elided".into(), true.into());
        obj.insert("body_bytes".into(), message.body.len().into());
        obj.insert(
            "body_head".into(),
            text::BODY_DIGEST.render(&message.body).into(),
        );
        obj.insert("how_to_read".into(), how_to_read.into());
    }
    body
}

/// One delivered message: the whole record, plus whether a previous read had
/// already handed it over.
pub(crate) fn delivered_json(delivered: &Delivered) -> serde_json::Value {
    let mut body = message_json(&delivered.message);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("seen_before".into(), delivered.seen_before.into());
    }
    body
}

/// A whole delivery.
///
/// **`new_only` changes what is shipped, never what is owed.** Every message
/// the delivery covers is here either way, counted and flagged the same, and
/// every one of them still has to be marked processed — the crash contract is
/// exactly as it was. What it drops is the BODIES of the leftovers, which is
/// the whole cost of polling a box that holds a message somebody is
/// deliberately keeping open: the report stays unprocessed on purpose until its
/// round closes, and every poll in between was re-shipping it in full.
///
/// The elision is announced per message rather than once for the delivery,
/// because a reader walking the list must not have to remember a flag from the
/// envelope to know what it is looking at.
pub(crate) fn delivery_json(delivery: &Delivery, new_only: bool) -> serde_json::Value {
    serde_json::json!({
        "mailbox": delivery.mailbox.as_str(),
        "count": delivery.messages.len(),
        "new_only": new_only,
        "messages": delivery
            .messages
            .iter()
            .map(|d| if new_only && d.seen_before {
                let mut body = message_receipt_json(
                    &d.message,
                    "an earlier read already handed you this one. read_message returns it in \
                     full, or read_mailbox without new_only",
                );
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("seen_before".into(), true.into());
                }
                body
            } else {
                delivered_json(d)
            })
            .collect::<Vec<_>>(),
    })
}

/// One of the mailbox guard's candidates on the wire.
pub(crate) fn mailbox_candidate_json(candidate: &MailboxMatch) -> serde_json::Value {
    serde_json::json!({
        "name": candidate.name.as_str(),
        "reason": match candidate.reason {
            mailbox::guard::MatchReason::Exact => "exact",
            mailbox::guard::MatchReason::Near => "near",
            mailbox::guard::MatchReason::Contains => "contains",
        },
    })
}
