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

use jojobot_bar::{
    FileGrowth, FileSize, Summary, grown_files, oversized_files, render_size_report,
    summarize_test_output, test_healthy,
};

/// **The cargo binary the outer `make` recipe was told to use.** The
/// Makefile's own `CARGO ?= cargo` is an override hook, and this reads the
/// same variable so a phase this build spawns honours it too rather than
/// hardcoding a second, silently different default.
fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check") => run_check(),
        Some("narrow") => run_narrow(&args[1..]),
        _ => {
            eprintln!("usage: bar check | bar narrow --crate <name> [--filter <substring>]");
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
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .status()?;
    let bytes = fs::read(log_path)?;
    let phase_text = String::from_utf8_lossy(&bytes[start as usize..]).into_owned();
    Ok((status.success(), phase_text))
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

fn run_check() -> std::io::Result<ExitCode> {
    let code = run_check_phases()?;
    // **Printed after the verdict, and unconditionally.** A standing duty
    // gets a mechanism rather than diligence (rule 235) — this runs whichever
    // phase fails or passes, and never touches `summary.green`. It comes
    // after the phases because it never changes the verdict, and a reader
    // takes the verdict first.
    print!("{}", size_report());
    Ok(code)
}

fn run_check_phases() -> std::io::Result<ExitCode> {
    let cargo = cargo_bin();
    let log_path = PathBuf::from("target/bar/check.log");
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

    // **Built before it is driven, not left to `cargo test` to build only
    // what it happens to depend on.** `jojobot-exercise`'s suites spawn the
    // `jojobot` binary as a child process rather than linking against the
    // `jojobot` crate, so `cargo test --workspace` has no dependency edge
    // that would rebuild it — a source `jojobot-exercise` never imports can
    // go stale under a binary those suites still spawn, and the suite's own
    // guard then refuses rather than driving it. The guard is correct and
    // stays exactly as it is; this phase is what keeps it from firing on an
    // ordinary green run.
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

    let (ok, text) = run_phase(
        &log_path,
        "cargo test --workspace --no-fail-fast --locked",
        &cargo,
        &["test", "--workspace", "--no-fail-fast", "--locked"],
    )?;
    let verdict = summarize_test_output(&text);
    summary.test_phase(ok, &verdict);
    if !test_healthy(ok, &verdict) {
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }

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

/// How many tests a `-- --list` run printed: one `name: test` line each.
fn count_listed_tests(listing: &str) -> usize {
    listing
        .lines()
        .filter(|l| l.trim_end().ends_with(": test"))
        .count()
}

fn run_narrow(args: &[String]) -> std::io::Result<ExitCode> {
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
    summary.test_phase(ok, &verdict);
    if !test_healthy(ok, &verdict) {
        summary.phase_skipped("lint");
        finish(&mut summary, &log_path);
        return Ok(ExitCode::FAILURE);
    }

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
