//! **The build cache's hit rate for one run, read from sccache.**
//!
//! An sccache server counts every compile it serves, and a host may share one
//! server among many runs, so its totals say nothing about one run. **The run
//! counts on a server of its own:** unless the environment already names a
//! server (the CI job wrapper names one per job), the bar names a socket, and
//! its build children and its stats reads all use that socket. The compiles in
//! the totals are then this run's. The bar stops that server when the run ends,
//! whichever target it ran. A run with no server of its own, named or private,
//! reads no rate: it never reads a server it shares. The totals are read twice
//! and subtracted, which is exact for a server that starts empty and right for
//! one the job already named. The line is a report. A failed read says so and
//! changes nothing else.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::Value;

/// The socket of the server this run started, when it started one.
static PRIVATE_SOCKET: OnceLock<PathBuf> = OnceLock::new();

/// A socket path holds at most 107 bytes.
const SOCKET_PATH_MAX: usize = 100;

/// A server stops by itself after this many idle seconds, and the run stops it
/// first. Long enough that a quiet phase never drops the counts.
const IDLE_SECONDS: &str = "7200";

/// Name a private server for the run rooted at `run_root`, unless one is already
/// named or this host has no sccache.
pub fn use_private_server(run_root: &Path) {
    if std::env::var_os("SCCACHE_SERVER_UDS").is_some()
        || std::env::var_os("JOJOBOT_SCCACHE").is_none()
    {
        return;
    }
    let id = run_root.file_name().unwrap_or_default().to_string_lossy();
    if let Some(socket) = endpoint(run_root, &std::env::temp_dir(), &id) {
        let _ = PRIVATE_SOCKET.set(socket);
    }
}

/// The socket of a private server: in the run root when that path is short
/// enough, else in the temp directory under the run's own name. None when no
/// path is short enough.
fn endpoint(run_root: &Path, temp: &Path, id: &str) -> Option<PathBuf> {
    [
        run_root.join("sccache.sock"),
        temp.join(format!("{id}.sock")),
    ]
    .into_iter()
    .find(|socket| socket.as_os_str().len() <= SOCKET_PATH_MAX)
}

/// Point a command at this run's server, when it has one.
pub fn on_this_runs_server(command: &mut Command) -> &mut Command {
    match PRIVATE_SOCKET.get() {
        Some(socket) => command
            .env("SCCACHE_SERVER_UDS", socket)
            .env("SCCACHE_IDLE_TIMEOUT", IDLE_SECONDS),
        None => command,
    }
}

/// Stop the server this run started and remove its socket. A run that started
/// none stops nothing, and a server the environment names is never stopped.
pub fn stop_this_runs_server() {
    let (Some(sccache), Some(socket)) = (std::env::var_os("JOJOBOT_SCCACHE"), PRIVATE_SOCKET.get())
    else {
        return;
    };
    let _ = on_this_runs_server(&mut Command::new(sccache))
        .arg("--stop-server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = std::fs::remove_file(socket);
}

/// Whether the totals of the server this process would reach are this run's.
fn has_own_server(private: Option<&PathBuf>, named: bool) -> bool {
    private.is_some() || named
}

/// The compiles the cache answered and the ones it did not.
#[derive(Clone, Copy)]
pub struct Totals {
    hits: u64,
    misses: u64,
}

/// The totals of the sccache named by `JOJOBOT_SCCACHE`, which the dev shell
/// sets on a host that uses sccache. `None` is any reason the totals cannot be
/// read, and a run with no server of its own among them.
pub fn read() -> Option<Totals> {
    let sccache = std::env::var_os("JOJOBOT_SCCACHE")?;
    if !has_own_server(
        PRIVATE_SOCKET.get(),
        std::env::var_os("SCCACHE_SERVER_UDS").is_some(),
    ) {
        return None;
    }
    let output = on_this_runs_server(&mut Command::new(sccache))
        .args(["--show-stats", "--stats-format", "json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}

fn parse(text: &str) -> Option<Totals> {
    let stats = serde_json::from_str::<Value>(text).ok()?;
    let sum = |key: &str| -> Option<u64> {
        let counts = stats.get("stats")?.get(key)?.get("counts")?.as_object()?;
        Some(counts.values().filter_map(Value::as_u64).sum())
    };
    Some(Totals {
        hits: sum("cache_hits")?,
        misses: sum("cache_misses")?,
    })
}

/// The report line for the compiles between two reads.
pub fn line(before: Option<Totals>, after: Option<Totals>) -> String {
    let (Some(before), Some(after)) = (before, after) else {
        let why = match std::env::var_os("JOJOBOT_SCCACHE") {
            Some(_) => "sccache gave no totals of this run's own",
            None => "no sccache on this host",
        };
        return format!("sccache: hit rate not read ({why})\n");
    };
    let hits = after.hits.saturating_sub(before.hits);
    let misses = after.misses.saturating_sub(before.misses);
    match hits + misses {
        0 => "sccache: no cacheable compile in this run\n".to_string(),
        total => format!(
            "sccache: hit rate {}% ({hits} hits, {misses} misses)\n",
            (hits * 100 + total / 2) / total
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run root a disk placement made in a test of this crate: the scratch
    /// naming format with one-digit numbers.
    const LONG_ROOT: &str = "/tmp/jojobot-scratch-1-0-jojobot-bar-sccache-test-private/jojobot-bar-1-1793580000000000000";

    #[test]
    fn sccache_a_short_run_root_holds_the_socket() {
        let root = Path::new("/dev/shm/jojobot-bar-1-1793580000000000000");
        let socket = endpoint(root, Path::new("/tmp"), "id").expect("a short root has an endpoint");
        assert_eq!(socket, root.join("sccache.sock"));
    }

    #[test]
    fn sccache_long_run_root_never_reads_shared_totals() {
        let socket = endpoint(
            Path::new(LONG_ROOT),
            Path::new("/tmp"),
            "jojobot-bar-1-1793580000000000000",
        )
        .expect("a long root still gets a private endpoint");
        assert!(socket.as_os_str().len() <= SOCKET_PATH_MAX, "{socket:?}");
        assert!(socket.starts_with("/tmp"), "{socket:?}");
    }

    #[test]
    fn sccache_no_path_short_enough_names_no_endpoint() {
        let temp = PathBuf::from(format!("/tmp/{}", "d".repeat(120)));
        assert_eq!(endpoint(Path::new(LONG_ROOT), &temp, "id"), None);
    }

    #[test]
    fn sccache_a_run_with_no_server_of_its_own_reads_none() {
        let socket = PathBuf::from("/tmp/x.sock");
        assert!(!has_own_server(None, false));
        assert!(has_own_server(Some(&socket), false));
        assert!(has_own_server(None, true));
    }
}
