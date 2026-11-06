//! **The differential**: for a run of `add_entity` and `capture` calls, what
//! [`prepare`] answers over the state equals what the fake store answers to the
//! same capture, and the state is kept by the events [`prepare`] admits.
//!
//! The fake is the oracle and the state is built only from the events each
//! admitted capture gives, so the two copies of capture's policy can fail
//! against each other. Nothing here calls a helper [`prepare`] uses to decide.
//! The other writers are not in the run: the edit, the merge, the archive and
//! the import are the switch's and the import's, whose replay completes the
//! property at activation.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::date;

use super::super::tests::{apply, body, object};
use super::super::{ClaimEvent, ObjectBody, State};
use super::{CaptureRequest, ClaimSize, Door, Measures, Refusal, Route, prepare};
use crate::actor::Actor;
use crate::log::RecordId;
use crate::memory::testing::InMemoryMemory;
use crate::memory::{Edge, EdgeShape, EntityId, Guarded, Memory, MemoryError, NewEntity, NewFact};

/// A small deterministic generator: the property needs repeatable runs.
struct Dice(u64);

impl Dice {
    fn roll(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.roll(from.len())]
    }
}

/// The things a run creates, in the order it creates them: the handle, its
/// kind, and the parent the kind requires or none.
const ROSTER: [(&str, &str, Option<&str>); 5] = [
    ("thing:alpha", "thing", None),
    ("promise:beta", "promise", Some("thing:alpha")),
    ("work:gamma", "work", None),
    ("project:delta", "project", None),
    ("thing:epsilon", "thing", None),
];

/// Keys a capture may carry, each with values the kinds accept and refuse.
const FIELDS: [(&str, &[&str]); 6] = [
    ("promised_by", &["2026-07-01", "", "someday"]),
    ("status", &["now", "done", "waiting", "bogus", ""]),
    ("columns", &["someday,next,now,waiting,done", "now", ""]),
    ("note", &["x", ""]),
    ("mood", &["quiet"]),
    ("leaves_on", &["2026-08-01", "never"]),
];

/// What a capture was answered, as far as the two copies must agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Admitted,
    Absent,
    BreaksKind,
    Other,
}

fn fake_verdict(answer: &Result<Guarded<crate::memory::Fact>, MemoryError>) -> Verdict {
    match answer {
        Ok(Guarded::Written(_)) => Verdict::Admitted,
        Ok(Guarded::Blocked { .. }) => Verdict::Absent,
        Err(MemoryError::BreaksFit { .. } | MemoryError::BreaksType { .. }) => Verdict::BreaksKind,
        Err(_) => Verdict::Other,
    }
}

fn prepared_verdict(answer: &Result<Vec<ClaimEvent>, Refusal>) -> Verdict {
    match answer {
        Ok(_) => Verdict::Admitted,
        Err(
            Refusal::UnknownSubject(..) | Refusal::UnknownEdgeObject(..) | Refusal::UnknownRef(..),
        ) => Verdict::Absent,
        Err(Refusal::BreaksKind(_)) => Verdict::BreaksKind,
        Err(_) => Verdict::Other,
    }
}

fn request_of(fact: &NewFact, record: &str) -> CaptureRequest {
    CaptureRequest::of_fact(
        fact,
        RecordId(record.to_string()),
        Route::Verb,
        Door {
            actor: Actor::Operator,
            owned_keys: BTreeSet::new(),
            owner_revoked: false,
            measures: Measures {
                claim: Some(ClaimSize {
                    size: 1,
                    room: 100,
                    row: None,
                }),
                ..Measures::default()
            },
            runs: BTreeMap::new(),
            run: None,
            aged_before: None,
        },
    )
}

/// The two events that make an object, as the log states them.
fn made(handle: &str, kind: &str, parent: Option<&str>) -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object(handle),
            body: ObjectBody {
                parent: parent.map(object),
                ..body(kind, handle)
            },
        },
        ClaimEvent::AddressBound {
            object: object(handle),
            address: handle.to_string(),
        },
    ]
}

