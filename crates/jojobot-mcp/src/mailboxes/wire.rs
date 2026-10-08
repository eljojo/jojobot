//! **The mailbox response vocabulary** — one record, one spelling.
//!
//! Rendered by hand for the reason the fact renderer is: `post_message`,
//! `read_mailbox` and `mark_processed` must not drift into three spellings of
//! one message. The receipt renderer is here too, and it is where the rule that
//! eliding is never silent is actually enforced.
//!
//! [`Ownership`] lives here, not in `orient`: it scopes both the boot
//! snapshot and a poll's counts alike, and mailbox scoping written inside
//! orientation would be worse than machinery kept beside the vocabulary it
//! renders.

use super::*;

/// Which boxes a caller drains, and **whether jojobot could tell**.
///
/// The two are separate answers on purpose. "You drain none of these" and
/// "jojobot cannot read the store that says which you drain" produce the same
/// listing and mean opposite things, and a caller acts on both.
/// Ownership is never unknown here, so there is no flag for it: whoever
/// renders a listing has already answered the ownership question by
/// construction.
pub(crate) struct Ownership {
    /// The boxes this caller drains. Empty when they drain none.
    mine: Vec<String>,
}

impl Ownership {
    pub(crate) fn known(mine: Vec<String>) -> Self {
        Ownership { mine }
    }

    /// Whether this caller drains this box — and so whether its counts are
    /// theirs to see. One question, not two, because every box has an owner
    /// by construction: there is no "nobody drains it, show counts to
    /// everybody" case.
    pub(crate) fn drains(&self, name: &str) -> bool {
        self.mine.iter().any(|m| m == name)
    }
}

