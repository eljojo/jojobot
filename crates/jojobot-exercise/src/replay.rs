//! **A paid run's recorded calls, sent again to another build.**
//!
//! A paid run keeps the raw event stream of what the occupant did
//! ([`crate::calls`]). Those calls are a free regression guard: send them in
//! the same order through the served surface of a candidate build, and every
//! call whose class changed (accepted, refused with a fix, error) is a place
//! where the candidate answers a real model differently from the build the
//! stream was recorded on. No model runs and nothing is billed.
//!
//! **It reports and never fails.** An intended fix changes a class too, and it
//! is accepted by name in the notes of the deploy that ships it. What this
//! module owes the reader is the list, with the first divergence of each sitting
//! named and the calls that followed it counted apart.
//!
//! ⚠️ **A stream is never committed.** It describes the machine the run
//! happened on; see [`crate::calls`]. The tests below build their own from the
//! shapes a real stream has.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::surface::Surface;

/// What the CLI calls a jojobot verb: the prefix, then the verb as the surface
/// names it.
pub const VERB_PREFIX: &str = "mcp__jojobot-room__";

/// **One call the occupant made, whole.** Not the 160-character rendering
/// [`crate::calls::Call`] keeps for a reader: a replay sends the input again,
/// so it keeps all of it.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    /// The verb as the surface names it, without the CLI's prefix.
    pub verb: String,
    pub input: Value,
    /// The answer's text, or `None` for a call whose answer never arrived (the
    /// run was stopped mid-sitting).
    pub answer: Option<String>,
    /// The CLI's own flag that the call failed.
    pub is_error: bool,
}

/// One sitting's calls, in the order they were made.
#[derive(Debug, Clone, PartialEq)]
pub struct Sitting {
    pub name: String,
    pub calls: Vec<RecordedCall>,
}

/// **The jojobot calls of a raw stream, sitting by sitting.**
///
/// Reads events and nothing else. A call to anything but a jojobot verb is not
/// kept. A sitting with no calls is kept with none: silence and a capture that
/// failed must not read alike.
pub fn read_stream(stream: &str) -> Vec<Sitting> {
    // **Nothing past this line holds a secret a server issued.** A recording
    // is a file that outlives the run it came from, and a key or a wrap code in
    // it would still open a run if the instance it came from were still up.
    let stream = scrub_secrets(stream);
    let events: Vec<Value> = stream
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // **The answer arrives in a later event than the call**, so the answers are
    // gathered first and each call then finds its own by id.
    let mut answers: HashMap<String, (String, bool)> = HashMap::new();
    for event in &events {
        for block in blocks_of(event) {
            if block["type"] != "tool_result" {
                continue;
            }
            let Some(id) = block["tool_use_id"].as_str() else {
                continue;
            };
            // `is_error` is absent on a call that worked.
            let failed = block["is_error"].as_bool().unwrap_or(false);
            answers.insert(id.to_string(), (text_of(&block["content"]), failed));
        }
    }
    let mut sittings: Vec<Sitting> = Vec::new();
    for event in &events {
        if let Some(name) = event["jojobot_sitting"].as_str() {
            sittings.push(Sitting {
                name: name.to_string(),
                calls: Vec::new(),
            });
            continue;
        }
        for block in blocks_of(event) {
            if block["type"] != "tool_use" {
                continue;
            }
            let Some(verb) = block["name"]
                .as_str()
                .and_then(|name| name.strip_prefix(VERB_PREFIX))
            else {
                continue;
            };
            if sittings.is_empty() {
                sittings.push(Sitting {
                    name: "(before the first sitting)".to_string(),
                    calls: Vec::new(),
                });
            }
            let (answer, is_error) = match block["id"].as_str().and_then(|id| answers.get(id)) {
                Some((text, failed)) => (Some(text.clone()), *failed),
                None => (None, false),
            };
            if let Some(sitting) = sittings.last_mut() {
                sitting.calls.push(RecordedCall {
                    verb: verb.to_string(),
                    input: block["input"].clone(),
                    answer,
                    is_error,
                });
            }
        }
    }
    sittings
}

/// The characters a server draws its handles and secrets from.
const DRAWN: &str = "0123456789abcdefghjkmnpqrstvwxyz";

/// **The secrets of a stream, replaced by stand-ins that keep their shape.**
/// A run key is `sk-` and 26 drawn characters, a wrap code `wc-` and 12. Each
/// distinct secret becomes the same stand-in wherever it appears, and two
/// secrets never share one, so a replay that maps a recorded key to the key it
/// minted still pairs them. The stand-in is a count written in the drawn
/// characters, so it is shaped like a secret and opens nothing.
pub fn scrub_secrets(stream: &str) -> String {
    let mut stand_ins: HashMap<String, String> = HashMap::new();
    let mut out = String::with_capacity(stream.len());
    let mut at = 0;
    while at < stream.len() {
        let rest = &stream[at..];
        let secret = [("sk-", 26usize), ("wc-", 12usize)]
            .iter()
            .find_map(|(prefix, drawn)| {
                let body = rest.strip_prefix(prefix)?.get(..*drawn)?;
                body.chars()
                    .all(|c| DRAWN.contains(c))
                    .then(|| (*prefix, *drawn, &rest[..prefix.len() + drawn]))
            });
        match secret {
            Some((prefix, drawn, found)) => {
                let next = stand_ins.len();
                let stand_in = stand_ins
                    .entry(found.to_string())
                    .or_insert_with(|| format!("{prefix}{:0>width$}", next, width = drawn));
                out.push_str(stand_in);
                at += found.len();
            }
            None => {
                let c = rest.chars().next().expect("not at the end");
                out.push(c);
                at += c.len_utf8();
            }
        }
    }
    out
}

/// The content blocks of one event, which is where calls and answers live.
fn blocks_of(event: &Value) -> Vec<&Value> {
    event["message"]["content"]
        .as_array()
        .map(|blocks| blocks.iter().collect())
        .unwrap_or_default()
}

/// **The text of an answer's content**, whole: a string, or the text blocks of
/// a list joined.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// **What kind of answer a call got.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    /// The surface did what was asked.
    Accepted,
    /// The surface turned the call down, and said who fixes it.
    Refused(String),
    /// The surface broke, or answered something that is not an answer.
    Error,
    /// The recording holds no answer for the call, so there is nothing to
    /// compare it with.
    Unanswered,
}

/// **The class of one answer body**, the same way for the recording and for the
/// replay. `is_error` is the CLI's flag on a recorded answer; a replayed answer
/// has none.
pub fn classify(body: Option<&str>, is_error: bool) -> Class {
    let Some(body) = body else {
        return Class::Unanswered;
    };
    if is_error {
        return Class::Error;
    }
    let Ok(answer) = serde_json::from_str::<Value>(body) else {
        return Class::Error;
    };
    if answer.get("transport_error").is_some_and(|e| !e.is_null()) {
        return Class::Error;
    }
    // **Only the top-level status is a refusal.** A fact inside an answer has a
    // status of its own, and it is an answer.
    if answer["status"] == "blocked" {
        return Class::Refused(answer["fix_by"].as_str().unwrap_or("none").to_string());
    }
    Class::Accepted
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Class::Accepted => write!(f, "accepted"),
            Class::Refused(fix_by) => write!(f, "refused (fix_by {fix_by})"),
            Class::Error => write!(f, "error"),
            Class::Unanswered => write!(f, "unanswered"),
        }
    }
}

/// What a run-minted value is, so it is mapped only to a value of its own kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    Sid,
    Address,
    Message,
}

/// **Where an answer holds a value the server minted** and a later call may
/// send back: the verb whose answer it is (any verb when `None`) and the path
/// to it, keys joined by `.` and an array marked by `[]` after its key.
#[derive(Debug, Clone, Copy)]
pub struct Mint {
    pub verb: Option<&'static str>,
    pub path: &'static str,
    pub kind: IdKind,
}

