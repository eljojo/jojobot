//! **The loop room** — two phases, and the second one is cold.
//!
//! The bike room asks whether the occupant did the work. This one asks whether
//! it did the work in a shape the software can answer questions about, and it
//! asks by ENDING ON A QUESTION NO SENTENCE ANSWERS: which of these loops has
//! gone quiet as of a named day is cadence plus an anchor plus a date, worked
//! out by the machinery. A loop written down as prose is not a late loop — it
//! is not a loop — and no amount of careful reading gets a session past that.
//!
//! **The second phase is cold for a reason.** Inside one session a model
//! answers any question out of its own context: it wrote the thing, so it
//! remembers it, and the store is optional. A session with no memory of the
//! first is what makes the store load-bearing, and it is the only arrangement
//! in which a badly shaped write costs anybody anything.
//!
//! The occupant names its own loops. **Nothing here pins a handle it chose** —
//! the loops are found by the things they hang under, which is furniture.

use serde_json::{Value, json};

use crate::run::{Expectation, Observed, Outcome};
use crate::surface::Seed;

/// The document this room is driven by.
pub const LOOP_ROOM: &str = "LOOP-ROOM.md";

/// The three things a loop hangs under. The fern is furnished with its loop
/// already keeping properly: it is the vocabulary the occupant can find, and
/// the control that stays untouched at the end.
const KETTLE: &str = "thing:kettle";
const FILTER: &str = "thing:the-air-filter";
const FERN: &str = "thing:the-fern";

/// The days the brief gives, and the day the fern was last watered. All three
/// are the room's own values, so a check that reads them is pinning data.
const KETTLE_LAST_DONE: &str = "2026-06-15";
const FILTER_LAST_DONE: &str = "2026-09-20";
const FERN_LAST_DONE: &str = "2026-08-01";

/// The key a loop keeps its last turn under, and the key that carries its
/// frequency. Both are the shipped kind's own; the occupant is told neither.
const LAST_TURN: &str = "last_check_in";
const CADENCE: &str = "cadence_days";

/// What the brief is called. Shared by the furniture that posts it and the
/// check that has to tell it from anything the occupant leaves.
const BRIEF: &str = "the things I keep having to do";

/// **The whole of what the occupant is told in the first phase.**
///
/// The work and none of the method. It says the fern already works, which is
/// where the vocabulary is to be found, and it names no key, no verb and no
/// place anything is kept.
const BRIEF_BODY: &str = "\
The fern is already on here and that one works the way I want. Two more things \
to keep the same way.

The kettle needs descaling every sixty days. I last did it on 2026-06-15.

The air filter I swap when it looks bad, so there is no schedule for that one \
at all. The last swap was 2026-09-20.

Leave it so whoever comes next can pick it up.";

/// **What the room holds before the occupant arrives.**
///
/// Three things, and one loop kept properly under one of them. The fern's loop
/// is the room's teacher and its control: it shows what a kept loop looks like,
/// and it is one of the two that must be left alone at the end.
pub fn seed() -> anyhow::Result<Seed> {
    Ok(Seed::new()
        .entity("thing", "kettle", "The Kettle")?
        .entity("thing", "the-air-filter", "The Air Filter")?
        .entity("thing", "the-fern", "The Fern")?
        .child(FERN, "rhythm", "water-the-fern", "Water the fern")?
        .record(
            "rhythm:water-the-fern",
            "water it every ninety days, and it survives being forgotten",
            json!({
                "name": "Water the fern",
                LAST_TURN: FERN_LAST_DONE,
                CADENCE: "90",
                "counts_from": FERN_LAST_DONE,
                "advances_from": "due_date",
            }),
        )?
        .message("assistant", BRIEF, BRIEF_BODY))
}

