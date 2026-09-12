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
//! * **`septembers_account_of_the_pump_is_corrected_in_place`** — say that ONE
//!   record now reads what October said, with what it said before still
//!   reachable through that same record's own history. A retraction is
//!   marked rather than filtered — `recall` serves a retracted record's own
//!   text back, by design — so `carries person:ralph` cannot see the
//!   `status` key beside an edge it found, and a retraction reads
//!   identically to an active claim to a substring. And no query on this
//!   surface correlates a record's CURRENT content against its own PAST
//!   content: `facts` answers what it says now, `history_record` answers
//!   what it said before, and only a caller that reads both can tell a
//!   correction from a retraction, an untouched claim, or a second claim
//!   filed beside the first.
//! * **`junes_survey_drew_a_standing_attendee_for_each`** — say that a
//!   record's own edge and its own status are the SAME record's, asked in
//!   June's own window. A retraction is marked rather than filtered, so
//!   `carries person:nelson` cannot see the `status` key beside the edge it
//!   found, and late October legitimately retracts Nelson's own attendance
//!   at this same survey — the pump lock's own historical bug, on
//!   attendance rather than the pump.
//! * **`februarys_club_drew_a_standing_member_for_each`** — the same
//!   correlation, on the club's membership edge rather than the survey's
//!   attendee one, asked in February's own window.
//! * **`aprils_move_drew_a_standing_location_edge`** — the same
//!   correlation, on Milhouse's move to Shelbyville, asked in April's own
//!   window. Also pins the OBJECT, not only the shape: Milhouse already
//!   carries a `location` edge to Springfield when April opens.
//! * **`octobers_note_drew_a_standing_location_edge_to_the_trail`** — the
//!   same correlation again, on where the survey was held, asked in
//!   October's own window. No subject is pinned: October renames the
//!   survey's own event in the same sitting that writes this claim.
//! * **`late_octobers_club_drew_a_standing_member_for_bart`** — the same
//!   correlation again, on Bart's membership, asked in late October's own
//!   window.
//! * **`one_record_points_at_two_kinds`** — say that handles of two different
//!   kinds landed on ONE record. `carries` lines are claims about the whole
//!   answer, so two of them hold on two records naming one thing each, which is
//!   the easy case rather than the one being watched. Naming the kinds is not a
//!   way out either: the sitting chooses which things it points at.
//! * **`junes_survey_mention_renders_under_the_current_handle`** — say that
//!   ONE mention's rendering CHANGED between two readings. `carries` is a
//!   claim about the answer as it stands, never about a difference from an
//!   earlier answer, and pinning either handle is not a way out: January and
//!   October each invent one.
//! * **`a_colleague_exists_with_its_box`**, **`the_pile_is_in_the_colleagues_box`**
//!   and **`what_became_of_the_pile_is_on_the_record`** — **name a thing the
//!   OCCUPANT named.** A lock's query is text written before the run, and these
//!   three are about *whichever bot is not the one that ships*. Nothing in the
//!   query surface computes that, and pinning a handle would assert the
//!   occupant guessed the same word the room did.
//! * **`a_late_sitting_folds_the_canoes_pile_without_being_told_to`** — say
//!   that a mark is NEW in one sitting's own window rather than merely
//!   present on the finished board. A plain "does a mark exist" question
//!   cannot tell a mark this sitting made from one that happened to exist
//!   before the room asked anything.
//! * **`the_canoes_five_repairs_are_still_active_and_named_by_the_fold`** —
//!   correlate a mark's `stands_for` list against the STATUS of each address
//!   it should cover, across five separate records. An assertion is a
//!   substring of the whole answer, so it cannot ask whether every one of
//!   five specific addresses is both active and named, only whether some
//!   address somewhere is.
//! * **`the_canoes_fold_invents_no_date_the_repairs_never_gave`** — compare
//!   one record's own date-bearing fields and prose against the union of
//!   every OTHER record's dates. Nothing on the query surface computes a set
//!   difference across records.

use serde_json::{Value, json};

use crate::run::{Checks, Observed, checked};