/// **The one table of mint points.** A verb that starts to mint something a
/// later call sends back is one line here. [`unmapped_ids`] says when the table
/// has fallen behind a recording.
pub const MINTS: &[Mint] = &[
    Mint {
        verb: Some("start_here"),
        path: "session.sid",
        kind: IdKind::Sid,
    },
    Mint {
        verb: Some("start_here"),
        path: "session.choices[].sid",
        kind: IdKind::Sid,
    },
    // **A run's public name**, which a later boot sends back as `resume`.
    Mint {
        verb: Some("start_here"),
        path: "session.run",
        kind: IdKind::Sid,
    },
    Mint {
        verb: Some("start_here"),
        path: "session.choices[].run",
        kind: IdKind::Sid,
    },
    Mint {
        verb: None,
        path: "address",
        kind: IdKind::Address,
    },
    Mint {
        verb: Some("read_mailbox"),
        path: "messages[].id",
        kind: IdKind::Message,
    },
    Mint {
        verb: Some("post_message"),
        path: "id",
        kind: IdKind::Message,
    },
];

/// The string leaves of `value` at `path`, in document order.
pub fn leaves_at(value: &Value, path: &str) -> Vec<String> {
    let segments: Vec<&str> = path.split('.').collect();
    let mut found = Vec::new();
    collect(value, &segments, &mut found);
    found
}

fn collect(value: &Value, segments: &[&str], found: &mut Vec<String>) {
    let Some((first, rest)) = segments.split_first() else {
        if let Some(leaf) = value.as_str() {
            found.push(leaf.to_string());
        }
        return;
    };
    let (key, many) = match first.strip_suffix("[]") {
        Some(key) => (key, true),
        None => (*first, false),
    };
    let Some(child) = value.get(key) else {
        return;
    };
    match (many, child.as_array()) {
        (true, Some(items)) => {
            for item in items {
                collect(item, rest, found);
            }
        }
        (true, None) => {}
        (false, _) => collect(child, rest, found),
    }
}

/// **Recorded run-minted values and what the replay minted in their place.**
#[derive(Debug, Default)]
pub struct IdMap {
    mapped: HashMap<String, String>,
    unmapped: HashSet<String>,
}

impl IdMap {
    /// Pair the mint points of a recorded answer with those of the replayed
    /// one, by position. A recorded value the replay did not mint is remembered
    /// as unmapped.
    pub fn learn(&mut self, verb: &str, recorded: &Value, replayed: Option<&Value>) {
        for mint in MINTS.iter().filter(|m| m.verb.is_none_or(|v| v == verb)) {
            let before = leaves_at(recorded, mint.path);
            let after = replayed
                .map(|answer| leaves_at(answer, mint.path))
                .unwrap_or_default();
            for (at, token) in before.into_iter().enumerate() {
                match after.get(at) {
                    Some(minted) => {
                        self.unmapped.remove(&token);
                        self.mapped.insert(token, minted.clone());
                    }
                    None => {
                        if !self.mapped.contains_key(&token) {
                            self.unmapped.insert(token);
                        }
                    }
                }
            }
        }
    }

    /// **A recorded input with every mapped value replaced**, and how many
    /// values it still carried that the replay never minted. A value is
    /// replaced only when a whole string leaf equals it.
    pub fn rewrite(&self, input: &Value) -> (Value, usize) {
        let mut orphans = 0;
        let rewritten = self.rewritten(input, &mut orphans);
        (rewritten, orphans)
    }

