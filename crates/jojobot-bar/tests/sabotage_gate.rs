//! A sabotage a crash left applied stops the bar of its own checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod support;

/// A checkout with its own copy of the tool, which names the checkout it lives
/// in. Every line shares the state directory `root/state`.
struct Line(PathBuf);

impl Line {
    fn new(tag: &str, root: &Path) -> Self {
        let dir = root.join(tag);
        fs::create_dir_all(dir.join("scripts")).unwrap();
        let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sabotage");
        fs::copy(tool, dir.join("scripts/sabotage")).unwrap();
        fs::write(dir.join("t.txt"), "before\n").unwrap();
        Self(dir)
    }

    fn run(&self, program: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(program);
        let state = self.0.parent().unwrap().join("state");
        cmd.current_dir(&self.0).env("SABOTAGE_STATE_DIR", state);
        cmd.args(args).stdin(Stdio::null());
        cmd
    }

    fn sabotage(&self, then: &[&str]) -> Command {
        let head = ["scripts/sabotage", "t.txt", "before", "after", "--"];
        self.run("python3", &[&head[..], then].concat())
    }

    /// Whether the bar got past its gate to its usage line, and what it said.
    fn bar(&self) -> (bool, String) {
        let out = support::output_fenced(&mut self.run(env!("CARGO_BIN_EXE_jojobot-bar"), &[]));
        let out = out.unwrap();
        let said = [out.stdout, out.stderr].map(|s| String::from_utf8_lossy(&s).into_owned());
        let said = said.concat();
        (said.contains("usage: bar"), said)
    }

    fn text(&self) -> String {
        fs::read_to_string(self.0.join("t.txt")).unwrap()
    }
}

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gate-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("state")).unwrap();
    dir
}

/// The tool, hard-killed after it mutated the file: a real crash's record.
fn crash(line: &Line) {
    support::output_fenced(&mut line.sabotage(&["sh", "-c", "kill -KILL $PPID"])).unwrap();
    assert_eq!(line.text(), "after\n");
}

#[test]
fn a_leftover_sabotage_stops_the_bar_and_names_how_to_put_it_back() {
    let line = Line::new("mine", &root("left"));
    crash(&line);
    let (ran, said) = line.bar();
    assert!(!ran && said.contains("\"before\""), "{said}");
    let restore = said.lines().find(|l| l.starts_with("cp ")).expect(&said);
    support::output_fenced(&mut line.run("sh", &["-c", restore])).unwrap();
    assert_eq!(line.text(), "before\n");
    assert!(line.bar().0, "the put-back did not clear the gate");
}

#[test]
fn a_sabotage_still_running_does_not_stop_the_bar() {
    let line = Line::new("mine", &root("live"));
    let mut running = support::fenced_spawn(&mut line.sabotage(&["sleep", "30"])).unwrap();
    while line.text() != "after\n" {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let (ran, said) = line.bar();
    running.kill().unwrap();
    running.wait().unwrap();
    assert!(ran, "a live sabotage stopped its own bar: {said}");
}

#[test]
fn another_checkouts_leftover_sabotage_does_not_stop_this_bar() {
    let root = root("other");
    crash(&Line::new("theirs", &root));
    let (ran, said) = Line::new("mine", &root).bar();
    assert!(ran, "another checkout's break stopped this bar: {said}");
}

#[test]
fn a_sabotage_record_the_gate_cannot_read_stops_the_bar() {
    let root = root("unreadable");
    fs::write(root.join("state/run-damaged"), "not a record\n").unwrap();
    let (ran, said) = Line::new("mine", &root).bar();
    assert!(
        !ran && said.contains("run-damaged"),
        "an unreadable record passed: {said}"
    );
}
