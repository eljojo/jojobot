//! **The claims events a claim write makes**, built from the claim as it was
//! written. A store resolves the names it holds into keys and stored identities,
//! reads who wrote and when, and calls these; none of them reads a store. The
//! dolt writers and the fake write their events through the same functions, so a
//! claim the State holds is the claim the import would have made of the same row
//! (rule 9.3.3-d (a)).

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;

use super::import::{EVENT_KIND, MEANING_VERSION};
use super::{ClaimBody, ClaimEvent, FieldValue, KeyId, MovedRecord, MovedWrite};
use crate::actor::Actor;
use crate::log::{Event, ObjectId, RecordId, RunId};
use crate::memory::merge::{ServedWrite, Side, interleave};
use crate::memory::{Fact, WriteClass};

/// **Who wrote, in which run, and when.** Read from the write itself by the store
/// that made it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    pub actor: Option<Actor>,
    pub run: Option<String>,
    pub written_at: Option<Timestamp>,
}

/// **Where a claim is served and what it points at**, every name turned into the
/// key it stands for. A pointer to a claim names the record that claim is stored
/// in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    pub home: ObjectId,
    pub label: String,
    pub edge: Option<(String, ObjectId)>,
    pub derived_from: Option<RecordId>,
    pub refs: Vec<ObjectId>,
    pub stands_for: Vec<RecordId>,
}

fn event(origin: &Origin, payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
        event_kind: EVENT_KIND.into(),
        meaning_version: MEANING_VERSION,
        actor: origin.actor.clone(),
        run: origin.run.clone().map(RunId),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: origin.written_at,
        imported_source_time: None,
        payload,
    }
}

/// **A claim as the State holds it**, from the claim as it was written and the
/// keys the store resolved for it.
pub fn claim_body(fact: &Fact, served: Served) -> ClaimBody {
    ClaimBody {
        label: served.label,
        home: served.home,
        content: fact.content.clone(),
        details: fact.details.clone(),
        provenance: fact.provenance,
        standing: fact.standing,
        status: fact.status,
        recorded_at: fact.recorded_at,
        happened_at: fact.happened_at,
        happened_through: fact.happened_through,
        stale_after: fact.stale_after,
        edge: served.edge,
        derived_from: served.derived_from,
        refs: served.refs,
        stands_for: served.stands_for,
        receipt: fact.receipt.clone(),
        app: fact.app_stamp.clone(),
        inserted_at: fact.inserted_at,
    }
}

/// **The event of one claim write**: the first write of a claim creates its
/// record and every later one revises it. `ordinal` is the place of the write
/// among the claim's writes, from one.
pub fn claim_write(
    record: RecordId,
    ordinal: i64,
    claim: ClaimBody,
    origin: &Origin,
) -> Event<ClaimEvent> {
    event(
        origin,
        if ordinal == 1 {
            ClaimEvent::RecordCreated { record, claim }
        } else {
            ClaimEvent::RecordRevised { record, claim }
        },
    )
}

/// **The event of one key write**: a value, or a clear when there is none.
/// `targets` are the keys of the things the value points at.
#[allow(clippy::too_many_arguments)]
pub fn key_write(
    record: RecordId,
    subject: ObjectId,
    key: &str,
    ordinal: u64,
    class: WriteClass,
    value: &Option<String>,
    targets: Vec<ObjectId>,
    origin: &Origin,
) -> Event<ClaimEvent> {
    event(
        origin,
        ClaimEvent::FieldWritten {
            record,
            subject,
            key: KeyId(key.to_string()),
            ordinal,
            class,
            write: match value {
                Some(value) => FieldValue::Set(value.clone()),
                None => FieldValue::Clear,
            },
            targets,
        },
    )
}

/// **A write and the place a merge gives it among the survivor's.**
#[derive(Debug, Clone)]
pub struct Placement<T> {
    pub side: Side,
    pub write: T,
    /// The place the write holds in the key's order after the merge, from one.
    pub ordinal: i64,
}

/// **Where a merge puts the writes of each key**, by the one rule the store
/// reads them by ([`interleave`]): the folded thing's writes of a key, only those
/// of a claim the merge gave a new id, interleaved with the survivor's by the
/// moment each was made and numbered from one. A key the folded thing never wrote
/// is left as it is, and so is a survivor write that keeps its place.
pub fn placements<T: ServedWrite>(
    theirs: Vec<T>,
    ours: Vec<T>,
    claims: &HashMap<&str, &str>,
) -> Vec<Placement<T>> {
    type Sides<T> = (Vec<T>, Vec<T>);
    let mut by_key: BTreeMap<String, Sides<T>> = BTreeMap::new();
    for write in theirs {
        if claims.contains_key(write.at().3) {
            by_key
                .entry(write.at().0.to_string())
                .or_default()
                .0
                .push(write);
        }
    }
    for write in ours {
        by_key
            .entry(write.at().0.to_string())
            .or_default()
            .1
            .push(write);
    }
    let mut placed = Vec::new();
    for (_, (mut theirs, mut ours)) in by_key {
        if theirs.is_empty() {
            continue;
        }
        theirs.sort_by_key(|write| write.at().1);
        ours.sort_by_key(|write| write.at().1);
        for (place, (side, write)) in interleave(theirs, ours).into_iter().enumerate() {
            let ordinal = place as i64 + 1;
            if side == Side::Survivor && ordinal == write.at().1 {
                continue;
            }
            placed.push(Placement {
                side,
                write,
                ordinal,
            });
        }
    }
    placed
}

