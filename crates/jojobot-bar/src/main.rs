//! `bar` — runs the green-bar phases (`fmt-check`, `test`, `lint`) and prints
//! a short verdict instead of the raw stream, with the full output kept at a
//! known path.
//!
//! **Why this exists rather than the Makefile calling cargo directly**: a
//! `cargo test --workspace` run prints thousands of lines, and a caller who
//! pipes that through `tail`/`head` to stay within its own context keeps only
//! the doc-tests and the clippy banner — the part that always looks fine —
//! while the suite counts, the failing names and the compile status are all
//! above the cut. This never pipes the run: it spawns cargo itself, writes
//! everything to the log, and prints the short version from what it captured
//! directly, so there is no pipe for an exit code to go missing in.
//!
//! **Deliberately silent while a phase runs.** `scripts/sabotage` tees its
//! run live because a person is watching ONE file mutate for a few seconds.
//! This wraps a run that can take minutes and whose whole point is that
//! nobody should have to read it as it happens — the summary at the end is
//! the answer, not a live feed of it.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use jojobot_bar::bundle::{self, Exe, Manifest};
use jojobot_bar::changes::{changed_paths, git_stdout, pushed_paths};
use jojobot_bar::dev;
use jojobot_bar::{
    CALL_USAGE, HISTORY_DIR_VARIABLE, KnownReds, ROSTER_FILE, Run, RunHeader, Summary,
    TEST_JOBS_VARIABLE, cargo_bin, contained_from_exit, history_root, is_a_revision, is_slow,
    iso_utc, job_args, jobs_limit, load_known_reds, new_roster_entries, parse_call, parse_job,
    read_history, render_roster_report, report_line, report_plan, roster_entries, run_id,
    start_lines, summarize_test_output, test_healthy, test_jobs, write_call,
};

mod local_upgrade;
mod plan;

/// **The tests that are red and already have a card**, read from the directory
/// the bar runs in, which is the repository root.
const KNOWN_REDS_FILE: &str = "known-reds.toml";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(target @ ("check" | "report" | "narrow" | "dev" | "rooms")) =
        args.first().map(String::as_str)
    {
        announce_expectation(target);
    }
    // **Refused before any phase runs**, like a limit that is not a number: a
    // list that cannot be read must not cost a build and then be ignored.
    let known = match load_known_reds(Path::new(KNOWN_REDS_FILE)) {
        Ok(known) => known,
        Err(refusal) => {
            eprintln!("bar: {refusal}");
            return ExitCode::from(2);
        }
    };
    // **Held until the run ends**, so the directory its children work in goes
    // with it, whatever those children left.
    let _run_root = if matches!(
        args.first().map(String::as_str),
        Some("check" | "report" | "narrow" | "rooms" | "dev")
    ) {
        sweep_orphaned_scratch();
        RunRoot::make()
    } else {
        None
    };
    let result = match args.first().map(String::as_str) {
        Some("check") => run_check(&known, Level::Boundary),
        Some("report") => run_check(&known, Level::Report),
        Some("narrow") => with_loop_wall_line(|| run_narrow(&args[1..], &known)),
        Some("dev") => with_loop_wall_line(|| run_dev(&args[1..], &known)),
        Some("rooms") => run_rooms(&args[1..], &known),
        Some("plan") => plan::run(&args[1..]),
        Some("size") => jojobot_bar::size::run(&args[1..]),
        Some("static") => jojobot_bar::static_checks::run(&args[1..]),
        Some("bundle") => run_bundle(&args[1..]),
        Some("call") => run_call(&args[1..]),
        _ => {
            eprintln!(
                "usage: bar check | bar report | bar dev | bar narrow --crate <name> [--filter <substring>] \
                 | bar rooms [--lane <name>] [--all-features] --test <name> [--test <name> ...] \
                 | bar size [--write | --init] | bar static | bar bundle [--room <name> ...] | bar call ..."
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bar: {e}");
            ExitCode::FAILURE
        }
    }
}

/// **Run a dev-loop target and end its output with its whole wall time**, on
/// every path it can leave by: a refusal and a red run are slow too.
fn with_loop_wall_line(
    run: impl FnOnce() -> std::io::Result<ExitCode>,
) -> std::io::Result<ExitCode> {
    let started = std::time::Instant::now();
    let result = run();
    println!("{}", jojobot_bar::loop_wall_line(started.elapsed()));
    result
}

/// **Say how long this target is expected to take and what a background run
/// has to allow for**, before anything else is printed. Every target that runs
/// tests starts through here, so a caller that has run the bar in the
/// background learns the timeout it needed from the log, not from a kill.
fn announce_expectation(target: &str) {
    let variable = std::env::var(HISTORY_DIR_VARIABLE).ok();
    let home = std::env::var("HOME").ok();
    let history = history_root(variable.as_deref(), home.as_deref())
        .map(|root| read_history(&root, &["check", "report", "rooms"]))
        .unwrap_or_default();
    for line in start_lines(target, &history) {
        println!("{line}");
    }
}

/// **Remove the scratch directories a killed run left behind**, before this
/// run makes its own. The directories to look in are the temp directory the run
/// was started with and, when that is a shell's own scratch directory, the one
/// that holds it and so holds the earlier shells' too: a killed run's leftovers
/// sit in whichever shell it started under. Said on stderr only when something
/// was found.
fn sweep_orphaned_scratch() {
    let temp = std::env::temp_dir();
    let parent = match temp.file_name() {
        Some(name) if name.to_string_lossy().starts_with("nix-shell.") => {
            temp.parent().map_or(temp.clone(), Path::to_path_buf)
        }
        _ => temp,
    };
    say_what_was_swept(
        &parent,
        jojobot_bar::sweep::sweep_orphans(&parent, &jojobot_bar::sweep::process_is_gone),
    );
    // **A run root left in memory costs memory until the host restarts**, so the
    // memory-backed directory is swept whichever base this run chooses. It is
    // shared and owned by root, so the sweep is of what this user made in it.
    let memory = Path::new(jojobot_bar::scratch::MEMORY_DIR);
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let user = unsafe { libc::geteuid() };
    say_what_was_swept(
        memory,
        jojobot_bar::sweep::sweep_orphans_of(memory, user, &jojobot_bar::sweep::process_is_gone),
    );
}

/// Said on stderr only when something was found.
fn say_what_was_swept(parent: &Path, swept: jojobot_bar::sweep::Swept) {
    if !(swept.removed.is_empty() && swept.in_use.is_empty() && swept.failed.is_empty()) {
        eprintln!(
            "bar: swept {} scratch directories whose owner is gone from {} ({} kept, a process works in them; {} could not be removed)",
            swept.removed.len(),
            parent.display(),
            swept.in_use.len(),
            swept.failed.len(),
        );
    }
}

/// The directory this run gives its children as their temp directory.
static RUN_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// **A temp directory for the build and test phases, removed when the run
/// ends.** The test processes make directories in their temp directory and do
/// not all remove them, and a killed one removes none. Pointing them all at one
/// directory of this run's own, and removing that, is what makes the cleanup
/// the run's and not each fixture's. A run that is killed leaves the directory,
/// and the sweep at the next start removes it by the process number in its name.
///
/// **It is made under the memory-backed directory when that has room** (see
/// [`jojobot_bar::scratch`]), so the test stores do not wait on a disk. This is
/// the one place the bar chooses it: build and test children get it through
/// [`in_the_run_root`]; read-only fmt and metadata inherit their caller's TMPDIR.
struct RunRoot(PathBuf);

impl RunRoot {
    fn make() -> Option<RunRoot> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let memory = Path::new(jojobot_bar::scratch::MEMORY_DIR);
        let placement = jojobot_bar::scratch::place(
            memory,
            jojobot_bar::scratch::free_bytes(memory),
            jojobot_bar::scratch::MINIMUM_FREE_BYTES,
            &std::env::temp_dir(),
        );
        if let jojobot_bar::scratch::Placement::Disk { base, reason } = &placement {
            eprintln!(
                "bar: test directories go on disk, under {}: {reason}",
                base.display()
            );
        }
        let root = placement
            .base()
            .join(jojobot_bar::sweep::run_root_name(std::process::id(), nanos));
        fs::create_dir_all(&root).ok()?;
        RUN_ROOT.set(root.clone()).ok()?;
        Some(RunRoot(root))
    }
}

