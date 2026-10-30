//! The read-only boundary shared by the verbs and their other renderers.

mod answer;
mod list_ticks;
mod mail;
mod question;

#[cfg(doctest)]
#[path = "boundary.rs"]
mod read_path_boundary;

pub use answer::{
    Answered, Backing, Failed, LeftOut, Narrowed, OwedPass, ReadyPass, TickCell, Verdict,
};
pub use jojobot_domain::memory::graph::{GraphQuery, Include, Object, Selected, Selection};
pub use jojobot_domain::memory::{EntityId, FieldAges, FieldBacking, MemoryError};
pub use list_ticks::TickRead;
pub use mail::{MailHandle, MailRail, OperatorIs, OperatorMail, OperatorMailCommands, Refused};
pub use question::{Asks, Question, Readiness, parse_date};

use std::sync::Arc;

/// The implementation is supplied by the composition root. It exposes one read
/// and no way to retrieve the implementation or its dependencies.
#[async_trait::async_trait]
pub trait Answerer: Send + Sync {
    async fn ask(&self, question: Question) -> Result<Answered, Failed>;
}

#[derive(Clone)]
pub struct QueryHandle(Arc<dyn Answerer>);

impl From<Arc<dyn Answerer>> for QueryHandle {
    fn from(answerer: Arc<dyn Answerer>) -> Self {
        Self(answerer)
    }
}

impl QueryHandle {
    pub async fn ask(&self, question: Question) -> Result<Answered, Failed> {
        self.0.ask(question).await
    }
}
