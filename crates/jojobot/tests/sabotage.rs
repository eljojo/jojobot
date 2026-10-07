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

/// **`SIGKILL`'s own hole, and what closes it: a later invocation names what
/// a hard kill left behind.**
///
/// Every case below runs its own invocations under a private state
/// directory — the mechanism's own record is one shared file otherwise, and
/// tests run concurrently.
fn private_state(named: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sabotage-state-{named}"));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// [`sabotage`], with the tool pointed at a state directory this case owns.
fn sabotage_with_state(
    state: &std::path::Path,
    path: &PathBuf,
    old: &str,
    new: &str,
    command: &[&str],
) -> (String, Option<i32>) {
    let ran = Command::new(tool())
        .env("SABOTAGE_STATE_DIR", state)
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

/// 🚨 **The case the mechanism exists for: a hard kill, and a later
/// invocation that finds it.**
///
/// `SIGTERM` is caught above and the file comes back — this is the one
/// ending nothing can catch, so the file stays mutated on purpose, and what
/// is proven here is that the NEXT invocation says so, names the file, and
/// hands over the exact command that restores it — never that it restores
/// anything itself, which is excluded on purpose.
#[test]
fn a_hard_kill_leaves_the_next_invocation_naming_the_outstanding_mutation() {
    let state = private_state("hard-kill");
    let held = "the answer is 41\nand a second line\n";
    let path = a_file("hard-killed", held);

    let mut running = Command::new(tool())
        .env("SABOTAGE_STATE_DIR", &state)
        .arg(&path)
        .arg("41")
        .arg("42")
        .arg("--")
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .spawn()
        .expect("the tool starts");

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
        "the file was never sabotaged, so what follows proves nothing",
    );

    Command::new("kill")
        .arg("-KILL")
        .arg(running.id().to_string())
        .status()
        .expect("the case can send a hard kill");
    running.wait().expect("the tool ends");

    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "the answer is 42\nand a second line\n",
        "a hard kill is the one ending nothing can catch — the file staying mutated is the \
         hole this mechanism exists beside, not one it closes",
    );

    // A second, unrelated invocation — the report is not scoped to the same
    // file or command, because a hard kill's evidence has to surface on
    // whatever sabotage happens to run next.
    let other = a_file("hard-kill-bystander", "nothing to see here\n");
    let (said, code) = sabotage_with_state(&state, &other, "nothing", "something", &["true"]);
    assert_eq!(
        code,
        Some(0),
        "the bystander invocation itself is ordinary: {said}"
    );
    assert!(
        said.contains("SABOTAGE OUTSTANDING") && said.contains(path.to_str().unwrap()),
        "the next invocation does not name the file the hard kill left mutated: {said}",
    );
    assert!(
        said.contains("cp ") && said.contains(path.to_str().unwrap()),
        "the next invocation does not hand over the exact restore command: {said}",
    );

    // **No automatic restore, ever.** The report names the command; it does
    // not run it.
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "the answer is 42\nand a second line\n",
        "a later invocation restored the file itself, which is excluded — it only reports",
    );
}

/// **Paired with the case above, and it is the one that matters.** A
/// sabotage that restores cleanly — pass or fail, it does not matter which —
/// leaves nothing for the next invocation to report. A tool that always
/// warns would pass the positive above alone.
#[test]
fn a_sabotage_that_restores_cleanly_leaves_nothing_for_the_next_invocation_to_report() {
    let state = private_state("clean-restore");
    let held = "the answer is 41\n";

    let path = a_file("restores-clean-pass", held);
    sabotage_with_state(&state, &path, "41", "42", &["true"]);
    let path2 = a_file("restores-clean-fail", held);
    sabotage_with_state(&state, &path2, "41", "42", &["false"]);

    let bystander = a_file("clean-restore-bystander", "nothing to see here\n");
    let (said, code) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    assert_eq!(
        code,
        Some(0),
        "the bystander invocation itself is ordinary: {said}"
    );
    assert!(
        !said.contains("SABOTAGE OUTSTANDING"),
        "a run that restored itself, whether it passed or failed, left something for the \
         next invocation to report: {said}",
    );
}

