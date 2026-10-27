//! **The size check turns the run red.** It is a phase of `bar check`, `bar
//! report` and `bar dev`, run first, and it fails the verdict without skipping
//! the phases after it. These cases drive the real binary against a real git
//! tree, so they prove the phase is wired to the exit code and not only that the
//! library judges a change correctly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// **A fake `cargo` where every phase succeeds.** Whatever these cases assert
/// about the exit code is then the size phase's, not another phase failing.
fn write_fake_cargo_always_green(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) echo 'some::test::name: test' ;;
    esac
    exit 0 ;;
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

/// The file `size-ratchet.toml`: the cap, and whatever else the case adds.
fn ratchet(extra: &str) -> String {
    format!("cap = 5000\n{extra}")
}

/// Write a file and its list into the tree, track both and commit them, returning
/// the commit.
fn commit_files(dir: &Path, files: &[(&str, String)]) -> String {
    for (path, text) in files {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(dir.join(parent)).unwrap();
        }
        fs::write(dir.join(path), text).unwrap();
        git(dir, &["add", path]);
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
    bar_with(dir, &[verb], base)
}

fn bar_with(dir: &Path, args: &[&str], base: Option<&str>) -> Ran {
    let fake_cargo = write_fake_cargo_always_green(dir);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(args)
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
        let base = commit_files(&dir, &[("size-ratchet.toml", ratchet(""))]);
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
fn an_unborn_repo_still_refuses_a_new_test_file_over_100_kb() {
    for path in ["tests/homer.sql", "crates/jojobot/tests/homer.sql"] {
        let dir = repo("unborn-test-bytes");
        fs::write(dir.join("size-ratchet.toml"), ratchet("")).unwrap();
        git(&dir, &["add", "size-ratchet.toml"]);
        fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
        fs::write(dir.join(path), vec![b'x'; 101_000]).unwrap();
        git(&dir, &["add", path]);
        let ran = bar(&dir, "size", None);
        assert_eq!(ran.code, Some(1), "{path}: {}", ran.stdout);
        assert!(ran.stdout.contains(path), "{}", ran.stdout);
        assert!(ran.stdout.contains("101000 bytes"), "{}", ran.stdout);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn moving_a_large_file_into_tests_is_a_new_test_file() {
    let dir = repo("test-file-move");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.sql", "x".repeat(101_000)),
        ],
    );
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
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("tests/old.sql", "x".repeat(101_000)),
        ],
    );
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
    commit_files(
        &dir,
        &[(
            "size-ratchet.toml",
            "cap = 5000\ntest_file_bytes = 100001\n".to_string(),
        )],
    );
    let ran = bar(&dir, "size", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("test_file_bytes"), "{}", ran.stdout);
    fs::remove_dir_all(dir).unwrap();
}

/// **A tree that changed nothing is green, and the size line is on the verdict
/// block** between `verdict:` and `log:`, naming both counts and the base.
#[test]
fn a_tree_that_changed_nothing_is_green_with_a_size_line_in_the_verdict_block() {
    let dir = repo("quiet");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", lines_of(5001)),
            ("marge.md", lines_of(2500)),
        ],
    );
    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    let verdict = ran.stdout.find("verdict: GREEN").expect(&ran.stdout);
    let size = ran.stdout.find("size: ok").expect(&ran.stdout);
    let log = ran.stdout.find("log:").expect(&ran.stdout);
    assert!(verdict < size && size < log, "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(
        line.contains("1 red, 0 orange and 1 yellow files"),
        "{line}"
    );
    assert!(line.contains("compared with"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **One added line to a red file turns the run red**, and the size line names the
/// file, the before and the after. The phases after it still run.
#[test]
fn one_line_added_to_a_red_file_is_red_and_names_the_file() {
    let dir = repo("grew");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", lines_of(5001)),
        ],
    );
    fs::write(dir.join("homer.rs"), lines_of(5002)).unwrap();

    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("verdict: RED"), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains("homer.rs: red file grew from 5001 to 5002"),
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

/// **A new file of 5001 lines turns the run red**, whatever it holds.
#[test]
fn a_new_5001_line_markdown_file_is_red() {
    let dir = repo("new");
    let base = commit_files(&dir, &[("size-ratchet.toml", ratchet(""))]);
    fs::write(dir.join("marge.md"), lines_of(5001)).unwrap();
    git(&dir, &["add", "marge.md"]);

    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains("marge.md: new file over the cap, 5001 lines"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A yellow file may grow, and the check says so** without turning red.
#[test]
fn a_yellow_file_that_grew_warns_and_stays_green() {
    let dir = repo("yellow");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", lines_of(2500)),
        ],
    );
    fs::write(dir.join("homer.rs"), lines_of(2600)).unwrap();
    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains("warning: homer.rs: over 2000 lines and grew from 2500 to 2600"),
        "{}",
        ran.stdout
    );
    assert!(size_line(&ran.stdout).contains("0 red, 0 orange and 1 yellow"));
    let _ = fs::remove_dir_all(&dir);
}

