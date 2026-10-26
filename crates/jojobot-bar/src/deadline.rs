//! **The deadline of one test binary.** A binary that hangs used to hold a runner
//! until the job's own timeout and leave nothing behind. A binary that is still
//! running at its deadline is killed with the processes it started, the job is red,
//! and the log says which binary it was. `main.rs` routes here.

use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// **How long one test binary may run.** The slowest binary of a recent green run
/// took 545 s (the year room), and the slowest of a job at the report level took
/// 328 s, both on a loaded runner; twenty minutes is more than twice the slowest.
pub const BINARY_DEADLINE: Duration = Duration::from_secs(20 * 60);

/// **The variable that sets the deadline in seconds**, so a case can use one
/// second where a run uses twenty minutes.
pub const DEADLINE_VARIABLE: &str = "BAR_BINARY_DEADLINE_SECS";

/// The marker every timed-out binary leaves in its job's output.
const MARKER: &str = "bar: TIMED OUT:";

/// The deadline this run uses: the variable when it holds a positive number of
/// seconds, else [`BINARY_DEADLINE`].
pub fn deadline() -> Duration {
    std::env::var(DEADLINE_VARIABLE)
        .ok()
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .filter(|seconds| *seconds > 0.0)
        .map_or(BINARY_DEADLINE, Duration::from_secs_f64)
}

/// **Run one binary's command to its end, to its deadline, or to a stop.** The
/// command leads a process group of its own, so the test binaries a `cargo test`
/// started die with it. `Some` is whether it exited zero; `None` is a command
/// killed because another job was red. **A command that passes its deadline is
/// killed and is `Some(false)`**: it is red, and a line naming `label` is appended
/// to `output`, the file the command printed into, after whatever it had printed.
pub fn run_job(
    command: &mut Command,
    stop: Option<&AtomicBool>,
    output: &Path,
    label: &str,
) -> std::io::Result<Option<bool>> {
    use std::os::unix::process::CommandExt;
    let limit = deadline();
    let began = Instant::now();
    let mut child = command.process_group(0).spawn()?;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.success()));
        }
        let stopped = stop.is_some_and(|stop| stop.load(Ordering::SeqCst));
        let late = began.elapsed() >= limit;
        if stopped || late {
            // SAFETY: `killpg` takes a process group id and a signal, and reads
            // no memory of ours. The group is the one this child leads.
            unsafe {
                libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
            }
            child.wait()?;
            if stopped {
                return Ok(None);
            }
            let mut file = std::fs::OpenOptions::new().append(true).open(output)?;
            writeln!(
                file,
                "\n{MARKER} {label} was still running after {}s and was killed",
                limit.as_secs_f64()
            )?;
            return Ok(Some(false));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// **The binaries that timed out**, one verdict line each, read from the text the
/// jobs printed.
pub fn named(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix(MARKER))
        .map(|rest| format!("  TIMED OUT:{rest}"))
        .collect()
}
