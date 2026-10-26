//! One owner of prepare, commit and publication across the serving rails.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use jojobot_domain::request::context::Context;
use tokio::sync::{mpsc, oneshot};
use tracing::Instrument;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

// Ports borrow their arguments; an accepted write must own them after the caller
// stops waiting. A nested rail already runs in this same owner.
macro_rules! owned {
    ($port:expr, $this:ident, [$($name:ident = $value:expr),* $(,)?], $work:expr) => {
        if !$port.writer.inside() {
            let $this = $port.clone();
            $(let $name = $value;)*
            return $port.writer.run(async move { $work }).await;
        }
    };
}
pub(crate) use owned;

tokio::task_local! {
    static CURRENT: u64;
}

struct Driver {
    id: u64,
    sender: OnceLock<mpsc::UnboundedSender<Job>>,
    #[cfg(any(test, feature = "testing"))]
    submitted: AtomicU64,
}

/// Clones submit to the same task. The serving process shares this owner across
/// its rails; dropping an answer never drops an accepted write.
#[derive(Clone)]
pub struct Writer(Arc<Driver>);

impl Default for Writer {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(Arc::new(Driver {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            sender: OnceLock::new(),
            #[cfg(any(test, feature = "testing"))]
            submitted: AtomicU64::new(0),
        }))
    }
}

impl Writer {
    /// A nested call shares the current write rather than queueing behind it.
    pub(crate) fn inside(&self) -> bool {
        CURRENT.try_with(|id| *id == self.0.id).unwrap_or(false)
    }

    /// Open the owner lazily so port assembly needs no executor. The queue owns
    /// the complete operation, including publication after commit.
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
    ) -> T {
        if self.inside() {
            return work.await;
        }
        let sender = self.0.sender.get_or_init(|| {
            let (sender, mut jobs) = mpsc::unbounded_channel::<Job>();
            let id = self.0.id;
            tokio::spawn(CURRENT.scope(id, async move {
                while let Some(job) = jobs.recv().await {
                    job.await;
                }
            }));
            sender
        });
        let context = Context::capture();
        let span = tracing::Span::current();
        let (answer, waiting) = oneshot::channel();
        #[cfg(any(test, feature = "testing"))]
        self.0.submitted.fetch_add(1, Ordering::SeqCst);
        sender
            .send(Box::pin(async move {
                let result = context.run(work).instrument(span).await;
                let _ = answer.send(result);
            }))
            .unwrap_or_else(|_| panic!("the writer task stopped before accepting its write"));
        waiting
            .await
            .expect("the writer task stopped during a write")
    }

    /// Wait for accepted writes before stopping their store.
    pub async fn drain(&self) {
        self.run(async {}).await;
    }

    /// Let a fixture observe queue admission without a timer.
    #[cfg(any(test, feature = "testing"))]
    pub fn submitted_jobs(&self) -> u64 {
        self.0.submitted.load(Ordering::SeqCst)
    }
}
