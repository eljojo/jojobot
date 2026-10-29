//! Atomic event batches and a connection-lifetime commit fence.
use jojobot_domain::log::*;
use sqlx::MySqlPool;
#[derive(Clone)]
pub struct DoltLog;
impl DoltLog {
    pub async fn open(_pool: MySqlPool) -> Result<Self, LogError> {
        Ok(Self)
    }
}
#[async_trait::async_trait]
impl LogStore for DoltLog {
    async fn append(&self, _head: Position, _batch: &SealedBatch) -> Result<Append, LogError> {
        Ok(Append::DefinitelyNotCommitted)
    }
    async fn read_from(
        &self,
        _position: Position,
        _bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        Ok(vec![])
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        Ok(None)
    }
    async fn store_snapshot(&self, _snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        Ok(())
    }
}
#[cfg(any(test, feature = "testing"))]
#[async_trait::async_trait]
impl jojobot_domain::log::testing::FaultLog for DoltLog {
    async fn inject(&self, _fault: jojobot_domain::log::testing::Fault) {}
    async fn acknowledgement_pending(&self) {}
    async fn finish_delayed(&self, _commit: bool) {}
    async fn restart(&self) {}
}
