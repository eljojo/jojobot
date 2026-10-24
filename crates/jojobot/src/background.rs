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

#[cfg(test)]
mod tests {
    use super::*;

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
}