/// **A moved red file is the same file**: it may move at its size and may not
/// grow while moving, and the move shows in both areas of the table.
#[test]
fn a_moved_red_file_keeps_its_size_and_cannot_grow_in_the_move() {
    let dir = repo("moved");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("crates/a/src/big.rs", lines_of(5200)),
        ],
    );
    fs::create_dir_all(dir.join("crates/b/src")).unwrap();
    git(&dir, &["mv", "crates/a/src/big.rs", "crates/b/src/big.rs"]);
    let moved = bar(&dir, "check", Some(&base));
    assert_eq!(moved.code, Some(0), "{}", moved.stdout);
    assert!(
        moved.stdout.contains("-5200"),
        "the old area loses it: {}",
        moved.stdout
    );
    assert!(
        moved.stdout.contains("5200"),
        "the new area gains it: {}",
        moved.stdout
    );
    fs::write(dir.join("crates/b/src/big.rs"), lines_of(5201)).unwrap();
    git(&dir, &["add", "crates/b/src/big.rs"]);
    let grown = bar(&dir, "check", Some(&base));
    assert_eq!(grown.code, Some(1), "{}", grown.stdout);
    assert!(
        grown
            .stdout
            .contains("crates/b/src/big.rs: red file grew from 5200 to 5201"),
        "{}",
        grown.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A raise that names the operator's rule lets a red file grow once**, and the
/// check prints it. A base that already held the same rule for the file does not
/// pay for a second raise.
#[test]
fn a_raise_pays_once_and_is_printed() {
    let dir = repo("raise");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", lines_of(5001)),
        ],
    );
    fs::write(dir.join("homer.rs"), lines_of(5010)).unwrap();
    let bare = bar(&dir, "check", Some(&base));
    assert_eq!(bare.code, Some(1), "{}", bare.stdout);

    let raise = "\n[[raise]]\npath = \"homer.rs\"\nrule = 704\n";
    fs::write(dir.join("size-ratchet.toml"), ratchet(raise)).unwrap();
    let named = bar(&dir, "check", Some(&base));
    assert_eq!(named.code, Some(0), "{}", named.stdout);
    assert!(
        named
            .stdout
            .contains("homer.rs: red file grew from 5001 to 5010 under rule 704"),
        "{}",
        named.stdout
    );

    git(&dir, &["add", "homer.rs", "size-ratchet.toml"]);
    git(&dir, &["commit", "-q", "-m", "raise"]);
    let second_base = git(&dir, &["rev-parse", "HEAD"]);
    fs::write(dir.join("homer.rs"), lines_of(5020)).unwrap();
    let again = bar(&dir, "check", Some(&second_base));
    assert_eq!(again.code, Some(1), "{}", again.stdout);
    let _ = fs::remove_dir_all(&dir);
}