/// **The other half of pairing it**: a sabotage refused before it ever
/// mutates leaves nothing behind either, because it never wrote a record to
/// begin with.
#[test]
fn a_sabotage_refused_before_it_mutates_leaves_nothing_for_the_next_invocation_to_report() {
    let state = private_state("refused-before-mutating");
    let path = a_file(
        "refused-ambiguous",
        "the answer is 41\nthe answer is 41 again\n",
    );
    let (refused, code) = sabotage_with_state(&state, &path, "41", "42", &["true"]);
    assert_eq!(
        code,
        Some(2),
        "the ambiguous site should have been refused: {refused}"
    );

    let bystander = a_file("refused-bystander", "nothing to see here\n");
    let (said, code) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    assert_eq!(
        code,
        Some(0),
        "the bystander invocation itself is ordinary: {said}"
    );
    assert!(
        !said.contains("SABOTAGE OUTSTANDING"),
        "a sabotage refused before it mutated anything still left a record: {said}",
    );
}

/// **A stale entry whose file already matches its kept copy must not cry
/// wolf.** Somebody put it back by hand — the same bytes a `cp` from the
/// kept copy would have produced — and a later invocation that still warned
/// about it would be a tool worth learning to ignore.
#[test]
fn an_outstanding_mutation_restored_by_hand_is_not_reported_again() {
    let state = private_state("restored-by-hand");
    let held = "the answer is 41\nand a second line\n";
    let path = a_file("hand-restored", held);

    let mut running = Command::new(tool())
        .env("SABOTAGE_STATE_DIR", &state)
        .arg(&path)
        .arg("41")
        .arg("42")
        .arg("--")
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .spawn()
        .expect("the tool starts");

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
        "the file was never sabotaged, so the restore below proves nothing"
    );

    Command::new("kill")
        .arg("-KILL")
        .arg(running.id().to_string())
        .status()
        .expect("the case can send a hard kill");
    running.wait().expect("the tool ends");

    // The operator's own repair: put the original text back, exactly as the
    // named restore command would have.
    fs::write(&path, held).expect("the case can restore the file by hand");

    let bystander = a_file("hand-restored-bystander", "nothing to see here\n");
    let (said, code) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    assert_eq!(
        code,
        Some(0),
        "the bystander invocation itself is ordinary: {said}"
    );
    assert!(
        !said.contains("SABOTAGE OUTSTANDING"),
        "a mutation the operator already restored by hand was reported anyway: {said}",
    );
}

/// What `cargo test -q` printed for a filter that matched no test, kept as it
/// arrived: every target reports `0 passed`, and the exit code is zero.
const NO_TEST_MATCHED: &str = "\nrunning 0 tests\n\n\
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\n\
running 0 tests\n\n\
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.00s\n\n";

/// What it printed for a filter that matched one test in the second of two
/// targets: the first target ran nothing and the run still ran a test.
const ONE_TEST_MATCHED: &str = "\nrunning 0 tests\n\n\
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\n\
running 1 test\n.\n\
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.00s\n\n";

/// Run the tool over a file with a command that prints `output` and exits 0,
/// the way `cargo test` does over a filter that matches nothing. The word
/// `filter` rides on the command line, where a real filter would.
fn sabotage_printing(named: &str, filter: &str, output: &str) -> (PathBuf, String, Option<i32>) {
    let held = "the answer is 41\nand a second line\n";
    let path = a_file(named, held);
    let printed = a_file(&format!("{named}-output"), output);
    let printed = printed.to_string_lossy().into_owned();
    let (said, code) = sabotage(
        &path,
        "41",
        "42",
        &["sh", "-c", "cat \"$1\"", filter, &printed],
    );
    (path, said, code)
}

