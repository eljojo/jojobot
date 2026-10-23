/// **How many thoughts a room must archive to take one more.** The write adds a
/// thought, so the room has to end below its capacity first; a room an earlier
/// borrow left over its capacity needs more than one archived. The one count
/// (rule 498) behind the edit's refusal, the over-capacity refusal and the
/// `archive_needed` they carry.
pub(super) fn thoughts_to_archive_count(live: usize, capacity: usize) -> usize {
    (live + 1).saturating_sub(capacity).max(1)
}

/// The count as a phrase: "one of the thoughts above", "2 of the thoughts above".
pub(super) fn thoughts_to_archive(live: usize, capacity: usize) -> String {
    match thoughts_to_archive_count(live, capacity) {
        1 => "one of the thoughts above".to_string(),
        n => format!("{n} of the thoughts above"),
    }
}