/// **With no base the phase never says ok**: growth cannot be judged, so the line
/// says unknown. The run is not failed by it. The same clean tree with a known
/// base is ok.
#[test]
fn with_no_base_the_size_line_says_unknown_and_never_ok() {
    let dir = repo("unknown");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", "fn main() {}\n".to_string()),
        ],
    );
    let without = bar(&dir, "check", None);
    assert_eq!(without.code, Some(0), "{}", without.stdout);
    let line = size_line(&without.stdout);
    assert!(line.starts_with("size: unknown"), "{line}");
    assert!(
        line.contains("SIZE_BASE"),
        "the line names what would set a base: {line}"
    );
    assert!(!without.stdout.contains("size: ok"), "{}", without.stdout);

    let with = bar(&dir, "check", Some(&base));
    assert_eq!(with.code, Some(0), "{}", with.stdout);
    assert!(
        size_line(&with.stdout).starts_with("size: ok"),
        "{}",
        with.stdout
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

/// **A malformed file fails the phase and names the line**, not an empty file.
#[test]
fn a_malformed_list_fails_the_phase_naming_the_line() {
    let dir = repo("malformed");
    commit_files(
        &dir,
        &[
            ("homer.rs", "fn main() {}\n".to_string()),
            (
                "size-ratchet.toml",
                "cap = 5000\n\n[[raise]]\nrule = lots\n".to_string(),
            ),
        ],
    );
    let ran = bar(&dir, "check", None);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    let line = size_line(&ran.stdout);
    assert!(line.starts_with("size: FAILED"), "{line}");
    assert!(line.contains("line 4"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **The area table shows on every run**: net lines by area and by kind, and a
/// deletion is a negative number.
#[test]
fn the_area_table_shows_net_lines_by_area_and_kind() {
    let dir = repo("table");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("crates/a/src/x.rs", lines_of(100)),
            ("crates/a/tests/t.rs", lines_of(50)),
            ("README.md", lines_of(10)),
        ],
    );
    fs::write(dir.join("crates/a/src/x.rs"), lines_of(60)).unwrap();
    fs::write(dir.join("crates/a/tests/t.rs"), lines_of(55)).unwrap();
    fs::write(dir.join("README.md"), lines_of(14)).unwrap();
    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    let table: Vec<&str> = ran.stdout.lines().filter(|l| l.starts_with("  ")).collect();
    let row = |area: &str| {
        table
            .iter()
            .find(|line| line.ends_with(area))
            .unwrap_or_else(|| panic!("no row for {area}: {table:?}"))
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(row("crates/a/x")[..3], ["-40", "0", "0"], "{table:?}");
    assert_eq!(row("crates/a")[..3], ["0", "5", "0"], "{table:?}");
    assert_eq!(row("(root)")[..3], ["0", "0", "4"], "{table:?}");
    assert!(
        table.iter().any(|l| l.contains("source plus tests -35")),
        "{table:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **`bar size --net` prints the table; a positive source plus tests needs a
/// named reason**, and an empty reason file names none.
#[test]
fn net_positive_code_needs_a_reason_file_with_a_reason_in_it() {
    let dir = repo("net");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("crates/a/src/x.rs", lines_of(10)),
        ],
    );
    fs::write(dir.join("crates/a/src/x.rs"), lines_of(30)).unwrap();
    let bare = bar_with(&dir, &["size", "--net"], Some(&base));
    assert_eq!(bare.code, Some(1), "{}", bare.stdout);
    assert!(
        bare.stdout.contains("net lines by area since"),
        "{}",
        bare.stdout
    );
    fs::write(dir.join("reason.txt"), "").unwrap();
    let empty = bar_with(
        &dir,
        &["size", "--net", "--reason-file", "reason.txt"],
        Some(&base),
    );
    assert_eq!(empty.code, Some(1), "{}", empty.stdout);
    fs::write(
        dir.join("reason.txt"),
        "grew to shrink; the fold goes at 9.3.4\n",
    )
    .unwrap();
    let named = bar_with(
        &dir,
        &["size", "--net", "--reason-file", "reason.txt"],
        Some(&base),
    );
    assert_eq!(named.code, Some(0), "{}", named.stdout);
    fs::write(dir.join("crates/a/src/x.rs"), lines_of(3)).unwrap();
    let negative = bar_with(&dir, &["size", "--net"], Some(&base));
    assert_eq!(negative.code, Some(0), "{}", negative.stdout);
    let unknown = bar_with(&dir, &["size", "--net"], None);
    assert_eq!(unknown.code, Some(1), "{}", unknown.stdout);
    let _ = fs::remove_dir_all(&dir);
}

/// Growth anywhere in the branch is refused before the selected crate builds.
#[test]
fn narrow_refuses_growth_in_another_crate_before_cargo_runs() {
    let dir = repo("narrow");
    let path = "crates/jojobot-domain/src/homer.rs";
    let base = commit_files(
        &dir,
        &[("size-ratchet.toml", ratchet("")), (path, lines_of(5001))],
    );
    let args = ["narrow", "--crate", "jojobot-bar"];
    let unchanged = bar_with(&dir, &args, Some(&base));
    assert_eq!(unchanged.code, Some(0), "{}", unchanged.stdout);
    assert!(
        fs::read_to_string(dir.join("argv.log"))
            .unwrap()
            .contains("build --workspace"),
        "a size-clean branch still builds"
    );
    fs::remove_file(dir.join("argv.log")).unwrap();
    fs::write(dir.join(path), lines_of(5002)).unwrap();
    let grew = bar_with(&dir, &args, Some(&base));
    assert_eq!(grew.code, Some(1), "{}", grew.stdout);
    assert!(grew.stdout.contains("size: FAILED"), "{}", grew.stdout);
    assert!(
        grew.stdout
            .contains(&format!("{path}: red file grew from 5001 to 5002")),
        "{}",
        grew.stdout
    );
    assert!(
        !dir.join("argv.log").exists(),
        "the cheap refusal precedes every cargo phase"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **The owned markers are judged through the bar.** The marker texts are built
/// here so this file never matches its own scan.
#[test]
fn a_past_step_a_bare_gap_and_a_step_gone_back_are_red_through_the_bar() {
    let temporary = ["TEMPO", "RARY(9.3.1): x\n"].concat();
    let gap = ["// GA", "P — x\n"].concat();
    let stories = "crates/jojobot/tests/user_stories/x.rs";
    let dir = repo("markers");
    let base = commit_files(
        &dir,
        &[("size-ratchet.toml", ratchet("step = \"9.3.2\"\n"))],
    );
    let red = |files: &[(&str, String)]| {
        let head = commit_files(&dir, files);
        let ran = bar(&dir, "check", Some(&base));
        assert_ne!(ran.code, Some(0), "{head}: {}", ran.stdout);
        ran.stdout
    };
    let past = red(&[("a.rs", format!("// {temporary}"))]);
    assert!(past.contains("a.rs:1"), "{past}");
    let bare = red(&[(stories, gap.clone())]);
    assert!(bare.contains(&format!("{stories}:1")), "{bare}");
    commit_files(&dir, &[(stories, gap.replace("x", "work:q-1001 —"))]);
    let owned = bar(&dir, "check", Some(&base));
    assert!(
        owned.stdout.contains("a.rs:1"),
        "only the past step is left"
    );
    assert!(!owned.stdout.contains(stories), "{}", owned.stdout);
    let back = red(&[
        ("a.rs", "fn a() {}\n".into()),
        ("size-ratchet.toml", ratchet("step = \"9.3.1\"\n")),
    ]);
    assert!(back.contains("step went back"), "{back}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A base that was named and cannot be used is red.** Unset stays a note.
#[test]
fn a_named_base_that_does_not_resolve_is_red_through_the_bar() {
    let dir = repo("named-base");
    commit_files(&dir, &[("size-ratchet.toml", ratchet(""))]);
    let ran = bar(&dir, "check", Some("no-such-revision"));
    assert_ne!(ran.code, Some(0), "{}", ran.stdout);
    assert!(size_line(&ran.stdout).starts_with("size: unknown"));
    let _ = fs::remove_dir_all(&dir);
}

/// **A step cannot be dropped to get past the rule.** The base holds a step and a
/// marker retires at it; the branch deletes the step line.
#[test]
fn a_deleted_step_is_red_through_the_bar() {
    let temporary = ["TEMPO", "RARY(9.3.1): x\n"].concat();
    let dir = repo("step-deleted");
    let base = commit_files(
        &dir,
        &[("size-ratchet.toml", ratchet("step = \"9.3.2\"\n"))],
    );
    commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("a.rs", format!("// {temporary}")),
        ],
    );
    let ran = bar(&dir, "check", Some(&base));
    assert_ne!(ran.code, Some(0), "{}", ran.stdout);
    assert!(ran.stdout.contains("step"), "{}", ran.stdout);
    let _ = fs::remove_dir_all(&dir);
}

/// **A step that is no step is refused at the parse**, so it cannot hide a marker.
#[test]
fn a_garbled_step_is_red_through_the_bar() {
    let temporary = ["TEMPO", "RARY(9.3.1): x\n"].concat();
    let dir = repo("step-garbled");
    let base = commit_files(
        &dir,
        &[("size-ratchet.toml", ratchet("step = \"9.3.2\"\n"))],
    );
    commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("step = \"soon\"\n")),
            ("a.rs", format!("// {temporary}")),
        ],
    );
    let ran = bar(&dir, "check", Some(&base));
    assert_ne!(ran.code, Some(0), "{}", ran.stdout);
    assert!(ran.stdout.contains("step must be"), "{}", ran.stdout);
    let _ = fs::remove_dir_all(&dir);
}

/// **A file over 3,000 lines is orange and says when it turns red; one at 2,999
/// is yellow only.** The weekly number counts the orange file apart.
#[test]
fn a_file_over_3000_lines_is_orange_and_one_under_it_is_not() {
    let dir = repo("orange");
    let base = commit_files(
        &dir,
        &[
            ("size-ratchet.toml", ratchet("")),
            ("homer.rs", lines_of(3001)),
            ("marge.rs", lines_of(2999)),
            ("bart.rs", lines_of(3000)),
        ],
    );
    let ran = bar(&dir, "check", Some(&base));
    assert_eq!(ran.code, Some(0), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains("homer.rs: over 3000 lines (orange), 3001"),
        "{}",
        ran.stdout
    );
    assert!(
        !ran.stdout.contains("marge.rs: over 3000") && !ran.stdout.contains("bart.rs: over 3000"),
        "{}",
        ran.stdout
    );
    assert!(size_line(&ran.stdout).contains("0 red, 1 orange and 2 yellow"));
    let weekly = bar_with(&dir, &["size", "--weekly"], None);
    assert!(
        weekly.stdout.contains("0 red, 1 orange, 2 yellow"),
        "{}",
        weekly.stdout
    );
    let _ = fs::remove_dir_all(&dir);
}
