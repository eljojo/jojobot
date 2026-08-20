//! **What must be true of the room when a run is over.**
//!
//! Rust rather than prose in the playbook, because an assertion over store
//! state reads the surface, branches and reports — that is code, and every
//! attempt to say it in English ends as a small language somebody has to learn.
//! Each one is keyed to a phase by name, and the playbook stays a document.
//!
//! **The bar for writing one at all**: does it measure a visible external side
//! effect? Not that the agent said the right thing — that it LEFT something,
//! and that the something is observable through the served surface. A phase
//! whose claim lives only in the agent's answer gets no expectation here, and
//! that is a property of the phase rather than a gap.
//!
//! **The trap this file has to keep stepping around**: a room starts nearly
//! empty, so an assertion that something is absent holds by default on a run
//! where the agent did nothing at all. Every absence here carries the positive
//! it rests on, in the same check.

use serde_json::json;

use crate::run::{Expectation, Observed, Outcome};
use crate::surface::Seed;

/// The suite these expectations belong to. Keyed by the document's own name:
/// a playbook nobody has written expectations for must not run at all, and a
/// match on the file is what makes that refusal possible.
pub const COLD_SESSION_SUITE: &str = "COLD-SESSION-SUITE.md";

pub use crate::bike_room::BIKE_ROOM;
pub use crate::ledger_room::LEDGER_ROOM;
pub use crate::loop_room::LOOP_ROOM;

/// The expectations for a playbook, or nothing when none are written.
pub fn for_playbook(source: &str) -> Option<Vec<Box<dyn Expectation>>> {
    if source.ends_with(BIKE_ROOM) {
        return Some(crate::bike_room::expectations());
    }
    if source.ends_with(LOOP_ROOM) {
        return Some(crate::loop_room::expectations());
    }
    if source.ends_with(LEDGER_ROOM) {
        return Some(crate::ledger_room::expectations());
    }
    if !source.ends_with(COLD_SESSION_SUITE) {
        return None;
    }
    Some(vec![
        Box::new(TheRosterIsUntouched),
        Box::new(ReadingMovedNoMail),
        Box::new(TheWritesLanded),
        Box::new(TheSessionKeptItsOwnRecord),
        Box::new(TheLoopIsOnTheRecord),
        Box::new(TheVocabularyGovernsAThing),
        Box::new(TheFrameStampedTwoDays),
        Box::new(TheRunWasLeftOpen),
        Box::new(TheHandoffWasPickedUp),
        Box::new(TheWrappedRunIsNotOfferedBack),
    ])
}

/// **What the room has to be furnished with for those checks to mean
/// anything**, and an empty seed for a playbook nobody wrote any for.
///
/// It lives beside the expectations because it is the other half of one of
/// them, and two halves of one claim in two files drift apart.
///
/// **One message, waiting.** Phase 7's whole claim is that polling a box moves
/// nothing in it, and the positive that claim rests on is that there was mail
/// to leave alone — while the playbook posts its first message in phase 8. So
/// an unfurnished room reaches that boundary empty, the check correctly reports
/// that it tested nothing, and the run fails on the harness rather than on the
/// product. Exactly one, because the phases after this one count what is in the
/// boxes.
pub fn seed_for(source: &str) -> anyhow::Result<Seed> {
    if source.ends_with(BIKE_ROOM) {
        return crate::bike_room::seed();
    }
    if source.ends_with(LOOP_ROOM) {
        return crate::loop_room::seed();
    }
    if source.ends_with(LEDGER_ROOM) {
        return crate::ledger_room::seed();
    }
    if !source.ends_with(COLD_SESSION_SUITE) {
        return Ok(Seed::new());
    }
    Ok(Seed::new().message(
        "assistant",
        SEEDED_SUBJECT,
        "The delta list was left half sorted.",
    ))
}

/// **The subject the furniture carries, and it is how a check tells the
/// furniture apart from a handoff.**
///
/// Posting into a box its sender owns delivers nothing, so this message is
/// still unfinished when a cold session arrives — sitting in that box beside
/// the handoff an earlier phase left, and indistinguishable from it by state.
/// A check that means the handoff has to be able to say which one it means.
const SEEDED_SUBJECT: &str = "from before the room opened";

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

