//! Forward the Teachings port without losing its overrides.

use super::{Observed, Operation, TeachingOperation};
use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};

#[async_trait]
impl Teachings for Observed<dyn Teachings> {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        self.result(
            Operation::Teachings(TeachingOperation::FirstContact),
            self.inner.first_contact(sid, domain, at),
        )
        .await
    }
    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.result(
            Operation::Teachings(TeachingOperation::HasContact),
            self.inner.has_contact(sid, domain),
        )
        .await
    }
    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.result(
            Operation::Teachings(TeachingOperation::Forget),
            self.inner.forget(sid),
        )
        .await
    }
    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.result(
            Operation::Teachings(TeachingOperation::Taught),
            self.inner.taught(),
        )
        .await
    }
}
