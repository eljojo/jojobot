//! Build SLO samples must be readable Prometheus text without unbounded labels.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

mod support;

fn script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-metrics")
}

fn render(payload: &serde_json::Value) -> String {
    let mut child = support::fenced_spawn(
        Command::new("python3")
            .arg(script())
            .arg("render")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .expect("sample writer");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

// Parse the emitted subset of the exposition grammar independently of the
// writer: a numeric sample, six run labels (plus binary when present), and an optional time.
fn samples(text: &str) -> Vec<(String, BTreeMap<String, String>, f64)> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let (metric, rest) = line.split_once('{').expect("metric and labels");
            assert!(
                metric.starts_with("jojobot_build_")
                    && (metric.ends_with("_seconds")
                        || metric == "jojobot_build_unlisted_binaries"
                        || metric == "jojobot_build_unmeasured_failures")
            );
            let (labels, rest) = rest.split_once("} ").expect("labels and value");
            let labels: BTreeMap<String, String> = labels
                .split(',')
                .map(|label| {
                    let (key, value) = label.split_once('=').expect("label assignment");
                    (
                        key.into(),
                        serde_json::from_str(value).expect("quoted label"),
                    )
                })
                .collect();
            if metric == "jojobot_build_test_binary_seconds" {
                assert_eq!(labels.len(), 7, "{line}");
                assert!(
                    include_str!("../build-metrics-binaries.txt")
                        .lines()
                        .any(|b| b == labels["binary"])
                );
            } else {
                assert_eq!(labels.len(), 6, "{line}");
            }
            for (key, allowed) in [
                (
                    "workflow",
                    &["branch", "prepush", "pull-request", "local", "other"][..],
                ),
                (
                    "job",
                    &[
                        "report", "rooms", "prepush", "build", "check", "year", "dev", "narrow",
                        "other",
                    ][..],
                ),
                (
                    "step",
                    &[
                        "workflow",
                        "job",
                        "fmt",
                        "build",
                        "test_compile",
                        "test_list",
                        "test",
                        "lint",
                        "doc",
                        "dev",
                        "narrow",
                        "unmeasured",
                        "other",
                    ][..],
                ),
                (
                    "outcome",
                    &["success", "failure", "cancelled", "skipped", "unknown"][..],
                ),
                ("branch_class", &["main", "release", "dev", "other"][..]),
                (
                    "runner_host",
                    &[
                        "bart", "lisa", "nelly", "willie", "moe", "wiggum", "local", "other",
                    ][..],
                ),
            ] {
                let value = labels
                    .get(key)
                    .unwrap_or_else(|| panic!("missing {key}: {line}"));
                let rank = key == "step" && (1..=20).any(|r| value == &format!("test-rank-{r:02}"));
                assert!(
                    allowed.contains(&value.as_str()) || rank,
                    "unbounded {key}={value}"
                );
            }
            let mut words = rest.split_whitespace();
            let value: f64 = words.next().unwrap().parse().expect("numeric sample");
            assert!(value.is_finite() && value >= 0.0);
            if let Some(time) = words.next() {
                time.parse::<u64>().expect("millisecond timestamp");
            }
            assert!(words.next().is_none());
            (metric.into(), labels, value)
        })
        .collect()
}

#[test]
fn build_metrics_exposition_is_parseable_and_every_label_is_from_a_closed_set() {
    let text = render(&serde_json::json!({
        "workflow": "branch", "job": "report", "ref": "refs/heads/dev-sol3-slos-923-work",
        "runner": "lisa.example.invalid",
        "samples": [
            {"metric": "duration", "step": "job", "outcome": "success", "seconds": 12.5},
            {"metric": "first_failure", "step": "test", "outcome": "failure", "seconds": 23.25}
        ]
    }));
    let parsed = samples(&text);
    assert_eq!(
        parsed.len(),
        2,
        "the writer must emit both measurements: {text}"
    );
    assert_eq!(parsed[0].0, "jojobot_build_duration_seconds");
    assert_eq!(parsed[0].1["branch_class"], "dev");
    assert_eq!(parsed[0].1["runner_host"], "lisa");
    assert_eq!(parsed[0].2, 12.5);
    assert_eq!(parsed[1].0, "jojobot_build_first_failure_seconds");
    assert_eq!(parsed[1].2, 23.25);

    let hostile = render(&serde_json::json!({
        "workflow": "a-new-workflow", "job": "private-card", "ref": "a-private-branch",
        "runner": "a-new-host", "samples": [
            {"metric": "duration", "step": "private-step", "outcome": "private-outcome", "seconds": 1}
        ]
    }));
    let hostile = samples(&hostile);
    assert_eq!(hostile.len(), 1);
    assert!(
        hostile[0]
            .1
            .values()
            .all(|v| v == "other" || v == "unknown")
    );
}