/// **Phase 2, steps 7 and 8 — the door minted no identity.**
///
/// ⚠️ **It covers those two steps and no others, and the name says so.** The
/// phase also asks, at step 5, what the charter told the agent it is for —
/// **a judgement with no side effect anywhere**, so nothing here can measure
/// it. A check named for the phase while asserting one of its steps reports
/// PASS over a question nobody answered, which is how the shipped charter went
/// through a whole paid run with nothing recording what it taught.
///
/// **A phase can have a check for one claim and none for another**, and the
/// suite says which is which where the steps are written.
///
/// The phase asks the agent to boot a name that is no bot, and the claim is
/// that the door mints nothing. So what must be true afterwards is an absence:
/// no bot by that name.
///
/// **An absence proves nothing on its own here.** A room where the agent never
/// connected has no such bot either. So the positive it rests on is in the same
/// check: the roster the instance ships with is still there, which says the
/// read reached a room that has bots in it at all.
struct TheRosterIsUntouched;

#[async_trait::async_trait]
impl Expectation for TheRosterIsUntouched {
    fn name(&self) -> &str {
        "Phase 2 — the door minted no identity (steps 7 and 8)"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let bots = seen
            .room
            .call("list_entities", json!({"kind": "bot"}))
            .await;
        if !bots.contains("bot:assistant") {
            return missed(
                self.name(),
                format!(
                    "the shipped identity is not in the room, so its absence proves nothing: \
                     {bots}"
                ),
            );
        }
        if bots.contains("zzz-not-a-bot") {
            return missed(
                self.name(),
                format!("the door minted an identity for a name that is no bot: {bots}"),
            );
        }
        held(
            self.name(),
            "the shipped roster is there and the refused name minted nothing",
        )
    }
}

/// **Phase 7 — mail, without taking on work.**
///
/// The one phase whose whole claim is that nothing moved: polling a box counts
/// what is waiting and takes delivery of none of it. So the assertion is a
/// comparison across the phase boundary rather than a read of the end state —
/// later phases post and process mail on purpose, and a check that only looked
/// at the finished room could not tell those apart from a poll that took
/// delivery.
///
/// The positive it rests on: there was mail to leave alone. A room whose boxes
/// were empty through the phase would satisfy "nothing moved" without the claim
/// having been tested at all.
struct ReadingMovedNoMail;

#[async_trait::async_trait]
impl Expectation for ReadingMovedNoMail {
    fn name(&self) -> &str {
        "Phase 7 — polling a box moved nothing in it"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some((before, after)) = seen.across("Phase 7") else {
            return missed(
                self.name(),
                "the run recorded no boundary for this phase, so nothing was compared",
            );
        };
        // **The positive, read out of the answer rather than off its size.** A
        // board reading is whatever `search` returned, and `search` returns its
        // envelope — counts, coverage, an empty result list — whether or not a
        // message matched. So a reading is never an empty string, and a guard
        // on its length is a guard that cannot fire: the check would report
        // that mail was left alone on a room that had none.
        let waiting = messages_in(&before.mail);
        if waiting == 0 {
            return missed(
                self.name(),
                "no message was on the board before the phase, so 'nothing moved' tested nothing",
            );
        }
        if before.mail != after.mail {
            return missed(
                self.name(),
                format!(
                    "a poll moved mail.\nbefore: {}\nafter:  {}",
                    before.mail, after.mail
                ),
            );
        }
        held(
            self.name(),
            format!(
                "{waiting} message(s) were on the board and the phase left every one where it was"
            ),
        )
    }
}

/// How many message hits a board reading carries.
///
/// **Counted out of the answer, never inferred from its length.** The reading
/// is a whole `search` answer, so its size says how much envelope came back
/// and nothing about whether a message did.
fn messages_in(reading: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(reading)
        .ok()
        .and_then(|body| {
            body["results"]
                .as_array()
                .map(|hits| hits.iter().filter(|hit| hit["hit"] == "message").count())
        })
        .unwrap_or(0)
}

/// **Phase 8 — writes.** The richest phase, and the one closest to what the
/// product claims: a session that was told a task and not a tool leaves records
/// a later session can find.
///
/// Each part is a visible external side effect, read back through the surface
/// a later session would use.
struct TheWritesLanded;

