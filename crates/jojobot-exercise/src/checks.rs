//! **The escape hatches, and every one of them is a finding.**
//!
//! A room's document names a check here when the claim it makes cannot be
//! written as a query and an assertion. **That is not a convenience**: each
//! entry names a specific thing jojobot's query surface cannot say, the run
//! counts and prints the ones a room took, and a room that cannot reach zero
//! is telling you something about the surface rather than about the harness.
//!
//! **The check answers the verdict and never the sentence.** The prose a reader
//! sees is written beside the lock, in the room, by somebody who knows what the
//! room is about.
//!
//! # What each entry here says the surface cannot do
//!
//! * **`the_brief_left_the_box`** — say that ONE named message is no longer
//!   `new`. An assertion is a substring of the whole answer, so it cannot
//!   correlate a hit's subject with that hit's state: `lacks "state":"new"`
//!   is a claim about every message on the board, and it fails the moment the
//!   occupant posts one of its own.
//! * **`the_service_day_is_a_value`** — ask whether a string is held as a
//!   VALUE under some key the caller does not know the name of, as opposed to
//!   sitting in prose. `fields` selects by a named key and `values` reports the
//!   values of a named key; neither asks *is this anywhere as a value*.
//! * **`a_handoff_is_waiting`** — say **either** of two claims. The assertion
//!   vocabulary is three words that do not branch, on purpose, and an `or`
//!   would be the first branch in it.

use serde_json::{Value, json};

use crate::run::{Checks, Observed, checked};

/// One hatch: the name a document calls it, and what it does.
type Hatch = (&'static str, fn() -> Box<dyn Checks>);

/// **Every named check this build ships.** A room adds one line here and one
/// `check` line in its document, and both are visible in the count.
pub const CHECKS: [Hatch; 3] = [
    ("the_brief_left_the_box", || {
        checked(|seen| Box::pin(the_brief_left_the_box(seen)))
    }),
    ("the_service_day_is_a_value", || {
        checked(|seen| Box::pin(the_service_day_is_a_value(seen)))
    }),
    ("a_handoff_is_waiting", || {
        checked(|seen| Box::pin(a_handoff_is_waiting(seen)))
    }),
];

/// **The message the room was furnished with is no longer waiting.**
///
/// **The oldest message on the board is the brief**, because the furniture is
/// posted before anybody arrives — so this needs no room's constants and reads
/// the same in every room. Delivery is the claim and not the verb that took
/// it: draining the box, taking the one message, posting from inside it and
/// retiring it straight from `new` all count.
///
/// The positive it rests on is that there is a message at all. An unfurnished
/// room has nothing to move, and must not read as a session that moved it.
async fn the_brief_left_the_box(seen: &Observed<'_>) -> Result<(), String> {
    let board = seen
        .room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let mut mail = messages(&board);
    if mail.is_empty() {
        return Err(format!(
            "there is no mail on the board at all, so the room was never furnished: {board}"
        ));
    }
    mail.sort_by_key(|hit| hit["sent_at"].as_str().unwrap_or("").to_string());
    let brief = &mail[0];
    match brief["state"] == "new" {
        true => Err(format!(
            "{} is still waiting, so nothing here says the occupant found its box",
            brief["subject"].as_str().unwrap_or("the brief"),
        )),
        false => Ok(()),
    }
}

/// The bike the brief's service belongs to, and the day it gives.
const RIDDEN: &str = "thing:gravel-bike";
const SERVICE_DAY: &str = "2026-08-11";

/// What the bike room's brief is called.
const BIKE_BRIEF: &str = "the bikes, and a few things I want off them";

/// **The service day is readable off the bike as a value**, under a key on a
/// record or as the day the record is dated — rather than written into a
/// sentence.
async fn the_service_day_is_a_value(seen: &Observed<'_>) -> Result<(), String> {
    let read = seen
        .room
        .call("recall", json!({"subject": RIDDEN, "facts": true}))
        .await;
    if read.contains("\"status\":\"blocked\"") {
        return Err(format!(
            "the bike is not readable, so nothing was measured: {read}"
        ));
    }
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    let mut values = Vec::new();
    collect_values(&parsed, &mut values);
    if values.iter().any(|value| value == SERVICE_DAY) {
        return Ok(());
    }
    // **The two ways of not holding are worth telling apart**: a day written
    // into a sentence is a session that recorded the work and lost the
    // question, and no day at all is a session that never got here.
    match read.contains(SERVICE_DAY) {
        true => Err(format!(
            "{SERVICE_DAY} is on the bike as prose only — no key holds it and no record is dated \
             with it"
        )),
        false => Err(format!("nothing on the bike mentions {SERVICE_DAY} at all")),
    }
}

/// **Either rail counts.** A message the occupant left, or a run left open
/// saying what it was working on.
async fn a_handoff_is_waiting(seen: &Observed<'_>) -> Result<(), String> {
    let board = seen
        .room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let mail = messages(&board);
    // The positive it rests on: a board with no mail at all is an unfurnished
    // room rather than a session that left nothing.
    if !mail.iter().any(|hit| hit["subject"] == BIKE_BRIEF) {
        return Err(format!(
            "the brief is not on the board, so the room was never furnished: {board}"
        ));
    }
    if mail.iter().any(|hit| hit["subject"] != BIKE_BRIEF) {
        return Ok(());
    }
    // The other rail. The door is the only place a run's record is served, and
    // being offered one is exactly how the next session would meet it.
    let door = seen
        .room
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let parsed: Value = serde_json::from_str(&door).unwrap_or(Value::Null);
    let open = parsed["session"]["choices"].as_array().is_some_and(|runs| {
        runs.iter()
            .filter_map(|run| run["working_on"].as_str())
            .any(|said| !said.trim().is_empty())
    });
    match open {
        true => Ok(()),
        false => Err("no message was left and no open run says what it was doing".to_string()),
    }
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
