//! **The size check turns the run red.** It is a phase of `bar check`, `bar
//! report` and `bar dev`, run first, and it fails the verdict without skipping
//! the phases after it. These cases drive the real binary against a real git
//! tree, so they prove the phase is wired to the exit code and not only that the
//! library judges a list correctly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// **A fake `cargo` where every phase succeeds.** Whatever these cases assert
/// about the exit code is then the size phase's, not another phase failing.
fn write_fake_cargo_always_green(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*) echo 'some::test::name: test'; exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-size-test-{tag}"
    )));

    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out =
        support::output_fenced(Command::new("git").args(args).current_dir(dir)).expect("run git");
    assert!(out.status.success(), "git {args:?} failed in {dir:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository of its own, so `git ls-files` reads the scratch tree and not
/// this workspace.
fn repo(tag: &str) -> PathBuf {
    let dir = scratch_dir(tag);
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    dir
}

fn lines_of(count: usize) -> String {
    "x\n".repeat(count)
}

/// A ratchet file listing the given files at the given counts, under a cap of
/// 2000. The third part is the card an entry names, for the entries that name one.
fn ratchet(entries: &[(&str, usize, Option<u32>)]) -> String {
    let mut text = String::from("cap = 2000\n");
    for (path, lines, card) in entries {
        text.push_str(&format!("\n[[file]]\npath = \"{path}\"\nlines = {lines}\n"));
        if let Some(card) = card {
            text.push_str(&format!("raised_by = {card}\n"));
        }
    }
    text
}

/// Track `files` and commit them, returning the commit.
fn commit(dir: &Path, files: &[&str]) -> String {
    for file in files {
        git(dir, &["add", file]);
    }
    git(dir, &["commit", "-q", "-m", "scratch"]);
    git(dir, &["rev-parse", "HEAD"])
}

struct Ran {
    code: Option<i32>,
    stdout: String,
}

/// Run `bar <verb>` over the fake cargo, with `base` as the size base when given.
fn bar(dir: &Path, verb: &str, base: Option<&str>) -> Ran {
    let fake_cargo = write_fake_cargo_always_green(dir);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .arg(verb)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", &fake_cargo)
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env_remove("SIZE_BASE")
        .env_remove("GITHUB_BASE_REF");
    if let Some(base) = base {
        command.env("SIZE_BASE", base);
    }
    let output = support::output_fenced(&mut command).expect("run bar");
    Ran {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    }
}

fn size_line(stdout: &str) -> &str {
    stdout
        .lines()
        .find(|line| line.starts_with("size:"))
        .unwrap_or_else(|| panic!("no size line: {stdout}"))
}

#[test]
fn a_new_test_file_over_100_kb_is_refused() {
    for path in ["tests/homer.sql", "crates/jojobot/tests/homer.sql"] {
        let dir = repo("test-bytes");
        fs::write(dir.join("size-ratchet.toml"), ratchet(&[])).unwrap();
        let base = commit(&dir, &["size-ratchet.toml"]);
        fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        fs::write(dir.join(path), vec![b'x'; 101_000]).unwrap();
        git(&dir, &["add", path]);
        let ran = bar(&dir, "size", Some(&base));
        assert_eq!(ran.code, Some(1), "{path}: {}", ran.stdout);
        assert!(ran.stdout.contains(path), "{}", ran.stdout);
        assert!(ran.stdout.contains("101000 bytes"), "{}", ran.stdout);
        fs::remove_file(dir.join(path)).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn moving_a_large_file_into_tests_is_a_new_test_file() {
    let dir = repo("test-file-move");
    fs::write(dir.join("size-ratchet.toml"), ratchet(&[])).unwrap();
    fs::write(dir.join("homer.sql"), vec![b'x'; 101_000]).unwrap();
    let base = commit(&dir, &["size-ratchet.toml", "homer.sql"]);
    fs::create_dir_all(dir.join("tests")).unwrap();
    git(&dir, &["mv", "homer.sql", "tests/homer.sql"]);
    let ran = bar(&dir, "size", Some(&base));
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("101000 bytes"), "{}", ran.stdout);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_test_file_limit_allows_the_boundary_and_existing_files() {
    let dir = repo("test-file-boundary");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(dir.join("size-ratchet.toml"), ratchet(&[])).unwrap();
    fs::write(dir.join("tests/old.sql"), vec![b'x'; 101_000]).unwrap();
    let base = commit(&dir, &["size-ratchet.toml", "tests/old.sql"]);
    for (path, bytes) in [("tests/homer.sql", 100_000), ("marge.sql", 101_000)] {
        fs::write(dir.join(path), vec![b'x'; bytes]).unwrap();
        git(&dir, &["add", path]);
    }
    let ran = bar(&dir, "size", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_test_file_limit_cannot_be_raised_without_a_base() {
    let dir = repo("test-file-limit-raise");
    fs::write(
        dir.join("size-ratchet.toml"),
        "cap = 2000\ntest_file_bytes = 100001\n",
    )
    .unwrap();
    commit(&dir, &["size-ratchet.toml"]);
    let ran = bar(&dir, "size", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("test_file_bytes"), "{}", ran.stdout);
    fs::remove_dir_all(dir).unwrap();
}

/// **A tree that changed nothing is green, and the size line is on the verdict
/// block** between `verdict:` and `log:`, naming both counts.
#[test]
fn a_tree_that_changed_nothing_is_green_with_a_size_line_in_the_verdict_block() {
    let dir = repo("quiet");
    fs::write(dir.join("homer.rs"), lines_of(2001)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2001, None)]),
    )
    .unwrap();
    commit(&dir, &["homer.rs", "size-ratchet.toml"]);

    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    let verdict = ran.stdout.find("verdict: GREEN").expect(&ran.stdout);
    let size = ran.stdout.find("size: ok").expect(&ran.stdout);
    let log = ran.stdout.find("log:").expect(&ran.stdout);
    assert!(verdict < size && size < log, "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(line.contains("1 files over 2000 lines"), "{line}");
    assert!(line.contains("1 listed in size-ratchet.toml"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **One added line to a listed file turns the run red**, and the size line
/// names the file, the before and the after. The phases after it still run.
#[test]
fn one_line_added_to_a_listed_file_is_red_and_names_the_file() {
    let dir = repo("grew");
    fs::write(dir.join("homer.rs"), lines_of(2001)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2001, None)]),
    )
    .unwrap();
    commit(&dir, &["homer.rs", "size-ratchet.toml"]);
    fs::write(dir.join("homer.rs"), lines_of(2002)).unwrap();

    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("verdict: RED"), "{}", ran.stdout);
    assert!(
        ran.stdout.contains("homer.rs: grew from 2001 to 2002"),
        "{}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("fmt-check: ok"),
        "later phases run: {}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("test: ok"),
        "later phases run: {}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A new file of 2001 lines turns the run red**: it is over the cap and has
/// no entry.
#[test]
fn a_new_file_over_the_cap_is_red() {
    let dir = repo("new");
    fs::write(dir.join("size-ratchet.toml"), ratchet(&[])).unwrap();
    commit(&dir, &["size-ratchet.toml"]);
    fs::write(dir.join("marge.rs"), lines_of(2001)).unwrap();
    git(&dir, &["add", "marge.rs"]);

    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains("marge.rs: new file over the cap, 2001 lines"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A raise with no card is red against the base; a raise that names a card is
/// green.** The raised entry matches the file in both runs, so only the base
/// half can tell them apart.
#[test]
fn a_raised_entry_without_a_card_is_red_and_with_a_card_is_green() {
    let dir = repo("raise");
    fs::write(dir.join("homer.rs"), lines_of(2001)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2001, None)]),
    )
    .unwrap();
    let base = commit(&dir, &["homer.rs", "size-ratchet.toml"]);

    fs::write(dir.join("homer.rs"), lines_of(2005)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2005, None)]),
    )
    .unwrap();
    let bare = bar(&dir, "check", Some(&base));
    assert_eq!(bare.code, Some(1), "{}", bare.stdout);
    assert!(
        bare.stdout
            .contains("homer.rs: the entry went from 2001 to 2005"),
        "{}",
        bare.stdout
    );

    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2005, Some(2778))]),
    )
    .unwrap();
    let named = bar(&dir, "check", Some(&base));
    assert_eq!(named.code, Some(0), "{}", named.stdout);
    assert!(
        size_line(&named.stdout).contains("compared with"),
        "{}",
        named.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A card that justified the base's entry does not justify another raise.**
#[test]
fn a_card_already_named_at_the_base_does_not_pay_for_a_second_raise() {
    let dir = repo("again");
    fs::write(dir.join("homer.rs"), lines_of(2005)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2005, Some(2778))]),
    )
    .unwrap();
    let base = commit(&dir, &["homer.rs", "size-ratchet.toml"]);
    fs::write(dir.join("homer.rs"), lines_of(2009)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2009, Some(2778))]),
    )
    .unwrap();

    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("without a new card"), "{}", ran.stdout);
    let _ = fs::remove_dir_all(&dir);
}

