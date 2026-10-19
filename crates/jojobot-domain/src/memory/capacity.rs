//! **How many live records a thing holds before a write must give.**
//!
//! A thread is the container built for an ongoing line in a life, and a line
//! that can always be appended to grows until it is too dear to read whole. So
//! a thread holds [`THREAD_RECORDS`] live records unless it carries a setting of
//! its own, and a write that would add one more is refused with nothing
//! written. The writer chooses what gives.
//!
//! **One entry point, [`admit`], answers for every write that adds a member**:
//! a capture, a merge that brings records onto the thing, and a retype that
//! makes a thing a thread. Each store reads what [`admit`] takes inside the
//! transaction that makes the write, so the count and the write commit
//! together.
//!
//! **The interface takes any thing; only the thread has a default.** A thing of
//! another kind is bound only when it carries the setting.
//!
//! **A thought is not counted here.** A thought is a claim on a bot, and a
//! thread's ceiling for the bots that write thoughts into it is
//! [`THOUGHT_CAPACITY`](super::THOUGHT_CAPACITY), which this module leaves as it
//! was.

use std::collections::BTreeMap;

use super::review::is_review_mark;
use super::{
    EntityId, EntityKind, Fact, FactStatus, MemoryError, THOUGHT_BODY_CAP, THOUGHT_CAPACITY,
};

/// **The key a thing's own record ceiling is read from.** An ordinary field,
/// folded like any other. The spelling is stored in a fields bag and nothing
/// outside this process declares it, so a case pins the literal.
pub const RECORD_CAPACITY: &str = "record_capacity";

/// **How many live records a thread holds when it carries no setting.**
pub const THREAD_RECORDS: usize = 3;

/// **The refusal a ceiling key gets for a value it cannot hold, or none.** The
/// one place that says what each ceiling key accepts, so a write is judged by
/// the same words wherever it arrives (rule 498). A record ceiling is a whole
/// number of one or more: zero would make a thing unwritable. A thought's
/// capacity and body cap are a whole number of zero or more. A key that is not a
/// ceiling is not judged. The key is matched trimmed, as the store keeps it.
///
/// [`ceiling_of`] stays lenient on purpose: a store that already holds a bad
/// value still boots and reads it as the default.
pub fn ceiling_value_refusal(key: &str, value: &str) -> Option<MemoryError> {
    let named = key.trim();
    let (least, form) = match named {
        RECORD_CAPACITY => (1, "one or more"),
        THOUGHT_CAPACITY | THOUGHT_BODY_CAP => (0, "zero or more"),
        _ => return None,
    };
    if value.trim().parse::<usize>().is_ok_and(|n| n >= least) {
        return None;
    }
    Some(MemoryError::InvalidFact(format!(
        "'{named}' holds a whole number of {form}, and '{value}' is not one. Send it again \
         with a number such as 5, or leave the key off to leave the container without that \
         ceiling"
    )))
}

/// **The label on a record jojobot writes about an act of its own**, a field of
/// that record and never of the thing: the history of a restore, the reason a
/// rename was let through. The act asked for no record, so the record forces no
/// decision about what to keep and takes no slot (see [`takes_no_slot`]). The
/// value names the act. Written by jojobot, and never by a caller. The spelling
/// is stored with the record and nothing outside this process declares it, so a
/// case pins the literal.
pub const ENGINE_RECORD: &str = "engine_record";

/// **Whether a record takes no slot under a record ceiling**, by its fields: the
/// review mark and a record jojobot wrote about an act of its own. The one
/// predicate (rule 498) that [`live_records`] counts by and [`slots_taken`] adds
/// by, so a record is never counted that an add did not count.
pub fn takes_no_slot(fields: &BTreeMap<String, String>) -> bool {
    is_review_mark(fields) || fields.contains_key(ENGINE_RECORD)
}

/// Where a ceiling came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingFrom {
    /// The thing carries [`RECORD_CAPACITY`].
    Setting,
    /// The thing carries none, and its kind has a default.
    Default,
}

/// **A thing's ceiling and where it came from.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ceiling {
    /// How many live records the thing holds.
    pub limit: usize,
    /// Whether the thing set it or the kind supplied it.
    pub from: CeilingFrom,
}