#[test]
fn build_metrics_local_push_is_opt_in_and_a_failed_push_keeps_the_command_verdict() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("build-metrics-local"));
    std::fs::create_dir_all(&dir).unwrap();
    support::write_executable(
        &dir.join("curl"),
        r#"#!/bin/sh
printf '%s\n' "$@" > "$METRICS_TEST_DIR/argv"
cat > "$METRICS_TEST_DIR/samples"
exit 7
"#,
    );
    let run = |enabled| {
        let mut cmd = Command::new("python3");
        cmd.arg(script())
            .args(["run", "narrow", "--", "sh", "-c", "exit 17"])
            .current_dir(&dir)
            .env(
                "PATH",
                format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
            )
            .env("METRICS_TEST_DIR", &dir)
            .env_remove("JOJOBOT_CI_METRICS_URL")
            .env_remove("JOJOBOT_BUILD_METRICS_FILE")
            .env_remove("JOJOBOT_DEV_METRICS_URL");
        if enabled {
            cmd.env("JOJOBOT_DEV_METRICS_URL", "http://metrics.invalid/import");
        }
        support::output_fenced(&mut cmd).unwrap()
    };
    let disabled = run(false);
    assert_eq!(disabled.status.code(), Some(17));
    assert!(!dir.join("argv").exists(), "no opt-in, no network call");
    let enabled = run(true);
    assert_eq!(
        enabled.status.code(),
        Some(17),
        "push failure must preserve the command's red"
    );
    let argv = std::fs::read_to_string(dir.join("argv")).expect("curl ran");
    assert!(argv.contains("--data-binary\n@-\n"), "{argv}");
    let text = std::fs::read_to_string(dir.join("samples")).unwrap();
    let parsed = samples(&text);
    assert_eq!(parsed.len(), 1, "{text}");
    assert_eq!(parsed[0].1["workflow"], "local");
    assert_eq!(parsed[0].1["job"], "narrow");
    assert_eq!(parsed[0].1["outcome"], "failure");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_metrics_local_sample_setup_failure_keeps_the_command_and_its_verdict() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("build-metrics-setup"));
    std::fs::create_dir_all(&dir).unwrap();
    // A blocked sample directory must not stop the build from starting.
    std::fs::write(dir.join("target"), "occupied").unwrap();
    for code in [0, 17] {
        let out = support::output_fenced(
            Command::new("python3")
                .arg(script())
                .args(["run", "narrow", "--", "sh", "-c"])
                .arg(format!("printf command-ran; exit {code}"))
                .current_dir(&dir)
                .env("JOJOBOT_DEV_METRICS_URL", "http://metrics.invalid/import"),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "command-ran",
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(code));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_metrics_bar_records_wall_time_for_phases_and_each_test_binary() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("build-metrics-bar"));
    std::fs::create_dir_all(&dir).unwrap();
    support::git_init(&dir);
    support::write_executable(
        &dir.join("cargo"),
        r#"#!/bin/sh
case " $* " in
  *" --no-run "*)
    echo '  Executable unittests src/lib.rs (/x/deps/jojobot_domain-1)'
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    exit 0 ;;
esac
case "$1" in
  test)
    sleep 0.1
    echo 'test alpha::first ... ok <0.010s>'
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s' ;;
esac
exit 0
"#,
    );
    let file = dir.join("metrics.jsonl");
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("report")
            .current_dir(&dir)
            .env("CARGO", dir.join("cargo"))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("JOJOBOT_BUILD_METRICS_FILE", &file),
    )
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let records: Vec<serde_json::Value> = std::fs::read_to_string(file)
        .expect("recorded wall times")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for step in ["fmt", "build", "test_compile", "lint"] {
        assert!(
            records
                .iter()
                .any(|r| r["kind"] == "step" && r["step"] == step),
            "{step}: {records:?}"
        );
    }
    let binaries: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "test_binary")
        .collect();
    assert_eq!(binaries.len(), 2, "each compiled test binary: {records:?}");
    for r in binaries {
        assert!(
            r["seconds"].as_f64().unwrap() >= 0.09,
            "wall time, not the suite's 0.01s: {r}"
        );
        assert!(r["ended"].as_f64().unwrap() > 1_000_000_000.0);
    }
    // The stub also prints one named test in the separate doc-test job.
    assert_eq!(records.iter().filter(|r| r["kind"] == "test").count(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_metrics_every_workflow_job_ends_with_a_non_gating_push() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (workflow, jobs) in [
        ("branch", &["report", "rooms"][..]),
        ("prepush", &["prepush"][..]),
        ("pull-request", &["build", "check", "rooms", "year"][..]),
    ] {
        let text = std::fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml")))
            .unwrap();
        assert!(text.contains(&format!("JOJOBOT_METRICS_WORKFLOW: {workflow}\n")));
        assert!(text.contains("JOJOBOT_METRICS_JOB: ${{ forgejo.job }}\n"));
        assert!(text.contains(
            "JOJOBOT_BUILD_METRICS_FILE: target/bar/metrics-${{ forgejo.run_id }}-${{ forgejo.job }}.jsonl\n"
        ));
        for job in jobs {
            let marker = format!("\n  {job}:\n");
            let start = text.find(&marker).expect("workflow job") + marker.len();
            let block: Vec<_> = text[start..]
                .lines()
                .take_while(|line| {
                    line.trim().is_empty()
                        || line.starts_with("    ")
                        || line.trim_start().starts_with('#')
                })
                .collect();
            let last = block
                .iter()
                .rposition(|l| l.starts_with("      - "))
                .expect("last step");
            let last = block[last..].join("\n");
            for wanted in [
                "if: ${{ always() }}",
                "continue-on-error: true",
                "python3 scripts/build-metrics ci",
                "JOJOBOT_METRICS_OUTCOME: ${{ job.status }}",
            ] {
                assert!(
                    last.contains(wanted),
                    "{workflow}/{job}: last step lacks {wanted}: {last}"
                );
            }
        }
    }
    let makefile = std::fs::read_to_string(root.join("Makefile")).unwrap();
    for target in ["dev", "narrow"] {
        assert!(
            makefile.contains(&format!("python3 scripts/build-metrics run {target} --")),
            "{target} must opt in through the same writer"
        );
    }
}

#[test]
fn build_metrics_ci_keeps_the_first_failure_time_and_the_job_wall_time() {
    let mut child = support::fenced_spawn(
        Command::new("python3")
            .args([
                "-c",
                r#"
import importlib.machinery, importlib.util, json, sys
loader = importlib.machinery.SourceFileLoader('build_metrics', sys.argv[1])
spec = importlib.util.spec_from_loader(loader.name, loader)
m = importlib.util.module_from_spec(spec)
loader.exec_module(m)
context = json.load(sys.stdin)
# Only an overall red job has a first-failure sample.
green = m.ci_payload({**context, 'outcome':'success'})
assert not any(s['metric'] == 'first_failure' for s in green['samples'])
print(m.render(m.ci_payload(context)), end='')
"#,
            ])
            .arg(script())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .unwrap();
    child.stdin.take().unwrap().write_all(serde_json::json!({
        "workflow":"branch", "job":"report", "ref":"dev-sol3-slos-923-work", "runner":"lisa",
        "created": 1_000_000_000.0, "started":1_000_000_050.0, "now":1_000_000_150.0,
        "outcome":"failure", "records":[
            {"kind":"step", "step":"build", "outcome":"success", "seconds":10.0, "ended":999_999_999.0},
            {"kind":"step", "step":"test", "outcome":"failure", "seconds":20.0, "ended":1_000_000_120.0},
            {"kind":"step", "step":"lint", "outcome":"failure", "seconds":10.0, "ended":1_000_000_140.0}
        ]
    }).to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let parsed = samples(&text);
    let find = |metric: &str, step: &str| {
        parsed
            .iter()
            .find(|r| r.0 == metric && r.1["step"] == step)
            .unwrap()
    };
    assert_eq!(find("jojobot_build_first_failure_seconds", "test").2, 120.0);
    assert_eq!(find("jojobot_build_duration_seconds", "job").2, 100.0);
    assert_eq!(find("jojobot_build_duration_seconds", "workflow").2, 150.0);
    assert!(
        !parsed.iter().any(|r| r.1["step"] == "build"),
        "a prior attempt is excluded"
    );
}

#[test]
fn build_metrics_ci_counts_an_unmeasured_failure_without_inventing_a_time() {
    let mut child = support::fenced_spawn(
        Command::new("python3")
            .args([
                "-c",
                r#"
import importlib.machinery, importlib.util, json, sys
loader = importlib.machinery.SourceFileLoader('build_metrics', sys.argv[1])
spec = importlib.util.spec_from_loader(loader.name, loader)
m = importlib.util.module_from_spec(spec)
loader.exec_module(m)
context = json.load(sys.stdin)
empty = m.ci_payload({**context, 'records':[]})
gaps = [s for s in empty['samples'] if s['metric'] == 'unmeasured_failures']
assert len(gaps) == 1 and gaps[0]['seconds'] == 1
assert not any(s['metric'] == 'first_failure' for s in empty['samples'])
for outcome in ['success', 'cancelled', 'skipped', 'unknown']:
    control = m.ci_payload({**context, 'outcome':outcome})
    assert not any(s['metric'] in {'unmeasured_failures', 'first_failure'} for s in control['samples'])
measured = m.ci_payload({**context, 'records':[{**context['records'][1], 'outcome':'failure'}]})
assert any(s['metric'] == 'first_failure' for s in measured['samples'])
assert not any(s['metric'] == 'unmeasured_failures' for s in measured['samples'])
print(m.render(m.ci_payload(context)), end='')
"#,
            ])
            .arg(script())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .unwrap();
    child.stdin.take().unwrap().write_all(serde_json::json!({
        "workflow":"branch", "job":"report", "ref":"dev-sol3-slos-923-work", "runner":"lisa",
        "created":1_000_000_000.0, "started":1_000_000_050.0, "now":1_000_000_150.0,
        "outcome":"failure", "records":[
            {"kind":"step", "step":"test", "outcome":"failure", "seconds":10.0, "ended":999_999_999.0},
            {"kind":"step", "step":"build", "outcome":"success", "seconds":20.0, "ended":1_000_000_120.0}
        ]
    }).to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed = samples(&String::from_utf8(out.stdout).unwrap());
    let counts: Vec<_> = parsed
        .iter()
        .filter(|r| r.0 == "jojobot_build_unmeasured_failures")
        .collect();
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].1["step"], "unmeasured");
    assert_eq!(counts[0].1["outcome"], "failure");
    assert_eq!(counts[0].2, 1.0);
    assert!(
        !parsed
            .iter()
            .any(|r| r.0 == "jojobot_build_first_failure_seconds")
    );
}

#[test]
fn build_metrics_ci_entry_reads_the_runner_context_and_keeps_a_failed_push_non_gating() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("build-metrics-ci"));
    std::fs::create_dir_all(&dir).unwrap();
    support::write_executable(
        &dir.join("curl"),
        r#"#!/bin/sh
printf '%s\n' "$@" > "$METRICS_TEST_DIR/argv"
cat > "$METRICS_TEST_DIR/samples"
exit 7
"#,
    );
    let out = support::output_fenced(
        Command::new("python3")
            .args([
                "-c",
                r#"
import importlib.machinery, importlib.util, io, json, os, pathlib, sys
loader = importlib.machinery.SourceFileLoader('build_metrics', sys.argv[1])
spec = importlib.util.spec_from_loader(loader.name, loader)
m = importlib.util.module_from_spec(spec)
loader.exec_module(m)
root = pathlib.Path.cwd()
record = {'checkout':str(root), 'kind':'step', 'step':'test_compile',
          'outcome':'failure', 'seconds':20, 'ended':1_700_000_120}
(root / 'rows.jsonl').write_text(json.dumps(record) + '\n' +
    json.dumps({**record, 'checkout':str(root / 'scratch'), 'ended':1_700_000_100}) + '\n')
# These are the run, jobs and timestamped-log shapes read by scripts/wait-run.
api = 'http://forge.invalid/api/v1/repos/jojo/jojobot'
responses = {
    api + '/actions/runs/1481': json.dumps({'created':'2023-11-14T22:13:20Z',
        'prettyref':'dev-sol3-slos-923-work'}),
    api + '/actions/runs/1481/jobs': json.dumps([{'name':'report', 'id':13}]),
    api + '/actions/jobs/13/logs': '2023-11-14T22:14:10.0000000Z job accepted\n',
}
seen = []
def read(url, timeout):
    seen.append(url)
    return io.BytesIO(responses[url].encode())
m.urllib.request.urlopen = read
m.time.time = lambda: 1_700_000_150
m.socket.gethostname = lambda: 'lisa'
os.environ.update(GITHUB_API_URL='http://forge.invalid/api/v1', GITHUB_REPOSITORY='jojo/jojobot',
    GITHUB_RUN_ID='1481', JOJOBOT_METRICS_WORKFLOW='branch', JOJOBOT_METRICS_JOB='report',
    JOJOBOT_BUILD_METRICS_FILE='rows.jsonl', JOJOBOT_METRICS_OUTCOME='failure')
os.environ.pop('JOJOBOT_CI_METRICS_URL', None)
assert m.main(['ci']) == 0
assert not seen and not (root / 'argv').exists()
os.environ['JOJOBOT_CI_METRICS_URL'] = 'http://metrics.invalid/import'
assert m.main(['ci']) == 0
assert seen == list(responses)
assert (root / 'argv').read_text().endswith('--data-binary\n@-\nhttp://metrics.invalid/import\n')
"#,
            ])
            .arg(script())
            .current_dir(&dir)
            .env("METRICS_TEST_DIR", &dir)
            .env(
                "PATH",
                format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
            ),
    )
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed = samples(&std::fs::read_to_string(dir.join("samples")).unwrap());
    assert_eq!(parsed.len(), 4);
    for (_, labels, _) in &parsed {
        for (key, value) in [
            ("workflow", "branch"),
            ("job", "report"),
            ("branch_class", "dev"),
            ("runner_host", "lisa"),
            ("outcome", "failure"),
        ] {
            assert_eq!(labels[key], value);
        }
    }
    for (metric, step, value) in [
        ("jojobot_build_duration_seconds", "test_compile", 20.0),
        ("jojobot_build_duration_seconds", "job", 100.0),
        ("jojobot_build_duration_seconds", "workflow", 150.0),
        ("jojobot_build_first_failure_seconds", "test_compile", 120.0),
    ] {
        assert!(
            parsed
                .iter()
                .any(|r| r.0 == metric && r.1["step"] == step && r.2 == value)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_metrics_binary_allowlist_matches_the_workspace_test_binary_list() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = support::output_fenced(
        Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(root),
    )
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut binaries = std::collections::BTreeSet::new();
    for package in metadata["packages"].as_array().unwrap() {
        for target in package["targets"].as_array().unwrap() {
            if target["test"] != true {
                continue;
            }
            for kind in target["kind"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| v.as_str())
            {
                match kind {
                    "lib" | "proc-macro" => {
                        binaries.insert(format!("lib-{}", package["name"].as_str().unwrap()));
                    }
                    "bin" => {
                        binaries.insert("bins".into());
                    }
                    "test" => {
                        binaries.insert(format!("test-{}", target["name"].as_str().unwrap()));
                    }
                    _ => {}
                }
            }
        }
    }
    let generated = binaries.into_iter().collect::<Vec<_>>().join("\n") + "\n";
    assert_eq!(
        include_str!("../build-metrics-binaries.txt"),
        generated,
        "regenerate the visible allowlist from this cargo metadata test-binary list"
    );
}

#[test]
fn build_metrics_unlisted_binaries_are_counted_and_the_twenty_slowest_tests_are_anonymous() {
    let mut samples_in = vec![
        serde_json::json!({"metric":"test_binary", "binary":"test-build_metrics",
        "step":"test", "outcome":"success", "seconds":2.0}),
    ];
    samples_in.push(
        serde_json::json!({"metric":"test_binary", "binary":"private-new-binary",
        "step":"test", "outcome":"failure", "seconds":9.0}),
    );
    samples_in.push(
        serde_json::json!({"metric":"test_binary", "binary":"another-private-binary",
        "step":"test", "outcome":"failure", "seconds":8.0}),
    );
    for i in 0..21 {
        samples_in.push(
            serde_json::json!({"metric":"test", "step":format!("test-rank-{:02}", i+1),
            "test":format!("private_case_{i}"), "outcome":"success", "seconds":(21-i) as f64}),
        );
    }
    let text = render(
        &serde_json::json!({"workflow":"branch", "job":"report", "ref":"dev-alpha-work",
        "runner":"lisa", "samples":samples_in}),
    );
    assert!(
        !text.contains("private"),
        "unknown names and test names must not be emitted: {text}"
    );
    let parsed = samples(&text);
    assert_eq!(
        parsed
            .iter()
            .filter(|r| r.0 == "jojobot_build_test_binary_seconds")
            .count(),
        1
    );
    let unlisted = parsed
        .iter()
        .find(|r| r.0 == "jojobot_build_unlisted_binaries")
        .expect("unlisted count");
    assert_eq!(unlisted.2, 2.0);
    let ranked: Vec<_> = parsed
        .iter()
        .filter(|r| r.0 == "jojobot_build_test_seconds")
        .collect();
    assert_eq!(ranked.len(), 20);
    assert_eq!(ranked[0].1["step"], "test-rank-01");
    assert_eq!(ranked[0].2, 21.0);
    assert_eq!(ranked[19].1["step"], "test-rank-20");
    assert_eq!(ranked[19].2, 2.0);
}
