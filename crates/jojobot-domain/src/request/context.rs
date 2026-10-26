//! The request and part travel with a write onto its owned task.

use super::*;

/// An accepted write keeps the caller's claim after its answer is dropped.
/// Bookkeeping writes that arrive unarmed remain unarmed.
pub struct Context {
    current: Option<Arc<Scope>>,
    armed: bool,
    part: Option<Armed>,
}

impl Context {
    /// Capture the task-local claim before handing a write to its owner.
    pub fn capture() -> Self {
        Self {
            current: CURRENT.try_with(Arc::clone).ok(),
            armed: ARMED.try_with(|()| ()).is_ok(),
            part: PART.try_with(Clone::clone).ok(),
        }
    }

    /// Run under the claim and part that accompanied this write.
    pub async fn run<F: Future>(self, work: F) -> F::Output {
        let work = async {
            if self.armed {
                ARMED.scope((), work).await
            } else {
                work.await
            }
        };
        let work = async {
            if let Some(part) = self.part {
                PART.scope(part, work).await
            } else {
                work.await
            }
        };
        if let Some(current) = self.current {
            CURRENT.scope(current, work).await
        } else {
            work.await
        }
    }
}
