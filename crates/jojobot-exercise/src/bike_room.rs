//! **The bike room** — one goal, one line of entry, four locks.
//!
//! The cold-session suite walks a model through numbered steps and asks it to
//! report on each. This room hands over a goal and nothing else: what jojobot
//! is, which verb does what, and even that the work exists at all are things
//! the occupant has to find. **The brief is not in the document — it is waiting
//! in a mailbox**, which is where the first lock is.
//!
//! Everything here is either the furniture the room is built with or a claim
//! about the state the occupant left. **Nothing asserts on what it said.**
//!
//! The document beside this file — [`BIKE_ROOM`] — is the entry and the reading
//! for whoever maintains the room. The words the occupant actually meets live
//! HERE, once: a copy of the brief in the document is a copy that drifts away
//! from the one a run posts.

use serde_json::{Value, json};

use crate::run::{Expectation, Observed, Outcome};
use crate::surface::Seed;

/// The document this room is driven by.
pub const BIKE_ROOM: &str = "BIKE-ROOM.md";

/// The bike the operator rides, and the one in the basement. The second is not
/// decoration: without a bike whose cover has run out, "what is still under
/// warranty" comes back with everything and looks like an answer.
const RIDDEN: &str = "thing:gravel-bike";
const IDLE: &str = "thing:road-bike";

/// **The key the room already keeps its distances under.** The occupant is
/// never told it; it is there to be found, and finding it is what makes this
/// year's number join the years before it instead of starting a set of one.
const TALLY: &str = "km";

/// The day the brief gives for the service, and the distance it gives for the
/// year. Both are the room's own values, so a check that looks for them is
/// pinning data rather than anybody's prose.
const SERVICE_DAY: &str = "2026-08-11";
const THIS_YEAR: &str = "4100";

/// What the brief is called. Shared by the furniture that posts it and the
/// checks that have to tell it from anything the occupant leaves — one
/// constant, because two copies of a subject line drift.
const BRIEF: &str = "the bikes, and a few things I want off them";

/// **The whole of what the occupant is told**, and it arrives as mail.
///
/// It carries the work and none of the method: no verb, no argument, no word
/// about where anything is kept, and nothing about anybody measuring anything.
/// The operator writes to an assistant the way an operator does.
const BRIEF_BODY: &str = "\
Both bikes are on here already, and the gravel one is the one I actually ride.

Three things, none of them urgent.

The gravel bike went in for its service on 2026-08-11 — chain and cables, and \
they left the bearings alone this time. Put it with the rest of what has been \
done to it.

This year came to 4100 km on it. The road bike has not moved at all.

And tell me which of the two is still covered by its warranty. I keep working \
that out by hand and I would rather ask.

When you are done, leave what you did where whoever comes next will pick it \
up. I will ask again in a month.";

/// **What the room holds before the occupant arrives.**
///
/// A fleet with a history: two bikes, the distances one of them has accrued
/// year after year, the cover on each of them, and the last service. The
/// occupant meets a room that has been kept for two years by somebody else,
/// because a room with nothing in it teaches no vocabulary and every key the
/// occupant invents would be as good as any other.
pub fn seed() -> anyhow::Result<Seed> {
    Ok(Seed::new()
        .entity("thing", "gravel-bike", "Gravel Bike")?
        .entity("thing", "road-bike", "Road Bike")?
        .fact(RIDDEN, "ridden most weeks", "testimony")?
        .fact(
            IDLE,
            "hanging in the basement, unridden for two years",
            "testimony",
        )?
        .record(
            RIDDEN,
            "the year's tally for 2024",
            json!({TALLY: "2600", "year": "2024"}),
        )?
        .record(
            RIDDEN,
            "the year's tally for 2025",
            json!({TALLY: "3800", "year": "2025"}),
        )?
        .record(
            RIDDEN,
            "annual service",
            json!({"done_on": "2025-04-18", "work": "chain, cables, bearings"}),
        )?
        .record(RIDDEN, "frame warranty", json!({"expires": "2029-04-11"}))?
        .record(IDLE, "frame warranty", json!({"expires": "2025-06-30"}))?
        .message("assistant", BRIEF, BRIEF_BODY))
}

