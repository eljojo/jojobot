//! **The mailbox response vocabulary** — one record, one spelling.
//!
//! Rendered by hand for the reason the fact renderer is: `post_message`,
//! `read_mailbox` and `mark_processed` must not drift into three spellings of
//! one message. The receipt renderer is here too, and it is where the rule that
//! eliding is never silent is actually enforced.
//!
//! [`Ownership`] is here because its verb is gone. It scoped `list_mailboxes`'s
//! counts and the boot snapshot's alike; with that verb retired its one caller
//! is `orient`, which is another context's — and mailbox scoping written inside
//! orientation would be worse than machinery kept beside the vocabulary it
//! renders.

use super::*;

/// Which boxes a caller drains, and **whether jojobot could tell**.
///
/// The two are separate answers on purpose. "You drain none of these" and
/// "jojobot cannot read the store that says which you drain" produce the same
/// listing and mean opposite things, and a caller acts on both.
/// **Ownership is never unknown here, and that is why there is no flag.** This
/// used to carry a `known: bool`, because ownership was a read of Memory and
/// the mailbox listing could arrive with nobody able to say whose was whose.
/// The two are one read now: whoever renders a listing has already answered the
/// ownership question, so the flag could only ever say `true` where it appeared
/// — it was rendered inside the `Ok` arm of the very read whose `Err` arm set
/// it false.
pub(crate) struct Ownership {
    /// The boxes this caller drains. Empty when they drain none.
    mine: Vec<String>,
}

impl Ownership {
    pub(crate) fn known(mine: Vec<String>) -> Self {
        Ownership { mine }
    }

    /// Whether this caller drains this box — and so whether its counts are
    /// theirs to see. **One question, not two.** It used to be two: a box
    /// nobody drained had no queue to shield, so its counts went to everybody.
    /// A box has an owner by construction now, so that second case cannot
    /// arise.
    pub(crate) fn drains(&self, name: &str) -> bool {
        self.mine.iter().any(|m| m == name)
    }

    /// Which of the boxes actually on the board this answer counted.
    pub(crate) fn shown_for(&self, boxes: &[Mailbox]) -> Vec<String> {
        boxes
            .iter()
            .map(|b| b.name.as_str())
            .filter(|name| self.drains(name))
            .map(str::to_string)
            .collect()
    }

    /// The clause that says what this listing's counts mean, including when it
    /// cannot say.
    pub(crate) fn note(&self) -> &'static str {
        "Counts are shown for the boxes you drain. A box somebody else works is listed by \
         name only — it exists and you can post into it; what is waiting in it belongs to \
         whoever works it."
    }
}

impl Jojobot {
    /// Which boxes this caller drains, **off a listing already in hand**.
    ///
    /// **Ownership is a read of the boxes themselves, never an ACL and no
    /// longer a read of Memory**: a box states its one owner, so the answer is
    /// in the same listing being rendered. It used to be a `mailbox:` claim on
    /// the bot's own entity record, which is why this once asked the entity
    /// index — and why there was a separate `boxes_drained_by` that went and
    /// fetched a listing of its own. Two reads of one world could disagree,
    /// and the disagreement rendered as "jojobot cannot tell who drains what"
    /// beside a listing that plainly said. One read, one answer.
    ///
    /// A caller that names no bot drains nothing — the right answer for a pure
    /// sender, and for an anonymous `start_here`.
    pub(crate) fn ownership_of(
        &self,
        boxes: &[mailbox::Mailbox],
        named: Option<&EntityId>,
    ) -> Ownership {
        // **Whoever the caller says they are, and nobody by default.** There is
        // no connection to fall back to any more: a caller with no handle owns
        // nothing, which is exactly right for one that only posts.
        let bot = named.cloned();
        // **Every box has an owner now**, so the old "a box nobody owns has no
        // queue to protect" case cannot arise: there is no unclaimed box to
        // leave visible to everybody. The scoping is therefore simply what it
        // always meant — a caller sees the counts of the boxes it drains, and
        // every other box by name alone.
        let Some(bot) = bot else {
            return Ownership::known(Vec::new());
        };
        Ownership::known(
            boxes
                .iter()
                .filter(|b| b.owner == bot)
                .map(|b| b.name.to_string())
                .collect(),
        )
    }
}

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

/// **A poll's answer: what is waiting in your own box, and nothing taken.**
///
/// Built from [`mailbox_json`] rather than beside it, so the counts a poll sees
/// and the counts a boot sees are one rendering — this is the job
/// `list_mailboxes` used to do, and it landed here rather than on a verb of its
/// own so the surface arrives one tool smaller instead of one tool renamed.
///
/// **`delivered: false` is not decoration.** A caller has to be able to tell a
/// count from a delivery that happened to be empty, and the difference is
/// whether it now owes anybody anything. Same rule as every other elision here:
/// less came back, and the answer says so rather than leaving a reader to infer
/// it from a key that is not there.
pub(crate) fn counted_json(mailbox: &Mailbox) -> serde_json::Value {
    let mut body = mailbox_json(mailbox);
    if let Some(obj) = body.as_object_mut() {
        // **`mailbox`, the spelling the delivery beside it uses.** `name` is
        // right in a LISTING of boxes, where the field distinguishes one row
        // from the next; here it is the same single answer `delivery_json`
        // gives, from the same verb, about the same box — and one verb calling
        // one thing two names is the drift this file exists to stop.
        obj.remove("name");
        obj.insert("mailbox".into(), mailbox.name.as_str().into());
        obj.insert("delivered".into(), false.into());
        obj.insert(
            "note".into(),
            "Nothing was delivered and nothing is owed: every message here is still waiting \
             exactly as it was. Call read_mailbox again without counts_only to take delivery."
                .into(),
        );
    }
    body
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
