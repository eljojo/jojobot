//! **What a write left unworked because the instance record could not be read.**
//!
//! A write never fails over the instance record (rule 130): the claim lands.
//! What the write owes its caller is the fact that part of its bookkeeping was
//! done without the operator's status words, so an absent note or an unmoved
//! day never reads as "the instance mapped nothing". Each write path that reads
//! the record names here what it could not work out, and the receipt carries
//! one block saying so (rule 498: one place for the three paths).

use std::collections::BTreeSet;
use std::sync::Mutex;

/// **The due day**: worked out as if the instance mapped no status words.
pub(crate) const DUE_ON: &str = "due_on";
/// **The bot's `working_on`**: not filled, since whether the word means `now`
/// could not be read.
pub(crate) const WORKING_ON: &str = "working_on";
/// **The note on a status word the thing does not hold**: left off, since
/// whether the instance maps the word could not be read.
pub(crate) const STATUS_WORD_NOTE: &str = "status_word_note";

/// The receipt key, beside the precedent `linked_unread` on a tick.
const KEY: &str = "instance_unread";

/// **What one write could not work out**, collected across the write's own
/// steps. Shared by reference, so a step deep in a verb can add to it.
#[derive(Default)]
pub(crate) struct InstanceUnread(Mutex<BTreeSet<&'static str>>);

impl InstanceUnread {
    /// **Record that the write could not work `what` out**, after logging the
    /// read that failed.
    pub(crate) fn left(&self, what: &'static str, error: &jojobot_domain::memory::MemoryError) {
        tracing::warn!(%error, what, "the instance record could not be read for a write");
        self.0.lock().expect("never poisoned").insert(what);
    }

    /// **Put the block on the receipt** when the write left anything unworked,
    /// and nothing when it did not.
    pub(crate) fn note(&self, body: &mut serde_json::Value) {
        let left = self.0.lock().expect("never poisoned");
        if left.is_empty() {
            return;
        }
        let record = crate::orientation::instance_zone::INSTANCE_RECORD;
        if let Some(object) = body.as_object_mut() {
            object.insert(
                KEY.into(),
                serde_json::json!({
                    "record": record,
                    "not_worked_out": left.iter().collect::<Vec<_>>(),
                    "note": format!(
                        "The write landed. {record} could not be read, so what \
                         not_worked_out names was worked out as if it maps no status \
                         words, or left off. recall {record} to see whether it reads."
                    ),
                }),
            );
        }
    }
}