#[tokio::test]
async fn prepare_answers_as_the_fake_store_does_for_add_entity_and_capture() {
    crate::memory::kinds::load_shipped();
    let mut divergences: Vec<String> = Vec::new();
    // What the runs covered: a property that never meets a verdict says
    // nothing about it.
    let mut met: BTreeMap<String, usize> = BTreeMap::new();
    let mut served_facts = 0usize;
    for seed in 0..40u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let fake = InMemoryMemory::seeded();
        let mut state = State::default();
        let mut created: Vec<&str> = Vec::new();
        let mut records = 0;
        for step in 0..30 {
            let next = ROSTER.get(created.len()).copied();
            if let Some((handle, kind, parent)) = next
                && dice.roll(3) == 0
            {
                let entity = NewEntity {
                    parent: parent.map(|parent| EntityId(parent.to_string())),
                    ..NewEntity::new(EntityId(handle.to_string()), handle, "differential")
                };
                if let Ok(Guarded::Written(_)) = fake.add_entity(entity).await {
                    for event in made(handle, kind, parent) {
                        apply(&mut state, &event);
                    }
                    created.push(handle);
                }
                continue;
            }
            let subject = if created.is_empty() || dice.roll(10) == 0 {
                "thing:omega"
            } else {
                created[dice.roll(created.len())]
            };
            let content = dice.pick(&["a claim", "a claim", "a claim", "", "two\nlines"]);
            let mut fact =
                NewFact::about(EntityId(subject.to_string()), content, date(2026, 4, 19));
            if dice.roll(8) == 0 {
                fact.happened_through = Some(date(2026, 4, 20));
            }
            for _ in 0..dice.roll(3) {
                let (key, values) = FIELDS[dice.roll(FIELDS.len())];
                fact.fields
                    .insert(key.to_string(), dice.pick(values).to_string());
            }
            fact.sets = fact
                .fields
                .keys()
                .filter(|_| dice.roll(2) == 0)
                .cloned()
                .collect();
            if dice.roll(5) == 0 {
                let object = if dice.roll(2) == 0 {
                    "thing:omega"
                } else {
                    "thing:alpha"
                };
                fact.edge = Some(Edge {
                    shape: dice.pick(&[EdgeShape::Connection, EdgeShape::Location]),
                    object: EntityId(object.to_string()),
                });
            }
            records += 1;
            let record = format!("r{records}");
            let wanted = prepare(&state, &[], &request_of(&fact, &record));
            let got = fake.capture(fact.clone()).await;
            let (ours, theirs) = (prepared_verdict(&wanted), fake_verdict(&got));
            if ours != theirs {
                divergences.push(format!(
                    "seed {seed} step {step}: {} {:?} {:?}: prepare {wanted:?}, fake {got:?}",
                    fact.subject.0, fact.fields, fact.sets
                ));
                break;
            }
            *met.entry(format!("{ours:?}")).or_default() += 1;
            if let Ok(events) = wanted {
                for event in &events {
                    apply(&mut state, event);
                }
                // The fact the State serves for the record equals the fact the
                // fake store returned for the same capture, but for the moment
                // each stamped.
                if let Ok(Guarded::Written(theirs)) = &got {
                    let served = state.fact(&RecordId(record.clone())).map(|mut fact| {
                        fact.inserted_at = None;
                        fact
                    });
                    let mut theirs = theirs.clone();
                    theirs.inserted_at = None;
                    if served.as_ref() != Some(&theirs) {
                        divergences.push(format!(
                            "seed {seed} step {step}: the State serves {served:?}, the fake returned {theirs:?}"
                        ));
                        break;
                    }
                    served_facts += 1;
                }
            }
        }
    }
    for verdict in ["Admitted", "Absent", "BreaksKind", "Other"] {
        assert!(
            met.get(verdict).is_some_and(|count| *count > 0),
            "no run met the verdict {verdict}: {met:?}"
        );
    }
    assert!(served_facts > 0, "no run compared a served fact");
    assert!(
        divergences.is_empty(),
        "{} runs diverged:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}
