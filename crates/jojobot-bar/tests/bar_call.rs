//! **`bar call` records a person's judgement of a red.** The test that failed
//! and was later called a flake, or a defect, is written as a file of its own
//! under the history directory, so a reader of the history can tell the two
//! apart. The real `bar` binary runs against a history directory the case
//! names; the real one is never written.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A directory no other case in this process uses.
fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-call"));

    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn call(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .arg("call")
        .args(args)
        .current_dir(dir)
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .output()
        .expect("run bar call")
}

fn written(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir.join("history/calls"))
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// **A call is written as one file under `calls/`, and says who made it.**
#[test]
fn a_flake_call_is_written_as_a_file_that_says_what_who_and_when() {
    let dir = scratch();
    let output = call(
        &dir,
        &[
            "--test",
            "mailboxes::tests::a_poll_finds_the_post",
            "--call",
            "flake",
            "--by",
            "bot:alpha",
            "--run",
            "20261008233044-jojobot-1",
            "--head",
            "679be4568266033d84813ac2e21abb727f7db907",
            "--note",
            "green on the same commit when run again",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let files = written(&dir);
    assert_eq!(files.len(), 1, "one call, one file: {files:?}");
    let text = fs::read_to_string(&files[0]).expect("the call");
    assert!(
        text.starts_with(
            "{\"test\":\"mailboxes::tests::a_poll_finds_the_post\",\"call\":\"flake\",\
             \"by\":\"bot:alpha\",\"run_id\":\"20261008233044-jojobot-1\",\
             \"head\":\"679be4568266033d84813ac2e21abb727f7db907\",\"at\":\""
        ),
        "{text}"
    );
    assert!(
        text.ends_with("\",\"note\":\"green on the same commit when run again\"}"),
        "{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **The run and the note are optional, and a call without them says so.**
#[test]
fn a_defect_call_without_a_run_or_a_note_records_neither() {
    let dir = scratch();
    let output = call(
        &dir,
        &["--test", "a::b", "--call", "defect", "--by", "bot:alpha"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&written(&dir)[0]).expect("the call");
    assert!(text.contains("\"call\":\"defect\""), "{text}");
    assert!(text.contains("\"run_id\":null,\"head\":null"), "{text}");
    assert!(text.ends_with("\"note\":null}"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A call that is not a flake or a defect, or that names no test or no
/// caller, is refused with the usage and writes nothing.**
#[test]
fn a_call_that_is_not_understood_is_refused_and_writes_nothing() {
    let dir = scratch();
    for args in [
        &["--test", "a::b", "--call", "maybe", "--by", "bot:alpha"][..],
        &["--call", "flake", "--by", "bot:alpha"][..],
        &["--test", "a::b", "--call", "flake"][..],
        &["--test", "a::b", "--by", "bot:alpha"][..],
        &[
            "--test",
            "a::b",
            "--call",
            "flake",
            "--by",
            "bot:alpha",
            "--whenever",
            "x",
        ][..],
    ] {
        let output = call(&dir, args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage: bar call"),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(written(&dir).is_empty(), "a refused call writes nothing");
    let _ = fs::remove_dir_all(&dir);
}

/// **Two calls on one test are two files**, and neither replaces the other:
/// the second call about a test does not erase the first.
#[test]
fn two_calls_on_one_test_are_two_files() {
    let dir = scratch();
    for call_word in ["flake", "defect"] {
        let output = call(
            &dir,
            &["--test", "a::b", "--call", call_word, "--by", "bot:alpha"],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The file name carries the second the call was made, so the second
        // call waits for the next one.
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    assert_eq!(written(&dir).len(), 2);
    let _ = fs::remove_dir_all(&dir);
}
