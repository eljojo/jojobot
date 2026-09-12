//! **What somebody holds that gets them in, ranked for a reader who did not
//! ask.**
//!
//! Holding a pass is a claim about a PERSON, with its own provenance and its
//! own window; whether a thing offers that tier at all is a claim about the
//! thing and lives there. This module is the read from the other end: given the
//! records that point at a thing, which of them a session needs to be told
//! about, and in what order.
//!
//! **The order is the feature.** A budget filled by a bad ranking is a dump
//! with a limit on it, so the order is a function of the records and one date
//! — no store, no query, no index — and it has its own cases below.
//!
//! **Nothing here decides anything.** It says what is recorded and how it
//! stands as of a day. Whether somebody can get in is a judgement, and jojobot
//! does not make judgements.

use jiff::civil::Date;

use super::{EntityId, Fact, Provenance};

/// **The key the shipped `entitlement` type declares**, and the one this reads.
///
/// A record is an entitlement because it carries this key, whether or not
/// anybody declared the type — matching here is structural, like everywhere
/// else.
pub const ADMITS: &str = "admits";

/// The day the entitlement starts, when it has one.
pub const VALID_FROM: &str = "valid_from";

/// The last day it is good for, when it has one.
pub const VALID_UNTIL: &str = "valid_until";

/// **How a record that points at a thing stands, as of a day.**
///
/// The order of the variants is the order a reader wants them in, and
/// `#[derive(PartialOrd)]` is what makes that a fact about the type rather
/// than a comparison somebody has to remember to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standing {
    /// It admits to this thing and the day falls inside its window — or it has
    /// no window, which is an entitlement nobody dated rather than an expired
    /// one.
    Live,
    /// It admits to this thing and the day is outside its window. **Kept
    /// rather than dropped:** *you had one and it ran out* changes what a
    /// reader does next, and silence does not.
    Lapsed,
    /// It points at this thing through some other declared reference key.
    /// **Only ever reached when nothing admits at all** — the wider answer a
    /// narrow one that came back empty opens onto.
    Related,
}

/// **One record pointing at the thing, with how it stands.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held<'a> {
    /// The record itself — its subject is the holder, and it carries its own
    /// provenance, which a reader acts on differently.
    pub fact: &'a Fact,
    /// How it stands as of the day asked about.
    pub standing: Standing,
}

/// **Whether the wider set may be reached at all.**
///
/// The wider tier fills space a caller left empty. **A call that named a walk
/// of its own has already said which links it is after**, so answering it with
/// *and here is everything else pointing here* argues with the question — and
/// spends an unasked budget doing it. What GATES the thing still comes back
/// either way: that is the answer nobody thinks to ask for, which is the whole
/// reason this read exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Widen {
    /// Reach the wider set when nothing admits anybody — the ordinary read.
    WhenEmpty,
    /// Never reach it: this call walks links it chose itself.
    Never,
}

/// **Which records a reader is told about, best first.**
///
/// `keys` is every key some declaration made a reference. It is passed in
/// rather than read here so this stays a function of its arguments: the
/// declarations live in the store, and the ranking must be testable without
/// one.
///
/// The order, and each step has a reason a reader can check:
///
/// 1. **What gates the action, live now.** A pass that works today is the
///    answer to the question somebody is about to ask.
/// 2. **What gated it and no longer does.**
/// 3. **Everything else pointing here** — and only when the first two are
///    empty, because a wider answer beside a narrow one buries it.
///
/// **Testimony before inference inside every tier.** The user acts on this: a
/// pass somebody SAID they hold and one an assistant worked out are not
/// interchangeable when the consequence of being wrong is being turned away at
/// a door. Then the newest record first, and the record's own address last, so
/// two runs over one store answer the same way.
pub fn ranked<'a>(
    facts: &'a [Fact],
    target: &EntityId,
    as_of: Date,
    keys: &[String],
    widen: Widen,
) -> Vec<Held<'a>> {
    let admitting: Vec<Held<'a>> = facts
        .iter()
        .filter(|fact| stands(fact))
        .filter(|fact| points_at(fact, ADMITS, target))
        .map(|fact| Held {
            fact,
            standing: if live(fact, as_of) {
                Standing::Live
            } else {
                Standing::Lapsed
            },
        })
        .collect();
    let mut held = if admitting.is_empty() && widen == Widen::WhenEmpty {
        facts
            .iter()
            .filter(|fact| stands(fact))
            .filter(|fact| {
                keys.iter()
                    .any(|key| key != ADMITS && points_at(fact, key, target))
            })
            .map(|fact| Held {
                fact,
                standing: Standing::Related,
            })
            .collect()
    } else {
        admitting
    };
    held.sort_by(|a, b| {
        a.standing
            .cmp(&b.standing)
            .then_with(|| backing(a.fact).cmp(&backing(b.fact)))
            .then_with(|| b.fact.recorded_at.cmp(&a.fact.recorded_at))
            .then_with(|| a.fact.id.0.cmp(&b.fact.id.0))
    });
    held
}