impl Drop for RunRoot {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        // A fixture that left a directory nobody may write cannot be emptied.
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// **Point a child's temp directory at this run's own**, when it has one.
///
/// **A child never sees [`BUNDLE_VARIABLE`].** It names the bundle for this bar
/// alone. A test that starts a bar of its own, in a directory of its own, would
/// otherwise run that bar on this run's binaries and fail to read the manifest.
fn in_the_run_root(command: &mut Command) -> &mut Command {
    command.env_remove(BUNDLE_VARIABLE);
    match RUN_ROOT.get() {
        Some(root) => command.env("TMPDIR", root),
        None => command,
    }
}

/// **One phase's own slice of the log** — written and read back so the
/// summary is built from what actually landed on disk, never from a copy
/// kept only in memory.
fn run_phase(
    log_path: &Path,
    header: &str,
    program: &str,
    args: &[&str],
) -> std::io::Result<(bool, String)> {
    {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        writeln!(f, "\n$ {header}")?;
    }
    let start = fs::metadata(log_path)?.len();
    let out = OpenOptions::new().append(true).open(log_path)?;
    let err = out.try_clone()?;
    let started = std::time::Instant::now();
    let status = in_the_run_root(&mut Command::new(program))
        .args(args)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .status()?;
    jojobot_bar::metrics::phase(args, status.success(), started.elapsed());
    let bytes = fs::read(log_path)?;
    let phase_text = String::from_utf8_lossy(&bytes[start as usize..]).into_owned();
    Ok((status.success(), phase_text))
}

/// **The jobs line of a verdict**: how many test binaries ran side by side, at
/// which limit, and the wall time of all of them together when it was measured.
fn jobs_detail(limit: usize, wall: Option<std::time::Duration>) -> String {
    match wall {
        Some(wall) => format!(
            "{limit} test binaries at a time ({TEST_JOBS_VARIABLE}), wall {}s",
            wall.as_secs()
        ),
        None => format!("{limit} test binaries at a time ({TEST_JOBS_VARIABLE})"),
    }
}

/// **The variable that names a bundle's manifest.** A run that sets it runs
/// the binaries the bundle holds and compiles no test; a run that does not
/// compiles and runs them as `cargo test` does.
const BUNDLE_VARIABLE: &str = "BAR_BUNDLE";

/// **A bundle a run was told to use, and the lane of it this run takes.**
struct Prebuilt {
    manifest: Manifest,
    lane: String,
}

impl Prebuilt {
    /// **Read the bundle named by [`BUNDLE_VARIABLE`]**, or `None` when the
    /// variable is unset or empty. A bundle that cannot be read, one with no
    /// binary in this lane, and one run from a directory other than the one it
    /// was compiled in are errors and never a fall back to compiling: a
    /// pull-request job that quietly compiled would pass and cost what the
    /// bundle exists to save.
    fn from_env(lane: &str) -> Result<Option<Prebuilt>, String> {
        let Some(path) = std::env::var_os(BUNDLE_VARIABLE).filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("{BUNDLE_VARIABLE} names {path:?}, which cannot be read: {e}"))?;
        let manifest = Manifest::from_json(&text)?;
        manifest.refuse_unless_arch(std::env::consts::ARCH)?;
        let here = std::env::current_dir()
            .and_then(fs::canonicalize)
            .map_err(|e| format!("the working directory cannot be read: {e}"))?;
        let root = fs::canonicalize(&manifest.root).map_err(|e| {
            format!(
                "the bundle was compiled in {}, which is absent here: {e}",
                manifest.root
            )
        })?;
        if here != root {
            return Err(format!(
                "the bundle was compiled in {} and this run is in {}: the binaries read files \
                 by the first path, so run it there (scripts/ci-root)",
                root.display(),
                here.display()
            ));
        }
        if !manifest.exes.iter().any(|exe| exe.lane == lane) {
            return Err(format!("the bundle holds no binary for the {lane} lane"));
        }
        Ok(Some(Prebuilt {
            manifest,
            lane: lane.to_string(),
        }))
    }

    /// The binaries of this run's lane.
    fn exes(&self) -> Vec<Exe> {
        self.manifest
            .exes
            .iter()
            .filter(|exe| exe.lane == self.lane)
            .cloned()
            .collect()
    }

    /// The listing of this run's lane, as `cargo test --no-run` printed it.
    fn listing(&self) -> String {
        self.manifest.listing(&self.lane)
    }

    /// **The commands that run one job's binaries**, with what `cargo test`
    /// gives a test binary: the directory of its package, that package's
    /// manifest directory in the environment, and the server the rooms spawn.
    /// The doc-tests have no binary and return `None`: cargo runs them. A job
    /// that selects no binary is an empty list, which the caller reports as
    /// red.
    fn commands(&self, job: &[String]) -> Option<Vec<(String, Command)>> {
        if job == ["--doc"] {
            return None;
        }
        let root = Path::new(&self.manifest.root);
        let exes = self.exes();
        Some(
            bundle::exes_for_job(job, &exes)
                .into_iter()
                .map(|exe| {
                    let path = root.join(&exe.path);
                    let dir = root.join(&exe.dir);
                    let mut command = Command::new(&path);
                    command.current_dir(&dir).env("CARGO_MANIFEST_DIR", &dir);
                    jojobot_bar::metrics::time_tests(&mut command);
                    // `target/<profile>/deps/<binary>` sits beside the server.
                    if let Some(profile) = path.parent().and_then(Path::parent)
                        && std::env::var_os("JOJOBOT_BIN").is_none()
                    {
                        command.env("JOJOBOT_BIN", profile.join("jojobot"));
                    }
                    (path.display().to_string(), command)
                })
                .collect(),
        )
    }
}

/// **Run one job's command to its end, or kill it when `stop` is set.** The
/// command leads a process group of its own, so the test binaries a `cargo test`
/// started die with it. `Some` is whether it exited zero; `None` is a command
/// killed because another job was red. With no `stop` it waits as `status` does.
fn run_job_command(
    command: &mut Command,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> std::io::Result<Option<bool>> {
    use std::os::unix::process::CommandExt;
    let Some(stop) = stop else {
        return Ok(Some(command.status()?.success()));
    };
    let mut child = command.process_group(0).spawn()?;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.success()));
        }
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            // SAFETY: `killpg` takes a process group id and a signal, and reads
            // no memory of ours. The group is the one this child leads.
            unsafe {
                libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
            }
            child.wait()?;
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// **What each test job did, in the order the jobs were listed**: whether it
/// exited zero, and the wall time of that one job.
type JobWalls = Vec<(bool, std::time::Duration)>;

