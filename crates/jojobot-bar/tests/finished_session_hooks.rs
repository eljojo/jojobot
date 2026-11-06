//! The configured Claude hooks mark a wrap call and refuse later prompts.
//! Host witnesses still have to exercise Claude's prompt dispatch itself.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

mod support;

const AGENT: &str = "00000000-0000-4000-8000-000000000001";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(jojobot_bar::scratch::name("finished-session"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn invoke(command: &mut Command, input: &Value, dir: &Path, agent: Option<&str>) -> Output {
    command
        .env("HOME", dir.join("home"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env_remove("PASEO_AGENT_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(agent) = agent {
        command.env("PASEO_AGENT_ID", agent);
    }
    let mut child = support::fenced_spawn(command).unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn hook(name: &str, input: &Value, dir: &Path, agent: Option<&str>) -> Output {
    invoke(
        Command::new("python3").arg(root().join("scripts").join(name)),
        input,
        dir,
        agent,
    )
}

// Claude 2.1.310 forwards MCP content to PostToolUse, without structuredContent.
fn wrap_call() -> Value {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "mcp__jojobot__wrap_session",
        "tool_input": {"sid": "not-a-real-key", "story": "Done."},
        "tool_response": [{"type": "text", "text": "{\"status\":\"blocked\"}"}]
    })
}

fn marker(dir: &Path) -> PathBuf {
    dir.join("state/jojobot/finished").join(AGENT)
}

fn matches(pattern: &str, name: &str) -> bool {
    support::output_fenced(Command::new("python3").args([
        "-c",
        "import re, sys; sys.exit(0 if re.search(sys.argv[1], sys.argv[2]) else 1)",
        pattern,
        name,
    ]))
    .unwrap()
    .status
    .success()
}

fn quiet(out: &Output) {
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
}

#[test]
fn finished_session_hooks_mark_then_refuse_only_later_prompts() {
    let dir = scratch();
    quiet(&hook(
        "mark-finished-session",
        &wrap_call(),
        &dir,
        Some(AGENT),
    ));
    let kept: Value = serde_json::from_slice(&std::fs::read(marker(&dir)).unwrap()).unwrap();
    assert_eq!(kept["agent"], AGENT);
    assert_eq!(kept.as_object().unwrap().len(), 2);
    let at = kept["wrapped_at"].as_str().unwrap();
    assert!(matches(
        r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?\+00:00$",
        at
    ));
    assert!(!kept.to_string().contains("not-a-real-key"));
    let prompt = json!({"hook_event_name": "UserPromptSubmit", "prompt": "Resume"});
    let out = hook("refuse-finished-session", &prompt, &dir, Some(AGENT));
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let reason = String::from_utf8(out.stderr).unwrap();
    for value in [at, "document:build-runtime"] {
        assert!(reason.contains(value), "{reason}");
    }
    assert!(reason.to_lowercase().contains("end the turn"), "{reason}");
    assert!(out.stdout.is_empty());
    quiet(&hook(
        "refuse-finished-session",
        &prompt,
        &dir,
        Some("another-agent"),
    ));
    std::fs::remove_file(marker(&dir)).unwrap();
    quiet(&hook("refuse-finished-session", &prompt, &dir, Some(AGENT)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn finished_session_hooks_ignore_other_tools_and_report_unidentified_wraps() {
    let dir = scratch();
    for tool in [
        "mcp__jojobot__recall",
        "Bash",
        "mcp__jojobot__wrap_session_later",
    ] {
        let mut event = wrap_call();
        event["tool_name"] = json!(tool);
        quiet(&hook("mark-finished-session", &event, &dir, Some(AGENT)));
        assert!(!marker(&dir).exists());
    }
    let out = hook("mark-finished-session", &wrap_call(), &dir, None);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("PASEO_AGENT_ID")
    );
    quiet(&hook("refuse-finished-session", &json!({}), &dir, None));
    assert!(!marker(&dir).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn finished_session_hooks_accept_empty_content_and_default_state_home() {
    let dir = scratch();
    let mut event = wrap_call();
    event["tool_response"] = json!([]);
    let mut mark = Command::new("sh");
    mark.args([
        "-c",
        "unset XDG_STATE_HOME; exec python3 \"$CLAUDE_PROJECT_DIR/scripts/mark-finished-session\"",
    ])
    .env("CLAUDE_PROJECT_DIR", root());
    quiet(&invoke(&mut mark, &event, &dir, Some(AGENT)));
    assert!(
        dir.join("home/.local/state/jojobot/finished")
            .join(AGENT)
            .is_file()
    );
    let mut check = Command::new("sh");
    check.args(["-c", "unset XDG_STATE_HOME; exec python3 \"$CLAUDE_PROJECT_DIR/scripts/refuse-finished-session\""])
        .env("CLAUDE_PROJECT_DIR", root());
    assert_eq!(
        invoke(&mut check, &json!({}), &dir, Some(AGENT))
            .status
            .code(),
        Some(2)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn finished_session_hooks_project_settings_route_the_journey() {
    let settings: Value =
        serde_json::from_slice(&std::fs::read(root().join(".claude/settings.json")).unwrap())
            .unwrap();
    let dir = scratch();
    for tool in [
        "mcp__jojobot__wrap_session",
        "mcp__claude_ai_jojobot__wrap_session",
    ] {
        let group = settings["hooks"]["PostToolUse"]
            .as_array()
            .expect("wrap hook")
            .iter()
            .find(|group| matches(group["matcher"].as_str().unwrap(), tool))
            .expect("wrap matcher");
        let mut event = wrap_call();
        event["tool_name"] = json!(tool);
        let mut command = Command::new("sh");
        command
            .args(["-c", group["hooks"][0]["command"].as_str().unwrap()])
            .env("CLAUDE_PROJECT_DIR", root())
            .current_dir(root().join("crates/jojobot-bar"));
        quiet(&invoke(&mut command, &event, &dir, Some(AGENT)));
        assert!(marker(&dir).exists());
    }
    let group = &settings["hooks"]["UserPromptSubmit"][0];
    let mut command = Command::new("sh");
    command
        .args([
            "-c",
            group["hooks"][0]["command"].as_str().expect("prompt hook"),
        ])
        .env("CLAUDE_PROJECT_DIR", root());
    assert_eq!(
        invoke(
            &mut command,
            &json!({"prompt": "Resume"}),
            &dir,
            Some(AGENT)
        )
        .status
        .code(),
        Some(2)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