/// The four locks, in the order the goal reaches them.
pub fn expectations() -> Vec<Box<dyn Expectation>> {
    vec![
        Box::new(TheBriefLeftTheBox),
        Box::new(TheServiceIsAValue),
        Box::new(TheYearJoinedTheOnesBeforeIt),
        Box::new(AHandoffIsWaiting),
    ]
}

/// A check that held, with what it found.
fn held(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: true,
        saying: saying.into(),
    }
}

/// A check that did not, with what was there instead.
fn missed(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: false,
        saying: saying.into(),
    }
}

/// **Lock 1 — the brief left the box.**
///
/// The first lock, and the one every other one stands on: a session that never
/// opened its box never learnt what the work was. **Delivery is the claim, not
/// the verb that took it** — draining the box, taking the one message, posting
/// from inside it and retiring it straight from `new` all move it, and all four
/// are a session that met its mail.
///
/// The positive it rests on: the brief is on the board at all. "No message is
/// waiting" is true of a room nobody furnished, and a check that read the
/// absence as success would pass hardest on the run that went most wrong.
struct TheBriefLeftTheBox;

#[async_trait::async_trait]
impl Expectation for TheBriefLeftTheBox {
    fn name(&self) -> &str {
        "Phase 1 — the brief left the box"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let board = board(seen).await;
        let Some(brief) = messages(&board)
            .into_iter()
            .find(|hit| hit["subject"] == BRIEF)
            .clone()
        else {
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

/// **Lock 2 — the service is on the bike as a VALUE.**
///
/// "Put it with the rest of what has been done to it" is a question somebody
/// asks again later, and a day written into a sentence cannot answer it. So the
/// claim is that the day the brief gave is readable off the bike as a value —
/// under a key on a record, or as the day the record is dated.
///
/// **Which key is the occupant's business.** The room keeps its last service
/// under one name and says nothing about it; a session that reuses that name
/// and a session that invents its own both leave a bike whose service history
/// answers, and this check takes either.
struct TheServiceIsAValue;

#[async_trait::async_trait]
impl Expectation for TheServiceIsAValue {
    fn name(&self) -> &str {
        "Phase 1 — the service went on the bike as a value"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let read = seen
            .room
            .call("recall", json!({"subject": RIDDEN, "facts": true}))
            .await;
        if read.contains("\"status\":\"blocked\"") {
            return missed(
                self.name(),
                format!("the bike is not readable, so nothing was measured: {read}"),
            );
        }
        let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
        let mut values = Vec::new();
        collect_values(&parsed, &mut values);
        if values.iter().any(|value| value == SERVICE_DAY) {
            return held(
                self.name(),
                format!("the bike carries {SERVICE_DAY} as a value a later question can reach"),
            );
        }
        // The two ways of not holding are worth telling apart: a day written
        // into a sentence is a session that recorded the work and lost the
        // question, and no day at all is a session that never got here.
        if read.contains(SERVICE_DAY) {
            return missed(
                self.name(),
                format!(
                    "{SERVICE_DAY} is on the bike as prose only — no key holds it and no record \
                     is dated with it, so what has been done to the bike cannot be asked for"
                ),
            );
        }
        missed(
            self.name(),
            format!("nothing on the bike mentions {SERVICE_DAY} at all"),
        )
    }
}

/// **Lock 3 — this year's number joined the ones before it.**
///
/// The accumulation lock, and the one the room is built around. Two years of
/// distances are already on the bike under a key the occupant is never told;
/// the brief gives a third year's number and never says where it goes. A
/// session that writes it under the same key leaves a bike that can answer
/// "how far, year after year"; a session that writes a sentence leaves three
/// numbers that are not a set.
///
/// **Read as the key's own history**, which is what makes both halves one
/// question: how many times it has been written, and what it holds now.
struct TheYearJoinedTheOnesBeforeIt;

#[async_trait::async_trait]
impl Expectation for TheYearJoinedTheOnesBeforeIt {
    fn name(&self) -> &str {
        "Phase 1 — this year's distance joined the years before it"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let read = seen
            .room
            .call("recall", json!({"subject": RIDDEN, "history": TALLY}))
            .await;
        let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
        let writes: Vec<String> = parsed["objects"][0]["history"]["writes"]
            .as_array()
            .map(|writes| {
                writes
                    .iter()
                    .filter_map(|write| write["value"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        // The furniture wrote two. Fewer than that is a room that was never
        // furnished, and the check must not read that as a session that failed.
        if writes.len() < 2 {
            return missed(
                self.name(),
                format!(
                    "the bike carries {} distance(s), so the room was never furnished with \
                         the years this one had to join: {read}",
                    writes.len()
                ),
            );
        }
        if writes.len() < 3 {
            return missed(
                self.name(),
                format!(
                    "the bike still carries the {} distances it was furnished with, so this \
                     year's number went somewhere a question cannot reach: {writes:?}",
                    writes.len(),
                ),
            );
        }
        // Oldest first, so the newest write is what the bike reads as now.
        let newest = writes.last().map(String::as_str).unwrap_or_default();
        if digits(newest) != THIS_YEAR {
            return missed(
                self.name(),
                format!(
                    "the bike's distance now reads {newest:?} rather than this year's number: \
                     {writes:?}"
                ),
            );
        }
        held(
            self.name(),
            format!(
                "the key carries {} writes and now reads {newest}",
                writes.len()
            ),
        )
    }
}

/// **Lock 4 — a handoff is waiting.**
///
/// "Leave what you did where whoever comes next will pick it up" names no rail,
/// and the product offers two: a message waiting in a box, and a run left open
/// saying what it was working on. **The check takes either**, because choosing
/// one here would fail a session that chose the other and call it a product
/// failure.
///
/// The positive it rests on is the brief again: a board with no mail on it at
/// all is an unfurnished room rather than a session that left nothing.
struct AHandoffIsWaiting;

#[async_trait::async_trait]
impl Expectation for AHandoffIsWaiting {
    fn name(&self) -> &str {
        "Phase 1 — a handoff is waiting for whoever comes next"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let board = board(seen).await;
        let mail = messages(&board);
        if !mail.iter().any(|hit| hit["subject"] == BRIEF) {
            return missed(
                self.name(),
                format!("the brief is not on the board, so the room was never furnished: {board}"),
            );
        }
        if let Some(left) = mail.iter().find(|hit| hit["subject"] != BRIEF) {
            return held(
                self.name(),
                format!(
                    "a message the occupant left is in the {} box",
                    left["mailbox"].as_str().unwrap_or("?"),
                ),
            );
        }
        // The other rail: a run nobody wrapped, saying what it was doing. The
        // door is the only place a run's record is served, and being offered
        // one is exactly how the next session would meet it.
        let door = seen
            .room
            .call("start_here", json!({"bot": "assistant", "brief": true}))
            .await;
        let parsed: Value = serde_json::from_str(&door).unwrap_or(Value::Null);
        let working_on = parsed["session"]["choices"]
            .as_array()
            .map(|runs| {
                runs.iter()
                    .filter_map(|run| run["working_on"].as_str())
                    .find(|said| !said.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        match working_on {
            Some(said) => held(
                self.name(),
                format!("a run is open and says what it was working on: {said:?}"),
            ),
            None => missed(
                self.name(),
                "no message was left and no open run says what it was doing, so the next session \
                 arrives at what the occupant found and not at what it did",
            ),
        }
    }
}

/// Every message the board carries, mail asked for.
async fn board(seen: &Observed<'_>) -> String {
    seen.room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await
}

/// The message hits in a board reading. **Counted out of the answer**: a
/// `search` answers with its envelope whether or not anything matched, so the
/// text of the reading says nothing about whether a message is in it.
fn messages(board: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(board)
        .ok()
        .and_then(|body| {
            body["results"].as_array().map(|hits| {
                hits.iter()
                    .filter(|hit| hit["hit"] == "message")
                    .cloned()
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// **Every value a read answers with that a question could reach** — what each
/// key on each record holds, and the day each record carries.
///
/// Walked rather than indexed: the answer nests, and a check reaching for one
/// path would go quiet the day the shape gained a level, which reads as a pass.
fn collect_values(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            for (key, held) in members {
                match (key.as_str(), held) {
                    ("fields", Value::Object(keys)) => {
                        into.extend(keys.values().filter_map(|v| v.as_str().map(str::to_string)))
                    }
                    ("date", Value::String(day)) => into.push(day.clone()),
                    _ => {}
                }
                collect_values(held, into);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_values(item, into);
            }
        }
        _ => {}
    }
}

/// The digits in a value, so a number written `4,100` and one written `4100 km`
/// are the same number. A session's punctuation is not what this room measures.
fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}