#[async_trait::async_trait]
impl Expectation for TheWritesLanded {
    fn name(&self) -> &str {
        "Phase 8 — the writes are in the room"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let mut short = Vec::new();

        let recalled = seen
            .room
            // This check reads the CLAIMS, so it asks for them: a read answers
            // with what a thing is, and the records it was folded from come
            // back only when the call says so.
            .call(
                "recall",
                json!({"subject": "person:smoke-alpha", "facts": true}),
            )
            .await;
        if recalled.contains("\"status\":\"blocked\"") {
            return missed(
                self.name(),
                format!("person:smoke-alpha was never created: {recalled}"),
            );
        }
        // One active claim, and it defaulted the cautious way. A phase that
        // wrote three claims and a phase that wrote none are both failures, and
        // counting is what tells them apart.
        let active = recalled.matches("\"status\":\"active\"").count();
        if active != 1 {
            short.push(format!(
                "person:smoke-alpha carries {active} active claims rather than one"
            ));
        }
        if !recalled.contains("\"provenance\":\"inference\"") {
            short.push("the claim is not recorded as an inference".to_string());
        }

        // The near-miss the phase provokes must have minted nothing.
        let people = seen
            .room
            .call("list_entities", json!({"kind": "person"}))
            .await;
        if people.contains("person:smoke-alfa") {
            short.push("the near miss minted a second person".to_string());
        }

        // A bot arrives with the box named for it, in the one act.
        let bots = seen
            .room
            .call("list_entities", json!({"kind": "bot"}))
            .await;
        if !bots.contains("bot:smoke-gamma") {
            short.push("bot:smoke-gamma was never created".to_string());
        }
        let boarded = seen.room.call("start_here", json!({"brief": true})).await;
        if !bot_has_mailbox(&boarded, "bot:smoke-gamma") {
            short.push("no mailbox opened for bot:smoke-gamma".to_string());
        }

        // And the message a later phase has to find.
        let mail = seen
            .room
            .call("search", json!({"query": "smoke", "include_mail": true}))
            .await;
        if !mail.contains("\"hit\":\"message\"") {
            short.push("no message was left in a box".to_string());
        }

        if short.is_empty() {
            held(
                self.name(),
                "the person, its single inferred claim, the bot with its box and the message are \
                 all readable through the surface",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// Whether a boarding snapshot shows a box hanging off this bot.
///
/// **Read off that bot's own `mail`, never off the text of the answer.** A
/// snapshot lists every bot by handle whether or not it owns a box, and hangs
/// the box beside it — absent when there is none. So a search for the handle in
/// the answer measures that the bot exists, which is a different claim and the
/// one the check beside this already makes.
fn bot_has_mailbox(boarded: &str, handle: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(boarded)
        .ok()
        .and_then(|body| {
            body["snapshot"]["entities"]["bots"].as_array().map(|bots| {
                bots.iter()
                    .any(|bot| bot["handle"] == handle && !bot["mail"].is_null())
            })
        })
        .unwrap_or(false)
}

/// How many entries the board says a run carries, largest first — read from the
/// resume offer, which is the only place a session's record is served.
fn entry_counts(board: &str) -> Vec<u64> {
    let parsed: serde_json::Value = serde_json::from_str(board).unwrap_or(serde_json::Value::Null);
    let mut counts: Vec<u64> = parsed["session"]["choices"]
        .as_array()
        .map(|choices| {
            choices
                .iter()
                .filter_map(|c| c["entry_count"].as_u64())
                .collect()
        })
        .unwrap_or_default();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    counts
}

/// The handles of the runs the board offers back.
fn offered(board: &str) -> Vec<String> {
    let parsed: serde_json::Value = serde_json::from_str(board).unwrap_or(serde_json::Value::Null);
    parsed["session"]["choices"]
        .as_array()
        .map(|choices| {
            choices
                .iter()
                .filter_map(|c| c["sid"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// **Phase 9 — the loop that has gone quiet.**
///
/// Three things the phase leaves, and the third is an absence that needs the
/// other two beside it: a loop nobody set a frequency for, a loop with one, and
/// nothing anywhere carrying the word the check-in's own key refuses.
///
/// **The loop with no cadence is the load-bearing one.** A build that demanded
/// every key its kind names would have dropped that write, and the room would
/// simply be missing it — which is indistinguishable from an agent that never
/// got there unless the other loop is checked in the same breath.
struct TheLoopIsOnTheRecord;

#[async_trait::async_trait]
impl Expectation for TheLoopIsOnTheRecord {
    fn name(&self) -> &str {
        "Phase 9 — both loops are on the record, cadence or no cadence"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let mut short = Vec::new();

        let bare = seen
            .room
            .call("recall", json!({"subject": "rhythm:smoke-descale"}))
            .await;
        if bare.contains("\"status\":\"blocked\"") {
            return missed(
                self.name(),
                format!("rhythm:smoke-descale was never created: {bare}"),
            );
        }
        if !bare.contains("last_check_in") {
            short.push("the loop with no cadence carries no last check-in either".to_string());
        }
        // The whole point of that loop: it was taken WITHOUT one.
        if bare.contains("cadence_days") {
            short.push(
                "the loop meant to have no cadence carries one, so nothing here says a loop \
                 without one is takeable"
                    .to_string(),
            );
        }

        let scheduled = seen
            .room
            .call("recall", json!({"subject": "rhythm:smoke-filter"}))
            .await;
        if !scheduled.contains("cadence_days") {
            short.push(
                "the loop with a cadence carries none, so the pair proves nothing".to_string(),
            );
        }
        // The check-in's key holds one of a named set. A record carrying the
        // word the phase deliberately tried is a refusal that did not happen.
        if scheduled.contains("swapped") {
            short.push(
                "a record carries 'swapped' under the check-in's key, so the closed set let it \
                 through"
                    .to_string(),
            );
        }
        if !scheduled.contains("outcome") {
            short.push("no check-in was recorded on the scheduled loop".to_string());
        }

        if short.is_empty() {
            held(
                self.name(),
                "the loop nobody set a frequency for and the loop with one are both there, and \
                 nothing carries a word the check-in's key does not name",
            )
        } else {
            missed(self.name(), short.join("; "))
        }
    }
}

/// **Phase 10 — a vocabulary of your own, and it DESCRIBES.**
///
/// The declaration is proved by USING it: a read that selects by
/// `smoke-errand` can only answer if the type was declared. That is one read
/// for two claims, and neither can pass without the other.
///
/// **The second half is the surprise the phase exists for.** A caller's type
/// turns no write away — only a kind's declaration gates — so the value its set
/// does not name is KEPT, and the read is what flags it. So this requires the
/// bad value to be present AND reported as mistyped: a room where it is missing
/// is a room where something refused a write that should have landed, and a
/// room where it is present unflagged is a read that lost the mistake.
struct TheVocabularyGovernsAThing;

#[async_trait::async_trait]
impl Expectation for TheVocabularyGovernsAThing {
    fn name(&self) -> &str {
        "Phase 10 — a caller's own type is declared and something fits it"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        // **Selected BY the type**, which is the read that cannot answer unless
        // the declaration is there. The tolerant selector is the one a caller
        // gets by default, and it reports what each thing lacks — so this asks
        // it and then reads the gaps, rather than asking a stricter question
        // and losing the reason for a miss.
        let answering = seen
            .room
            .call(
                "recall",
                json!({"kind": "thing", "answers_type": "smoke-errand"}),
            )
            .await;
        if answering.contains("\"status\":\"blocked\"") {
            return missed(
                self.name(),
                format!("the type could not be selected by, so it was never declared: {answering}"),
            );
        }
        // **The answer's OWN count, read rather than grepped.** A body carries
        // more than one `count` — an object's entitlement block has its own —
        // so a substring reads whichever came first and calls the room empty
        // while the thing is sitting in it.
        let counted = serde_json::from_str::<serde_json::Value>(&answering)
            .ok()
            .and_then(|body| body["count"].as_u64());
        if counted == Some(0) {
            return missed(
                self.name(),
                "the type is declarable but nothing in the room answers it, so no thing was \
                 written under the vocabulary"
                    .to_string(),
            );
        }
        // **The write a caller's type does not turn away.** The phase writes a
        // value outside the set on purpose, so the read must carry it and must
        // say it is wrong. Both, because either one alone is a different build:
        // no flag is a read that lost the mistake, and no value is a gate
        // nobody asked for.
        if !answering.contains("\"mistyped\"") || answering.contains("\"mistyped\":[]") {
            return missed(
                self.name(),
                format!(
                    "nothing on the thing is reported as mistyped, so either the value the set \
                     does not name never landed — which would mean a caller's type gated a \
                     write — or the read lost it: {answering}"
                ),
            );
        }
        held(
            self.name(),
            "a caller's own type is declared, something answers it, and the value its set does \
             not name is kept and flagged rather than refused",
        )
    }
}

/// **Phase 11 — the day it is where you are.**
///
/// Two undated claims on **this phase's own subject**, written minutes apart by
/// one run, coming back stamped with two different days.
///
/// Its own subject on purpose: an earlier phase's check counts the claims on
/// the person IT wrote to, and a later phase adding two more would break a
/// correct check. A phase that leaves its evidence on somebody else's subject
/// is a phase that measures its neighbour. **That can only happen if the frame
/// came from the caller**: a server with a zone of its own stamps both the
/// same, and so does a build that ignores the argument.
///
/// It counts DISTINCT dates rather than naming either, because which two days
/// they are depends on when the run happened, and a check that named them would
/// have to be rewritten every time the calendar moved.
struct TheFrameStampedTwoDays;

#[async_trait::async_trait]
impl Expectation for TheFrameStampedTwoDays {
    fn name(&self) -> &str {
        "Phase 11 — two undated claims came back on two different days"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let recalled = seen
            .room
            .call(
                "recall",
                json!({"subject": "person:smoke-beta", "facts": true}),
            )
            .await;
        let Ok(body) = serde_json::from_str::<serde_json::Value>(&recalled) else {
            return missed(
                self.name(),
                format!("the subject could not be read back: {recalled}"),
            );
        };
        let mut days: Vec<String> = Vec::new();
        collect_dates(&body, &mut days);
        days.sort();
        days.dedup();
        if days.len() < 2 {
            return missed(
                self.name(),
                format!(
                    "the claims on person:smoke-beta carry {} distinct day(s) — {:?} — so \
                     nothing here says the frame came from the caller",
                    days.len(),
                    days,
                ),
            );
        }
        held(
            self.name(),
            format!(
                "one run stamped undated claims with {} different days: {days:?}",
                days.len()
            ),
        )
    }
}

/// Every `date` a read answered with, wherever it sits in the shape.
///
/// Walked rather than indexed: the answer nests, and a check reaching for one
/// path would go quiet the day the shape gained a level — which reads as a pass.
fn collect_dates(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, held) in fields {
                if key == "date"
                    && let Some(day) = held.as_str()
                {
                    into.push(day.to_string());
                }
                collect_dates(held, into);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_dates(item, into);
            }
        }
        _ => {}
    }
}

/// **Phase 6 — the session's own record.**
///
/// A run has to have accrued a chronology by the end of this phase. It is read
/// from the resume offer, because that is the only place a session's record is
/// served — there is no verb that reads one by id, and after a later phase
/// wraps it the run stops being offered at all. So the claim is taken at the
/// boundary while the run is still open, which is also how a cold session
/// would meet it.
///
/// **The phase is measured in two halves by two parties, and the split is
/// deliberate rather than something nobody got round to.** This check takes
/// the STRUCTURAL half — a run exists, it grew here, it is still offered — all
/// of which the offer carries.
///
/// The SEMANTIC half — that the amendment landed on the newest entry and left
/// the first one alone — is the MODEL's to report, because the model is the
/// party that resumes its own session and reads the chronology back. The
/// harness must not resume one to look: being offered a run and choosing it is
/// exactly what a later phase watches the agent do, so a check that took the
/// run would consume the thing under observation to buy itself an assertion.
struct TheSessionKeptItsOwnRecord;

#[async_trait::async_trait]
impl Expectation for TheSessionKeptItsOwnRecord {
    fn name(&self) -> &str {
        "Phase 6 — the run accrued a chronology"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some((before, after)) = seen.across("Phase 6") else {
            return missed(
                self.name(),
                "the run recorded no boundary for this phase, so nothing was compared",
            );
        };
        let (was, now) = (entry_counts(&before.board), entry_counts(&after.board));
        let most = now.first().copied().unwrap_or(0);
        if most < 2 {
            return missed(
                self.name(),
                format!(
                    "the fullest run the board offers carries {most} entries, and the phase \
                     journals then amends: {now:?}"
                ),
            );
        }
        // The positive that makes the count mean something: it GREW here. A run
        // that already carried entries before the phase would satisfy the
        // threshold without this phase having written anything.
        if most <= was.first().copied().unwrap_or(0) {
            return missed(
                self.name(),
                format!("no run grew across this phase: {was:?} then {now:?}"),
            );
        }
        held(
            self.name(),
            format!("a run carries {most} entries, up from {was:?} before the phase"),
        )
    }
}

/// **Phase 12 — stopping without finishing.**
///
/// The phase deliberately does NOT wrap, because a later phase has to be
/// offered the open run. So what must be true is that the run is still there to
/// be offered, and that it moved.
struct TheRunWasLeftOpen;

#[async_trait::async_trait]
impl Expectation for TheRunWasLeftOpen {
    fn name(&self) -> &str {
        "Phase 12 — the run was left open for whoever came next"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some((before, after)) = seen.across("Phase 12") else {
            return missed(
                self.name(),
                "the run recorded no boundary for this phase, so nothing was compared",
            );
        };
        let still = offered(&after.board);
        if still.is_empty() {
            return missed(
                self.name(),
                "no run is offered back after this phase, so nothing was left for a cold session",
            );
        }
        let (was, now) = (entry_counts(&before.board), entry_counts(&after.board));
        if now.first() <= was.first() {
            return missed(
                self.name(),
                format!("the run is open but recorded nothing here: {was:?} then {now:?}"),
            );
        }
        held(
            self.name(),
            format!("{} run(s) still offered, and the record grew", still.len()),
        )
    }
}

/// **Phase 13 — the reader who was not here.**
///
/// The phase the whole suite exists for: a session arrives cold, is told a task
/// rather than a tool, and has to find what an earlier one left. What that
/// leaves behind is a message taken and retired — the one part of the phase
/// with a visible external side effect, read back through the same board the
/// mail check reads.
///
/// **The positive it rests on is that a handoff was waiting.** "Nothing is
/// unhandled" is true of a board that never carried a message, so a phase whose
/// room held nothing for the reader reports that it tested nothing rather than
/// reporting held.
///
/// What it deliberately does NOT check is whether the model found it the right
/// way. That is the judgement the report carries and no assertion can hold it.
struct TheHandoffWasPickedUp;

#[async_trait::async_trait]
impl Expectation for TheHandoffWasPickedUp {
    fn name(&self) -> &str {
        "Phase 13 — what an earlier run left was picked up"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some((before, after)) = seen.across("Phase 13") else {
            return missed(
                self.name(),
                "the run recorded no boundary for this phase, so nothing was compared",
            );
        };
        // **One refusal, not three.** The phase fails three ways — nothing was
        // left for the reader, nobody took what was left, nobody recorded what
        // they did — and a separate guard per cause is a guard nothing can
        // observe: whichever
        // one is removed, the next refuses the same room for a different
        // reason, and a test reading only held-or-not cannot tell them apart.
        //
        // Carrying the cause structurally would mean a field on the outcome
        // every check in the crate shares, which is a larger change than this
        // one. So the three collapse into one condition, and the reason a
        // reader gets carries every count rather than a sentence chosen from
        // three.
        //
        // **"Nobody took it" is a claim about one message, not about a count.**
        // The box the cold session meets holds the handoff and the furniture,
        // both unfinished and alike in every count — so a session that retires
        // the furniture and never touches the handoff drops the unfinished
        // total by one, exactly as the session under test would. What says the
        // handoff was picked up is that the id of a message that was waiting
        // for it, and was not the furniture, comes back retired with a note.
        let waiting = handoffs_in(&before.mail);
        let retired = retired_with_a_note(&after.mail);
        let picked_up: Vec<&String> = waiting.iter().filter(|id| retired.contains(id)).collect();
        if waiting.is_empty() || picked_up.is_empty() {
            return missed(
                self.name(),
                format!(
                    "the phase leaves the message an earlier one left taken and retired with a \
                     note; this room shows {} such message(s) waiting before it, of which {} came \
                     back retired with a note ({} message(s) are retired with a note in all)",
                    waiting.len(),
                    picked_up.len(),
                    retired.len(),
                ),
            );
        }
        held(
            self.name(),
            format!(
                "{} of the {} message(s) left for the reader were taken and retired with a note",
                picked_up.len(),
                waiting.len(),
            ),
        )
    }
}

/// The ids of the messages a board reading leaves for a later session — every
/// one not yet finished, except the furniture the room was seeded with.
fn handoffs_in(reading: &str) -> Vec<String> {
    messages(reading)
        .iter()
        .filter(|hit| hit["state"] != "processed" && hit["subject"] != SEEDED_SUBJECT)
        .filter_map(|hit| hit["id"].as_str().map(str::to_string))
        .collect()
}

/// The ids of the retired messages carrying an account of what was done with
/// them.
fn retired_with_a_note(reading: &str) -> Vec<String> {
    messages(reading)
        .iter()
        .filter(|hit| hit["state"] == "processed" && hit["notes"].is_string())
        .filter_map(|hit| hit["id"].as_str().map(str::to_string))
        .collect()
}

/// The message hits in a board reading.
fn messages(reading: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(reading)
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

/// **Phase 14 — the ending, from cold.**
///
/// A wrapped run is terminal and is not offered back, so what is checkable is
/// that the run a cold session finished is gone from the offer.
///
/// **An absence, and it passes on a boot that returned nothing at all** — which
/// is the trap the suite's own table names. So the positive is in the same
/// check: runs ARE offered at the end, and the one that was wrapped is not
/// among them.
struct TheWrappedRunIsNotOfferedBack;

#[async_trait::async_trait]
impl Expectation for TheWrappedRunIsNotOfferedBack {
    fn name(&self) -> &str {
        "Phase 14 — the wrapped run is not offered back, and newer ones are"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some(open_before) = seen
            .boundaries
            .iter()
            .find(|b| b.before.starts_with("Phase 13"))
            .map(|b| offered(&b.board))
        else {
            return missed(
                self.name(),
                "the run recorded no boundary before the wrap, so there is no run to look for",
            );
        };
        if open_before.is_empty() {
            return missed(
                self.name(),
                "no run was open before the wrap, so its absence afterwards proves nothing",
            );
        }
        let ending = seen
            .boundaries
            .last()
            .map(|b| offered(&b.board))
            .unwrap_or_default();
        if ending.is_empty() {
            return missed(
                self.name(),
                "the board offers no run at all at the end, so the absence below reads nothing",
            );
        }
        let lingering: Vec<&String> = open_before
            .iter()
            .filter(|sid| ending.contains(sid))
            .collect();
        if !lingering.is_empty() {
            return missed(
                self.name(),
                format!("a run that should have been wrapped is still offered: {lingering:?}"),
            );
        }
        held(
            self.name(),
            format!(
                "the run open before the wrap is gone from the offer, and {} newer run(s) are \
                 there",
                ending.len()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A boarding snapshot carrying one bot, with or without a box on it.
    fn boarding(handle: &str, mail: serde_json::Value) -> String {
        json!({
            "snapshot": {
                "entities": {
                    "bots": [{"handle": handle, "yours": false, "mail": mail}],
                },
            },
        })
        .to_string()
    }

    /// **A bot's handle is in the snapshot whether or not it owns a box.**
    ///
    /// Every bot on the roster is listed by handle, and mail is a field beside
    /// it that is null when there is none — so looking for the handle in the
    /// answer measures that the bot was created, which a separate check already
    /// claims. Both directions in one case: the box that is there must read as
    /// there, or the check refuses a phase that did its job.
    #[test]
    fn a_bot_without_a_box_is_not_read_as_having_one() {
        assert!(
            !bot_has_mailbox(
                &boarding("bot:smoke-gamma", serde_json::Value::Null),
                "bot:smoke-gamma",
            ),
            "a bot with no box read as having one",
        );
        assert!(
            bot_has_mailbox(
                &boarding("bot:smoke-gamma", json!({"counts": {"new": 0}})),
                "bot:smoke-gamma",
            ),
            "a bot whose box is on the snapshot read as having none",
        );
    }
}
