//! **The build cache's hit rate for one run, read from sccache.**
//!
//! An sccache server counts compiles for as long as it lives, and a host may
//! share one server among many runs. So the bar reads the totals twice, before
//! its phases and after them, and reports the difference. It never zeroes the
//! counters: another run may be reading them. The line is a report. A failed
//! read says so and changes nothing else.

use std::process::{Command, Stdio};

use serde_json::Value;

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
    let output = Command::new(sccache)
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
