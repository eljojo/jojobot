//! The cache is an optimization: a miss must record the last deployed tag. The memory
//! job's script, at the end, runs one case of the check lane's boot_measures binary.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

#[test]
fn boot_store_cache_key_follows_recording_inputs() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/prepare-boot-store");
    let out = support::output_fenced(
        Command::new("python3")
            .args([
                "-c",
                r#"
import os, pathlib, runpy, shutil, sys, tempfile
script = runpy.run_path(sys.argv[1])
digest = script['grower_digest']
# Keep the fixture commit fixed: these are inputs of the current recorder.
inputs = [
    'crates/jojobot-adapters/migrations/0124_field_write_live_clear.sql',
    'crates/jojobot-exercise/src/surface.rs',
    'crates/jojobot-exercise/src/room/seed.rs',
    'crates/jojobot-adapters/src/dolt/migrate.rs',
    'crates/jojobot-mcp/release-notes/deploy-9.3.3/alpha.md',
    'crates/jojobot/src/main.rs',
    'crates/jojobot-adapters/build.rs',
    'crates/jojobot-exercise/Cargo.toml',
    'crates/jojobot-mcp/served-texts/tools.md',
    'crates/jojobot-domain/data/nicknames.txt',
    'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'flake.nix', 'flake.lock',
    'scripts/prepare-boot-store',
    'scripts/refresh-upgrade-fixture',
    'crates/jojobot/tests/boot_measures.rs',
]
with tempfile.TemporaryDirectory() as directory:
    os.chdir(directory)
    # Every pattern finds one file, so a tree is complete when none is refused.
    for pattern in script['GROWER_INPUTS']:
        inputs.append(pattern.replace('**/*', 'sub/file.txt').replace('*', 'x'))
    for name in inputs:
        path = pathlib.Path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('original\n')
    baseline = digest()
    # Crates the recorder does not link are not inputs: they change on most slices.
    for name in ('crates/jojobot-bar/src/lib.rs', 'crates/jojobot-roster/src/lib.rs'):
        path = pathlib.Path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('unlinked\n')
        assert digest() == baseline, f'an unlinked crate invalidated the cache: {name}'
        path.write_text('edited unlinked\n')
        assert digest() == baseline, f'an unlinked crate edit invalidated the cache: {name}'
    unused = pathlib.Path('pm/feature-briefs/alpha.md')
    unused.parent.mkdir(parents=True)
    unused.write_text('unrelated document\n')
    assert digest() == baseline, 'an unused document invalidated the cache'
    unused.write_text('edited unrelated document\n')
    assert digest() == baseline, 'an unused document edit invalidated the cache'
    for name in inputs:
        path = pathlib.Path(name)
        path.write_text('changed\n')
        assert digest() != baseline, f'cache key ignored {name}'
        path.write_text('original\n')
        assert digest() == baseline, f'cache key is unstable after restoring {name}'
    added = pathlib.Path('crates/jojobot-adapters/migrations/9999_alpha.sql')
    added.write_text('new migration\n')
    assert digest() != baseline, 'cache key ignored an added migration'
    added.unlink()
    removed = pathlib.Path(inputs[0])
    removed.unlink()
    assert digest() != baseline, 'cache key ignored a removed migration'
    # A pattern that finds nothing is refused: the directory it names was renamed.
    shutil.rmtree('crates/jojobot-mcp/release-notes')
    try:
        digest()
    except SystemExit as refused:
        assert 'release-notes' in str(refused), refused
    else:
        raise AssertionError('a pattern that matches nothing was accepted')
print('recording input edits, migration additions/removals, and unused document control checked')
"#,
            ])
            .arg(script),
    )
    .unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The cases above run on a made-up tree, which cannot say that a pattern still
