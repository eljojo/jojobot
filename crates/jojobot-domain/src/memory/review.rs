//! **What defrag has reviewed, and what it has not** (rules 458 and 4).
//!
//! Agents write as they always did. When the writer is defrag, jojobot sets one
//! computed key on the thing in the same write: [`DEFRAGGED_AT`]. A thing is
//! UNREVIEWED when its newest write is later than that key's own write, or when
//! it holds none. The question is asked of the two moments a thing's
//! [`FieldAges`] already carries, by [`is_unreviewed`], and nothing else decides
//! it: the recall argument, the shipped view and the boot's front page all call
//! this one function (rule 498).

use super::{EntityId, Fact, FactStatus, FieldAges, Provenance};

/// A record generated to carry settings, never a caller's authored claim.
/// Kept on the record alone, so the label never becomes one of the settings.
pub const SETTINGS_RECORD: &str = "settings_record";

/// Label a generated settings claim after the caller's keys have been screened.
pub fn settings_fields(
    mut fields: std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    fields.insert(SETTINGS_RECORD.into(), "true".into());
    fields
}

/// The working-on record keeps these words even after its link is cleared.
pub const WORKING_ON_RECORD: &str = "What this bot is working on now";

/// Only claims with rule content bind a bot. Engine labels identify new
/// settings; the exact generated sentence recognises settings written before
/// the label existed. A setting bag on authored words is still a rule.
pub fn is_rule(fact: &Fact) -> bool {
    fact.status == FactStatus::Active
        && !super::authorship::is_connection(fact.edge.as_ref())
        && !super::capacity::takes_no_slot(&fact.fields)
        && !is_settings_record(fact)
        && !is_working_on_record(fact)
}

fn is_settings_record(fact: &Fact) -> bool {
    fact.fields.contains_key(SETTINGS_RECORD)
        || (plain_generated_shape(fact)
            && !fact.sets.is_empty()
            && fact.content
                == format!(
                    "Set {} on @{}.",
                    fact.sets.iter().cloned().collect::<Vec<_>>().join(", "),
                    fact.subject.as_str()
                ))
}

fn is_working_on_record(fact: &Fact) -> bool {
    fact.content == WORKING_ON_RECORD && plain_generated_shape(fact)
}

fn plain_generated_shape(fact: &Fact) -> bool {
    fact.provenance == Provenance::Inference
        && fact.details.is_none()
        && fact.edge.is_none()
        && fact.fields.keys().all(|key| {
            fact.sets.contains(key) || key == DEFRAGGED_AT || key == crate::attention::DUE_ON
        })
}

/// **The key the stamp is stored under**, a moment on the thing. Computed by
/// jojobot when defrag writes, and never written by a caller. The spelling is
/// stored with every stamp and nothing outside this process declares it, so a
/// case pins the literal.
pub const DEFRAGGED_AT: &str = "defragged_at";

/// **The label on the one record that marks a thing reviewed**, a field of that
/// record and never of the thing. Written by jojobot when defrag marks a thing,
/// and never by a caller. The spelling is stored with the record and nothing
/// outside this process declares it, so a case pins the literal.
pub const REVIEW_MARK: &str = "review_mark";

/// **Whether a record is a thing's review mark**, by its fields. The mark holds
/// no content a reader chose to keep, so it takes no slot under a record ceiling
/// (see `capacity::live_records`).
pub fn is_review_mark(fields: &std::collections::BTreeMap<String, String>) -> bool {
    fields.contains_key(REVIEW_MARK)
}

/// **The identity whose writes stamp a thing.** The handle is defrag's for now.
pub const DEFRAG: &str = "bot:defrag";

/// **Whether a key is jojobot's bookkeeping on a thing**: the review stamp, and
/// the release notes a bot was told. Both stay out of an ordinary read of a
/// thing's fields and come back when a read names them.
pub fn is_bookkeeping_key(key: &str) -> bool {
    key == DEFRAGGED_AT || key == super::RELEASE_NOTES_TOLD
}

/// **The records that carry a thing's ledger of release notes**, found by the label
/// they hold among ALL of a thing's field writes, as `(key, value, record)`: the
/// label stays on its record and never reaches the thing, so the writes that
/// reach it do not show it.
pub fn ledger_records<'a, F: Ord>(
    writes: impl IntoIterator<Item = (&'a str, Option<&'a str>, F)>,
) -> std::collections::BTreeSet<F> {
    writes
        .into_iter()
        .filter(|(key, value, _)| {
            *key == super::capacity::ENGINE_RECORD
                && *value == Some(super::capacity::RELEASE_NOTES_RECORD)
        })
        .map(|(_, _, record)| record)
        .collect()
}

