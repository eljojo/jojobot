//! The cheap gates shared by the local loop and its pre-push entry.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::{Summary, changes, size};

pub fn run(args: &[String]) -> std::io::Result<ExitCode> {
    if !args.is_empty() {
        eprintln!("usage: bar static");
        return Ok(ExitCode::from(2));
    }
    let mut summary = Summary::new();
    phase(&mut summary);
    print!("{}", summary.render());
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

pub fn phase(summary: &mut Summary) {
    size_and_roster(summary);
    if summary.green {
        let _ = format_and_allowlist(summary);
    }
}

pub fn size_and_roster(summary: &mut Summary) {
    size::phase(summary);
    roster(summary);
}

pub fn format_and_allowlist(summary: &mut Summary) -> Option<serde_json::Value> {
    // These read-only calls inherit TMPDIR; the build/test phases own the stores.
    let started = std::time::Instant::now();
    let args = ["fmt", "--all", "--check"];
    let output = Command::new(crate::cargo_bin())
        .args(args)
        .env_remove("BAR_BUNDLE")
        .output();
    let took = started.elapsed();
    let ok = output.as_ref().is_ok_and(|out| out.status.success());
    crate::metrics::phase(&args, ok, took);
    if !ok {
        summary.phase_failed_in("fmt-check", "not formatted", took);
        match output {
            Ok(out) => {
                summary.note(&String::from_utf8_lossy(&out.stdout));
                summary.note(&String::from_utf8_lossy(&out.stderr));
            }
            Err(error) => summary.note(&error.to_string()),
        }
        return None;
    }
    summary.phase_ok_in("fmt-check", "formatted", took);
    crate::binary_allowlist::phase(summary)
}

fn source_path(path: &Path) -> bool {
    let extension = path.extension().and_then(|part| part.to_str());
    (path.starts_with("crates") && matches!(extension, Some("rs" | "md" | "json" | "jsonl")))
        || (path
            .parent()
            .is_some_and(|parent| parent.as_os_str().is_empty())
            && extension == Some("md"))
}

fn roster(summary: &mut Summary) {
    let (base, paths) = match changes::checked_changed_paths() {
        Ok(Some((base, paths))) => (Some(base), paths),
        Ok(None) => {
            let paths = match changes::git_paths(&[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ]) {
                Ok(paths) => paths,
                Err(error) => {
                    summary.phase_failed(
                        "roster",
                        &format!("source files could not be listed: {error}"),
                    );
                    return;
                }
            };
            summary.note("roster: no branch base; scanning all current source files");
            (None, paths)
        }
        Err(error) => {
            summary.phase_failed(
                "roster",
                &format!("source files could not be listed: {error}"),
            );
            return;
        }
    };
    let files: Vec<PathBuf> = paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| source_path(path) && path.is_file())
        .collect();
    let handles = match jojobot_roster::violations_in(&files) {
        Ok(found) => found,
        Err(error) => {
            summary.phase_failed("roster", &format!("not measured — {error}"));
            return;
        }
    };
    let bare = jojobot_roster::bare_slugs_in(&files);
    let declared = jojobot_roster::unlisted_slugs_under_a_declared_kind_in(&files);
    let count = handles.len() + bare.len() + declared.len();
    if count == 0 {
        summary.phase_ok("roster", &format!("{} touched source files", files.len()));
    } else {
        summary.phase_failed("roster", &format!("{count} offence(s)"));
        if !handles.is_empty() {
            summary.note(&jojobot_roster::refusal(&handles));
        }
        if !bare.is_empty() {
            summary.note(&jojobot_roster::bare_refusal(&bare));
        }
        if !declared.is_empty() {
            summary.note(&jojobot_roster::declared_refusal(&declared));
        }
    }

    let range = match base {
        Some(base) => format!("{base}..HEAD"),
        None => {
            let boundary = jojobot_roster::COMMIT_HANDLE_CHECK_BOUNDARY;
            let known = changes::git_stdout(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{boundary}^{{commit}}"),
            ]);
            if known.trim().is_empty() {
                "HEAD".to_string()
            } else {
                format!("{boundary}..HEAD")
            }
        }
    };
    let out = match Command::new("git")
        .args(["log", "--format=%h%x00%B%x01", &range])
        .output()
    {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            summary.phase_failed(
                "commit-roster",
                &format!(
                    "range {range} did not read: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            );
            return;
        }
        Err(error) => {
            summary.phase_failed("commit-roster", &format!("not measured — {error}"));
            return;
        }
    };
    let commits: Vec<_> = String::from_utf8_lossy(&out.stdout)
        .split('\u{1}')
        .filter_map(|commit| commit.trim().split_once('\u{0}'))
        .map(|(name, message)| (name.to_string(), message.to_string()))
        .collect();
    let violations = jojobot_roster::off_roster_handles_in(&commits);
    if violations.is_empty() {
        summary.phase_ok(
            "commit-roster",
            &format!("{} branch commit messages", commits.len()),
        );
    } else {
        summary.phase_failed("commit-roster", &format!("{} offence(s)", violations.len()));
        summary.note(&jojobot_roster::refusal(&violations));
    }
}