/// names a path of THIS checkout, or that a crate added later was weighed.
#[test]
#[cfg(not(jojobot_sandbox))]
fn boot_store_cache_key_patterns_all_match_the_checkout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = support::output_fenced(
        Command::new("python3")
            .args([
                "-c",
                r#"
import pathlib, runpy, sys
script = runpy.run_path(sys.argv[1])
script['grower_digest']()
named = {pattern.split('/')[1] for pattern in script['GROWER_INPUTS'] if pattern.startswith('crates/')}
unlinked = set(script['UNLINKED_CRATES'])
assert not named & unlinked, f'a crate is both an input and unlinked: {named & unlinked}'
unweighed = {p.name for p in pathlib.Path('crates').iterdir() if p.is_dir()} - named - unlinked
assert not unweighed, f'a crate is neither an input nor named unlinked: {unweighed}'
print('every pattern matches, every crate is weighed')
"#,
            ])
            .arg(root.join("scripts/prepare-boot-store"))
            .current_dir(&root),
    )
    .unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn boot_store_recording_uses_the_deployed_fixture_binary_lifetime() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-boot-recorder"));
    for folder in ["scripts", "bin", "fixture", "output"] {
        fs::create_dir_all(dir.join(folder)).unwrap();
    }
    fs::write(dir.join("fixture/ref.txt"), "a".repeat(40)).unwrap();
    support::write_executable(&dir.join("bin/cargo"), "#!/bin/sh\nexit 91\n");
    support::write_executable(
        &dir.join("scripts/refresh-upgrade-fixture"),
        r#"#!/bin/sh
set -eu
test "$1" = "$(cat "$UPGRADE_FIXTURE_DIR/ref.txt")"
test "$2" = "$BOOT_STORE_DIR"
echo deployed > "$BOOT_STORE_DIR/recorded"
"#,
    );
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/prepare-boot-store");
    let out = support::output_fenced(
        Command::new("python3")
            .args(["-c", "import pathlib,runpy,sys; runpy.run_path(sys.argv[1])['record'](pathlib.Path(sys.argv[2]))"])
            .arg(script)
            .arg(dir.join("output"))
            .current_dir(&dir)
            .env("UPGRADE_FIXTURE_DIR", dir.join("fixture"))
            .env("PATH", format!("{}:{}", dir.join("bin").display(), std::env::var("PATH").unwrap())),
    ).unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        fs::read_to_string(dir.join("output/recorded")).unwrap(),
        "deployed\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

// **The memory job's script** (`scripts/memory-day`): it runs the two ignored cases of the check
// lane's boot_measures binary where the bar runs it, and reads a run that ran fewer, or ran past its
// deadline, as red.

/// A checkout holding a bundle of one binary in the shape the bar writes: the check
/// lane's `tests/boot_measures.rs` under `cargo-target/debug/deps`, in the package
/// directory `crates/jojobot`. The binary is a script that writes down what it was
/// given and prints `body`.
fn bundle(body: &str) -> PathBuf {
    bundle_with(body, "1237.4", "1625.9")
}

/// The same, with the two numbers the report writes in MiB: the server's peak through
/// the first boot and its resident memory at the end of the day, which is also the
/// most it held in the day. The lines are the shapes a run of the case wrote.
fn bundle_with(body: &str, boot_peak: &str, day_resident: &str) -> PathBuf {
    bundle_report(
        body,
        &[boot_peak],
        "900.0",
        "1000.0",
        day_resident,
        day_resident,
        day_resident,
    )
}

