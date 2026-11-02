//! **The build cache's hit rate for one run, read from sccache.**
//!
//! An sccache server counts every compile it serves, and a host may share one
//! server among many runs, so its totals say nothing about one run. **The run
//! counts on a server of its own:** unless the environment already names a
//! server (the CI job wrapper names one per job), the bar names a socket in its
//! run root, and its build children and its stats reads all use that socket. The
//! compiles in the totals are then this run's. The bar stops that server when
//! the run ends. The totals are still read twice and subtracted, which is exact
//! for a server that starts empty and right for one the job already named. The
//! line is a report. A failed read says so and changes nothing else.

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

/// Name a private server in `run_root`, unless one is already named or this host
/// has no sccache.
pub fn use_private_server(run_root: &Path) {
    if std::env::var_os("SCCACHE_SERVER_UDS").is_some()
        || std::env::var_os("JOJOBOT_SCCACHE").is_none()
    {
        return;
    }
    let socket = run_root.join("sccache.sock");
    if socket.as_os_str().len() <= SOCKET_PATH_MAX {
        let _ = PRIVATE_SOCKET.set(socket);
    }
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

/// Stop the server this run started. A run that started none stops nothing.
pub fn stop_this_runs_server() {
    let (Some(sccache), Some(_)) = (std::env::var_os("JOJOBOT_SCCACHE"), PRIVATE_SOCKET.get())
    else {
        return;
    };
    let _ = on_this_runs_server(&mut Command::new(sccache))
        .arg("--stop-server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The compiles the cache answered and the ones it did not.
#[derive(Clone, Copy)]
pub struct Totals {
    hits: u64,
    misses: u64,
}

/// The totals of the sccache named by `JOJOBOT_SCCACHE`, which the dev shell
/// sets on a host that uses sccache. `None` is any reason the totals cannot be read.
pub fn read() -> Option<Totals> {
    let sccache = std::env::var_os("JOJOBOT_SCCACHE")?;
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
            Some(_) => "sccache gave no totals",
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
