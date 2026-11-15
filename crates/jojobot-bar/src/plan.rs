//! The startup plan needs metadata, never compiled test binaries.

use super::*;
use jojobot_bar::{job_name, startup_targets};
use serde_json::Value;

/// **The text of a list file named by a dispatch**: a path inside the working
/// directory, which is the checkout. An absolute path, a `..` and a link that
/// leaves the checkout are refused, and so is a file with no item in it.
fn list_file_text(list: &str) -> std::io::Result<String> {
    let refused = |why: &str| std::io::Error::other(format!("targeted LIST file {list:?}: {why}"));
    if !Path::new(list)
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(refused(
            "a path inside the repository, with no .. and not absolute, is needed",
        ));
    }
    let inside = std::env::current_dir()?.canonicalize()?;
    let path = Path::new(list)
        .canonicalize()
        .map_err(|e| refused(&e.to_string()))?;
    if !path.starts_with(&inside) || !path.is_file() {
        return Err(refused("a file inside the repository is needed"));
    }
    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Err(refused("holds no item"));
    }
    Ok(text)
}

/// **Everything the upgrade gate's cases are written in**, as one string: the
/// local gate's source, the gate's test file and the files of its directory.
fn upgrade_gate_sources() -> String {
    fn walk(path: &Path, into: &mut String) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                walk(&entry.path(), into);
            }
        } else if let Ok(text) = fs::read_to_string(path) {
            into.push_str(&text);
        }
    }
    let mut text = String::new();
    for path in [
        "crates/jojobot-bar/src/local_upgrade.rs",
        "crates/jojobot/tests/upgrade_gate.rs",
        "crates/jojobot/tests/upgrade_gate",
    ] {
        walk(Path::new(path), &mut text);
    }
    text
}

pub(super) fn run(args: &[String]) -> std::io::Result<ExitCode> {
    let lane = args.first().map(String::as_str).unwrap_or("");
    if lane == "targeted" {
        let (krate, filter, list) = match args {
            [_, krate, filter] => (krate, filter.clone(), ""),
            [_, krate, filter, list] => (krate, filter.clone(), list.as_str()),
            _ => {
                return Err(std::io::Error::other(
                    "targeted plan needs a crate, a FILTER and a LIST file",
                ));
            }
        };
        // **The list is FILTER, or the lines of a file in the repository that LIST
        // names, never both.** The forge refuses a FILTER input of about 250
        // characters or more, so a long list comes from a file on the dispatch's ref.
        let filter = if list.is_empty() {
            filter
        } else if filter.is_empty() {
            list_file_text(list)?.replace('\n', " ")
        } else {
            return Err(std::io::Error::other(
                "targeted plan takes a FILTER or a LIST file, not both",
            ));
        };
        let filter = &filter;
        // **FILTER is a list of test-name substrings separated by spaces.** `make
        // narrow` takes the union of the items and refuses an item that selects no
        // test, naming it. These values reach make's command-line variables, so an
        // item's characters cannot become make expansions, shell syntax or libtest
        // options, and no whitespace but the space is taken.
        let items: Vec<&str> = filter.split(' ').filter(|item| !item.is_empty()).collect();
        if krate.is_empty()
            || krate.starts_with('-')
            || !krate
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            || items.is_empty()
            || !items.iter().all(|item| {
                item.chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
            })
        {
            return Err(std::io::Error::other(
                "targeted plan needs a Cargo crate and test-name substrings separated by spaces (letters, digits, _ or :)",
            ));
        }
        // **The later steps read the list and the fixture answer from here.** The
        // fixture is the last deployed store the upgrade gate's cases read: it is
        // prepared for the server crate when ANY item names text in the gate's
        // sources, so a list of several items is judged item by item.
        if let Ok(path) = std::env::var("GITHUB_ENV") {
            let fixture = krate == "jojobot" && {
                let sources = upgrade_gate_sources();
                items.iter().any(|item| sources.contains(item))
            };
            writeln!(
                OpenOptions::new().create(true).append(true).open(path)?,
                "TARGETED_FILTER={}\nTARGETED_FIXTURE={fixture}",
                items.join(" ")
            )?;
        }
        println!(
            "level: boundary, targeted {krate}, FILTER={filter}; run: selected cases with all features, fmt, build, lint; left to full boundary: other tests"
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !matches!(lane, "report" | "check" | "build" | "rooms" | "year") {
        return Err(std::io::Error::other(
            "plan needs report, check, build, rooms or year",
        ));
    }
    let docs_only =
        matches!(lane, "report" | "check" | "build" | "rooms" | "year") && fast_base().is_some();
    // **A branch push runs the rooms only when its change is to a room's own
    // files.** The rooms job reads the same `fast` answer a docs-only push gets
    // and skips its step; the pull request and the release gate run the rooms in
    // full.
    let rooms_left = if lane == "rooms" && !docs_only {
        rooms_not_selected()
    } else {
        None
    };
    let fast = docs_only || rooms_left.is_some();
    if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
        writeln!(
            OpenOptions::new().create(true).append(true).open(path)?,
            "fast={fast}"
        )?;
    }
    if let Some(why) = rooms_left {
        println!(
            "level: boundary, rooms; not run: {why}; left to the pull request and the release gate"
        );
        return Ok(ExitCode::SUCCESS);
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
    let all = if lane == "report" {
        if let Some(selection) = selection::current() {
            selection.announce();
            selection.jobs(all)
        } else {
            all
        }
    } else {
        all
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
                && !selection::current().is_some_and(|s| s.scoped())
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
            "level: boundary, {lane}; check: {}; rooms: {}; year: {}; left to boundary: none",
            names(&jobs),
            rooms.join(", "),
            year.join(", ")
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// **The directory of the rooms' own files**: a room's document, a room's test
/// file and the exercise harness all live under the exercise crate.
const ROOM_FILES: &str = "crates/jojobot-exercise/";

/// **Why a push leaves the rooms to the pull request, or `None` when it runs them.**
/// A server-crate change, the domain included, selects no room (rule 760); only a
/// change to a room's own files does, whatever the crate selection answers for
/// the paths: a manifest, a migration or a workflow file is a full run for the
/// crates and no room file. A dispatch (no selection by crate), a missing base and
/// a diff that cannot be read leave the rooms to run.
fn rooms_not_selected() -> Option<String> {
    if std::env::var("BAR_SELECT_BY_CRATE").ok().as_deref() != Some("1") {
        return None;
    }
    let base = std::env::var("BAR_SELECT_BASE").ok()?;
    let joint = Command::new("git")
        .args(["merge-base", &base, "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let joint = String::from_utf8(joint.stdout).ok()?;
    let diff = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            joint.trim(),
            "HEAD",
            "--",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let paths: Vec<String> = String::from_utf8(diff.stdout)
        .ok()?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if paths.is_empty() || paths.iter().any(|p| p.starts_with(ROOM_FILES)) {
        return None;
    }
    Some(format!(
        "no room document, room test file or exercise harness changed ({} paths)",
        paths.len()
    ))
}

/// Only a diff from a proved base can license skipping code checks: a newer
/// push may have canceled its predecessor before the code ran. Missing proof,
/// a git failure, or a dirty checkout keeps the full path.
pub(super) fn fast_base() -> Option<String> {
    if selection::current().is_some_and(|s| !s.allow_docs) {
        return None;
    }
    let base = std::env::var("BAR_CI_BASE").ok()?;
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
    if !jojobot_bar::docs_only(&paths) {
        return None;
    }
    let proof = Command::new("scripts/wait-run")
        .args([&base, "--code-proof"])
        .output()
        .ok()?;
    proof.status.success().then_some(base)
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