/// The full report: one `memory-report boot:` line for each first boot the job made (the
/// first-boot case's, then the day case's own), one `memory-report second boot:` line
/// holding `second_peak` (none when it is empty), one `memory-report no-snapshot boot:`
/// line holding `no_snapshot_peak` (none when it is empty), and a day that held
/// `day_highest` at the most and settled at `day_resident`.
fn bundle_report(
    body: &str,
    boot_peaks: &[&str],
    second_peak: &str,
    no_snapshot_peak: &str,
    day_resident: &str,
    day_highest: &str,
    day_peak: &str,
) -> PathBuf {
    let boot_lines: String = boot_peaks
        .iter()
        .map(|peak| {
            format!(
                "echo 'memory-report boot: the store needed the claims import at this boot; \
                 server peak {peak} MiB (1267052 kB), store peak 1343.0 MiB (1375244 kB), server \
                 resident after boot 1146.2 MiB, of which anonymous 1104.5 MiB' >> \
                 \"$ROOT/target/bar/boot-measures.log\"\n"
            )
        })
        .collect();
    let second_line = if second_peak.is_empty() {
        String::new()
    } else {
        format!(
            "echo 'memory-report second boot: the store held the claims import and a snapshot; \
             server peak {second_peak} MiB (921600 kB), store peak 1243.0 MiB (1272832 kB), \
             server resident after boot 846.2 MiB, of which anonymous 804.5 MiB' >> \
             \"$ROOT/target/bar/boot-measures.log\"\n"
        )
    };
    let no_snapshot_line = if no_snapshot_peak.is_empty() {
        String::new()
    } else {
        format!(
            "echo 'memory-report no-snapshot boot: the store held the claims import and no \
             snapshot; server peak {no_snapshot_peak} MiB (1024000 kB), store peak 1343.0 MiB \
             (1375244 kB), server resident after boot 946.2 MiB, of which anonymous 904.5 MiB' \
             >> \"$ROOT/target/bar/boot-measures.log\"\n"
        )
    };
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-memory-day"));
    let deps = dir.join("cargo-target/debug/deps");
    fs::create_dir_all(&deps).unwrap();
    fs::create_dir_all(dir.join("crates/jojobot")).unwrap();
    fs::create_dir_all(dir.join("target/bundle")).unwrap();
    fs::create_dir_all(dir.join("target/bar")).unwrap();
    support::write_executable(
        &deps.join("boot_measures-0123456789abcdef"),
        &format!(
            "#!/bin/sh\n\
             echo \"$@\" > \"{root}/seen-args\"\n\
             echo \"$PWD\" > \"{root}/seen-dir\"\n\
             echo \"$CARGO_MANIFEST_DIR $JOJOBOT_BIN\" > \"{root}/seen-env\"\n\
             ROOT=\"{root}\"\n\
             {boot_lines}\
             {second_line}\
             {no_snapshot_line}\
             echo 'memory-report day: all 5300 entries of a day made, which is 6179 tool calls \
             before any retry, in 1682 s; server resident {day_resident} MiB at the end, \
             {day_highest} MiB at the most; peak {day_peak} MiB' >> \
             \"{root}/target/bar/boot-measures.log\"\n\
             echo 'memory-report wall: 7 s from the start of the case to its end' >> \
             \"{root}/target/bar/boot-measures.log\"\n\
             {body}\n",
            root = dir.display()
        ),
    );
    // The `describes` of another binary and of another lane must not be picked.
    let exe = |lane: &str, describes: &str, path: &str| {
        serde_json::json!({
            "lane": lane, "describes": describes, "path": path, "dir": "crates/jojobot",
        })
    };
    let manifest = serde_json::json!({
        "root": dir,
        "arch": "x86_64",
        "exes": [
            exe("check", "tests/ci.rs", "cargo-target/debug/deps/ci-fedcba9876543210"),
            exe("check", "tests/boot_measures.rs",
                "cargo-target/debug/deps/boot_measures-0123456789abcdef"),
            exe("year", "tests/boot_measures.rs", "cargo-target/debug/deps/elsewhere-1"),
        ],
    });
    fs::write(
        dir.join("target/bundle/manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    dir
}

fn run(dir: &Path, deadline: &str) -> Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/memory-day");
    support::output_fenced(
        Command::new("python3")
            .arg(script)
            .current_dir(dir)
            .env("BAR_BUNDLE", "target/bundle/manifest.json")
            // The bar sets it for the binaries it runs, and this case runs inside one.
            .env_remove("JOJOBOT_BIN")
            .env("MEMORY_DAY_DEADLINE_SECONDS", deadline),
    )
    .unwrap()
}

/// **The job prints `target/bar/memory-day.log` as its last line**, so the file has
/// to hold the one line the script ended on, ok, FAILED or TIMED OUT.
fn the_verdict_file_is_the_last_line(dir: &Path, said: &str) {
    let file = fs::read_to_string(dir.join("target/bar/memory-day.log")).unwrap();
    assert_eq!(file.trim_end(), said.lines().last().unwrap(), "{said}");
}

#[test]
fn memory_day_runs_the_checks_boot_measures_binary_where_the_bar_runs_it() {
    let dir =
        bundle("echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out'");
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seen = |name: &str| {
        fs::read_to_string(dir.join(name))
            .unwrap()
            .trim()
            .to_string()
    };
    assert_eq!(
        seen("seen-args"),
        "--ignored --exact memory_report::the_servers_memory_after_a_replayed_day \
         --nocapture --test-threads=1"
    );
    let package = dir.join("crates/jojobot");
    assert_eq!(seen("seen-dir"), package.display().to_string());
    assert_eq!(
        seen("seen-env"),
        format!(
            "{} {}",
            package.display(),
            dir.join("cargo-target/debug/jojobot").display()
        ),
        "the manifest directory and the server beside the binary"
    );
    assert!(
        said.contains("memory-report wall: 7 s"),
        "the numbers reach the job log: {said}"
    );
    assert!(said.contains("memory-day: ok after "), "{said}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

const PASSED: &str =
    "echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out'";

/// **The verdict states both of rule 800's numbers against its 4 GiB, and reads OVER
/// when either reaches it.** The first-boot peak and the resident memory after the
/// day are in MiB in the report, and 4 GiB is 4,096 MiB. `ok` means both are under.
/// The two numbers are in the line, so a reader of the job's last line needs nothing else.
#[test]
fn memory_day_verdict_states_both_numbers_and_reads_ok_when_both_are_under_4_gib() {
    let dir = bundle_with(PASSED, "4095.9", "1625.9");
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(out.status.success(), "{said}");
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    for number in ["4095.9", "1625.9", "4096"] {
        assert!(
            verdict.contains(number),
            "the verdict lacks {number}: {verdict}"
        );
    }
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_day_verdict_reads_over_when_the_day_reaches_4_gib() {
    for (boot, day) in [("1237.4", "5120.0"), ("1237.4", "4096.0")] {
        let dir = bundle_with(PASSED, boot, day);
        let out = run(&dir, "60");
        let said = String::from_utf8_lossy(&out.stdout);
        let verdict = said.lines().last().unwrap();
        assert!(
            verdict.starts_with("memory-day: OVER"),
            "boot {boot}, day {day}: {verdict}"
        );
        assert_eq!(out.status.code(), Some(3), "{said}");
        assert!(
            verdict.contains(boot) && verdict.contains(day),
            "both numbers are named: {verdict}"
        );
        the_verdict_file_is_the_last_line(&dir, &said);
        fs::remove_dir_all(dir).unwrap();
    }
}

/// **Production is killed at its peak, not where it settles**, so the verdict also names
/// the most the day held and reads OVER when that passed 4 GiB, even if the day settled
/// under.
#[test]
fn memory_day_verdict_reads_over_for_a_day_that_peaked_over_and_settled_under() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "900.0",
        "1000.0",
        "1625.9",
        "4300.0",
        "4300.0",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: OVER"), "{verdict}");
    assert_eq!(out.status.code(), Some(3), "{said}");
    for number in ["1237.4", "1625.9", "4300.0"] {
        assert!(
            verdict.contains(number),
            "the verdict lacks {number}: {verdict}"
        );
    }
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// The process's own peak is the kernel's high-water mark and can pass the highest of the
/// samples, which are half a second apart; it counts as the most the day held too.
#[test]
fn memory_day_verdict_counts_the_process_peak_as_the_most_the_day_held() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "900.0",
        "1000.0",
        "1625.9",
        "3000.0",
        "4200.0",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: OVER"), "{verdict}");
    assert!(verdict.contains("4200.0"), "{verdict}");
    fs::remove_dir_all(dir).unwrap();
}

/// An `ok` names the most the day held beside where it settled.
#[test]
fn memory_day_verdict_names_the_most_the_day_held_when_it_reads_ok() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "900.0",
        "1000.0",
        "1625.9",
        "2100.5",
        "2100.5",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    assert!(verdict.contains("2100.5"), "{verdict}");
    fs::remove_dir_all(dir).unwrap();
}

/// **The job makes two import boots, and the verdict names the larger.** The import
/// boot is the calibration pair printed beside production's reading; it prepares the
/// store and no longer gates, so even a larger peak over 4 GiB reads ok when every boot
/// production can pay is under.
#[test]
fn memory_day_verdict_names_the_larger_of_the_two_import_boot_peaks_and_does_not_gate_on_it() {
    for peaks in [["4500.0", "1237.4"], ["1237.4", "4500.0"]] {
        let dir = bundle_report(
            PASSED, &peaks, "900.0", "1000.0", "1625.9", "1625.9", "1625.9",
        );
        let out = run(&dir, "60");
        let said = String::from_utf8_lossy(&out.stdout);
        let verdict = said.lines().last().unwrap();
        assert!(
            verdict.starts_with("memory-day: ok"),
            "{peaks:?}: {verdict}"
        );
        assert!(verdict.contains("4500.0"), "{peaks:?}: {verdict}");
        assert_eq!(out.status.code(), Some(0), "{said}");
        fs::remove_dir_all(dir).unwrap();
    }
}

/// **The import boot is printed beside production's reading, as fitted.** Production's
/// first boot at 9.3.3 held 3.47 GiB (3,553 MiB): the Rust process's resident memory at
/// the first sample after serving began, one instant. The share is therefore taken from
/// this run's resident memory after the import boot, not from its peak, which is the
/// most it held at any moment; the verdict names both and says the comparison is fitted
/// while production's byte totals are unread.
#[test]
fn memory_day_verdict_prints_the_import_boot_beside_production_as_fitted() {
    let dir = bundle_with(PASSED, "1776.5", "1625.9");
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    // The bundle's import boot line holds a resident memory after boot of 1146.2 MiB,
    // 32 percent of production's 3,553, and a peak of 1776.5 MiB, which would be 50
    // percent and is not the share.
    for word in ["1776.5", "1146.2", "3553", "32%", "fitted"] {
        assert!(
            verdict.contains(word),
            "the verdict lacks {word}: {verdict}"
        );
    }
    assert!(
        !verdict.contains("50%"),
        "the share is taken from the peak: {verdict}"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// **The boot with no usable snapshot is held to the bound.** Production's own deploy
/// of the next release is one: the import is done and there is no snapshot to load. A
/// no-snapshot peak that reaches 4 GiB reads OVER however low the import boot, the
/// second boot and the day stayed.
#[test]
fn memory_day_verdict_reads_over_when_the_no_snapshot_boot_reaches_4_gib() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "900.0",
        "4300.0",
        "1625.9",
        "1625.9",
        "1625.9",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: OVER"), "{verdict}");
    assert_eq!(out.status.code(), Some(3), "{said}");
    assert!(verdict.contains("4300.0"), "{verdict}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// A report with no no-snapshot line is a failed run, not an ok one.
#[test]
fn memory_day_verdict_fails_a_report_with_no_no_snapshot_boot() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "900.0",
        "",
        "1625.9",
        "1625.9",
        "1625.9",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: FAILED"), "{verdict}");
    assert!(verdict.contains("no-snapshot"), "{verdict}");
    assert_ne!(out.status.code(), Some(0), "{said}");
    fs::remove_dir_all(dir).unwrap();
}

/// **The boot production pays at its next release is held to the bound too.** The
/// import is done by then and a snapshot is loaded, so that boot is the second one
/// the job makes. A second-boot peak that reaches 4 GiB reads OVER however low the
/// first boot and the day stayed, and the verdict names it.
#[test]
fn memory_day_verdict_reads_over_when_the_second_boot_reaches_4_gib() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "4400.0",
        "1000.0",
        "1625.9",
        "1625.9",
        "1625.9",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: OVER"), "{verdict}");
    assert_eq!(out.status.code(), Some(3), "{said}");
    assert!(verdict.contains("4400.0"), "{verdict}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// An `ok` names the second boot's peak beside the first's.
#[test]
fn memory_day_verdict_names_the_second_boot_when_it_reads_ok() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "950.5",
        "1000.0",
        "1625.9",
        "1625.9",
        "1625.9",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    assert!(verdict.contains("950.5"), "{verdict}");
    fs::remove_dir_all(dir).unwrap();
}

/// **A report with no second-boot line is a failed run**, not an ok one: the job
/// would otherwise pass without measuring the boot production pays.
#[test]
fn memory_day_verdict_fails_a_report_with_no_second_boot() {
    let dir = bundle_report(
        PASSED,
        &["1237.4"],
        "",
        "1000.0",
        "1625.9",
        "1625.9",
        "1625.9",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: FAILED"), "{verdict}");
    assert!(verdict.contains("second-boot"), "{verdict}");
    assert_ne!(out.status.code(), Some(0), "{said}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// A report written by the case itself, to the file the script reads, with the import boot's
/// resident memory after boot (`None` leaves it out) and fixed numbers for the rest.
fn bundle_with_boot_resident(resident: Option<&str>) -> PathBuf {
    let after = resident.map_or(String::new(), |mib| {
        format!(", server resident after boot {mib} MiB")
    });
    let boot = format!("memory-report boot: server peak 3000.0 MiB{after}");
    let second = "memory-report second boot: server peak 900.0 MiB";
    let nosnap = "memory-report no-snapshot boot: server peak 950.0 MiB";
    let day = "memory-report day: server resident 1500.0 MiB at the end, 1500.0 MiB at the most; \
               peak 1500.0 MiB";
    bundle(&format!(
        "{PASSED}; printf '%s\\n%s\\n%s\\n%s\\n' '{boot}' '{second}' '{nosnap}' '{day}' > \
         ../../target/bar/boot-measures.log"
    ))
}

/// **An ok on a store far smaller than production's cannot be read as the proof.** The
/// bound is on boots of a store the size CI holds; when the import boot is under 75
/// percent of production's first-boot reading, the verdict leads with that share and says
/// it does not show that production fits in 4 GiB.
#[test]
fn memory_day_verdict_leads_with_the_share_and_says_a_small_store_proves_nothing() {
    let dir = bundle_with_boot_resident(Some("1300.0"));
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(
        verdict.starts_with("memory-day: ok after "),
        "an ok stays an ok: {verdict}"
    );
    assert!(
        verdict.contains("but the store is at 37% of production's first-boot reading"),
        "{verdict}"
    );
    assert!(
        verdict.contains("does not show that production fits in 4 GiB"),
        "{verdict}"
    );
    assert_eq!(out.status.code(), Some(0), "{said}");
    fs::remove_dir_all(dir).unwrap();
}

/// A store at or over 75 percent of production's reading is a store the bound can speak
/// for: the verdict does not warn.
#[test]
fn memory_day_verdict_does_not_warn_for_a_store_near_production_size() {
    let dir = bundle_with_boot_resident(Some("2800.0"));
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok after "), "{verdict}");
    assert!(!verdict.contains("does not show"), "{verdict}");
    assert!(verdict.contains("79%"), "{verdict}");
    fs::remove_dir_all(dir).unwrap();
}

/// A boot line with no resident memory leaves the share unread, and the verdict says so
/// where a reader looks, not by leaving the words out.
#[test]
fn memory_day_verdict_says_when_the_import_boots_share_was_not_read() {
    let dir = bundle_with_boot_resident(None);
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok after "), "{verdict}");
    assert!(verdict.contains("import boot share: not read"), "{verdict}");
    fs::remove_dir_all(dir).unwrap();
}

/// **The deadline leaves room for the case on a loaded runner and stays under the
/// job's own timeout.** The day took 1,756 to 2,000 s on a free runner and about 3,450 s
/// on a loaded one, with the boots before it under 300 s; the memory job is stopped at 100
/// minutes. Nothing outside the script declares its default, so the value is pinned here.
#[test]
fn memory_day_default_deadline_is_4800_seconds_under_the_jobs_hundred_minutes() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/memory-day");
    let out = support::output_fenced(
        Command::new("python3")
            .args([
                "-I",
                "-c",
                "import runpy, sys; print(runpy.run_path(sys.argv[1])['DEADLINE'])",
            ])
            .arg(script)
            .env_remove("MEMORY_DAY_DEADLINE_SECONDS"),
    )
    .unwrap();
    let deadline: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    assert_eq!(deadline, 4800);
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.forgejo/workflows/pull-request.yml"),
    )
    .unwrap();
    let job = workflow
        .split("\n  memory:\n")
        .nth(1)
        .expect("the memory job");
    let minutes: u64 = job
        .lines()
        .find_map(|line| line.trim().strip_prefix("timeout-minutes: "))
        .expect("the memory job has a timeout")
        .parse()
        .unwrap();
    assert!(
        deadline < minutes * 60,
        "the deadline passes the job's timeout"
    );
}

/// What an earlier invocation left in the append-only report: a boot and a day that
/// passed 4 GiB, in the shapes a run wrote.
const STALE_REPORT: &str = "memory-report boot: the store needed the claims import at this boot; \
server peak 4500.0 MiB (4608000 kB), store peak 1343.0 MiB (1375244 kB), server resident \
after boot 1146.2 MiB, of which anonymous 1104.5 MiB\n\
memory-report day: all 5300 entries of a day made, which is 6179 tool calls before any \
retry, in 1682 s; server resident 5000.0 MiB at the end, 5000.0 MiB at the most; peak 5000.0 MiB\n";

fn seed_the_report(dir: &Path) {
    fs::write(dir.join("target/bar/boot-measures.log"), STALE_REPORT).unwrap();
}

/// **A verdict is about the invocation that made it.** The report is appended to by
/// every run in the checkout, so an earlier run's 4,500 MiB boot must not make a later
/// run that stays under 4 GiB read OVER, nor be printed in this run's lines. The old
/// lines stay in the file: the history is the report's, the verdict is the run's.
#[test]
fn memory_day_verdict_ignores_what_an_earlier_invocation_wrote() {
    let dir = bundle_with(PASSED, "1237.4", "1625.9");
    seed_the_report(&dir);
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert!(
        !said.contains("4500.0") && !said.contains("5000.0"),
        "an earlier run's numbers reached this run's output: {said}"
    );
    let kept = fs::read_to_string(dir.join("target/bar/boot-measures.log")).unwrap();
    assert!(
        kept.starts_with(STALE_REPORT),
        "the earlier lines were not kept"
    );
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// **A report replaced by a shorter file is all this run's.** The script reads from the
/// size the file had when it began, and a file with fewer bytes than that has been
/// replaced, so a slice from that size would read nothing. The run writes a boot and a
/// day of its own in less than the earlier report held.
#[test]
fn memory_day_verdict_reads_a_report_replaced_by_a_shorter_file_whole() {
    let boot = "memory-report boot: server peak 1200.0 MiB, server resident after boot 1100.0 MiB";
    let second = "memory-report second boot: server peak 900.0 MiB";
    let nosnap = "memory-report no-snapshot boot: server peak 950.0 MiB";
    let day = "memory-report day: server resident 1500.0 MiB at the end, 1500.0 MiB at the most; \
               peak 1500.0 MiB";
    assert!(
        boot.len() + second.len() + nosnap.len() + day.len() < STALE_REPORT.len(),
        "the file must be shorter"
    );
    let dir = bundle(&format!(
        "{PASSED}; printf '%s\\n%s\\n%s\\n%s\\n' '{boot}' '{second}' '{nosnap}' '{day}' > \
         ../../target/bar/boot-measures.log"
    ));
    seed_the_report(&dir);
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    let verdict = said.lines().last().unwrap();
    assert!(verdict.starts_with("memory-day: ok"), "{verdict}");
    assert_eq!(out.status.code(), Some(0), "{said}");
    for number in ["1200.0", "1500.0"] {
        assert!(
            verdict.contains(number),
            "the verdict lacks {number}: {verdict}"
        );
    }
    assert!(
        !said.contains("4500.0"),
        "an earlier run's number reached this run: {said}"
    );
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// **An old line does not fill a number this run did not measure.** The run writes a
/// boot and no day, after an earlier run wrote both.
#[test]
fn memory_day_verdict_is_red_when_this_run_lacks_a_number_an_old_run_had() {
    let dir = bundle(&format!(
        "{PASSED}; head -n -2 ../../target/bar/boot-measures.log > ../../target/bar/t && \
         mv ../../target/bar/t ../../target/bar/boot-measures.log"
    ));
    seed_the_report(&dir);
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.lines()
            .last()
            .unwrap()
            .starts_with("memory-day: FAILED"),
        "{said}"
    );
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// A report that lost its lines is not a day under 4 GiB.
#[test]
fn memory_day_verdict_is_red_when_the_report_has_no_numbers() {
    // The fake binary writes its report lines first, so the body empties the file after.
    let dir = bundle(&format!("{PASSED}; : > ../../target/bar/boot-measures.log"));
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.lines()
            .last()
            .unwrap()
            .starts_with("memory-day: FAILED"),
        "{said}"
    );
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_day_is_red_when_the_case_fails() {
    let dir = bundle("echo 'test result: FAILED. 0 passed; 1 failed'; exit 101");
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(101), "{said}");
    assert!(said.contains("memory-day: FAILED with exit 101"), "{said}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// A name no test carries selects nothing, and the binary exits zero.
#[test]
fn memory_day_is_red_when_no_test_ran() {
    let dir = bundle(
        "echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out'",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("memory-day: FAILED"), "{said}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// A name no test carries selects none: the binary exits zero and counts no pass, and
/// the run is still red.
#[test]
fn memory_day_is_red_when_the_case_did_not_run() {
    let dir = bundle(
        "echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out'",
    );
    let out = run(&dir, "60");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("memory-day: FAILED"), "{said}");
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// Whether the process is still there to run.
fn is_running(pid: &str) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // The state is the first field after the command name, which ends at the last `)`.
    !stat
        .rsplit(')')
        .next()
        .unwrap_or("")
        .trim_start()
        .starts_with('Z')
}

/// **A process that has exited and not been waited for is gone.** It stays in /proc as
/// a zombie until its parent reaps it, and on a host where nothing reaps an orphan
/// promptly a killed child stays one, so a check that looks only for the directory
/// reads a dead process as alive. The test is the parent and does not wait.
#[test]
fn memory_day_reads_an_unreaped_process_as_gone() {
    let mut child = support::fenced_spawn(&mut Command::new("true")).unwrap();
    let pid = child.id().to_string();
    let zombie = (0..50).any(|_| {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let state = stat
            .rsplit(')')
            .next()
            .unwrap_or("")
            .trim_start()
            .chars()
            .next();
        if state != Some('Z') {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        state == Some('Z')
    });
    assert!(zombie, "the exited child never showed as a zombie");
    assert!(!is_running(&pid), "a zombie was read as running");
    child.wait().unwrap();
}

/// **The deadline stops the server and its store too.** The case starts them in
/// process groups of their own, which a kill of the binary's group does not reach, so a
/// day that timed out would leave both running on the runner. The fake binary starts a
/// child in a session of its own, as the case does, and the case waits for the child to
/// be gone. **The positive is in the same case**: the child's pid was written, so it
/// ran.
#[test]
#[cfg(not(jojobot_sandbox))]
fn memory_day_stops_what_the_binary_started_in_a_group_of_its_own() {
    let dir = bundle("setsid sh -c 'echo $$ > ../../server-pid; exec sleep 300' & sleep 30");
    let out = run(&dir, "2");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(124), "{said}");
    let pid = fs::read_to_string(dir.join("server-pid"))
        .expect("the child ran and wrote its pid")
        .trim()
        .to_string();
    let gone = (0..50).any(|_| {
        let gone = !is_running(&pid);
        if !gone {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        gone
    });
    if !gone {
        // Do not leave the sleeper on the host the case failed on.
        let _ = support::status_fenced(Command::new("kill").args(["-9", &pid]));
    }
    assert!(gone, "process {pid} outlived the deadline kill: {said}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_day_kills_a_run_past_its_deadline_and_says_so() {
    let dir = bundle("sleep 30");
    let began = std::time::Instant::now();
    let out = run(&dir, "1");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(124), "{said}");
    assert!(said.contains("memory-day: TIMED OUT after "), "{said}");
    assert!(
        began.elapsed() < std::time::Duration::from_secs(20),
        "the sleeping binary was waited for"
    );
    the_verdict_file_is_the_last_line(&dir, &said);
    fs::remove_dir_all(dir).unwrap();
}

/// **The Nix sandbox leaves out a case by one marker and keeps no list of names.**
/// A case the sandbox cannot run carries `cfg(not(jojobot_sandbox))`, which the flake
/// sets. Nothing outside this process declares that spelling, so this case pins it.
#[test]
#[cfg(not(jojobot_sandbox))]
fn the_nix_sandbox_skips_cases_by_one_marker_and_keeps_no_list_of_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let flake = fs::read_to_string(root.join("flake.nix")).unwrap();
    let list = flake
        .split("cargoTestExtraArgs =")
        .nth(1)
        .expect("the test arguments");
    let list = &list[..list.find("];").expect("the end of the list")];
    assert!(
        !list.contains("--skip="),
        "a case is skipped by name:\n{list}"
    );
    assert!(flake.contains("RUSTFLAGS = \"--cfg jojobot_sandbox\""));
    let gate = fs::read_to_string(root.join("crates/jojobot/tests/upgrade_gate.rs")).unwrap();
    assert!(gate.lines().any(|l| l == "#![cfg(not(jojobot_sandbox))]"));
}