    fn rewritten(&self, value: &Value, orphans: &mut usize) -> Value {
        match value {
            Value::String(leaf) => match self.mapped.get(leaf) {
                Some(minted) => Value::String(minted.clone()),
                None => {
                    if self.unmapped.contains(leaf) {
                        *orphans += 1;
                    }
                    value.clone()
                }
            },
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.rewritten(v, orphans)).collect())
            }
            Value::Object(entries) => Value::Object(
                entries
                    .iter()
                    .map(|(key, v)| (key.clone(), self.rewritten(v, orphans)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// **Argument values that came out of an earlier answer, look minted, and are
/// not in [`MINTS`]**, as `verb.path` of the answer that held them and how many
/// times a call sent one back. A table that has fallen behind a recording shows
/// here instead of as a replay that diverges for no visible reason.
pub fn unmapped_ids(sittings: &[Sitting]) -> Vec<String> {
    let mut seen: HashMap<String, String> = HashMap::new();
    // **Values the table already learns at their mint point.** A read that shows
    // one again is not a second source of it.
    let mut minted: HashSet<String> = HashSet::new();
    let mut found: std::collections::BTreeMap<String, usize> = Default::default();
    for call in sittings.iter().flat_map(|sitting| &sitting.calls) {
        for (_, leaf) in leaf_paths(&call.input) {
            if looks_minted(&leaf)
                && let Some(origin) = seen.get(&leaf)
            {
                *found.entry(origin.clone()).or_default() += 1;
            }
        }
        let Some(answer) = call
            .answer
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
        else {
            continue;
        };
        for (path, leaf) in leaf_paths(&answer) {
            if is_mint_point(&call.verb, &path) {
                seen.remove(&leaf);
                minted.insert(leaf);
                continue;
            }
            if minted.contains(&leaf) || !(3..=80).contains(&leaf.len()) {
                continue;
            }
            seen.entry(leaf)
                .or_insert_with(|| format!("{}.{}", call.verb, path));
        }
    }
    found
        .into_iter()
        .map(|(origin, uses)| match uses {
            1 => format!("{origin} (1 use)"),
            n => format!("{origin} ({n} uses)"),
        })
        .collect()
}

/// Whether the table names this path of this verb's answer.
fn is_mint_point(verb: &str, path: &str) -> bool {
    MINTS
        .iter()
        .any(|m| m.path == path && m.verb.is_none_or(|v| v == verb))
}

/// **A value shaped like something a server draws**: a record address, or a
/// short run of lowercase letters and digits with a digit in it. Plain words,
/// dates and handles are not.
fn looks_minted(leaf: &str) -> bool {
    leaf.contains('#')
        || is_secret_shaped(leaf)
        || ((4..=8).contains(&leaf.len())
            && leaf
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && leaf.chars().any(|c| c.is_ascii_digit()))
}

/// Whether a leaf has the shape of a run key (`sk-` and 26 drawn characters) or
/// of a wrap code (`wc-` and 12). The same two shapes [`scrub_secrets`] hides.
fn is_secret_shaped(leaf: &str) -> bool {
    [("sk-", 26usize), ("wc-", 12usize)]
        .iter()
        .any(|(prefix, drawn)| {
            leaf.strip_prefix(prefix)
                .is_some_and(|body| body.len() == *drawn && body.chars().all(|c| DRAWN.contains(c)))
        })
}

/// Every string leaf of `value` with its path, arrays marked `[]` after their
/// key as in [`MINTS`].
fn leaf_paths(value: &Value) -> Vec<(String, String)> {
    fn walk(value: &Value, path: String, out: &mut Vec<(String, String)>) {
        match value {
            Value::String(leaf) => out.push((path, leaf.clone())),
            Value::Object(entries) => {
                for (key, child) in entries {
                    let next = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    match child {
                        Value::Array(items) => {
                            for item in items {
                                walk(item, format!("{next}[]"), out);
                            }
                        }
                        other => walk(other, next, out),
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, format!("{path}[]"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, String::new(), &mut out);
    out
}

/// **Something a replay can send a call through.** The served surface is one;
/// a case can supply another.
pub trait Door {
    fn call(&self, verb: &str, args: Value) -> impl std::future::Future<Output = String> + Send;
}

impl Door for Surface {
    async fn call(&self, verb: &str, args: Value) -> String {
        Surface::call(self, verb, args).await
    }
}

/// One replayed call beside its recording.
#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    /// Position in the sitting, from zero.
    pub index: usize,
    pub verb: String,
    pub recorded: Class,
    pub replayed: Class,
    /// The start of each answer, for a reader.
    pub recorded_head: String,
    pub replayed_head: String,
    /// How many values the input carried that the replay never minted.
    pub orphan_uses: usize,
    /// Whether the room broke under the call (a transport error), as opposed to
    /// answering it.
    pub broke: bool,
}

impl CallResult {
    /// Whether the replay answered in another class than the recording.
    pub fn differs(&self) -> bool {
        self.recorded != Class::Unanswered && self.recorded != self.replayed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SittingResult {
    pub name: String,
    pub calls: Vec<CallResult>,
}

/// **Send every recorded call again, in order, through `door`.**
pub async fn replay<D: Door>(sittings: &[Sitting], door: &D) -> Vec<SittingResult> {
    // **One map for the whole stream**: a sitting that resumes a run an earlier
    // sitting started sends that run's recorded handle, and the replay minted
    // its own for it earlier.
    let mut ids = IdMap::default();
    let mut results = Vec::new();
    for sitting in sittings {
        let mut calls = Vec::new();
        for (index, call) in sitting.calls.iter().enumerate() {
            let (input, orphan_uses) = ids.rewrite(&call.input);
            let body = door.call(&call.verb, input).await;
            let recorded_answer = call
                .answer
                .as_deref()
                .and_then(|text| serde_json::from_str::<Value>(text).ok());
            let replayed_answer = serde_json::from_str::<Value>(&body).ok();
            if let Some(recorded) = &recorded_answer {
                ids.learn(&call.verb, recorded, replayed_answer.as_ref());
            }
            let recorded_class = classify(call.answer.as_deref(), call.is_error);
            calls.push(CallResult {
                index,
                verb: call.verb.clone(),
                recorded: recorded_class.clone(),
                replayed: classify(Some(&body), false),
                recorded_head: head_of(
                    call.answer
                        .as_deref()
                        .unwrap_or("[no answer in the stream]"),
                ),
                replayed_head: head_of(&body),
                orphan_uses,
                // **Only a call the recording got an answer to can show a broken
                // room**: the surface hands an error the server returned back the
                // same way it hands back a lost connection, so a recorded error
                // that comes again is the server doing the same thing twice.
                broke: recorded_class != Class::Error
                    && replayed_answer
                        .as_ref()
                        .is_some_and(|a| a.get("transport_error").is_some_and(|e| !e.is_null())),
            });
        }
        results.push(SittingResult {
            name: sitting.name.clone(),
            calls,
        });
    }
    results
}

/// How much of an answer a report quotes.
const HEAD: usize = 160;

fn head_of(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(HEAD) {
        Some((at, _)) => format!("{}…", &flat[..at]),
        None => flat,
    }
}

/// What a sitting came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub name: String,
    pub calls: usize,
    pub differing: usize,
    /// The first call whose class differs.
    pub first: Option<CallResult>,
    /// Differing calls after the first.
    pub after_first: usize,
    /// Of those, the ones whose input carried a value the replay never minted:
    /// certainly the first divergence's cascade.
    pub after_first_orphan: usize,
}

pub fn summarise(result: &SittingResult) -> Summary {
    let differing: Vec<&CallResult> = result.calls.iter().filter(|c| c.differs()).collect();
    let later = differing.iter().skip(1);
    Summary {
        name: result.name.clone(),
        calls: result.calls.len(),
        differing: differing.len(),
        first: differing.first().map(|c| (*c).clone()),
        after_first: later.clone().count(),
        after_first_orphan: later.filter(|c| c.orphan_uses > 0).count(),
    }
}

/// **What the classes cannot see**, said at the top of every report.
pub const SCOPE: &str = "This compares classes only: accepted, refused (with its fix_by) and error. \
     It cannot see a read that found something and now finds nothing, because both are accepted.";

/// **A way a replay fell short of the whole recording.** A report that has one
/// prints as INCOMPLETE: zero changes over a recording the replay did not fully
/// send or compare is not a clean result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gap {
    /// Lines of the stream that are not events this reader can parse.
    UnreadLines(usize),
    /// Recorded calls with no answer in the stream, so nothing to compare.
    Unanswered(usize),
    /// The room broke under calls: these sittings, this many calls.
    RoomBroke { sittings: Vec<String>, calls: usize },
    /// Values that look minted, flow into later calls, and are not in the
    /// table, so they were sent as recorded.
    Unmapped(Vec<String>),
}

/// **Every way this replay fell short**, in a fixed order.
pub fn gaps(stream: &str, sittings: &[Sitting], results: &[SittingResult]) -> Vec<Gap> {
    let mut found = Vec::new();
    let unread = stream
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| serde_json::from_str::<Value>(line).is_err())
        .count();
    if unread > 0 {
        found.push(Gap::UnreadLines(unread));
    }
    let unanswered = sittings
        .iter()
        .flat_map(|sitting| &sitting.calls)
        .filter(|call| call.answer.is_none())
        .count();
    if unanswered > 0 {
        found.push(Gap::Unanswered(unanswered));
    }
    let broken: Vec<(&str, usize)> = results
        .iter()
        .map(|sitting| {
            (
                sitting.name.as_str(),
                sitting.calls.iter().filter(|call| call.broke).count(),
            )
        })
        .filter(|(_, broke)| *broke > 0)
        .collect();
    if !broken.is_empty() {
        found.push(Gap::RoomBroke {
            sittings: broken.iter().map(|(name, _)| name.to_string()).collect(),
            calls: broken.iter().map(|(_, broke)| broke).sum(),
        });
    }
    let unmapped = unmapped_ids(sittings);
    if !unmapped.is_empty() {
        found.push(Gap::Unmapped(unmapped));
    }
    found
}

impl std::fmt::Display for Gap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Gap::UnreadLines(n) => write!(f, "{n} line(s) of the stream could not be read"),
            Gap::Unanswered(n) => write!(
                f,
                "{n} recorded call(s) have no answer in the stream, so they were sent and not compared"
            ),
            Gap::RoomBroke { sittings, calls } => write!(
                f,
                "the room broke under {calls} call(s), in: {}",
                sittings.join("; ")
            ),
            Gap::Unmapped(values) => write!(
                f,
                "values that look minted were sent as recorded, not mapped (add the mint point if a divergence follows from them): {}",
                values.join(", ")
            ),
        }
    }
}

/// **Where a replay's day was read from.** Both are the run's own log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaySource {
    /// The run's own server announced it, in `clock.stated_day` of an answer it
    /// gave. Every run made on a stated day carries it.
    Announced,
    /// The stamp in the log's file name, `<room>-<stamp>.jsonl`, read as the
    /// UTC day it falls on. A run made on the wall clock announces nothing, and
    /// this is the day it was made.
    FileName,
}

/// **The day a replay's server acts out**, and where it came from.
///
/// A replay reproduces the run it replays, so its server acts out THAT run's
/// day. A server on the wall clock would stamp the replay with the day it is
/// made, and an answer that carries a stamp could meet a recorded value on one
/// real day and not on the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDay {
    pub day: String,
    pub source: DaySource,
}

impl std::fmt::Display for ReplayDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let from = match self.source {
            DaySource::Announced => "announced by the run's own server",
            DaySource::FileName => "the stamp in the run's file name, read as UTC",
        };
        write!(f, "{} ({from})", self.day)
    }
}

