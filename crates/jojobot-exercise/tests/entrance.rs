//! **The entrance, walked through rather than assumed.**
//!
//! Everything under the paid target is tested; the target itself was not, and a
//! tier whose entrance nobody has walked through is the shape this project has
//! already paid for once — a suite that existed and was never run.
//!
//! These run the real binary and cost nothing, because every case here is one
//! that must stop BEFORE a room is built or anything is billed. That is the
//! property being asserted: the refusals are cheap, and they are refusals.

use std::process::Command;

fn run(args: &[&str]) -> (bool, String) {
    let done = Command::new(env!("CARGO_BIN_EXE_jojobot-exercise"))
        .args(args)
        .output()
        .expect("the binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr),
    );
    (done.status.success(), said)
}

/// **A playbook nobody has written expectations for stops at the door.**
///
/// The case the tier's honesty rests on: a run that asserts nothing must not
/// happen at all, rather than happening and reporting a pass over an empty
/// list. It exits non-zero, and it says what is missing.
#[test]
fn a_playbook_with_no_expectations_refuses_before_anything_is_billed() {
    let playbook = std::env::temp_dir().join("jojobot-entrance-playbook.md");
    std::fs::write(
        &playbook,
        "## Phase 1 — a thing\n\n**Session: fresh.**\n\n> Say something.\n",
    )
    .expect("a playbook on disk");

    let (ok, said) = run(&["--playbook", playbook.to_str().expect("a path")]);
    assert!(!ok, "a run with nothing to assert reported success: {said}");
    assert!(
        said.contains("no expectations are registered"),
        "the refusal must say what is missing: {said}",
    );
    // The positive the refusal rests on: it stopped at the door rather than
    // after doing the expensive half. A room prints its server's log, so a
    // refusal that had built one would carry it.
    assert!(
        !said.contains("listening on"),
        "a room was built before the refusal: {said}",
    );
    let _ = std::fs::remove_file(&playbook);
}

/// An unknown argument is refused by name, before anything is billed.
///
/// **An omitted `--playbook` is not a case here.** It used to refuse the same
/// way; now it resolves to the rooms table's default room — decision log
/// 299 — and proving that would mean spawning this binary with no arguments
/// at all, which would run that room for real. `main.rs`'s own unit tests
/// prove the resolution without spawning anything, which is the only way to
/// prove it and keep this suite free.
#[test]
fn an_unknown_argument_is_refused_by_name() {
    let (ok, unknown) = run(&["--playbook", "/dev/null", "--nonsense", "x"]);
    assert!(!ok, "an unknown argument was accepted: {unknown}");
    assert!(
        unknown.contains("--nonsense"),
        "the refusal names what it did not understand: {unknown}",
    );
}

/// Every call site in THIS FILE that spawns the real binary, as `line, args`.
///
/// **Textual, on purpose.** Proving a call omits `--playbook` by running it
/// would mean spawning the binary with no room named — the exact thing being
/// guarded against. Reading the source is the only way to ask the question
/// without doing the thing it forbids.
fn calls_to_run(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(at, line)| {
            let start = line.find("run(&[")?;
            let end = line[start..].find("])")?;
            Some((at + 1, line[start..start + end + 2].to_string()))
        })
        .collect()
}

/// **Nothing in this file may spawn the binary with no room named.**
///
/// An omitted `--playbook` resolves to the rooms table's default room and runs
/// it for real (decision log 299) — that used to be a refusal and no longer
/// is, so nothing stops a future call site here from doing it by accident.
/// This scans the file's own source for every `run(&[...])` call and refuses
/// any that does not carry `--playbook`.
#[test]
fn nothing_here_calls_the_binary_with_no_playbook() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/entrance.rs");
    let text = std::fs::read_to_string(&here).expect("this file is readable");
    // Everything above the scanner itself: the guard's own source names the
    // pattern it looks for, which would otherwise match itself.
    let boundary = text
        .find("fn calls_to_run")
        .expect("this function stays in the file it scans");
    let calls = calls_to_run(&text[..boundary]);
    assert!(
        calls.len() >= 2,
        "found only {} call site(s) to run(), so this scan is reading almost nothing",
        calls.len(),
    );
    let unguarded: Vec<String> = calls
        .iter()
        .filter(|(_, args)| !args.contains("--playbook"))
        .map(|(line, args)| format!("line {line}: {args}"))
        .collect();
    assert!(
        unguarded.is_empty(),
        "a call to run() with no --playbook would spawn the real binary against the default \
         room, for real:\n{}",
        unguarded.join("\n"),
    );
}
