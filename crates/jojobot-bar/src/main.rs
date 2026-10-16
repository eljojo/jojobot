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

use jojobot_bar::dev;
use jojobot_bar::{
    CALL_USAGE, FileGrowth, FileSize, HISTORY_DIR_VARIABLE, KnownReds, ROSTER_FILE, Run, RunHeader,
    Summary, TEST_JOBS_VARIABLE, contained_from_exit, grown_files, history_root, is_a_revision,
    is_slow, iso_utc, job_args, jobs_limit, load_known_reds, new_roster_entries, oversized_files,
    parse_call, parse_job, read_history, render_roster_report, render_size_report, report_line,
    report_plan, roster_entries, run_id, start_lines, summarize_test_output, test_healthy,
    test_jobs, write_call,
};

/// **The tests that are red and already have a card**, read from the directory
/// the bar runs in, which is the repository root.
const KNOWN_REDS_FILE: &str = "known-reds.toml";

/// **The cargo binary the outer `make` recipe was told to use.** The
/// Makefile's own `CARGO ?= cargo` is an override hook, and this reads the
/// same variable so a phase this build spawns honours it too rather than
/// hardcoding a second, silently different default.
fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

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
        Some("narrow") => run_narrow(&args[1..], &known),
        Some("dev") => run_dev(&args[1..], &known),
        Some("rooms") => run_rooms(&args[1..], &known),
        Some("call") => run_call(&args[1..]),
        _ => {
            eprintln!(
                "usage: bar check | bar report | bar dev | bar narrow --crate <name> [--filter <substring>] \
                 | bar rooms --test <name> [--test <name> ...] | bar call ..."
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

/// **A temp directory for everything this run starts, removed when the run
/// ends.** The test processes make directories in their temp directory and do
/// not all remove them, and a killed one removes none. Pointing them all at one
/// directory of this run's own, and removing that, is what makes the cleanup
/// the run's and not each fixture's. A run that is killed leaves the directory,
/// and the sweep at the next start removes it by the process number in its name.
///
/// **It is made under the memory-backed directory when that has room** (see
/// [`jojobot_bar::scratch`]), so the test stores do not wait on a disk. This is
/// the one place the bar chooses it: every child gets it through
/// [`in_the_run_root`], and no case sets a temp directory of its own.
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
fn in_the_run_root(command: &mut Command) -> &mut Command {
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
    let status = in_the_run_root(&mut Command::new(program))
        .args(args)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .status()?;
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
fn run_test_jobs(
    log_path: &Path,
    cargo: &str,
    prefix: &[&str],
    jobs: &[Vec<String>],
    limit: usize,
    history: Option<&Run>,
) -> std::io::Result<(bool, String, JobWalls)> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = log_path.with_extension("jobs");
    fs::create_dir_all(&dir)?;
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<Option<(bool, String, std::time::Duration)>>> =
        Mutex::new(vec![None; jobs.len()]);
    let workers = limit.min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(i) else { break };
                    let file = dir.join(format!("{i}.log"));
                    let result = (|| -> std::io::Result<(bool, String, std::time::Duration)> {
                        let out = File::create(&file)?;
                        let err = out.try_clone()?;
                        let started = std::time::Instant::now();
                        let status = in_the_run_root(&mut Command::new(cargo))
                            .arg("test")
                            .args(job_args(prefix, job))
                            .args(["--no-fail-fast", "--locked"])
                            .stdout(Stdio::from(out))
                            .stderr(Stdio::from(err))
                            .status()?;
                        let took = started.elapsed();
                        Ok((status.success(), fs::read_to_string(&file)?, took))
                    })()
                    .unwrap_or_else(|e| {
                        (
                            false,
                            format!("bar: could not run this job: {e}\n"),
                            std::time::Duration::ZERO,
                        )
                    });
                    if let Some(run) = history
                        && let Err(e) = run.record_job(job, result.0, &parse_job(&result.1))
                    {
                        eprintln!("bar: test history: this job was not written: {e}");
                    }
                    done.lock().expect("no job panics while holding this")[i] = Some(result);
                }
            });
        }
    });

    let mut all_ok = true;
    let mut all_text = String::new();
    let mut walls = Vec::with_capacity(jobs.len());
    let mut log = OpenOptions::new().append(true).open(log_path)?;
    let results = done.into_inner().expect("no job panics while holding this");
    for (job, result) in jobs.iter().zip(results) {
        let (ok, text, took) = result.unwrap_or((
            false,
            String::from("bar: this job never ran\n"),
            std::time::Duration::ZERO,
        ));
        all_ok &= ok;
        walls.push((ok, took));
        writeln!(
            log,
            "\n$ cargo test {} --no-fail-fast --locked",
            job_args(prefix, job).join(" ")
        )?;
        log.write_all(text.as_bytes())?;
        all_text.push_str(&text);
    }
    Ok((all_ok, all_text, walls))
}

