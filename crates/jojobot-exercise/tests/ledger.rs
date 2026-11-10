//! **The ledger the repository keeps is the ledger the shipped rooms make.**
//!
//! `ledger/room-assertions.tsv` lists every lock of every shipped room and every
//! integration case of this crate (rule 767). A lock or a case added, removed or
//! reworded without the file moving fails here; the file is rewritten with
//! `LEDGER_UPDATE=1`.
//!
//! The mutation sample's manifest is checked by `scripts/mutant-sample --check`
//! and not here: a test that reads the sites fails under every mutant, and a lane
//! that runs it would record a kill no behavioural oracle made.

use jojobot_exercise::ledger;

fn file() -> std::path::PathBuf {
    jojobot_exercise::expectations::room_document("ledger/room-assertions.tsv")
}

#[test]
fn the_kept_ledger_is_the_one_the_shipped_rooms_make() {
    let made = ledger::render();
    if std::env::var_os("LEDGER_UPDATE").is_some() {
        std::fs::create_dir_all(file().parent().unwrap()).unwrap();
        std::fs::write(file(), &made).unwrap();
    }
    let kept = std::fs::read_to_string(file()).unwrap_or_else(|e| {
        panic!(
            "the ledger at {} must exist (LEDGER_UPDATE=1 writes it): {e}",
            file().display()
        )
    });
    assert!(
        kept == made,
        "the kept ledger differs from the shipped rooms; LEDGER_UPDATE=1 rewrites it"
    );
}

/// Every row has every column, every lock of the rooms has a row, and every
/// integration test has a row, counted a second way.
#[test]
fn every_row_has_all_its_columns_and_every_lock_and_case_has_a_row() {
    let made = ledger::render();
    let columns = ledger::HEADER.split('\t').count();
    for row in made.lines() {
        assert_eq!(row.split('\t').count(), columns, "{row}");
    }
    let rows = |kind: &str| {
        made.lines()
            .skip(1)
            .filter(|row| row.split('\t').next() == Some(kind))
            .count()
    };
    let locks = jojobot_exercise::failability::shipped_locks().len();
    assert!(locks > 0);
    assert_eq!(rows("lock"), locks);
    assert_eq!(rows("case"), ledger::counted_cases());
}

/// **A live lock reads the room as the run left it, so its history is the final
/// room; the other two windows read a stored boundary, so theirs is the phase.**
/// The authored phase stays in its own column.
#[test]
fn ledger_lock_history_follows_its_window() {
    let made = ledger::render();
    let mut live = 0;
    let mut bounded = 0;
    for row in made.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        if cells[0] != "lock" {
            continue;
        }
        let (phase, window, history) = (cells[3], cells[4], cells[10]);
        match window {
            "live" => {
                live += 1;
                assert!(history.contains("end of the room"), "{row}");
                assert!(!history.contains("through"), "{row}");
            }
            _ if !phase.is_empty() => {
                bounded += 1;
                assert!(history.contains(&format!("through {phase}")), "{row}");
            }
            _ => {}
        }
    }
    assert!(live > 0 && bounded > 0, "both windows are exercised");
}

/// **A case row carries the assertions of its body**, so taking one out of a test
/// whose name and comment stay the same changes the ledger.
#[test]
fn ledger_case_row_carries_its_assertions() {
    let body = "fn a_case() {\n    let n = 1;\n    assert!(\n        n > 0,\n        \"it holds\"\n    );\n    assert_eq!(n, 1);\n}\n";
    let held = ledger::assertions_of(body);
    assert_eq!(held.len(), 2, "{held:?}");
    assert!(held[0].contains("n > 0"), "{held:?}");
    assert!(held[1].contains("assert_eq!(n, 1)"), "{held:?}");
    let without = body.replace("    assert_eq!(n, 1);\n", "");
    assert_ne!(held, ledger::assertions_of(&without));

    let made = ledger::render();
    let blank = made
        .lines()
        .skip(1)
        .map(|row| row.split('\t').collect::<Vec<_>>())
        .filter(|cells| cells[0] == "case" && cells[7].is_empty())
        .count();
    assert_eq!(blank, 0, "a case holds an assertion or says it holds none");
}

