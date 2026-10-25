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
