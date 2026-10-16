//! Request ids — a write the network may have dropped the answer to.
//!
//! **The problem this answers.** A caller sends a write and the answer never
//! arrives. The write may have landed or not, and sending it again may write it
//! twice. A caller that sends a `request_id` with the write can send the same
//! call again: if the first one landed, the second returns the first one's
//! receipt, marked as a repeat, and writes nothing.
//!
//! **The id is spent by a write, and by nothing else.** The write inserts its
//! own row in the same transaction that stores it, so a write and the record
//! that it happened land together or not at all. A call that writes nothing (a
//! refusal, a block, a no-op) spends no id, and sending it again simply runs
//! again.
//!
//! **The receipt text is filled in after the transaction commits**, because a
//! verb builds its receipt from what the store returned. A process that stops
//! between the two leaves a row with no receipt. The row still holds what the
//! write returned ([`Stored::landed`]), so a repeat can say the write landed.
//!
//! **A row is keyed by the permanent identity of the bot**, never its handle,
//! so a rename cannot hand one bot's receipt to another. The same id from
//! another bot is another row.
//!
//! **How a store learns that this write carries an id.** The dispatch opens a
//! [`Scope`] around the verb with [`within`]. The verb wraps its one call to the
//! store in [`armed`], and the commit inside that call takes the request with
//! [`take`]. A commit outside [`armed`] (a lease renewal the dispatch makes for
//! its own bookkeeping) takes nothing, and so does a second commit inside it (a
//! clear of a due day on another record, say): the request is handed over once.

use std::future::Future;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use crate::memory::EntityId;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The longest id a caller may send, in bytes.
pub const MAX_ID_BYTES: usize = 128;

/// How long a store keeps a spent id. The retention is a day at least; a store
/// removes older rows when it is next written.
pub const KEPT_FOR: jiff::SignedDuration = jiff::SignedDuration::from_hours(72);

/// A caller-chosen string that names one write.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// The id as the caller sent it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Accept an id: one to [`MAX_ID_BYTES`] bytes of text with no control
    /// character and no edge space. What is kept is exactly what was sent, so
    /// two ids that differ in any byte are two ids.
    pub fn parse(raw: &str) -> Result<Self, RequestError> {
        if raw.is_empty() {
            return Err(RequestError::Invalid("the id is empty".into()));
        }
        if raw.len() > MAX_ID_BYTES {
            return Err(RequestError::Invalid(format!(
                "the id is {} bytes and the most it may be is {MAX_ID_BYTES}",
                raw.len()
            )));
        }
        if raw.trim() != raw {
            return Err(RequestError::Invalid(
                "the id begins or ends with a space".into(),
            ));
        }
        if raw.chars().any(char::is_control) {
            return Err(RequestError::Invalid(
                "the id holds a control character".into(),
            ));
        }
        Ok(Self(raw.to_string()))
    }
}

/// One call's claim on an id: who sent it, on which verb, and with which
/// arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The handle the bot answers to when the call arrives. A store resolves it
    /// to the permanent identity inside the transaction.
    pub bot: EntityId,
    pub id: RequestId,
    pub verb: String,
    /// [`digest_of`] the arguments, with `sid` and `request_id` left out.
    pub digest: String,
}

/// What a store keeps for a spent id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub verb: String,
    pub digest: String,
    /// What the write returned to the verb, as JSON text. It is the answer a
    /// repeat gives when [`Stored::receipt`] is missing.
    pub landed: String,
    /// The verb's answer, as JSON text, once the verb gave it.
    pub receipt: Option<String>,
}

/// A port onto the ledger of spent ids.
///
/// **There is no `reserve` here.** A reservation is made by the store's own
/// write, in its own transaction; this port reads the ledger and fills the
/// receipt after the commit.
#[async_trait::async_trait]
pub trait Requests: Send + Sync {
    /// What this bot spent `id` on, or `None` when it has not spent it.
    async fn lookup(&self, bot: &EntityId, id: &RequestId) -> Result<Option<Stored>, RequestError>;

    /// Record the answer the verb gave for a spent id. Filling an id nobody
    /// spent changes nothing.
    async fn fill(&self, request: &Request, receipt: &str) -> Result<(), RequestError>;