/// **With no base the half says unknown and the tree half still decides.** An
/// unknown base is never printed as a pass.
#[test]
fn with_no_base_the_size_line_says_unknown_and_never_ok() {
    let dir = repo("unknown");
    fs::write(dir.join("homer.rs"), "fn main() {}\n").unwrap();
    commit(&dir, &["homer.rs"]);

    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(line.contains("base: unknown"), "{line}");
    assert!(
        line.contains("SIZE_BASE"),
        "the line names what would set a base: {line}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A base with no list at it is unknown too**, the way the first change that
/// adds the list meets it.
#[test]
fn a_base_with_no_list_is_unknown() {
    let dir = repo("firstlist");
    fs::write(dir.join("homer.rs"), "fn main() {}\n").unwrap();
    let base = commit(&dir, &["homer.rs"]);

    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    assert!(
        size_line(&ran.stdout).contains("base: unknown — no size-ratchet.toml at"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A tree the bar cannot list is a failure, never no growth.** A directory
/// that is no git repository gives no tracked files to count.
#[test]
fn a_directory_that_is_no_repository_fails_the_phase() {
    let dir = scratch_dir("norepo");
    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(line.starts_with("size: FAILED"), "{line}");
    assert!(line.contains("not measured"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A malformed list fails the phase and names the line**, not an empty list.
#[test]
fn a_malformed_list_fails_the_phase_naming_the_line() {
    let dir = repo("malformed");
    fs::write(dir.join("homer.rs"), "fn main() {}\n").unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        "cap = 2000\n\n[[file]]\nlines = lots\n",
    )
    .unwrap();
    commit(&dir, &["homer.rs", "size-ratchet.toml"]);

    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(line.starts_with("size: FAILED"), "{line}");
    assert!(line.contains("line 4"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **`bar report` runs the same phase**, and an offence turns it red too.
#[test]
fn the_report_level_is_red_for_growth_as_well() {
    let dir = repo("report");
    fs::write(dir.join("homer.rs"), lines_of(2001)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2000, None)]),
    )
    .unwrap();
    commit(&dir, &["homer.rs", "size-ratchet.toml"]);

    let ran = bar(&dir, "report", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stdout.contains("homer.rs: grew from 2000 to 2001"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **`bar size` alone checks, and `--write` lowers a stale entry and adds
/// nothing.** A new file over the cap stays refused after `--write`.
#[test]
fn bar_size_write_lowers_a_stale_entry_and_adds_nothing() {
    let dir = repo("write");
    fs::write(dir.join("homer.rs"), lines_of(2003)).unwrap();
    fs::write(dir.join("marge.rs"), lines_of(2500)).unwrap();
    fs::write(
        dir.join("size-ratchet.toml"),
        ratchet(&[("homer.rs", 2010, None)]),
    )
    .unwrap();
    commit(&dir, &["homer.rs", "marge.rs", "size-ratchet.toml"]);

    let before = bar(&dir, "size", None);
    assert_eq!(before.code, Some(1), "{}", before.stdout);
    assert!(
        before.stdout.contains("Run `bar size --write`"),
        "{}",
        before.stdout
    );

    let written = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(["size", "--write"])
            .current_dir(&dir)
            .env_remove("SIZE_BASE")
            .env_remove("GITHUB_BASE_REF"),
    )
    .expect("run bar size --write");
    let out = String::from_utf8_lossy(&written.stdout);
    assert_eq!(
        written.status.code(),
        Some(1),
        "marge.rs is still new: {out}"
    );
    let list = fs::read_to_string(dir.join("size-ratchet.toml")).unwrap();
    assert!(list.contains("lines = 2003"), "{list}");
    assert!(!list.contains("marge.rs"), "--write adds nothing: {list}");
    assert!(out.contains("marge.rs: new file over the cap"), "{out}");
    let _ = fs::remove_dir_all(&dir);
}

/// **`--init` writes the first list from the tree and refuses a second.**
#[test]
fn bar_size_init_writes_the_first_list_once() {
    let dir = repo("init");
    fs::write(dir.join("homer.rs"), lines_of(2004)).unwrap();
    git(&dir, &["add", "homer.rs"]);
    let run = |dir: &Path| {
        support::output_fenced(
            Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
                .args(["size", "--init"])
                .current_dir(dir),
        )
        .expect("run bar size --init")
    };
    let first = run(&dir);
    assert!(first.status.success());
    let list = fs::read_to_string(dir.join("size-ratchet.toml")).unwrap();
    assert!(
        list.contains("path = \"homer.rs\"") && list.contains("lines = 2004"),
        "{list}"
    );
    let second = run(&dir);
    assert_eq!(second.status.code(), Some(1));
    let _ = fs::remove_dir_all(&dir);
}

/// **`bar narrow` runs no size phase**: it is the inner loop between edits.
#[test]
fn narrow_prints_no_size_line() {
    let dir = repo("narrow");
    fs::write(dir.join("homer.rs"), lines_of(2001)).unwrap();
    git(&dir, &["add", "homer.rs"]);
    let fake_cargo = write_fake_cargo_always_green(&dir);
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(["narrow", "--crate", "jojobot-bar"])
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &fake_cargo),
    )
    .expect("run bar narrow");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("size:"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}