/// One hatch: the name a document calls it, and what it does.
type Hatch = (&'static str, fn() -> Box<dyn Checks>);

/// **Every named check this build ships.** A room adds one line here and one
/// `check` line in its document, and both are visible in the count.
pub const CHECKS: [Hatch; 26] = [
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
    ("a_records_trace_matches_the_writes_the_run_made", || {
        checked(|seen| Box::pin(a_records_trace_matches_the_writes_the_run_made(seen)))
    }),
    ("julys_claim_is_withdrawn_rather_than_rewritten", || {
        checked(|seen| Box::pin(julys_claim_is_withdrawn_rather_than_rewritten(seen)))
    }),
    (
        "septembers_account_of_the_pump_is_corrected_in_place",
        || checked(|seen| Box::pin(septembers_account_of_the_pump_is_corrected_in_place(seen))),
    ),
    ("august_put_nobody_new_at_the_survey", || {
        checked(|seen| Box::pin(august_put_nobody_new_at_the_survey(seen)))
    }),
    ("late_october_put_nobody_new_at_the_survey", || {
        checked(|seen| Box::pin(late_october_put_nobody_new_at_the_survey(seen)))
    }),
    ("the_pump_reached_its_holder_in_february", || {
        checked(|seen| Box::pin(the_pump_reached_its_holder_in_february(seen)))
    }),
    ("one_record_points_at_two_kinds", || {
        checked(|seen| Box::pin(one_record_points_at_two_kinds(seen)))
    }),
    (
        "junes_survey_mention_renders_under_the_current_handle",
        || checked(|seen| Box::pin(junes_survey_mention_renders_under_the_current_handle(seen))),
    ),
    ("junes_survey_drew_a_standing_attendee_for_each", || {
        checked(|seen| Box::pin(junes_survey_drew_a_standing_attendee_for_each(seen)))
    }),
    ("februarys_club_drew_a_standing_member_for_each", || {
        checked(|seen| Box::pin(februarys_club_drew_a_standing_member_for_each(seen)))
    }),
    ("aprils_move_drew_a_standing_location_edge", || {
        checked(|seen| Box::pin(aprils_move_drew_a_standing_location_edge(seen)))
    }),
    (
        "octobers_note_drew_a_standing_location_edge_to_the_trail",
        || {
            checked(|seen| {
                Box::pin(octobers_note_drew_a_standing_location_edge_to_the_trail(
                    seen,
                ))
            })
        },
    ),
    ("late_octobers_club_drew_a_standing_member_for_bart", || {
        checked(|seen| Box::pin(late_octobers_club_drew_a_standing_member_for_bart(seen)))
    }),
    (
        "a_late_sitting_folds_the_canoes_pile_without_being_told_to",
        || {
            checked(|seen| {
                Box::pin(a_late_sitting_folds_the_canoes_pile_without_being_told_to(
                    seen,
                ))
            })
        },
    ),
    (
        "the_canoes_five_repairs_are_still_active_and_named_by_the_fold",
        || {
            checked(|seen| {
                Box::pin(the_canoes_five_repairs_are_still_active_and_named_by_the_fold(seen))
            })
        },
    ),
    (
        "the_canoes_fold_invents_no_date_the_repairs_never_gave",
        || checked(|seen| Box::pin(the_canoes_fold_invents_no_date_the_repairs_never_gave(seen))),
    ),
    ("the_bike_locks_mistake_is_rewritten_in_place", || {
        checked(|seen| Box::pin(the_bike_locks_mistake_is_rewritten_in_place(seen)))
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

/// The two sittings that each write one account of the pump's return, named
/// by the phase whose window carries the write — not by content, which an
/// occupant may word any way it likes.
const SEPTEMBER: &str = "Phase 9";
const OCTOBER: &str = "Phase 10";

/// **Every address on the pump, either side of a phase's window.** Diffing
/// the two tells which address that window wrote, without needing to know
/// what the occupant said — the same reason the day assertion reads addresses
/// rather than prose.
fn floor_pump_addresses(world: &str) -> std::collections::HashSet<String> {
    let mut found = std::collections::HashSet::new();
    for line in world.lines() {
        let Ok(parsed) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(hits) = parsed["results"].as_array() else {
            continue;
        };
        for hit in hits {
            if let Some(address) = hit["address"].as_str()
                && address.starts_with("thing:floor-pump#")
            {
                found.insert(address.to_string());
            }
        }
    }
    found
}

/// 🚨 **October corrects September's account of the pump, IN PLACE.**
///
/// The operator ruled 2026-09-10: a later statement is a correction whether
/// or not it is worded as one. September's sentence about who returned the
/// pump is not sealed against October's — October's account is the same
/// claim, better informed, so the record after October must be September's
/// own address, rewritten to say what October said, with what September said
/// still reachable through that address's own history.
///
/// ⛔️ **The two ways of not correcting are worth telling apart, and both
/// fail here.** A sitting that captures a fresh claim beside September's
/// leaves September's own address still saying Ralph, unrevised — a
/// correction that never reached the record. A sitting that retracts
/// September's address rather than rewriting it picked a winner by deletion
/// instead of by correction.
///
/// ⛔️ **Naming the edge is not enough.** February also draws a `connection`
/// edge at Ralph — lending the pump, not returning it — and that edge
/// outlives everything this lock is about. So this is scoped by WHICH WINDOW
/// wrote the address, the same technique February's own neighbour lock uses
/// for the identical ambiguity.
///
/// ⛔️ **A hatch, and it needs two things a document assertion cannot give
/// together.** A retraction is marked rather than filtered, so `carries
/// person:ralph` cannot see the `status` key beside the edge it found, and a
/// retracted account reads identically to an active one to a substring.
/// Worse, nothing on this surface correlates a record's CURRENT content
/// against its own PAST content in one query: `facts` answers what the
/// record says now, `history_record` answers what it said before, and only a
/// caller that reads both can tell a correction from a retraction, an
/// untouched claim, or a second claim filed beside the first.
///
/// 🚨 **September's own wording survives EITHER as an edge or as a mention in
/// content, and this reads both.** A paid run named Ralph by drawing a
/// `connection` edge at him; a later one drew an `attendance` edge at the
/// event instead and named Ralph inside the sentence — both are reasonable
/// accounts of who returned the pump, and a check that recognised only the
/// first read the second's history as destroyed rather than superseded. The
/// claim being tested is that the WORDING survives, not that it survives in
/// one structural place, so this is not a widening: an account that named
/// nobody at all, by either route, still fails.
async fn septembers_account_of_the_pump_is_corrected_in_place(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let Some((before_sep, after_sep)) = seen.across(SEPTEMBER) else {
        return Err(format!(
            "this run took no reading either side of {SEPTEMBER}, so nothing here can say which \
             address is Ralph's account of the pump's return",
        ));
    };
    if seen.across(OCTOBER).is_none() {
        return Err(format!(
            "this run took no reading either side of {OCTOBER}, so nothing here can say whether \
             that sitting corrected Ralph's account",
        ));
    }
    let ralphs: Vec<String> = floor_pump_addresses(&after_sep.world)
        .difference(&floor_pump_addresses(&before_sep.world))
        .cloned()
        .collect();
    let [address] = ralphs.as_slice() else {
        return Err(format!(
            "{SEPTEMBER}'s window wrote {} address(es) on the pump rather than one, so nothing \
             here can say which is Ralph's account to watch get corrected: {ralphs:?}",
            ralphs.len(),
        ));
    };
    let read = seen
        .room
        .call(
            "recall",
            json!({"subject": "thing:floor-pump", "facts": true, "history_record": address}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    let Some(current) = parsed["objects"][0]["facts"].as_array().and_then(|facts| {
        facts
            .iter()
            .find(|fact| fact["address"] == address.as_str())
    }) else {
        return Err(format!(
            "{address} is no longer on the pump's record at all, so nothing here can say it was \
             corrected rather than something else: {read}"
        ));
    };
    if current["status"] != "active" {
        return Err(format!(
            "{address} is not active — September's account was retracted rather than corrected \
             in place, a sitting picked a winner by deletion: {read}"
        ));
    }
    if current["edge"]["object"] != "person:nelson" {
        return Err(format!(
            "{address} still carries Ralph's account, so October's statement about the pump \
             either went nowhere or was filed as a second claim instead of a correction: {read}"
        ));
    }
    let held_ralph = parsed["objects"][0]["record_history"]["writes"]
        .as_array()
        .is_some_and(|writes| {
            writes.iter().any(|write| {
                write["edge"]["object"] == "person:ralph"
                    || write["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("person:ralph"))
            })
        });
    match held_ralph {
        true => Ok(()),
        false => Err(format!(
            "{address}'s own history no longer carries Ralph's account, so the correction \
             destroyed the earlier wording instead of superseding it: {read}"
        )),
    }
}

/// The sitting that takes the March claim back.
const JULY: &str = "Phase 7";

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

/// **Every search hit whose subject is `subject`, re-serialized to JSON
/// text** — so a substring needle can be asked of the ONE record a sitting's
/// claim is about, never of the whole window.
///
/// **Re-serializing is safe for a needle that names one key beside its
/// value**, which is every needle this asks: `"recorded_at":"2026-07-05"` and
/// `"retracts":` both stay intact regardless of where the rest of the
/// object's keys land, because neither depends on a NEIGHBOUR key's position.
///
/// 🚨 **THE RULE, NOT JUST THIS LOCK'S FIX.** A whole-world count answers "did
/// this text appear in the window", never "did it appear on the record this
/// sitting's claim is about" — so it stops discriminating the moment a
/// SITTING does two things, on two subjects, in the same window. That is
/// ordinary now: every later thread woven through this room adds a sitting
/// that already had other business. **Any lock built the same way this one
/// was needs the same fix**, which is why this is its own function rather
/// than inlined once.
fn hits_naming_subject(world: &str, subject: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in world.lines() {
        let Ok(parsed) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(hits) = parsed["results"].as_array() else {
            continue;
        };
        for hit in hits {
            if hit["subject"].as_str() == Some(subject) {
                found.push(hit.to_string());
            }
        }
    }
    found
}

/// 🚨 **The claim was taken back, not rewritten — asked in July's own
/// window.**
///
/// The operator ruled 2026-09-12: the axis is not whether the subject is an
/// ongoing state or a past event, and not whether anybody could have read the
/// claim. **It is which session wrote it.** A sitting correcting its own
/// mistake, in the same breath, rewrites in place — nothing else has had a
/// chance to build on the wrong words yet. A sitting correcting an EARLIER
/// one's claim leaves the correction visible instead: the record that held
/// the wrong words is marked rather than rewritten, because a later sitting
/// does not get to edit what an earlier one said and call it the same claim.
/// **March and July are different sittings, full stop** — there is nothing
/// here to weigh, unlike the reader-visibility question this used to ask.
///
/// ⛔️ **Asked of the finished room, the positive half belongs to nobody.** The
/// club gains records after July — August writes on it — and any later
/// sitting that took a claim back would put a retraction on that subject with
/// July's name on the credit. **A count over a whole subject, graded at the
/// end of the year, credits whichever sitting the sentence happens to name.**
///
/// **So this is asked across July alone.** Late October also retracts, on
/// Nelson's attendance rather than the club's schedule, and it is four
/// sittings away.
async fn julys_claim_is_withdrawn_rather_than_rewritten(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(JULY) else {
        return Err(format!(
            "this run took no reading either side of {JULY}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    // 🚨 **Scoped to the CLUB's own records, not the whole window.** July now
    // does other business too — every woven thread adds one more sitting that
    // does — so counting the needle across the whole world would be satisfied
    // by a retraction anywhere, on any subject.
    let club_before = hits_naming_subject(&before.world, "org:north-trail-club");
    let club_after = hits_naming_subject(&after.world, "org:north-trail-club");
    let count = |hits: &[String], needle: &str| hits.iter().filter(|h| h.contains(needle)).count();
    match count(&club_after, TAKEN_BACK) > count(&club_before, TAKEN_BACK) {
        true => Ok(()),
        false => Err(format!(
            "no retraction appeared on the club in {JULY}'s window, so the March claim about \
             Tuesdays was either left standing or rewritten in place instead of withdrawn",
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

/// **How a handle reads when it is written into a sentence**, which is the
/// spelling the surface teaches and the store serves back.
///
/// **The server declares it, not this crate**, so these are a reader of a
/// spelling rather than a definition of one. The room's own fixtures write the
/// mentions out in full, which is what holds this to what a caller really
/// sends.
const MENTION: char = '@';
const KIND_ENDS: char = ':';

/// **How many kinds one record has to point at.**
///
/// **Two, because two is the least that is a LINK.** One mention is a tag: it
/// says the claim is about a thing, which the claim's own subject and its edges
/// already say. Two mentions of DIFFERENT kinds on one record is the smallest
/// thing a later sitting can leave in two directions — from the claim to a
/// thing of one kind, and from the same claim to a thing of another. That is
/// the whole of what *a later sitting has something to follow* means, and it is
/// what this room's October actually needs from June.
///
/// ⛔️ **Three is not a stronger version of the same claim.** It is a claim
/// about what the year's story happens to contain. Nothing in the capability
/// says a pointer-bearing claim names three kinds, and a sitting may file the
/// operator's three nouns across two claims and still have done the thing.
///
/// ⛔️ **And no count above two, of kinds, records or mentions.** Any larger
/// number is a measurement of one run rather than a statement about the
/// capability — it would be read off whatever a sample produced, and it would
/// go red again the first time a sitting wrote well and briefly.
const TOGETHER: usize = 2;

/// **The run of bytes a handle is drawn from**: lower case, digits, hyphen.
fn handle_run(text: &str) -> &str {
    let end = text
        .find(|one: char| !(one.is_ascii_lowercase() || one.is_ascii_digit() || one == '-'))
        .unwrap_or(text.len());
    &text[..end]
}

/// **Every handle KIND this text points at**, each named once.
///
/// ⛔️ **No kind is named here and no slug is.** The kinds a sitting reaches for
/// are its own choice, exactly as the slugs are: January invents the event's
/// handle, and a sitting that pointed at the club and the trail rather than at
/// a person and an event did the identical thing. A check that spelled either
/// would fail a run for the word it chose rather than for what it wrote — the
/// fault this room removed from its late November lock.
fn kinds_pointed_at(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (at, _) in text.match_indices(MENTION) {
        let rest = &text[at + MENTION.len_utf8()..];
        let kind = handle_run(rest);
        let after = &rest[kind.len()..];
        if kind.is_empty() || !after.starts_with(KIND_ENDS) {
            continue;
        }
        // **The slug has to be there.** `@place:` with nothing after it is a
        // word that looks like a pointer and leads nowhere.
        if handle_run(&after[KIND_ENDS.len_utf8()..]).is_empty() {
            continue;
        }
        if !found.iter().any(|seen| seen == kind) {
            found.push(kind.to_string());
        }
    }
    found
}

/// The sitting whose window this reads — the only sitting the year's story
/// asks to write a pointer at all.
const JUNE: &str = "Phase 6";

/// **Whether a standing edge of the named shape at the named object exists**,
/// read off one boundary's world — on ONE subject's own record when `subject`
/// names one, or on any record at all when it does not. Correlated on the
/// record, not just present somewhere: `edge.type`, `edge.object` and an
/// ACTIVE `status` all have to land on the SAME hit, which is the thing a
/// `carries` needle cannot ask. **The object matters whenever a subject can
/// carry more than one edge of the same shape** — Milhouse carries two
/// `location` edges over the year, one to each place he has ever lived, and a
/// check that asked only about the SHAPE would read his standing Shelbyville
/// edge as already present before April ever wrote it, because his
/// Springfield one already was. **`subject` is `None` when the record it
/// lands on can change its own handle inside the window being read** — a
/// rename does not move the id underneath, but it does move which handle a
/// read renders, so a check that pinned the OLD handle would miss its own
/// record the moment the same sitting also renames it. One body for every
/// edge-window check in this room, so a second shape is a call rather than a
/// second copy of the walk.
fn has_standing_edge(world: &str, subject: Option<&str>, shape: &str, object: &str) -> bool {
    search_hits(world).into_iter().flatten().any(|hit| {
        subject.is_none_or(|subject| hit["subject"].as_str() == Some(subject))
            && hit["status"].as_str() == Some("active")
            && hit["edge"]["type"].as_str() == Some(shape)
            && hit["edge"]["object"].as_str() == Some(object)
    })
}

/// **Whether a standing edge of the named shape links `subject` to some
/// entity of `kind`**, read off one boundary's world — the object's KIND
/// checked rather than its exact handle. The mirror image of
/// [`has_standing_edge`]'s own `subject: None`: there the record's SUBJECT
/// can rename itself inside the window being read, so the object is pinned
/// instead; here the record's OBJECT is an entity the occupant just
/// invented and was never handed a spelling for, so the subject is pinned
/// and the object is matched by kind instead. `kind` is passed with its own
/// colon (`"event:"`), which is what makes this a prefix test rather than an
/// accidental match on a handle that merely starts the same way.
fn has_standing_edge_to_a(world: &str, subject: &str, shape: &str, kind: &str) -> bool {
    search_hits(world).into_iter().flatten().any(|hit| {
        hit["subject"].as_str() == Some(subject)
            && hit["status"].as_str() == Some("active")
            && hit["edge"]["type"].as_str() == Some(shape)
            && hit["edge"]["object"]
                .as_str()
                .is_some_and(|object| object.starts_with(kind))
    })
}

/// 🚨 **June drew a standing attendee edge for each of the two people it
/// names — asked in June's own window.**
///
/// The finished board is the wrong place to ask this: late October
/// legitimately retracts Nelson's own attendance at this same survey, and a
/// retraction is marked rather than filtered — `recall` serves the retracted
/// record's own edge back, so `carries person:nelson` could not see the
/// `status` key beside it. The old lock held on that dead text exactly as
/// the pump lock once held on Ralph's, before its own fix.
///
/// **Scoped to June's own window rather than corrected in place**, unlike
/// the pump: nothing here is ever corrected, and a later retraction of
/// Nelson's attendance is legitimate rather than a mistake to catch — the
/// question this asks is only what June itself left standing, the same
/// question July's own lock asks about the club's schedule.
///
/// ⛔️ **No handle is named here, on either side.** January invents the
/// survey's own handle just as it invents everything else about it — a
/// check pinning `event:trail-survey` would fail a run that chose a
/// different word for the identical thing, the fault this room's own
/// `one_record_points_at_two_kinds` and October's location-edge check
/// already remove for their own handles. **The object is matched by KIND**
/// rather than left unpinned entirely, because an unpinned object would
/// also hold on an edge to the wrong kind of thing.
async fn junes_survey_drew_a_standing_attendee_for_each(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(JUNE) else {
        return Err(format!(
            "this run took no reading either side of {JUNE}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let gained = |who: &str| {
        !has_standing_edge_to_a(&before.world, who, "attendee", "event:")
            && has_standing_edge_to_a(&after.world, who, "attendee", "event:")
    };
    let (milhouse, nelson) = (gained("person:milhouse"), gained("person:nelson"));
    match (milhouse, nelson) {
        (true, true) => Ok(()),
        _ => Err(format!(
            "{JUNE}'s window did not draw a standing attendee edge for both — milhouse: {}, \
             nelson: {}. A later, legitimate retraction is not this check's business; it asks \
             only what {JUNE} itself left standing",
            if milhouse { "gained" } else { "not gained" },
            if nelson { "gained" } else { "not gained" },
        )),
    }
}

/// 🚨 **Nelson gained a standing membership edge in February's own window,
/// and Milhouse's — drawn earlier, in January — still stands by its end.**
///
/// Asked of the finished board, `carries person:nelson` and
/// `carries person:milhouse` hold on a retracted membership exactly as they
/// hold on a standing one — nothing in this room's honest storyline ever
/// retracts either, so the gap is structural rather than reproducing today,
/// the same shape as June's own attendance walk carried before its fix.
///
/// ⚠️ **The two are not the same claim, and a single `gained` test across
/// both is wrong for Milhouse.** He joined in January, a sitting before this
/// window opens — a check that asked whether HIS edge is gained inside
/// February's own window would fail the honest year, not catch a bug. What
/// carries over both is standing by February's own end: Nelson's, newly
/// drawn here, and Milhouse's, drawn earlier and undisturbed since — read at
/// February's own boundary either way, never the finished board's.
async fn februarys_club_drew_a_standing_member_for_each(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(FEBRUARY) else {
        return Err(format!(
            "this run took no reading either side of {FEBRUARY}, so nothing here can say what \
             that sitting recorded. A check scoped to one sitting needs the run's own \
             boundaries.",
        ));
    };
    let club = "org:north-trail-club";
    let nelson_gained = !has_standing_edge(&before.world, Some("person:nelson"), "memberOf", club)
        && has_standing_edge(&after.world, Some("person:nelson"), "memberOf", club);
    let milhouse_stands =
        has_standing_edge(&after.world, Some("person:milhouse"), "memberOf", club);
    match (milhouse_stands, nelson_gained) {
        (true, true) => Ok(()),
        _ => Err(format!(
            "by the end of {FEBRUARY} the club does not carry a standing membership edge for \
             both — milhouse standing: {milhouse_stands}, nelson gained here: {nelson_gained}",
        )),
    }
}

/// The sitting that moves Milhouse, and the place he moves to.
const APRIL: &str = "Phase 4";

/// 🚨 **April drew a standing location edge to Shelbyville — asked in April's
/// own window.**
///
/// Asked of the finished board, `carries place:shelbyville` holds on a
/// retracted claim exactly as on a standing one — nothing in this room's
/// honest storyline ever retracts it, so the gap is structural rather than
/// reproducing today, the same shape as June's attendance walk carried
/// before its fix.
///
/// **The object has to be pinned, not only the shape.** Milhouse already
/// carries a `location` edge to Springfield when April opens — a check
/// asking only whether SOME location edge is gained would read his standing
/// Shelbyville edge as already present, because a location edge of some
/// kind already was.
async fn aprils_move_drew_a_standing_location_edge(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(APRIL) else {
        return Err(format!(
            "this run took no reading either side of {APRIL}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let gained = !has_standing_edge(
        &before.world,
        Some("person:milhouse"),
        "location",
        "place:shelbyville",
    ) && has_standing_edge(
        &after.world,
        Some("person:milhouse"),
        "location",
        "place:shelbyville",
    );
    match gained {
        true => Ok(()),
        false => Err(format!(
            "{APRIL}'s window did not draw a standing location edge to place:shelbyville, so the \
             move was not recorded where a later sitting would find it",
        )),
    }
}

/// 🚨 **October drew a standing location edge to the trail — asked in
/// October's own window.**
///
/// Asked of the finished board, `carries place:north-trail` holds on a
/// retracted claim exactly as on a standing one — nothing in this room's
/// honest storyline ever retracts it, so the gap is structural rather than
/// reproducing today, the same shape as June's attendance walk carried
/// before its fix.
///
/// **No subject is pinned, unlike June's, February's or April's own
/// checks.** October renames the survey's own event in the SAME sitting
/// that writes this claim — the rename moves no id, but it does move which
/// handle a read renders the record's subject under, so a check pinning
/// `event:trail-survey` would miss its own record the moment the rename
/// runs first. Only the event ever draws a `location` edge to this place, so
/// the object alone is enough to correlate on.
async fn octobers_note_drew_a_standing_location_edge_to_the_trail(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let Some((before, after)) = seen.across(OCTOBER) else {
        return Err(format!(
            "this run took no reading either side of {OCTOBER}, so nothing here can say what \
             that sitting recorded. A check scoped to one sitting needs the run's own \
             boundaries.",
        ));
    };
    let gained = !has_standing_edge(&before.world, None, "location", "place:north-trail")
        && has_standing_edge(&after.world, None, "location", "place:north-trail");
    match gained {
        true => Ok(()),
        false => Err(format!(
            "{OCTOBER}'s window did not draw a standing location edge to place:north-trail, so \
             where the survey was held was not recorded where a later reader would find it",
        )),
    }
}

/// 🚨 **Late October drew a standing membership edge for Bart — asked in
/// late October's own window.**
///
/// Asked of the finished board, `carries person:bart` holds on a retracted
/// membership exactly as on a standing one — nothing in this room's honest
/// storyline ever retracts it, so the gap is structural rather than
/// reproducing today, the same shape February's own club walk carried
/// before its fix.
async fn late_octobers_club_drew_a_standing_member_for_bart(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let Some((before, after)) = seen.across(LATE_OCTOBER) else {
        return Err(format!(
            "this run took no reading either side of {LATE_OCTOBER}, so nothing here can say \
             what that sitting recorded. A check scoped to one sitting needs the run's own \
             boundaries.",
        ));
    };
    let club = "org:north-trail-club";
    let gained = !has_standing_edge(&before.world, Some("person:bart"), "memberOf", club)
        && has_standing_edge(&after.world, Some("person:bart"), "memberOf", club);
    match gained {
        true => Ok(()),
        false => Err(format!(
            "{LATE_OCTOBER}'s window did not draw a standing membership edge for Bart, so he is \
             not on the roster where a later reader would find him",
        )),
    }
}

/// 🚨 **One record points at two kinds of thing as HANDLES, asked in June's own
/// window.**
///
/// The operator names several things in one breath. A sitting that writes them
/// as words leaves a sentence; a sitting that writes them as handles leaves
/// pointers, and a later sitting can go from the claim to the things it names.
///
/// ⛔️ **Asked of the finished room, this discriminated for June only by
/// accident.** The check used to run a live `search` over the whole board at
/// the end of the run, so ANY sitting that ever wrote two handles into one
/// record satisfied it — June was the one sitting the reference transcript
/// happened to write pointers in, not something the check enforced. A later
/// sitting that wrote the pointer instead would have passed this exactly the
/// same way, crediting June for work it did not do.
///
/// **So this reads the world either side of June and asks whether a
/// two-kind record landed there**, the same pattern the club's and the pump's
/// locks use. No later sitting can satisfy it, because no later sitting's
/// window is read.
///
/// ⚠️ **A hatch, because an assertion is a substring of the WHOLE answer.**
/// Two `carries` lines hold on two separate records naming one thing each,
/// which is the easy case and not the one being watched. **Whether two landed
/// on ONE record is a correlation inside one object**, and the lock format's
/// three words do not branch — the same reason the loop's own lock is a hatch.
///
/// ⛔️ **It used to demand a person, a place and an event together, and that
/// could not be met.** The year's story never asks a sitting to name all three
/// in one sentence, so a run that wrote pointers on half its claims failed a
/// lock about pointers. **A red that survives the fix it asks for is a red
/// nobody trusts the next time it fires**, so the floor is now what the
/// capability is for rather than one combination of nouns: see [`TOGETHER`].
///
/// ⛔️ **The empty board is a failure with its own words.** A store the run
/// never wrote in holds no records, and reporting *no record points at two
/// things* about it would blame a sitting for a room nobody worked.
async fn one_record_points_at_two_kinds(seen: &Observed<'_>) -> Result<(), String> {
    let Some((before, after)) = seen.across(JUNE) else {
        return Err(format!(
            "this run took no reading either side of {JUNE}, so nothing here can say what that \
             sitting recorded. A check scoped to one sitting needs the run's own boundaries.",
        ));
    };
    let had = records_pointing_at_two_kinds(&before.world);
    let has = records_pointing_at_two_kinds(&after.world);
    if has > had {
        return Ok(());
    }
    // **The two ways of not holding are worth telling apart.** No pointer at
    // all is the sitting that wrote words; pointers that never share a record
    // is a sitting that tagged things one at a time and linked nothing. Both
    // are read off June's own window, never off the finished board.
    let Some(hits) = search_hits(&after.world) else {
        return Err(format!(
            "the board came back with no results at all after {JUNE}, so nothing here was \
             measured: {}",
            after.world,
        ));
    };
    let mut anywhere: Vec<String> = hits
        .iter()
        .flat_map(|hit| kinds_pointed_at(&written(hit)))
        .collect();
    anywhere.sort_unstable();
    anywhere.dedup();
    match anywhere.is_empty() {
        true => Err(format!(
            "not one of the {} records the board holds after {JUNE} writes a handle into its \
             words, so every thing the year names is spelled out rather than pointed at",
            hits.len(),
        )),
        false => Err(format!(
            "handles are written into sentences and the kinds they point at anywhere on the \
             board after {JUNE} are {anywhere:?}, but no ONE of the {} records names two of \
             different kinds together — so every pointer stands alone and no claim leads from a \
             thing of one kind to a thing of another",
            hits.len(),
        )),
    }
}

/// **What a reader sees, which is the only side of a mention a room can
/// reach.** The store keeps a badge; every read serves the handle it wears
/// today, and the index scans through the same layer — so a hit's own text
/// is what a later sitting would follow.
fn written(hit: &Value) -> String {
    ["content", "details"]
        .iter()
        .filter_map(|key| hit[*key].as_str())
        .collect::<Vec<&str>>()
        .join(" ")
}

/// **The search half of a boundary's world**, parsed. A boundary's world is
/// `list_entities` and a `search` over everything, joined by a newline; only
/// the second carries the records a mention could be written on.
fn search_hits(world: &str) -> Option<Vec<Value>> {
    let (_, searched) = world.split_once('\n')?;
    let parsed: Value = serde_json::from_str(searched).ok()?;
    parsed["results"].as_array().cloned()
}

/// How many records in one boundary's world point at two different kinds of
/// thing as handles.
fn records_pointing_at_two_kinds(world: &str) -> usize {
    search_hits(world)
        .into_iter()
        .flatten()
        .filter(|hit| kinds_pointed_at(&written(hit)).len() >= TOGETHER)
        .count()
}

/// **The one full handle of `kind` written into a sentence**, slug included.
///
/// [`kinds_pointed_at`] asks only which KINDS a sentence points at, and says
/// beside itself why: the slug is the sitting's own choice and a lock that
/// spelled it would fail a run that chose a different one. This reads the
/// slug too, on purpose — it is safe here because nothing calls it with a
/// literal slug to compare against, only with another slug this same
/// function read off the board a different time.
fn mention_of_kind(text: &str, kind: &str) -> Option<String> {
    for (at, _) in text.match_indices(MENTION) {
        let rest = &text[at + MENTION.len_utf8()..];
        let found = handle_run(rest);
        if found != kind {
            continue;
        }
        let after = &rest[found.len()..];
        if !after.starts_with(KIND_ENDS) {
            continue;
        }
        let slug = handle_run(&after[KIND_ENDS.len_utf8()..]);
        if slug.is_empty() {
            continue;
        }
        return Some(format!("{found}:{slug}"));
    }
    None
}

/// **Address and event-mention of every record in one boundary's world that
/// points at an event.** The address is what lets a later read ask about the
/// SAME record again rather than about whichever one currently qualifies.
fn event_mentions_by_address(world: &str) -> std::collections::HashMap<String, String> {
    search_hits(world)
        .into_iter()
        .flatten()
        .filter_map(|hit| {
            let address = hit["address"].as_str()?.to_string();
            let mention = mention_of_kind(&written(&hit), "event")?;
            Some((address, mention))
        })
        .collect()
}

/// The subject half of a record's address — everything before the `#`.
fn subject_of(address: &str) -> &str {
    address.split('#').next().unwrap_or(address)
}

/// 🚨 **A stored mention renders under whichever handle its thing wears NOW,
/// and this is where that is watched rather than assumed.**
///
/// June points at the survey by handle, not by word — the claim
/// [`one_record_points_at_two_kinds`] exists to prove happened. October gives
/// a reason to rename the survey. Nothing about a rename touches June's own
/// words: the badge behind its mention is permanent, and what changes is
/// what a READ of that claim renders back.
///
/// ⚠️ **A hatch, for the reason [`one_record_points_at_two_kinds`] already
/// gives one level up.** No query on this surface asks *what did a mention
/// render as at one time, against what it renders as at another* — `carries`
/// is a claim about the answer as it stands, never about a change in it.
///
/// 🚨 **Anchored to June's own RECORDS, not to whichever claim currently
/// points at an event.** A sitting can reach the same end state two ways: it
/// can rename the survey, or it can retract June's claim and write a fresh
/// one naming a different event. Both leave a claim pointing at an event
/// under a handle June's own words never used — but only one of them is a
/// rename, and only one is what October's reason was given for. **A
/// retraction never appears in a boundary at all** — `search` serves active
/// records only, [`septembers_account_of_the_pump_is_corrected_in_place`]
/// says why — so this
/// finds June's own addresses by the same window-diff that check uses, and
/// asks a LIVE, status-aware read of each one whether it is still active
/// before comparing what it renders now against what June's window first
/// recorded. A retracted address fails naming that rather than reporting an
/// unrelated handle as unchanged.
///
/// 🚨 **Plural, on purpose — a run 19 transcript wrote June's claim as more
/// than one record**, an attendance claim per person as well as the club's
/// own, each pointing at the survey by its own mention. **Every one of them
/// must clear the bar, and a zero stays refused.** A pass on any single
/// candidate would let a rename that reached half of June's claims and left
/// the rest stale call itself done; a run where the count happens to be one
/// is this at its smallest, not a different question.
///
/// ⛔️ **No handle is named here, on either side.** January invents the
/// survey's first one and October's reason invents its second, and a lock
/// that spelled either would fail a run that chose different words for
/// either sitting — the fault this room already removed from June's own
/// lock. **What is locked is that every one of June's own addresses, read
/// live, renders a different handle than the one its own window first
/// recorded**: a build that stores the word June typed shows the same
/// handle either time; a build that stores the pointer does not.
async fn junes_survey_mention_renders_under_the_current_handle(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let Some((before, after)) = seen.across(JUNE) else {
        return Err(format!(
            "this run took no reading either side of {JUNE}, so nothing here can say which \
             record is June's own claim about the survey",
        ));
    };
    let earlier = event_mentions_by_address(&before.world);
    // **Sorted rather than left in whatever order a HashMap gives them.** The
    // rows this folds over carry no order of their own, and a verdict that
    // could depend on which candidate happens to be visited first is a
    // verdict that could hide a failing one behind a passing one.
    let mut candidates: Vec<(String, String)> = event_mentions_by_address(&after.world)
        .into_iter()
        .filter(|(address, _)| !earlier.contains_key(address))
        .collect();
    candidates.sort();
    if candidates.is_empty() {
        return Err(format!(
            "{JUNE}'s own window left no record newly pointing at an event, so there is no \
             claim here to watch get renamed",
        ));
    }
    // **One live read per distinct subject, never per candidate.** June may
    // point at the survey from more than one subject — the club's own claim,
    // an attendance claim on a person — and a read is asked once of each.
    let mut subjects: Vec<&str> = candidates
        .iter()
        .map(|(address, _)| subject_of(address))
        .collect();
    subjects.sort_unstable();
    subjects.dedup();
    let mut current: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for subject in subjects {
        let read = seen
            .room
            .call("recall", json!({"subject": subject, "facts": true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
        for fact in parsed["objects"][0]["facts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(address) = fact["address"].as_str() {
                current.insert(address.to_string(), fact.clone());
            }
        }
    }
    // **Every candidate, not the first one that passes.** A rename that
    // reached some of June's claims and not others is a worse result than
    // one that reached none, and an answer satisfied by any single candidate
    // would call that a pass.
    for (address, original) in &candidates {
        let Some(fact) = current.get(address) else {
            return Err(format!(
                "{address} — one of {JUNE}'s own claims pointing at the survey — is not on its \
                 subject's record any more"
            ));
        };
        if fact["status"] != "active" {
            return Err(format!(
                "{address} — one of {JUNE}'s own claims pointing at the survey — is {} rather \
                 than active, so whatever satisfies a different mention there is a fresh claim \
                 naming a different event rather than a rename of the one {JUNE} pointed at",
                fact["status"],
            ));
        }
        let now = fact["content"]
            .as_str()
            .and_then(|content| mention_of_kind(content, "event"));
        match now {
            Some(now) if now != *original => {}
            Some(now) => {
                return Err(format!(
                    "{address} still renders the survey under {now}, the handle {JUNE}'s own \
                     window recorded there, so nothing here shows that mention resolving to a \
                     handle it did not wear when it was written"
                ));
            }
            None => {
                return Err(format!(
                    "{address} no longer renders as pointing at an event at all"
                ));
            }
        }
    }
    Ok(())
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

/// The subject the pairing's control record hangs off, and the address it is
/// read through. **The occupant creates this record and may legitimately
/// correct it**, which is why the lock below asks about a relation rather than
/// a count.
const UNTOUCHED: &str = "person:bart";
const UNTOUCHED_RECORD: &str = "person:bart#f1";

/// **What one boundary saw this record holding**, or nothing when the record
/// did not exist yet.
///
/// A boundary's world is `list_entities` and a `search` over everything, joined
/// by a newline; the search half carries each fact under `results` with its
/// own `address` and `content`. **The shape is read off a real boundary rather
/// than assumed** — a stand-in built from what the format ought to be would
/// exercise a world this room never produces.
fn held_at(world: &str, address: &str) -> Option<String> {
    let (_, searched) = world.split_once('\n')?;
    let parsed: Value = serde_json::from_str(searched).ok()?;
    parsed["results"]
        .as_array()?
        .iter()
        .find(|hit| hit["address"].as_str() == Some(address))
        .and_then(|hit| hit["content"].as_str())
        .map(str::to_string)
}

/// **A record's trace carries exactly the writes the run made to it.**
///
/// ⛔️ **Not a count of writes, and the difference is the whole lock.** The
/// control record is one the OCCUPANT creates, so a sitting may legitimately
/// notice its own mistake and rewrite it — and a lock demanding one write
/// scored that correction as a fault. It was asserting what the model happened
/// to do that run, never anything about jojobot. **Moving to a different
/// control record would move the trap rather than close it**: every record in
/// this room is reachable by some sitting.
///
/// So the two halves are read from two places that cannot both be wrong in the
/// same direction: **how many times the run wrote this record is counted from
/// the phase boundaries**, which are readings taken by the runner and not by
/// the verb under test, and **what the record says about itself is read from
/// its trace**. A legitimate correction moves both and passes. A trace that
/// reports a write nobody made moves only one and fails, which is the class
/// this exists for — a history read that fabricates a wording says jojobot
/// changed its mind when it did not, and that is worse than silence because a
/// reader acts on it.
///
/// ⚠️ **The count is exact at PHASE granularity, which is the granularity
/// every reading in this room has.** Two writes to this record inside one
/// sitting are one observed change, so the lock would read them as one. No
/// sitting here writes this record twice, and a room that grew one would have
/// to say so.
///
/// ⚠️ **The failing half cannot be staged by a play.** A play can only make
/// writes that really happened, so every play produces a trace that agrees
/// with the boundaries. The negative is a product fault, and it is watched by
/// breaking the trace rather than by driving the year differently.
async fn a_records_trace_matches_the_writes_the_run_made(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let held: Vec<String> = seen
        .boundaries
        .iter()
        .filter_map(|at| held_at(&at.world, UNTOUCHED_RECORD))
        .collect();
    // **The positive the lock rests on.** Without it a year where the record
    // was never written reports a trace of one write as correct, because zero
    // observed states and a one-write trace would never be compared at all.
    if held.is_empty() {
        return Err(format!(
            "no boundary saw {UNTOUCHED_RECORD} at all, so the record this lock is about was never written and there is nothing here to have a trace"
        ));
    }
    // The record's first appearance is one write; every later change of what it
    // holds is one more.
    let made = 1 + held.windows(2).filter(|pair| pair[0] != pair[1]).count();

    let read = seen
        .room
        .call(
            "recall",
            json!({"subject": UNTOUCHED, "history_record": UNTOUCHED_RECORD}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    // **`count` rather than the length of `writes`.** A trace ships what it
    // shows and says how many there are; a long history comes back elided, and
    // measuring the shipped list would read an elision as a missing write.
    let trace = &parsed["objects"][0]["record_history"];
    let Some(reported) = trace["count"].as_u64().map(|count| count as usize) else {
        return Err(format!(
            "the trace of {UNTOUCHED_RECORD} came back with no writes on it at all, so nothing was measured: {read}"
        ));
    };
    if reported == made {
        return Ok(());
    }
    let seen_holding: Vec<&str> = held.iter().map(String::as_str).collect();
    Err(format!(
        "the trace of {UNTOUCHED_RECORD} reports {reported} write(s) and the run made {made} — across the boundaries the record was seen holding {seen_holding:?}, so the trace and what the run actually did disagree: {read}"
    ))
}

/// The canoe's own subject, and the sitting whose entry asks the operator's
/// real question — pull the year together without losing anything and
/// without a stack of separate notes — never the word "fold" or a mark's
/// name.
const CANOE: &str = "thing:canoe";
const LATER_DECEMBER: &str = "Phase 15";

/// Every fact `recall` currently reports for the canoe.
async fn canoes_facts(seen: &Observed<'_>) -> Vec<Value> {
    let read = seen
        .room
        .call("recall", json!({"subject": CANOE, "facts": true}))
        .await;
    serde_json::from_str::<Value>(&read)
        .ok()
        .and_then(|body| body["objects"][0]["facts"].as_array().cloned())
        .unwrap_or_default()
}

/// The one fact, if any, that stands for others — the fold this room is
/// watching for. `stands_for` is unforgeable by construction (rule 234's
/// shape does not apply here; this is the dedicated-field kind), so finding
/// one non-empty array here means a caller actually set the mark through the
/// served surface.
fn the_fold(facts: &[Value]) -> Option<&Value> {
    facts.iter().find(|fact| {
        fact["stands_for"]
            .as_array()
            .is_some_and(|marks| !marks.is_empty())
    })
}

/// 🚨 **A sitting late in the year folds the canoe's pile, and nothing in its
/// entry told it to.** The document's own entry for {LATER_DECEMBER} asks the
/// operator's real question — pull the year together without losing anything
/// and without a stack of separate notes — and never names a mark, a verb, or
/// the word "fold". A session that reads the orientation essay's own example
/// is the only thing that can make this pass; nothing here scripts it.
///
/// ⛔️ **Existence alone is not enough — the window is the point.** A mark
/// that happened to exist before this room ever asked anything would satisfy
/// a plain "does a mark exist" question while proving nothing about THIS
/// sitting choosing to make it. So this reads the board either side of
/// {LATER_DECEMBER}'s own window and asks whether the mark is new there — the
/// same technique March's own lock uses for the identical shape of claim.
async fn a_late_sitting_folds_the_canoes_pile_without_being_told_to(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let Some((before, after)) = seen.across(LATER_DECEMBER) else {
        return Err(format!(
            "this run took no reading either side of {LATER_DECEMBER}, so nothing here can say \
             whether that sitting folded anything",
        ));
    };
    let had = before
        .world
        .matches("\"stands_for\":[\"thing:canoe")
        .count();
    let has = after.world.matches("\"stands_for\":[\"thing:canoe").count();
    match has > had {
        true => Ok(()),
        false => Err(format!(
            "no record standing for the canoe's other claims appeared in {LATER_DECEMBER}'s own \
             window — the pile of small repairs is still just a pile",
        )),
    }
}

/// ⭐ **Both directions of the one property this feature promises: synthesis
/// layers and never discards (rule 263).** A run where the fold made the pile
/// look like one right up until the pile stopped existing is a worse failure
/// than never folding at all, however tidy the finished board reads — so this
/// asks whether EVERY one of the canoe's five small repairs is still active,
/// and whether the fold's own mark actually names each of them, rather than
/// asking either question alone.
///
/// 🚨 **A hatch, because both halves live on the SAME set of records and
/// nothing on this surface reports them together.** `facts` says what stands
/// today; whether the fold's `stands_for` list covers a given address is a
/// set-membership question over that same answer, which is past what a
/// substring assertion can say.
///
/// **The fold's own address is exempt from being named** — a fold made by
/// rewriting one of the five in place cannot name itself (rule: a mark
/// naming its own address is refused), so its coverage is that it is still
/// active, not that the mark lists it.
///
/// 🚨 **EVERY record on a day, not the first one `recorded_at` happens to
/// list.** February plausibly writes two canoe facts on the same date —
/// picking it up, and finding the soft spot — and a fold that named one but
/// not the other left the second exactly as lost as if it named neither. A
/// day is a SELECTION, never an address by another name.
async fn the_canoes_five_repairs_are_still_active_and_named_by_the_fold(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let facts = canoes_facts(seen).await;
    let Some(fold) = the_fold(&facts) else {
        return Err(
            "no record on the canoe stands for any other, so there is no fold to check the \
             sources of"
                .into(),
        );
    };
    let fold_address = fold["address"].as_str().unwrap_or_default();
    let named: std::collections::HashSet<&str> = fold["stands_for"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|address| address.as_str())
        .collect();
    let mut missing = Vec::new();
    for (day, _) in CANOE_DAYS {
        let sources: Vec<&Value> = facts
            .iter()
            .filter(|fact| fact["recorded_at"] == day)
            .collect();
        if sources.is_empty() {
            missing.push(format!("{day} — no repair on record for that day at all"));
            continue;
        }
        for source in sources {
            if source["status"] != "active" {
                missing.push(format!(
                    "{day} — {} is not active, so the fold cost the original account",
                    source["address"]
                ));
                continue;
            }
            let address = source["address"].as_str().unwrap_or_default();
            if address != fold_address && !named.contains(address) {
                missing.push(format!(
                    "{day} — {address} is active but the fold does not name it, so it reads as \
                     lost even though it is still there"
                ));
            }
        }
    }
    match missing.is_empty() {
        true => Ok(()),
        false => Err(format!(
            "the fold does not leave the whole picture reachable: {}",
            missing.join("; ")
        )),
    }
}

/// The five days the canoe gains a small repair — read by the lock above to
/// find each one, and by the sentence beside it to say what a reader loses.
const CANOE_DAYS: [(&str, &str); 5] = [
    ("2026-02-08", "the soft spot was noticed"),
    ("2026-03-15", "the soft spot was patched"),
    ("2026-05-10", "the seat was varnished"),
    ("2026-07-05", "the foot brace was replaced"),
    ("2026-09-13", "a new crack was patched"),
];

/// 🚨 **THE FABRICATION CHECK.** The folded record must not assert a date
/// that none of the canoe's other records ever gave — the failure this mark
/// is most likely to produce, being asked to say less while sounding more
/// certain than any one part of the pile it is drawn from.
///
/// **Dates only, not cadence or confidence.** The other two are a person's
/// reading of tone — whether an answer reads well is not something a lock
/// can hold, the same line this room already draws around a rewrite's
/// wording. A date is the one part of "invented a cadence, a made-up date, or
/// a confidence nobody earned" that is a plain fact, checkable without
/// reading anyone's prose for how it sounds.
///
/// **`recorded_at` is sourced against EVERY canoe record, the fold's own
/// included — `happened_at` is not.** The fold's own `recorded_at` is the
/// day it was actually written, and stating that day is not invention;
/// excluding it convicted an honest fold for naming the day it was itself
/// written on. But `happened_at` is exactly the field a fold's own
/// invention would land on — a single day claimed for a pile that spans
/// several — so the fold's OWN `happened_at` cannot be a source for judging
/// itself, only the other five records' can be.
async fn the_canoes_fold_invents_no_date_the_repairs_never_gave(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let facts = canoes_facts(seen).await;
    let Some(fold) = the_fold(&facts) else {
        return Err(
            "no record on the canoe stands for any other, so there is nothing here to check for \
             an invented date"
                .into(),
        );
    };
    let fold_address = fold["address"].as_str().unwrap_or_default().to_string();
    let mut known: std::collections::HashSet<&str> = facts
        .iter()
        .filter_map(|fact| fact["recorded_at"].as_str())
        .collect();
    known.extend(
        facts
            .iter()
            .filter(|fact| fact["address"] != fold_address.as_str())
            .filter_map(|fact| fact["happened_at"].as_str()),
    );
    if let Some(claimed) = fold["happened_at"].as_str()
        && !known.contains(claimed)
    {
        return Err(format!(
            "the fold says the pile happened on {claimed}, a day none of the canoe's other \
             records give"
        ));
    }
    let content = fold["content"].as_str().unwrap_or_default();
    for date in iso_dates_in(content) {
        if !known.contains(date.as_str()) {
            return Err(format!(
                "the fold's own words name {date}, a day none of the canoe's other records give"
            ));
        }
    }
    Ok(())
}

/// Every `YYYY-MM-DD` substring in `text`, hand-rolled because this crate
/// carries no regex dependency and a fabricated date is as likely to land in
/// prose as in a structured field.
fn iso_dates_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while at + 10 <= bytes.len() {
        if !text.is_char_boundary(at) || !text.is_char_boundary(at + 10) {
            at += 1;
            continue;
        }
        let slice = &text[at..at + 10];
        let shaped = slice.as_bytes().iter().enumerate().all(|(offset, &b)| {
            if offset == 4 || offset == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
        if shaped {
            found.push(slice.to_string());
            at += 10;
        } else {
            at += 1;
        }
    }
    found
}

/// The same-breath mistake December's own entry asks a sitting to make and
/// catch, and the subject it lands on — invented for this alone, so nothing
/// else in the year ever reads or writes it.
const BIKE_LOCK: &str = "thing:bike-lock";

/// 🚨 **The bike lock's mistake is rewritten in place, not withdrawn — because
/// this sitting is the one that made it.**
///
/// The operator ruled 2026-09-12: the axis is which SESSION wrote a claim, not
/// whether the subject is an ongoing state or whether anybody could have read
/// it. March's Tuesday claim and July's correction are different sittings, so
/// July withdraws — [`julys_claim_is_withdrawn_rather_than_rewritten`] is the
/// other half of this same rule. **Here the mistake and its correction are the
/// SAME sitting**, so nothing has had a chance to build on the wrong words
/// yet, and a rewrite in place is right.
///
/// **Three things prove a rewrite happened rather than a retract-and-recapture
/// wearing a rewrite's clothes**: one record, still active, whose own trace
/// carries more than the one write. A retract-and-recapture leaves TWO records
/// — one retracted, one fresh — where a rewrite leaves one. Checking only the
/// trace's length would pass a record correction plus an unrelated second
/// claim beside it; checking only "one active record" would pass a rewrite
/// that never actually corrected anything.
///
/// **The entry's own `was` annotation is a second, legitimate record on this
/// same subject, and it is not this claim.** [`the_bike_locks_mistake_is_rewritten_in_place`]'s
/// neighbour lock asks the sitting to capture a note carrying a `was` field
/// once it has read the trace; counting that note as a second account of the
/// bike lock's own mistake would fail the honest year. It is told apart
/// structurally, by the field the entry itself names, rather than by its
/// prose.
async fn the_bike_locks_mistake_is_rewritten_in_place(seen: &Observed<'_>) -> Result<(), String> {
    let read = seen
        .room
        .call("recall", json!({"subject": BIKE_LOCK, "facts": true}))
        .await;
    let parsed: Value = serde_json::from_str(&read).unwrap_or(Value::Null);
    let Some(facts) = parsed["objects"][0]["facts"].as_array() else {
        return Err(format!(
            "nothing is on file for {BIKE_LOCK} at all, so this sitting never made the mistake \
             its own entry asks it to catch: {read}"
        ));
    };
    let active: Vec<&Value> = facts
        .iter()
        .filter(|f| f["status"] == "active" && f["fields"]["was"].is_null())
        .collect();
    let [fact] = active.as_slice() else {
        return Err(format!(
            "{BIKE_LOCK} carries {} active record(s) of its own mistake rather than one, so the \
             correction was filed as a fresh claim beside the first — or beside a retraction of \
             it — instead of a rewrite of the one record: {read}",
            active.len(),
        ));
    };
    let Some(address) = fact["address"].as_str() else {
        return Err(format!(
            "the claim on {BIKE_LOCK} carries no address: {read}"
        ));
    };
    let trace = seen
        .room
        .call(
            "recall",
            json!({"subject": BIKE_LOCK, "history_record": address}),
        )
        .await;
    let parsed_trace: Value = serde_json::from_str(&trace).unwrap_or(Value::Null);
    let writes = parsed_trace["objects"][0]["record_history"]["writes"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    match writes >= 2 {
        true => Ok(()),
        false => Err(format!(
            "{address}'s own trace carries {writes} write(s), so nothing here shows a mistake \
             being caught and corrected in the same breath it was made: {trace}"
        )),
    }
}
