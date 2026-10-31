//! The read-only boundary shared by the verbs and their other renderers.

mod answer;
mod list_ticks;
mod mail;
mod question;
mod recalled;
mod runs;

#[cfg(doctest)]
#[path = "boundary.rs"]
mod read_path_boundary;

pub use answer::{
    Answered, Backing, Board, Failed, LeftOut, Narrowed, OwedPass, ReadyPass, TickCell, Verdict,
    ViewAnswer, ViewOutcome,
};
pub use jojobot_domain::memory::graph::{GraphQuery, Include, Object, Selected, Selection};
pub use jojobot_domain::memory::{EntityId, FieldAges, FieldBacking, MemoryError};
pub use list_ticks::TickRead;
pub use mail::{MailHandle, MailRail, OperatorIs, OperatorMail, OperatorMailCommands, Refused};
pub use question::{Asks, Question, Readiness, ViewRequest, parse_date};
pub use recalled::{RecalledObject, RecalledView};
pub use runs::{Runs, RunsAsk, RunsFailed, may_read_runs};

use std::sync::Arc;

use jojobot_domain::actor::Actor;

/// The implementation is supplied by the composition root. It exposes the
/// question, and no way to retrieve the implementation or its dependencies.
#[async_trait::async_trait]
pub trait Answerer: Send + Sync {
    /// **A question, answered for the one asking** (rule 444). The asker comes
    /// beside every question and is never defaulted, so a read with no asker
    /// cannot exist.
    async fn ask(&self, asker: &Actor, question: Question) -> Result<Answered, Failed>;
}

#[derive(Clone)]
pub struct QueryHandle(Arc<dyn Answerer>);

impl From<Arc<dyn Answerer>> for QueryHandle {
    fn from(answerer: Arc<dyn Answerer>) -> Self {
        Self(answerer)
    }
}

impl QueryHandle {
    pub async fn ask(&self, asker: &Actor, question: Question) -> Result<Answered, Failed> {
        self.0.ask(asker, question).await
    }
}