    /// **The parts of a write that an earlier call under this id wrote**, in the
    /// order they were written. Empty when the id began no write that commits in
    /// parts.
    async fn parts(&self, bot: &EntityId, id: &RequestId) -> Result<Vec<PartRow>, RequestError>;

    /// **Spend the id of a write that committed in parts, now that every part is
    /// written**, with the answer the verb gave. A repeat then answers from it. An
    /// id already spent keeps its row and takes the receipt.
    async fn finish(&self, request: &Request, receipt: &str) -> Result<(), RequestError>;
}

/// Why a request operation failed.
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    /// The id is not one this build accepts.
    #[error("request id refused: {0}")]
    Invalid(String),
    /// The store could not be reached or answered with something this port
    /// cannot read.
    #[error("request store error: {0}")]
    Store(String),
}

/// **`text` as a JSON string, quotes included.** A fake store builds the `landed`
/// of a part by hand and a reason or a name carries any character.
pub fn json_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A digest of the canonical text of a call's arguments.
pub fn digest_of(canonical: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// What the dispatch knows about the call in flight, and what a store tells it
/// back.
#[derive(Debug)]
pub struct Scope {
    request: Request,
    state: Mutex<State>,
    parts: Mutex<Parts>,
}

/// **What a part of a write left in the ledger**, as a later call reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// What the store returned for the part, as JSON text.
    pub landed: String,
    /// What the verb asked the ledger to keep beside the part, as text: the
    /// plan a repeat must finish. `None` when the verb kept nothing.
    pub note: Option<String>,
}

/// One part of a write that commits in several, as the ledger holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRow {
    pub part: String,
    pub verb: String,
    pub digest: String,
    pub recorded: Recorded,
}

#[derive(Debug, Default)]
struct Parts {
    done: std::collections::HashMap<String, Recorded>,
    inflight: std::collections::HashSet<String>,
    /// Whether any commit of this call armed a part.
    used: bool,
    /// Whether the verb said it has written every part.
    finished: bool,
    /// The parts this call found written and skipped, in the order it met them.
    skipped: Vec<String>,
}

#[derive(Debug)]
enum State {
    /// No commit has asked for the request yet.
    Open,
    /// A commit has it and is writing.
    Taken,
    /// The commit stored the write together with the id's row.
    Spent,
}

impl Scope {
    pub fn new(request: Request) -> Arc<Self> {
        Arc::new(Self {
            request,
            state: Mutex::new(State::Open),
            parts: Mutex::new(Parts::default()),
        })
    }

    /// **A call that sends an id some of whose parts an earlier call already
    /// wrote.** The parts are skipped by [`part`] and read back from here.
    pub fn resuming(request: Request, recorded: Vec<(String, Recorded)>) -> Arc<Self> {
        let scope = Self::new(request);
        scope
            .parts
            .lock()
            .expect("a scope is poisoned")
            .done
            .extend(recorded);
        scope
    }

    /// What an earlier call left for the part `name`.
    pub fn recorded(&self, name: &str) -> Option<Recorded> {
        self.parts
            .lock()
            .expect("a scope is poisoned")
            .done
            .get(name)
            .cloned()
    }

    /// Whether this call wrote or skipped any part.
    pub fn took_parts(&self) -> bool {
        let parts = self.parts.lock().expect("a scope is poisoned");
        parts.used || !parts.done.is_empty()
    }

