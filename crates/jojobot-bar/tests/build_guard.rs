//! **The build guard refuses a target dir that is not the line's own, and
//! cleans up after a build that was killed partway.** A stub stands in for
//! cargo: it only reports a target directory, so no real cache is corrupted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod support;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-guard-{tag}"
    )));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A unit directory of the incremental cache with one file in it.
fn unit(target: &Path, name: &str) -> PathBuf {
    let dir = target.join("debug/incremental").join(name).join("session");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("data"), name).unwrap();
    dir.parent().unwrap().to_path_buf()
}

/// Run the guard over `command` from a copy of the script inside a checkout
/// named `checkout`. The stub cargo reports `reported` as the target directory.
/// The run is not a forge runner's: the marker is removed whatever CI runs this.
fn guard(root: &Path, reported: &Path, checkout: &str, command: &str) -> Output {
    guard_as(root, reported, checkout, command, None)
}

/// As `guard`, with `GITHUB_ACTIONS` set to `runner` when it is given.
fn guard_as(
    root: &Path,
    reported: &Path,
    checkout: &str,
    command: &str,
    runner: Option<&str>,
) -> Output {
    let script = root
        .join("checkouts")
        .join(checkout)
        .join("scripts/build-guard");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-guard"),
        &script,
    )
    .unwrap();
    let cargo = root.join("cargo");
    support::write_executable(
        &cargo,
        &format!(
            "#!/bin/sh\necho '{{\"target_directory\":\"{}\"}}'\n",
            reported.display()
        ),
    );
    let mut run = Command::new("python3");
    run.arg(&script)
        .args(["sh", "-c", command])
        .env("CARGO", &cargo)
        .env_remove("GITHUB_ACTIONS");
    if let Some(value) = runner {
        run.env("GITHUB_ACTIONS", value);
    }
    support::output_fenced(&mut run).expect("the guard runs")
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// **A target dir that is shared or another line's refuses before the command
/// starts**, and a dir named after the checkout, or the checkout's own `target`,
/// runs it. The checkout is the only source: no environment variable is read.
#[test]
fn a_target_dir_that_is_not_the_lines_own_refuses_before_the_command_starts() {
    let root = scratch("own");
    let (own, other) = (root.join("own"), root.join("other"));
    let started = root.join("started");
    let command = format!("touch {}", started.display());
    for (reported, checkout) in [(&root, "own"), (&other, "own"), (&own, "other")] {
        let out = guard(&root, reported, checkout, &command);
        assert_eq!(out.status.code(), Some(2), "{}", said(&out));
        assert!(said(&out).contains("expected"), "{}", said(&out));
        assert!(!started.exists(), "the command started: {}", said(&out));
    }
    let local = root.join("checkouts/own/target");
    for reported in [&own, &local] {
        let _ = fs::remove_file(&started);
        let out = guard(&root, reported, "own", &command);
        assert_eq!(out.status.code(), Some(0), "{}", said(&out));
        assert!(started.exists());
    }
    let _ = fs::remove_dir_all(root);
}

/// **A forge runner's job owns the target dir its runner sets**, a shared one
/// named after neither the checkout nor a line. The same dir refuses off a runner,
/// and for any value of `GITHUB_ACTIONS` but `true`.
#[test]
fn a_forge_runners_shared_target_dir_runs_the_command_and_a_line_s_does_not() {
    let root = scratch("runner");
    let shared = root.join("cargo-target");
    let started = root.join("started");
    let command = format!("touch {}", started.display());
    for runner in [None, Some("false"), Some("")] {
        let out = guard_as(&root, &shared, "hostexecutor", &command, runner);
        assert_eq!(out.status.code(), Some(2), "{}", said(&out));
        assert!(!started.exists(), "the command started: {}", said(&out));
    }
    let out = guard_as(&root, &shared, "hostexecutor", &command, Some("true"));
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(started.exists());
    let _ = fs::remove_dir_all(root);
}

/// **A build killed partway leaves a marker, and the next run removes exactly
/// the units the dead run changed or created**: an untouched unit and another
/// line's cache stay.
#[test]
fn a_killed_build_is_cleaned_of_the_units_it_touched_and_nothing_else() {
    let root = scratch("killed");
    let own = root.join("own");
    let other_line = unit(&root.join("other"), "theirs");
    let untouched = unit(&own, "untouched");
    let touched = unit(&own, "touched");
    // The command changes a file of one existing unit, creates a unit, and
    // kills the guard (its parent) the way a timeout's SIGKILL would.
    let command = format!(
        "echo more > {t}/session/data; touch -d @1 {t}/session/data; mkdir -p {n}/session; \
         echo x > {n}/session/data; kill -9 $PPID; sleep 1",
        t = touched.display(),
        n = own.join("debug/incremental/created").display()
    );
    let out = guard(&root, &own, "own", &command);
    assert_ne!(out.status.code(), Some(0), "the guard was killed");
    assert!(own.join(".build-guard.json").exists(), "no marker left");

    let out = guard(&root, &own, "own", "true");
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!touched.exists(), "the changed unit stayed");
    assert!(!own.join("debug/incremental/created").exists());
    assert!(untouched.exists(), "an untouched unit was removed");
    assert!(other_line.exists(), "another line's cache was touched");
    assert!(!own.join(".build-guard.json").exists(), "the marker stayed");
    let _ = fs::remove_dir_all(root);
}

