//! **How a background task ends when the server stops.**
//!
//! A task spawned and forgotten keeps running after the stop signal, and one
//! that writes can write after the store's pool has closed. A task the server
//! owns is waited for instead: it is given a grace to finish, and abandoned at
//! its next await when it does not. An abandoned write is a dropped transaction,
//! which the store rolls back, and every task started here picks up what is
//! unfinished at the next start.

use std::time::Duration;

use tokio::task::JoinHandle;

/// How a task ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// It ran to its end inside the grace.
    Finished,
    /// It was still running when the grace ended and was aborted.
    Abandoned,
}

/// **Wait for `task` up to `grace`, then abort it and wait for the abort.** When
/// this returns the task holds nothing: it has finished or it is gone.
pub async fn finish_or_abandon(mut task: JoinHandle<()>, grace: Duration) -> Ended {
    match tokio::time::timeout(grace, &mut task).await {
        Ok(_) => Ended::Finished,
        Err(_) => {
            task.abort();
            let _ = task.await;
            Ended::Abandoned
        }
    }
}

/// **The writer's last act, then the store's stop, whichever way the last act
/// ends.** Every accepted write and the shutdown snapshot run as the writer's final
/// job, and `bound` is how long that job is given. A job still running at the bound
/// is let go: a cut snapshot costs the next boot a longer replay and nothing else,
/// while a store left to the service manager's kill never gets its own stop. The
/// writer keeps running the job it was left with; `stop` ends the store beneath it.
pub async fn finish_writer_then_stop(
    writer: &jojobot_adapters::writer::Writer,
    bound: Duration,
    last: impl std::future::Future<Output = ()> + Send + 'static,
    stop: impl std::future::Future<Output = ()>,
) -> Ended {
    let ended = match tokio::time::timeout(bound, writer.finish(last)).await {
        Ok(()) => Ended::Finished,
        Err(_) => {
            tracing::warn!(
                bound_seconds = bound.as_secs(),
                "the writer's final job outlasted its bound: the store stops anyway, and the \
                 next boot replays the tail of the log"
            );
            Ended::Abandoned
        }
    };
    stop.await;
    ended
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer the case's subscriber writes to, read back when the case asserts.
    #[derive(Clone, Default)]
    struct Kept(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
        type Writer = Kept;
        fn make_writer(&'a self) -> Kept {
            self.clone()
        }
    }

    impl Kept {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().expect("log buffer").clone()).expect("utf8")
        }
    }

    /// The subscriber for one case, held until the case ends. It is the thread's
    /// own, so it sees a case that runs on the test's single-threaded runtime.
    fn capture() -> (Kept, tracing::subscriber::DefaultGuard) {
        let kept = Kept::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(kept.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .finish();
        (kept, tracing::subscriber::set_default(subscriber))
    }

    /// **A task that ends inside the grace is waited for and reports finished**,
    /// with its work done.
    #[tokio::test]
    async fn a_task_that_ends_inside_the_grace_is_finished() {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writing = done.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            writing.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        assert_eq!(
            finish_or_abandon(task, Duration::from_secs(10)).await,
            Ended::Finished
        );
        assert!(done.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// **A task still running when the grace ends is aborted, and is gone when
    /// this returns.** The pair of the case above: without the abort this never
    /// returns.
    #[tokio::test]
    async fn a_task_past_the_grace_is_abandoned_and_gone() {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writing = done.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            writing.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        assert_eq!(
            finish_or_abandon(task, Duration::from_millis(100)).await,
            Ended::Abandoned
        );
        assert!(!done.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// **A final writer job that outlasts its bound is cut, and the store is still
    /// stopped.** The final job is every accepted write and then the shutdown
    /// snapshot, whose tail replay has no length of its own. Without a bound the
    /// stop waits behind it for as long as the service manager lets it, and the
    /// store is left to the manager's kill.
    #[tokio::test]
    async fn a_final_job_past_its_bound_is_cut_and_the_store_still_stops() {
        let (log, _subscriber) = capture();
        let writer = jojobot_adapters::writer::Writer::default();
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stopped.clone();
        let ended = tokio::time::timeout(
            Duration::from_secs(5),
            finish_writer_then_stop(
                &writer,
                Duration::from_millis(100),
                std::future::pending::<()>(),
                async move { stopping.store(true, std::sync::atomic::Ordering::SeqCst) },
            ),
        )
        .await
        .expect("the stop did not wait behind a final job that never ends");
        assert_eq!(ended, Ended::Abandoned);
        assert!(
            stopped.load(std::sync::atomic::Ordering::SeqCst),
            "the store was not stopped after the cut"
        );
        // **The cut says so, as a warning that names the bound**, and the case
        // below ends with none: a cut nobody is told of reads as a stop that ran
        // its course.
        let text = log.text();
        assert!(
            text.contains("WARN") && text.contains("bound_seconds"),
            "the cut logged no warning naming its bound: {text:?}"
        );
    }

    /// **The pair of the case above**: a final job that ends inside its bound runs
    /// to its end, and the store is stopped after it.
    #[tokio::test]
    async fn a_final_job_inside_its_bound_runs_and_the_store_stops_after_it() {
        let (log, _subscriber) = capture();
        let writer = jojobot_adapters::writer::Writer::default();
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = ran.clone();
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (stopping, seen_ran) = (stopped.clone(), ran.clone());
        let ended = finish_writer_then_stop(
            &writer,
            Duration::from_secs(30),
            async move { running.store(true, std::sync::atomic::Ordering::SeqCst) },
            async move {
                assert!(
                    seen_ran.load(std::sync::atomic::Ordering::SeqCst),
                    "the store stopped before the final job ran"
                );
                stopping.store(true, std::sync::atomic::Ordering::SeqCst)
            },
        )
        .await;
        assert_eq!(ended, Ended::Finished);
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            !log.text().contains("WARN"),
            "a final job that ended inside its bound logged a warning: {:?}",
            log.text()
        );
    }
}