    /// Whether the verb said every part is written.
    pub fn was_finished(&self) -> bool {
        self.parts.lock().expect("a scope is poisoned").finished
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    /// Whether a commit stored the write with the id's row.
    pub fn was_spent(&self) -> bool {
        matches!(
            *self.state.lock().expect("a scope is poisoned"),
            State::Spent
        )
    }
}

/// The part a commit belongs to, and what its verb keeps beside it.
#[derive(Debug, Clone)]
struct Armed {
    name: String,
    note: Option<String>,
}

tokio::task_local! {
    static CURRENT: Arc<Scope>;
    static ARMED: ();
    static PART: Armed;
}

/// Run `work` with `scope` visible to every commit it makes on this task.
pub async fn within<F: Future>(scope: Arc<Scope>, work: F) -> F::Output {
    CURRENT.scope(scope, work).await
}

/// **Mark `work` as the verb's own write.** A commit inside it may take the
/// request of the call in flight; a commit anywhere else may not.
pub async fn armed<F: Future>(work: F) -> F::Output {
    ARMED.scope((), work).await
}

/// **The claim of the call in flight, handed over once.** The first commit
/// inside [`armed`] that asks gets it, and says afterwards how it went with
/// [`Reservation::spent`] or [`Reservation::failed`]. Every later ask gets
/// `None`, as does a call that carries no id and a commit outside [`armed`].
///
/// **Inside [`armed_part`] the claim is the part's.** Each part of a write that
/// commits in several takes one claim of its own, once, and a part an earlier
/// call wrote takes none.
pub fn take() -> Option<Reservation> {
    ARMED.try_with(|()| ()).ok()?;
    let part = PART.try_with(Armed::clone).ok();
    CURRENT
        .try_with(|scope| match part {
            None => {
                let mut state = scope.state.lock().expect("a scope is poisoned");
                if !matches!(*state, State::Open) {
                    return None;
                }
                *state = State::Taken;
                Some(Reservation {
                    scope: Arc::clone(scope),
                    part: None,
                })
            }
            Some(armed) => {
                let mut parts = scope.parts.lock().expect("a scope is poisoned");
                if parts.done.contains_key(&armed.name)
                    || !parts.inflight.insert(armed.name.clone())
                {
                    return None;
                }
                parts.used = true;
                Some(Reservation {
                    scope: Arc::clone(scope),
                    part: Some(armed),
                })
            }
        })
        .ok()
        .flatten()
}

/// **Mark `work` as one part of a verb that commits in several.** A commit
/// inside it may take the claim of the part called `name`; `note` is what the
/// verb asks the ledger to keep beside the part, such as the plan a repeat
/// must finish.
pub async fn armed_part<F: Future>(name: &str, note: Option<String>, work: F) -> F::Output {
    let part = Armed {
        name: name.to_string(),
        note,
    };
    PART.scope(part, ARMED.scope((), work)).await
}

/// What [`part`] did with a part.
#[derive(Debug)]
pub enum Part<T> {
    /// The part had not been written, and this call wrote it.
    Ran(T),
    /// An earlier call wrote the part. Its work did not run, and this is what
    /// the earlier call left.
    Recorded(Recorded),
}

/// **Write one part of a verb that commits in several, once under an id.** A
/// part that an earlier call under the same id wrote is read back, and its work
/// does not run. A call that carries no id just runs the work.
pub async fn part<F: Future>(name: &str, note: Option<String>, work: F) -> Part<F::Output> {
    if let Ok(Some(recorded)) = CURRENT.try_with(|scope| {
        let found = scope.recorded(name);
        if found.is_some() {
            scope
                .parts
                .lock()
                .expect("a scope is poisoned")
                .skipped
                .push(name.to_string());
        }
        found
    }) {
        return Part::Recorded(recorded);
    }
    Part::Ran(armed_part(name, note, work).await)
}

/// **What an earlier call under this id left for the part `name`**, without
/// running or skipping it. A verb reads it to tell, before it checks anything,
/// that it is finishing a write rather than making one.
pub fn recorded(name: &str) -> Option<Recorded> {
    CURRENT
        .try_with(|scope| scope.recorded(name))
        .ok()
        .flatten()
}

/// **Skip the part `name` without a piece of work to skip**: what an earlier call
/// left for it, with the part named among those this call skipped. For a verb
/// that finishes by a path of its own when its first part is written.
pub fn skip(name: &str) -> Option<Recorded> {
    let found = recorded(name);
    if found.is_some() {
        let _ = CURRENT.try_with(|scope| {
            scope
                .parts
                .lock()
                .expect("a scope is poisoned")
                .skipped
                .push(name.to_string());
        });
    }
    found
}

/// **The parts this call found written and skipped**, in the order it met them.
/// Empty for a call that wrote every part itself and for a call with no id.
pub fn skipped() -> Vec<String> {
    CURRENT
        .try_with(|scope| {
            scope
                .parts
                .lock()
                .expect("a scope is poisoned")
                .skipped
                .clone()
        })
        .unwrap_or_default()
}

/// Whether the call in flight carries a request id.
pub fn carries_an_id() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

/// **Say that every part of the write is written.** The dispatch then stores the
/// answer as the receipt of the id. A verb that stops before it, because a part
/// failed, leaves the id open, and the same call under the same id finishes it.
pub fn finished() {
    let _ = CURRENT.try_with(|scope| {
        scope.parts.lock().expect("a scope is poisoned").finished = true;
    });
}

/// A commit's claim on the id of the call in flight.
#[derive(Debug)]
pub struct Reservation {
    scope: Arc<Scope>,
    part: Option<Armed>,
}

impl Reservation {
    pub fn request(&self) -> &Request {
        &self.scope.request
    }

