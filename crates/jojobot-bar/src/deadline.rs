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

/// The marker of an execution the bar cannot follow, left in the output before the
/// deadline runs from the line that named it.
const UNTRACKABLE: &str = "bar: untrackable:";

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

    /// The lines since the last call that claim an executable started, oldest first.
    fn starts(&mut self) -> std::io::Result<Vec<String>> {
        self.file.seek(SeekFrom::Start(self.position))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        self.position += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        let Some(end) = self.partial.rfind('\n') else {
            return Ok(Vec::new());
        };
        let whole: String = self.partial.drain(..=end).collect();
        Ok(whole
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("Running ") || line.starts_with("Doc-tests "))
            .map(str::to_string)
            .collect())
    }
}

/// **The path a `Running` line names**, the last parenthesis of the line. A
/// `Doc-tests` line names none.
fn claimed_path(line: &str) -> Option<&str> {
    line.trim_end()
        .strip_suffix(')')?
        .rsplit_once('(')
        .map(|(_, path)| path)
}

/// **One process the command started itself**, told apart from any other by its
/// number and the moment it began: a number is used again, a number with its start
/// is not.
struct Member {
    identity: (u32, u64),
    exe: Option<std::path::PathBuf>,
    name: String,
}

/// **The direct children of the command now.** Cargo starts each test binary and
/// each `rustdoc` itself, so they are its children; whatever a test binary starts is
/// the test's own and is never read as a start. Empty where `/proc` does not exist.
fn children_of(command: u32) -> Vec<Member> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            // `pid (name) state parent group ... start`: the name may hold spaces
            // and parentheses, so the fields are counted from its last parenthesis.
            // `rest` begins at field 3, so the parent is index 1 and field 22 is 19.
            let (head, rest) = stat.rsplit_once(')')?;
            let mut fields = rest.split_whitespace();
            let parent: u32 = fields.nth(1)?.parse().ok()?;
            let began: u64 = fields.nth(17)?.parse().ok()?;
            (parent == command).then(|| Member {
                identity: (pid, began),
                exe: std::fs::read_link(format!("/proc/{pid}/exe")).ok(),
                name: head
                    .split_once('(')
                    .map_or("", |(_, name)| name)
                    .to_string(),
            })
        })
        .collect()
}

