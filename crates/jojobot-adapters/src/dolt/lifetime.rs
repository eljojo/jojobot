//! **How the store's server starts a group of its own and ends.**
//!
//! A service manager stops a unit by signalling every process of it, and a
//! terminal signals every process of a group. Both would reach the store at the
//! moment they reach jojobot, and a store that stops at the signal leaves the
//! drain window with nothing to finish its calls against. So the server leads a
//! process group of its own, and it ends only when jojobot ends it, after the
//! drain: first asked to stop, then killed if it has not stopped within a grace.

use std::time::Duration;

use sqlx::MySqlPool;
use tokio::process::{Child, Command};

/// How long the server is given to stop when asked, and how long the pool is
/// given to hand back its connections. Both are short: the drain is over.
pub(super) const STOP_GRACE: Duration = Duration::from_secs(5);

/// Start the server in a process group of its own.
pub(super) fn lead_a_group(spawning: &mut Command) {
    #[cfg(unix)]
    spawning.process_group(0);
    #[cfg(not(unix))]
    let _ = spawning;
}

/// **Close the pool, ask the server to stop, and kill it if it does not within
/// `grace`.** The kill is what the server always got before; the request ahead of
/// it lets the server close its files in order.
pub(super) async fn stop(pool: &MySqlPool, child: &mut Child, grace: Duration) {
    let _ = tokio::time::timeout(grace, pool.close()).await;
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: `kill` takes two integers and touches no memory.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if tokio::time::timeout(grace, child.wait()).await.is_ok() {
            return;
        }
    }
    let _ = child.kill().await;
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn pool() -> MySqlPool {
        sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://root@127.0.0.1:1/none")
            .expect("a lazy pool")
    }

    /// **The server's group is its own**, so a signal to jojobot's group does not
    /// reach it. Paired with a child spawned the ordinary way, which shares ours.
    #[tokio::test]
    async fn a_server_started_in_its_own_group_leads_it() {
        let ours = unsafe { libc::getpgid(0) };
        let mut apart = Command::new("sleep");
        apart.arg("30").kill_on_drop(true);
        lead_a_group(&mut apart);
        let apart = apart.spawn().expect("sleep runs");
        let mut beside = Command::new("sleep");
        beside.arg("30").kill_on_drop(true);
        let beside = beside.spawn().expect("sleep runs");

        let led = unsafe { libc::getpgid(apart.id().expect("a pid") as libc::pid_t) };
        let shared = unsafe { libc::getpgid(beside.id().expect("a pid") as libc::pid_t) };
        assert_eq!(
            led,
            apart.id().expect("a pid") as libc::pid_t,
            "it leads its own group"
        );
        assert_ne!(led, ours, "…which is not ours");
        assert_eq!(shared, ours, "an ordinary child shares ours");
    }

    /// **A server that stops when asked is gone well inside the grace.**
    #[tokio::test]
    async fn a_server_that_stops_when_asked_is_stopped_without_waiting_out_the_grace() {
        let mut serving = Command::new("sleep");
        serving.arg("60").kill_on_drop(true);
        let mut serving = serving.spawn().expect("sleep runs");
        let began = std::time::Instant::now();
        stop(&pool(), &mut serving, Duration::from_secs(30)).await;
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "it waited: {:?}",
            began.elapsed()
        );
        assert!(
            serving.try_wait().expect("readable").is_some(),
            "it is still running"
        );
    }

    /// **A server that ignores the request is killed when the grace ends.** The
    /// pair of the case above: without the kill this would never return.
    #[tokio::test]
    async fn a_server_that_ignores_the_request_is_killed_when_the_grace_ends() {
        let mut stubborn = Command::new("sh");
        stubborn
            .args(["-c", "trap '' TERM; while true; do sleep 1; done"])
            .kill_on_drop(true);
        let mut stubborn = stubborn.spawn().expect("sh runs");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let began = std::time::Instant::now();
        stop(&pool(), &mut stubborn, Duration::from_millis(500)).await;
        assert!(
            began.elapsed() >= Duration::from_millis(500),
            "it was killed before the grace"
        );
        assert!(
            stubborn.try_wait().expect("readable").is_some(),
            "it is still running"
        );
    }
}
