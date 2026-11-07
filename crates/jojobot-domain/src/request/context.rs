//! The request and part travel with a write onto its owned task.

use super::*;

/// An accepted write keeps the caller's claim after its answer is dropped.
/// Bookkeeping writes that arrive unarmed remain unarmed.
pub struct Context {
    current: Option<Arc<Scope>>,
    armed: bool,
    part: Option<Armed>,
    /// The capture job of the call, when it carries one.
    capture: Option<crate::log::claims::job::CaptureJob>,
}

impl Context {
    /// Capture the task-local claim before handing a write to its owner.
    pub fn capture() -> Self {
        Self {
            current: CURRENT.try_with(Arc::clone).ok(),
            armed: ARMED.try_with(|()| ()).is_ok(),
            part: PART.try_with(Clone::clone).ok(),
            capture: crate::log::claims::job::current(),
        }
    }

    /// Run under the claim and part that accompanied this write.
    pub async fn run<F: Future>(self, work: F) -> F::Output {
        // Boxed so the wrapper holds a pointer and not a second copy of `work`: a
        // future is held by value in debug builds, and a write's `work` can be large
        // enough that one more copy overflows a 2 MiB test stack.
        let work = Box::pin(crate::log::claims::job::carried(self.capture, work));
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