/// 🚨 **A run that executed no test has no verdict to give.**
///
/// A filter that matches nothing makes the runner exit zero, so the run came
/// back GREEN and a sabotage that measured nothing read as a case that does
/// not depend on the site. The refusal names the command, which carries the
/// filter, and it comes before any verdict. **Paired with the run that ran one
/// test among targets that ran none**, which still gets its verdict, so the
/// refusal is about the count and not about a zero in one target.
#[test]
fn a_run_where_no_test_ran_is_refused_and_a_run_where_one_ran_gets_its_verdict() {
    let held = "the answer is 41\nand a second line\n";

    let (path, said, code) =
        sabotage_printing("empty-run", "no_such_case_anywhere", NO_TEST_MATCHED);
    assert_eq!(code, Some(2), "an empty run ends as a refusal: {said}");
    assert!(
        said.contains("SABOTAGE REFUSED"),
        "the refusal is said on stdout: {said}"
    );
    assert!(
        said.contains("no_such_case_anywhere"),
        "the refusal names the command, which carries the filter: {said}",
    );
    assert!(
        !said.contains("VERDICT"),
        "no verdict is given for a run that tested nothing: {said}",
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "the file did not come back after a refused empty run",
    );

    let (path, said, code) = sabotage_printing("one-run", "a_case_that_exists", ONE_TEST_MATCHED);
    assert_eq!(
        code,
        Some(0),
        "a run that tested something ends as it did: {said}"
    );
    assert!(
        said.contains("VERDICT GREEN"),
        "a run that executed a test gets its verdict: {said}"
    );
    assert!(
        !said.contains("REFUSED"),
        "a run that executed a test is not refused: {said}"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "the file did not come back after a run that tested something",
    );
}

/// 🚨 **The probe run is held to the same count.**
///
/// A probe over a command that ran nothing stays green under its panic, so it
/// called the site BLIND when nothing had been measured. Here the first run
/// executes a test and the second, the probe, executes none: the probe says
/// so and does not call the site blind.
#[test]
fn a_probe_run_where_no_test_ran_is_not_called_blind() {
    let held = "the answer is 41\nand a second line\n";
    let path = a_file("probe-empty", held);
    let one = a_file("probe-empty-one", ONE_TEST_MATCHED);
    let none = a_file("probe-empty-none", NO_TEST_MATCHED);
    let marker = std::env::temp_dir().join("sabotage-case-probe-empty-marker");
    let _ = fs::remove_file(&marker);
    let script = "if [ -e \"$1\" ]; then cat \"$3\"; else touch \"$1\"; cat \"$2\"; fi";
    let (said, code) = sabotage(
        &path,
        "41",
        "42",
        &[
            "sh",
            "-c",
            script,
            "sh",
            &marker.to_string_lossy(),
            &one.to_string_lossy(),
            &none.to_string_lossy(),
        ],
    );
    let _ = fs::remove_file(&marker);
    assert_eq!(code, Some(0), "the first run tested something: {said}");
    assert!(
        said.contains("VERDICT GREEN"),
        "the first run keeps its verdict: {said}"
    );
    assert!(
        said.contains("PROBE EMPTY"),
        "the probe says that its run executed no test: {said}"
    );
    assert!(
        !said.contains("BLIND"),
        "an empty probe run is not a blind site: {said}"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        held,
        "the file did not come back after the probe",
    );
}

/// **A checkout of its own, holding a copy of the tool.** The tool names the
/// checkout it lives in, so two copies in two directories are two lines.
fn a_checkout_with_the_tool(named: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sabotage-checkout-{named}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("scripts")).expect("the case makes a checkout");
    fs::copy(tool(), root.join("scripts/sabotage")).expect("the case copies the tool");
    root
}

