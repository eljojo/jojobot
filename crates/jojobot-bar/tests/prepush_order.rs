//! **`make prepush` runs the flake check beside the bar steps, and every step
//! still runs and reports.**
//!
//! The flake check builds on a remote builder and takes most of an hour, so
//! running it last made a prepush the sum of its steps. It now starts with the
//! bar steps and runs concurrently with them, while the bar steps stay serial
//! among themselves: they share the runner and its target directory.
//!
//! The cases run the real `Makefile`'s `prepush` target with every step's
//! recipe replaced by a stub that records when it started and ended and can be
//! told to fail. A scratch directory holds a `Makefile` that includes the
//! real one and then redefines the steps. The scratch directory is the working
//! directory, because a sub-make reads the `Makefile` of its working directory
//! and not the file its parent was given with `-f`.

use std::path::{Path, PathBuf};
use std::process::Command;

const BAR_STEPS: [&str; 4] = ["check", "rooms", "year", "nix-build"];

fn real_makefile() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../Makefile")
        .canonicalize()
        .expect("the repository's Makefile")
}

fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-prepush-order-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// One step as the stub recorded it, in nanoseconds since the epoch.
#[derive(Debug)]
struct Step {
    name: String,
    start: u128,
    end: u128,
}

/// The prepush target with stub steps. `failing` names the one step that
/// exits 1. Returns the exit code and the steps that ran.
fn prepush(failing: Option<&str>) -> (i32, Vec<Step>) {
    let dir = scratch();
    let log = dir.join("steps.log");
    // The bar steps take 0.4 s each and the flake check 2.5 s, so a flake check
    // that starts with the bar overlaps every bar step, and one
    // that starts after them overlaps none.
    let stub = |name: &str, seconds: &str| {
        let outcome = if failing == Some(name) {
            "exit 1"
        } else {
            "true"
        };
        format!(
            "{name}:\n\t@echo \"{name} start $$(date +%s%N)\" >> {log}; sleep {seconds}; \
             echo \"{name} end $$(date +%s%N)\" >> {log}; {outcome}\n",
            log = log.display()
        )
    };
    let mut makefile = format!("include {}\n", real_makefile().display());
    for name in BAR_STEPS {
        makefile.push_str(&stub(name, "0.4"));
    }
    makefile.push_str(&stub("flake-check", "2.5"));
    std::fs::write(dir.join("Makefile"), makefile).expect("write the scratch Makefile");

    let ran = Command::new("make")
        .current_dir(&dir)
        .args(["-k", "prepush"])
        .output()
        .expect("make runs");
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let mut steps: Vec<Step> = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(name), Some(what), Some(at)) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        let at: u128 = at.parse().expect("a time");
        if what == "start" {
            steps.push(Step {
                name: name.to_string(),
                start: at,
                end: 0,
            });
        } else if let Some(step) = steps.iter_mut().find(|s| s.name == name) {
            step.end = at;
        }
    }
    (ran.status.code().expect("make exits"), steps)
}

fn step<'a>(steps: &'a [Step], name: &str) -> &'a Step {
    steps
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("{name} did not run: {steps:?}"))
}

/// **The flake check overlaps the bar steps, and the bar steps stay in order and
/// one at a time.**
#[test]
fn the_flake_check_runs_beside_the_bar_steps_and_the_bar_steps_stay_serial() {
    let (code, steps) = prepush(None);
    assert_eq!(code, 0, "{steps:?}");
    let flake = step(&steps, "flake-check");
    let first = step(&steps, "check");
    let last = step(&steps, "nix-build");
    assert!(
        flake.start < first.end && flake.end > last.start,
        "the flake check does not overlap the bar steps: {steps:?}"
    );
    for pair in BAR_STEPS.windows(2) {
        let (before, after) = (step(&steps, pair[0]), step(&steps, pair[1]));
        assert!(
            after.start >= before.end,
            "{} started before {} ended: {steps:?}",
            pair[1],
            pair[0]
        );
    }
}

/// **A red flake check leaves every bar step run, and the target red.**
#[test]
fn a_red_flake_check_does_not_hide_the_bar_steps() {
    let (code, steps) = prepush(Some("flake-check"));
    assert_ne!(code, 0, "{steps:?}");
    for name in BAR_STEPS {
        assert!(
            step(&steps, name).end > 0,
            "{name} did not finish: {steps:?}"
        );
    }
}

/// **A red bar step leaves the later bar steps and the flake check run, and the
/// target red.** The first step and a middle one are made red in turn.
#[test]
fn a_red_bar_step_does_not_hide_the_steps_after_it_or_the_flake_check() {
    for red in ["check", "year"] {
        let (code, steps) = prepush(Some(red));
        assert_ne!(code, 0, "{red}: {steps:?}");
        for name in BAR_STEPS.iter().chain(&["flake-check"]) {
            assert!(
                step(&steps, name).end > 0,
                "{name} did not finish after {red} failed: {steps:?}"
            );
        }
    }
}

/// **A dry run lists every step and runs none.**
#[test]
fn a_dry_run_of_prepush_lists_every_step() {
    let ran = Command::new("make")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(["-n", "prepush"])
        .output()
        .expect("make runs");
    let text = String::from_utf8_lossy(&ran.stdout).to_string();
    assert!(ran.status.success(), "{text}");
    for step in [
        "jojobot-bar -- check",
        "jojobot-bar -- rooms",
        "make room ROOM=year",
        "nix build",
        "nix flake check",
    ] {
        assert!(
            text.contains(step),
            "`make -n prepush` omits {step}: {text}"
        );
    }
}