/// **A test that asserts through a helper of its own file carries that helper's
/// assertions**, each under the call that reaches it, so taking the helper's
/// `assert_eq!` out changes every row that calls it.
#[test]
fn ledger_case_row_carries_the_assertions_of_the_helpers_it_calls() {
    let file = "async fn check(n: usize, m: usize) {\n    assert_eq!(n, m, \"same\");\n}\n\n#[tokio::test]\nasync fn a_case() {\n    check(1, 1).await;\n}\n";
    let body = "async fn a_case() {\n    check(1, 1).await;\n}\n";
    let held = ledger::assertions_through_helpers(body, file);
    assert_eq!(held.len(), 1, "{held:?}");
    assert!(held[0].contains("check(1, 1)"), "{held:?}");
    assert!(held[0].contains("assert_eq!(n, m"), "{held:?}");
    let without = file.replace("    assert_eq!(n, m, \"same\");\n", "");
    assert!(ledger::assertions_through_helpers(body, &without).is_empty());

    let made = ledger::render();
    let row = made
        .lines()
        .find(|row| {
            row.starts_with("case\t")
                && row.contains("::january_launch_day_in_a_sentence_reds_only_the_launch_lock\t")
        })
        .expect("the agency room case has a row");
    let oracle = row.split('\t').nth(7).unwrap();
    assert!(oracle.contains("discriminates(0, 0)"), "{oracle}");
    assert!(oracle.contains("assert_eq!(held, all_but"), "{oracle}");
}

