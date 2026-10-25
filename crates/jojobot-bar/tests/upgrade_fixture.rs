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
