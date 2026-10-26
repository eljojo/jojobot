//! A sabotage a crash left applied stops the bar of its own checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
        let root = self.0.parent().unwrap();
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.0).args(args).stdin(Stdio::null());
        cmd.env("SABOTAGE_STATE_DIR", root.join("state"));
        cmd.env("BAR_HISTORY_DIR", root.join("history"));
        cmd
    }

    /// Whether the bar got past its gate to its usage line, and what it said.
    fn bar(&self) -> (bool, String) {
        let out = self.run(env!("CARGO_BIN_EXE_jojobot-bar"), &[]);
        let out = support::output_fenced(&mut { out }).unwrap();
        let said = [out.stdout, out.stderr].map(|s| String::from_utf8_lossy(&s).into_owned());
        (said[1].contains("usage: bar"), said.concat())
    }

    fn text(&self) -> String {
        fs::read_to_string(self.0.join("t.txt")).unwrap()
    }

    /// The tool, hard-killed after it mutated the file: a real crash's record,
    /// which is then rewritten field by field by `edit`.
    fn crash(&self, edit: impl FnOnce(&mut Vec<String>)) {
        let then = ["sh", "-c", "kill -KILL $PPID"];
        let head = ["scripts/sabotage", "t.txt", "before", "after", "--"];
        support::output_fenced(&mut self.run("python3", &[&head[..], &then].concat())).unwrap();
        assert_eq!(self.text(), "after\n");
        let state = self.0.parent().unwrap().join("state");
        let mine = |p: &PathBuf| {
            fs::read_to_string(p)
                .unwrap()
                .starts_with(self.0.to_str().unwrap())
        };
        let record = fs::read_dir(state)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(mine)
            .unwrap();
        let text = fs::read_to_string(&record).unwrap();
        let mut fields: Vec<String> = text.trim_end().split('\t').map(String::from).collect();
        edit(&mut fields);
        fs::write(record, fields.join("\t") + "\n").unwrap();
    }
}

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gate-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("state")).unwrap();
    dir
}

#[test]
fn a_leftover_sabotage_stops_the_bar_and_names_how_to_put_it_back() {
    let line = Line::new("mine", &root("left"));
    line.crash(|_| {});
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
    let head = [
        "scripts/sabotage",
        "t.txt",
        "before",
        "after",
        "--",
        "sleep",
        "30",
    ];
    let mut running = support::fenced_spawn(&mut line.run("python3", &head)).unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while line.text() != "after\n" {
        assert!(Instant::now() < until, "the file was never mutated");
        std::thread::sleep(Duration::from_millis(25));
    }
    let (ran, said) = line.bar();
    running.kill().unwrap();
    running.wait().unwrap();
    assert!(ran, "a live sabotage stopped its own bar: {said}");
}

#[test]
fn another_checkouts_leftover_sabotage_does_not_stop_this_bar() {
    let root = root("other");
    let theirs = Line::new("theirs", &root);
    theirs.crash(|_| {});
    // A torn row that still names its checkout is theirs too.
    fs::write(
        root.join("state/run-torn"),
        format!("{}\tnot a pid\n", theirs.0.display()),
    )
    .unwrap();
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
fn an_old_format_sabotage_record_stops_only_its_own_checkout() {
    let root = root("old");
    let (mine, theirs) = (Line::new("mine", &root), Line::new("theirs", &root));
    for line in [&mine, &theirs] {
        line.crash(|fields| fields.truncate(4));
    }
    let (ran, said) = mine.bar();
    assert!(
        !ran && said.contains("mine/t.txt"),
        "an old leftover was not named: {said}"
    );
    assert!(
        !said.contains("theirs"),
        "another checkout's old leftover was named: {said}"
    );
}

#[test]
fn a_process_that_reused_the_number_of_a_dead_sabotage_does_not_hide_its_leftover() {
    let line = Line::new("mine", &root("reuse"));
    // The clock ticks every 10 ms, so the later process has to start a tick away.
    let mut later = None;
    line.crash(|fields| {
        std::thread::sleep(Duration::from_millis(50));
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        fields[1] = child.id().to_string();
        later = Some(child);
    });
    let (ran, said) = line.bar();
    let mut later = later.unwrap();
    later.kill().unwrap();
    later.wait().unwrap();
    assert!(!ran, "a reused process number hid the leftover: {said}");
}