/// **A directory of its own for one run of the mutation sample**: a git checkout
/// with one commit, a site file and a thirty-row manifest over it, a stand-in
/// `scripts/sabotage` that writes the file `sabotaged` and says the mutant was
/// killed, and a stand-in `make` that writes the file `lane_ran` and fails. The
/// real manifest's sites are the ones a mutant changes, and a case that read them
/// would fail under every mutant and record a kill no behaviour made.
struct SampleDir {
    dir: std::path::PathBuf,
    manifest: std::path::PathBuf,
    sha: String,
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn an_executable(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn a_sample_dir(label: &str) -> SampleDir {
    let dir = std::env::temp_dir().join(format!("mutant-sample-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let site = dir.join("site.txt");
    let tokens: Vec<String> = (0..30).map(|n| format!("SITE{n:02}")).collect();
    std::fs::write(&site, tokens.join("\n")).unwrap();
    let mut manifest = String::from("file\tfrom\tto\n");
    for token in &tokens {
        manifest.push_str(&format!("{}\t{token}\t{token}X\n", site.display()));
    }
    let manifest_path = dir.join("mutants.tsv");
    std::fs::write(&manifest_path, manifest).unwrap();
    an_executable(
        &dir.join("scripts/sabotage"),
        "#!/bin/sh\ntouch sabotaged\necho 'SABOTAGE VERDICT RED: stand-in'\n",
    );
    an_executable(&dir.join("bin/make"), "#!/bin/sh\ntouch lane_ran\nexit 1\n");
    git_in(&dir, &["init", "-q"]);
    git_in(&dir, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let sha = git_in(&dir, &["rev-parse", "HEAD"]);
    SampleDir {
        dir,
        manifest: manifest_path,
        sha,
    }
}

/// **A stand-in for the forge**: it answers the two reads the sample makes, the
/// list of runs and the jobs of one run, from the bodies given, and stops when the
/// returned flag is set.
fn a_forge(
    runs: serde_json::Value,
    jobs: serde_json::Value,
) -> (
    String,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stopped.load(Ordering::SeqCst) {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 1024];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => seen.extend_from_slice(&chunk[..n]),
                }
            }
            let text = String::from_utf8_lossy(&seen).to_string();
            let path = text
                .lines()
                .next()
                .and_then(|line| line.split(' ').nth(1))
                .unwrap_or("")
                .to_string();
            let body = if path.contains("/jobs") {
                jobs.to_string()
            } else if path.starts_with("/actions/runs") {
                runs.to_string()
            } else {
                String::new()
            };
            let status = if body.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (api, stop, handle)
}

/// One run of the sample in its directory, against the forge stand-in.
fn sampled(dir: &SampleDir, api: &str, args: &[&str]) -> std::process::Output {
    let root = jojobot_exercise::expectations::room_document("../..");
    let path = format!(
        "{}:{}",
        dir.dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    std::process::Command::new("python3")
        .current_dir(&dir.dir)
        .env("MUTANT_MANIFEST", &dir.manifest)
        .env("WAIT_RUN_API", api)
        .env("PATH", path)
        .arg(root.join("scripts/mutant-sample"))
        .args(["--from", "0", "--count", "1"])
        .args(args)
        .output()
        .unwrap()
}

fn said(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stdout).to_string() + &String::from_utf8_lossy(&run.stderr)
}

/// A kill means something only against a lane that passes unmutated. The sample
/// runs the lane once untouched and stops, mutating nothing and recording nothing,
/// when that run is red.
///
/// **The case runs the tool on a manifest and a file of its own**, in a directory
/// of its own: see [`SampleDir`].
#[test]
fn the_mutation_sample_stops_when_its_lane_is_red_before_any_mutation() {
    let one = a_sample_dir("baseline");
    let run = sampled(&one, "", &["--", "false"]);
    let said = said(&run);
    let mutated = one.dir.join("site.txt").exists()
        && std::fs::read_to_string(one.dir.join("site.txt")).unwrap()
            != (0..30)
                .map(|n| format!("SITE{n:02}"))
                .collect::<Vec<_>>()
                .join("\n");
    let sabotaged = one.dir.join("sabotaged").exists();
    let recorded = one.dir.join("target").exists();
    let _ = std::fs::remove_dir_all(&one.dir);
    assert_eq!(
        run.status.code(),
        Some(3),
        "a red lane before any mutation exits 3: {said}"
    );
    assert!(
        said.contains("baseline"),
        "the refusal names the baseline: {said}"
    );
    assert!(!mutated && !sabotaged, "no site was mutated: {said}");
    assert!(!recorded, "nothing is recorded: {said}");
}

/// **A pull-request run's job on the same commit stands in for the sample's own
/// baseline.** That job is the lane run unmutated, so the sample does not run it
/// again: the stand-in `make`, which fails, is never called, and the mutant gets
/// its verdict.
#[test]
fn a_pull_request_check_job_on_this_commit_stands_for_the_baseline() {
    let one = a_sample_dir("proved");
    let (api, stop, forge) = a_forge(
        serde_json::json!({"workflow_runs": [{"id": 7, "index_in_repo": 70,
            "commit_sha": one.sha, "workflow_id": "pull-request.yml", "status": "success"}]}),
        serde_json::json!([{"name": "static", "status": "success"},
            {"name": "check", "status": "success"}]),
    );
    let run = sampled(&one, &api, &["--baseline-run", "7", "--", "make", "check"]);
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    forge.join().unwrap();
    let said = said(&run);
    let record = std::fs::read_to_string(one.dir.join("target/mutants/killed.tsv"));
    let lane_ran = one.dir.join("lane_ran").exists();
    let _ = std::fs::remove_dir_all(&one.dir);
    assert_eq!(run.status.code(), Some(0), "{said}");
    assert!(!lane_ran, "the lane was not run for a baseline: {said}");
    assert!(
        record.is_ok_and(|rows| rows.contains("\tRED")),
        "the mutant got its verdict: {said}"
    );
}

/// One way a named run fails to prove the baseline.
struct NotAProof {
    wrong: &'static str,
    /// The commit the run is on, the checkout's own when none.
    commit: Option<&'static str>,
    workflow: &'static str,
    /// The one job the forge lists for the run that the case is about, and its status.
    job_name: &'static str,
    job_status: &'static str,
    /// Whether a job of another name that passed is listed before it.
    beside: bool,
    /// Whether the forge lists the run at all.
    listed: bool,
    lane: &'static [&'static str],
}

/// **A baseline named by a run that does not prove it is refused before any
/// mutation**: another commit, no such run, a failed job, a failed job beside a
/// passing one, another workflow, the wrong lane's job, or a lane no pull-request
/// job runs. Each leaves nothing recorded and calls no sabotage, and the refusal
/// says which input it refused.
#[test]
fn a_baseline_run_that_does_not_prove_the_lane_is_refused() {
    const CHECK: &[&str] = &["make", "check"];
    let proof = |wrong, lane| NotAProof {
        wrong,
        commit: None,
        workflow: "pull-request.yml",
        job_name: "check",
        job_status: "success",
        beside: false,
        listed: true,
        lane,
    };
    let cases = [
        NotAProof {
            commit: Some("0000000000000000000000000000000000000000"),
            ..proof("another commit", CHECK)
        },
        NotAProof {
            listed: false,
            ..proof("no such run", CHECK)
        },
        NotAProof {
            job_status: "failure",
            ..proof("a failed job", CHECK)
        },
        // The job is found by its name: a run whose first job passed is not a run
        // whose check job passed.
        NotAProof {
            job_status: "failure",
            beside: true,
            ..proof("a failed check beside a passing job", CHECK)
        },
        NotAProof {
            workflow: "ledger-runs.yml",
            ..proof("another workflow", CHECK)
        },
        // The rooms lane is proved by the rooms job, not by the check job.
        proof("the check job for the rooms lane", &["make", "rooms"]),
        proof("a lane no job runs", &["false"]),
    ];
    for case in cases {
        let one = a_sample_dir("refused");
        let sha = case.commit.map_or_else(|| one.sha.clone(), str::to_string);
        let runs = if case.listed {
            serde_json::json!({"workflow_runs": [{"id": 7, "index_in_repo": 70,
                "commit_sha": sha, "workflow_id": case.workflow, "status": "success"}]})
        } else {
            serde_json::json!({"workflow_runs": []})
        };
        let mut jobs = Vec::new();
        if case.beside {
            jobs.push(serde_json::json!({"name": "static", "status": "success"}));
        }
        jobs.push(serde_json::json!({"name": case.job_name, "status": case.job_status}));
        let (api, stop, forge) = a_forge(runs, serde_json::Value::Array(jobs));
        let mut args = vec!["--baseline-run", "7", "--"];
        args.extend_from_slice(case.lane);
        let run = sampled(&one, &api, &args);
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        forge.join().unwrap();
        let said = said(&run);
        let sabotaged = one.dir.join("sabotaged").exists();
        let recorded = one.dir.join("target").exists();
        let _ = std::fs::remove_dir_all(&one.dir);
        assert_eq!(run.status.code(), Some(2), "{}: {said}", case.wrong);
        assert!(said.contains("baseline-run"), "{}: {said}", case.wrong);
        assert!(
            !sabotaged && !recorded,
            "{}: nothing ran: {said}",
            case.wrong
        );
    }
}

/// **The rooms job of a pull-request run stands in for the rooms lane's
/// baseline**, and a failed check job beside it does not matter: each lane is
/// proved by its own job.
#[test]
fn a_pull_request_rooms_job_stands_for_the_rooms_lane() {
    let one = a_sample_dir("rooms");
    let (api, stop, forge) = a_forge(
        serde_json::json!({"workflow_runs": [{"id": 7, "index_in_repo": 70,
            "commit_sha": one.sha, "workflow_id": "pull-request.yml", "status": "failure"}]}),
        serde_json::json!([{"name": "check", "status": "failure"},
            {"name": "rooms", "status": "success"}]),
    );
    let run = sampled(&one, &api, &["--baseline-run", "7", "--", "make", "rooms"]);
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    forge.join().unwrap();
    let said = said(&run);
    let record = std::fs::read_to_string(one.dir.join("target/mutants/killed.tsv"));
    let lane_ran = one.dir.join("lane_ran").exists();
    let _ = std::fs::remove_dir_all(&one.dir);
    assert_eq!(run.status.code(), Some(0), "{said}");
    assert!(!lane_ran, "the lane was not run for a baseline: {said}");
    assert!(
        record.is_ok_and(|rows| rows.contains("\tRED")),
        "the mutant got its verdict: {said}"
    );
}

/// **An option value that is not a number is a usage refusal, not a traceback**,
/// and nothing runs.
#[test]
fn an_option_value_that_is_not_a_number_is_a_usage_refusal() {
    for args in [
        &["--baseline-run", "x", "--", "make", "check"][..],
        &["--baseline-run", "--", "make", "check"][..],
        &["--from", "x", "--", "make", "check"][..],
    ] {
        let one = a_sample_dir("usage");
        let run = sampled(&one, "", args);
        let said = said(&run);
        let sabotaged = one.dir.join("sabotaged").exists();
        let recorded = one.dir.join("target").exists();
        let _ = std::fs::remove_dir_all(&one.dir);
        assert_eq!(run.status.code(), Some(2), "{args:?}: {said}");
        assert!(said.contains("usage"), "{args:?}: {said}");
        assert!(!said.contains("Traceback"), "{args:?}: {said}");
        assert!(!sabotaged && !recorded, "{args:?}: nothing ran: {said}");
    }
}
