//! **The deadline of one test binary.** A binary that hangs used to hold a runner
//! until the job's own timeout and leave nothing behind. A binary that is still
//! running at its deadline is killed with the processes it started, the job is red,
//! and the log says which binary it was. `main.rs` routes here.

use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
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

/// **The deadline a value sets**: a finite number of seconds above zero that a
/// `Duration` can hold. Anything else, `inf`, `NaN`, a negative, zero, a number too
/// large or text that is no number, leaves [`BINARY_DEADLINE`].
pub fn parse_deadline(value: Option<&str>) -> Duration {
    value
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
        .unwrap_or(BINARY_DEADLINE)
}

/// The deadline this run uses, from [`DEADLINE_VARIABLE`].
pub fn deadline() -> Duration {
    parse_deadline(std::env::var(DEADLINE_VARIABLE).ok().as_deref())
}

/// **What the clock measures.** A bundled binary is one executable, so its clock
/// starts when it does. A `cargo test` command compiles and then runs one or more
/// executables, and prints a `Running` line (or `Doc-tests`) as each starts: its
/// clock starts at that line and starts again at the next one, so compiling and
/// the sum of several honest executables never count against one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    FromSpawn,
    PerExecutable,
}

/// **The `Running` lines a command has printed into its output file**, read as the
/// file grows.
struct Watch {
    file: File,
    position: u64,
    partial: String,
}

impl Watch {
    fn new(path: &Path) -> std::io::Result<Watch> {
        Ok(Watch {
            file: File::open(path)?,
            position: 0,
            partial: String::new(),
        })
    }

    /// The newest line since the last call that says an executable started.
    fn newest_start(&mut self) -> std::io::Result<Option<String>> {
        self.file.seek(SeekFrom::Start(self.position))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        self.position += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        let Some(end) = self.partial.rfind('\n') else {
            return Ok(None);
        };
        let whole: String = self.partial.drain(..=end).collect();
        Ok(whole
            .lines()
            .map(str::trim)
            .rfind(|line| line.starts_with("Running ") || line.starts_with("Doc-tests "))
            .map(str::to_string))
    }
}

/// **Run one command to its end, to its deadline, or to a stop.** The command
/// leads a process group of its own, so the test binaries a `cargo test` started
/// die with it. `Some` is whether it exited zero; `None` is a command killed
/// because another job was red. **A stop wins over a deadline**: a command killed
/// by both is not red. A command that passes its deadline is killed and is
/// `Some(false)`: a line naming `label` is written to `note` after whatever the
/// command printed. **`note` must share its open file description with the
/// command's output**, as a clone of that handle does, so a later writer goes on
/// after the line and never over it.
pub fn run_job(
    command: &mut Command,
    stop: Option<&AtomicBool>,
    output: &Path,
    note: &File,
    label: &str,
    clock: Clock,
) -> std::io::Result<Option<bool>> {
    run_job_for(deadline(), command, stop, output, note, label, clock)
}

/// [`run_job`] with the deadline given.
pub fn run_job_for(
    limit: Duration,
    command: &mut Command,
    stop: Option<&AtomicBool>,
    output: &Path,
    note: &File,
    label: &str,
    clock: Clock,
) -> std::io::Result<Option<bool>> {
    use std::os::unix::process::CommandExt;
    let mut watch = (clock == Clock::PerExecutable)
        .then(|| Watch::new(output))
        .transpose()?;
    let mut child = command.process_group(0).spawn()?;
    let mut since = (clock == Clock::FromSpawn).then(Instant::now);
    let mut running = String::new();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.success()));
        }
        if let Some(watch) = watch.as_mut()
            && let Some(line) = watch.newest_start()?
        {
            since = Some(Instant::now());
            running = line;
        }
        let stopped = stop.is_some_and(|stop| stop.load(Ordering::SeqCst));
        let late = since.is_some_and(|since| since.elapsed() >= limit);
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
            let what = if running.is_empty() {
                label.to_string()
            } else {
                format!("{label} ({running})")
            };
            writeln!(
                &*note,
                "\n{MARKER} {what} was still running after {}s and was killed",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// **The override is a finite positive number of seconds, or it is ignored.**
    /// `inf` and a number past what a `Duration` holds used to panic.
    #[test]
    fn a_value_that_is_no_deadline_leaves_the_default() {
        for value in ["inf", "-inf", "NaN", "-1", "0", "abc", "", "1e999", "1e30"] {
            assert_eq!(parse_deadline(Some(value)), BINARY_DEADLINE, "{value:?}");
        }
        assert_eq!(parse_deadline(None), BINARY_DEADLINE);
        assert_eq!(parse_deadline(Some("2.5")), Duration::from_millis(2500));
        assert_eq!(parse_deadline(Some(" 3 ")), Duration::from_secs(3));
    }

    /// A scratch output file and a second handle that shares its offset.
    fn output(tag: &str) -> (std::path::PathBuf, File) {
        let path = std::env::temp_dir().join(crate::scratch::name(&format!("deadline-{tag}")));
        (path.clone(), File::create(path).unwrap())
    }

    /// **A stop wins over a deadline.** A command that both a stop and its deadline
    /// reach is killed as not run and leaves no marker, so it is not charged.
    #[test]
    fn a_stop_wins_over_a_deadline() {
        let (path, out) = output("stop");
        let stop = AtomicBool::new(true);
        let mut command = Command::new("sleep");
        command
            .arg("30")
            .stdout(out.try_clone().unwrap())
            .stderr(out.try_clone().unwrap());
        let began = Instant::now();
        let ran = run_job_for(
            Duration::ZERO,
            &mut command,
            Some(&stop),
            &path,
            &out,
            "sleep",
            Clock::FromSpawn,
        )
        .unwrap();
        assert_eq!(ran, None);
        assert!(began.elapsed() < Duration::from_secs(10));
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("TIMED OUT")
        );
        let _ = std::fs::remove_file(path);
    }
}