impl Jojobot {
    /// Which boxes this caller drains, **off a listing already in hand**.
    ///
    /// Ownership is a read of the boxes themselves, never an ACL and never a
    /// read of Memory: a box states its one owner, so the answer is in the
    /// same listing being rendered. Never split this into two reads of one
    /// world — they can disagree, rendering as "jojobot cannot tell who
    /// drains what" beside a listing that plainly said.
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
        // Every box has an owner, so there is no unclaimed box that should be
        // visible to everybody: a caller sees counts only for boxes it
        // drains, and every other box by name alone.
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

/// **A bot holding more than one box, where a box would otherwise go.** One
/// box per bot is settled, so this is damage: it names every box, weighs none
/// of them, and says the repair takes a person — which of them is the real one
/// is not something jojobot can work out.
///
/// One rendering, because a boot answers this question TWICE — once for the
/// caller's own identity and once for its entry on the board — and the two
/// halves of one payload disagreeing about whether the mail was measured is
/// worse than either half alone. The counts key is present and null with
/// `counts_elided` beside it, the same way every other withheld count on this
/// surface is: eliding is never silent.
pub(crate) fn several_boxes_json<'a>(
    boxes: impl IntoIterator<Item = &'a Mailbox>,
) -> serde_json::Value {
    let named: Vec<&str> = boxes.into_iter().map(|b| b.name.as_str()).collect();
    serde_json::json!({
        "counts": serde_json::Value::Null,
        "counts_elided": true,
        "damage": format!(
            "owns more than one mailbox ({}), and one bot has exactly one. jojobot will not \
             weigh half of somebody's mail as though it were all of it. It needs a person.",
            named.join(", "),
        ),
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
/// This field is named `ids`, never `card_ids` or other board vocabulary — it
/// must never teach a fresh session that messages are cards. What it holds
/// is the ids a person needs to repair these by hand.
pub(crate) fn quarantined_json(mailbox: &Mailbox) -> serde_json::Value {
    serde_json::json!({
        "count": mailbox.quarantined.len(),
        "ids": mailbox.quarantined.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
    })
}

/// **A poll's answer: what is waiting in your own box, and nothing taken.**
///
/// Built from [`mailbox_json`] rather than beside it, so the counts a poll
/// sees and the counts a boot sees are one rendering. Kept here rather than
/// as its own verb, so the surface stays one tool smaller instead of gaining
/// a renamed duplicate.
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
        // **How it left `new`, for the sender who is asking whether anybody
        // looked.** `reading` means somebody opened their box or took this
        // message by id; `posting` means it came back with the answer to
        // something they posted, so it reached them and nobody went looking.
        //
        // Null while it is still waiting, and null for anything delivered
        // before there was a record of how. **Neither may be read as "nobody
        // looked"** — null says nothing was recorded, never that nothing
        // happened.
        "taken_by": message.taken_by.map(|taken| taken.as_token()),
        // **What was genuinely waiting in the SENDER's own box when they sent
        // this** — read at send time, before this message could be read by
        // anyone, so a claim in the body about the sender's own mail is
        // checkable against a number nobody transcribed from memory. Null for
        // every message posted before this field existed, and for one jojobot
        // could not determine at send time — neither is "zero were waiting".
        "sender_mail_waiting_at_send": message.sender_mail_waiting_at_send,
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
/// **`how_to_read` is optional because a LIST says it once.** One sentence
/// repeated on every item of a hundred-item answer is most of that answer, and
/// it teaches the reader nothing after the first. A receipt for a single write
/// carries its own; a list carries one beside the list.
///
/// The write is still verified server-side: the store's read-back invariant
/// means a body that did not survive storage is an error, not a mangled
/// success. Dropping the full echo only drops the 4-8 KB report shipped back
/// to the writer.
pub(crate) fn message_receipt_json(
    message: &Message,
    how_to_read: Option<&str>,
) -> serde_json::Value {
    let mut body = message_json(message);
    elide_body(&mut body, message);
    if let Some(how_to_read) = how_to_read
        && let Some(obj) = body.as_object_mut()
    {
        obj.insert("how_to_read".into(), how_to_read.into());
    }
    body
}

/// **Put a rendered message's body aside, saying so.** The body becomes null,
/// `body_elided` is true, `body_bytes` is the exact size of what is stored and
/// `body_head` is enough of it to tell which message this is. One function for
/// the receipt of a write and for a body that did not fit under the answer
/// ceiling, so the two cannot flag it differently.
pub(crate) fn elide_body(rendered: &mut serde_json::Value, message: &Message) {
    if let Some(obj) = rendered.as_object_mut() {
        obj.insert("body".into(), serde_json::Value::Null);
        obj.insert("body_elided".into(), true.into());
        obj.insert("body_bytes".into(), message.body.len().into());
        obj.insert(
            "body_head".into(),
            text::BODY_DIGEST.render(&message.body).into(),
        );
    }
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

impl Jojobot {
    /// **Whether `message` was posted by a different run than `viewer`'s
    /// own**, and if so, by which — and whether that run has since ended,
    /// read cheaply through [`Sessions::read_session`] rather than a listing
    /// of every run a bot has had. `None` when the message carries no stamp
    /// at all, or the stamp names `viewer` itself: neither is "a different
    /// run", and both must read the same as the other.
    async fn other_run(
        &self,
        message: &Message,
        viewer: Option<&SessionId>,
    ) -> Option<(String, Option<bool>)> {
        let posted = message.posted_by_session.as_deref()?;
        if viewer.is_some_and(|card| card.as_str() == posted) {
            return None;
        }
        let ended = self
            .sessions
            .read_session(&SessionId(posted.to_string()))
            .await
            .ok()
            .map(|session| session.state.is_terminal());
        Some((posted.to_string(), ended))
    }

    /// **Mark every message in a rendered delivery that a different run
    /// posted.** `rendered` must be [`delivery_json`]'s own output, whose
    /// `messages` array sits in the same order as `delivery.messages` — the
    /// pairing this walks by zipping the two.
    pub(crate) async fn mark_other_runs(
        &self,
        rendered: &mut serde_json::Value,
        delivery: &Delivery,
        viewer: Option<&SessionId>,
    ) {
        let Some(messages) = rendered.get_mut("messages").and_then(|m| m.as_array_mut()) else {
            return;
        };
        for (json, delivered) in messages.iter_mut().zip(&delivery.messages) {
            if let Some((run, ended)) = self.other_run(&delivered.message, viewer).await {
                note_written_by_other_run(json, &run, ended);
            }
        }
    }

    /// The single-message form [`read_message`] and [`post_message`]'s own
    /// receipt need, over a rendered message JSON rather than a whole
    /// delivery.
    pub(crate) async fn mark_other_run(
        &self,
        rendered: &mut serde_json::Value,
        message: &Message,
        viewer: Option<&SessionId>,
    ) {
        if let Some((run, ended)) = self.other_run(message, viewer).await {
            note_written_by_other_run(rendered, &run, ended);
        }
    }
}

/// **Name the run that posted this, when it was not the run reading it now.**
/// `run` is `posted_by_session`'s own value; `ended` is whether that run has
/// since ended, when the store could say so cheaply enough to ask on every
/// message in a delivery — `None` when it could not. Absent from the wire
/// entirely on a message this does not apply to, never present-and-null: the
/// key's mere presence is what a reader branches on.
pub(crate) fn note_written_by_other_run(
    body: &mut serde_json::Value,
    run: &str,
    ended: Option<bool>,
) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(
        "written_by_other_run".into(),
        serde_json::json!({ "run": run, "ended": ended }),
    );
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
            .map(delivered_json)
            .collect::<Vec<_>>(),
    })
}

/// **Set the leftovers aside when a read asks for the news only.** A message a
/// previous read already handed over stays owed, but its envelope is not shipped
/// again: the read returns the fresh mail, and the leftovers come back as a
/// separate list for [`note_leftovers`] to name. A read that asks for everything
/// (`new_only` false, the recovery read) gets every message whole.
pub(crate) fn split_leftovers(delivery: Delivery, new_only: bool) -> (Delivery, Vec<Delivered>) {
    if !new_only {
        return (delivery, Vec::new());
    }
    let (leftovers, fresh): (Vec<_>, Vec<_>) = delivery
        .messages
        .into_iter()
        .partition(|delivered| delivered.seen_before);
    (
        Delivery {
            mailbox: delivery.mailbox,
            messages: fresh,
        },
        leftovers,
    )
}

/// **Name the leftovers a read did not ship again**: how many, which ids, and the
/// call that returns them. Added only when there are some, by the one function
/// both `read_mailbox` and `post_message` use, so the two cannot describe the same
/// mail differently.
pub(crate) fn note_leftovers(rendered: &mut serde_json::Value, leftovers: &[Delivered]) {
    if leftovers.is_empty() {
        return;
    }
    rendered["leftovers"] = serde_json::json!({
        "count": leftovers.len(),
        "ids": leftovers
            .iter()
            .map(|delivered| delivered.message.id.as_str())
            .collect::<Vec<_>>(),
        "how_to_read": "mail an earlier read already handed you, still owed until you mark it \
                        processed. read_message returns one by id, or read_mailbox with new_only \
                        false returns them all, flagged seen_before",
    });
}

/// **Fit a delivery's messages under the answer ceiling, and take none of them
/// back.** Every message the delivery covers was taken before this runs, so this
/// only decides how much of each the answer carries. In order, oldest first: the
/// bodies that fit come whole; from the first that does not, a message comes as
/// an envelope with its body left out and flagged (`body_elided`, `body_bytes`,
/// `body_head`, the shape a write's receipt has); and a message that does not fit
/// even so is named by id in `not_shown`. `read_message` returns any of them
/// whole, and the next read names the ones this one took as leftovers.
///
/// `beside` is the size of everything else the answer carries: a read's
/// delivery is the whole answer and passes none, and the delivery that rides on
/// a post passes the size of the post's own receipt, which it shares the
/// ceiling with.
///
/// Silent when everything fits: the answer is untouched and carries neither key.
pub(crate) fn fit_delivery(rendered: &mut serde_json::Value, delivery: &Delivery, beside: usize) {
    use jojobot_domain::text::ANSWER_CEILING;
    let Some(messages) = rendered
        .get_mut("messages")
        .and_then(|m| m.as_array_mut())
        .map(std::mem::take)
    else {
        return;
    };
    let size = |json: &serde_json::Value| json.to_string().chars().count() + 1;
    let rest = rendered.to_string().chars().count() + beside + crate::answer::STATUS_BAR_ROOM;
    if rest + messages.iter().map(size).sum::<usize>() <= ANSWER_CEILING {
        rendered["messages"] = messages.into();
        return;
    }
    let ids: Vec<&str> = delivery
        .messages
        .iter()
        .map(|delivered| delivered.message.id.as_str())
        .collect();
    // **Everything a cut adds is measured at its widest**: the block that names
    // the messages left out, naming every one of them, and the sentence that
    // says why, each with the key it rides under.
    let reserve = serde_json::json!({
        "not_shown": not_shown_ids(&ids),
        "how_to_read": HOW_TO_READ_A_CUT,
    })
    .to_string()
    .chars()
    .count();
    let mut room = ANSWER_CEILING.saturating_sub(rest + reserve);
    let mut shipped: Vec<serde_json::Value> = Vec::new();
    let mut named: Vec<&str> = Vec::new();
    let mut whole_fits = true;
    let mut elided = false;
    for (mut json, delivered) in messages.into_iter().zip(&delivery.messages) {
        if whole_fits && size(&json) <= room {
            room -= size(&json);
            shipped.push(json);
            continue;
        }
        whole_fits = false;
        elide_body(&mut json, &delivered.message);
        if named.is_empty() && size(&json) <= room {
            room -= size(&json);
            elided = true;
            shipped.push(json);
        } else {
            named.push(delivered.message.id.as_str());
        }
    }
    rendered["count"] = shipped.len().into();
    rendered["messages"] = shipped.into();
    if !named.is_empty() {
        rendered["not_shown"] = not_shown_ids(&named);
    }
    if elided || !named.is_empty() {
        rendered["how_to_read"] = HOW_TO_READ_A_CUT.into();
    }
}

/// What a delivery says when it was cut to fit the ceiling.
const HOW_TO_READ_A_CUT: &str = "every message was taken, and a body that did not fit under the \
    answer ceiling is left out: read_message returns one whole by id";

/// What an answer says of the messages it took and did not carry: how many, and
/// which. The same name and the same `count` as the block a list cut at the
/// ceiling carries.
fn not_shown_ids(ids: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "count": ids.len(),
        "ids": ids,
        "how_to_proceed": "these were taken with the rest and are owed like them: read_message \
                           returns one by id, and the next read_mailbox names them as leftovers",
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