    /// The part this claim is for, or `None` for the whole write.
    pub fn part(&self) -> Option<&str> {
        self.part.as_ref().map(|armed| armed.name.as_str())
    }

    /// What the verb asked the ledger to keep beside this part.
    pub fn note(&self) -> Option<&str> {
        self.part.as_ref().and_then(|armed| armed.note.as_deref())
    }

    /// The commit stored the write with the id's row.
    pub fn spent(self) {
        match self.part {
            None => *self.scope.state.lock().expect("a scope is poisoned") = State::Spent,
            Some(_) => self.spent_with(String::new()),
        }
    }

    /// The commit stored a part with its row, and `landed` is what the store
    /// returned for it.
    pub fn spent_with(self, landed: String) {
        let Some(armed) = self.part else {
            *self.scope.state.lock().expect("a scope is poisoned") = State::Spent;
            return;
        };
        let mut parts = self.scope.parts.lock().expect("a scope is poisoned");
        parts.inflight.remove(&armed.name);
        parts.done.insert(
            armed.name,
            Recorded {
                landed,
                note: armed.note,
            },
        );
    }

    /// The commit failed for another reason and stored nothing. The id stays
    /// unspent.
    pub fn failed(self) {
        match self.part {
            None => *self.scope.state.lock().expect("a scope is poisoned") = State::Open,
            Some(armed) => {
                self.scope
                    .parts
                    .lock()
                    .expect("a scope is poisoned")
                    .inflight
                    .remove(&armed.name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str) -> Request {
        Request {
            bot: EntityId("bot:milhouse".into()),
            id: RequestId::parse(id).expect("a good id"),
            verb: "capture".into(),
            digest: digest_of("{}"),
        }
    }

    #[test]
    fn an_id_is_one_to_the_limit_of_plain_text() {
        assert!(RequestId::parse("r1").is_ok());
        assert!(RequestId::parse(&"x".repeat(MAX_ID_BYTES)).is_ok());
        for bad in [
            "",
            " r1",
            "r1 ",
            "r\n1",
            "r\u{0}1",
            &"x".repeat(MAX_ID_BYTES + 1),
        ] {
            assert!(RequestId::parse(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn two_ids_that_differ_by_one_byte_are_two_ids() {
        assert_ne!(
            RequestId::parse("r1").unwrap(),
            RequestId::parse("R1").unwrap()
        );
        assert_eq!(RequestId::parse("r1").unwrap().as_str(), "r1");
    }

    #[test]
    fn a_text_is_written_as_a_json_string() {
        assert_eq!(json_text("plain"), "\"plain\"");
        assert_eq!(json_text("a \"b\" \\ c"), "\"a \\\"b\\\" \\\\ c\"");
        assert_eq!(json_text("one\ntwo\u{1}"), "\"one\\ntwo\\u0001\"");
    }

    #[test]
    fn a_digest_is_the_same_for_the_same_text_and_not_for_another() {
        assert_eq!(digest_of("a"), digest_of("a"));
        assert_ne!(digest_of("a"), digest_of("b"));
        assert_eq!(digest_of("a").len(), 64);
    }

    #[tokio::test]
    async fn the_first_commit_takes_the_request_and_a_later_one_takes_nothing() {
        let scope = Scope::new(request("r1"));
        within(
            Arc::clone(&scope),
            armed(async {
                let first = take().expect("the first commit takes the claim");
                assert_eq!(first.request().id.as_str(), "r1");
                // Awaiting does not move the scope off the task.
                tokio::task::yield_now().await;
                assert!(
                    take().is_none(),
                    "a second commit in the call takes nothing"
                );
                first.spent();
            }),
        )
        .await;
        assert!(scope.was_spent());
    }

    #[tokio::test]
    async fn a_commit_that_fails_leaves_the_claim_for_the_next_commit() {
        let scope = Scope::new(request("r1"));
        within(
            Arc::clone(&scope),
            armed(async {
                take().expect("taken").failed();
                assert!(take().is_some(), "a failed commit spent nothing");
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn a_commit_the_dispatch_makes_for_itself_takes_nothing() {
        let scope = Scope::new(request("r1"));
        within(Arc::clone(&scope), async {
            assert!(take().is_none(), "a commit outside armed takes nothing");
            // The positive the negative rests on: the same scope gives the
            // claim to the verb's own write.
            assert!(armed(async { take() }).await.is_some());
        })
        .await;
    }

    #[tokio::test]
    async fn outside_a_scope_nothing_is_taken() {
        assert!(take().is_none());
    }

    #[tokio::test]
    async fn each_part_of_a_call_takes_a_claim_of_its_own_once() {
        let scope = Scope::new(request("r1"));
        within(Arc::clone(&scope), async {
            let first = armed_part("one", Some("plan".into()), async { take() })
                .await
                .expect("the first part takes a claim");
            assert_eq!(first.part(), Some("one"));
            assert!(
                armed_part("one", None, async { take() }).await.is_none(),
                "a part in flight is taken once"
            );
            first.spent_with("{\"id\":1}".into());
            assert!(
                armed_part("one", None, async { take() }).await.is_none(),
                "a part that is written is not taken again"
            );
            // The positive the two refusals rest on: another part takes its own.
            let second = armed_part("two", None, async { take() })
                .await
                .expect("the second part takes a claim");
            second.spent_with("{\"id\":2}".into());
        })
        .await;
        assert_eq!(
            scope.recorded("one"),
            Some(Recorded {
                landed: "{\"id\":1}".into(),
                note: Some("plan".into()),
            })
        );
        assert_eq!(
            scope.recorded("two").map(|r| r.landed).as_deref(),
            Some("{\"id\":2}")
        );
        assert!(scope.took_parts());
        assert!(
            !scope.was_spent(),
            "parts spend no claim of the whole write"
        );
    }

    #[tokio::test]
    async fn a_part_that_failed_can_be_taken_again_in_the_same_call() {
        let scope = Scope::new(request("r1"));
        within(Arc::clone(&scope), async {
            armed_part("one", None, async { take() })
                .await
                .expect("taken")
                .failed();
            assert!(armed_part("one", None, async { take() }).await.is_some());
        })
        .await;
        assert!(scope.recorded("one").is_none());
    }

    #[tokio::test]
    async fn a_recorded_part_is_read_back_and_its_work_does_not_run() {
        let scope = Scope::resuming(
            request("r1"),
            vec![(
                "one".into(),
                Recorded {
                    landed: "{\"id\":1}".into(),
                    note: None,
                },
            )],
        );
        within(Arc::clone(&scope), async {
            match part("one", None, async { panic!("a written part runs nothing") }).await {
                Part::Recorded(recorded) => assert_eq!(recorded.landed, "{\"id\":1}"),
                Part::Ran(()) => panic!("the part was written already"),
            }
            // The positive: a part nobody wrote runs.
            assert!(matches!(part("two", None, async { 7 }).await, Part::Ran(7)));
        })
        .await;
    }

    #[tokio::test]
    async fn the_parts_a_call_skipped_are_named_in_the_order_it_met_them() {
        let scope = Scope::resuming(
            request("r1"),
            ["one", "two"]
                .map(|name| {
                    (
                        name.to_string(),
                        Recorded {
                            landed: String::new(),
                            note: None,
                        },
                    )
                })
                .into(),
        );
        within(Arc::clone(&scope), async {
            assert!(carries_an_id());
            assert!(
                skipped().is_empty(),
                "nothing is skipped before a part is met"
            );
            part("two", None, async {}).await;
            part("three", None, async {}).await;
            part("one", None, async {}).await;
            assert_eq!(skipped(), ["two", "one"], "a part that ran is not skipped");
        })
        .await;
        assert!(!carries_an_id());
        assert!(skipped().is_empty());
    }

    #[tokio::test]
    async fn without_a_scope_a_part_just_runs() {
        assert!(matches!(part("one", None, async { 7 }).await, Part::Ran(7)));
        // Saying it is finished with no scope is no error.
        finished();
    }

    #[tokio::test]
    async fn the_verb_says_when_every_part_is_written() {
        let scope = Scope::new(request("r1"));
        within(Arc::clone(&scope), async {
            assert!(!scope.was_finished());
            finished();
        })
        .await;
        assert!(scope.was_finished());
    }
}