/// Start the tool at `at` over `path` with a command that never ends on its
/// own, and wait until the file is really mutated.
fn start_and_wait_for_the_mutation(
    at: &std::path::Path,
    state: &std::path::Path,
    path: &PathBuf,
) -> std::process::Child {
    let mut running = Command::new(at)
        .env("SABOTAGE_STATE_DIR", state)
        .arg(path)
        .arg("41")
        .arg("42")
        .arg("--")
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .spawn()
        .expect("the tool starts");
    let mut mutated = false;
    for _ in 0..400 {
        if fs::read_to_string(path).is_ok_and(|now| now.contains("42")) {
            mutated = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    if !mutated {
        let _ = running.kill();
        let _ = running.wait();
        panic!("the file was never sabotaged, so what follows proves nothing");
    }
    running
}

/// 🚨 **Runs that start together each leave their own entry.**
///
/// The record was one file every run read and rewrote whole, so two runs
/// starting at once could each read the old list and the second write dropped
/// the first run's entry. A hard kill of the dropped run was then never
/// reported. Here eight runs start at once, every one is killed outright, and
/// the next invocation has to name all eight files.
#[test]
fn runs_that_start_together_all_leave_an_entry_a_later_run_reports() {
    let state = private_state("together");
    let paths: Vec<PathBuf> = (0..8)
        .map(|n| a_file(&format!("together-{n}"), "the answer is 41\n"))
        .collect();
    let mut running: Vec<std::process::Child> = paths
        .iter()
        .map(|path| {
            Command::new(tool())
                .env("SABOTAGE_STATE_DIR", &state)
                .arg(path)
                .arg("41")
                .arg("42")
                .arg("--")
                .args(["sleep", "30"])
                .stdout(Stdio::null())
                .spawn()
                .expect("the tool starts")
        })
        .collect();
    for path in &paths {
        let mut mutated = false;
        for _ in 0..400 {
            if fs::read_to_string(path).is_ok_and(|now| now.contains("42")) {
                mutated = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(mutated, "{path:?} was never sabotaged");
    }
    for child in &mut running {
        Command::new("kill")
            .arg("-KILL")
            .arg(child.id().to_string())
            .status()
            .expect("the case can send a hard kill");
        child.wait().expect("the tool ends");
    }

    let bystander = a_file("together-bystander", "nothing to see here\n");
    let (said, _) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    for path in &paths {
        assert!(
            said.contains(path.to_str().unwrap()),
            "a run's entry was lost, so its hard kill is never reported: {path:?} in {said}",
        );
    }
}

/// 🚨 **The startup report names only the entries of its own checkout.**
///
/// Every line shares one state directory, so the report listed other lines'
/// mutations as if they were the caller's. A hard-killed run in one checkout is
/// reported by the next run in that checkout and by none in another, so each
/// line sees what is its own to put back.
#[test]
fn a_hard_kill_is_reported_in_its_own_checkout_and_in_no_other() {
    let state = private_state("checkouts");
    let mine = a_checkout_with_the_tool("mine");
    let theirs = a_checkout_with_the_tool("theirs");
    let path = a_file("checkouts-killed", "the answer is 41\n");

    let mut running =
        start_and_wait_for_the_mutation(&mine.join("scripts/sabotage"), &state, &path);
    Command::new("kill")
        .arg("-KILL")
        .arg(running.id().to_string())
        .status()
        .expect("the case can send a hard kill");
    running.wait().expect("the tool ends");

    let run_in = |checkout: &std::path::Path, named: &str| {
        let bystander = a_file(named, "nothing to see here\n");
        let ran = Command::new(checkout.join("scripts/sabotage"))
            .env("SABOTAGE_STATE_DIR", &state)
            .arg(&bystander)
            .arg("nothing")
            .arg("something")
            .arg("--")
            .arg("true")
            .output()
            .expect("the tool runs");
        String::from_utf8_lossy(&ran.stdout).into_owned()
    };
    let other = run_in(&theirs, "checkouts-other-line");
    assert!(
        !other.contains("SABOTAGE OUTSTANDING"),
        "another checkout's mutation was reported as this one's: {other}",
    );
    let own = run_in(&mine, "checkouts-own-line");
    assert!(
        own.contains("SABOTAGE OUTSTANDING") && own.contains(path.to_str().unwrap()),
        "the checkout that was killed in does not find its own entry: {own}",
    );
}

/// 🚨 **A run that is still going is not outstanding.**
///
/// A record of a live run names a file that is mutated on purpose. Another
/// invocation in the same checkout read it as left behind: it reported the live
/// mutation as a hard kill's, and an invocation that started before the first
/// run had mutated its file read the record as a put-back and removed it, so a
/// kill that came later was never reported. Only a run whose process is gone
/// is outstanding.
#[test]
fn a_run_that_is_still_going_is_not_reported_and_keeps_its_entry() {
    let state = private_state("live");
    let path = a_file("live-run", "the answer is 41\n");
    let mut running = start_and_wait_for_the_mutation(&tool(), &state, &path);

    let bystander = a_file("live-bystander", "nothing to see here\n");
    let (said, _) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    assert!(
        !said.contains("SABOTAGE OUTSTANDING"),
        "a mutation a live run holds on purpose was reported as left behind: {said}",
    );

    Command::new("kill")
        .arg("-KILL")
        .arg(running.id().to_string())
        .status()
        .expect("the case can send a hard kill");
    running.wait().expect("the tool ends");
    let (said, _) = sabotage_with_state(&state, &bystander, "nothing", "something", &["true"]);
    assert!(
        said.contains("SABOTAGE OUTSTANDING") && said.contains(path.to_str().unwrap()),
        "the entry the bystander passed over was lost, so the kill is not reported: {said}",
    );
}