/// **A build that ends by itself removes nothing**, whatever it changed.
#[test]
fn a_build_that_ends_by_itself_removes_nothing() {
    let root = scratch("clean");
    let own = root.join("own");
    let touched = unit(&own, "touched");
    let command = format!("echo more > {}/session/data", touched.display());
    for _ in 0..2 {
        let out = guard(&root, &own, "own", &command);
        assert_eq!(out.status.code(), Some(0), "{}", said(&out));
        assert!(touched.exists(), "a clean run removed a unit");
        assert!(!own.join(".build-guard.json").exists());
    }
    let _ = fs::remove_dir_all(root);
}

/// **Cleanup is refused, and nothing is removed or started**, when a unit is a
/// symlink out of the cache or the run that left the marker is still alive.
#[test]
fn cleanup_is_refused_for_a_symlink_out_of_the_cache_and_for_a_live_owner() {
    let root = scratch("refused");
    let own = root.join("own");
    let marker = own.join(".build-guard.json");
    unit(&own, "touched");
    let started = root.join("started");
    let command = format!("kill -9 $PPID; sleep 1; touch {}", started.display());
    guard(&root, &own, "own", &command);
    assert!(marker.exists(), "no marker left");

    let kept = root.join("outside");
    fs::create_dir_all(&kept).unwrap();
    fs::write(kept.join("file"), "keep").unwrap();
    std::os::unix::fs::symlink(&kept, own.join("debug/incremental/escape")).unwrap();
    let out = guard(&root, &own, "own", "true");
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    assert!(said(&out).contains("escape"), "{}", said(&out));
    assert!(kept.join("file").exists());
    assert!(marker.exists(), "a refused cleanup dropped the marker");
    fs::remove_file(own.join("debug/incremental/escape")).unwrap();

    let mut state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker).unwrap()).unwrap();
    state["pid"] = std::process::id().into();
    fs::write(&marker, state.to_string()).unwrap();
    let after = root.join("after");
    let out = guard(&root, &own, "own", &format!("touch {}", after.display()));
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    assert!(
        said(&out).contains(&format!("pid {}", std::process::id())),
        "{}",
        said(&out)
    );
    assert!(!after.exists(), "the command started beside a live owner");
    let _ = fs::remove_dir_all(root);
}

/// **A symlinked incremental root leads out of the owned target**, so cleanup is
/// refused and another line's cache is left whole, even though every unit under
/// the link resolves inside the link's own target.
#[test]
fn an_incremental_root_that_leads_out_of_the_target_is_never_cleaned() {
    let root = scratch("root-link");
    let own = root.join("own");
    unit(&own, "touched");
    let started = root.join("started");
    guard(&root, &own, "own", "kill -9 $PPID; sleep 1");
    assert!(own.join(".build-guard.json").exists(), "no marker left");

    let theirs = unit(&root.join("other"), "theirs");
    fs::remove_dir_all(own.join("debug/incremental")).unwrap();
    std::os::unix::fs::symlink(
        root.join("other/debug/incremental"),
        own.join("debug/incremental"),
    )
    .unwrap();
    let out = guard(&root, &own, "own", &format!("touch {}", started.display()));
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    assert!(theirs.exists(), "another line's unit was removed");
    assert!(!started.exists(), "the command started");
    let _ = fs::remove_dir_all(root);
}

/// **The dead run's whole process group is reaped before anything is measured**,
/// even when its leader has exited and a descendant still writes units.
#[test]
fn a_descendant_of_a_dead_run_is_reaped_before_the_units_are_measured() {
    let root = scratch("reap");
    let own = root.join("own");
    let kept = unit(&own, "kept");
    let pidfile = root.join("writer.pid");
    let units = own.join("debug/incremental");
    // The leader starts a writer in its group, kills the guard and exits.
    let command = format!(
        "(i=0; while :; do mkdir -p {u}/late-$i/s; i=$((i+1)); sleep 0.005; done) </dev/null >/dev/null 2>&1 & \
         echo $! > {p}; sleep 0.3; kill -9 $PPID",
        u = units.display(),
        p = pidfile.display()
    );
    guard(&root, &own, "own", &command);
    let writer = fs::read_to_string(&pidfile).unwrap().trim().to_string();
    let alive = |pid: &str| {
        Command::new("kill")
            .args(["-0", pid])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    assert!(alive(&writer), "the writer ended with its leader");

    let out = guard(&root, &own, "own", "true");
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!alive(&writer), "the writer outlived the recovery");
    let late: Vec<String> = fs::read_dir(&units)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("late-"))
        .collect();
    assert!(late.is_empty(), "units the writer made were left: {late:?}");
    assert!(kept.exists());
    let _ = fs::remove_dir_all(root);
}

