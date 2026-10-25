//! Forward the Requests port without losing its overrides.

use super::{Observed, Operation, RequestOperation};
use async_trait::async_trait;
use jojobot_domain::memory::EntityId;
use jojobot_domain::request::{PartRow, Request, RequestError, RequestId, Requests, Stored};

#[async_trait]
impl Requests for Observed<dyn Requests> {
    async fn lookup(&self, bot: &EntityId, id: &RequestId) -> Result<Option<Stored>, RequestError> {
        self.result(
            Operation::Requests(RequestOperation::Lookup),
            self.inner.lookup(bot, id),
        )
        .await
    }
    async fn fill(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        self.result(
            Operation::Requests(RequestOperation::Fill),
            self.inner.fill(request, receipt),
        )
        .await
    }
    async fn parts(&self, bot: &EntityId, id: &RequestId) -> Result<Vec<PartRow>, RequestError> {
        self.result(
            Operation::Requests(RequestOperation::Parts),
            self.inner.parts(bot, id),
        )
        .await
    }
    async fn finish(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        self.result(
            Operation::Requests(RequestOperation::Finish),
            self.inner.finish(request, receipt),
        )
        .await
    }
}
