//! **What an answer could not work out because a read failed.** A read that
//! fails never reads as nothing (rule 130): a note an answer would have carried,
//! or a count it would have made, is not left off or computed from a default
//! without saying so. Each step that reads something besides the instance
//! record names here what it left, and the answer carries one block saying so.
//! The instance record has its own, [`super::instance_unread`]; this is the same
//! mechanism for every other read (rule 498).

use std::sync::Mutex;

/// What one answer left unworked, collected across its steps. Shared by
/// reference, so a step deep in a verb can add to it.
#[derive(Default)]
pub(crate) struct ReadsLeft(Mutex<Vec<(&'static str, String)>>);

impl ReadsLeft {
    /// **Record that the answer could not work `what` out**, because `read`
    /// failed, after logging the failure.
    pub(crate) fn left(
        &self,
        what: &'static str,
        read: impl std::fmt::Display,
        error: &jojobot_domain::memory::MemoryError,
    ) {
        tracing::warn!(%error, what, read = %read, "a read failed, so an answer left something out");
        self.left_for(what, read);
    }

    /// **Record a note that was left for want of a read already named**, without
    /// logging a failure again. **A pair already held is not held twice**: two
    /// measures of one answer that fail on the same read name it once.
    pub(crate) fn left_for(&self, what: &'static str, read: impl std::fmt::Display) {
        let entry = (what, read.to_string());
        let mut held = self.0.lock().expect("never poisoned");
        if !held.contains(&entry) {
            held.push(entry);
        }
    }

    /// **The block the answer carries**, or nothing when nothing was left.
    pub(crate) fn block(&self) -> Option<serde_json::Value> {
        let left = self.0.lock().expect("never poisoned");
        (!left.is_empty()).then(|| {
            serde_json::json!({
                "not_worked_out": left
                    .iter()
                    .map(|(what, read)| serde_json::json!({"what": what, "read": read}))
                    .collect::<Vec<_>>(),
                "note": "A read failed, so what not_worked_out names was left off, or worked \
                         out from a default, rather than worked out from what is stored. \
                         The answer is otherwise as it was. Ask again to see whether it reads.",
            })
        })
    }

    /// **Put the block on a receipt** when anything was left. A receipt that already
    /// carries the block, from another step's collector, gains these entries beside
    /// its own, so two collectors never overwrite each other.
    pub(crate) fn note(&self, body: &mut serde_json::Value) {
        let (Some(block), Some(object)) = (self.block(), body.as_object_mut()) else {
            return;
        };
        match object.get_mut(KEY) {
            Some(held) => {
                if let (Some(held), Some(more)) = (
                    held["not_worked_out"].as_array_mut(),
                    block["not_worked_out"].as_array(),
                ) {
                    held.extend(more.iter().cloned());
                }
            }
            None => {
                object.insert(KEY.into(), block);
            }
        }
    }
}

/// The key the block rides under, beside `instance_unread`.
pub(crate) const KEY: &str = "reads_left";

/// **The count of accounts a capture's receipt gives**: left off, since the claims
/// of the thing could not be read.
pub(crate) const ACCOUNT_COUNT: &str = "account_count";
/// **Which keys of a record reached the thing**: left unsaid, since the store could
/// not say which it holds off the thing.
pub(crate) const REACH_NOTE: &str = "reach_note";
/// **The role a bot claims by default**: not claimed, since the bot's fields could
/// not be read.
pub(crate) const CARRIED_ROLE: &str = "carried_role";
/// **The seat count of a boot**: worked out at the default, since the bot's fields
/// could not be read.
pub(crate) const SEAT_COUNT: &str = "seat_count";
/// **The note on a status word the board does not hold**: left off, since the
/// board that governs the thing could not be read.
pub(crate) const BOARD_WORD_NOTE: &str = "status_word_note";
/// **The keys that left the thing when a record came off it**: left off, since what
/// the thing held could not be read.
pub(crate) const KEYS_LEFT_NOTE: &str = "keys_left_note";
/// **The default policy a rhythm got**: left off, since what it holds could not be read.
pub(crate) const POLICY_NOTE: &str = "policy_note";
/// **The emergency-reserve note of a thought**: left off, since the room could not be read.
pub(crate) const RESERVE_NOTE: &str = "reserve_note";
/// **The emergency-reserve note of a record**: left off, since the thing's fields could not be read.
pub(crate) const RECORD_RESERVE_NOTE: &str = "record_reserve_note";
/// **The boot floor a write was held to**: not measured, since the bot's fields,
/// claims or boot could not be read.
pub(crate) const BOOT_FLOOR: &str = "boot_floor";
