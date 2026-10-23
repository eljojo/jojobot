//! The advice a refused numbered write carries.

/// **What to do about a number a write would free**, in the words of the verb
/// that asked. One place decides the text, so the verbs cannot drift.
///
/// An edit has a number to send, so it is sent back to the number the member
/// holds. A move out of the collection has none, and it is told to leave the
/// member where it is or to archive it, which keeps its number. Only a rename
/// with a new parent reaches the numbering guard that frees a number by a move. A merge has none: the survivor serves what the duplicate held, so two
/// numbered members cannot be folded into one, and a numbered member cannot be
/// folded out of its collection. The ways that work for a merge are the two that
/// leave no number to lose: take back the claim that gave one of the numbers
/// (a retraction is not held to the numbering) and merge again, or archive the
/// duplicate, which keeps its number.
pub(super) fn number_freed_advice(
    verb: &str,
    reads: &str,
    collection: &str,
    holder: &str,
    number: u64,
    taken_back: bool,
) -> String {
    if taken_back {
        // **The member holds no number; it carries the record of one.** A retraction
        // took the claim back and the collection still issued the number, so the
        // record stays with the collection only while the member is in it.
        return format!(
            "Nothing was written: {reads}. {holder} carries {number}, a number {collection} \
             issued before a retraction took its claim back, and {collection} keeps the record \
             only while {holder} is a member of it. No token overrides this. Archive {holder} \
             with archive_entity: an archived member stays in {collection} and keeps the \
             record. A merge works too when the survivor is a member of {collection}, because \
             the survivor then serves what {holder} carried. A merge into a thing outside it, \
             or a move to another parent, does not."
        );
    }
    let head = format!(
        "Nothing was written: {reads}. A number names one member for ever, so it stays with \
         {holder} while it is a member of {collection}, and no token overrides this."
    );
    let tail = format!("To read the holder, recall {collection}/{number}.");
    match verb {
        "merge_entities" => format!(
            "{head} A merge makes the survivor serve what the duplicate held, so it cannot \
             carry two numbers, and it cannot carry a number out of {collection}. Two ways go \
             on. Archive the duplicate with archive_entity: an archived member keeps its \
             number. Or, when the survivor is a member of {collection}, take back the claim \
             that gave the duplicate's number with retract (the number is not issued again) \
             and send the merge again: the survivor then serves what the duplicate carried. A \
             merge into a thing outside {collection} stays refused after a retraction too. \
             {tail}"
        ),
        "rename_entity" => format!(
            "{head} A move with a new `parent` takes {holder} out of {collection}, and a \
             number stays with a member only while it is a member. Two ways go on. Leave it \
             where it is: send the rename again without the new `parent`. Or, if it is \
             retired, archive it with archive_entity: an archived member keeps its number \
             and stays in {collection}. {tail}"
        ),
        _ => format!(
            "{head} Send the number it holds, or change something else. An archived member \
             keeps its number. A number given by mistake is taken back by retracting the \
             claim that gave it, and then the right one can be given. {tail}"
        ),
    }
}

/// **What to do about a number a collection issued and a retraction took back**:
/// send the next one. No token overrides this.
pub(super) fn number_retired_advice(reads: &str, collection: &str, next: u64) -> String {
    format!(
        "Nothing was written: {reads}. A number is never reused, even when the claim that \
         gave it was retracted, and no token overrides this. The next free number is \
         {next}: send that. To see the numbers in use, recall {collection}."
    )
}

#[cfg(test)]
mod tests {
    use super::number_freed_advice;

    /// The words of a text, with a tool name kept whole.
    fn words(text: &str) -> Vec<&str> {
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|word| !word.is_empty())
            .collect()
    }

    /// **A member that only carries a taken-back number is told to archive it, and
    /// never to retract.** The claim is retracted already, and a retraction does
    /// not free the exit: archiving keeps the member and its record.
    #[test]
    fn a_member_carrying_a_taken_back_number_is_told_to_archive_and_not_to_retract() {
        for verb in ["merge_entities", "rename_entity", "update_entity"] {
            let advice =
                number_freed_advice(verb, "reads", "topic:widgets", "thing:handcart", 5, true);
            let said = words(&advice);
            assert!(said.contains(&"archive_entity"), "{verb}: {advice}");
            assert!(!said.contains(&"retract"), "{verb}: {advice}");
            assert!(
                !advice.contains("topic:widgets/5"),
                "{verb}: no member holds 5 to be read: {advice}"
            );
        }
    }

    /// **A member that holds the number and one that carries it are told the same
    /// condition for a merge**: a merge goes on into a member of the collection and
    /// into nothing outside it, and a retraction does not change that.
    #[test]
    fn a_held_and_a_carried_number_are_told_one_condition_for_a_merge() {
        for taken_back in [false, true] {
            let advice = number_freed_advice(
                "merge_entities",
                "reads",
                "topic:widgets",
                "thing:handcart",
                5,
                taken_back,
            );
            assert!(
                advice.contains("survivor is a member of topic:widgets"),
                "taken_back {taken_back}: {advice}"
            );
        }
    }

    /// The pair: a member that holds the number is still told both routes for a
    /// merge, because a retraction lets a merge into a member that stays go on.
    #[test]
    fn a_member_holding_the_number_is_still_told_both_routes_for_a_merge() {
        let advice = number_freed_advice(
            "merge_entities",
            "reads",
            "topic:widgets",
            "thing:handcart",
            5,
            false,
        );
        let said = words(&advice);
        assert!(
            said.contains(&"archive_entity") && said.contains(&"retract"),
            "{advice}"
        );
    }
}
