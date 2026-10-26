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
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-ci-needed"));
    fs::create_dir_all(&dir).unwrap();
    support::git_init(&dir);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    fs::write(dir.join("size-ratchet.toml"), "cap = 2000\n").unwrap();
    fs::write(
        dir.join(".gitignore"),
        "argv.log\nhistory/\ntarget/\nproofs/\nproof.log\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("scripts")).unwrap();
    support::write_executable(
        &dir.join("scripts/pr-proof"),
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> proof.log\ntest -f \"proofs/$1\"\n",
    );
    support::write_executable(
        &dir.join("cargo"),
        r#"#!/bin/sh
echo "$*" >> argv.log
case " $* " in
  *" --no-run "*) echo '  Executable tests/alpha.rs (/x/deps/alpha-1)'; exit 0 ;;
esac
if [ "$1" = metadata ]; then
  echo '{"packages":[],"workspace_members":[],"resolve":{"nodes":[]}}'
fi
if [ "$1" = build ] && grep -q 'compile_error!' crates/alpha/src/lib.rs 2>/dev/null; then
  echo 'error: broken preceding code'
  exit 101
fi
if [ "$1" = test ]; then
  echo 'test alpha ... ok'
  echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
fi
"#,
    );
    git(
        &dir,
        &[
            "add",
            "size-ratchet.toml",
            ".gitignore",
            "cargo",
            "scripts/pr-proof",
        ],
    );
    git(&dir, &["commit", "-qm", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    fs::create_dir_all(dir.join("proofs")).unwrap();
    fs::write(dir.join("proofs").join(&base), "completed full proof\n").unwrap();
    (dir, base)
}

#[test]
fn ci_needed_two_pushes_do_not_hide_a_canceled_code_predecessor() {
    let (dir, proved) = repo();
    fs::create_dir_all(dir.join("crates/alpha/src")).unwrap();
    fs::write(
        dir.join("crates/alpha/src/lib.rs"),
        "compile_error!(\"broken preceding code\");\n",
    )
    .unwrap();
    git(&dir, &["add", "crates/alpha/src/lib.rs"]);
    git(&dir, &["commit", "-qm", "code push"]);
    let canceled = git(&dir, &["rev-parse", "HEAD"]);
    change(&dir, &["README.md"]);
    assert_ne!(proved, canceled);
    assert_eq!(
        git(&dir, &["diff", "--name-only", &canceled, "HEAD"]),
        "README.md"
    );
    let out = bar(&dir, &canceled, "report");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "unchecked code escaped: {out:?}");
    assert!(!text.contains("docs/ratchet-only"), "{text}");
    assert!(text.contains("build: FAILED"), "{text}");
    assert_eq!(
        fs::read_to_string(dir.join("proof.log")).unwrap().trim(),
        canceled
    );

    let plan = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(["plan", "rooms"])
            .current_dir(&dir)
            .env("BAR_CI_BASE", &canceled)
            .env("SIZE_BASE", &canceled)
            .env("CARGO", dir.join("cargo"))
            .env("GITHUB_OUTPUT", dir.join("target/plan-output")),
    )
    .unwrap();
    assert!(plan.status.success(), "{plan:?}");
    assert_eq!(
        fs::read_to_string(dir.join("target/plan-output"))
            .unwrap()
            .trim(),
        "fast=false"
    );
    fs::remove_dir_all(dir).unwrap();
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
            assert_eq!(
                jojobot_ports::scratch::owner(dir.file_name().unwrap().to_str().unwrap()),
                Some(std::process::id()),
                "the orphan sweep must recognize this fixture's owner"
            );
            change(&dir, &paths);
            let out = bar(&dir, &base, lane);
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "{out:?}");
            assert!(
                text.contains("docs/ratchet-only"),
                "{paths:?}, {lane}: {text}"
            );
            assert!(text.contains("size: ok"), "{text}");
            assert_eq!(
                fs::read_to_string(dir.join("proof.log")).unwrap().trim(),
                base
            );
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
    fs::write(dir.join("size-ratchet.toml"), "cap = 5001\n").unwrap();
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
