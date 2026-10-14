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
}

#[derive(Debug)]
enum State {
    /// No commit has asked for the request yet.
    Open,
    /// A commit has it and is writing.
    Taken,
    /// The commit stored the write together with the id's row.
    Spent,
    /// The id was already spent when the commit tried to spend it, and the
    /// commit stored nothing.
    Lost,
}

impl Scope {
    pub fn new(request: Request) -> Arc<Self> {
        Arc::new(Self {
            request,
            state: Mutex::new(State::Open),
        })
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

    /// Whether a commit found the id already spent and stored nothing.
    pub fn was_lost(&self) -> bool {
        matches!(
            *self.state.lock().expect("a scope is poisoned"),
            State::Lost
        )
    }
}

tokio::task_local! {
    static CURRENT: Arc<Scope>;
    static ARMED: ();
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
pub fn take() -> Option<Reservation> {
    ARMED.try_with(|()| ()).ok()?;
    CURRENT
        .try_with(|scope| {
            let mut state = scope.state.lock().expect("a scope is poisoned");
            if !matches!(*state, State::Open) {
                return None;
            }
            *state = State::Taken;
            Some(Reservation {
                scope: Arc::clone(scope),
            })
        })
        .ok()
        .flatten()
}

/// A commit's claim on the id of the call in flight.
#[derive(Debug)]
pub struct Reservation {
    scope: Arc<Scope>,
}

impl Reservation {
    pub fn request(&self) -> &Request {
        &self.scope.request
    }

    /// The commit stored the write with the id's row.
    pub fn spent(self) {
        *self.scope.state.lock().expect("a scope is poisoned") = State::Spent;
    }

    /// The commit found the id already spent and stored nothing.
    pub fn lost(self) {
        *self.scope.state.lock().expect("a scope is poisoned") = State::Lost;
    }

    /// The commit failed for another reason and stored nothing. The id stays
    /// unspent.
    pub fn failed(self) {
        *self.scope.state.lock().expect("a scope is poisoned") = State::Open;
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
        assert!(!scope.was_lost());
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
    async fn a_lost_race_is_reported_to_the_scope() {
        let scope = Scope::new(request("r1"));
        within(
            Arc::clone(&scope),
            armed(async {
                take().expect("taken").lost();
            }),
        )
        .await;
        assert!(scope.was_lost());
        assert!(!scope.was_spent());
    }
}
