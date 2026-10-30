//! The watcher that stops its job as failed when a sibling job has failed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// One job as the forge lists it, in the shape its jobs endpoint answered.
fn job(id: u32, attempt: u32, name: &str, status: &str) -> String {
    format!(
        "{{\"id\":{id},\"run_id\":1630,\"attempt\":{attempt},\"handle\":\"31a008c8\",\
         \"repo_id\":1,\"owner_id\":1,\"name\":\"{name}\",\"needs\":null,\
         \"runs_on\":[\"nix\"],\"task_id\":{},\"status\":\"{status}\"}}",
        id + 4000
    )
}

/// Run the watcher in job `rooms` over `jobs` and a command that lasts three
/// polls. Returns the posts the fake forge received (none are expected), what the
/// command printed, what the watcher said on stderr, and the exit code.
fn watched(tag: &str, jobs: &[String], command: &str) -> (String, String, String, i32) {
    let dir: PathBuf = std::env::temp_dir().join(format!("stop-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("jobs.json"), format!("[{}]", jobs.join(","))).unwrap();
    support::write_executable(
        &dir.join("bin/curl"),
        &format!(
            "#!/bin/sh\ncase \" $* \" in\n  *\" -X POST \"*) echo \"$*\" >> {0}/posts; sed 's/^/stdin: /' >> {0}/posts ;;\n  *) cat {0}/jobs.json ;;\nesac\n",
            dir.display()
        ),
    );
    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/stop-with-siblings");
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap()
    );
    let out = support::output_fenced(
        Command::new(tool)
            .args(["sh", "-c", command])
            .env("PATH", path)
            .env("TOKEN", "the-job-token")
            .env("GITHUB_API_URL", "http://forge.invalid/api/v1")
            .env("GITHUB_REPOSITORY", "homer/jojobot")
            .env("GITHUB_RUN_ID", "1630")
            .env("GITHUB_JOB", "rooms")
            .env("STOP_POLL_SECONDS", "1"),
    )
    .unwrap();
    // A watcher that outlived the job would post within a poll of its end.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let posts = fs::read_to_string(dir.join("posts")).unwrap_or_default();
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (posts, said, stderr, out.status.code().unwrap_or(-1))
}

#[test]
fn watcher_stops_its_job_as_failed_on_a_failed_sibling_and_cancels_nothing() {
    let jobs = [
        job(3209, 1, "report", "failure"),
        job(3210, 1, "rooms", "running"),
    ];
    let (posts, said, stderr, code) = watched("fresh", &jobs, "sleep 3; echo token=${TOKEN-unset}");
    assert_eq!(posts, "", "the run was cancelled: {posts:?}");
    assert_eq!(code, 75, "a stopped job ends with its own code: {stderr:?}");
    assert!(
        stderr.contains("stopped: report failed first; this job has no verdict of its own"),
        "the line does not name the sibling that failed first: {stderr:?}"
    );
    assert!(
        !said.contains("token="),
        "the command was not stopped before it finished: {said:?}"
    );
}

#[test]
fn watcher_names_the_first_failed_sibling_and_the_command_never_sees_the_token() {
    let jobs = [
        job(3208, 1, "check", "failure"),
        job(3209, 1, "report", "failure"),
        job(3210, 1, "rooms", "running"),
    ];
    let (_, _, stderr, code) = watched("first", &jobs, "sleep 3");
    assert_eq!(code, 75, "{stderr:?}");
    assert!(stderr.contains("stopped: check failed first"), "{stderr:?}");

    // No sibling has failed, so the command finishes with its own status.
    let jobs = [
        job(3209, 1, "report", "running"),
        job(3210, 1, "rooms", "running"),
    ];
    let (posts, said, stderr, code) = watched("quiet", &jobs, "echo token=${TOKEN-unset}; exit 3");
    assert_eq!(posts, "");
    assert!(
        said.contains("token=unset"),
        "the command saw the token: {said:?}"
    );
    assert_eq!(code, 3, "the command's own status is the job's: {stderr:?}");
}

#[test]
fn watcher_ignores_a_failure_an_earlier_attempt_left() {
    // A re-run of rooms alone: the forge resets rooms (attempt 2) and leaves
    // check, which failed in attempt 1, as it was.
    let jobs = [
        job(3209, 1, "check", "failure"),
        job(3210, 2, "rooms", "running"),
    ];
    let (posts, _, _, code) = watched("stale", &jobs, "sleep 3");
    assert_eq!(posts, "", "a stale failure cancelled the re-run: {posts:?}");
    assert_eq!(code, 0, "a stale failure stopped the re-run");
}

#[test]
fn watcher_ignores_its_own_jobs_failure() {
    let jobs = [job(3210, 1, "rooms", "failure")];
    let (posts, _, _, code) = watched("own", &jobs, "sleep 3");
    assert_eq!(
        posts, "",
        "a job cancelled the run for its own failure: {posts:?}"
    );
    assert_eq!(code, 0, "a job stopped itself for its own failure");
}

#[test]
fn watcher_dies_with_a_terminated_job() {
    // A sibling has failed, so a watcher left alive would cancel within a poll.
    let jobs = [
        job(3209, 1, "report", "failure"),
        job(3210, 1, "rooms", "running"),
    ];
    let (posts, _, _, code) = watched("term", &jobs, "kill -TERM $PPID; sleep 5");
    assert_eq!(posts, "", "the watcher outlived the job: {posts:?}");
    assert_eq!(code, 143, "a terminated job ends with the signal's status");
}
