//! Only a known, entirely allowlisted diff can leave the code path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod support;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = support::output_fenced(Command::new("git").args(args).current_dir(dir)).unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn repo() -> (PathBuf, String) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-ci-needed-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    support::git_init(&dir);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    fs::write(dir.join("size-ratchet.toml"), "cap = 2000\n").unwrap();
    fs::write(dir.join(".gitignore"), "argv.log\nhistory/\ntarget/\n").unwrap();
    support::write_executable(
        &dir.join("cargo"),
        r#"#!/bin/sh
echo "$*" >> argv.log
case " $* " in
  *" --no-run "*) echo '  Executable tests/alpha.rs (/x/deps/alpha-1)'; exit 0 ;;
esac
if [ "$1" = test ]; then
  echo 'test alpha ... ok'
  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
fi
"#,
    );
    git(&dir, &["add", "size-ratchet.toml", ".gitignore", "cargo"]);
    git(&dir, &["commit", "-qm", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    (dir, base)
}

fn change(dir: &Path, paths: &[&str]) {
    for path in paths {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let text = if *path == "size-ratchet.toml" {
            "cap = 2000\n# measured\n"
        } else {
            "changed\n"
        };
        fs::write(file, text).unwrap();
        git(dir, &["add", path]);
    }
    git(dir, &["commit", "-qm", "change"]);
}

fn bar(dir: &Path, base: &str, lane: &str) -> Output {
    support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg(lane)
            .current_dir(dir)
            .env("BAR_CI_BASE", base)
            .env("SIZE_BASE", base)
            .env("CARGO", dir.join("cargo"))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env_remove("BAR_JOBS"),
    )
    .unwrap()
}

#[test]
fn ci_needed_docs_pm_and_ratchet_run_size_without_code_tests() {
    for paths in [
        vec!["README.md"],
        vec!["pm/ruling"],
        vec!["size-ratchet.toml"],
    ] {
        for lane in ["report", "check"] {
            let (dir, base) = repo();
            change(&dir, &paths);
            let out = bar(&dir, &base, lane);
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "{out:?}");
            assert!(
                text.contains("docs/ratchet-only"),
                "{paths:?}, {lane}: {text}"
            );
            assert!(text.contains("size: ok"), "{text}");
            assert!(
                !fs::read_to_string(dir.join("argv.log"))
                    .unwrap_or_default()
                    .contains("test "),
                "code tests ran: {text}"
            );
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn ci_needed_mixed_code_crates_markdown_and_unknown_base_keep_full_checks() {
    for (paths, unknown) in [
        (vec!["README.md", "src/lib.rs"], false),
        (vec!["crates/jojobot-mcp/served-texts/tools.md"], false),
        (vec!["README.md"], true),
    ] {
        let (dir, base) = repo();
        change(&dir, &paths);
        let out = bar(&dir, if unknown { "missing" } else { &base }, "report");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("docs/ratchet-only"), "{paths:?}: {text}");
        assert!(
            fs::read_to_string(dir.join("argv.log"))
                .unwrap()
                .contains("test "),
            "full path skipped: {out:?}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn ci_needed_ratchet_only_still_rejects_a_cap_raise() {
    let (dir, base) = repo();
    fs::write(dir.join("size-ratchet.toml"), "cap = 2001\n").unwrap();
    git(&dir, &["add", "size-ratchet.toml"]);
    git(&dir, &["commit", "-qm", "bad raise"]);
    let out = bar(&dir, &base, "check");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        text.contains("docs/ratchet-only") && text.contains("size: FAILED"),
        "{text}"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ci_needed_workflows_use_the_conservative_plan_for_expensive_steps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for workflow in ["branch", "pull-request"] {
        let text =
            fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml"))).unwrap();
        for job in text.split("    steps:\n").skip(1) {
            assert!(
                job.contains("id: plan") && text.contains("BAR_CI_BASE:"),
                "{workflow}: {job}"
            );
            for step in job.split("      - ").skip(1) {
                if step.contains("name: test-bundle")
                    || step.lines().any(|line| {
                        let line = line.trim();
                        (line.starts_with("run:")
                            && [
                                "make bundle ",
                                "make rooms",
                                "make year ",
                                "make check CI_ROOT=",
                                "tar -xzf",
                            ]
                            .iter()
                            .any(|s| line.contains(s)))
                            || (line.starts_with("uses:") && line.contains("/download-artifact@"))
                    })
                {
                    assert!(
                        step.contains("if: ${{ steps.plan.outputs.fast != 'true' }}"),
                        "{workflow}: {step}"
                    );
                }
            }
        }
    }
}
