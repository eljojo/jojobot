//! The read-only boundary shared by the verbs and their other renderers.

mod ancestry;
mod answer;
mod list_ticks;
mod mail;
mod question;
mod recalled;
mod runs;
mod search;

#[cfg(doctest)]
#[path = "boundary.rs"]
mod read_path_boundary;

pub use ancestry::ancestry;
pub use answer::{
    Answered, Backing, Board, DueMark, Failed, LeftOut, Narrowed, OwedPass, ReadyPass, TickCell,
    Verdict, ViewAnswer, ViewOutcome,
};
pub use jojobot_domain::memory::graph::{GraphQuery, Include, Object, Selected, Selection};
pub use jojobot_domain::memory::{EntityId, FieldAges, FieldBacking, MemoryError};
pub use list_ticks::TickRead;
pub use mail::{
    MailHandle, MailRail, OperatorIs, OperatorMail, OperatorMailCommands, RailMessages, Refused,
};
pub mod left_out;
pub mod words;
pub use question::{Asks, Question, Readiness, ViewRequest, parse_date};
pub use recalled::{RecalledObject, RecalledView};
pub use runs::{Runs, RunsAsk, RunsFailed, may_read_runs};
pub use search::{SearchQuestion, Searched};

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

    /// **A search by words, answered for the one asking**: the search verb's own
    /// prepared query, so a bot and a page ask the index one way.
    async fn search(
        &self,
        asker: &Actor,
        question: SearchQuestion,
    ) -> Result<Searched, MemoryError>;

    /// **Every entity the store holds, archived and folded ones included**, and
    /// the records of none of them. A page resolves a handle to its entity
    /// whatever became of it, so this list is total and costs one pass over the
    /// index: a recall reads each thing's records, so it costs a pass over all of
    /// them and omits the archived and folded. This read is the index read, kept
    /// for that reason, and a bot's question may use it too.
    async fn entity_index(
        &self,
        asker: &Actor,
    ) -> Result<Vec<jojobot_domain::memory::Entity>, MemoryError>;
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

    pub async fn search(
        &self,
        asker: &Actor,
        question: SearchQuestion,
    ) -> Result<Searched, MemoryError> {
        self.0.search(asker, question).await
    }

    /// **Every entity the store holds**, see [`Answerer::entity_index`].
    pub async fn entity_index(
        &self,
        asker: &Actor,
    ) -> Result<Vec<jojobot_domain::memory::Entity>, MemoryError> {
        self.0.entity_index(asker).await
    }
}