/// **The event of a merge**: who was folded into whom, the claims it gave new
/// ids and the writes whose places it moved.
pub fn merge(
    folded: ObjectId,
    survivor: ObjectId,
    records: Vec<MovedRecord>,
    writes: Vec<MovedWrite>,
    origin: &Origin,
) -> Event<ClaimEvent> {
    event(
        origin,
        ClaimEvent::ObjectsMerged {
            folded,
            survivor,
            records,
            writes,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::claims::import::record_id;

    /// A key write as a merge reads it.
    #[derive(Debug, Clone, PartialEq)]
    struct Write {
        key: &'static str,
        ordinal: i64,
        at: i64,
        fact: &'static str,
    }

    impl ServedWrite for Write {
        fn at(&self) -> (&str, i64, Option<Timestamp>, &str) {
            (
                self.key,
                self.ordinal,
                Some(Timestamp::from_second(self.at).expect("a moment")),
                self.fact,
            )
        }
        fn place(&mut self, ordinal: i64, _fact: Option<&str>) {
            self.ordinal = ordinal;
        }
    }

    fn write(key: &'static str, ordinal: i64, at: i64, fact: &'static str) -> Write {
        Write {
            key,
            ordinal,
            at,
            fact,
        }
    }

    fn body() -> ClaimBody {
        ClaimBody {
            label: "f1".into(),
            home: ObjectId("k1".into()),
            content: "c".into(),
            details: None,
            provenance: crate::memory::Provenance::Inference,
            standing: crate::memory::Standing::Open,
            status: crate::memory::FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 9, 1),
            happened_at: None,
            happened_through: None,
            stale_after: None,
            edge: None,
            derived_from: None,
            refs: Vec::new(),
            stands_for: Vec::new(),
            receipt: None,
            app: None,
            inserted_at: None,
        }
    }

    /// **The first write of a claim creates its record and a later one revises it.**
    #[test]
    fn the_first_claim_write_creates_and_a_later_one_revises() {
        let record = record_id(&ObjectId("k1".into()), "f1");
        let origin = Origin::default();
        let first = claim_write(record.clone(), 1, body(), &origin);
        let later = claim_write(record, 2, body(), &origin);
        assert!(matches!(first.payload, ClaimEvent::RecordCreated { .. }));
        assert!(matches!(later.payload, ClaimEvent::RecordRevised { .. }));
    }

    /// **A key write with no value is a clear, and the envelope carries the
    /// origin it was given.**
    #[test]
    fn a_key_write_without_a_value_is_a_clear_in_the_origin_it_was_given() {
        let origin = Origin {
            actor: None,
            run: Some("run-a".into()),
            written_at: Some(Timestamp::from_second(1_700_000_000).unwrap()),
        };
        let record = record_id(&ObjectId("k1".into()), "f1");
        let cleared = key_write(
            record.clone(),
            ObjectId("k1".into()),
            "mood",
            3,
            WriteClass::Sets,
            &None,
            Vec::new(),
            &origin,
        );
        let set = key_write(
            record,
            ObjectId("k1".into()),
            "mood",
            4,
            WriteClass::Sets,
            &Some("calm".into()),
            Vec::new(),
            &origin,
        );
        assert!(matches!(
            cleared.payload,
            ClaimEvent::FieldWritten {
                write: FieldValue::Clear,
                ..
            }
        ));
        assert!(matches!(
            set.payload,
            ClaimEvent::FieldWritten { write: FieldValue::Set(ref v), .. } if v == "calm"
        ));
        assert_eq!(cleared.run, Some(RunId("run-a".into())));
        assert_eq!(cleared.written_at, origin.written_at);
    }

    /// **A folded write placed among the survivor's moves the later ones on**, and
    /// a survivor write that keeps its place is not named.
    #[test]
    fn a_merge_places_the_folded_write_and_names_the_survivor_writes_it_moves() {
        let claims = HashMap::from([("g1", "f9")]);
        let placed = placements(
            vec![write("mood", 1, 20, "g1")],
            vec![write("mood", 1, 10, "f1"), write("mood", 2, 30, "f2")],
            &claims,
        );
        let got: Vec<(Side, &str, i64)> = placed
            .iter()
            .map(|p| (p.side, p.write.fact, p.ordinal))
            .collect();
        assert_eq!(
            got,
            vec![(Side::Folded, "g1", 2), (Side::Survivor, "f2", 3)]
        );
    }

    /// **A folded write of a claim the merge did not map is left out**, and so is
    /// a key the folded thing never wrote.
    #[test]
    fn a_merge_leaves_out_unmapped_writes_and_keys_the_fold_never_wrote() {
        let claims = HashMap::from([("g1", "f9")]);
        let placed = placements(
            vec![write("mood", 1, 20, "unmapped")],
            vec![write("mood", 1, 10, "f1"), write("hat", 1, 10, "f1")],
            &claims,
        );
        assert!(placed.is_empty());
    }
}