/// **Run each job as its own `cargo test --workspace <job> --no-fail-fast
/// --locked`, `limit` at a time**, and return whether every one
/// exited zero together with all of their text.
///
/// Each job writes stdout and stderr into one file of its own, as
/// [`run_phase`] does, so a job's lines stay in the order it printed them.
/// The jobs are appended to the log in listing order, whatever order they
/// finished in, each under the command that ran it. A job that cannot be
/// started counts as a failed job and says why in its own section; it never
/// ends the others.
///
/// **With `fail_fast`, the first job that fails ends the run.** The jobs still
/// running are killed, with the processes they started, and no job starts after
/// it. A killed job and a job that never started are not red and are not
/// written to the history: the answer counts them. Without it every job runs.
// Seven arguments name one run of the jobs; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn run_test_jobs(
    log_path: &Path,
    cargo: &str,
    prefix: &[&str],
    jobs: &[Vec<String>],
    limit: usize,
    history: Option<&Run>,
    prebuilt: Option<&Prebuilt>,
    fail_fast: bool,
) -> std::io::Result<(bool, String, JobWalls, usize)> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let dir = log_path.with_extension("jobs");
    fs::create_dir_all(&dir)?;
    let next = AtomicUsize::new(0);
    // Set by the first job that fails under `fail_fast`.
    let stop = AtomicBool::new(false);
    let done: Mutex<Vec<Option<(bool, String, std::time::Duration)>>> =
        Mutex::new(vec![None; jobs.len()]);
    let not_run: Mutex<Vec<bool>> = Mutex::new(vec![false; jobs.len()]);
    let workers = limit.min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if fail_fast && stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(i) else { break };
                    let file = dir.join(format!("{i}.log"));
                    let (killed, result) =
                        (|| -> std::io::Result<(bool, (bool, String, std::time::Duration))> {
                            let out = File::create(&file)?;
                            let err = out.try_clone()?;
                            let started = std::time::Instant::now();
                            let by_bundle = prebuilt.and_then(|p| p.commands(job));
                            // `None` is a job killed because another one was red.
                            let success = match by_bundle {
                                Some(commands) if commands.is_empty() => {
                                    writeln!(
                                        &out,
                                        "bar: the bundle holds no binary for the job {job:?}"
                                    )?;
                                    Some(false)
                                }
                                Some(commands) => {
                                    let mut all = Some(true);
                                    for (name, mut command) in commands {
                                        writeln!(&out, "\n     Running {name}")?;
                                        let ran = run_job_command(
                                            in_the_run_root(&mut command)
                                                .stdout(Stdio::from(out.try_clone()?))
                                                .stderr(Stdio::from(err.try_clone()?)),
                                            fail_fast.then_some(&stop),
                                        )?;
                                        all = match (all, ran) {
                                            (Some(so_far), Some(ok)) => Some(so_far && ok),
                                            _ => None,
                                        };
                                        if all.is_none() {
                                            break;
                                        }
                                    }
                                    all
                                }
                                None => run_job_command(
                                    jojobot_bar::metrics::time_tests(
                                        in_the_run_root(&mut Command::new(cargo))
                                            .arg("test")
                                            .args(job_args(prefix, job))
                                            .args(["--no-fail-fast", "--locked"])
                                            .arg("--"),
                                    )
                                    .stdout(Stdio::from(out))
                                    .stderr(Stdio::from(err)),
                                    fail_fast.then_some(&stop),
                                )?,
                            };
                            let took = started.elapsed();
                            Ok((
                                success.is_none(),
                                (success.unwrap_or(false), fs::read_to_string(&file)?, took),
                            ))
                        })()
                        .unwrap_or_else(|e| {
                            (
                                false,
                                (
                                    false,
                                    format!("bar: could not run this job: {e}\n"),
                                    std::time::Duration::ZERO,
                                ),
                            )
                        });
                    if killed {
                        not_run.lock().expect("no job panics while holding this")[i] = true;
                    } else {
                        jojobot_bar::metrics::test_binary(job, result.0, &result.1, result.2);
                        if let Some(run) = history
                            && let Err(e) = run.record_job(job, result.0, &parse_job(&result.1))
                        {
                            eprintln!("bar: test history: this job was not written: {e}");
                        }
                        if fail_fast && !result.0 {
                            stop.store(true, Ordering::SeqCst);
                        }
                    }
                    done.lock().expect("no job panics while holding this")[i] = Some(result);
                }
            });
        }
    });

    let mut all_ok = true;
    let mut all_text = String::new();
    let mut walls = Vec::with_capacity(jobs.len());
    let mut left_out = 0usize;
    let mut log = OpenOptions::new().append(true).open(log_path)?;
    let results = done.into_inner().expect("no job panics while holding this");
    let killed = not_run
        .into_inner()
        .expect("no job panics while holding this");
    for ((job, result), killed) in jobs.iter().zip(results).zip(killed) {
        // **A job that never started, or was killed, is neither red nor green**:
        // it is counted, and says why it has no answer.
        let result = match result {
            Some(result) if !killed => Some(result),
            Some((_, text, took)) => Some((
                false,
                format!("{text}bar: killed: an earlier job was red\n"),
                took,
            )),
            None => None,
        };
        let did_not_run = killed || (fail_fast && result.is_none());
        if did_not_run {
            left_out += 1;
        }
        let (ok, text, took) = result.unwrap_or((
            false,
            if fail_fast {
                String::from("bar: this job did not start: an earlier job was red\n")
            } else {
                String::from("bar: this job never ran\n")
            },
            std::time::Duration::ZERO,
        ));
        all_ok &= ok || did_not_run;
        walls.push((ok, took));
        writeln!(
            log,
            "\n$ cargo test {} --no-fail-fast --locked",
            job_args(prefix, job).join(" ")
        )?;
        log.write_all(text.as_bytes())?;
        if !did_not_run {
            all_text.push_str(&text);
        }
    }
    Ok((all_ok, all_text, walls, left_out))
}