/// The four locks, in the order the goal reaches them.
pub fn expectations() -> Vec<Box<dyn Expectation>> {
    vec![
        Box::new(TheBriefLeftTheBox),
        Box::new(BothLoopsStandAsLoops),
        Box::new(TheLoopsCarryTheirScheduleAndTheirLastTurn),
        Box::new(TheLoopThatWentQuietGotTheCheckIn),
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

/// **Lock 1 — the brief left the box.** The first lock of every room, and the
/// one the others stand on: a session that never opened its box never learnt
/// what the work was. Delivery is the claim, never the verb that took it.
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
        held(
            self.name(),
            format!(
                "the brief is {}, so delivery was taken",
                brief["state"].as_str().unwrap_or("moved"),
            ),
        )
    }
}

/// **Lock 2 — both loops stand as loops.**
///
/// A recurring job is a thing in its own right, hanging under whatever it is a
/// job for. A session that wrote two sentences onto the kettle and the filter
/// has recorded the same words and left nothing that can fall due.
///
/// **Found by the thing each hangs under, never by name.** The occupant chooses
/// what to call its loops, and a check that pinned a handle would be asserting
/// that it guessed the same word this file did.
struct BothLoopsStandAsLoops;

#[async_trait::async_trait]
impl Expectation for BothLoopsStandAsLoops {
    fn name(&self) -> &str {
        "Phase 1 — both jobs stand as loops under the things they belong to"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let listed = seen
            .room
            .call("list_entities", json!({"kind": "rhythm"}))
            .await;
        // The positive: the furnished loop is in the listing, so an empty
        // answer cannot read as two missing loops.
        if loops_under(&listed, FERN).is_empty() {
            return missed(
                self.name(),
                format!("the furnished loop is not in the listing, so nothing was read: {listed}"),
            );
        }
        let mut short = Vec::new();
        for (thing, what) in [(KETTLE, "the kettle"), (FILTER, "the air filter")] {
            match loops_under(&listed, thing).len() {
                1 => {}
                0 => short.push(format!("no loop hangs under {what}")),
                many => short.push(format!("{many} loops hang under {what}")),
            }
        }
        if short.is_empty() {
            held(
                self.name(),
                "one loop under each of the two things the brief named",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// **Lock 3 — the loops carry their schedule and the day they were last done.**
///
/// Four halves, and the load-bearing one is an absence: **the loop the operator
/// keeps no schedule for was taken anyway**, with no cadence on it. A surface
/// that demanded one would have lost that loop entirely, and a check that only
/// looked at the scheduled loop would never notice.
///
/// The kettle's day is read off the key's own history rather than off what it
/// holds now, because the cold phase moves it — the first write is the day the
/// brief gave, and that stays true afterwards.
struct TheLoopsCarryTheirScheduleAndTheirLastTurn;

#[async_trait::async_trait]
impl Expectation for TheLoopsCarryTheirScheduleAndTheirLastTurn {
    fn name(&self) -> &str {
        "Phase 1 — each loop carries its own schedule, or none, and its last turn"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let listed = seen
            .room
            .call("list_entities", json!({"kind": "rhythm"}))
            .await;
        let (Some(kettle), Some(filter)) = (
            loops_under(&listed, KETTLE).first().cloned(),
            loops_under(&listed, FILTER).first().cloned(),
        ) else {
            return missed(
                self.name(),
                "one of the two loops is not there, so there is nothing to read a schedule off",
            );
        };

        let mut short = Vec::new();
        let scheduled = fields_of(seen, &kettle).await;
        if !scheduled.contains_key(CADENCE) {
            short.push("the loop with a frequency carries none".to_string());
        }
        let turns = history(seen, &kettle, LAST_TURN).await;
        match turns.first() {
            Some(first) if first == KETTLE_LAST_DONE => {}
            Some(first) => short.push(format!(
                "the kettle's first turn reads {first:?} rather than the day the brief gave"
            )),
            None => short.push(format!("the kettle's loop carries no {LAST_TURN} at all")),
        }

        let unscheduled = fields_of(seen, &filter).await;
        // The half that matters: a loop nobody set a frequency for is still a
        // loop, and a run that invented one for it has recorded something the
        // operator did not say.
        if unscheduled.contains_key(CADENCE) {
            short.push(
                "the loop the operator keeps no schedule for was given one, which nobody said"
                    .to_string(),
            );
        }
        if unscheduled.get(LAST_TURN).and_then(Value::as_str) != Some(FILTER_LAST_DONE) {
            short.push(format!(
                "the filter's last turn reads {:?} rather than the day the brief gave",
                unscheduled.get(LAST_TURN),
            ));
        }

        if short.is_empty() {
            held(
                self.name(),
                "one loop with a cadence and its day, one with a day and deliberately no cadence",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// **Lock 4 — the loop that had gone quiet got the check-in, and only it.**
///
/// The terminal lock, and the reason the room has a cold phase. As of the day
/// the operator named, exactly one of the three loops has fallen due: the fern
/// is not due for another month, the filter has no schedule and so can never be
/// late, and the kettle went past its day in August.
///
/// **The answer is not in any sentence.** A session that recorded its loops as
/// prose has nothing to ask, and a session that recorded them properly can ask
/// and be told. So this measures the FIRST phase's structure, from the one
/// place where reading carefully cannot rescue it.
///
/// Both halves: the one that had gone quiet moved, and the two that had not are
/// exactly where they were.
struct TheLoopThatWentQuietGotTheCheckIn;

#[async_trait::async_trait]
impl Expectation for TheLoopThatWentQuietGotTheCheckIn {
    fn name(&self) -> &str {
        "Phase 2 — the loop that had gone quiet is the one that moved"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let listed = seen
            .room
            .call("list_entities", json!({"kind": "rhythm"}))
            .await;
        let Some(kettle) = loops_under(&listed, KETTLE).first().cloned() else {
            return missed(
                self.name(),
                "the loop that had gone quiet was never recorded, so the question it answers \
                 could not be asked",
            );
        };
        let turns = history(seen, &kettle, LAST_TURN).await;
        if turns.len() < 2 {
            return missed(
                self.name(),
                format!(
                    "the loop that had gone quiet carries {} turn(s), so the cold session left \
                     it where it was: {turns:?}",
                    turns.len(),
                ),
            );
        }
        let mut short = Vec::new();
        if turns.last().map(String::as_str) == Some(KETTLE_LAST_DONE) {
            short.push("the newest turn is still the day the brief gave".to_string());
        }
        // The two that had not gone quiet. A session that checked everything in
        // did not answer the question, it painted the wall.
        for (thing, what, day) in [
            (FILTER, "the filter", FILTER_LAST_DONE),
            (FERN, "the fern", FERN_LAST_DONE),
        ] {
            let Some(other) = loops_under(&listed, thing).first().cloned() else {
                short.push(format!(
                    "{what}'s loop is not there to have been left alone"
                ));
                continue;
            };
            let turns = history(seen, &other, LAST_TURN).await;
            if turns.len() != 1 || turns.first().map(String::as_str) != Some(day) {
                short.push(format!(
                    "{what} was not due and was checked in anyway: {turns:?}"
                ));
            }
        }
        if short.is_empty() {
            held(
                self.name(),
                format!(
                    "the loop that had gone quiet moved to {:?} and the other two did not",
                    turns.last()
                ),
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// The handles of the loops hanging under one thing, read off a listing.
fn loops_under(listed: &str, parent: &str) -> Vec<String> {
    serde_json::from_str::<Value>(listed)
        .ok()
        .and_then(|body| {
            body["entities"].as_array().map(|all| {
                all.iter()
                    .filter(|entity| entity["parent"] == parent)
                    .filter_map(|entity| entity["id"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// What a thing holds now — every key on it, folded.
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

/// Every write of one key on one thing, oldest first.
async fn history(seen: &Observed<'_>, subject: &str, key: &str) -> Vec<String> {
    let read = seen
        .room
        .call("recall", json!({"subject": subject, "history": key}))
        .await;
    serde_json::from_str::<Value>(&read)
        .ok()
        .and_then(|body| {
            body["objects"][0]["history"]["writes"].as_array().map(|w| {
                w.iter()
                    .filter_map(|write| write["value"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}