/// **The ceiling of a thing, from its kind and what it holds as fields.** A
/// setting that is not a whole number above zero reads as no setting at all, so
/// a mistyped value never makes a thing unwritable. `None` means the thing is
/// not bound.
pub fn ceiling_of(kind: Option<EntityKind>, held: &BTreeMap<String, String>) -> Option<Ceiling> {
    let setting = held
        .get(RECORD_CAPACITY)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0);
    match setting {
        Some(limit) => Some(Ceiling {
            limit,
            from: CeilingFrom::Setting,
        }),
        None if kind == Some(EntityKind::THREAD) => Some(Ceiling {
            limit: THREAD_RECORDS,
            from: CeilingFrom::Default,
        }),
        None => None,
    }
}

/// **What the survivor of a merge holds as fields for the ceiling's question.**
/// The merged thing folds to one of two numbers, whichever was written last, and
/// the guard of a merge runs before it. So the question is asked against the
/// lower of the survivor's ceiling and the number the duplicate carries: a merge
/// that fits one and not the other would leave the survivor over its number with
/// no way to write to it. A number the duplicate carries that is no lower than
/// the survivor's ceiling changes nothing, and a duplicate that carries none
/// leaves the survivor's fields as they are.
pub fn held_after_a_merge(
    kind: Option<EntityKind>,
    survivor: &BTreeMap<String, String>,
    duplicate: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let carried = duplicate
        .get(RECORD_CAPACITY)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0);
    let mut held = survivor.clone();
    if let Some(carried) = carried
        && ceiling_of(kind, survivor).is_none_or(|own| carried < own.limit)
    {
        held.insert(RECORD_CAPACITY.to_string(), carried.to_string());
    }
    held
}

/// **The records that take a slot: the active ones filed on the thing.** An
/// archived record, an old version of a record, a write of keys alone, a record
/// filed on another thing, the thing's review mark and a record jojobot wrote
/// about an act of its own (see [`ENGINE_RECORD`]) take none, so `filed_on`
/// is the thing's own records and nothing else. The mark records that defrag
/// read the thing and holds no content, and the engine's record says what
/// jojobot did, so a ceiling has no decision to force on either.
pub fn live_records(filed_on: &[Fact]) -> Vec<Fact> {
    filed_on
        .iter()
        .filter(|fact| fact.status == FactStatus::Active && !takes_no_slot(&fact.fields))
        .cloned()
        .collect()
}

/// **How many slots a new record takes**, from the fields it is written with:
/// none for a record that [`takes_no_slot`], one for every other record. The
/// count [`admit`] is given as `adding`, so such a record is exempt on the way in
/// as it is when counted.
pub fn slots_taken(fields: &BTreeMap<String, String>) -> usize {
    usize::from(!takes_no_slot(fields))
}

/// **The thing a write adds to.**
#[derive(Debug, Clone, Copy)]
pub struct Container<'a> {
    /// The handle the caller reached it by.
    pub handle: &'a EntityId,
    /// Its kind.
    pub kind: Option<EntityKind>,
    /// What it holds as fields.
    pub held: &'a BTreeMap<String, String>,
}

/// What [`admit`] decides.
#[derive(Debug)]
pub enum Admission {
    /// The write fits.
    Lands,
    /// The write lands over the ceiling on the emergency reserve. The thing now
    /// holds `live` records against `limit`.
    Borrowed {
        /// How many live records the thing holds once the write lands.
        live: usize,
        /// The ceiling.
        limit: usize,
    },
    /// The write is refused and nothing is written.
    Refused(MemoryError),
}

/// **Whether a write that adds `adding` records to a thing may land.**
///
/// `live` is [`live_records`] of the thing as it stands. The write lands when
/// the thing ends at or under its ceiling. **A thing already over its ceiling
/// admits nothing that adds a record**, so it is read, edited and archived and
/// is not written to again until it is back under; a write that adds none, such
/// as a retype of a thing that fits, lands.
///
/// **The emergency reserve is one record, once.** With `borrow` set, a write
/// that adds exactly one record to a thing that is exactly full lands over the
/// ceiling. A thing already over it has spent the reserve, and archiving back to
/// the ceiling repays it. A write that adds several, as a merge does, has no
/// reserve to spend.
pub fn admit(container: Container<'_>, live: &[Fact], adding: usize, borrow: bool) -> Admission {
    let Some(ceiling) = ceiling_of(container.kind, container.held) else {
        return Admission::Lands;
    };
    let held = live.len();
    if held + adding <= ceiling.limit {
        return Admission::Lands;
    }
    if borrow && adding == 1 && held == ceiling.limit {
        return Admission::Borrowed {
            live: held + 1,
            limit: ceiling.limit,
        };
    }
    Admission::Refused(MemoryError::OverCapacity {
        subject: container.handle.to_string(),
        live: held,
        limit: ceiling.limit,
        adding,
        from_setting: ceiling.from == CeilingFrom::Setting,
        records: live.to_vec(),
    })
}

