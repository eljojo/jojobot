//! **The ledger room** — the words the operator uses, and the ones nobody
//! agreed to.
//!
//! Two phases, the second cold, exactly as the loop room. The terminal question
//! is *which of these is filled in with a word I do not use*, and the answer is
//! reachable in one way: **the set of words the operator allows lives in a
//! DECLARATION, and the declaration is the only thing that carries it out of
//! the first phase.** The brief says the three words once, to a session that is
//! gone by the time the question is asked. A session that wrote the jobs down
//! and declared nothing has left the cold session no way to know that `sent` is
//! not one of the operator's words, because on the record it looks exactly like
//! `paid`.
//!
//! **What a thing HOLDS is what its keys fold to**, and conformance is asked of
//! the thing rather than of one record. So the words nobody agreed to are the
//! CURRENT word on two things, and the jobs the brief adds go on two others —
//! a new job on the same thing would fold over the odd word and hide the
//! question this room is built around. That is not a detail: the first version
//! of this room put them on one thing, and the terminal question came back
//! saying everything was in order.
//!
//! **The occupant names its own type**, so nothing here asserts the
//! declaration by name. It is not asserted at all: it is what the terminal lock
//! is reachable through, which is a stronger claim than its existence.

use serde_json::{Value, json};

use crate::run::{Expectation, Observed, Outcome};
use crate::surface::Seed;

/// The document this room is driven by.
pub const LEDGER_ROOM: &str = "LEDGER-ROOM.md";

/// The things whose current word is one of the operator's, and which nothing
/// in this room should touch.
const RIGHT: [(&str, &str); 2] = [
    ("thing:jukebox", "paid"),
    ("thing:torque-wrench", "invoiced"),
];

/// The things whose current word is one nobody agreed to.
const WRONG: [&str; 2] = ["thing:kettle", "thing:the-air-filter"];

/// The two things the brief's new jobs go on. They carry no job at all, so the
/// occupant's write is the whole of what they hold.
const PUMP: &str = "thing:floor-pump";
const BIKE: &str = "thing:gravel-bike";

/// The keys the older jobs already use. The occupant is told neither; they are
/// there to be found and reused, and a job written under a key of somebody's
/// own invention is a job the operator's question does not reach.
const COST: &str = "cost";
const SETTLED: &str = "settled";

/// The two jobs the brief adds, and what they cost.
const PUMP_JOB: &str = "35";
const BIKE_JOB: &str = "60";

/// The words the operator uses, and the two that somebody wrote before anybody
/// agreed on them. **The odd ones are two rather than one on purpose**: one odd
/// value among four is a pattern a reader can guess at, and two make the
/// question about the set rather than about the outlier.
const ALLOWED: [&str; 3] = ["invoiced", "paid", "waived"];
const NOT_AGREED: [&str; 2] = ["pending", "sent"];

/// What the brief is called.
const BRIEF: &str = "the jobs I pay for, and the words I want on them";

/// **The whole of what the occupant is told in the first phase**, and the only
/// place the three words are ever said.
const BRIEF_BODY: &str = "\
I have been writing down the jobs I pay people for, and I want to be able to \
ask which of them are still owing.

From now on there are three words for where a job has got to, and no others: \
invoiced, paid, waived.

Two from last week to put on. The floor pump was serviced, thirty five, and I \
paid on the spot. The bike has a new chain, sixty, and they have invoiced me \
for that one.

Leave it so whoever comes next can pick it up.";

/// **What the room holds before the occupant arrives.**
///
/// Five jobs written months ago by somebody who had no type in mind, three of
/// them using words the operator does use and two using words nobody agreed to.
pub fn seed() -> anyhow::Result<Seed> {
    Ok(Seed::new()
        .entity("thing", "jukebox", "The Jukebox")?
        .entity("thing", "torque-wrench", "The Torque Wrench")?
        .entity("thing", "kettle", "The Kettle")?
        .entity("thing", "the-air-filter", "The Air Filter")?
        .entity("thing", "floor-pump", "The Floor Pump")?
        .entity("thing", "gravel-bike", "The Gravel Bike")?
        .record(
            "thing:jukebox",
            "new valves",
            json!({COST: "180", SETTLED: "paid"}),
        )?
        .record(
            "thing:torque-wrench",
            "calibration",
            json!({COST: "55", SETTLED: "invoiced"}),
        )?
        .record(
            "thing:kettle",
            "descaled by the shop",
            json!({COST: "25", SETTLED: "pending"}),
        )?
        .record(
            "thing:the-air-filter",
            "filter swap",
            json!({COST: "18", SETTLED: "sent"}),
        )?
        .message("assistant", BRIEF, BRIEF_BODY))
}

/// The three locks, in the order the goal reaches them.
pub fn expectations() -> Vec<Box<dyn Expectation>> {
    vec![
        Box::new(TheBriefLeftTheBox),
        Box::new(TheNewJobsUseTheWordsAndTheKeys),
        Box::new(TheWordsNobodyAgreedToAreGone),
    ]
}

fn held(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: true,
        saying: saying.into(),
    }
}

fn missed(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: false,
        saying: saying.into(),
    }
}

/// **Lock 1 — the brief left the box.**
struct TheBriefLeftTheBox;

