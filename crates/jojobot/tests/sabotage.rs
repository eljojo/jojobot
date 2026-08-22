//! **The sabotage tool, held to the thing it exists for: it cannot leave a
//! dirty tree.**
//!
//! Watching a test fail means breaking the code it covers. Done in two steps —
//! mutate, then restore — a decision sits between them, and that decision is
//! where a closed git verb becomes the convenient answer. The tool makes it one
//! act.
//!
//! **The case this exists for is the run that does NOT end cleanly**, because a
//! version that only restores on the happy path is the same reflex with better
//! manners. Every case here reads the file afterwards rather than the tool's
//! own account of itself.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The tool, resolved from this crate rather than from wherever a caller
/// happened to stand.
fn tool() -> PathBuf {
    PathBuf::new()
        .join(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/sabotage")
}

/// A file with one line worth sabotaging, and one that only looks like it.
fn a_file(named: &str, holding: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("sabotage-case-{named}"));
    fs::write(&path, holding).expect("the case writes its own file");
    path
}

/// Run the tool over a file and hand back what it printed and how it ended.
fn sabotage(path: &PathBuf, old: &str, new: &str, command: &[&str]) -> (String, Option<i32>) {
    let ran = Command::new(tool())
        .arg(path)
        .arg(old)
        .arg(new)
        .arg("--")
        .args(command)
        .output()
        .expect("the tool runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    (said, ran.status.code())
}

/// **An ordinary run: the verdict is the real one and the file comes back.**
///
/// Both endings, because a tool that restored only after a pass would satisfy
/// half of this and be the reflex it replaces.
#[test]
fn a_run_that_passes_and_a_run_that_fails_both_leave_the_file_as_it_was() {
    let held = "the answer is 41\nand a second line\n";

    let path = a_file("green", held);
    let (said, code) = sabotage(&path, "41", "42", &["true"]);
    assert_eq!(
        code,
        Some(0),
        "a passing command ends the tool cleanly: {said}"
    );
    assert!(
        said.contains("VERDICT GREEN"),
        "the verdict is the real one: {said}"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "the file did not come back after a run that passed",
    );

    let path = a_file("red", held);
    let (said, code) = sabotage(&path, "41", "42", &["false"]);
    assert_eq!(
        code,
        Some(1),
        "the tool ends as the command it ran did: {said}"
    );
    assert!(
        said.contains("VERDICT RED"),
        "the verdict is the real one: {said}"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "the file did not come back after a run that failed",
    );
}

/// 🚨 **The case the tool exists for: it is asked to stop mid-flight.**
///
/// A run that is interrupted is the moment the tree used to be left dirty and
/// somebody had to decide how to clean it. **The file has to come back without
/// anybody deciding anything.**
///
/// **The positive it rests on, in the same case**: the file really was mutated
/// while the run was in flight. Without that this passes on a tool that never
/// wrote anything at all.
#[test]
fn a_run_asked_to_stop_mid_flight_still_puts_the_file_back() {
    let held = "the answer is 41\nand a second line\n";
    let path = a_file("stopped", held);

    let mut running = Command::new(tool())
        .arg(&path)
        .arg("41")
        .arg("42")
        .arg("--")
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .spawn()
        .expect("the tool starts");

    // Wait until the mutation is really on disk, so the assertion below is
    // about restoring rather than about a race.
    let mut mutated = false;
    for _ in 0..200 {
        if fs::read_to_string(&path).is_ok_and(|now| now.contains("42")) {
            mutated = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(
        mutated,
        "the file was never sabotaged, so a clean tree afterwards says nothing",
    );

    // **Asked to stop, not killed outright.** SIGKILL cannot be caught by this
    // or by anything, which the tool says of itself; what a mechanism can
    // promise is every ending that is catchable.
    Command::new("kill")
        .arg("-TERM")
        .arg(running.id().to_string())
        .status()
        .expect("the case can ask the tool to stop");
    running.wait().expect("the tool ends");

    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "a run asked to stop left the file sabotaged, which is the dirty tree this exists to \
         make impossible",
    );
}

/// **A site that is not unique is refused, and nothing runs.**
///
/// Asserting that a sabotage applied is not enough when a file has more than
/// one site that looks like the target: a patch landing elsewhere changes
/// something, passes a did-anything-move check, and reports a verdict about
/// code nobody touched.
///
/// **Both halves**: the ambiguous site is refused and the unique one is not.
#[test]
fn a_site_that_appears_twice_is_refused_and_a_unique_one_is_not() {
    let twice = "the answer is 41\nthe answer is 41 again\n";
    let path = a_file("ambiguous", twice);
    let (said, code) = sabotage(&path, "41", "42", &["false"]);
    assert_eq!(
        code,
        Some(2),
        "an ambiguous site is refused rather than run: {said}"
    );
    assert!(
        said.contains("appears 2 times"),
        "the refusal does not say how many sites it found: {said}",
    );
    assert!(
        !said.contains("VERDICT"),
        "a refused sabotage reported a verdict about a run it never made: {said}",
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        twice,
        "a refused sabotage wrote to the file anyway",
    );

    let once = "the answer is 41\nand a second line\n";
    let path = a_file("unique", once);
    let (said, code) = sabotage(&path, "41", "42", &["true"]);
    assert_eq!(code, Some(0), "a unique site was refused as well: {said}");
    assert!(
        said.contains("APPLIED"),
        "a unique site was not sabotaged: {said}"
    );
}

/// **A site that is not there at all is refused too**, and says so as an
/// absence rather than as an ambiguity — the two send an author to different
/// places.
#[test]
fn a_site_that_is_not_there_is_refused_as_an_absence() {
    let held = "the answer is 41\n";
    let path = a_file("missing", held);
    let (said, code) = sabotage(&path, "nothing like this", "42", &["true"]);
    assert_eq!(code, Some(2), "a missing site was run anyway: {said}");
    assert!(
        said.contains("appears 0 times"),
        "the refusal does not tell an absence from an ambiguity: {said}",
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "a refused sabotage wrote to the file anyway",
    );
}