/// **The ceiling's answer for a capture.** A record that takes no slot (the
/// review mark) lands on a thing over its ceiling, because refusing it would
/// force a decision on a write that adds nothing to decide about. Every other
/// record faces [`admit`] with the one slot it takes. The one entry point both
/// stores call at a capture.
pub fn admit_capture(
    container: Container<'_>,
    live: &[Fact],
    fields: &BTreeMap<String, String>,
    borrow: bool,
) -> Admission {
    match slots_taken(fields) {
        0 => Admission::Lands,
        adding => admit(container, live, adding, borrow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Edge, EdgeShape, FactId, Provenance, Standing};

    fn handle() -> EntityId {
        EntityId("thread:milhouse-moves".into())
    }

    fn record(local: &str, status: FactStatus) -> Fact {
        Fact {
            app_stamp: None,
            sets: Default::default(),
            id: FactId(local.into()),
            home: handle(),
            subject: handle(),
            content: format!("record {local}"),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status,
            recorded_at: jiff::civil::date(2026, 10, 16),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
            receipt: None,
        }
    }

    fn active(count: usize) -> Vec<Fact> {
        (1..=count)
            .map(|n| record(&format!("f{n}"), FactStatus::Active))
            .collect()
    }

    fn held(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn lands(admission: Admission) -> bool {
        matches!(admission, Admission::Lands)
    }

    fn thread() -> Option<EntityKind> {
        Some(EntityKind::THREAD)
    }

    fn ask(
        kind: Option<EntityKind>,
        fields: &BTreeMap<String, String>,
        live: &[Fact],
        adding: usize,
        borrow: bool,
    ) -> Admission {
        let id = handle();
        admit(
            Container {
                handle: &id,
                kind,
                held: fields,
            },
            live,
            adding,
            borrow,
        )
    }

    /// A thread with no setting holds three and refuses the fourth.
    #[test]
    fn a_thread_with_no_setting_holds_three_and_refuses_a_fourth() {
        let none = held(&[]);
        assert!(lands(ask(thread(), &none, &active(2), 1, false)));
        assert!(lands(ask(thread(), &none, &active(3), 0, false)));
        let Admission::Refused(MemoryError::OverCapacity {
            live,
            limit,
            adding,
            from_setting,
            records,
            ..
        }) = ask(thread(), &none, &active(3), 1, false)
        else {
            panic!("the fourth record must be refused");
        };
        assert_eq!(
            (live, limit, adding, from_setting, records.len()),
            (3, 3, 1, false, 3)
        );
    }

    /// A thread's own setting bounds it instead of the default, both ways.
    #[test]
    fn a_threads_own_setting_replaces_the_default() {
        let five = held(&[(RECORD_CAPACITY, "5")]);
        assert!(lands(ask(thread(), &five, &active(4), 1, false)));
        let Admission::Refused(MemoryError::OverCapacity {
            limit,
            from_setting,
            ..
        }) = ask(thread(), &five, &active(5), 1, false)
        else {
            panic!("the sixth record must be refused");
        };
        assert_eq!((limit, from_setting), (5, true));
        let two = held(&[(RECORD_CAPACITY, "2")]);
        assert!(matches!(
            ask(thread(), &two, &active(2), 1, false),
            Admission::Refused(_)
        ));
    }

    /// Only a record that is active and filed on the thing takes a slot.
    #[test]
    fn an_archived_record_takes_no_slot() {
        let filed = vec![
            record("f1", FactStatus::Active),
            record("f2", FactStatus::Archived),
            record("f3", FactStatus::Active),
            record("f4", FactStatus::Archived),
            record("f5", FactStatus::Active),
        ];
        let live = live_records(&filed);
        assert_eq!(live.len(), 3);
        assert!(matches!(
            ask(thread(), &held(&[]), &live, 1, false),
            Admission::Refused(MemoryError::OverCapacity {
                live: 3,
                limit: 3,
                ..
            })
        ));
        assert!(lands(ask(thread(), &held(&[]), &live[..2], 1, false)));
    }

    /// The reserve lands one record over a full thing, once.
    #[test]
    fn the_reserve_admits_one_over_a_full_thread_and_no_second() {
        let none = held(&[]);
        assert!(matches!(
            ask(thread(), &none, &active(3), 1, true),
            Admission::Borrowed { live: 4, limit: 3 }
        ));
        assert!(
            matches!(
                ask(thread(), &none, &active(4), 1, true),
                Admission::Refused(_)
            ),
            "a thread already over its ceiling has spent the reserve"
        );
        assert!(
            lands(ask(thread(), &none, &active(2), 1, true)),
            "a reserve nobody needed is not spent"
        );
    }

    /// A write that brings several records has no reserve to spend.
    #[test]
    fn a_write_that_adds_several_cannot_borrow() {
        assert!(matches!(
            ask(thread(), &held(&[]), &active(3), 2, true),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ask(thread(), &held(&[]), &active(1), 3, false),
            Admission::Refused(_)
        ));
        assert!(lands(ask(thread(), &held(&[]), &active(1), 2, false)));
    }

    /// A thing over its ceiling admits nothing that adds a record, and a retype
    /// that adds none is refused for it too.
    #[test]
    fn a_thing_over_its_ceiling_admits_no_write() {
        let none = held(&[]);
        assert!(matches!(
            ask(thread(), &none, &active(5), 0, false),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ask(thread(), &none, &active(5), 1, false),
            Admission::Refused(_)
        ));
    }

    /// Another kind has no default, and takes the setting when it carries one.
    #[test]
    fn a_thing_of_another_kind_is_bound_only_by_its_own_setting() {
        let topic = Some(EntityKind::TOPIC);
        assert!(lands(ask(topic, &held(&[]), &active(40), 1, false)));
        assert!(matches!(
            ask(
                topic,
                &held(&[(RECORD_CAPACITY, "4")]),
                &active(4),
                1,
                false
            ),
            Admission::Refused(_)
        ));
        assert!(lands(ask(None, &held(&[]), &active(40), 1, false)));
    }

    /// A setting that is not a whole number above zero reads as no setting.
    #[test]
    fn a_setting_that_is_not_a_positive_number_reads_as_none() {
        for bad in ["", "0", "-2", "many", "2.5"] {
            let ceiling = ceiling_of(thread(), &held(&[(RECORD_CAPACITY, bad)]));
            assert_eq!(
                ceiling,
                Some(Ceiling {
                    limit: THREAD_RECORDS,
                    from: CeilingFrom::Default
                }),
                "{bad:?}"
            );
        }
        assert_eq!(
            ceiling_of(Some(EntityKind::TOPIC), &held(&[(RECORD_CAPACITY, "many")])),
            None
        );
    }

    /// **Each ceiling key's edge.** A record ceiling of zero would make a thing
    /// unwritable, so it starts at one; a thought's capacity and body cap keep
    /// the zero they have always taken. The literals are the stored spellings.
    #[test]
    fn each_ceiling_key_accepts_exactly_its_own_edge() {
        let accepts = |key: &str, value: &str| ceiling_value_refusal(key, value).is_none();
        assert!(!accepts("record_capacity", "0"));
        assert!(accepts("record_capacity", "1"));
        assert!(accepts(" record_capacity ", " 5 "));
        for bad in ["three", "-2", "2.5", "", " "] {
            assert!(!accepts("record_capacity", bad), "{bad:?}");
        }
        assert!(accepts("thought_capacity", "0"));
        assert!(accepts("thought_body_cap", "0"));
        assert!(!accepts("thought_capacity", "-1"));
        assert!(!accepts("thought_body_cap", "many"));
        assert!(
            accepts("any_other_key", "three"),
            "only ceilings are judged"
        );
    }

    /// **The review mark is the one record that takes no slot**, whether it is
    /// counted or about to be added. The literal is the stored label.
    #[test]
    fn the_review_mark_takes_no_slot_counted_or_added() {
        let mut mark = record("f4", FactStatus::Active);
        mark.fields.insert("review_mark".into(), "true".into());
        let mut held = active(3);
        held.push(mark);
        assert_eq!(live_records(&held).len(), 3);
        assert_eq!(slots_taken(&held[3].fields), 0);
        assert_eq!(slots_taken(&held[0].fields), 1);
    }

    /// **A capture that takes no slot lands on a thing over its ceiling, and any
    /// other capture is judged as it was.**
    #[test]
    fn a_capture_that_takes_no_slot_lands_on_a_thing_over_its_ceiling() {
        let id = handle();
        let over = active(4);
        let ask = |fields: &BTreeMap<String, String>| {
            admit_capture(
                Container {
                    handle: &id,
                    kind: thread(),
                    held: &held(&[]),
                },
                &over,
                fields,
                false,
            )
        };
        assert!(lands(ask(&held(&[("review_mark", "true")]))));
        assert!(matches!(ask(&held(&[])), Admission::Refused(_)));
    }

    /// **A merge is asked against the lower of the two numbers.** The duplicate's
    /// number counts only when it is lower than the survivor's ceiling, and a
    /// survivor with no ceiling takes the duplicate's.
    #[test]
    fn a_merge_is_held_to_the_lower_of_the_two_numbers() {
        let number = |held: &BTreeMap<String, String>| {
            ceiling_of(thread(), held).map(|ceiling| ceiling.limit)
        };
        let after = |kind, survivor: &[(&str, &str)], duplicate: &[(&str, &str)]| {
            held_after_a_merge(kind, &held(survivor), &held(duplicate))
        };
        let own = [(RECORD_CAPACITY, "20")];
        assert_eq!(
            number(&after(thread(), &own, &[(RECORD_CAPACITY, "3")])),
            Some(3)
        );
        assert_eq!(
            number(&after(thread(), &own, &[(RECORD_CAPACITY, "30")])),
            Some(20)
        );
        assert_eq!(number(&after(thread(), &own, &[])), Some(20));
        // A thread with no number of its own holds the default, and a higher
        // number on the duplicate does not raise it here.
        assert_eq!(
            number(&after(thread(), &[], &[(RECORD_CAPACITY, "9")])),
            Some(3)
        );
        assert_eq!(
            number(&after(thread(), &[], &[(RECORD_CAPACITY, "2")])),
            Some(2)
        );
        // A thing with no ceiling takes the duplicate's.
        let topic = Some(EntityKind::TOPIC);
        assert_eq!(
            ceiling_of(topic, &after(topic, &[], &[(RECORD_CAPACITY, "4")])).map(|c| c.limit),
            Some(4)
        );
        // A number that is no number is no number.
        assert_eq!(
            number(&after(thread(), &own, &[(RECORD_CAPACITY, "many")])),
            Some(20)
        );
    }

    /// **A record jojobot wrote about an act of its own takes no slot either**,
    /// counted or about to be added, and the one predicate says so for both. The
    /// literal is the stored label.
    #[test]
    fn an_engine_record_takes_no_slot_counted_or_added() {
        let mut restore = record("f4", FactStatus::Active);
        restore
            .fields
            .insert("engine_record".into(), "restore".into());
        let mut held = active(3);
        held.push(restore);
        assert_eq!(live_records(&held).len(), 3);
        assert_eq!(slots_taken(&held[3].fields), 0);
        assert!(takes_no_slot(&held[3].fields));
        assert!(!takes_no_slot(&held[0].fields));
        assert_eq!(ENGINE_RECORD, "engine_record");
    }

    /// A key that is the same spelling in storage as in this constant is
    /// pinned here, because nothing outside this process declares it.
    #[test]
    fn the_setting_is_spelled_record_capacity() {
        assert_eq!(RECORD_CAPACITY, "record_capacity");
        assert_eq!(THREAD_RECORDS, 3);
    }

    /// A thought points at the thread without being filed on it, so the records
    /// of the thread never hold one: `live_records` counts what it is handed.
    #[test]
    fn a_record_drawing_an_edge_is_still_a_record() {
        let mut edged = record("f1", FactStatus::Active);
        edged.edge = Some(Edge::new(
            EdgeShape::About,
            EntityId("thing:jukebox".into()),
        ));
        assert_eq!(live_records(&[edged]).len(), 1);
    }
}
