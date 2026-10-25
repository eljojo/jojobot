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
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while line.text() != "after\n" {
        assert!(
            std::time::Instant::now() < until,
            "the file was never mutated"
        );
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

#[test]
fn a_process_that_reused_the_number_of_a_dead_sabotage_does_not_hide_its_leftover() {
    let root = root("reuse");
    let line = Line::new("mine", &root);
    crash(&line);
    // Hand the dead run's number to a process that started later. The clock
    // ticks every 10 ms, so the later start has to be a tick away.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut later = Command::new("sleep").arg("30").spawn().unwrap();
    let record = fs::read_dir(root.join("state"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(&record).unwrap();
    let mut fields: Vec<&str> = text.trim_end().split('\t').collect();
    let number = later.id().to_string();
    fields[1] = &number;
    fs::write(&record, fields.join("\t")).unwrap();
    let (ran, said) = line.bar();
    later.kill().unwrap();
    later.wait().unwrap();
    assert!(!ran, "a reused process number hid the leftover: {said}");
}

#[test]
fn a_sabotage_record_of_another_checkout_stops_no_bar_whatever_its_form() {
    let root = root("legacy");
    let elsewhere = root.join("elsewhere").display().to_string();
    let state = root.join("state");
    // The older form had four fields; the second row is torn after its checkout.
    fs::write(
        state.join("run-old"),
        format!("{elsewhere}\t1\tt.txt\tkept\n"),
    )
    .unwrap();
    fs::write(state.join("run-torn"), format!("{elsewhere}\tnot a pid\n")).unwrap();
    let line = Line::new("mine", &root);
    let (ran, said) = line.bar();
    assert!(
        ran,
        "another checkout's old or torn record stopped this bar: {said}"
    );
    // The same older form, left by a dead run of this checkout, still gates.
    let mut dead = Command::new("true").spawn().unwrap();
    dead.wait().unwrap();
    let (target, kept) = (line.0.join("t.txt"), root.join("kept.txt"));
    fs::write(&kept, "before\n").unwrap();
    fs::write(&target, "after\n").unwrap();
    let old = format!(
        "{}\t{}\t{}\t{}\n",
        line.0.display(),
        dead.id(),
        target.display(),
        kept.display()
    );
    fs::write(state.join("run-mine-old"), old).unwrap();
    let (ran, said) = line.bar();
    assert!(
        !ran && said.contains("cp "),
        "an older leftover of this checkout was not named: {said}"
    );
}
