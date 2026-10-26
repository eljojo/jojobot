//! The cache is an optimization: a miss must record the last deployed tag.
use std::fs;
use std::path::Path;
use std::process::Command;

mod support;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = support::output_fenced(Command::new("git").args(args).current_dir(dir)).unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}

#[test]
fn upgrade_fixture_cache_miss_rebuilds_and_a_hit_reuses_the_tagged_store() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-upgrade-cache"));
    fs::create_dir_all(dir.join("scripts")).unwrap();
    support::git_init(&dir);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    support::write_executable(
        &dir.join("scripts/refresh-upgrade-fixture"),
        r#"#!/bin/sh
set -eu
echo "$1" >> recorded
mkdir -p "$2"
echo "select 'homer';" > "$2/doltdump.sql"
git rev-parse "$1^{commit}" > "$2/ref.txt"
echo holder > "$2/role_holder.txt"
"#,
    );
    git(&dir, &["add", "scripts/refresh-upgrade-fixture"]);
    git(&dir, &["commit", "-qm", "scratch"]);
    git(&dir, &["tag", "deploy-9.2.2"]);
    fs::write(dir.join("homer"), "next release").unwrap();
    git(&dir, &["add", "homer"]);
    git(&dir, &["commit", "-qm", "scratch"]);
    git(&dir, &["tag", "deploy-9.2.3"]);
    let sha = git(&dir, &["rev-parse", "HEAD"]);
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/prepare-upgrade-fixture");
    for expected in ["cache miss", "cache hit"] {
        let out = support::output_fenced(
            Command::new("python3")
                .arg(&script)
                .current_dir(&dir)
                .env("GITHUB_ACTIONS", "true")
                .env("XDG_CACHE_HOME", dir.join("cache"))
                .env("UPGRADE_FIXTURE_DIR", dir.join("output")),
        )
        .unwrap();
        assert!(out.status.success(), "{out:?}");
        let log = String::from_utf8(out.stdout).unwrap();
        assert!(
            log.contains(expected) && log.contains("deploy-9.2.3"),
            "{log}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("output/ref.txt"))
                .unwrap()
                .trim(),
            sha
        );
    }
    assert_eq!(
        fs::read_to_string(dir.join("recorded")).unwrap(),
        "deploy-9.2.3\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

fn a_cache_miss_wait_is_bounded(wait: &str) {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-upgrade-wait"));
    fs::create_dir_all(dir.join("scripts")).unwrap();
    support::git_init(&dir);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    support::write_executable(
        &dir.join("scripts/refresh-upgrade-fixture"),
        "#!/bin/sh\necho $$ > building_pid\nexec sleep 60\n",
    );
    git(&dir, &["add", "scripts/refresh-upgrade-fixture"]);
    git(&dir, &["commit", "-qm", "scratch"]);
    git(&dir, &["tag", "deploy-9.2.3"]);
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/prepare-upgrade-fixture");
    let out = support::output_fenced(
        Command::new("python3")
            .args(["-c", r#"
import fcntl, os, pathlib, signal, subprocess, sys
script, wait = sys.argv[1:]
def kill_build():
    pid = pathlib.Path('building_pid')
    if pid.exists():
        try:
            os.killpg(int(pid.read_text()), signal.SIGKILL)
        except ProcessLookupError:
            pass
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
cache = pathlib.Path(os.environ['XDG_CACHE_HOME']) / 'jojobot' / 'upgrade-fixtures'
cache.mkdir(parents=True)
runner = "import runpy,sys; g=runpy.run_path(sys.argv[1]); g['main'].__globals__.update(LOCK_TIMEOUT=0.1, BUILD_TIMEOUT=0.1); g['main']()"
with (cache / ('deploy-9.2.3-' + revision + '.lock')).open('w') as lock:
    if wait == 'lock':
        fcntl.flock(lock, fcntl.LOCK_EX)
    process = subprocess.Popen([sys.executable, '-c', runner, script],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, start_new_session=True)
    try:
        try:
            out, err = process.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            kill_build()
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate()
            raise AssertionError(wait + ' wait remained unbounded on a cache miss')
        assert process.returncode != 0, out + err
        assert wait + ' wait exceeded' in out + err, out + err
        if wait == 'build':
            assert 'cache miss' in out, out
        print(out + err)
    finally:
        kill_build()
"#])
            .arg(script)
            .arg(wait)
            .current_dir(&dir)
            .env("GITHUB_ACTIONS", "true")
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("UPGRADE_FIXTURE_DIR", dir.join("output")),
    )
    .unwrap();
    fs::remove_dir_all(dir).unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn upgrade_fixture_cache_lock_wait_is_bounded_on_a_miss() {
    a_cache_miss_wait_is_bounded("lock");
}

#[test]
fn upgrade_fixture_cache_build_wait_is_bounded_on_a_miss() {
    a_cache_miss_wait_is_bounded("build");
}

fn script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/prepare-upgrade-fixture")
}

/// Run `code` with the script's path as its argument.
fn python(code: &str) -> String {
    let out =
        support::output_fenced(Command::new("python3").args(["-c", code]).arg(script())).unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn upgrade_fixture_lock_wait_outlasts_the_build_it_waits_for() {
    // The first job holds the lock across its whole build, so a waiter that gives
    // up sooner turns red on infrastructure while the recording is still running.
    let said = python(
        "import runpy,sys; g=runpy.run_path(sys.argv[1]); \
         print(g['LOCK_TIMEOUT'], g['BUILD_TIMEOUT'])",
    );
    let numbers: Vec<f64> = said
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert!(
        numbers[0] > numbers[1],
        "lock wait {} must exceed the build wait {}",
        numbers[0],
        numbers[1]
    );
}

const RECORDER_INPUTS: [&str; 3] = [
    "scripts/refresh-upgrade-fixture",
    "crates/jojobot/tests/record_upgrade_fixture.rs",
    "crates/jojobot-exercise/src/surface.rs",
];

/// A scratch repository tagged `deploy-9.2.3` whose recorder logs each recording.
fn recording_repository() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-upgrade-key"));
    for input in RECORDER_INPUTS {
        fs::create_dir_all(dir.join(input).parent().unwrap()).unwrap();
    }
    support::git_init(&dir);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    support::write_executable(
        &dir.join(RECORDER_INPUTS[0]),
        r#"#!/bin/sh
set -eu
echo "$1" >> recorded
mkdir -p "$2"
echo "select 'marge';" > "$2/doltdump.sql"
git rev-parse "$1^{commit}" > "$2/ref.txt"
echo holder > "$2/role_holder.txt"
"#,
    );
    for input in &RECORDER_INPUTS[1..] {
        fs::write(dir.join(input), "// the recorder\n").unwrap();
    }
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "scratch"]);
    git(&dir, &["tag", "deploy-9.2.3"]);
    dir
}

fn prepared(dir: &Path) -> String {
    let out = support::output_fenced(
        Command::new("python3")
            .arg(script())
            .current_dir(dir)
            .env("GITHUB_ACTIONS", "true")
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("UPGRADE_FIXTURE_DIR", dir.join("output")),
    )
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn upgrade_fixture_cache_key_follows_everything_the_recording_reads() {
    for input in RECORDER_INPUTS {
        let dir = recording_repository();
        assert!(prepared(&dir).contains("cache miss"), "{input}");
        assert!(prepared(&dir).contains("cache hit"), "{input}");
        let mut text = fs::read_to_string(dir.join(input)).unwrap();
        text.push_str("\n# changed\n");
        fs::write(dir.join(input), text).unwrap();
        let log = prepared(&dir);
        assert!(
            log.contains("cache miss"),
            "a change to {input} reused the old store: {log}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("recorded"))
                .unwrap()
                .lines()
                .count(),
            2,
            "{input}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn upgrade_fixture_branch_run_watches_for_a_red_sibling_while_it_records() {
    // A recording lasts up to twenty minutes. A rooms job that goes red during it
    // frees nothing unless the recording step is wrapped by the sibling watcher,
    // which needs the job's token to cancel the run.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.forgejo/workflows/branch.yml");
    let yaml = fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = yaml.lines().collect();
    let at = lines
        .iter()
        .position(|l| {
            !l.trim_start().starts_with('#') && l.contains("scripts/prepare-upgrade-fixture")
        })
        .expect("branch.yml records the fixture");
    assert!(
        lines[at].contains("scripts/stop-with-siblings "),
        "the fixture step is not watched: {}",
        lines[at]
    );
    let step: Vec<&str> = lines[at..]
        .iter()
        .take_while(|l| !l.trim_start().starts_with("- "))
        .copied()
        .collect();
    assert!(
        step.iter().any(|l| l.trim_start().starts_with("TOKEN:")),
        "the watcher cannot cancel without the job's token: {step:?}"
    );
}

#[test]
fn upgrade_fixture_complete_refuses_every_damaged_entry() {
    let said = python(
        r#"
import runpy, sys, hashlib, pathlib, tempfile
g = runpy.run_path(sys.argv[1])
complete = g['complete']
tag, rev = 'deploy-9.2.3', 'a' * 40
base = pathlib.Path(tempfile.mkdtemp())
def build(root, dump=b'select 1;'):
    root.mkdir()
    (root / 'doltdump.sql').write_bytes(dump)
    (root / 'tag.txt').write_text(tag + '\n')
    (root / 'ref.txt').write_text(rev + '\n')
    (root / 'role_holder.txt').write_text('holder\n')
    (root / 'dump.sha256').write_text(hashlib.sha256(dump).hexdigest() + '\n')
def write(name, text):
    return lambda d: (d / name).write_text(text)
def remove(name):
    return lambda d: (d / name).unlink()
def empty_dump(d):
    (d / 'doltdump.sql').write_bytes(b'')
    (d / 'dump.sha256').write_text(hashlib.sha256(b'').hexdigest() + '\n')
cases = {
    'whole': (lambda d: None, True),
    'wrong dump.sha256': (write('dump.sha256', hashlib.sha256(b'other').hexdigest() + '\n'), False),
    'edited dump': (write('doltdump.sql', 'select 2;'), False),
    'missing tag.txt': (remove('tag.txt'), False),
    'wrong tag.txt': (write('tag.txt', 'deploy-9.2.2\n'), False),
    'missing ref.txt': (remove('ref.txt'), False),
    'wrong ref.txt': (write('ref.txt', 'b' * 40 + '\n'), False),
    'empty role_holder.txt': (write('role_holder.txt', '\n'), False),
    'missing role_holder.txt': (remove('role_holder.txt'), False),
    'missing dump': (remove('doltdump.sql'), False),
    'empty dump': (empty_dump, False),
}
bad = []
for name, (damage, expected) in cases.items():
    entry = base / name.replace(' ', '_')
    build(entry)
    damage(entry)
    if complete(entry, tag, rev) != expected:
        bad.append(name)
print('wrong answers:', bad)
sys.exit(1 if bad else 0)
"#,
    );
    assert!(said.contains("wrong answers: []"), "{said}");
}
