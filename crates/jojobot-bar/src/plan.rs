//! The startup plan needs metadata, never compiled test binaries.

use super::*;
use jojobot_bar::{job_name, startup_targets};
use serde_json::Value;

pub(super) fn run(args: &[String]) -> std::io::Result<ExitCode> {
    let lane = args.first().map(String::as_str).unwrap_or("");
    if !matches!(
        lane,
        "report" | "check" | "build" | "rooms" | "year" | "prepush"
    ) {
        return Err(std::io::Error::other(
            "plan needs report, check, build, rooms, year or prepush",
        ));
    }
    let fast =
        matches!(lane, "report" | "check" | "build" | "rooms" | "year") && fast_base().is_some();
    if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "fast={fast}"
        )?;
    }
    if fast {
        println!(
            "level: docs/ratchet-only; run: size, diff whitespace; left to boundary: builds, code tests, rooms, year, lint"
        );
        return Ok(ExitCode::SUCCESS);
    }
    let output = Command::new(cargo_bin())
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let (all, rooms, year) = startup_targets(&metadata)?;
    let level = if lane == "report" {
        Level::Report
    } else {
        Level::Boundary
    };
    let (jobs, line) = jobs_for(level, all);
    let names = |jobs: &[Vec<String>]| {
        jobs.iter()
            .map(|job| job_name(job))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match lane {
        "report" | "check" => {
            println!("level: {line}; run: {}", names(&jobs));
            if lane == "report"
                && let Some((first, _)) = touched_library_jobs(&cargo_bin())
            {
                println!("libraries run first: {}", names(&first));
            }
        }
        "rooms" => println!(
            "level: boundary, rooms; run: {}; left to check/year: other tests",
            rooms.join(", ")
        ),
        "year" => println!(
            "level: boundary, year; run: {}; left to check/rooms: other tests",
            year.join(", ")
        ),
        _ => println!(
            "level: boundary, {lane}; check: {}; rooms: {}; year: {}; left to boundary: none{}",
            names(&jobs),
            rooms.join(", "),
            year.join(", "),
            if lane == "prepush" {
                "; PR proof may reuse check/rooms/year; always nix-build and flake-check"
            } else {
                ""
            }
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// Only the workflow's explicit diff can license skipping code checks. A
/// missing base, git failure, or dirty checkout keeps the full path.
pub(super) fn fast_base() -> Option<String> {
    let base = std::env::var("BAR_CI_BASE").ok()?;
    if std::env::var("SIZE_BASE").ok().as_deref() != Some(base.as_str()) {
        return None;
    }
    if base.len() != 40 || !base.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let status = Command::new("git")
        .args(["status", "--porcelain", "-z"])
        .output()
        .ok()?;
    if !status.status.success() || !status.stdout.is_empty() {
        return None;
    }
    let diff = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &base,
            "HEAD",
            "--",
        ])
        .output()
        .ok()?;
    if !diff.status.success() {
        return None;
    }
    let paths = std::str::from_utf8(&diff.stdout)
        .ok()?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    jojobot_bar::docs_only(&paths).then_some(base)
}

pub(super) fn check_docs(base: &str) -> std::io::Result<ExitCode> {
    println!(
        "level: docs/ratchet-only; run: size, diff whitespace; left to boundary: builds, code tests, rooms, year, lint"
    );
    let log = Path::new("target/bar/docs.log");
    fresh_log(log)?;
    let mut summary = Summary::new();
    jojobot_bar::size::phase(&mut summary);
    let (ok, _) = run_phase(
        log,
        "git diff --check",
        "git",
        &["diff", "--check", base, "HEAD", "--"],
    )?;
    if ok {
        summary.phase_ok("whitespace", "diff checked");
    } else {
        summary.phase_failed("whitespace", "see log");
    }
    finish(&mut summary, log);
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
