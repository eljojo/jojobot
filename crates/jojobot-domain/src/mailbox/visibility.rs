//! Mail visibility follows the current org chart, in every message state.

use std::collections::{BTreeMap, BTreeSet};

use super::{Mailbox, MailboxName};
use crate::memory::{EntityId, EntityKind, manager_in};

/// The current caller and chart, used by one per-message visibility decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VisibleBoxes {
    caller: Option<EntityId>,
    private_boxes: BTreeSet<MailboxName>,
    bot_boxes: BTreeSet<MailboxName>,
    allowed: BTreeSet<MailboxName>,
    /// A grader reads every bot box; see [`every_bot_box`].
    reads_every_bot_box: bool,
    /// Ancestor boxes, to explain an above-chart refusal without another walk.
    pub above: BTreeSet<MailboxName>,
}

/// What one message may expose to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageVisibility {
    /// The full message, including a digest when bodies were not requested.
    Readable,
    /// Only the existing private or unlisted-box receipt, never message text.
    Receipt,
    /// No message metadata or text; only a withheld count.
    Withheld,
}

impl VisibleBoxes {
    /// **Whether the caller reads the inside of this box**: its own, one below
    /// it, or any bot box for a grading handler. An author also reads its own
    /// message in a box above it (see [`message`](Self::message)), and this is
    /// what tells that read from the others: the recipient's notes are not for it.
    pub fn reads_inside(&self, mailbox: &MailboxName) -> bool {
        self.reads_every_bot_box || self.allowed.contains(mailbox)
    }

    /// One predicate for every read, from the message's box and sender.
    /// An unknown sender cannot establish an author's outbox entitlement.
    pub fn message(&self, mailbox: &MailboxName, sender: Option<&str>) -> MessageVisibility {
        if self.private_boxes.contains(mailbox) {
            return MessageVisibility::Receipt;
        }
        if self.reads_every_bot_box {
            return MessageVisibility::Readable;
        }
        let Some(caller) = &self.caller else {
            return MessageVisibility::Withheld;
        };
        let authored = sender == Some(caller.as_str());
        if !self.bot_boxes.contains(mailbox) {
            return if authored {
                MessageVisibility::Receipt
            } else {
                MessageVisibility::Withheld
            };
        }
        if authored || self.allowed.contains(mailbox) {
            MessageVisibility::Readable
        } else {
            MessageVisibility::Withheld
        }
    }
}

/// Gather own and descendant boxes for the per-message predicate.
/// Person boxes never expose their contents (rules 139 and 400).
///
/// The caller supplies the current folded `reports_to` values and board on
/// every read. No store, cached permission or message state decides visibility.
pub fn visible_boxes(
    caller: Option<&EntityId>,
    reports_to: &BTreeMap<EntityId, String>,
    board: &[Mailbox],
) -> VisibleBoxes {
    let mut visible = VisibleBoxes {
        private_boxes: board
            .iter()
            .filter(|mailbox| mailbox.is_private())
            .map(|mailbox| mailbox.name.clone())
            .collect(),
        ..Default::default()
    };
    let Some(caller) = caller.filter(|caller| caller.kind() == Some(EntityKind::BOT)) else {
        return visible;
    };
    visible.caller = Some(caller.clone());
    let above = ancestors(caller, reports_to);
    for mailbox in board {
        if mailbox.owner.kind() != Some(EntityKind::BOT) {
            continue;
        }
        visible.bot_boxes.insert(mailbox.name.clone());
        if mailbox.owner == *caller {
            visible.allowed.insert(mailbox.name.clone());
        } else if above.contains(&mailbox.owner) {
            // Even a damaged cycle must not turn an ancestor into a report.
            visible.above.insert(mailbox.name.clone());
        } else if ancestors(&mailbox.owner, reports_to).contains(caller) {
            visible.allowed.insert(mailbox.name.clone());
        }
    }
    visible
}

/// **What a room's grader sees**: every message in every bot box, and a private
/// box only as a receipt. The grader has no caller and no chart position, so no
/// chart rule can place it; it is built only by the compiled-in room harness.
pub fn every_bot_box(board: &[Mailbox]) -> VisibleBoxes {
    VisibleBoxes {
        private_boxes: board
            .iter()
            .filter(|mailbox| mailbox.is_private())
            .map(|mailbox| mailbox.name.clone())
            .collect(),
        reads_every_bot_box: true,
        ..Default::default()
    }
}

fn ancestors(bot: &EntityId, reports_to: &BTreeMap<EntityId, String>) -> BTreeSet<EntityId> {
    let mut seen = BTreeSet::from([bot.clone()]);
    let mut above = BTreeSet::new();
    let mut at = bot.clone();
    while let Some(manager) = reports_to.get(&at).and_then(|held| manager_in(held)) {
        if manager.kind() != Some(EntityKind::BOT) || !seen.insert(manager.clone()) {
            break;
        }
        above.insert(manager.clone());
        at = manager;
    }
    above
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_of(kind: &str, owner: &str) -> Mailbox {
        // The kind of an id is read from the loaded set.
        crate::memory::kinds::load_shipped();
        Mailbox {
            name: MailboxName(owner.to_string()),
            owner: EntityId(format!("{kind}:{owner}")),
            counts: Default::default(),
            quarantined: Vec::new(),
        }
    }

    /// **A chart that loops never turns a manager into a report.** A loop is
    /// stored damage (a `reports_to` written before the cycle guard), and
    /// without the ancestor check each bot of the loop would be shown the box of
    /// the bot above it.
    #[test]
    fn a_loop_in_the_chart_shows_each_bot_its_own_box_and_never_the_one_above() {
        let chart = BTreeMap::from([
            (EntityId("bot:alpha".into()), "bot:beta".to_string()),
            (EntityId("bot:beta".into()), "bot:alpha".to_string()),
        ]);
        let board = [box_of("bot", "alpha"), box_of("bot", "beta")];
        for (caller, own, other) in [("alpha", "alpha", "beta"), ("beta", "beta", "alpha")] {
            let seen = visible_boxes(Some(&EntityId(format!("bot:{caller}"))), &chart, &board);
            assert_eq!(
                seen.message(&MailboxName(own.into()), Some("bot:gamma")),
                MessageVisibility::Readable,
                "{caller} reads its own box"
            );
            assert_eq!(
                seen.message(&MailboxName(other.into()), Some("bot:gamma")),
                MessageVisibility::Withheld,
                "{caller} does not read the box above it"
            );
            assert!(
                seen.above.contains(&MailboxName(other.into())),
                "{caller} is told the box is above it"
            );
        }
    }

    /// **A grader reads every bot box and a person's box only as a receipt.**
    #[test]
    fn a_grader_reads_every_bot_box_and_no_private_one() {
        let board = [
            box_of("bot", "alpha"),
            box_of("bot", "beta"),
            box_of("person", "milhouse"),
        ];
        let seen = every_bot_box(&board);
        for bot in ["alpha", "beta"] {
            assert_eq!(
                seen.message(&MailboxName(bot.into()), Some("bot:gamma")),
                MessageVisibility::Readable
            );
        }
        assert_eq!(
            seen.message(&MailboxName("milhouse".into()), Some("bot:gamma")),
            MessageVisibility::Receipt
        );
    }
}