/// **Whether a `Running` or `Doc-tests` line is an execution that started.** The
/// line is output, and a hung binary can print any line it likes, so the claim is
/// checked against what the command started itself: only a direct child of the
/// command can be a test binary or `rustdoc`, and each child is credited with one
/// start. A `Running` line counts when a child not yet credited runs the file it
/// names, whatever way the path is spelled, and a `Doc-tests` line when one runs as
/// `rustdoc`. A child is credited by its number and its start, so printing the line
/// again, or in another spelling, starts nothing, and a process a test starts is
/// not a child of the command at all. `Some` is a start, with the path the line
/// named. Without `/proc` a line is believed, as it was before.
fn started(
    line: &str,
    command: u32,
    credited: &mut std::collections::HashSet<(u32, u64)>,
) -> Option<Option<String>> {
    let path = claimed_path(line).map(str::to_string);
    if !std::path::Path::new("/proc/self").exists() {
        return Some(path);
    }
    let file = match &path {
        Some(named) => Some(std::fs::canonicalize(named).ok()?),
        None if line.starts_with("Doc-tests ") => None,
        None => return None,
    };
    let member = children_of(command).into_iter().find(|member| {
        !credited.contains(&member.identity)
            && match &file {
                Some(file) => member.exe.as_ref() == Some(file),
                None => member.name == "rustdoc",
            }
    })?;
    credited.insert(member.identity);
    Some(path)
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
    // A line that claims a start waits a few seconds for the process it names.
    let mut claims: Vec<(String, Instant)> = Vec::new();
    let mut credited = std::collections::HashSet::new();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.success()));
        }
        if let Some(watch) = watch.as_mut() {
            let now = Instant::now();
            claims.extend(watch.starts()?.into_iter().map(|line| (line, now)));
            // A hung binary can print lines faster than they are judged.
            let surplus = claims.len().saturating_sub(64);
            claims.drain(..surplus);
        }
        for (line, seen) in std::mem::take(&mut claims) {
            match started(&line, child.id(), &mut credited) {
                Some(_) => {
                    since = Some(Instant::now());
                    running = line;
                }
                None if seen.elapsed() < Duration::from_secs(5) => claims.push((line, seen)),
                // **Named by cargo and run by no child of the command.** The test sits
                // behind a runner or wrapper, or it ended before it was seen. A run with
                // no start yet is not left to the job's bound: its deadline runs from
                // that line, and the log says so. A later line never moves a start.
                None if since.is_none() => {
                    since = Some(seen);
                    writeln!(
                        &*note,
                        "\n{UNTRACKABLE} {label}: {line} is run by no child of the command, \
                         so its deadline runs from that line"
                    )?;
                    running = line;
                }
                None => {}
            }
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

    /// **A hung test that starts a fresh helper and prints a start line for it does
    /// not move its own deadline.** The command starts one test binary and names it;
    /// the test starts a new `sleep` each time round and prints a `Running` line for
    /// it, also in another spelling, and for a file nobody runs. Only the command's
    /// own child can start the clock, so the run is killed at the deadline of the
    /// first start.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hung_test_that_starts_fresh_helpers_and_names_them_is_still_killed() {
        let (path, out) = output("fake-running");
        let shell = std::fs::canonicalize("/bin/sh").unwrap();
        let sleep = std::fs::canonicalize(
            Command::new("sh")
                .args(["-c", "command -v sleep"])
                .output()
                .map(|found| String::from_utf8_lossy(&found.stdout).trim().to_string())
                .unwrap(),
        )
        .unwrap();
        let test = format!(
            "n=0; while [ $n -lt 150 ]; do n=$((n + 1)); sleep 0.5 & \
               echo '     Running unittests b ({})'; \
               echo '     Running unittests c ({})'; \
               echo '     Running unittests d (/no/such/binary)'; \
               sleep 0.1; done; wait",
            sleep.display(),
            sleep.display().to_string().replacen('/', "//", 1)
        );
        let file = std::env::temp_dir().join(crate::scratch::name("fake-test-binary"));
        std::fs::write(&file, test).unwrap();
        let script = format!(
            "/bin/sh {} & echo '     Running unittests a ({})'; wait",
            file.display(),
            shell.display()
        );
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .stdout(out.try_clone().unwrap())
            .stderr(out.try_clone().unwrap());
        let began = Instant::now();
        let ran = run_job_for(
            Duration::from_secs(2),
            &mut command,
            None,
            &path,
            &out,
            "hung",
            Clock::PerExecutable,
        )
        .unwrap();
        assert_eq!(ran, Some(false));
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("TIMED OUT")
        );
        let _ = std::fs::remove_file(path);
    }

    /// **A test behind a wrapper is not left to the job's bound.** The command starts
    /// a wrapper that stays alive and starts the sleeping binary the line names
    /// beneath it, so no child of the command runs that file. The run must still end
    /// near its deadline, and the log must say the execution was untrackable.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_test_behind_a_wrapper_is_refused_promptly_and_named_in_the_log() {
        let (path, out) = output("wrapped");
        let found = Command::new("sh")
            .args(["-c", "command -v sleep"])
            .output()
            .map(|found| String::from_utf8_lossy(&found.stdout).trim().to_string())
            .unwrap();
        // The program is run by the name it was found under, and named by its file.
        let named = std::fs::canonicalize(&found).unwrap();
        let wrapper = std::env::temp_dir().join(crate::scratch::name("wrapper"));
        std::fs::write(&wrapper, format!("{found} 25 & wait\n")).unwrap();
        let script = format!(
            "/bin/sh {} & echo '     Running unittests a ({})'; wait",
            wrapper.display(),
            named.display()
        );
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .stdout(out.try_clone().unwrap())
            .stderr(out.try_clone().unwrap());
        let began = Instant::now();
        let ran = run_job_for(
            Duration::from_secs(2),
            &mut command,
            None,
            &path,
            &out,
            "wrapped",
            Clock::PerExecutable,
        )
        .unwrap();
        assert_eq!(ran, Some(false));
        assert!(
            began.elapsed() < Duration::from_secs(12),
            "{:?}",
            began.elapsed()
        );
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(
            log.contains(UNTRACKABLE) && log.contains("TIMED OUT"),
            "{log}"
        );
        let _ = std::fs::remove_file(path);
    }

    /// **Doc-tests lines do not move the deadline of a hung `rustdoc`.** One
    /// `rustdoc` process is credited with one start however many `Doc-tests` lines
    /// the command prints, so the run is killed at the deadline of the first.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hung_rustdoc_that_prints_doc_tests_lines_is_still_killed() {
        let (path, out) = output("fake-doc-tests");
        let dir = std::env::temp_dir().join(crate::scratch::name("fake-rustdoc"));
        std::fs::create_dir_all(&dir).unwrap();
        // A process named `rustdoc`: a link to a real program, run by that name.
        let python = std::fs::canonicalize(
            Command::new("sh")
                .args(["-c", "command -v python3"])
                .output()
                .map(|found| String::from_utf8_lossy(&found.stdout).trim().to_string())
                .unwrap(),
        )
        .unwrap();
        let link = dir.join("rustdoc");
        std::os::unix::fs::symlink(&python, &link).unwrap();
        let script = format!(
            "{} -c 'import time; time.sleep(20)' & n=0; \
             while [ $n -lt 150 ]; do n=$((n + 1)); echo '   Doc-tests the_crate'; \
               sleep 0.1; done; wait",
            link.display()
        );
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .stdout(out.try_clone().unwrap())
            .stderr(out.try_clone().unwrap());
        let began = Instant::now();
        let ran = run_job_for(
            Duration::from_secs(2),
            &mut command,
            None,
            &path,
            &out,
            "hung",
            Clock::PerExecutable,
        )
        .unwrap();
        assert_eq!(ran, Some(false));
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("TIMED OUT")
        );
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_file(path);
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
