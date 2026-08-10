//! **What the caller should know and did not ask about.**
//!
//! A uniform block on every answer this surface returns, carrying what is true
//! FOR the caller right now. It rides beside whatever the verb answered and
//! never replaces it.
//!
//! # It is attached at the dispatch, not by the verbs
//!
//! One place, for the reason the argument gate is in one place: a verb written
//! tomorrow carries this without doing anything, and no verb can forget it.
//! **A block each verb had to add is a block that would be missing from
//! whichever verb somebody wrote in a hurry** — which is the verb whose caller
//! most needs telling.
//!
//! # One entrant, and the list is the failure mode
//!
//! It ships carrying waiting mail and nothing else. "Information when
//! relevant" is an open slot, and the way it goes wrong is a registry of
//! reportable facts that grows because growing is what registries do. A second
//! entrant earns its place on evidence that a caller acted wrongly without it.
//!
//! # It says what is true for the caller, never how jojobot knows
//!
//! How much mail is waiting is the caller's business. That jojobot read the
//! board to find out, and whether that read went well, is machinery — so a
//! board this cannot read produces **no block at all** rather than a block
//! describing the trouble. Silence here is not a claim that the box is empty:
//! nothing here is the only honest thing an unsolicited block can say, and the
//! verbs that answer FOR mail — `read_mailbox`, `start_here` — report an
//! unreadable board where a caller actually asked.

use super::*;

/// The key the block rides under. One key, so a caller reads the answer it
/// asked for and finds this beside it rather than folded into it.
const STATUS_BAR: &str = "status_bar";

/// **What a bot has for a mailbox.** One box per bot is settled, so the other
/// two answers are damage rather than variety — and they are apart from each
/// other because a bot with no box and a bot with two are different repairs.
pub(crate) enum OwnBox {
    /// The one box it owns.
    The(mailbox::MailboxName),
    /// It owns none. A bot's box opens with the bot, so this is a creation
    /// that was interrupted or a record predating the rule.
    None,
    /// It owns more than one, which nothing should be able to produce.
    Several(Vec<mailbox::MailboxName>),
    /// The board could not be read, so this says nothing about the bot.
    Unreadable,
}

impl Jojobot {
    /// Attach the block to an answer, when there is anything to say.
    ///
    /// **Nothing to say means nothing added.** A block that appeared on every
    /// answer carrying zeroes is a block a reader learns to skip, and the one
    /// time it matters is the time it gets skipped.
    pub(crate) async fn add_status_bar(&self, answered: &mut CallToolResponse, sid: Option<&str>) {
        let Some(waiting) = self.mail_waiting(sid).await else {
            return;
        };
        // Only a completed call has an answer to ride on. The other responses
        // are the protocol asking the client for something, and this surface
        // does not produce them — a block added there would be attached to a
        // question rather than to an answer.
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        let bar = serde_json::json!({ "mail_waiting": waiting });
        for block in &mut result.content {
            let Some(text) = block.as_text() else {
                continue;
            };
            // Only a JSON object has somewhere to put this. Anything else is
            // left exactly as the verb wrote it: an answer this cannot add to
            // is an answer it must not rewrite.
            let Ok(serde_json::Value::Object(mut body)) =
                serde_json::from_str::<serde_json::Value>(&text.text)
            else {
                continue;
            };
            body.insert(STATUS_BAR.to_string(), bar.clone());
            *block = ContentBlock::text(serde_json::Value::Object(body).to_string());
            return;
        }
    }

    /// **The box a bot owns**, found by its owner rather than by its name.
    ///
    /// A bot's box is named for its handle, and nothing enforces that: the box
    /// states who owns it, so ownership is the fact to read and the name is a
    /// convention a caller should never have to know.
    ///
    /// **One box per bot is settled, so anything else is damage.** Not a case
    /// to choose from and not a shape to leave room for: picking one of two
    /// would deliver half somebody's mail and report it as all of it.
    pub(crate) async fn own_box(&self, bot: &EntityId) -> OwnBox {
        let Ok(boxes) = self.mailboxes.list_mailboxes().await else {
            return OwnBox::Unreadable;
        };
        owned_box(&boxes, bot)
    }

    /// How many messages are waiting for whoever is asking, when that is worth
    /// saying.
    ///
    /// `None` covers every case where there is nothing to report, and they are
    /// deliberately one answer: no session handle, a handle that addresses no
    /// session, a caller whose box holds nothing new, and a board that could
    /// not be read. **A caller cannot act differently on any of them**, and
    /// spelling them apart here would be this block explaining jojobot to
    /// somebody who asked about something else.
    async fn mail_waiting(&self, sid: Option<&str>) -> Option<usize> {
        let asking = self.caller(sid).ok()??;
        // **Which box is theirs and what is in it are one question of the
        // board.** The listing carries both, and this block runs on every
        // answer the surface gives — so a second read here is a second read of
        // the whole message table on every call jojobot serves.
        let boxes = self.mailboxes.list_mailboxes().await.ok()?;
        let OwnBox::The(own) = owned_box(&boxes, &asking.bot) else {
            return None;
        };
        let waiting = boxes
            .iter()
            .find(|held| held.name == own)
            .map(|held| held.counts.new)?;
        (waiting > 0).then_some(waiting)
    }
}

/// **The ownership question, over a board already read.** Apart from the fetch
/// so that a caller holding the listing can answer it — and anything else the
/// same listing answers — without asking the store for the table again.
pub(crate) fn owned_box(boxes: &[mailbox::Mailbox], bot: &EntityId) -> OwnBox {
    let mut owned: Vec<mailbox::MailboxName> = boxes
        .iter()
        .filter(|held| &held.owner == bot)
        .map(|held| held.name.clone())
        .collect();
    match owned.len() {
        1 => OwnBox::The(owned.pop().expect("one box")),
        0 => OwnBox::None,
        _ => OwnBox::Several(owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mailboxes::testing::{counting_handler, owning, send};

    /// **One read of the board per answer.** The block rides on every call this
    /// surface serves, so a second read here is a second read on every call —
    /// and the two questions it asks, which box is the caller's and what is
    /// waiting in it, are both answered by the one table.
    ///
    /// Paired with the count the block reports, because a bar that read the
    /// board no times at all would satisfy a bound on reads by itself.
    #[tokio::test]
    async fn the_block_reads_the_board_once() {
        let (jojobot, board) = counting_handler();
        let asking = owning(&jojobot, "otto").await;
        owning(&jojobot, "gamma").await;
        send(&jojobot, "otto", "gamma", "your turn on the damper").await;

        let before = board.listings();
        let mut answered: CallToolResponse = CallToolResult::success(vec![ContentBlock::text(
            serde_json::json!({ "status": "ok" }).to_string(),
        )])
        .into();
        jojobot.add_status_bar(&mut answered, Some(&asking)).await;

        let CallToolResponse::Complete(result) = &answered else {
            panic!("a completed call is what the block attaches to");
        };
        let text = result
            .content
            .first()
            .and_then(|block| block.as_text())
            .expect("the answer is one text block")
            .text
            .clone();
        let body: serde_json::Value = serde_json::from_str(&text).expect("the answer is JSON");
        assert_eq!(
            body["status_bar"]["mail_waiting"], 1,
            "the block reports the one message waiting: {body}"
        );
        assert_eq!(
            board.listings() - before,
            1,
            "the block asks the board for the whole table once and derives both facts from it"
        );
    }
}
