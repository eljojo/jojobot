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

/// **A thought ceiling exists on purpose**, so the refusal of a write to one
/// points at the route that same refusal names and treats a change to the
/// ceiling as the exception. It claims no earlier refusal, because a first
/// attempt has none. Any other key's refusal is returned as it is.
pub(super) fn thought_ceiling_note(key: &str, subject: &str, said: String) -> String {
    use jojobot_domain::memory::{THOUGHT_AGES_AFTER_RUNS, THOUGHT_BODY_CAP, THOUGHT_CAPACITY};
    if matches!(
        key,
        THOUGHT_CAPACITY | THOUGHT_BODY_CAP | THOUGHT_AGES_AFTER_RUNS
    ) {
        format!(
            "{said} Use the route named above, and ask for a change only when \
             the job of '{subject}' has changed."
        )
    } else {
        said
    }
}

/// **A full container's way forward, ending with the skill that shows how to
/// fit more than it holds** (rule 593): full seats, a full room and a full
/// thread all end the same way.
pub(super) fn naming_organizing(how_to_proceed: String) -> String {
    format!(
        "{how_to_proceed} {}",
        crate::orientation::skills::ORGANIZING_POINTER
    )
}
