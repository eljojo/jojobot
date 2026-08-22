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
//! * **`a_colleague_exists_with_its_box`**, **`the_pile_is_in_the_colleagues_box`**
//!   and **`what_became_of_the_pile_is_on_the_record`** — **name a thing the
//!   OCCUPANT named.** A lock's query is text written before the run, and these
//!   three are about *whichever bot is not the one that ships*. Nothing in the
//!   query surface computes that, and pinning a handle would assert the
//!   occupant guessed the same word the room did.

use serde_json::{Value, json};

use crate::run::{Checks, Observed, checked};

/// One hatch: the name a document calls it, and what it does.
type Hatch = (&'static str, fn() -> Box<dyn Checks>);

/// **Every named check this build ships.** A room adds one line here and one
/// `check` line in its document, and both are visible in the count.
pub const CHECKS: [Hatch; 7] = [
    ("the_brief_left_the_box", || {
        checked(|seen| Box::pin(the_brief_left_the_box(seen)))
    }),
    ("the_service_day_is_a_value", || {
        checked(|seen| Box::pin(the_service_day_is_a_value(seen)))
    }),
    ("a_handoff_is_waiting", || {
        checked(|seen| Box::pin(a_handoff_is_waiting(seen)))
    }),
    ("a_colleague_exists_with_its_box", || {
        checked(|seen| Box::pin(a_colleague_exists_with_its_box(seen)))
    }),
    ("the_pile_is_in_the_colleagues_box", || {
        checked(|seen| Box::pin(the_pile_is_in_the_colleagues_box(seen)))
    }),
    ("what_became_of_the_pile_is_on_the_record", || {
        checked(|seen| Box::pin(what_became_of_the_pile_is_on_the_record(seen)))
    }),
    ("the_club_was_given_a_claim_in_march", || {
        checked(|seen| Box::pin(the_club_was_given_a_claim_in_march(seen)))
    }),
];

/// The identity a fresh instance ships with, and the one every occupant wears.
const OCCUPANT: &str = "bot:assistant";

/// How many things the handover brief hands over. The number is the whole of
/// that room's terminal question: a cold session cannot guess it, and nothing
/// but the mail rail can tell it.
const PILE: usize = 3;

/// **A second identity stands, with the box that came with it.**
///
/// A box is not made: it opens with the bot that owns it, in the one act. The
/// occupant is never told that, which is what this is watching.
async fn a_colleague_exists_with_its_box(seen: &Observed<'_>) -> Result<(), String> {
    let board = seen.room.call("start_here", json!({"brief": true})).await;
    let bots = bots_on(&board);
    // ⚠️ **Nothing exercises this, and nothing can.** The shipped identity is
    // seeded before the server answers anything and no verb removes a bot, so
    // through the served surface this board always names it. What it guards is
    // a reading that came back unparseable — damage, or a build that changed
    // the door's shape — and reporting *no colleague* for either would blame
    // the occupant for the room.
    if !bots.iter().any(|bot| bot["handle"] == OCCUPANT) {
        return Err(format!(
            "the shipped identity is not on the board, so nothing was read: {board}"
        ));
    }
    let colleagues: Vec<&Value> = bots
        .iter()
        .filter(|bot| bot["handle"] != OCCUPANT)
        .collect();
    match colleagues.as_slice() {
        [] => Err("no second identity was made, so there is nobody to hand anything to".into()),
        // ⚠️ **Nothing exercises this either, and nothing can.** A box opens
        // with the bot that owns it, in the same act, and no verb opens or
        // removes one — so a bot with no box is damage a person caused
        // underneath the surface, which is the state the mcp crate writes
        // straight to Memory to test its own repair. It stays because a check
        // that read past it would report *no colleague* for a colleague that
        // is there.
        [colleague] if colleague["mail"].is_null() => Err(format!(
            "{} exists and owns no box, so it cannot be written to",
            colleague["handle"],
        )),
        [_] => Ok(()),
        many => Err(format!(
            "{} identities were made where the brief asked for one",
            many.len()
        )),
    }
}

/// **The pile is in the colleague's box, one thing at a time.**
///
/// The brief asks for them separately so they can be worked separately, which
/// is the mail rail doing what it is for. A run that handed the whole pile over
/// as one message has left a colleague with one thing to finish rather than
/// three, and the terminal question has nothing to count.
async fn the_pile_is_in_the_colleagues_box(seen: &Observed<'_>) -> Result<(), String> {
    let colleague = colleague_of(seen)
        .await
        .ok_or("there is no colleague, so there is no box to have handed anything to")?;
    let owned = colleague.trim_start_matches("bot:").to_string();
    let handed: Vec<Value> = mail(seen)
        .await
        .into_iter()
        .filter(|hit| hit["mailbox"] == owned.as_str())
        .collect();
    if handed.len() != PILE {
        return Err(format!(
            "{} thing(s) are in the colleague's box where the brief handed over {PILE}",
            handed.len(),
        ));
    }
    // The positive that makes the count mean anything: they came from the
    // occupant rather than from nowhere.
    match handed.iter().all(|hit| hit["sender"] == OCCUPANT) {
        true => Ok(()),
        false => Err(format!(
            "something in the colleague's box was not sent by {OCCUPANT}"
        )),
    }
}

/// **What became of the pile is written where a later session will find it.**
///
/// A session with no memory of the first cannot know how many things were
/// handed over, and nothing it can read says so: the colleague's box is not its
/// to open, and the state of somebody else's mail is in no sentence anywhere.
///
/// Both halves: the count is written onto the colleague, and the pile really is
/// untouched — so a run that wrote a number without looking is not credited
/// with an answer that happens to be right.
async fn what_became_of_the_pile_is_on_the_record(seen: &Observed<'_>) -> Result<(), String> {
    let colleague = colleague_of(seen)
        .await
        .ok_or("there is no colleague, so there is nothing to have found out about")?;
    let owned = colleague.trim_start_matches("bot:").to_string();
    let waiting = mail(seen)
        .await
        .into_iter()
        .filter(|hit| hit["mailbox"] == owned.as_str() && hit["state"] == "new")
        .count();
    if waiting != PILE {
        return Err(format!(
            "{waiting} of the pile is waiting where {PILE} was handed over, so the answer this \
             lock reads for is not {PILE} at all"
        ));
    }
    let read = seen
        .room
        .call("recall", json!({"subject": colleague, "facts": true}))
        .await;
    match said_the_count(&read, PILE) {
        true => Ok(()),
        false => Err(format!(
            "nothing on {colleague} says how much of the pile is still waiting, so a later \
             session has to go and find out again"
        )),
    }
}

/// Whether a read says the number, as a figure or as the word. **Both, because
/// which one somebody writes is not what that room measures.**
///
/// ⚠️ **Asked of what the SESSION wrote, never of the whole answer.** A read
/// carries the day a claim is about, the moment the store took it in and the
/// address that edits it, and those carry digits nobody chose: a nanosecond
/// stamp holds almost any figure. Over the whole payload this opens on a room
/// where the session wrote something and answered nothing.
fn said_the_count(read: &str, count: usize) -> bool {
    const WORDS: [&str; 4] = ["zero", "one", "two", "three"];
    let figure = count.to_string();
    let word = WORDS.get(count);
    authored(read).iter().any(|said| {
        let said = said.to_lowercase();
        said.contains(&figure) || word.is_some_and(|word| said.contains(word))
    })
}

/// **Everything on this read that a session chose the words of**: what each
/// thing holds, and each claim's own sentence, note and keys.
///
/// A payload this cannot parse says nothing, so the check reads nothing and
/// misses. That is the safe direction: a check that cannot read the answer must
/// not report one.
fn authored(read: &str) -> Vec<String> {
    let Ok(body) = serde_json::from_str::<Value>(read) else {
        return Vec::new();
    };
    let mut said = Vec::new();
    for object in body["objects"].as_array().into_iter().flatten() {
        written_values(&object["fields"], &mut said);
        for fact in object["facts"].as_array().into_iter().flatten() {
            for wording in ["content", "details"] {
                if let Some(text) = fact[wording].as_str() {
                    said.push(text.to_string());
                }
            }
            written_values(&fact["fields"], &mut said);
        }
    }
    said
}

/// The values of a key/value bag, which are the caller's own words. The KEYS
/// are the caller's too, and they are left out: a key called `pile_of_3` is a
/// name for the question rather than an answer to it.
fn written_values(fields: &Value, said: &mut Vec<String>) {
    for value in fields.as_object().into_iter().flatten().map(|(_, v)| v) {
        if let Some(text) = value.as_str() {
            said.push(text.to_string());
        }
    }
}

/// The colleague's handle — whichever bot is not the one that ships.
async fn colleague_of(seen: &Observed<'_>) -> Option<String> {
    let board = seen.room.call("start_here", json!({"brief": true})).await;
    bots_on(&board)
        .iter()
        .filter(|bot| bot["handle"] != OCCUPANT)
        .filter_map(|bot| bot["handle"].as_str().map(str::to_string))
        .next()
}

/// The bots a boarding snapshot names, each with its mail beside it.
fn bots_on(board: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(board)
        .ok()
        .and_then(|body| body["snapshot"]["entities"]["bots"].as_array().cloned())
        .unwrap_or_default()
}

/// Every message on the board, mail asked for.
async fn mail(seen: &Observed<'_>) -> Vec<Value> {
    let board = seen
        .room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    messages(&board)
}

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

/// The sitting whose window this reads, and the subject it counts records on.
const MARCH: &str = "Phase 3";
const CLUB: &str = "\"subject\":\"org:north-trail-club\"";

/// 🚨 **What ONE SITTING recorded, asked in that sitting's own window.**
///
/// **Every other check in every room runs once, against the finished room**, so
/// the only question a room can ask is whether something is still there at the
/// end. That is a weaker question, and March is where the difference bites: a
/// later sitting rewrites March's claim IN PLACE, under its own day, and
/// editing a claim destroys what it said before — the earlier text is on no
/// read, because a claim's content is a column on the fact row rather than one
/// of the field writes the store keeps. **So by the time the checks run there
/// is nothing dated March and nothing saying what March said.**
///
/// ⛔️ **The needle cannot be a word, either.** March's claim survives today
/// only because the sentence that replaced it happens to keep one — which is a
/// check standing on an accident.
///
/// **So this reads the world either side of March and asks whether the club
/// gained a record in that window.** Only March writes there. It is structural,
/// it is phrasing-free, and no later sitting can take it away.
///
/// ⚠️ **A run that took no readings must fail here and say why.** Every suite
/// passed an empty boundary list until this shipped, so a check that held
/// against no boundaries would pass everywhere it had not been wired up — which
/// is the worst answer available, because it looks like coverage.
async fn the_club_was_given_a_claim_in_march(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(MARCH) else {
        return Err(format!(
            "this run took no reading either side of {MARCH}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let had = before.world.matches(CLUB).count();
    let has = after.world.matches(CLUB).count();
    match has > had {
        true => Ok(()),
        false => Err(format!(
            "the club carried {had} records before {MARCH} and {has} after, so that sitting \
             recorded nothing about it and July has nothing to take back",
        )),
    }
}
