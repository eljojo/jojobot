use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct InMemoryLog(Arc<Shared>);

#[derive(Default)]
struct Shared {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    batches: Vec<SealedBatch>,
    snapshot: Option<ValidatedSnapshot>,
    fault: Option<Fault>,
    pending: Option<SealedBatch>,
    acknowledgement_pending: bool,
    acknowledgement_waiter: Option<std::task::Waker>,
}

#[async_trait::async_trait]
impl LogStore for InMemoryLog {
    async fn append(
        &self,
        expected_head: Position,
        batch: &SealedBatch,
    ) -> Result<Append, LogError> {
        let cancel_ack = {
            let mut inner = self.0.inner.lock().unwrap();
            if expected_head != batch.header().expected_previous_position {
                return Err(LogError::Conflict);
            }
            if inner.pending.is_some() {
                return Ok(Append::Indeterminate);
            }
            if let Some(held) = inner
                .batches
                .iter()
                .find(|held| held.header().batch_id == batch.header().batch_id)
            {
                return if held == batch {
                    Ok(Append::Committed(held.range()))
                } else {
                    Err(LogError::Conflict)
                };
            }
            let head = inner
                .batches
                .last()
                .map(|b| b.range().last)
                .unwrap_or_default();
            if expected_head != head {
                return Err(LogError::Conflict);
            }
            let fault = inner.fault.take();
            if matches!(fault, Some(Fault::DelayedCommit)) {
                inner.pending = Some(batch.clone());
                return Ok(Append::Indeterminate);
            }
            let fail_between = matches!(fault, Some(Fault::BetweenMembers));
            let mut staged = Vec::new();
            for member in batch.members() {
                staged.push(member.clone());
                if fail_between && staged.len() == 1 {
                    return Ok(Append::DefinitelyNotCommitted);
                }
            }
            // Publication is one replacement under the fence, after all members.
            inner.batches.push(batch.clone());
            if matches!(fault, Some(Fault::LostAcknowledgement)) {
                return Ok(Append::Indeterminate);
            }
            inner.acknowledgement_pending = matches!(fault, Some(Fault::CancelledAcknowledgement));
            inner.acknowledgement_pending
        };
        if cancel_ack {
            let waiter = self.0.inner.lock().unwrap().acknowledgement_waiter.take();
            if let Some(waiter) = waiter {
                waiter.wake();
            }
            std::future::pending::<()>().await;
        }
        Ok(Append::Committed(batch.range()))
    }

    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        let inner = self.0.inner.lock().unwrap();
        if inner.pending.is_some() {
            return Err(LogError::Indeterminate);
        }
        Ok(inner
            .batches
            .iter()
            .filter(|b| b.range().last > position)
            .take(bound)
            .cloned()
            .collect())
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        Ok(self.0.inner.lock().unwrap().snapshot.clone())
    }

    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.0.inner.lock().unwrap().snapshot = Some(snapshot.clone());
        Ok(())
    }
}

#[async_trait::async_trait]
impl FaultLog for InMemoryLog {
    async fn inject(&self, fault: Fault) {
        self.0.inner.lock().unwrap().fault = Some(fault);
    }

    async fn acknowledgement_pending(&self) {
        std::future::poll_fn(|cx| {
            let mut inner = self.0.inner.lock().unwrap();
            if inner.acknowledgement_pending {
                std::task::Poll::Ready(())
            } else {
                inner.acknowledgement_waiter = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        })
        .await;
    }

    async fn finish_delayed(&self, commit: bool) {
        let mut inner = self.0.inner.lock().unwrap();
        if let Some(batch) = inner.pending.take()
            && commit
        {
            inner.batches.push(batch);
        }
    }

    async fn restart(&self) {
        let mut inner = self.0.inner.lock().unwrap();
        // The old operation loses its capability to publish before a new writer starts.
        inner.pending = None;
        inner.fault = None;
        inner.acknowledgement_pending = false;
    }
}