/// **The day a recorded run's log records**, or a refusal naming the run.
///
/// In order: the day the run's own server announced in an answer, else the day
/// of the stamp in the file's name. A log that records neither is refused, and
/// so is one that announces two different days: nothing is guessed, because a
/// replay on a guessed day can pass or fail on the calendar.
pub fn day_of_run(stream: &str, file: &Path) -> anyhow::Result<ReplayDay> {
    let mut announced: Vec<String> = Vec::new();
    for sitting in read_stream(stream) {
        for call in sitting.calls {
            let Some(answer) = call.answer else { continue };
            let Ok(body) = serde_json::from_str::<Value>(&answer) else {
                continue;
            };
            if let Some(day) = body["clock"]["stated_day"].as_str()
                && !announced.iter().any(|seen| seen == day)
            {
                announced.push(day.to_string());
            }
        }
    }
    let name = file.display();
    match announced.as_slice() {
        [day] => {
            return Ok(ReplayDay {
                day: day.clone(),
                source: DaySource::Announced,
            });
        }
        [] => {}
        many => anyhow::bail!(
            "{name} announces more than one day ({}), so there is no one day to replay it on",
            many.join(", "),
        ),
    }
    match stamped(file) {
        Some((_, seconds)) => Ok(ReplayDay {
            day: utc_day_of(seconds),
            source: DaySource::FileName,
        }),
        None => anyhow::bail!(
            "{name} records no day: its server announced none, and its file name carries no \
             <room>-<stamp>.jsonl stamp. A replay on the wall clock could pass or fail on the \
             calendar, so none is made.",
        ),
    }
}

/// The UTC day a count of seconds since 1970 falls on, as `YYYY-MM-DD`.
///
/// The civil-from-days arithmetic, so this crate takes no date dependency for
/// one conversion.
fn utc_day_of(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// **A room opened on the day a replayed run was made.**
///
/// The one place a replay opens its server: the binary and the cases that watch
/// it both come here, so the day cannot be stated in one and forgotten in the
/// other.
pub async fn open_for_run(
    binary: &Path,
    stream: &str,
    file: &Path,
) -> anyhow::Result<(crate::room::Room, Surface, ReplayDay)> {
    let day = day_of_run(stream, file)?;
    let (room, surface) = crate::room::Room::open_with_client_stating(binary, &day.day).await?;
    Ok((room, surface, day))
}

/// **The report a reader gets.**
pub fn render(
    room: &str,
    stream: &str,
    candidate: &str,
    day: &ReplayDay,
    gaps: &[Gap],
    results: &[SittingResult],
) -> String {
    let summaries: Vec<Summary> = results.iter().map(summarise).collect();
    let calls: usize = summaries.iter().map(|s| s.calls).sum();
    let differing: usize = summaries.iter().map(|s| s.differing).sum();
    let mut out = format!(
        "replay of {stream} (room {room}) on {candidate}\n\
         server acted out {day}\n\
         {} sittings, {calls} calls, {differing} changed class\n",
        summaries.len()
    );
    out.push_str(SCOPE);
    out.push('\n');
    // **A replay that fell short is not a clean one**, so the shortfall comes
    // before any per-sitting line a reader might stop at.
    if !gaps.is_empty() {
        out.push_str(
            "INCOMPLETE REPLAY: zero changes below does not mean the recording was fully sent and compared.\n",
        );
        for gap in gaps {
            out.push_str(&format!("  - {gap}\n"));
        }
    }
    for (at, summary) in summaries.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. {}: {} calls, {} changed\n",
            at + 1,
            summary.name,
            summary.calls,
            summary.differing
        ));
        let Some(first) = &summary.first else {
            continue;
        };
        out.push_str(&format!(
            "   first divergence: call {} ({}): recorded {}, replayed {}\n      recorded: {}\n      replayed: {}\n",
            first.index + 1,
            first.verb,
            first.recorded,
            first.replayed,
            first.recorded_head,
            first.replayed_head
        ));
        if summary.after_first > 0 {
            out.push_str(&format!(
                "   after it: {} more changed, {} of them carrying a value the replay never minted, {} without\n",
                summary.after_first,
                summary.after_first_orphan,
                summary.after_first - summary.after_first_orphan
            ));
        }
    }
    out
}

/// **Where the replay keeps its baselines and its reports**, under the user's
/// state directory and never under the repository: a stream and a report both
/// quote answer bodies from a run. `xdg` is `XDG_STATE_HOME` and `home` is
/// `HOME`; a value that is unset or empty is not a place.
pub fn state_root_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match (
        xdg.filter(|s| !s.is_empty()),
        home.filter(|s| !s.is_empty()),
    ) {
        (Some(xdg), _) => PathBuf::from(xdg),
        (None, Some(home)) => PathBuf::from(home).join(".local").join("state"),
        (None, None) => return None,
    };
    Some(base.join("jojobot-replay"))
}

/// The room a stream belongs to, read off its name: `<room>-<stamp>.jsonl`.
pub fn room_of(stream: &Path) -> Option<String> {
    stamped(stream).map(|(room, _)| room)
}

/// `<room>-<stamp>` read off a stream's file name.
fn stamped(stream: &Path) -> Option<(String, u64)> {
    let stem = stream.file_stem()?.to_str()?;
    let (room, stamp) = stem.rsplit_once('-')?;
    Some((room.to_string(), stamp.parse().ok()?))
}

/// **Every stream in `dir` with its room and stamp.** A `.jsonl` that is not
/// named as a stream is refused by name: a skipped one would read as a run that
/// never happened.
fn streams_in(dir: &Path) -> anyhow::Result<Vec<(String, u64, PathBuf)>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let (room, stamp) = stamped(&path).ok_or_else(|| {
            anyhow::anyhow!("{} is not named <room>-<stamp>.jsonl", path.display())
        })?;
        found.push((room, stamp, path));
    }
    Ok(found)
}

/// The newest stream in `dir` for one room, by the stamp in its name.
pub fn newest_of(dir: &Path, room: &str) -> anyhow::Result<Option<PathBuf>> {
    Ok(streams_in(dir)?
        .into_iter()
        .filter(|(name, _, _)| name == room)
        .max_by_key(|(_, stamp, _)| *stamp)
        .map(|(_, _, path)| path))
}