/// **`bar rooms --test <name> [--test <name> ...]`: the room suites, side by
/// side.** The Makefile names the room targets, read out of the exercise
/// manifest, so the bar holds no list of rooms. Each is one job:
/// `cargo test -p jojobot-exercise --features rooms --test <name>`. The
/// workspace is built first, because a room spawns the built server and a test
/// run does not rebuild it. The limit is the same `BAR_JOBS` as the test phase.
fn run_rooms(args: &[String], known: &KnownReds) -> std::io::Result<ExitCode> {
    let mut jobs: Vec<Vec<String>> = Vec::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
            ("--test", Some(name)) => jobs.push(vec!["--test".to_string(), name.clone()]),
            _ => {
                eprintln!("usage: bar rooms --test <name> [--test <name> ...]");
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
    let log_path = PathBuf::from("target/bar/rooms.log");
    fresh_log(&log_path)?;
    let mut summary = Summary::new();

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

    let history = begin_history("rooms", limit);
    let (ok, text, _) = run_test_jobs(
        &log_path,
        &cargo,
        &["-p", "jojobot-exercise", "--features", "rooms"],
        &jobs,
        limit,
        history.as_ref(),
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

/// **The paths the branch changed**, for `bar dev`: everything that differs
/// from the merge base with `main`, committed or not, and the files git does
/// not track yet, with the merge base they were taken from. `None` when there is
/// no merge base to take.
fn changed_paths() -> Option<(String, Vec<String>)> {
    let base = git_stdout(&["merge-base", "main", "HEAD"]);
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    let mut paths: Vec<String> = git_stdout(&["diff", "--name-only", base])
        .lines()
        .chain(git_stdout(&["ls-files", "--others", "--exclude-standard"]).lines())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths.dedup();
    Some((base.to_string(), paths))
}

/// **`bar dev`: the everyday loop, a level below the report.** It checks the
/// format, builds the workspace, then runs the unit tests and the fast test
/// files of the packages the branch touches and of those that depend on them.
/// It runs no lint, no room and no doc-test, and the verdict names what it left
/// out and says it is not a boundary run.
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
        return Ok(ExitCode::SUCCESS);
    }

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
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok_in("fmt-check", "formatted", took);

    // **Only stdout is read as metadata**: a warning on stderr is not JSON. The
    // capture is large, so the log gets the command and any stderr, not the
    // capture.
    let metadata = in_the_run_root(&mut Command::new(&cargo))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .output()?;
    {
        let mut f = OpenOptions::new().append(true).open(&log_path)?;
        writeln!(
            f,
            "\n$ cargo metadata --no-deps --format-version 1 --locked"
        )?;
        f.write_all(&metadata.stderr)?;
    }
    let workspace = if metadata.status.success() {
        dev::parse_metadata(&String::from_utf8_lossy(&metadata.stdout))
    } else {
        Err("cargo metadata failed — see log".to_string())
    };
    let workspace = match workspace {
        Ok(workspace) => workspace,
        Err(reason) => {
            summary.phase_failed(
                "scope",
                &format!("the packages could not be read: {reason}"),
            );
            summary.phase_skipped("build");
            summary.phase_skipped("test");
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
    let selected = dev::with_dependents(&touched, &workspace);
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
        "boundary",
        "NOT a boundary run: no real store guaranteed absent; make check is the bar before a \
         report or a carry",
    );

    let started = std::time::Instant::now();
    let (ok, text, _) = run_test_jobs(&log_path, &cargo, &[], &plan.run, limit, None)?;
    summary.phase_ok("jobs", &jobs_detail(limit, Some(started.elapsed())));
    let verdict = summarize_test_output(&text);
    summary.test_phase(ok, &verdict, known);
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

/// One `git` invocation, stdout as text. Empty on any failure — a caller
/// missing `origin/main` or running outside a repo gets nothing to report
/// rather than a crash.
fn git_stdout(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// **Decision log 273, and rule 235: a standing duty gets a mechanism.**
/// Every tracked `.rs` file's current line count, and — against the merge
/// base with `origin/main`, the last pushed commit — how much each changed
/// file grew. Report only: nothing here touches `summary.green`.
fn size_report() -> String {
    let tracked = git_stdout(&["ls-files", "--", "*.rs"]);
    let sizes: Vec<FileSize> = tracked
        .lines()
        .filter_map(|path| {
            fs::read_to_string(path).ok().map(|text| FileSize {
                path: path.to_string(),
                lines: text.lines().count(),
            })
        })
        .collect();
    let over = oversized_files(&sizes);

    let base = git_stdout(&["merge-base", "HEAD", "origin/main"])
        .trim()
        .to_string();
    let mut growths = Vec::new();
    if !base.is_empty() {
        let changed = git_stdout(&[
            "diff",
            "--name-only",
            &format!("{base}..HEAD"),
            "--",
            "*.rs",
        ]);
        for path in changed.lines() {
            if !Path::new(path).is_file() {
                continue; // deleted since the base
            }
            let now = fs::read_to_string(path)
                .map(|t| t.lines().count())
                .unwrap_or(0);
            let then = git_stdout(&["show", &format!("{base}:{path}")])
                .lines()
                .count();
            growths.push(FileGrowth {
                path: path.to_string(),
                delta: now as i64 - then as i64,
            });
        }
    }
    let grown = grown_files(&growths);

    render_size_report(&over, &grown, &base)
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
        false => git_stdout(&["show", &format!("{base}:{ROSTER_FILE}")]),
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
    // takes the verdict first.
    print!("{}", size_report());
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
    let cargo = cargo_bin();
    let log_path = PathBuf::from(format!("target/bar/{}.log", level.name()));
    fresh_log(&log_path)?;
    let mut summary = Summary::new();

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

    // **The same run as `cargo test --workspace --no-fail-fast --locked`,
    // split by test binary and run side by side.** One serial run sums every
    // suite's wall time; the slowest single suite is what the split leaves.
    // The listing says which binaries exist, so a binary the plain run would
    // execute is in exactly one job and the suite and case counts are the same.
    let started = std::time::Instant::now();
    let (listed, listing) = run_phase(
        &log_path,
        "cargo test --workspace --no-run --locked",
        &cargo,
        &["test", "--workspace", "--no-run", "--locked"],
    )?;
    let took = started.elapsed();
    if listed {
        summary.phase_ok_in("test-compile", "test binaries built", took);
    } else {
        summary.phase_failed_in("test-compile", "see log", took);
    }
    let mut level_line = None;
    let mut wall = None;
    let mut doc_tests = None;
    let (ok, text) = if listed {
        let (jobs, line) = jobs_for(level, test_jobs(&listing));
        level_line = Some(line);
        let history = begin_history(level.name(), limit);
        let started = std::time::Instant::now();
        let (ok, text, walls) = run_test_jobs(
            &log_path,
            &cargo,
            &["--workspace"],
            &jobs,
            limit,
            history.as_ref(),
        )?;
        wall = Some(started.elapsed());
        // **The doc-tests are one job among the others**, and a slow one is
        // worth its own line: the jobs' wall time is the slowest of them.
        doc_tests = jobs
            .iter()
            .position(|job| job.as_slice() == ["--doc".to_string()])
            .map(|at| walls[at]);
        finish_history(history.as_ref());
        (ok, text)
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
    summary.phase_ok("jobs", &jobs_detail(limit, wall));
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

    let (ok, _) = run_phase(
        &log_path,
        "cargo fmt --all --check",
        &cargo,
        &["fmt", "--all", "--check"],
    )?;
    if !ok {
        summary.phase_failed("fmt-check", "not formatted — see log");
        summary.phase_skipped("build");
        summary.phase_skipped("test");
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }
    summary.phase_ok("fmt-check", "formatted");

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
    let mut list_args: Vec<&str> = vec!["test", "-p", &krate, "--all-features"];
    if let Some(f) = &filter {
        list_args.push(f);
    }
    list_args.extend(["--", "--list"]);
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
    ignored_args.push("--ignored");
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
    let ignored = count_listed_tests(&ignored_text);
    let selected = listed.saturating_sub(ignored);
    if selected == 0 {
        let detail = match (&filter, listed) {
            (Some(f), 0) => format!(
                "FILTER='{f}' selected no tests in {krate} — FILTER is a substring of a test's \
                 full path, not a regex"
            ),
            (None, 0) => format!("{krate} has no tests"),
            (Some(f), _) => {
                format!("FILTER='{f}' matched only ignored tests in {krate} — nothing would run")
            }
            (None, _) => format!("every test in {krate} is ignored — nothing would run"),
        };
        summary.phase_failed("test", &detail);
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        // **The same exit code the old inline guard used** (rule: same
        // exit codes) — 2, not the generic failure code, so a caller
        // scripted against the old refusal still recognises it.
        return Ok(ExitCode::from(2));
    }

    let mut test_args: Vec<&str> = vec!["test", "-p", &krate, "--all-features"];
    if let Some(f) = &filter {
        test_args.push(f);
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