/// **The command does not start until the marker is on disk.** A marker that
/// cannot be written stops the run before anything compiles.
#[test]
fn a_command_does_not_start_when_the_marker_cannot_be_written() {
    let root = scratch("handshake");
    let own = root.join("own");
    fs::create_dir_all(&own).unwrap();
    // A dangling link: the marker is absent, and writing it fails.
    std::os::unix::fs::symlink(
        root.join("no/such/dir/marker"),
        own.join(".build-guard.json"),
    )
    .unwrap();
    let started = root.join("started");
    let out = guard(&root, &own, "own", &format!("touch {}", started.display()));
    assert_ne!(out.status.code(), Some(0), "{}", said(&out));
    // The child may be released a moment after the guard decides; give it time.
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!started.exists(), "the command started without a marker");
    let _ = fs::remove_dir_all(root);
}

/// **A command that a signal ends leaves the marker**, the guard still alive:
/// the build was interrupted, so the next run cleans what it touched.
#[test]
fn a_command_ended_by_a_signal_leaves_the_marker_and_the_next_run_cleans_up() {
    let root = scratch("signal");
    let own = root.join("own");
    let untouched = unit(&own, "untouched");
    let touched = unit(&own, "touched");
    let command = format!(
        "echo more > {t}/session/data; touch -d @1 {t}/session/data; kill -9 $$",
        t = touched.display()
    );
    let out = guard(&root, &own, "own", &command);
    assert_eq!(out.status.code(), Some(137), "{}", said(&out));
    assert!(
        own.join(".build-guard.json").exists(),
        "the marker was dropped"
    );

    let out = guard(&root, &own, "own", "true");
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!touched.exists(), "the interrupted unit stayed");
    assert!(untouched.exists());
    let _ = fs::remove_dir_all(root);
}

/// **Both local loops run through the guard**, ahead of the first cargo run.
#[test]
fn make_dev_and_make_narrow_run_through_the_guard() {
    let makefile =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    assert!(makefile.contains("BUILD_GUARD ?= python3 scripts/build-guard"));
    for target in ["dev", "narrow"] {
        assert!(
            makefile.contains(&format!(
                "$(BUILD_GUARD) python3 scripts/build-metrics run {target} --"
            )),
            "make {target} does not run through the build guard"
        );
    }
}

/// Run `inner` as the build under the guard and the metrics wrapper, the way
/// the Makefile chains them. A stub `curl` takes the samples, so the local push
/// needs no network; `metrics_on` sets the opt-in URL and unset leaves it out.
fn metered(root: &Path, own: &Path, inner: &str, metrics_on: bool) -> Output {
    support::write_executable(&root.join("curl"), "#!/bin/sh\ncat > /dev/null\n");
    let metrics = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-metrics");
    let opt_in = if metrics_on {
        "export JOJOBOT_DEV_METRICS_URL=http://metrics.invalid/import"
    } else {
        "unset JOJOBOT_DEV_METRICS_URL"
    };
    let command = format!(
        "cd {root} && PATH={root}:$PATH && export PATH && {opt_in} && \
         exec python3 {metrics} run narrow -- sh -c '{inner}'",
        root = root.display(),
        metrics = metrics.display(),
    );
    guard(root, own, "own", &command)
}

/// **A build killed by a signal leaves the marker whether or not build metrics
/// are on.** The metrics wrapper reports the child's end as it was, so the guard,
/// which alone decides that a build was interrupted, sees the signal. The same
/// kill with metrics unset is the control, and a build that fails by itself
/// beside it removes the marker, so a marker that stays is not a marker that
/// never goes.
#[test]
fn a_killed_build_leaves_the_marker_with_build_metrics_on_and_off() {
    for metrics_on in [true, false] {
        let root = scratch(if metrics_on {
            "metered-kill"
        } else {
            "bare-kill"
        });
        let own = root.join("own");
        unit(&own, "touched");
        let out = metered(&root, &own, "kill -9 $$", metrics_on);
        assert_eq!(out.status.code(), Some(137), "{}", said(&out));
        assert!(
            own.join(".build-guard.json").exists(),
            "the marker was dropped with metrics_on={metrics_on}"
        );
        let _ = fs::remove_dir_all(root);
    }

    let root = scratch("metered-fail");
    let own = root.join("own");
    unit(&own, "touched");
    let out = metered(&root, &own, "exit 1", true);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        !own.join(".build-guard.json").exists(),
        "a build that failed by itself kept the marker"
    );
    let _ = fs::remove_dir_all(root);
}