/// **Does this record still stand?**
///
/// The read behind this answers with records of every status, because it serves
/// history as often as current truth. **This is a reader of current truth**, so
/// a claim somebody took back or replaced is not something anybody holds — and
/// of everything this ranking can get wrong, telling somebody they are covered
/// when the record was withdrawn is the one that costs most.
fn stands(fact: &Fact) -> bool {
    fact.status == crate::memory::FactStatus::Active
}

/// Testimony sorts before inference, and this is the number that says so.
fn backing(fact: &Fact) -> u8 {
    match fact.provenance {
        // The user's own word first: they are the one who will be at the door.
        Provenance::Testimony => 0,
        // **A confident read of a system of record outranks a guess and not the
        // user.** It is checkable — it names the system it came from, so a
        // reader can go back to it — while a derivation names nothing anybody
        // can return to. It sits below testimony because a system can be read
        // wrongly and the user cannot be misquoted about what they hold.
        Provenance::Observation => 1,
        Provenance::Inference => 2,
    }
}

/// Does this record point at that thing through this key?
fn points_at(fact: &Fact, key: &str, target: &EntityId) -> bool {
    fact.fields
        .get(key)
        .is_some_and(|value| value.trim() == target.as_str())
}

/// **Is it good on that day?**
///
/// A missing bound is an open one, and **a bound nothing can read is treated
/// as missing rather than as closed**. The two mistakes are not equal: a
/// record shown when it had lapsed is one a reader checks, and a record hidden
/// because its date was mistyped is one nobody ever hears about again.
fn live(fact: &Fact, as_of: Date) -> bool {
    let bound = |key: &str| {
        fact.fields
            .get(key)
            .and_then(|v| v.trim().parse::<Date>().ok())
    };
    bound(VALID_FROM).is_none_or(|from| from <= as_of)
        && bound(VALID_UNTIL).is_none_or(|until| as_of <= until)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{FactId, FactStatus, Standing as FactStanding};
    use jiff::civil::date;
    use std::collections::BTreeMap;

    /// A record on somebody, pointing at something, with a window and a
    /// backing.
    fn holding(
        id: &str,
        holder: &str,
        key: &str,
        target: &str,
        window: (Option<&str>, Option<&str>),
        provenance: Provenance,
    ) -> Fact {
        let mut fields = BTreeMap::new();
        fields.insert(key.to_string(), target.to_string());
        if let Some(from) = window.0 {
            fields.insert(VALID_FROM.to_string(), from.to_string());
        }
        if let Some(until) = window.1 {
            fields.insert(VALID_UNTIL.to_string(), until.to_string());
        }
        Fact {
            id: FactId(id.into()),
            home: EntityId(holder.into()),
            subject: EntityId(holder.into()),
            content: "holds one".into(),
            details: None,
            provenance,
            standing: FactStanding::Open,
            status: FactStatus::Active,
            recorded_at: date(2026, 8, 1),
            happened_at: None,
            edge: None,
            derived_from: None,
            stands_for: Vec::new(),
            fields,
            refs: Vec::new(),
            inserted_at: None,
            stale_after: None,
        }
    }

    fn keys() -> Vec<String> {
        vec![ADMITS.to_string(), "arrives_at".to_string()]
    }

    /// **The order a reader needs, and every step of it is load-bearing.**
    ///
    /// Live before lapsed, because one answers the question being asked and the
    /// other changes what the reader does next. Testimony before inference
    /// inside the tier, because the consequence of acting on a guess here is
    /// being turned away at a door.
    #[test]
    fn live_comes_before_lapsed_and_testimony_before_inference() {
        let target = EntityId("event:winter-fest".into());
        let facts = vec![
            holding(
                "f1",
                "person:milhouse",
                ADMITS,
                "event:winter-fest",
                (None, Some("2026-08-10")),
                Provenance::Testimony,
            ),
            holding(
                "f2",
                "person:otto",
                ADMITS,
                "event:winter-fest",
                (None, None),
                Provenance::Inference,
            ),
            holding(
                "f3",
                "person:bart",
                ADMITS,
                "event:winter-fest",
                (Some("2026-08-01"), Some("2026-08-31")),
                Provenance::Testimony,
            ),
        ];
        let ranked = ranked(
            &facts,
            &target,
            date(2026, 8, 19),
            &keys(),
            Widen::WhenEmpty,
        );
        assert_eq!(
            ranked
                .iter()
                .map(|held| (held.fact.id.0.as_str(), held.standing))
                .collect::<Vec<_>>(),
            vec![
                ("f3", Standing::Live),
                ("f2", Standing::Live),
                ("f1", Standing::Lapsed),
            ],
            "live first, testimony ahead of inference, and the lapsed one kept"
        );
    }

    /// **A claim that was archived does not get anybody in**, whether it was
    /// taken back or replaced — the two read the same status now.
    ///
    /// The read this ranking is fed answers with records of every status,
    /// archived included, because it serves history as often as current
    /// truth. **So the filtering is this function's job**, and an archived
    /// claim arriving as an entitlement they hold is the one failure that
    /// costs more than saying nothing: a reader told they are covered acts on
    /// it.
    ///
    /// **The active claim in the same read is what makes this mean anything.**
    /// Without it the case passes against a ranking that returns nothing at
    /// all.
    #[test]
    fn an_archived_claim_is_not_something_anybody_holds() {
        let target = EntityId("event:winter-fest".into());
        let with_status = |id: &str, holder: &str, status: FactStatus| Fact {
            status,
            ..holding(
                id,
                holder,
                ADMITS,
                "event:winter-fest",
                (None, None),
                Provenance::Testimony,
            )
        };
        let facts = vec![
            with_status("f1", "person:milhouse", FactStatus::Archived),
            with_status("f2", "person:otto", FactStatus::Archived),
            with_status("f3", "person:bart", FactStatus::Active),
        ];

        assert_eq!(
            ranked(
                &facts,
                &target,
                date(2026, 8, 19),
                &keys(),
                Widen::WhenEmpty
            )
            .iter()
            .map(|held| (held.fact.id.0.as_str(), held.standing))
            .collect::<Vec<_>>(),
            vec![("f3", Standing::Live)],
            "a claim nobody archived is what somebody holds, and the other two are not",
        );
    }

    /// **A record with no window is live, not expired.** An entitlement nobody
    /// dated is one nobody dated.
    #[test]
    fn a_record_with_no_window_is_live() {
        let target = EntityId("event:winter-fest".into());
        let facts = vec![holding(
            "f1",
            "person:milhouse",
            ADMITS,
            "event:winter-fest",
            (None, None),
            Provenance::Testimony,
        )];
        assert_eq!(
            ranked(
                &facts,
                &target,
                date(2026, 8, 19),
                &keys(),
                Widen::WhenEmpty
            )[0]
            .standing,
            Standing::Live,
        );
    }

    /// **A date nothing can read is treated as no bound.** A record hidden
    /// because somebody mistyped a day is one nobody hears about again, which
    /// is the failure this whole read exists to end.
    #[test]
    fn a_bound_that_cannot_be_read_does_not_hide_the_record() {
        let target = EntityId("event:winter-fest".into());
        let facts = vec![holding(
            "f1",
            "person:milhouse",
            ADMITS,
            "event:winter-fest",
            (None, Some("next thursday")),
            Provenance::Testimony,
        )];
        assert_eq!(
            ranked(
                &facts,
                &target,
                date(2026, 8, 19),
                &keys(),
                Widen::WhenEmpty
            )[0]
            .standing,
            Standing::Live,
        );
    }

    /// **The wider set is reached by the narrow one being empty**, and never
    /// beside it: a record that merely points here would otherwise sit next to
    /// the pass that gets somebody in.
    #[test]
    fn other_pointers_arrive_only_when_nothing_admits() {
        let target = EntityId("place:moes".into());
        let pointing = holding(
            "f9",
            "event:winter-fest",
            "arrives_at",
            "place:moes",
            (None, None),
            Provenance::Testimony,
        );
        let admitting = holding(
            "f1",
            "person:milhouse",
            ADMITS,
            "place:moes",
            (None, Some("2026-01-01")),
            Provenance::Testimony,
        );

        let alone = ranked(
            std::slice::from_ref(&pointing),
            &target,
            date(2026, 8, 19),
            &keys(),
            Widen::WhenEmpty,
        );
        assert_eq!(
            alone
                .iter()
                .map(|held| (held.fact.id.0.as_str(), held.standing))
                .collect::<Vec<_>>(),
            vec![("f9", Standing::Related)],
            "with nothing admitting, what else points here is the answer"
        );

        let both = vec![pointing.clone(), admitting];
        let with_a_pass = ranked(&both, &target, date(2026, 8, 19), &keys(), Widen::WhenEmpty);

        // **A call that walks links of its own does not get the wider set**,
        // even with nothing admitting: it said what it was after.
        assert!(
            ranked(
                std::slice::from_ref(&pointing),
                &target,
                date(2026, 8, 19),
                &keys(),
                Widen::Never,
            )
            .is_empty(),
            "the wider set arrived beside a walk the caller chose",
        );
        assert_eq!(
            with_a_pass
                .iter()
                .map(|held| held.fact.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["f1"],
            "an entitlement is here, so the wider set stays out — even lapsed"
        );
    }

    /// A record pointing at something else is not an answer about this thing.
    #[test]
    fn a_record_pointing_elsewhere_is_not_in_the_answer() {
        let facts = vec![holding(
            "f1",
            "person:milhouse",
            ADMITS,
            "event:leaving-party",
            (None, None),
            Provenance::Testimony,
        )];
        assert!(
            ranked(
                &facts,
                &EntityId("event:winter-fest".into()),
                date(2026, 8, 19),
                &keys(),
                Widen::WhenEmpty,
            )
            .is_empty(),
        );
    }
}
