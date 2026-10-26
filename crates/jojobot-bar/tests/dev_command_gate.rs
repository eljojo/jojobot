//! The Bash hook refuses expensive local commands before they can execute.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod support;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn invoke(hook: &mut Command, command: &str) -> Output {
    let input = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command }
    });
    let mut child = support::fenced_spawn(
        hook.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .expect("start the hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn direct(command: &str) -> Output {
    invoke(
        Command::new("python3").arg(root().join("scripts/dev-command-gate")),
        command,
    )
}

fn refused(out: &Output) -> bool {
    let text = String::from_utf8_lossy(&out.stderr);
    out.status.code() == Some(2)
        && out.stdout.is_empty()
        && text.lines().count() == 1
        && ["make dev", "make narrow", "CRATE=", "FILTER="]
            .iter()
            .all(|form| text.contains(form))
}

#[test]
fn dev_command_gate_refuses_local_build_bypasses() {
    let commands = [
        "cargo test -p x",
        "cargo build",
        "cargo run -p jojobot",
        "cargo check",
        "nix develop -c cargo test -p x",
        "make check",
        "make report",
        "make test",
        "make",
        "make dev check",
        "git worktree add /tmp/alpha",
        "git -C /tmp/alpha worktree add /tmp/beta",
        "git clone /tmp/alpha /tmp/beta",
        "export CARGO_TARGET_DIR=/tmp/alpha",
        "CARGO_TARGET_DIR=/tmp/alpha make dev",
        "env CARGO_TARGET_DIR=/tmp/alpha make narrow CRATE=jojobot-bar FILTER=foo",
        "nix develop -c env CARGO_TARGET_DIR=/tmp/alpha make dev",
        "make narrow CRATE=jojobot-bar FILTER='foo bar'",
        "make narrow CRATE=jojobot-bar FILTER=foo FILTER=bar",
        "make narrow CRATE=jojobot-bar",
        "git status && cargo test",
        "git status\nmake report",
        "scripts/sabotage file before after -- make check",
    ];
    let failures: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            let out = direct(command);
            (!refused(&out)).then(|| format!("{command}: {out:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dev_command_gate_allows_narrow_and_read_commands() {
    let commands = [
        "make dev",
        "make narrow CRATE=jojobot-domain FILTER=foo",
        "nix develop -c make narrow CRATE=jojobot-bar FILTER=foo",
        "scripts/sabotage file before after -- make narrow CRATE=jojobot-domain FILTER=foo",
        "scripts/sabotage file 'cargo test' 'make check' -- make narrow CRATE=jojobot-bar FILTER=foo",
        "git status",
        "git log --oneline -3",
        "git diff --stat",
        "git show HEAD:Makefile",
        "git fetch forgejo",
        "git switch -c dev-alpha-work",
        "git -C /tmp/alpha status",
        "git log --grep 'cargo test'",
        "echo cargo test",
        "echo \"$CARGO_TARGET_DIR\"",
        "printf '%s\\n' 'make check'",
    ];
    let failures: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            let out = direct(command);
            (!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty()))
                .then(|| format!("{command}: {out:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dev_command_gate_project_settings_run_the_guard() {
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root().join(".claude/settings.json")).unwrap())
            .unwrap();
    let groups = settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("project PreToolUse hooks");
    let bash = groups
        .iter()
        .find(|group| group["matcher"] == "Bash")
        .expect("Bash matcher");
    let hook = &bash["hooks"][0];
    assert_eq!(hook["type"], "command");
    let mut command = Command::new("sh");
    command
        .args(["-c", hook["command"].as_str().unwrap()])
        .env("CLAUDE_PROJECT_DIR", root())
        .current_dir(root().join("crates/jojobot-bar"));
    let out = invoke(&mut command, "cargo test");
    assert!(refused(&out), "{out:?}");
    let out = invoke(&mut command, "make dev");
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
}