#[async_trait::async_trait]
impl Expectation for TheBriefLeftTheBox {
    fn name(&self) -> &str {
        "Phase 1 — the brief left the box"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let board = seen
            .room
            .call(
                "search",
                json!({"query": "*", "include_mail": true, "limit": 200}),
            )
            .await;
        let brief = serde_json::from_str::<Value>(&board).ok().and_then(|body| {
            body["results"].as_array().and_then(|hits| {
                hits.iter()
                    .find(|hit| hit["hit"] == "message" && hit["subject"] == BRIEF)
                    .cloned()
            })
        });
        let Some(brief) = brief else {
            return missed(
                self.name(),
                format!("the brief is not on the board, so the room was never furnished: {board}"),
            );
        };
        if brief["state"] == "new" {
            return missed(
                self.name(),
                "the brief is still waiting, so nothing here says the occupant found its box",
            );
        }
        held(self.name(), "the brief left new, so delivery was taken")
    }
}

/// **Lock 2 — the new jobs use the keys the old ones use, and a word the
/// operator uses.**
///
/// The vocabulary lock. The room keeps its jobs under two keys and says nothing
/// about either; a job written under a key of the occupant's own invention is a
/// job the operator's question never reaches, and a job whose state is a
/// sentence is one nothing can ask about.
struct TheNewJobsUseTheWordsAndTheKeys;

#[async_trait::async_trait]
impl Expectation for TheNewJobsUseTheWordsAndTheKeys {
    fn name(&self) -> &str {
        "Phase 1 — the new jobs use the keys the old jobs use, and a word the operator uses"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let mut short = Vec::new();
        for (thing, what, paid) in [(PUMP, "the pump", PUMP_JOB), (BIKE, "the bike", BIKE_JOB)] {
            let holds = fields_of(seen, thing).await;
            if holds.is_empty() {
                short.push(format!("{what} carries no job at all"));
                continue;
            }
            match holds.get(COST).and_then(Value::as_str) {
                Some(cost) if digits(cost) == paid => {}
                other => short.push(format!(
                    "{what} says {other:?} for what the job cost rather than {paid}"
                )),
            }
            let word = holds.get(SETTLED).and_then(Value::as_str);
            if !word.is_some_and(|said| ALLOWED.contains(&said)) {
                short.push(format!(
                    "the new job on {what} says {word:?} for where it has got to"
                ));
            }
        }
        // The positive the whole check rests on: the older jobs are still there
        // to have been read, so an empty room cannot read as a bad one.
        if fields_of(seen, RIGHT[0].0).await.is_empty() {
            return missed(
                self.name(),
                "the older jobs are not in the room, so the room was never furnished",
            );
        }
        if short.is_empty() {
            held(
                self.name(),
                "both new jobs carry what they cost and one of the operator's three words",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// **Lock 3 — the words nobody agreed to are gone, and the rest are untouched.**
///
/// The terminal lock, and the one the cold phase exists for. Two of the older
/// jobs were written with words that are not the operator's, and finding WHICH
/// needs the set — which lives in a declaration, because the brief said it once
/// to a session that is gone.
///
/// Both halves. A run that rewrote every job to the same word would satisfy the
/// absence and has not answered anything, so the three jobs that were already
/// right must still say what they said.
struct TheWordsNobodyAgreedToAreGone;

#[async_trait::async_trait]
impl Expectation for TheWordsNobodyAgreedToAreGone {
    fn name(&self) -> &str {
        "Phase 2 — the words nobody agreed to are gone, and the rest are as they were"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let mut short = Vec::new();
        for thing in WRONG {
            let word = fields_of(seen, thing).await;
            match word.get(SETTLED).and_then(Value::as_str) {
                None => short.push(format!("{thing} carries no word at all now")),
                Some(said) if NOT_AGREED.contains(&said) => {
                    short.push(format!("{thing} still says {said:?}"))
                }
                Some(said) if !ALLOWED.contains(&said) => short.push(format!(
                    "{thing} now says {said:?}, which is nobody's word either"
                )),
                Some(_) => {}
            }
        }
        // The positive the absence rests on: the jobs that were already right
        // still say what they said. A run that painted every job the same word
        // satisfies an absence and has answered nothing.
        for (thing, word) in RIGHT {
            let holds = fields_of(seen, thing).await;
            if holds.get(SETTLED).and_then(Value::as_str) != Some(word) {
                short.push(format!(
                    "{thing} no longer says {word:?}, so a word was changed that was already right"
                ));
            }
        }
        if short.is_empty() {
            held(
                self.name(),
                "no job carries a word the operator does not use, and the ones that were right \
                 are as they were",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// **What a thing holds now** — every key on it, folded, which is the row a
/// question is asked of. Conformance is the thing's question rather than one
/// record's, so this is the read every check here makes.
async fn fields_of(seen: &Observed<'_>, subject: &str) -> serde_json::Map<String, Value> {
    let read = seen
        .room
        .call("recall", json!({"subject": subject, "facts": false}))
        .await;
    serde_json::from_str::<Value>(&read)
        .ok()
        .and_then(|body| body["objects"][0]["fields"].as_object().cloned())
        .unwrap_or_default()
}

/// The digits in a value, so `240` and `£240` are the same number.
fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}