/// The newest stream of each room present in `dir`.
pub fn newest_per_room(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut newest: std::collections::BTreeMap<String, (u64, PathBuf)> = Default::default();
    for (room, stamp, path) in streams_in(dir)? {
        let held = newest.entry(room).or_insert((stamp, path.clone()));
        if stamp > held.0 {
            *held = (stamp, path);
        }
    }
    Ok(newest.into_values().map(|(_, path)| path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── the shapes a real stream has ────────────────────────────────────────
    //
    // Read off the stream a paid run kept: an assistant event holds the call, a
    // user event holds its answer in a later line and names the call by id, and
    // the answer's content is a string for a jojobot verb and a list of text
    // blocks for another tool. `is_error` is there only on a failed call.

    fn boundary(name: &str) -> String {
        json!({ "jojobot_sitting": name }).to_string()
    }

    fn called(id: &str, verb: &str, input: Value) -> String {
        json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": id, "name": format!("{VERB_PREFIX}{verb}"), "input": input}
        ]}})
        .to_string()
    }

    fn called_other(id: &str, name: &str, input: Value) -> String {
        json!({"type": "assistant", "message": {"content": [
            {"type": "tool_use", "id": id, "name": name, "input": input}
        ]}})
        .to_string()
    }

    fn answered(id: &str, body: &str) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "content": body}
        ]}})
        .to_string()
    }

    fn answered_in_blocks(id: &str, body: &str) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "content": [{"type": "text", "text": body}]}
        ]}})
        .to_string()
    }

    fn failed(id: &str, body: &str) -> String {
        json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": id, "content": body, "is_error": true}
        ]}})
        .to_string()
    }

    fn stream_of(lines: &[String]) -> String {
        lines.join("\n")
    }

    // ── the reader ──────────────────────────────────────────────────────────

    /// A call keeps its whole input however long, finds its answer in a later
    /// line, and a call to another tool is not a jojobot call.
    #[test]
    fn the_reader_keeps_each_call_whole_with_its_answer_in_sitting_order() {
        let long = "x".repeat(400);
        let stream = stream_of(&[
            boundary("first"),
            called(
                "t1",
                "capture",
                json!({"subject": "thing:handcart", "content": long}),
            ),
            called_other("t2", "ToolSearch", json!({"query": "select:capture"})),
            answered("t1", r#"{"address":"thing:handcart#f1"}"#),
            answered_in_blocks("t2", "tools"),
            boundary("second"),
            called("t3", "recall", json!({"subject": "thing:handcart"})),
            answered("t3", r#"{"objects":[]}"#),
        ]);

        let read = read_stream(&stream);

        assert_eq!(read.len(), 2);
        assert_eq!(read[0].name, "first");
        assert_eq!(
            read[0].calls.len(),
            1,
            "the other tool is not a jojobot call"
        );
        assert_eq!(read[0].calls[0].verb, "capture");
        assert_eq!(read[0].calls[0].input["content"], json!(long));
        assert_eq!(
            read[0].calls[0].answer.as_deref(),
            Some(r#"{"address":"thing:handcart#f1"}"#)
        );
        assert_eq!(read[1].name, "second");
        assert_eq!(read[1].calls[0].verb, "recall");
    }

    /// A sitting that made no call is still a sitting, and a call whose answer
    /// never arrived says so rather than getting an invented one.
    #[test]
    fn the_reader_keeps_a_silent_sitting_and_a_call_nobody_answered() {
        let stream = stream_of(&[
            boundary("quiet"),
            boundary("cut off"),
            called("t1", "search", json!({"query": "kite"})),
        ]);

        let read = read_stream(&stream);

        assert_eq!(read.len(), 2);
        assert!(read[0].calls.is_empty());
        assert_eq!(read[1].calls.len(), 1);
        assert_eq!(read[1].calls[0].answer, None);
    }

    /// The CLI's failure flag rides on the call, and an answer in blocks is read
    /// like an answer in a string.
    #[test]
    fn the_reader_carries_the_failure_flag_and_reads_an_answer_in_blocks() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "recall", json!({"kind": "type"})),
            failed("t1", "kind 'type': no kind is named 'type'"),
            called("t2", "search", json!({"query": "kite"})),
            answered_in_blocks("t2", r#"{"results":[]}"#),
        ]);

        let read = read_stream(&stream);

        assert!(read[0].calls[0].is_error);
        assert!(!read[0].calls[1].is_error);
        assert_eq!(
            read[0].calls[1].answer.as_deref(),
            Some(r#"{"results":[]}"#)
        );
    }

    // ── the classifier ──────────────────────────────────────────────────────

    #[test]
    fn the_classifier_tells_an_answer_a_refusal_and_an_error_apart() {
        assert_eq!(classify(Some(r#"{"count":0}"#), false), Class::Accepted);
        assert_eq!(
            classify(Some(r#"{"status":"blocked","fix_by":"change"}"#), false),
            Class::Refused("change".into())
        );
        assert_eq!(
            classify(Some(r#"{"status":"blocked","fix_by":"person"}"#), false),
            Class::Refused("person".into()),
            "the word that says who fixes it is part of the class"
        );
        assert_eq!(
            classify(Some(r#"{"status":"blocked"}"#), false),
            Class::Refused("none".into())
        );
        assert_eq!(classify(Some("not json at all"), false), Class::Error);
        assert_eq!(
            classify(Some(r#"{"transport_error":"connection reset"}"#), false),
            Class::Error
        );
        assert_eq!(
            classify(Some(r#"{"count":0}"#), true),
            Class::Error,
            "the CLI's own failure flag wins over a body that parses"
        );
        assert_eq!(classify(None, false), Class::Unanswered);
    }

    /// Only the top-level status is a refusal: a fact whose own status is a word
    /// is an answer.
    #[test]
    fn the_classifier_reads_the_top_level_status_only() {
        assert_eq!(
            classify(Some(r#"{"objects":[{"status":"blocked"}]}"#), false),
            Class::Accepted
        );
        assert_eq!(
            classify(Some(r#"{"status":"active"}"#), false),
            Class::Accepted
        );
    }

    // ── the paths ───────────────────────────────────────────────────────────

    #[test]
    fn a_path_reads_nested_keys_and_arrays_in_document_order() {
        let answer = json!({
            "session": {"sid": "ab12", "choices": [{"sid": "cd34"}, {"sid": "ef56"}]},
            "address": "thing:handcart#f1",
        });

        assert_eq!(leaves_at(&answer, "session.sid"), vec!["ab12"]);
        assert_eq!(
            leaves_at(&answer, "session.choices[].sid"),
            vec!["cd34", "ef56"]
        );
        assert_eq!(leaves_at(&answer, "address"), vec!["thing:handcart#f1"]);
        assert!(leaves_at(&answer, "session.nothing").is_empty());
        assert!(leaves_at(&answer, "address.deeper").is_empty());
    }

    // ── the mapping ─────────────────────────────────────────────────────────

    /// A value the replay minted takes the place of the one the recording held,
    /// in the next call's input, wherever it sits and however deep.
    #[test]
    fn a_minted_value_is_mapped_by_position_and_rewritten_in_the_next_input() {
        let mut map = IdMap::default();
        map.learn(
            "start_here",
            &json!({"session": {"sid": "ab12"}}),
            Some(&json!({"session": {"sid": "zz99"}})),
        );
        map.learn(
            "read_mailbox",
            &json!({"messages": [{"id": "m1"}]}),
            Some(&json!({"messages": [{"id": "n1"}]})),
        );

        let (rewritten, orphans) = map.rewrite(&json!({
            "sid": "ab12",
            "message_id": "m1",
            "nested": {"refs": ["ab12", "plain"]},
        }));

        assert_eq!(
            rewritten,
            json!({
                "sid": "zz99",
                "message_id": "n1",
                "nested": {"refs": ["zz99", "plain"]},
            })
        );
        assert_eq!(orphans, 0);
    }

    /// **A record address is mapped like a sid.** A candidate whose numbering
    /// differs (a migration, a seeded record) mints another address for the same
    /// write, and every later call that names the record would otherwise be a
    /// false divergence.
    #[test]
    fn a_record_address_is_mapped_to_the_one_the_replay_minted() {
        let mut map = IdMap::default();
        map.learn(
            "capture",
            &json!({"address": "thing:handcart#f5"}),
            Some(&json!({"address": "thing:handcart#f4"})),
        );

        let (rewritten, orphans) = map.rewrite(&json!({"derived_from": "thing:handcart#f5"}));

        assert_eq!(rewritten, json!({"derived_from": "thing:handcart#f4"}));
        assert_eq!(orphans, 0);
    }

    /// Several values at one mint point are paired by position.
    #[test]
    fn a_list_of_minted_values_is_paired_by_position() {
        let mut map = IdMap::default();
        map.learn(
            "read_mailbox",
            &json!({"messages": [{"id": "m1"}, {"id": "m2"}]}),
            Some(&json!({"messages": [{"id": "n1"}, {"id": "n2"}]})),
        );

        let (rewritten, _) = map.rewrite(&json!({"message_id": "m2"}));

        assert_eq!(rewritten, json!({"message_id": "n2"}));
    }

    /// Only a whole string leaf is replaced, never a piece of one and never a
    /// key.
    #[test]
    fn only_a_whole_leaf_is_rewritten() {
        let mut map = IdMap::default();
        map.learn(
            "start_here",
            &json!({"session": {"sid": "ab12"}}),
            Some(&json!({"session": {"sid": "zz99"}})),
        );

        let input = json!({"ab12": "kept", "note": "run ab12 went well", "sid": "ab12x"});
        let (rewritten, _) = map.rewrite(&input);

        assert_eq!(rewritten, input);
    }

    /// A recorded value the replay never minted is left as recorded and counted:
    /// a call that carries it is the cascade of the call that should have
    /// minted it.
    #[test]
    fn a_value_the_replay_never_minted_is_left_alone_and_counted() {
        let mut map = IdMap::default();
        map.learn(
            "start_here",
            &json!({"session": {"sid": "ab12"}}),
            Some(&json!({"status": "blocked", "fix_by": "change"})),
        );

        let (rewritten, orphans) = map.rewrite(&json!({"sid": "ab12", "other": "ab12"}));

        assert_eq!(rewritten, json!({"sid": "ab12", "other": "ab12"}));
        assert_eq!(orphans, 2);
    }

    /// A mint point of another verb is not read: the table names the verb.
    #[test]
    fn a_mint_point_belongs_to_its_verb() {
        let mut map = IdMap::default();
        map.learn(
            "recall",
            &json!({"session": {"sid": "ab12"}}),
            Some(&json!({"session": {"sid": "zz99"}})),
        );

        let (rewritten, orphans) = map.rewrite(&json!({"sid": "ab12"}));

        assert_eq!(rewritten, json!({"sid": "ab12"}));
        assert_eq!(orphans, 0, "recall does not mint a session");
    }

    // ── secrets in a recording ──────────────────────────────────────────────

    /// A key and a wrap code in the shapes a server mints: observed from the
    /// boot answer and the wrap receipt, not invented.
    const KEY_ONE: &str = "sk-7fh3m9qz2k4r8tvn5xb1c6dwae";
    const KEY_TWO: &str = "sk-0a1b2c3d4e5f6g7h8j9k0m1n2p";
    const CODE: &str = "wc-4fk8m2qr7t3z";

    /// **A recording holds no key and no wrap code, and the stand-ins still
    /// pair.** The same secret is the same stand-in wherever it appears, two
    /// secrets are two stand-ins, the stand-ins keep the shape so the table's
    /// scan still reads them as minted, and words that merely begin the same way
    /// are left alone.
    #[test]
    fn a_stream_is_read_with_its_keys_and_wrap_codes_replaced_by_stand_ins_that_pair() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "start_here", json!({"bot": "assistant"})),
            answered(
                "t1",
                &format!(r#"{{"session":{{"sid":"{KEY_ONE}","run":"jm7z4k"}}}}"#),
            ),
            called(
                "t2",
                "journal",
                json!({"sid": KEY_ONE, "entry": "sk-short and wc-too-short"}),
            ),
            answered(
                "t2",
                &format!(r#"{{"wrap_code":"{CODE}","other":"{KEY_TWO}"}}"#),
            ),
            called("t3", "recall", json!({"sid": KEY_TWO})),
            answered("t3", r#"{}"#),
        ]);

        let sittings = read_stream(&stream);
        let calls = &sittings[0].calls;

        let text = serde_json::to_string(&json!({
            "inputs": calls.iter().map(|c| c.input.clone()).collect::<Vec<_>>(),
            "answers": calls.iter().map(|c| c.answer.clone()).collect::<Vec<_>>(),
        }))
        .unwrap();
        for secret in [KEY_ONE, KEY_TWO, CODE] {
            assert!(!text.contains(secret), "{secret} was kept: {text}");
        }
        let first = calls[1].input["sid"].as_str().unwrap().to_string();
        let second = calls[2].input["sid"].as_str().unwrap().to_string();
        assert_ne!(first, second, "two keys are two stand-ins");
        assert!(
            calls[0].answer.as_deref().unwrap().contains(&first),
            "the key a boot handed back and the key a call sent are one stand-in"
        );
        assert!(is_secret_shaped(&first) && is_secret_shaped(&second));
        assert_eq!(
            calls[1].input["entry"], "sk-short and wc-too-short",
            "words that only begin the same way are not secrets"
        );
    }

    /// **A key that flows outside the table is named, and one inside it is
    /// not.** A key is 29 characters, past the length the scan used to read as
    /// minted, so a verb that handed one back at a path the table lacks was
    /// invisible to it.
    #[test]
    fn the_scan_reads_a_key_as_minted_and_a_run_name_the_table_knows_is_not_reported() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "start_here", json!({"bot": "assistant"})),
            answered(
                "t1",
                &format!(
                    r#"{{"session":{{"sid":"{KEY_ONE}","run":"jm7z4k"}},"reserve":"{KEY_TWO}"}}"#
                ),
            ),
            called("t2", "journal", json!({"sid": KEY_ONE})),
            answered("t2", r#"{}"#),
            called(
                "t3",
                "start_here",
                json!({"bot": "assistant", "resume": "jm7z4k"}),
            ),
            answered("t3", r#"{}"#),
            called("t4", "recall", json!({"sid": KEY_TWO})),
            answered("t4", r#"{}"#),
        ]);

        let found = unmapped_ids(&read_stream(&stream));

        assert_eq!(found, vec!["start_here.reserve (1 use)"]);
    }

    /// **The boot's key and the run's name are both mint points**, so a replay
    /// pairs each recorded value with the one its own server minted and a resume
    /// by name finds the run.
    #[test]
    fn a_boots_key_and_name_are_each_mapped_to_the_ones_the_replay_minted() {
        let mut map = IdMap::default();
        let replayed_key = "sk-vvvvvvvvvvvvvvvvvvvvvvvvvv";
        map.learn(
            "start_here",
            &serde_json::from_str(&format!(
                r#"{{"session":{{"sid":"{KEY_ONE}","run":"jm7z4k"}}}}"#
            ))
            .unwrap(),
            Some(&json!({"session": {"sid": replayed_key, "run": "zz99zz"}})),
        );

        let (rewritten, orphans) =
            map.rewrite(&json!({"sid": KEY_ONE, "resume": "jm7z4k", "other": KEY_TWO}));

        assert_eq!(
            rewritten,
            json!({"sid": replayed_key, "resume": "zz99zz", "other": KEY_TWO})
        );
        assert_eq!(orphans, 0);
    }

    // ── the stale-table scan ────────────────────────────────────────────────

    #[test]
    fn the_scan_names_a_minted_looking_value_that_flows_outside_the_table() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "start_here", json!({"bot": "assistant"})),
            // A mint point in the table: not reported.
            answered(
                "t1",
                r#"{"session":{"sid":"ab12"},"ticket":"cd34","day":"2026-01-02","kind":"thing"}"#,
            ),
            called(
                "t2",
                "capture",
                json!({"sid": "ab12", "note": "cd34", "recorded_at": "2026-01-02", "kind": "thing"}),
            ),
            answered("t2", r#"{"address":"thing:handcart#f1"}"#),
            called("t3", "update_fact", json!({"address": "thing:handcart#f1"})),
            answered("t3", r#"{}"#),
            called(
                "t4",
                "capture",
                json!({"derived_from": "thing:handcart#f1"}),
            ),
            answered("t4", r#"{}"#),
        ]);

        let found = unmapped_ids(&read_stream(&stream));

        assert_eq!(found, vec!["start_here.ticket (1 use)"]);
    }

    /// A value the table learns at its mint point is not reported when a read
    /// shows it again: the mint point is where it is learnt, wherever it is read
    /// from.
    #[test]
    fn the_scan_ignores_a_value_a_read_repeats_after_its_mint_point() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "start_here", json!({"bot": "assistant"})),
            answered("t1", r#"{"session":{"sid":"ab12"}}"#),
            called("t2", "list_runs", json!({"sid": "ab12"})),
            answered("t2", r#"{"runs":[{"sid":"ab12"}]}"#),
            called("t3", "recall", json!({"sid": "ab12"})),
            answered("t3", r#"{}"#),
        ]);

        assert!(unmapped_ids(&read_stream(&stream)).is_empty());
    }

    // ── what a replay did not see ───────────────────────────────────────────

    #[tokio::test]
    async fn a_replay_that_saw_everything_has_no_gaps() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "search", json!({"query": "kite"})),
            answered("t1", r#"{"results":[]}"#),
        ]);
        let sittings = read_stream(&stream);
        let door = Scripted::new(&[r#"{"results":[]}"#]);

        let results = replay(&sittings, &door).await;

        assert_eq!(gaps(&stream, &sittings, &results), vec![]);
    }

    /// **A call the server answered with an error, in the recording and again in
    /// the replay, is the server doing the same thing twice.** The surface hands
    /// such an error back the way it hands back a lost connection, so only a
    /// call the recording got an answer to is read as a room that broke.
    #[tokio::test]
    async fn a_recorded_error_the_replay_reproduces_is_not_a_broken_room() {
        let stream = stream_of(&[
            boundary("s"),
            called("t1", "recall", json!({"kind": "type"})),
            failed("t1", "kind 'type': no kind is named 'type'"),
        ]);
        let sittings = read_stream(&stream);
        let door =
            Scripted::new(&[r#"{"transport_error":"kind 'type': no kind is named 'type'"}"#]);

        let results = replay(&sittings, &door).await;

        assert_eq!(results[0].calls[0].recorded, Class::Error);
        assert_eq!(results[0].calls[0].replayed, Class::Error);
        assert!(!results[0].calls[0].differs());
        assert_eq!(gaps(&stream, &sittings, &results), vec![]);
    }

    /// **Each way a replay can fall short is its own gap**, so a report of zero
    /// changes is never read as clean over a recording it did not fully send or
    /// compare: a line of the stream it could not read, a call the recording
    /// never answered, a room that broke under a call, and a value that looks
    /// minted and was sent as recorded.
    #[tokio::test]
    async fn every_way_a_replay_falls_short_is_named_as_a_gap() {
        let stream = stream_of(&[
            boundary("first"),
            "this line is not an event".to_string(),
            called("t1", "capture", json!({"subject": "thing:handcart"})),
            answered("t1", r#"{"address":"thing:handcart#f1","ticket":"cd34"}"#),
            called(
                "t2",
                "update_fact",
                json!({"address": "thing:handcart#f1", "note": "cd34"}),
            ),
            answered("t2", r#"{}"#),
            boundary("second"),
            called("t3", "search", json!({"query": "kite"})),
        ]);
        let sittings = read_stream(&stream);
        let door = Scripted::new(&[
            r#"{"address":"thing:handcart#f1"}"#,
            r#"{"transport_error":"connection reset"}"#,
            r#"{"results":[]}"#,
        ]);

        let results = replay(&sittings, &door).await;

        assert_eq!(
            gaps(&stream, &sittings, &results),
            vec![
                Gap::UnreadLines(1),
                Gap::Unanswered(1),
                Gap::RoomBroke {
                    sittings: vec!["first".to_string()],
                    calls: 1
                },
                Gap::Unmapped(vec!["capture.ticket (1 use)".to_string()]),
            ]
        );
    }

    // ── the replay ──────────────────────────────────────────────────────────

    /// A door that answers from a script and keeps what it was sent.
    struct Scripted {
        said: Vec<String>,
        seen: std::sync::Mutex<Vec<(String, Value)>>,
    }

    impl Scripted {
        fn new(said: &[&str]) -> Scripted {
            Scripted {
                said: said.iter().map(|s| s.to_string()).collect(),
                seen: Default::default(),
            }
        }

        fn sent(&self) -> Vec<(String, Value)> {
            self.seen.lock().expect("the lock").clone()
        }
    }

    impl Door for Scripted {
        fn call(
            &self,
            verb: &str,
            args: Value,
        ) -> impl std::future::Future<Output = String> + Send {
            let mut seen = self.seen.lock().expect("the lock");
            let at = seen.len();
            seen.push((verb.to_string(), args));
            let body = self.said[at].clone();
            async move { body }
        }
    }

    fn two_calls() -> Vec<Sitting> {
        read_stream(&stream_of(&[
            boundary("s"),
            called("t1", "start_here", json!({"bot": "assistant"})),
            answered("t1", r#"{"session":{"sid":"ab12"}}"#),
            called(
                "t2",
                "capture",
                json!({"sid": "ab12", "subject": "thing:handcart"}),
            ),
            answered("t2", r#"{"address":"thing:handcart#f5"}"#),
            called(
                "t3",
                "update_fact",
                json!({"sid": "ab12", "address": "thing:handcart#f5"}),
            ),
            answered("t3", r#"{"status":"active"}"#),
        ]))
    }

    /// The same classes everywhere: nothing differs, and the values the replay
    /// minted went into the calls that followed.
    #[tokio::test]
    async fn a_replay_that_answers_in_the_recorded_classes_reports_nothing_changed() {
        let door = Scripted::new(&[
            r#"{"session":{"sid":"zz99"}}"#,
            r#"{"address":"thing:handcart#f4"}"#,
            r#"{"status":"active"}"#,
        ]);

        let results = replay(&two_calls(), &door).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 3);
        assert!(results[0].calls.iter().all(|c| !c.differs()), "{results:?}");
        let sent = door.sent();
        assert_eq!(
            sent[1].1,
            json!({"sid": "zz99", "subject": "thing:handcart"})
        );
        assert_eq!(
            sent[2].1,
            json!({"sid": "zz99", "address": "thing:handcart#f4"}),
            "the sid and the address are the ones the replay minted"
        );
    }

    /// The first call whose class changed is the first divergence; what follows
    /// is counted apart, and the calls that carried a value the replay never
    /// minted are counted as its cascade.
    #[tokio::test]
    async fn the_first_divergence_is_named_and_the_calls_after_it_are_counted_apart() {
        let door = Scripted::new(&[
            r#"{"status":"blocked","fix_by":"change"}"#,
            r#"{"status":"blocked","fix_by":"change"}"#,
            r#"{"status":"blocked","fix_by":"change"}"#,
        ]);

        let results = replay(&two_calls(), &door).await;
        let summary = summarise(&results[0]);

        assert_eq!(summary.calls, 3);
        assert_eq!(summary.differing, 3);
        let first = summary.first.expect("a first divergence");
        assert_eq!(first.index, 0);
        assert_eq!(first.verb, "start_here");
        assert_eq!(first.recorded, Class::Accepted);
        assert_eq!(first.replayed, Class::Refused("change".into()));
        assert_eq!(summary.after_first, 2);
        assert_eq!(
            summary.after_first_orphan, 2,
            "both later calls carried the sid the replay never minted"
        );
    }

    /// A divergence that carries no unmapped value is independent of the first.
    #[tokio::test]
    async fn a_later_divergence_with_no_unmapped_value_counts_as_independent() {
        let door = Scripted::new(&[
            r#"{"session":{"sid":"zz99"}}"#,
            r#"{"status":"blocked","fix_by":"change"}"#,
            r#"{"status":"active"}"#,
        ]);

        let results = replay(&two_calls(), &door).await;
        let summary = summarise(&results[0]);

        assert_eq!(summary.differing, 1);
        assert_eq!(summary.first.expect("first").index, 1);
        assert_eq!(summary.after_first, 0);
    }

    /// A recorded call with no answer has nothing to compare with.
    #[tokio::test]
    async fn a_call_the_recording_never_answered_is_not_a_difference() {
        let sittings = read_stream(&stream_of(&[
            boundary("s"),
            called("t1", "search", json!({"query": "kite"})),
        ]));
        let door = Scripted::new(&[r#"{"status":"blocked","fix_by":"change"}"#]);

        let results = replay(&sittings, &door).await;

        assert_eq!(results[0].calls[0].recorded, Class::Unanswered);
        assert!(!results[0].calls[0].differs());
    }

    // ── the report ──────────────────────────────────────────────────────────

    #[test]
    fn the_report_names_the_first_divergence_and_what_the_table_missed() {
        let result = SittingResult {
            name: "Phase 3".into(),
            calls: vec![CallResult {
                index: 4,
                verb: "capture".into(),
                recorded: Class::Accepted,
                replayed: Class::Refused("change".into()),
                recorded_head: "recorded body".into(),
                replayed_head: "replayed body".into(),
                orphan_uses: 0,
                broke: false,
            }],
        };
        let gaps = [Gap::Unmapped(vec!["start_here.ticket (1 use)".to_string()])];

        let text = render(
            "vault",
            "vault-1.jsonl",
            "abc1234",
            &a_day(),
            &gaps,
            &[result],
        );

        for needle in [
            "Phase 3",
            "capture",
            "start_here.ticket",
            "abc1234",
            "vault-1.jsonl",
        ] {
            assert!(text.contains(needle), "{needle} is missing from:\n{text}");
        }
    }

    /// **A report with a gap says INCOMPLETE and one without does not**, and
    /// every report says what the classes cannot see.
    #[test]
    fn a_report_with_a_gap_says_incomplete_and_a_complete_one_does_not() {
        let result = SittingResult {
            name: "Phase 3".into(),
            calls: vec![],
        };

        let incomplete = render(
            "vault",
            "v-1.jsonl",
            "abc",
            &a_day(),
            &[Gap::UnreadLines(2)],
            std::slice::from_ref(&result),
        );
        let complete = render("vault", "v-1.jsonl", "abc", &a_day(), &[], &[result]);

        assert!(incomplete.contains("INCOMPLETE"), "{incomplete}");
        assert!(!complete.contains("INCOMPLETE"), "{complete}");
        assert!(incomplete.contains(SCOPE) && complete.contains(SCOPE));
    }

    // ── the day a replay acts out ───────────────────────────────────────────

    fn a_day() -> ReplayDay {
        ReplayDay {
            day: "2031-05-06".into(),
            source: DaySource::Announced,
        }
    }

    /// A stream holding one `start_here` call whose answer is `answer`, in the
    /// shape the CLI keeps.
    fn a_stream_answering(answer: &str) -> String {
        a_stream_answering_as("t1", answer)
    }

    /// The same, under a call id of its own, so two can share one log.
    fn a_stream_answering_as(id: &str, answer: &str) -> String {
        [
            serde_json::json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": id,
                 "name": format!("{VERB_PREFIX}start_here"), "input": {"bot": "assistant"}}
            ]}})
            .to_string(),
            serde_json::json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": id, "content": answer}
            ]}})
            .to_string(),
        ]
        .join("\n")
    }

    const ANNOUNCING: &str =
        r#"{"session":{"sid":"abcd"},"clock":{"stated_day":"2031-05-06","note":"acting out"}}"#;
    const SILENT: &str = r#"{"session":{"sid":"abcd"},"timezone":{"zone":"UTC"}}"#;

    /// **A run whose own server announced its day replays on that day**, and the
    /// day is read from what the server said, not from the file's name.
    #[test]
    fn a_run_that_announced_its_day_replays_on_it() {
        let day = day_of_run(
            &a_stream_answering(ANNOUNCING),
            Path::new("year-1893499200.jsonl"),
        )
        .expect("the log records a day");
        assert_eq!(day.day, "2031-05-06");
        assert_eq!(day.source, DaySource::Announced);
    }

    /// **A run made on the wall clock replays on the day its file name stamps**,
    /// read as UTC. Paired with the case above: the stamp is 2030-01-01 and the
    /// announcement is 2031-05-06, so a reader that preferred the stamp would
    /// pass this and fail that.
    #[test]
    fn a_run_that_announced_nothing_replays_on_the_day_of_its_file_name() {
        let day = day_of_run(
            &a_stream_answering(SILENT),
            Path::new("year-1893499200.jsonl"),
        )
        .expect("the file name records a day");
        assert_eq!(day.day, "2030-01-01");
        assert_eq!(day.source, DaySource::FileName);
    }

    /// **A run whose log records no day is refused, naming the run.** Nothing is
    /// guessed: a replay on the wall clock would pass or fail on the calendar.
    #[test]
    fn a_run_whose_log_records_no_day_is_refused_naming_the_run() {
        for name in ["year-latest.jsonl", "year.jsonl", "notes"] {
            let refused = day_of_run(&a_stream_answering(SILENT), Path::new(name))
                .expect_err("no day is on record");
            assert!(
                refused.to_string().contains(name)
                    && refused.to_string().to_lowercase().contains("no day"),
                "the refusal does not name the run and say no day is on record: {refused}",
            );
        }
    }

    /// **A log that announces two days cannot be acted out**, and is refused
    /// rather than replayed on the first.
    #[test]
    fn a_log_announcing_two_days_is_refused() {
        let two = format!(
            "{}\n{}",
            a_stream_answering(ANNOUNCING),
            a_stream_answering_as("t2", &ANNOUNCING.replace("2031-05-06", "2031-05-07")),
        );
        let refused =
            day_of_run(&two, Path::new("year-1893499200.jsonl")).expect_err("two days are not one");
        assert!(
            refused.to_string().contains("2031-05-06")
                && refused.to_string().contains("2031-05-07"),
            "{refused}",
        );
    }

    /// The conversion from a stamp to a day, against days worked out by hand.
    #[test]
    fn a_stamp_in_seconds_reads_as_the_utc_day_it_falls_on() {
        for (seconds, day) in [
            (0_u64, "1970-01-01"),
            (86_399, "1970-01-01"),
            (86_400, "1970-01-02"),
            (951_782_400, "2000-02-29"),
            (1_787_418_998, "2026-08-22"),
            (1_893_499_200, "2030-01-01"),
        ] {
            assert_eq!(utc_day_of(seconds), day, "{seconds}");
        }
    }

    // ── the streams on disk ─────────────────────────────────────────────────

    #[test]
    fn the_replay_keeps_its_files_under_the_state_directory() {
        assert_eq!(
            state_root_from(Some("/state"), Some("/home/x")),
            Some(PathBuf::from("/state/jojobot-replay"))
        );
        assert_eq!(
            state_root_from(None, Some("/home/x")),
            Some(PathBuf::from("/home/x/.local/state/jojobot-replay"))
        );
        assert_eq!(
            state_root_from(Some(""), Some("/home/x")),
            Some(PathBuf::from("/home/x/.local/state/jojobot-replay")),
            "an empty XDG_STATE_HOME is not a place"
        );
        assert_eq!(state_root_from(None, None), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("jojobot-replay-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    #[test]
    fn a_stream_names_its_room() {
        assert_eq!(
            room_of(Path::new("/anywhere/vault-1791505684.jsonl")),
            Some("vault".to_string())
        );
        assert_eq!(room_of(Path::new("year-3.jsonl")), Some("year".to_string()));
        assert_eq!(room_of(Path::new("unnamed.jsonl")), None);
    }

    /// The newest is the highest stamp as a number, not the last name.
    #[test]
    fn the_newest_stream_of_a_room_is_the_highest_stamp() {
        let dir = scratch("newest");
        for name in [
            "vault-999.jsonl",
            "vault-1000.jsonl",
            "year-5000.jsonl",
            "vault-1000.md",
        ] {
            std::fs::write(dir.join(name), "").expect("a file");
        }

        let newest = newest_of(&dir, "vault").expect("reads").expect("one");
        let per_room = newest_per_room(&dir).expect("reads");

        assert_eq!(
            newest.file_name().and_then(|n| n.to_str()),
            Some("vault-1000.jsonl")
        );
        let names: Vec<_> = per_room
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect();
        assert_eq!(names, vec!["vault-1000.jsonl", "year-5000.jsonl"]);
        assert_eq!(newest_of(&dir, "bike").expect("reads"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is not named as a stream is refused by name: a skipped one
    /// would read as a run that never happened.
    #[test]
    fn a_stream_that_is_not_named_as_one_is_refused_by_name() {
        let dir = scratch("refused");
        std::fs::write(dir.join("notes.jsonl"), "").expect("a file");

        let refused = newest_per_room(&dir).expect_err("refused");

        assert!(refused.to_string().contains("notes.jsonl"), "{refused}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
