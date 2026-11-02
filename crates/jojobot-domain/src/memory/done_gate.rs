//! **A work item or project is not finished while a list hung on it has an open
//! entry.** The one read of those entries, shared by every store: a store
//! gathers the lists under the thing inside its own write and hands them here,
//! and [`super::guard_fit_resolved`] refuses with what this names.

use super::{Fact, MemoryError, entries, kinds};
use std::collections::BTreeMap;

/// **Whether a write moves a thing to done**: a work item or project whose
/// status says the shipped word after the write and did not before. Only this
/// write gathers lists, so every other write pays nothing. An instance word
/// mapped to done is not the shipped word and is not checked.
pub fn moves_to_done(
    kind: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> bool {
    let says_done = |fields: &BTreeMap<String, String>| {
        fields
            .get(kinds::STATUS)
            .is_some_and(|word| word.trim() == kinds::FINISHED)
    };
    kinds::holds_columns(kind) && says_done(after) && !says_done(before)
}

/// One list hung on a thing: the fields it holds and every claim on it.
pub struct HungList<'a> {
    pub fields: &'a BTreeMap<String, String>,
    pub facts: &'a [Fact],
}

/// **The addresses of the open entries across these lists.** A checklist and a
/// queue count their open entries; a rotation never completes and counts none.
pub fn open_entries_of(lists: &[HungList<'_>]) -> Vec<String> {
    let mut open = Vec::new();
    for list in lists {
        let setting = entries::Setting::of(list.fields);
        if setting == entries::Setting::Back {
            continue;
        }
        let seen = entries::view(list.facts, setting);
        open.extend(seen.slots.into_iter().map(|slot| slot.address));
    }
    open
}

/// The refusal for a write that would finish a thing with these entries open.
pub fn refusal(open: &[String]) -> MemoryError {
    MemoryError::InvalidFact(format!(
        "this cannot be set done while a list on it has open entries: {}. Clear each with capture \
         on its list: tick {{entry: the address, act: done}}, or act: dropped with the reason as \
         content; then set done again",
        open.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::refusal;

    #[test]
    fn the_done_refusal_names_capture_with_tick_and_each_entry() {
        let entries = ["list:alpha#f1".to_owned(), "list:beta#f2".to_owned()];
        let message = refusal(&entries).to_string();
        for entry in &entries {
            assert!(message.contains(entry), "missing entry {entry}: {message}");
        }
        for argument in ["capture", "tick", "act"] {
            assert!(message.contains(argument), "missing {argument}: {message}");
        }
    }
}