/// **Whether a write on a thing is a change of the thing.** The writes of jojobot's
/// own bookkeeping are not: a bookkeeping key, and any key of a record that carries
/// the ledger. Both stores ask here when they work out the newest moment of a
/// thing, so a ledger written at start leaves the thing as reviewed and as old as
/// it was (rule 598: decided once).
pub fn counts_as_a_change<F: Ord>(
    key: Option<&str>,
    fact: &F,
    ledgers: &std::collections::BTreeSet<F>,
) -> bool {
    !ledgers.contains(fact) && key.is_none_or(|key| !is_bookkeeping_key(key))
}

/// **Whether a record is jojobot's record of the release notes a bot was told.**
/// It is left out of a listing of the bot's records unless a read asks for it.
pub fn is_release_notes_record(fields: &std::collections::BTreeMap<String, String>) -> bool {
    fields
        .get(super::capacity::ENGINE_RECORD)
        .map(String::as_str)
        == Some(super::capacity::RELEASE_NOTES_RECORD)
}

/// **Whether this writer is defrag**, the only identity a stamp is set for and
/// the only one that may mark a thing reviewed.
pub fn is_defrag(writer: &EntityId) -> bool {
    writer.as_str() == DEFRAG
}

/// **Whether a thing is waiting for defrag.** True when it holds no stamp, and
/// when its newest write is later than the write that holds the stamp. Equal
/// moments are reviewed: the stamp is written with the rest of defrag's write, so
/// that write is never a later change than itself. A thing whose stamp carries no
/// moment cannot be shown reviewed, so it waits.
pub fn is_unreviewed(ages: &FieldAges) -> bool {
    let Some(stamped) = ages.keys.get(DEFRAGGED_AT) else {
        return true;
    };
    ages.newest.is_some_and(|newest| newest > *stamped)
}

/// **The order the queue is read in: oldest change first**, a thing whose
/// newest write is not known last. The one key every reader of the queue sorts
/// by (rule 498).
pub fn oldest_change_first(ages: Option<&FieldAges>) -> (bool, jiff::Timestamp) {
    ages.and_then(|ages| ages.newest)
        .map_or((true, jiff::Timestamp::MIN), |at| (false, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
    }

    fn ages(newest: Option<i64>, stamp: Option<i64>) -> FieldAges {
        FieldAges {
            newest: newest.map(at),
            keys: stamp
                .map(|second| (DEFRAGGED_AT.to_string(), at(second)))
                .into_iter()
                .collect(),
        }
    }

    /// **The four shapes of a thing against its stamp.** None held, written
    /// since, written at the stamp, and written before it (a stamp never reads
    /// as later than the newest write, but the comparison must not call it
    /// unreviewed if it did).
    #[test]
    fn a_thing_waits_when_it_has_no_stamp_or_changed_after_it() {
        assert!(is_unreviewed(&ages(Some(10), None)), "no stamp waits");
        assert!(
            is_unreviewed(&ages(None, None)),
            "no moment and no stamp waits"
        );
        assert!(
            is_unreviewed(&ages(Some(20), Some(10))),
            "written after waits"
        );
        assert!(
            !is_unreviewed(&ages(Some(10), Some(10))),
            "the stamp's own write is the newest write and is not a change"
        );
        assert!(
            !is_unreviewed(&ages(Some(5), Some(10))),
            "nothing later than the stamp is reviewed"
        );
        assert!(
            !is_unreviewed(&ages(None, Some(10))),
            "a stamp with no newer moment known is reviewed"
        );
    }

    /// **The key and the identity are spelled as stored.**
    #[test]
    fn the_stamp_key_and_the_identity_are_spelled_as_stored() {
        assert_eq!(DEFRAGGED_AT, "defragged_at");
        assert_eq!(DEFRAG, "bot:defrag");
        assert!(is_bookkeeping_key("defragged_at") && !is_bookkeeping_key("status"));
        assert!(is_defrag(&EntityId("bot:defrag".into())));
        assert!(!is_defrag(&EntityId("bot:sigma".into())));
    }
}