/// **`bar rooms --test <name> [--test <name> ...]`: the room suites, side by
/// side.** The Makefile names the room targets, read out of the exercise
/// manifest, so the bar holds no list of rooms. Each is one job:
/// `cargo test -p jojobot-exercise --features rooms --test <name>`. The
/// workspace is built first, because a room spawns the built server and a test
/// run does not rebuild it. The limit is the same `BAR_JOBS` as the test phase.
fn run_rooms(args: &[String], known: &KnownReds) -> std::io::Result<ExitCode> {
    let mut jobs: Vec<Vec<String>> = Vec::new();
    let mut lane = "rooms".to_string();
    let mut all_features = false;
    let mut rest = args.iter().peekable();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--test" if rest.peek().is_some() => {
                jobs.push(vec![
                    "--test".to_string(),
                    rest.next().cloned().unwrap_or_default(),
                ]);
            }
            "--lane" if rest.peek().is_some() => lane = rest.next().cloned().unwrap_or_default(),
            "--all-features" => all_features = true,
            _ => {
                eprintln!(
                    "usage: bar rooms [--lane <name>] [--all-features] --test <name> [--test <name> ...]"
                );
                return Ok(ExitCode::from(2));
            }
        }
    }
    if jobs.is_empty() {
        eprintln!("bar rooms names no room: bar rooms --test <name> [--test <name> ...]");
        return Ok(ExitCode::from(2));
    }
    let limit = match jobs_limit(std::env::var(TEST_JOBS_VARIABLE).ok().as_deref()) {
        Ok(limit) => limit,
        Err(refusal) => {
            eprintln!("bar: {refusal}");
            return Ok(ExitCode::from(2));
        }
    };
    let cargo = cargo_bin();
    let log_path = PathBuf::from(format!("target/bar/{lane}.log"));
    fresh_log(&log_path)?;
    let mut summary = Summary::new();
    let prebuilt = match Prebuilt::from_env(&lane) {
        Ok(prebuilt) => prebuilt,
        Err(refusal) => {
            summary.phase_failed("bundle", &refusal);
            summary.phase_skipped("test");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
    };

    if prebuilt.is_some() {
        summary.phase_ok("build", "prebuilt bundle");
    } else {
        let (ok, text) = run_phase(
            &log_path,
            "cargo build --workspace --locked",
            &cargo,
            &["build", "--workspace", "--locked"],
        )?;
        if !ok {
            let verdict = summarize_test_output(&text);
            if verdict.compiled {
                summary.phase_failed("build", "see log");
            } else {
                summary.phase_failed("build", "DID NOT COMPILE — see log");
            }
            summary.phase_skipped("test");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
        summary.phase_ok("build", "compiled");
    }

    let history = begin_history(&lane, limit);
    let (ok, text, _, _) = run_test_jobs(
        &log_path,
        &cargo,
        if all_features {
            &["-p", "jojobot-exercise", "--all-features"]
        } else {
            &["-p", "jojobot-exercise", "--features", "rooms"]
        },
        &jobs,
        limit,
        history.as_ref(),
        prebuilt.as_ref(),
        false,
    )?;
    finish_history(history.as_ref());
    let verdict = summarize_test_output(&text);
    summary.phase_ok(
        "jobs",
        &format!("{limit} test binaries at a time ({TEST_JOBS_VARIABLE})"),
    );
    summary.test_phase(ok, &verdict, known);
    finish(&mut summary, &log_path);
    Ok(if test_healthy(ok, &verdict) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// **The bundle of test binaries**, written by `bar bundle [--room <name> ...]`.
const BUNDLE_MANIFEST: &str = "target/bundle/manifest.json";
const BUNDLE_ARCHIVE: &str = "target/bundle.tar.gz";

/// **`bar bundle [--room <name> ...]`: compile every test binary the pull
/// request bar runs, once, and pack them.** Three lanes, each under the
/// feature set its own target uses: `check` is the workspace as `cargo test
/// --workspace` builds it, `rooms` is the exercise crate with the `rooms`
/// feature and the room suites named by `--room`, and `year` is the year room
/// with every feature. `--all-features` over the workspace would select the
/// retired suites too.
///
/// The archive holds the server and the other binaries of the profile
/// directory, because a test that spawns one names it by a path fixed at
/// compile time, and the manifest. A job that unpacks it at the checkout root
/// and sets `BAR_BUNDLE` to the manifest runs the same tests without
/// compiling. A binary whose package cannot be told, or whose path lies outside
/// the checkout, ends the run red: a bundle that dropped it would pass.
fn run_bundle(args: &[String]) -> std::io::Result<ExitCode> {
    let mut rooms: Vec<&str> = Vec::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
            ("--room", Some(name)) => rooms.push(name),
            _ => {
                eprintln!("usage: bar bundle [--room <name> ...]");
                return Ok(ExitCode::from(2));
            }
        }
    }
    let cargo = cargo_bin();
    let log_path = PathBuf::from("target/bar/bundle.log");
    fresh_log(&log_path)?;
    let mut summary = Summary::new();

    let started = std::time::Instant::now();
    let (ok, _) = run_phase(
        &log_path,
        "cargo build --workspace --locked",
        &cargo,
        &["build", "--workspace", "--locked"],
    )?;
    if !ok {
        summary.phase_failed_in("build", "see log", started.elapsed());
        summary.phase_skipped("bundle");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok_in("build", "compiled", started.elapsed());

    let metadata = in_the_run_root(&mut Command::new(&cargo))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .output()?;
    let targets = if metadata.status.success() {
        bundle::targets_of(&String::from_utf8_lossy(&metadata.stdout))
    } else {
        Err("cargo metadata failed".to_string())
    };
    let (root, targets) = match targets {
        Ok(found) => found,
        Err(reason) => {
            summary.phase_failed(
                "bundle",
                &format!("the packages could not be read: {reason}"),
            );
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut lanes: Vec<(&str, Vec<&str>)> = vec![
        ("check", vec!["test", "--workspace", "--no-run", "--locked"]),
        (
            "rooms",
            ["test", "-p", "jojobot-exercise", "--features", "rooms"]
                .into_iter()
                .chain(rooms.iter().flat_map(|name| ["--test", *name]))
                .chain(["--no-run", "--locked"])
                .collect(),
        ),
        (
            "year",
            vec![
                "test",
                "-p",
                "jojobot-exercise",
                "--all-features",
                "--test",
                "year_room",
                "--no-run",
                "--locked",
            ],
        ),
    ];
    if rooms.is_empty() {
        lanes.retain(|(lane, _)| *lane != "rooms");
    }
    let mut exes: Vec<Exe> = Vec::new();
    for (lane, lane_args) in lanes {
        let started = std::time::Instant::now();
        let (ok, listing) = run_phase(
            &log_path,
            &format!("cargo {}", lane_args.join(" ")),
            &cargo,
            &lane_args,
        )?;
        if !ok {
            summary.phase_failed_in(&format!("compile-{lane}"), "see log", started.elapsed());
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
        let before = exes.len();
        for (describes, printed) in bundle::executables(&listing) {
            let path = bundle::relative_to(&root, &printed);
            let found = bundle::package_dir(&path, &targets).and_then(|dir| {
                if path.starts_with('/') {
                    Err(format!(
                        "{path} lies outside the checkout; build with the default target directory"
                    ))
                } else {
                    Ok(dir)
                }
            });
            let dir = match found {
                Ok(dir) => dir,
                Err(reason) => {
                    summary.phase_failed(&format!("compile-{lane}"), &reason);
                    finish(&mut summary, &log_path);
                    return Ok(ExitCode::FAILURE);
                }
            };
            exes.push(Exe {
                lane: lane.to_string(),
                describes,
                path,
                dir,
            });
        }
        summary.phase_ok_in(
            &format!("compile-{lane}"),
            &format!("{} test binaries", exes.len() - before),
            started.elapsed(),
        );
    }

    // The profile directory, `target/debug`, holds the binaries a test spawns.
    let mut files: Vec<String> = exes.iter().map(|exe| exe.path.clone()).collect();
    if let Some(profile) = exes
        .first()
        .and_then(|exe| Path::new(&exe.path).parent()?.parent())
    {
        for entry in fs::read_dir(profile)? {
            let entry = entry?;
            let named = entry.file_name().to_string_lossy().into_owned();
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                entry.metadata()?.is_file() && entry.metadata()?.permissions().mode() & 0o111 != 0
            };
            if executable && !named.contains('.') {
                files.push(format!("{}/{named}", profile.display()));
            }
        }
    }
    files.sort();
    files.dedup();

    fs::create_dir_all("target/bundle")?;
    fs::write(
        BUNDLE_MANIFEST,
        Manifest {
            root,
            arch: std::env::consts::ARCH.to_string(),
            exes,
        }
        .to_json(),
    )?;
    files.push(BUNDLE_MANIFEST.to_string());

    // **The debug information is stripped from copies**, never from the
    // binaries cargo built: the target directory stays whole and its next build
    // stays incremental. Debug information is most of a test binary's size and
    // no test reads it, and the archive travels between hosts.
    let stage = Path::new("target/bundle/stage");
    if stage.exists() {
        fs::remove_dir_all(stage)?;
    }
    let mut staged: Vec<String> = Vec::new();
    let mut before = 0;
    for file in &files {
        let to = stage.join(file);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        before += fs::copy(file, &to)?;
        if file != BUNDLE_MANIFEST {
            staged.push(to.display().to_string());
        }
    }
    let started = std::time::Instant::now();
    let mut strip_args = vec!["--strip-debug"];
    strip_args.extend(staged.iter().map(String::as_str));
    let (stripped, _) = run_phase(
        &log_path,
        &format!("strip --strip-debug ({} files)", staged.len()),
        "strip",
        &strip_args,
    )?;
    if !stripped {
        summary.phase_failed_in("strip", "see log", started.elapsed());
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    let mut after = 0;
    for file in &files {
        after += fs::metadata(stage.join(file))?.len();
    }
    summary.phase_ok_in(
        "strip",
        &format!("{before} bytes before, {after} bytes after"),
        started.elapsed(),
    );

    let started = std::time::Instant::now();
    let stage_dir = stage.display().to_string();
    let (ok, _) = run_phase(
        &log_path,
        &format!("tar -czf {BUNDLE_ARCHIVE} -C {stage_dir} ."),
        "tar",
        &["-czf", BUNDLE_ARCHIVE, "-C", &stage_dir, "."],
    )?;
    let bytes = fs::metadata(BUNDLE_ARCHIVE).map(|m| m.len()).unwrap_or(0);
    if ok {
        summary.phase_ok_in(
            "bundle",
            &format!("{} files, {} bytes in {BUNDLE_ARCHIVE}", files.len(), bytes),
            started.elapsed(),
        );
    } else {
        summary.phase_failed_in("bundle", "tar failed — see log", started.elapsed());
    }
    finish(&mut summary, &log_path);
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// **The workspace, and what the push being reported touched in it.** `None` when
/// the paths or the packages cannot be read; callers then keep their own order and
/// run what they ran.
fn touched_workspace(
    cargo: &str,
) -> Option<(
    dev::Workspace,
    dev::Touched,
    std::collections::BTreeSet<String>,
)> {
    let paths = pushed_paths().filter(|paths| !paths.is_empty())?;
    let metadata = in_the_run_root(&mut Command::new(cargo))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let workspace = dev::parse_metadata(&String::from_utf8_lossy(&metadata.stdout)).ok()?;
    let hit = dev::touched(&paths, &workspace);
    let selected = dev::with_dependents(&hit, &workspace);
    Some((workspace, hit, selected))
}

/// **The same jobs, the touched packages' first** (see [`dev::touched_first`]), so
/// that a case a push broke in the crates it touched is run before the others.
/// Nothing is dropped. When the paths or the packages cannot be read the listed
/// order stands, and the answer says whether the order was changed.
fn touched_jobs_first(cargo: &str, jobs: Vec<Vec<String>>) -> (Vec<Vec<String>>, Option<String>) {
    let Some((workspace, hit, selected)) = touched_workspace(cargo) else {
        return (jobs, None);
    };
    let named = if hit.whole_workspace {
        "the whole workspace".to_string()
    } else {
        hit.crates.iter().cloned().collect::<Vec<_>>().join(", ")
    };
    let ordered = dev::touched_first(jobs, &hit.crates, &selected, &workspace);
    (ordered, Some(named))
}

/// **Packages whose library cases spawn the built `jojobot` binary.** Run ahead
/// of the build they are red on a runner whose target holds none, so they wait
/// for the main run.
const SPAWNS_THE_BINARY: &[&str] = &["jojobot-exercise"];

/// **The library cases of the packages the push touched**, as jobs, and the
/// packages' names. The report level runs them before it builds anything for the
/// whole workspace, so a case the push broke there is red as early as the crates
/// they need can be compiled. A job the history lets the report level go is not
/// among them: the report would not have run it. `None` when nothing can be said
/// to be touched (the whole workspace, or no readable range), because then the
/// order of the main run is all there is.
fn touched_library_jobs(cargo: &str) -> Option<(Vec<Vec<String>>, String)> {
    let (workspace, hit, _) = touched_workspace(cargo)?;
    if hit.whole_workspace {
        return None;
    }
    let libs: Vec<Vec<String>> = workspace
        .packages
        .iter()
        .filter(|p| {
            p.has_lib
                && hit.crates.contains(&p.name)
                && !SPAWNS_THE_BINARY.contains(&p.name.as_str())
        })
        .map(|p| vec!["-p".to_string(), p.name.clone(), "--lib".to_string()])
        .collect();
    if libs.is_empty() {
        return None;
    }
    let (jobs, _) = jobs_for(Level::Report, libs);
    if jobs.is_empty() {
        return None;
    }
    let names = jobs
        .iter()
        .filter_map(|job| job.get(1).cloned())
        .collect::<Vec<_>>()
        .join(", ");
    Some((jobs, names))
}

/// **`bar dev`: the everyday loop, a level below the report.** It checks the
/// format, builds the workspace, then runs the unit tests and the fast test
/// files of the packages the branch touches and of those that depend on them.
/// It then lints the packages it selected, whatever the tests said. It runs no
/// room and no doc-test, and the verdict names what it left out and says it is not
/// a boundary run.
///
/// **Nothing touched is refused**, as a selection that matched no test is: a
/// green over nothing is the answer this guard exists to end.
fn run_dev(args: &[String], known: &KnownReds) -> std::io::Result<ExitCode> {
    if let Some(other) = args.first() {
        eprintln!("bar dev: unrecognised argument '{other}'");
        return Ok(ExitCode::from(2));
    }
    let cargo = cargo_bin();
    let limit = match jobs_limit(std::env::var(TEST_JOBS_VARIABLE).ok().as_deref()) {
        Ok(limit) => limit,
        Err(refusal) => {
            eprintln!("bar: {refusal}");
            return Ok(ExitCode::from(2));
        }
    };
    let log_path = PathBuf::from("target/bar/dev.log");
    fresh_log(&log_path)?;
    let mut summary = Summary::new();
    jojobot_bar::static_checks::size_and_roster(&mut summary);
    if !summary.green {
        for phase in ["fmt-check", "build", "test", "lint"] {
            summary.phase_skipped(phase);
        }
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }

    // **A branch that changed nothing has nothing to run**, and that is an
    // answer rather than a refusal: the loop is run from habit, on main as often
    // as on a branch. A diff that changed paths in no package is still refused
    // below, because that one is a question for the person.
    let changed = changed_paths();
    if let Some((base, paths)) = &changed
        && paths.is_empty()
    {
        summary.phase_ok(
            "scope",
            &format!(
                "nothing changed since {}; nothing to run",
                base.chars().take(10).collect::<String>()
            ),
        );
        finish(&mut summary, &log_path);
        return Ok(if summary.green {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }

    let Some(metadata) = jojobot_bar::static_checks::format_and_allowlist(&mut summary) else {
        for phase in ["build", "test", "lint"] {
            summary.phase_skipped(phase);
        }
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    };
    let workspace = dev::parse_metadata(&metadata.to_string());
    let workspace = match workspace {
        Ok(workspace) => workspace,
        Err(reason) => {
            summary.phase_failed(
                "scope",
                &format!("the packages could not be read: {reason}"),
            );
            summary.phase_skipped("build");
            summary.phase_skipped("test");
            summary.phase_skipped("lint");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
    };

    // **The scope.** With no merge base to take, every package runs: the bar
    // errs toward running more, and the line says why.
    let (touched, why) = match changed {
        Some((_, paths)) => (dev::touched(&paths, &workspace), None),
        None => (
            dev::Touched {
                whole_workspace: true,
                ..dev::Touched::default()
            },
            Some("there is no merge base with main, so every package is selected"),
        ),
    };
    let selected = dev::selected_packages(&touched, &workspace);
    let left_out = dev::dependents_left_out(&selected, &workspace);
    if selected.is_empty() {
        summary.phase_failed(
            "scope",
            &format!(
                "the diff touches no package ({}) — nothing would run",
                dev::outside_line(&touched.outside)
            ),
        );
        summary.phase_skipped("build");
        summary.phase_skipped("test");
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::from(2));
    }
    let named = |set: &std::collections::BTreeSet<String>| {
        set.iter().cloned().collect::<Vec<_>>().join(", ")
    };
    summary.phase_ok(
        "scope",
        &format!(
            "touched {}{}; selected {} of {}: {}; {}",
            if touched.whole_workspace {
                "the whole workspace".to_string()
            } else {
                named(&touched.crates)
            },
            why.map_or(String::new(), |w| format!(" ({w})")),
            selected.len(),
            workspace.packages.len(),
            named(&selected),
            dev::outside_line(&touched.outside),
        ),
    );

    let started = std::time::Instant::now();
    let (ok, text) = run_phase(
        &log_path,
        "cargo build --workspace --locked",
        &cargo,
        &["build", "--workspace", "--locked"],
    )?;
    let took = started.elapsed();
    if !ok {
        let verdict = summarize_test_output(&text);
        if verdict.compiled {
            summary.phase_failed_in("build", "see log", took);
        } else {
            summary.phase_failed_in("build", "DID NOT COMPILE — see log", took);
        }
        summary.phase_skipped("test");
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok_in("build", "compiled", took);

    let variable = std::env::var(HISTORY_DIR_VARIABLE).ok();
    let home = std::env::var("HOME").ok();
    let history = history_root(variable.as_deref(), home.as_deref())
        .map(|root| read_history(&root, &["check", "report"]))
        .unwrap_or_default();
    let plan = dev::plan(&selected, &workspace, &history, &is_slow);
    summary.phase_ok(
        "level",
        &format!(
            "dev, {} jobs; {} test files ran with no recorded history",
            plan.run.len(),
            plan.unmeasured
        ),
    );
    summary.phase_ok("skipped", &dev::skipped_line(&plan, &selected, &workspace));
    summary.phase_ok(
        "dependents",
        &dev::dependents_line(&left_out, git_stdout(&["branch", "--show-current"]).trim()),
    );
    summary.phase_ok(
        "boundary",
        "NOT a boundary run: no real store guaranteed absent; make check is the bar before a \
         report or a carry",
    );

    let started = std::time::Instant::now();
    let (ok, text, _, _) =
        run_test_jobs(&log_path, &cargo, &[], &plan.run, limit, None, None, false)?;
    summary.phase_ok("jobs", &jobs_detail(limit, Some(started.elapsed())));
    let verdict = summarize_test_output(&text);
    summary.test_phase(ok, &verdict, known);

    // **The lint runs whatever the tests said**, as the report and narrow levels
    // run it, over the packages the tests were chosen from.
    let lint_args = dev::clippy_args(&selected).expect("a selection that is empty was refused");
    let lint_args: Vec<&str> = lint_args.iter().map(String::as_str).collect();
    let started = std::time::Instant::now();
    let (lint_ok, _) = run_phase(
        &log_path,
        "cargo clippy -p <selected> --all-targets --all-features --locked -- -D warnings",
        &cargo,
        &lint_args,
    )?;
    let took = started.elapsed();
    if lint_ok {
        summary.phase_ok_in("lint", "clean", took);
    } else {
        summary.phase_failed_in("lint", "see log", took);
    }

    finish(&mut summary, &log_path);
    Ok(if test_healthy(ok, &verdict) && summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// **`bar call --test <name> --call flake|defect --by <handle>`: write down a
/// person's judgement of a red** in the history directory. It runs no test and
/// changes no verdict.
fn run_call(args: &[String]) -> std::io::Result<ExitCode> {
    let call = match parse_call(args) {
        Ok(call) => call,
        Err(reason) => {
            eprintln!("bar call: {reason}\n{CALL_USAGE}");
            return Ok(ExitCode::from(2));
        }
    };
    let variable = std::env::var(HISTORY_DIR_VARIABLE).ok();
    let home = std::env::var("HOME").ok();
    let Some(root) = history_root(variable.as_deref(), home.as_deref()) else {
        eprintln!("bar call: {HISTORY_DIR_VARIABLE} and HOME are both unset");
        return Ok(ExitCode::from(2));
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    write_call(&root, &call, &iso_utc(seconds), std::process::id())?;
    println!(
        "recorded: {} is called a {} by {}",
        call.test, call.call, call.by
    );
    Ok(ExitCode::SUCCESS)
}

/// **Open this run's history, or say why not.** A history that cannot be
/// written is said on stderr and never changes the verdict: the bar's answer is
/// the tests', and the record of them is a second thing.
fn begin_history(level: &str, limit: usize) -> Option<Run> {
    let variable = std::env::var(HISTORY_DIR_VARIABLE).ok();
    let home = std::env::var("HOME").ok();
    let Some(root) = history_root(variable.as_deref(), home.as_deref()) else {
        eprintln!("bar: test history not written: {HISTORY_DIR_VARIABLE} and HOME are both unset");
        return None;
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let started_at = iso_utc(seconds);
    let checkout = std::env::current_dir()
        .ok()
        .and_then(|dir| {
            dir.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let head =
        Some(git_stdout(&["rev-parse", "HEAD"]).trim().to_string()).filter(|h| !h.is_empty());
    let dirty = head
        .as_ref()
        .map(|_| !git_stdout(&["status", "--porcelain"]).trim().is_empty());
    let load_average = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().and_then(|n| n.parse().ok()));
    let header = RunHeader {
        run_id: run_id(&started_at, &checkout, std::process::id()),
        level: level.to_string(),
        checkout,
        head,
        dirty,
        started_at,
        bar_jobs: limit,
        load_average,
        cpus: std::thread::available_parallelism()
            .ok()
            .map(std::num::NonZero::get),
    };
    match Run::begin(&root, header) {
        Ok(run) => Some(run),
        Err(e) => {
            eprintln!("bar: test history not written: {e}");
            None
        }
    }
}

/// **Say when the run ended.** A run killed before this leaves a header with no
/// end, which is how a reader tells it from one that finished.
fn finish_history(history: Option<&Run>) {
    let Some(run) = history else { return };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    if let Err(e) = run.finish(&iso_utc(seconds)) {
        eprintln!("bar: test history: the end of the run was not written: {e}");
    }
}

fn fresh_log(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    File::create(path)?;
    Ok(())
}

fn finish(summary: &mut Summary, log_path: &Path) {
    let line_count = fs::read_to_string(log_path)
        .map(|s| s.lines().count())
        .unwrap_or(0);
    summary.log(&log_path.display().to_string(), line_count);
    print!("{}", summary.render());
}

/// **The cast names that arrived since `origin/main`.** The roster gate keeps
/// the fictional cast on one list and the handles that name nobody on another,
/// and a person reads the first because that is where a name that could be real
/// would sit. Report only: nothing here touches `summary.green`.
fn roster_report() -> String {
    let base = git_stdout(&["merge-base", "HEAD", "origin/main"])
        .trim()
        .to_string();
    let then = match base.is_empty() {
        true => String::new(),
        false => [ROSTER_FILE, jojobot_roster::LEGACY_ROSTER_FILE]
            .into_iter()
            .map(|path| git_stdout(&["show", &format!("{base}:{path}")]))
            .find(|source| !source.is_empty())
            .unwrap_or_default(),
    };
    // A base that cannot show the file is no base: say unknown, not "all new".
    let base = match then.is_empty() {
        true => String::new(),
        false => base,
    };
    let now = fs::read_to_string(ROSTER_FILE).unwrap_or_default();
    let arrived = new_roster_entries(&roster_entries(&now), &roster_entries(&then));
    render_roster_report(&arrived, &base)
}

/// **Which jobs a run of the test phase covers.** The boundary level is every
/// job; the report level is every job less the ones the history lets go.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Boundary,
    Report,
}

impl Level {
    /// The name the run's history and its log go by.
    fn name(self) -> &'static str {
        match self {
            Level::Boundary => "check",
            Level::Report => "report",
        }
    }
}

/// **Whether `main` contains a recorded head**, by `git merge-base
/// --is-ancestor`. `None` when git cannot tell: a head it does not know, a
/// head that would be read as an option, or no git at all.
fn main_contains(head: &str) -> Option<bool> {
    if !is_a_revision(head) {
        return None;
    }
    let status = Command::new("git")
        .args(["merge-base", "--is-ancestor", head, "main"])
        .output()
        .ok()?
        .status;
    contained_from_exit(status.code())
}

/// **The jobs a level runs, and the line that says which.** The boundary level
/// runs the listing as it stands. The report level asks the history, which is
/// empty when none can be read, so a missing history runs everything.
fn jobs_for(level: Level, all: Vec<Vec<String>>) -> (Vec<Vec<String>>, String) {
    match level {
        Level::Boundary => {
            let line = format!("boundary, {} jobs", all.len());
            (all, line)
        }
        Level::Report => {
            let variable = std::env::var(HISTORY_DIR_VARIABLE).ok();
            let home = std::env::var("HOME").ok();
            let history = history_root(variable.as_deref(), home.as_deref())
                .map(|root| read_history(&root, &["check", "report"]))
                .unwrap_or_default();
            // **Asked of git once per head.** A red counts against a job only when
            // `main` contains the run's head: the history holds every run of every
            // line of work, and a red on another line's unmerged work says nothing
            // about the jobs `main` has to pass.
            let asked = std::cell::RefCell::new(std::collections::HashMap::new());
            let ancestry = |head: &str| -> Option<bool> {
                *asked
                    .borrow_mut()
                    .entry(head.to_string())
                    .or_insert_with(|| main_contains(head))
            };
            let plan = report_plan(&all, &history, &is_slow, &ancestry);
            let line = report_line(&plan, all.len());
            (plan.run, line)
        }
    }
}

fn run_check(known: &KnownReds, level: Level) -> std::io::Result<ExitCode> {
    let code = run_check_phases(known, level)?;
    // **Printed after the verdict, and unconditionally.** A standing duty
    // gets a mechanism rather than diligence (rule 235) — this runs whichever
    // phase fails or passes, and never touches `summary.green`. It comes
    // after the phases because it never changes the verdict, and a reader
    // takes the verdict first. The size check is not here: it is a phase.
    print!("{}", roster_report());
    Ok(code)
}

fn run_check_phases(known: &KnownReds, level: Level) -> std::io::Result<ExitCode> {
    // **Refused before any phase runs**, so a limit that is not a number never
    // costs a build and never reads as a green run at some other number.
    let limit = match jobs_limit(std::env::var(TEST_JOBS_VARIABLE).ok().as_deref()) {
        Ok(limit) => limit,
        Err(refusal) => {
            eprintln!("bar: {refusal}");
            return Ok(ExitCode::from(2));
        }
    };
    if let Some(base) = plan::fast_base() {
        return plan::check_docs(&base);
    }
    let cargo = cargo_bin();
    let log_path = PathBuf::from(format!("target/bar/{}.log", level.name()));
    fresh_log(&log_path)?;
    let mut summary = Summary::new();
    jojobot_bar::size::phase(&mut summary);

    let started = std::time::Instant::now();
    let (ok, _) = run_phase(
        &log_path,
        "cargo fmt --all --check",
        &cargo,
        &["fmt", "--all", "--check"],
    )?;
    let took = started.elapsed();
    if !ok {
        summary.phase_failed_in("fmt-check", "not formatted — see log", took);
        summary.phase_skipped("build");
        summary.phase_skipped("test");
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok_in("fmt-check", "formatted", took);

    // **Built before it is driven, not left to `cargo test` to build only
    // what it happens to depend on.** `jojobot-exercise`'s suites spawn the
    // `jojobot` binary as a child process rather than linking against the
    // `jojobot` crate, so `cargo test --workspace` has no dependency edge
    // that would rebuild it — a source `jojobot-exercise` never imports can
    // go stale under a binary those suites still spawn, and the suite's own
    // guard then refuses rather than driving it. The guard is correct and
    // stays exactly as it is; this phase is what keeps it from firing on an
    // ordinary green run.
    let prebuilt = match Prebuilt::from_env("check") {
        Ok(prebuilt) => prebuilt,
        Err(refusal) => {
            summary.phase_failed("bundle", &refusal);
            summary.phase_skipped("build");
            summary.phase_skipped("test");
            summary.phase_skipped("lint");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
    };
    // **The report level runs the touched crates' library cases first**, before
    // anything is built for the whole workspace: a case the push broke there is
    // red when the crates it needs are compiled, not after the test binaries of
    // every crate are. The jobs run here are left out of the main run, which
    // adds their text to its own.
    let mut fast_done: Vec<Vec<String>> = Vec::new();
    let mut fast_text = String::new();
    let mut fast_history = None;
    if level == Level::Report
        && prebuilt.is_none()
        && let Some((jobs, names)) = touched_library_jobs(&cargo)
    {
        let history = begin_history(level.name(), limit);
        let started = std::time::Instant::now();
        let (ok, text, _, _) = run_test_jobs(
            &log_path,
            &cargo,
            &[],
            &jobs,
            limit,
            history.as_ref(),
            None,
            true,
        )?;
        let took = started.elapsed();
        if !ok {
            let verdict = summarize_test_output(&text);
            summary.phase_failed_in(
                "fast",
                &format!("the library cases of {names}, run first, are red — see log"),
                took,
            );
            summary.test_phase(false, &verdict, known);
            summary.phase_skipped("build");
            summary.phase_skipped("lint");
            finish_history(history.as_ref());
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
        summary.phase_ok_in(
            "fast",
            &format!("the library cases of {names} ran first and passed"),
            took,
        );
        fast_done = jobs;
        fast_text = text;
        fast_history = history;
    }
    if prebuilt.is_some() {
        summary.phase_ok("build", "prebuilt bundle");
    } else {
        let started = std::time::Instant::now();
        let (ok, text) = run_phase(
            &log_path,
            "cargo build --workspace --locked",
            &cargo,
            &["build", "--workspace", "--locked"],
        )?;
        let took = started.elapsed();
        if !ok {
            let verdict = summarize_test_output(&text);
            if verdict.compiled {
                summary.phase_failed_in("build", "see log", took);
            } else {
                summary.phase_failed_in("build", "DID NOT COMPILE — see log", took);
            }
            summary.phase_skipped("test");
            summary.phase_skipped("lint");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
        summary.phase_ok_in("build", "compiled", took);
    }

    // **The same run as `cargo test --workspace --no-fail-fast --locked`,
    // split by test binary and run side by side.** One serial run sums every
    // suite's wall time; the slowest single suite is what the split leaves.
    // The listing says which binaries exist, so a binary the plain run would
    // execute is in exactly one job and the suite and case counts are the same.
    let started = std::time::Instant::now();
    let (listed, listing) = match &prebuilt {
        Some(prebuilt) => (true, prebuilt.listing()),
        None => run_phase(
            &log_path,
            "cargo test --workspace --no-run --locked",
            &cargo,
            &["test", "--workspace", "--no-run", "--locked"],
        )?,
    };
    let took = started.elapsed();
    if prebuilt.is_some() {
        summary.phase_ok("test-compile", "prebuilt bundle");
    } else if listed {
        summary.phase_ok_in("test-compile", "test binaries built", took);
    } else {
        summary.phase_failed_in("test-compile", "see log", took);
    }
    let mut level_line = None;
    let mut wall = None;
    let mut doc_tests = None;
    let mut left_out = 0usize;
    let mut touched_line: Option<String> = None;
    let (ok, text) = if listed {
        let (jobs, line) = jobs_for(level, test_jobs(&listing));
        println!("level: {line}");
        std::io::stdout().flush()?;
        // **The report runs the jobs of the packages the push touched first.**
        let jobs = if level == Level::Report {
            let jobs: Vec<Vec<String>> = jobs
                .into_iter()
                .filter(|job| !fast_done.contains(job))
                .collect();
            let (jobs, touched) = touched_jobs_first(&cargo, jobs);
            if let Some(touched) = touched {
                touched_line = Some(touched);
            }
            jobs
        } else {
            jobs
        };
        level_line = Some(line);
        let history = match fast_history.take() {
            Some(history) => Some(history),
            None => begin_history(level.name(), limit),
        };
        let started = std::time::Instant::now();
        // **The report level ends at the first red job**; the boundary level
        // reports every target.
        let (ok, text, walls, not_run) = run_test_jobs(
            &log_path,
            &cargo,
            &["--workspace"],
            &jobs,
            limit,
            history.as_ref(),
            prebuilt.as_ref(),
            level == Level::Report,
        )?;
        left_out = not_run;
        wall = Some(started.elapsed());
        // **The doc-tests are one job among the others**, and a slow one is
        // worth its own line: the jobs' wall time is the slowest of them.
        doc_tests = jobs
            .iter()
            .position(|job| job.as_slice() == ["--doc".to_string()])
            .map(|at| walls[at])
            .filter(|(_, took)| !took.is_zero());
        finish_history(history.as_ref());
        (ok, format!("{fast_text}{text}"))
    } else {
        // A listing that failed is a build that failed: the same text reads
        // as "did not compile", exactly as a failed plain run did.
        (false, listing)
    };
    let verdict = summarize_test_output(&text);
    // **The limit that produced the timing**, on the verdict, so a wall time
    // read later says what it was measured at.
    if let Some(line) = &level_line {
        summary.phase_ok("level", line);
    }
    if let Some(touched) = &touched_line {
        summary.phase_ok(
            "order",
            &format!("jobs of the packages the push touched first: {touched}"),
        );
    }
    summary.phase_ok("jobs", &jobs_detail(limit, wall));
    if left_out > 0 {
        summary.phase_ok(
            "stopped",
            &format!(
                "the report level ends at the first red job; {left_out} jobs did not run to the end"
            ),
        );
    }
    match doc_tests {
        Some((true, took)) => summary.phase_ok_in("doc-tests", "ran", took),
        Some((false, took)) => summary.phase_failed_in("doc-tests", "see log", took),
        None => {}
    }
    summary.test_phase(ok, &verdict, known);
    // **The lint runs whatever the tests said.** A red test step used to end
    // the run here, and a lint failure then stayed hidden behind every test that
    // was red; the two are separate questions and the verdict carries both. The
    // run is green only when both are.

    // **A run that stopped at its first red job has its answer**; the lint is the
    // next thing a green run does.
    if left_out > 0 {
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    let started = std::time::Instant::now();
    let (ok, _) = run_phase(
        &log_path,
        "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
        &cargo,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            // **Lint reads the targets `cargo test` skips.** A room suite
            // sits behind a feature, so the test phase above does not build
            // or run it; this flag is what keeps it compiled and linted.
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    let took = started.elapsed();
    if ok {
        summary.phase_ok_in("lint", "clean", took);
    } else {
        summary.phase_failed_in("lint", "see log", took);
    }

    finish(&mut summary, &log_path);
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// How many tests a `-- --list` run printed: one `name: test` line each.
fn count_listed_tests(listing: &str) -> usize {
    listing
        .lines()
        .filter(|l| l.trim_end().ends_with(": test"))
        .count()
}

fn run_narrow(args: &[String], known: &KnownReds) -> std::io::Result<ExitCode> {
    let cargo = cargo_bin();
    let mut krate: Option<String> = None;
    let mut filter: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--crate" => {
                krate = args.get(i + 1).cloned();
                i += 2;
            }
            "--filter" => {
                filter = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                eprintln!("bar narrow: unrecognised argument '{other}'");
                return Ok(ExitCode::from(2));
            }
        }
    }
    let Some(krate) = krate else {
        eprintln!("bar narrow needs a crate: bar narrow --crate <name>");
        return Ok(ExitCode::from(2));
    };

    let log_path = PathBuf::from(format!("target/bar/narrow-{krate}.log"));
    fresh_log(&log_path)?;
    let mut summary = Summary::new();
    jojobot_bar::static_checks::phase(&mut summary);
    if !summary.green {
        for phase in ["build", "test", "lint"] {
            summary.phase_skipped(phase);
        }
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }

    let (ok, text) = run_phase(
        &log_path,
        "cargo build --workspace --locked",
        &cargo,
        &["build", "--workspace", "--locked"],
    )?;
    if !ok {
        let verdict = summarize_test_output(&text);
        if verdict.compiled {
            summary.phase_failed("build", "see log");
        } else {
            summary.phase_failed("build", "DID NOT COMPILE — see log");
        }
        summary.phase_skipped("test");
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok("build", "compiled");

    // **The zero-selection guard, unchanged**: FILTER is a substring of a
    // test's full path, and a mistyped one selects nothing rather than
    // failing outright — `cargo test` reports a pass over an empty
    // selection, so the count is read before the run and decides.
    // **`--all-features`, so a feature-gated suite can be narrowed to.** A
    // room suite is skipped by a plain `cargo test -p`, and a filter naming
    // one would read as "selected no tests".
    // **FILTER may name several cases, split on whitespace**: `cargo test`
    // takes each as a substring and runs a test that matches any of them,
    // which is how a hand-off's list of cases runs in one call.
    let filters: Vec<&str> = filter.as_deref().unwrap_or("").split_whitespace().collect();
    // **After `--`**, because `cargo test` itself takes one name before it and
    // the test harness takes any number after it.
    let mut list_args: Vec<&str> = vec!["test", "-p", &krate, "--all-features", "--", "--list"];
    list_args.extend(&filters);
    let (_, list_text) = run_phase(
        &log_path,
        "cargo test -p <crate> [filter] -- --list",
        &cargo,
        &list_args,
    )?;
    // **Only tests that will run count.** A plain `--list` prints an
    // `#[ignore]`d test beside the others, so the same selection is listed
    // again with `--ignored`, which prints that subset alone, and the
    // difference is what a test run would execute.
    let listed = count_listed_tests(&list_text);
    let mut ignored_args = list_args.clone();
    ignored_args.insert(ignored_args.len() - filters.len(), "--ignored");
    let (listing_ran, ignored_text) = run_phase(
        &log_path,
        "cargo test -p <crate> [filter] -- --list --ignored",
        &cargo,
        &ignored_args,
    )?;
    // **A listing that could not run prints no tests**, which would read as
    // none of them ignored and count every listed test as runnable.
    if !listing_ran {
        summary.phase_failed(
            "test",
            "the listing of ignored tests failed, so what would run is unknown — see log",
        );
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    let mut ignored_names: Vec<_> = ignored_text.lines().map(str::trim_end).collect();
    let runnable: Vec<_> = list_text
        .lines()
        .map(str::trim_end)
        .filter(|name| name.ends_with(": test"))
        .filter(|name| {
            if let Some(at) = ignored_names.iter().position(|ignored| ignored == name) {
                ignored_names.remove(at);
                false
            } else {
                true
            }
        })
        .map(|name| name.trim_end_matches(": test"))
        .collect();
    let skipped: Vec<_> = if krate == "jojobot" && std::env::var_os("UPGRADE_FIXTURE_DIR").is_none()
    {
        runnable
            .iter()
            .copied()
            .filter(|name| local_upgrade::CASES.contains(name))
            .collect()
    } else {
        Vec::new()
    };
    // Libtest's --skip is a substring, so a duplicate or a longer name must
    // not silently exclude a case that can run without the fixture.
    for name in &skipped {
        if runnable.iter().filter(|other| other.contains(name)).count() != 1 {
            summary.phase_failed("test", &format!("ambiguous CI-only skip: {name}"));
            summary.phase_skipped("lint");
            finish(&mut summary, &log_path);
            return Ok(ExitCode::FAILURE);
        }
        println!("skipped by design: {name} — needs CI-supplied UPGRADE_FIXTURE_DIR");
    }
    let selected = runnable.len() - skipped.len();
    if selected == 0 {
        let detail = if !skipped.is_empty() {
            format!(
                "only CI upgrade cases selected in {krate}; UPGRADE_FIXTURE_DIR is absent — nothing would run"
            )
        } else {
            match (&filter, listed) {
                (Some(f), 0) => format!(
                    "FILTER='{f}' selected no tests in {krate} — FILTER is a substring of a test's \
                 full path, not a regex"
                ),
                (None, 0) => format!("{krate} has no tests"),
                (Some(f), _) => {
                    format!(
                        "FILTER='{f}' matched only ignored tests in {krate} — nothing would run"
                    )
                }
                (None, _) => format!("every test in {krate} is ignored — nothing would run"),
            }
        };
        summary.phase_failed("test", &detail);
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        // **The same exit code the old inline guard used** (rule: same
        // exit codes) — 2, not the generic failure code, so a caller
        // scripted against the old refusal still recognises it.
        return Ok(ExitCode::from(2));
    }

    println!(
        "level: boundary, narrow {krate}, FILTER={}; {selected} runnable cases; left to full boundary: other tests",
        filter.as_deref().unwrap_or("all")
    );
    for name in runnable.iter().filter(|name| !skipped.contains(name)) {
        println!("selected: {name}");
    }
    std::io::stdout().flush()?;
    let mut test_args: Vec<&str> = vec!["test", "-p", &krate, "--all-features"];
    if !filters.is_empty() || !skipped.is_empty() {
        test_args.push("--");
        test_args.extend(&filters);
        for name in &skipped {
            test_args.extend(["--skip", name]);
        }
    }
    let (ok, text) = run_phase(
        &log_path,
        "cargo test -p <crate> [filter]",
        &cargo,
        &test_args,
    )?;
    let verdict = summarize_test_output(&text);
    summary.test_phase(ok, &verdict, known);
    // **The lint runs whatever the tests said.** A red test step used to end
    // the run here, and a lint failure then stayed hidden behind every test that
    // was red; the two are separate questions and the verdict carries both. The
    // run is green only when both are.

    let (ok, _) = run_phase(
        &log_path,
        "cargo clippy -p <crate> --all-targets --all-features -- -D warnings",
        &cargo,
        &[
            "clippy",
            "-p",
            &krate,
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    if ok {
        summary.phase_ok("lint", "clean");
    } else {
        summary.phase_failed("lint", "see log");
    }

    finish(&mut summary, &log_path);
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A command the bar starts carries no bundle variable**, even when this
    /// process has one.
    #[test]
    fn a_child_command_is_started_without_the_bundle_variable() {
        let mut command = Command::new("true");
        in_the_run_root(&mut command);
        let held: Vec<_> = command
            .get_envs()
            .filter(|(key, _)| *key == BUNDLE_VARIABLE)
            .collect();
        assert_eq!(held, [(std::ffi::OsStr::new(BUNDLE_VARIABLE), None)]);
    }
}
