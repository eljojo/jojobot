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
