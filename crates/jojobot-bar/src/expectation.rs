//! **What a bar says about itself before it does anything**: how long this
//! target is expected to take, and what a caller that runs it in the background
//! has to allow for.
//!
//! A tool call that runs the bar in the background is killed at its own timeout
//! unless the caller raised it, and a killed bar leaves no verdict and no word
//! about why. The caller cannot know the length unless the bar says it, so the
//! bar says it first, from the runs it has recorded.

use crate::reading::History;

/// **The timeout a background run needs, in milliseconds.** The longest a tool
/// call may be given; the slowest target is well inside it.
pub const BACKGROUND_TIMEOUT_MS: u64 = 7_200_000;

/// How many of a target's latest finished runs the expectation is taken from.
const RUNS_COUNTED: usize = 5;

/// **The two lines a target prints when it starts.** The first names how long
/// this target took in its recorded runs, the middle of the latest few so one
/// slow run does not move it, or says there is no history. The second names the
/// timeout and where the exit code is.
pub fn start_lines(target: &str, history: &History) -> [String; 2] {
    let first = match expected_wall_ms(target, history) {
        Some((ms, runs)) => format!(
            "bar: {target} is expected to take about {} (middle of the latest {runs} finished runs)",
            duration_text(ms)
        ),
        None => format!("bar: {target} has no history yet, so no expected time"),
    };
    let second = format!(
        "bar: run it in the background with a timeout of {BACKGROUND_TIMEOUT_MS} ms; \
         the exit code is the log's last line"
    );
    [first, second]
}

/// **The middle wall time of the target's latest finished runs, and how many
/// runs it came from.** `None` when no run of the target finished.
fn expected_wall_ms(target: &str, history: &History) -> Option<(u64, usize)> {
    let walls = history.walls_ms.get(target)?;
    let mut latest: Vec<u64> = walls[walls.len().saturating_sub(RUNS_COUNTED)..].to_vec();
    if latest.is_empty() {
        return None;
    }
    latest.sort_unstable();
    Some((latest[latest.len() / 2], latest.len()))
}

/// `1_320_000` → `22m`, `4_320_000` → `1h12m`, `45_000` → `45s`.
fn duration_text(ms: u64) -> String {
    let seconds = ms / 1000;
    match (seconds / 3600, seconds % 3600 / 60) {
        (0, 0) => format!("{seconds}s"),
        (0, minutes) => format!("{minutes}m"),
        (hours, minutes) => format!("{hours}h{minutes:02}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_of(level: &str, walls_ms: &[u64]) -> History {
        let mut history = History::default();
        history
            .walls_ms
            .insert(level.to_string(), walls_ms.to_vec());
        history
    }

    #[test]
    fn a_duration_is_said_in_the_largest_unit_that_fits() {
        assert_eq!(duration_text(45_000), "45s");
        assert_eq!(duration_text(1_320_000), "22m");
        assert_eq!(duration_text(4_320_000), "1h12m");
        assert_eq!(duration_text(3_660_000), "1h01m");
    }

    #[test]
    fn the_expectation_is_the_middle_of_the_latest_five_runs() {
        let minute = 60_000;
        // Seven runs: the two oldest are not counted, so a very old slow run
        // does not decide it. Latest five sorted: 10, 11, 12, 13, 90 -> 12.
        let walls = [
            500 * minute,
            400 * minute,
            90 * minute,
            10 * minute,
            12 * minute,
            11 * minute,
            13 * minute,
        ];
        assert_eq!(
            expected_wall_ms("check", &history_of("check", &walls)),
            Some((12 * minute, 5))
        );
    }

    #[test]
    fn a_target_with_no_finished_run_has_no_expectation() {
        assert_eq!(expected_wall_ms("check", &History::default()), None);
        assert_eq!(expected_wall_ms("check", &history_of("check", &[])), None);
        assert_eq!(
            expected_wall_ms("check", &history_of("rooms", &[60_000])),
            None
        );
    }
}
