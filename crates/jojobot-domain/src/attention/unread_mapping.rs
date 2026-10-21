//! **What a write does to the stored due moment when the instance's record of
//! status words could not be read** (rule 130, rule 598).
//!
//! Two callers face this: a write that carries on over an unreadable record, and
//! the pass that stamps the day a thing became late. They answer from this one
//! function, so they cannot disagree about what the unread words decide.
//!
//! **The words matter for one kind of thing and one kind of status.** A work item
//! or a project is finished at `done`, and an instance may map a word of its own
//! to `done`. A shipped word means itself whatever the instance says, so a thing
//! whose status is absent or a shipped word is judged the same with the words
//! and without them. Every other kind owes by its dates alone.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{Carrier, DueMove, moved_due_moment_as};

/// **Whether the unread words could change what this thing owes**: a work item
/// or a project whose status is a word the shipped five do not name, which the
/// instance may map to `done`.
pub fn unread_mapping_could_decide(kind: &str, fields: &BTreeMap<String, String>) -> bool {
    crate::memory::kinds::holds_columns(kind)
        && fields
            .get(crate::memory::kinds::STATUS)
            .is_some_and(|word| crate::memory::kinds::meaning_of(word, &BTreeMap::new()).is_none())
}

/// **[`moved_due_moment_as`] with the instance's words unread.** `Some` is the
/// answer the words could not change, the one the readable path gives. `None`
/// says the thing's status is a word the instance may have mapped, so whether it
/// is finished is not known: the caller stores no day, takes none off, and says
/// it did not work the day out.
pub fn moved_due_moment_under_unread_mapping(
    kind: &str,
    carriers: &[&dyn Carrier],
    existing: Option<Date>,
    held: &BTreeMap<String, String>,
    projected: &BTreeMap<String, String>,
    today: Date,
) -> Option<DueMove> {
    if unread_mapping_could_decide(kind, projected) {
        return None;
    }
    Some(moved_due_moment_as(
        kind,
        carriers,
        existing,
        held,
        projected,
        &BTreeMap::new(),
        today,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn asked(carriers: &[Box<dyn Carrier>]) -> Vec<&dyn Carrier> {
        carriers.iter().map(AsRef::as_ref).collect()
    }

    const DAY: Date = jiff::civil::date(2026, 10, 9);

    /// **A thing the words cannot decide gets the answer the readable path gives.**
    /// A work item at a shipped word, one with no status, and a kind whose
    /// status finishes nothing are each judged the same with the words mapped and
    /// unmapped. Paired with the word the instance may have mapped, which is not
    /// decided.
    #[test]
    fn a_thing_the_unread_words_cannot_change_is_answered_as_the_readable_path_answers() {
        let carriers = crate::attention::shipped();
        let asked = asked(&carriers);
        let mapped = fields(&[("status_word/shipped", "done")]);
        for (kind, status) in [
            ("work", Some("now")),
            ("work", Some("done")),
            ("work", None),
            ("thing", Some("shipped")),
        ] {
            let mut held = fields(&[("decide_by", "2026-11-01")]);
            if let Some(word) = status {
                held.insert("status".into(), word.into());
            }
            let blind =
                moved_due_moment_under_unread_mapping(kind, &asked, None, &held, &held, DAY);
            let healthy = moved_due_moment_as(kind, &asked, None, &held, &held, &mapped, DAY);
            assert_eq!(blind, Some(healthy), "{kind} at {status:?}");
        }
        let held = fields(&[("decide_by", "2026-11-01"), ("status", "shipped")]);
        assert_eq!(
            moved_due_moment_under_unread_mapping("work", &asked, None, &held, &held, DAY),
            None,
            "a word the instance may map to done is not decided"
        );
        assert!(unread_mapping_could_decide("project", &held));
        assert!(!unread_mapping_could_decide("thing", &held));
    }
}
