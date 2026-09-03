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
//! * **`the_service_landed_on_the_loop_that_already_existed`** — correlate two
//!   values inside ONE object. An assertion is a substring of the whole
//!   answer, so one loop carrying two check-in days and two loops carrying one
//!   each read identically, and the second of those is the failure being
//!   watched. Pinning the loop's handle is not the way out either: the
//!   occupant invents it.
//! * **`the_years_turns_are_on_file_as_derivations`** — correlate two KEYS
//!   inside ONE record. `provenance` cannot stand in for this: inference is
//!   the enum's own default, so a hand-set record and a check-in carry the
//!   identical token. What the built path leaves that nothing else does is
//!   `outcome` beside `last_check_in` on the same record, and an assertion
//!   cannot ask whether two keys landed together on one object out of a
//!   folded answer.
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
pub const CHECKS: [Hatch; 13] = [
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
    (
        "the_service_landed_on_the_loop_that_already_existed",
        || checked(|seen| Box::pin(the_service_landed_on_the_loop_that_already_existed(seen))),
    ),
    ("the_years_turns_are_on_file_as_derivations", || {
        checked(|seen| Box::pin(the_years_turns_are_on_file_as_derivations(seen)))
    }),
    ("the_club_was_corrected_in_place_in_july", || {
        checked(|seen| Box::pin(the_club_was_corrected_in_place_in_july(seen)))
    }),
    ("august_put_nobody_new_at_the_survey", || {
        checked(|seen| Box::pin(august_put_nobody_new_at_the_survey(seen)))
    }),
    ("late_october_put_nobody_new_at_the_survey", || {
        checked(|seen| Box::pin(late_october_put_nobody_new_at_the_survey(seen)))
    }),
    ("the_pump_reached_its_holder_in_february", || {
        checked(|seen| Box::pin(the_pump_reached_its_holder_in_february(seen)))
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
                    ("recorded_at", Value::String(day)) => into.push(day.clone()),
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

/// The sitting that records who has the thing, and the link a record draws when
/// it says so.
const FEBRUARY: &str = "Phase 2";
const HOLDS_IT: &str = "\"object\":\"person:ralph\"";

/// 🚨 **Who had the pump was recorded by FEBRUARY, asked in February's own
/// window.**
///
/// ⛔️ **Asked of the finished room, this claim belongs to nobody.** September
/// records that the pump came back and draws a second link at the same person,
/// on the same subject — so *the pump reaches its holder*, graded at the end of
/// the year, is satisfied by September's record. **A February that recorded
/// nothing about who had the thing passes on work done seven months later.**
///
/// **The needle was ambiguous rather than wrong**, which is why reading the
/// phases missed it twice: it matches, and it matches something else too.
///
/// **So the link is counted across February alone.** September is outside the
/// window and cannot reach it.
async fn the_pump_reached_its_holder_in_february(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(FEBRUARY) else {
        return Err(format!(
            "this run took no reading either side of {FEBRUARY}, so nothing here can say what \
             that sitting recorded. A check scoped to one sitting needs the run's own \
             boundaries.",
        ));
    };
    let had = before.world.matches(HOLDS_IT).count();
    let has = after.world.matches(HOLDS_IT).count();
    match has > had {
        true => Ok(()),
        false => Err(format!(
            "nothing came to point at the person holding the pump in {FEBRUARY}'s window, so who \
             had it is only in the prose of a sitting that is gone",
        )),
    }
}

/// The sitting that corrects the March claim, and the day it corrects it under.
const JULY: &str = "Phase 7";
const JULYS_DAY: &str = "\"recorded_at\":\"2026-07-05\"";

/// **The key the record that took a claim back carries**, naming what it
/// retracted.
///
/// 🚨 **Not the retracted claim's own status, and that is not a style
/// choice.** A boundary reads the index, and a claim that was taken back is
/// not in it — a search over everything returns the RETRACTION and not the
/// thing retracted. So `status: retracted` never appears in a boundary at all,
/// and a check watching for it can never fail. **The only trace a retraction
/// leaves where a window can see it is the record that made it.**
///
/// Counted rather than looked for: what this asks is whether a retraction
/// APPEARED in one window, and a room where one already stands is a room where
/// its presence says nothing.
///
/// ⚠️ **The literal, not the domain's constant.** This is a stored spelling
/// and nothing outside the process declares it, so asserting through the
/// constant would move both sides together and hold on a build that changed
/// what is written.
const TAKEN_BACK: &str = "\"retracts\":";

/// 🚨 **The correction was written IN, not taken back — asked in July's own
/// window.**
///
/// A claim that was true and then changed is corrected in place. A claim that
/// was never true is retracted. **March's claim is the first kind**, so July
/// rewriting it under July's day is right and July retracting it is wrong, and
/// the difference is the whole of what this sitting is for.
///
/// ⛔️ **Asked of the finished room, the negative half belongs to nobody.** The
/// club gains records after July — August writes on it — and any later sitting
/// that took a claim back would put a retraction on that subject with July's
/// name on the failure. **A negative over a whole subject, graded at the end of
/// the year, accuses whichever sitting the sentence happens to name.**
///
/// **So both halves are asked across July alone**: the club gained July's day
/// in that window, and no retraction appeared in it. Late October retracts, and
/// it is four sittings away.
///
/// ⚠️ **The positive is not decoration.** Without it a July that did nothing at
/// all satisfies *no retraction appeared* perfectly, which is the failure this
/// project repeats more than any other.
async fn the_club_was_corrected_in_place_in_july(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(JULY) else {
        return Err(format!(
            "this run took no reading either side of {JULY}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let dated = after.world.matches(JULYS_DAY).count() > before.world.matches(JULYS_DAY).count();
    let took_back =
        after.world.matches(TAKEN_BACK).count() > before.world.matches(TAKEN_BACK).count();
    match (dated, took_back) {
        (true, false) => Ok(()),
        (false, _) => Err(format!(
            "nothing gained {JULYS_DAY} in {JULY}'s window, so the March claim was not corrected \
             on the day the operator corrected it",
        )),
        (true, true) => Err(format!(
            "a claim was taken back in {JULY}'s window, so the correction was retracted rather \
             than written in — and March's claim was true in its day",
        )),
    }
}

/// The sitting that is asked who was there, and the link a record draws when it
/// says somebody was.
const AUGUST: &str = "Phase 8";
const WAS_THERE: &str = "\"type\":\"attendee\"";

/// 🚨 **August answered out of the record and added nobody to it — asked in
/// August's own window.**
///
/// The sitting is asked which club members were at the survey. **The answer is
/// two people and the store must still say two**: a sitting that cannot find
/// them and fills the gap writes a third, and an invented attendee is the
/// failure this room exists to catch.
///
/// ⛔️ **Naming the invented person cannot work.** Asked of the finished room,
/// the negative has to name somebody, and anybody it names either does not
/// exist yet in August — in which case nothing August does could ever trip it —
/// or arrives later, in which case a LATER sitting's mistake is reported under
/// August's name. **The first is a check that cannot fail. The second is a
/// check that blames the wrong sitting.**
///
/// **So it counts the links instead.** Somebody was at the survey before August
/// ran, and August added nobody. That is falsifiable by August and by nothing
/// else: a later sitting putting a person on the CLUB draws a different link,
/// and a later sitting taking an attendance back is four sittings away.
///
/// ⚠️ **The positive is the half that stops this holding on an empty room.** A
/// year where June recorded nothing has no links to add to, and *August added
/// none* would hold there perfectly.
async fn august_put_nobody_new_at_the_survey(seen: &Observed<'_>) -> Result<(), String> {
    nobody_new_was_put_at_the_survey(seen, AUGUST).await
}

/// The sitting that takes an attendance back, and the one that first has
/// somebody to invent.
const LATE_OCTOBER: &str = "Phase 11";

/// 🚨 **The late sitting put nobody new at the survey either.**
///
/// **The fault this catches belongs to a sitting late in the year, and until
/// now nothing caught it under its own name.** August's lock used to, by naming
/// the person who arrives here — which reported this sitting's mistake as
/// August's, four months earlier. **Attribution follows the ACT rather than the
/// consequence**, so the sitting that can commit it carries the lock.
///
/// **This is the first sitting that CAN.** It is handed a new person and told
/// to take an attendance back, so it holds both halves of the mistake: somebody
/// to file, and a reason to be writing about the survey at all.
///
/// ⚠️ **It is the same question as August's and asked the same way**, in this
/// sitting's own window. Taking an attendance back is welcome here and lowers
/// the count; adding one is not.
async fn late_october_put_nobody_new_at_the_survey(seen: &Observed<'_>) -> Result<(), String> {
    nobody_new_was_put_at_the_survey(seen, LATE_OCTOBER).await
}

/// **Whether one sitting added an attendance link**, asked in that sitting's
/// own window.
///
/// One body and two names because it is one question asked of two sittings, and
/// the names are what the room's documents call. A second copy would be a
/// second thing to keep in step.
async fn nobody_new_was_put_at_the_survey(seen: &Observed<'_>, phase: &str) -> Result<(), String> {
    let Some((before, after)) = seen.across(phase) else {
        return Err(format!(
            "this run took no reading either side of {phase}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let had = before.world.matches(WAS_THERE).count();
    let has = after.world.matches(WAS_THERE).count();
    if had == 0 {
        return Err(format!(
            "nobody was at the survey before {phase} ran, so this sitting had nothing to answer \
             out of and *it invented nobody* holds over an empty record",
        ));
    }
    match has > had {
        false => Ok(()),
        true => Err(format!(
            "the survey carried {had} attendance links before {phase} and {has} after, so that \
             sitting put somebody at an event instead of answering out of what was already there",
        )),
    }
}

/// The key a loop records its turns under, and the two days that matter: the
/// one January opened the loop with, and the one the late sitting is asked to
/// record.
const TURNS: &str = "last_check_in";
const OPENED: &str = "2025-12-20";
const SERVICED: &str = "2026-11-22";

/// **The late turn landed on the loop that already existed.**
///
/// The operator asks about the drivetrain and the loop is a chain check, so the
/// sitting has to reach the record through something other than the words it
/// was given. **What is watched is where the turn ended up**, and the failure
/// is a second loop rather than silence: a sitting that searched, found
/// nothing, and opened a new loop leaves a store holding two, each with half
/// the history, and neither able to say when the chain was last done.
///
/// **The loop is identified by the day January opened it with rather than by a
/// handle**, because the handle is a word the occupant invents.
///
/// A key's history is read rather than what it holds now, for the reason
/// January's lock gives: the newest write wins the fold, so the fold cannot say
/// which loop has been running all year.
async fn the_service_landed_on_the_loop_that_already_existed(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let read = seen
        .room
        .call("recall", json!({"kind": "rhythm", "history": TURNS}))
        .await;
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    let Some(loops) = parsed["objects"].as_array() else {
        return Err(format!(
            "no loop came back at all, so nothing was measured: {read}"
        ));
    };
    let turns = |one: &Value| -> Vec<String> {
        one["history"]["writes"]
            .as_array()
            .map(|writes| {
                writes
                    .iter()
                    .filter_map(|write| write["value"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    // **The positive the whole check rests on.** Without it a store where
    // January never ran reports the late sitting's failure, and the sentence
    // beside this lock would blame the wrong month.
    let Some(january) = loops
        .iter()
        .find(|one| turns(one).iter().any(|day| day == OPENED))
    else {
        return Err(format!(
            "no loop carries {OPENED}, so the loop this sitting was asked about was never \
             opened and there is nothing here for it to have found: {read}"
        ));
    };
    if turns(january).iter().any(|day| day == SERVICED) {
        return Ok(());
    }
    // **The two ways of not holding are worth telling apart**: a turn on some
    // other loop is the second-loop failure, and no turn anywhere is a sitting
    // that recorded nothing.
    match loops
        .iter()
        .any(|one| turns(one).iter().any(|day| day == SERVICED))
    {
        true => Err(format!(
            "{SERVICED} is recorded under {TURNS} on a loop that is not the one carrying \
             {OPENED}, so this sitting stood a second loop beside the first"
        )),
        false => Err(format!(
            "no loop records a turn on {SERVICED}, so this sitting wrote nothing under {TURNS}"
        )),
    }
}

/// The other key a check-in writes in the same act as `last_check_in` —
/// always, whatever the outcome. `counts_from` is the third computed key
/// (`attention::COUNTS_FROM`) and is left out on purpose: a snooze does not
/// consume the cycle, so it writes no `counts_from` at all, and requiring it
/// here would fail a genuine check-in that snoozed.
const OUTCOME: &str = "outcome";

/// **EVERY turn on the loop is on file as a derivation, and there is no
/// threshold.**
///
/// A count cannot support a sentence with a universal in it. This lock's own
/// words are about *the year's turns*, so it asks the question its sentence
/// asks: of the turns recorded on this loop, how many were checked in — and
/// the answer has to be all of them.
///
/// **Scoped to ONE loop**, because turns counted across every rhythm in the
/// store let two loops carrying one qualifying turn each stand in for one loop
/// carrying two. **The loop is identified by the day January opened it**
/// rather than by a handle, for the reason the sitting beside this one gives:
/// the handle is a word the occupant invents.
///
/// `provenance` cannot be the needle. Inference is the enum's own default
/// (`#[default]` on `Provenance`, `crates/jojobot-domain/src/memory.rs`), so a
/// capture that names no provenance gets the identical token a check-in
/// writes — counting `"provenance":"inference"` cannot tell a caller who said
/// nothing from a caller who ran the arithmetic.
///
/// **What a check-in writes that nothing else does is two keys landing on ONE
/// record in the same act**: `outcome` beside `last_check_in`
/// (`crates/jojobot-mcp/src/memory/capture.rs`, `attention::check_in`). A
/// caller hand-setting the day writes `last_check_in` alone. Correlating two
/// keys inside one record is past what an assertion can say, so this reads
/// each rhythm's own records (`facts: true`) rather than its folded fields —
/// folding would hide which record wrote which key.
///
/// ⚠️ **Not proof against a caller who types both keys by hand.** Nothing on
/// a record says which verb wrote it; a capture naming `outcome` and
/// `last_check_in` as ordinary fields, never calling `check_in`, reads
/// identically to the built path. What this catches is the shape a hand-set
/// turn actually takes in this room — the day alone — against the shape the
/// built path always takes; a caller motivated to fake the pair is a gap this
/// hatch does not close.
async fn the_years_turns_are_on_file_as_derivations(seen: &Observed<'_>) -> Result<(), String> {
    let read = seen
        .room
        .call("recall", json!({"kind": "rhythm", "facts": true}))
        .await;
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    let Some(loops) = parsed["objects"].as_array() else {
        return Err(format!(
            "no loop came back at all, so nothing was measured: {read}"
        ));
    };
    // A record is a TURN when it says which day the loop was last done. That
    // is the claim this lock is about, whoever wrote it and however.
    let turn_on = |fact: &Value| -> Option<String> {
        fact["fields"]
            .get(TURNS)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let checked_in = |fact: &Value| {
        fact["fields"]
            .get(OUTCOME)
            .and_then(Value::as_str)
            .is_some()
    };

    // **The positive the whole lock rests on.** Without it a store where
    // January never ran reports every turn as a derivation, vacuously, because
    // there are no turns to fail.
    let Some(january) = loops.iter().find(|one| {
        one["facts"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|fact| turn_on(fact).as_deref() == Some(OPENED))
    }) else {
        return Err(format!(
            "no loop carries a turn on {OPENED}, so the loop this year is about was never opened \
             and it has no turns to be derivations: {read}"
        ));
    };

    let turns: Vec<(String, bool)> = january["facts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|fact| turn_on(fact).map(|day| (day, checked_in(fact))))
        .collect();
    let by_hand: Vec<&str> = turns
        .iter()
        .filter(|(_, checked_in)| !checked_in)
        .map(|(day, _)| day.as_str())
        .collect();
    if by_hand.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the loop opened on {OPENED} records {} turn(s), and {} of them carry {TURNS} with no \
         {OUTCOME} beside it on the same record — the turn(s) dated {} were written by hand \
         rather than checked in, where a check-in writes both keys together every time: {read}",
        turns.len(),
        by_hand.len(),
        by_hand.join(", "),
    ))
}
