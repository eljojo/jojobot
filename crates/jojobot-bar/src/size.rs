//! **The size check: every tracked text file is measured, no new file is over the
//! ceiling, a red or yellow file that grew is counted, and the net lines of the
//! change are shown by area.** Rules 273, 597, 661, 704, 741 and the series rules
//! of 9.3.1-1.
//!
//! **What the limits are for.** They keep AI context and token use down and make
//! growth mindful (rule 740). They serve the app: a limit never forces a worse
//! design, so a change that honestly needs lines keeps them and gives the reason
//! (rule 741).
//!
//! - **Red** is a file over `cap` lines (5,000), of any kind. A new red file is
//!   refused. A red file that grew against the base is not refused: the check
//!   names it with its growth, and the report of the change gives the reason
//!   (rule 741).
//! - **Yellow** is a file over [`YELLOW`] lines and not over [`ORANGE`] (2,001 to
//!   3,000, subject to the cap). It may grow. The check warns about each that is new
//!   or grew. **Orange** is a file over [`ORANGE`] lines and not red: the check warns
//!   about each, and it turns red when the ceiling drops to 3,000.
//! - **The base** is where the change branched from `SIZE_BASE`, else
//!   `GITHUB_BASE_REF`. Growth is read from git alone, so there is no list of
//!   frozen counts to keep. **An unknown base is never ok:** growth cannot be
//!   judged, so the phase fails and says why. A full dispatch whose legacy
//!   default ref is absent keeps a note instead.
//! - **The table** is the net lines of the change, by area and by kind, as git
//!   counts them with renames off, so a move shows in both areas.
//!
//! **Incomplete measurement is a failure.** A file the bar cannot read, or a tree
//! git cannot list, fails the phase: it is never read as no growth.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use crate::Summary;

mod markers;

/// The checked-in file, at the repo root. It holds the cap, the byte limit of an
/// added test file and the step.
pub const RATCHET_FILE: &str = "size-ratchet.toml";

/// A file over this many lines is red.
pub const CEILING: usize = 5000;

/// A file over this many lines, and not over [`ORANGE`] or red, is yellow.
pub const YELLOW: usize = 2000;

/// A file over this many lines, and not red, is orange: it turns red when the
/// ceiling drops to this number.
pub const ORANGE: usize = 3000;

/// The base of the change, as a revision. It wins over [`BASE_REF_VARIABLE`]:
/// a workflow sets it to an exact commit, while the other names a branch that a
/// checkout may not hold.
pub const BASE_VARIABLE: &str = "SIZE_BASE";

/// The branch a pull request targets, as a runner sets it.
pub const BASE_REF_VARIABLE: &str = "GITHUB_BASE_REF";

/// The byte limit of an added test file when the file names none (rule 661).
const TEST_FILE_BYTES: usize = 100_000;

/// What `size-ratchet.toml` holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// A file over this many lines is red. It only falls.
    pub cap: usize,
    /// Byte limit for added test paths, never higher than rule 661's 100 KB.
    pub test_file_bytes: usize,
    /// The step of the series that a change is made in, such as `9.3.1`.
    pub step: Option<String>,
}

impl Limits {
    pub fn empty() -> Limits {
        Limits {
            cap: CEILING,
            test_file_bytes: TEST_FILE_BYTES,
            step: None,
        }
    }

    /// **Read the file's text.** `cap = N`, `test_file_bytes = N` and `step = "X"`.
    /// Every refusal names the line it is about.
    pub fn parse(text: &str) -> Result<Limits, String> {
        Self::read(text, false)
    }

    /// The same read for the file as it stood at the base: the `[[file]]` entries of
    /// the list this replaces and the `[[raise]]` entries of rule 704 are skipped,
    /// not refused. Only a file with those legacy tables has its old yellow cap
    /// read as the ceiling; a current-format cap keeps its value.
    fn parse_base(text: &str) -> Result<Limits, String> {
        Self::read(text, true)
    }

    fn read(text: &str, lenient: bool) -> Result<Limits, String> {
        let mut cap = None;
        let mut step = None;
        let mut test_file_bytes = None;
        let mut skipping = false;
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if (line == "[[file]]" || line == "[[raise]]") && lenient {
                skipping = true;
                continue;
            }
            if line.starts_with('[') {
                return Err(format!("line {number}: unknown table {line}"));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {number}: not a key = value line: {line}"));
            };
            let (key, value) = (key.trim(), value.trim());
            if skipping {
                continue;
            }
            match key {
                "cap" => {
                    if cap.is_some() {
                        return Err(format!("line {number}: cap is given twice"));
                    }
                    cap = Some(whole(key, value, number)?);
                }
                "step" => {
                    if step.is_some() {
                        return Err(format!("line {number}: step is given twice"));
                    }
                    let quoted = value
                        .strip_prefix('"')
                        .and_then(|rest| rest.strip_suffix('"'))
                        .filter(|inner| markers::steps(inner).is_some());
                    step = Some(
                        quoted
                            .ok_or_else(|| {
                                format!(
                                    "line {number}: step must be dotted numbers in double quotes, as 9.3.1"
                                )
                            })?
                            .to_string(),
                    );
                }
                "test_file_bytes" => {
                    if test_file_bytes.is_some() {
                        return Err(format!("line {number}: test_file_bytes is given twice"));
                    }
                    test_file_bytes = Some(whole(key, value, number)?);
                }
                other => return Err(format!("line {number}: unknown key {other}")),
            }
        }
        if test_file_bytes.is_some_and(|limit| limit > TEST_FILE_BYTES) {
            return Err("test_file_bytes may not exceed 100000".into());
        }
        let cap = match cap {
            Some(cap) if skipping && cap < CEILING => CEILING,
            Some(cap) if cap < YELLOW => {
                return Err(format!("cap may not be under {YELLOW}, the yellow line"));
            }
            Some(cap) => cap,
            None => CEILING,
        };
        Ok(Limits {
            cap,
            test_file_bytes: test_file_bytes.unwrap_or(TEST_FILE_BYTES),
            step,
        })
    }
}

fn whole(key: &str, value: &str, line: usize) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("line {line}: {key} must be a whole number, not {value}"))
}

/// A tracked text file and how many lines it holds now, counted as
/// `str::lines` counts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub path: String,
    pub lines: usize,
}

/// What a file is made of, for the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Source,
    Tests,
    Text,
}

/// **Source, tests or text.** Code in a `tests` or `testing` directory, or in a
/// file named for tests, is tests; other code is source; the rest is text.
pub fn kind_of(path: &str) -> Kind {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    let code = matches!(extension, "rs" | "py" | "nix" | "sh")
        || (extension.is_empty() && path.starts_with("scripts/"));
    if !code {
        return Kind::Text;
    }
    let in_tests = path
        .split('/')
        .any(|part| part == "tests" || part == "testing")
        || name.ends_with("tests.rs");
    if in_tests { Kind::Tests } else { Kind::Source }
}

/// **The area of a path**: a crate, with the top-level module under its `src`
/// when the file is in one; any other top-level directory by its name; and a file
/// at the root as `(root)`.
pub fn area_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["crates", name, "src", module, _, ..] => format!("crates/{name}/{module}"),
        ["crates", name, "src", file] => {
            format!("crates/{name}/{}", file.trim_end_matches(".rs"))
        }
        ["crates", name, ..] => format!("crates/{name}"),
        [only] => {
            let _ = only;
            "(root)".to_string()
        }
        [first, ..] => (*first).to_string(),
        [] => "(root)".to_string(),
    }
}

/// **How one file differs from the base.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub added: usize,
    pub removed: usize,
    /// The file is new in this change; a rename is not new.
    pub new: bool,
}

impl Change {
    fn net(&self) -> i64 {
        self.added as i64 - self.removed as i64
    }
}

/// **What the change looks like against its base**, by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// With renames followed: a moved file is the same file, so its growth is its own.
    pub followed: HashMap<String, Change>,
    /// With renames off: a move is a deletion in one area and an addition in another.
    pub flat: HashMap<String, Change>,
}

/// **What the base half compared with**, or why it could not.
pub enum Base {
    Unknown(String),
    Known {
        rev: String,
        limits: Limits,
        changes: Changes,
    },
}

/// **Why a check refuses.** Each says what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offence {
    TestFile {
        path: String,
        bytes: u64,
        limit: usize,
    },
    TestLimitRaised {
        base: usize,
        now: usize,
    },
    /// The cap is higher than at the base.
    CapRaised {
        base: usize,
        now: usize,
    },
    /// A file that is new in this change is red.
    New {
        path: String,
        now: usize,
    },
    /// An owned marker names no owner, or a step that is past.
    Marker(String),
}

impl Offence {
    /// One line, naming the file, the before and after, and what to do.
    pub fn line(&self) -> String {
        match self {
            Offence::TestFile { path, bytes, limit } => format!(
                "{path}: new test file holds {bytes} bytes, over {limit}. Generate fixtures outside git."
            ),
            Offence::TestLimitRaised { base, now } => format!(
                "{RATCHET_FILE}: test_file_bytes went from {base} to {now}. The limit only falls."
            ),
            Offence::CapRaised { base, now } => {
                format!("{RATCHET_FILE}: the cap went from {base} to {now}. The cap only falls.")
            }
            Offence::New { path, now } => format!(
                "{path}: new file over the cap, {now} lines. Make it smaller; do not move code \
                 to another file to keep the count."
            ),
            Offence::Marker(line) => line.clone(),
        }
    }
}

/// **The verdict of the change**: what is refused, what only warns, and the table.
#[derive(Debug, Default)]
pub struct Verdict {
    pub offences: Vec<Offence>,
    pub warnings: Vec<String>,
    pub red: usize,
    pub orange: usize,
    pub yellow: usize,
}

/// The lines a file held at the base, when it existed there.
fn lines_at_base(change: Option<&Change>, now: usize) -> Option<usize> {
    match change {
        None => Some(now),
        Some(change) if change.new => None,
        Some(change) => Some((now as i64 - change.net()).max(0) as usize),
    }
}

/// **Judge the tree against the base.** A red file new in the change is refused. A
/// red file longer than it was is named with its growth and not refused (rule 741).
pub fn judge(limits: &Limits, files: &[Measured], base: &Base) -> Verdict {
    let mut verdict = Verdict::default();
    let Base::Known {
        limits: was,
        changes,
        ..
    } = base
    else {
        return verdict;
    };
    if limits.test_file_bytes > was.test_file_bytes {
        verdict.offences.push(Offence::TestLimitRaised {
            base: was.test_file_bytes,
            now: limits.test_file_bytes,
        });
    }
    if limits.cap > was.cap {
        verdict.offences.push(Offence::CapRaised {
            base: was.cap,
            now: limits.cap,
        });
    }
    let step = |limits: &Limits| limits.step.as_deref().and_then(markers::steps);
    match (step(was), step(limits)) {
        (Some(then), Some(now)) if now < then => verdict.offences.push(Offence::Marker(format!(
            "{RATCHET_FILE}: step went back from {then:?} to {now:?}. The step only moves forward."
        ))),
        (Some(then), None) => verdict.offences.push(Offence::Marker(format!(
            "{RATCHET_FILE}: the step {then:?} at the base is missing. The step only moves forward."
        ))),
        _ => {}
    }
    for file in files.iter().filter(|file| file.lines > YELLOW) {
        let before = lines_at_base(changes.followed.get(&file.path), file.lines);
        if file.lines > limits.cap {
            verdict.red += 1;
            match before {
                None => verdict.offences.push(Offence::New {
                    path: file.path.clone(),
                    now: file.lines,
                }),
                Some(before) if file.lines > before => verdict.warnings.push(format!(
                    "{}: red file grew from {before} to {} lines, {} more. Name the reason in the \
                     report.",
                    file.path,
                    file.lines,
                    file.lines - before
                )),
                Some(_) => {}
            }
        } else {
            if file.lines > ORANGE {
                verdict.orange += 1;
                verdict.warnings.push(format!(
                    "{}: over {ORANGE} lines (orange), {}. It turns red when the ceiling \
                     drops to {ORANGE}.",
                    file.path, file.lines
                ));
            } else {
                verdict.yellow += 1;
            }
            match before {
                None => verdict.warnings.push(format!(
                    "{}: new file over {YELLOW} lines, {}",
                    file.path, file.lines
                )),
                Some(before) if file.lines > before => verdict.warnings.push(format!(
                    "{}: over {YELLOW} lines and grew from {before} to {}. Look for a deletion \
                     first, and name the reason in the report when there is none.",
                    file.path, file.lines
                )),
                Some(_) => {}
            }
        }
    }
    verdict
}

/// One row of the table: net lines by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetRow {
    pub area: String,
    pub source: i64,
    pub tests: i64,
    pub text: i64,
}

impl NetRow {
    pub fn code(&self) -> i64 {
        self.source + self.tests
    }
}

/// **Net lines of the change by area**, most negative code first.
pub fn net_by_area(changes: &Changes) -> Vec<NetRow> {
    let mut rows: BTreeMap<String, NetRow> = BTreeMap::new();
    for (path, change) in &changes.flat {
        let row = rows.entry(area_of(path)).or_insert_with(|| NetRow {
            area: area_of(path),
            ..NetRow::default()
        });
        match kind_of(path) {
            Kind::Source => row.source += change.net(),
            Kind::Tests => row.tests += change.net(),
            Kind::Text => row.text += change.net(),
        }
    }
    let mut rows: Vec<NetRow> = rows.into_values().collect();
    rows.sort_by(|a, b| a.code().cmp(&b.code()).then(a.area.cmp(&b.area)));
    rows
}

/// The table as lines, with its total.
pub fn render_net(rev: &str, rows: &[NetRow]) -> Vec<String> {
    let mut lines = vec![format!(
        "net lines by area since {rev}: {:>8} {:>8} {:>8}  area",
        "source", "tests", "text"
    )];
    let mut total = NetRow::default();
    for row in rows
        .iter()
        .filter(|r| r.source != 0 || r.tests != 0 || r.text != 0)
    {
        total.source += row.source;
        total.tests += row.tests;
        total.text += row.text;
        lines.push(format!(
            "  {:>8} {:>8} {:>8}  {}",
            row.source, row.tests, row.text, row.area
        ));
    }
    lines.push(format!(
        "  {:>8} {:>8} {:>8}  total; source plus tests {}",
        total.source,
        total.tests,
        total.text,
        total.code()
    ));
    lines
}

/// How many lines `str::lines` counts, read from bytes so a file that is not
/// UTF-8 is counted rather than skipped.
fn count_lines(bytes: &[u8]) -> usize {
    let breaks = bytes.iter().filter(|&&b| b == b'\n').count();
    breaks + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"))
}

/// Whether the bytes are text: no NUL in the first part of the file.
fn is_text(bytes: &[u8]) -> bool {
    !bytes.iter().take(8000).any(|&b| b == 0)
}

/// **Every tracked text file and its line count.** A tracked path that is gone
/// from the work tree is left out, as a deleted file is; a binary file is left
/// out. A path that cannot be read, or a tree git cannot list, is an error.
pub fn measure() -> Result<Vec<Measured>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .output()
        .map_err(|e| format!("git could not be run to list the tracked files: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut files = Vec::new();
    for path in output.stdout.split(|&b| b == 0).filter(|p| !p.is_empty()) {
        let path = std::str::from_utf8(path)
            .map_err(|_| "a tracked path is not UTF-8".to_string())?
            .to_string();
        match std::fs::read(&path) {
            Ok(bytes) if is_text(&bytes) => files.push(Measured {
                path,
                lines: count_lines(&bytes),
            }),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{path}: could not be read: {e}")),
        }
    }
    Ok(files)
}

/// The file in the work tree. No file is [`Limits::empty`]; a file that exists
/// and is malformed is an error.
pub fn load(path: &Path) -> Result<Limits, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Limits::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Limits::empty()),
        Err(e) => Err(format!("{}: could not be read: {e}", path.display())),
    }
}

fn git_output(args: &[&str]) -> Option<Vec<u8>> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| out.stdout)
}

/// **What git counts for each path since `joint`**, from `--numstat -z`. A binary
/// file has no count and is left out. With renames followed, a rename record
/// names the old path and then the new; the new one is the key.
fn numstat(joint: &str, follow: bool) -> Option<HashMap<String, Change>> {
    let flag = if follow { "-M" } else { "--no-renames" };
    let stdout = git_output(&["diff", flag, "--numstat", "-z", joint])?;
    let mut tokens = stdout.split(|&b| b == 0).filter(|t| !t.is_empty());
    let mut found = HashMap::new();
    while let Some(record) = tokens.next() {
        let text = String::from_utf8_lossy(record).into_owned();
        let mut parts = text.splitn(3, '\t');
        let (added, removed, path) = (parts.next()?, parts.next()?, parts.next()?);
        let path = if path.is_empty() {
            let _old = tokens.next()?;
            String::from_utf8_lossy(tokens.next()?).into_owned()
        } else {
            path.to_string()
        };
        if let (Ok(added), Ok(removed)) = (added.parse(), removed.parse()) {
            found.insert(
                path,
                Change {
                    added,
                    removed,
                    new: false,
                },
            );
        }
    }
    Some(found)
}

/// The paths that are new since `joint`, with renames followed: a rename is not new.
fn added_paths(joint: &str) -> Option<Vec<String>> {
    let stdout = git_output(&["diff", "-M", "--name-status", "-z", joint])?;
    let mut tokens = stdout.split(|&b| b == 0).filter(|t| !t.is_empty());
    let mut added = Vec::new();
    while let Some(status) = tokens.next() {
        let status = String::from_utf8_lossy(status).into_owned();
        let first = String::from_utf8_lossy(tokens.next()?).into_owned();
        if status.starts_with('R') || status.starts_with('C') {
            let _new = tokens.next()?;
        } else if status.starts_with('A') {
            added.push(first);
        }
    }
    Some(added)
}

/// **The change since `joint`**, or none when git could not say.
fn changes_since(joint: &str) -> Option<Changes> {
    let mut followed = numstat(joint, true)?;
    for path in added_paths(joint)? {
        followed
            .entry(path)
            .or_insert(Change {
                added: 0,
                removed: 0,
                new: true,
            })
            .new = true;
    }
    Some(Changes {
        followed,
        flat: numstat(joint, false)?,
    })
}

fn in_ci() -> bool {
    std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true")
}

/// **The remote line the checked-out branch was cut from**: of the remote-tracking
/// `main` and carry lines (`em-*`), the one HEAD is fewest commits past its
/// merge-base with. A checkout's own `main` can lag far behind a carry, so it is
/// never the answer here.
fn remote_line() -> Option<String> {
    let text = |args: &[&str]| {
        git_output(args).map(|out| String::from_utf8_lossy(&out).trim().to_string())
    };
    let refs = text(&["for-each-ref", "--format=%(refname:short)", "refs/remotes"])?;
    refs.lines()
        .filter(|name| {
            let branch = name.split_once('/').map_or(*name, |(_, branch)| branch);
            branch == "main" || branch.starts_with("em-")
        })
        .filter_map(|name| {
            let joint = text(&["merge-base", "HEAD", name]).filter(|joint| !joint.is_empty())?;
            let ahead = text(&["rev-list", "--count", &format!("{joint}..HEAD")])?;
            Some((ahead.parse::<usize>().ok()?, name))
        })
        .min()
        .map(|(_, name)| name.to_string())
}

/// CI's new-branch event has no before commit; measure from main in that case.
/// A valid explicit base still wins. Off CI, no base and a bare `main` both mean
/// the remote line the branch was cut from, when the checkout has one; a sha is
/// the spelling for the checkout's own `main`.
pub fn read_base() -> Base {
    let mut wanted = [BASE_VARIABLE, BASE_REF_VARIABLE]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    let zero = wanted
        .as_ref()
        .is_some_and(|value| value.len() == 40 && value.bytes().all(|byte| byte == b'0'));
    if in_ci() && (wanted.is_none() || zero) {
        wanted = ["origin/main", "main"].into_iter().find_map(|name| {
            git_output(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{name}^{{commit}}"),
            ])
            .map(|_| name.to_string())
        });
    }
    if !in_ci() && wanted.as_deref().is_none_or(|name| name == "main") {
        wanted = remote_line().or(wanted);
    }
    let Some(wanted) = wanted else {
        return Base::Unknown(format!(
            "neither {BASE_VARIABLE} nor {BASE_REF_VARIABLE} is set"
        ));
    };
    if !crate::is_a_revision(&wanted) {
        return Base::Unknown(format!("{wanted} is not a revision git knows"));
    }
    let text = |args: &[&str]| {
        git_output(args).map(|out| String::from_utf8_lossy(&out).trim().to_string())
    };
    let Some(joint) = text(&["merge-base", "HEAD", &wanted]).filter(|joint| !joint.is_empty())
    else {
        return Base::Unknown(format!("{wanted} has no common commit with HEAD here"));
    };
    let short: String = joint.chars().take(10).collect();
    let limits = match text(&["show", &format!("{joint}:{RATCHET_FILE}")]) {
        None => Limits::empty(),
        Some(text) => match Limits::parse_base(&text) {
            Ok(limits) => limits,
            Err(e) => {
                return Base::Unknown(format!("{RATCHET_FILE} at {short} cannot be read: {e}"));
            }
        },
    };
    let Some(changes) = changes_since(&joint) else {
        return Base::Unknown(format!("git could not list the change since {short}"));
    };
    Base::Known {
        rev: short,
        limits,
        changes,
    }
}

/// Rule 661 limits additions, including binary files, rather than grandfathered tests.
fn test_file_offences(limits: &Limits, base: &Base) -> Result<Vec<Offence>, String> {
    let revision = match base {
        Base::Known { rev, .. } => Some(rev.clone()),
        Base::Unknown(_) => ["main", "origin/main"]
            .iter()
            .find_map(|reference| {
                let output = Command::new("git")
                    .args(["merge-base", "HEAD", reference])
                    .output()
                    .ok()?;
                output
                    .status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
            })
            .or_else(|| {
                Command::new("git")
                    .args(["rev-parse", "--verify", "HEAD"])
                    .output()
                    .ok()?
                    .status
                    .success()
                    .then(|| "HEAD".to_string())
            }),
    };
    let mut command = Command::new("git");
    command.args([
        "diff",
        "--no-renames",
        "--name-only",
        "--diff-filter=A",
        "-z",
    ]);
    // With no first commit, every indexed path is an addition; --cached needs no HEAD.
    match revision {
        Some(revision) => command.arg(revision),
        None => command.arg("--cached"),
    };
    let output = command
        .output()
        .map_err(|error| format!("listing added test files: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff could not list added test files: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut offences = Vec::new();
    for raw in output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(raw).map_err(|_| "an added path is not UTF-8")?;
        let parts: Vec<_> = path.split('/').collect();
        let under_tests = parts.first() == Some(&"tests")
            || (parts.first() == Some(&"crates") && parts.get(2) == Some(&"tests"));
        if !under_tests {
            continue;
        }
        let bytes = std::fs::metadata(path)
            .map_err(|error| format!("{path}: {error}"))?
            .len();
        if bytes > limits.test_file_bytes as u64 {
            offences.push(Offence::TestFile {
                path: path.into(),
                bytes,
                limit: limits.test_file_bytes,
            });
        }
    }
    Ok(offences)
}

/// **The phase of `bar check`, `bar report` and `bar dev`**, run first. It
/// fails the summary on any offence, on an unknown base, and on any measurement
/// it could not make, and it skips none of the phases after it. An absent
/// legacy dispatch default keeps a note; its owned-marker checks still run.
pub fn phase(summary: &mut Summary) {
    let measured = measure().and_then(|files| Ok((files, load(Path::new(RATCHET_FILE))?)));
    let (files, limits) = match measured {
        Ok(both) => both,
        Err(refusal) => {
            summary.phase_failed("size", &format!("not measured — {refusal}"));
            return;
        }
    };
    let base = read_base();
    let mut verdict = judge(&limits, &files, &base);
    let owned = markers::scan(&files, limits.step.as_deref());
    match test_file_offences(&limits, &base).and_then(|own| Ok((own, owned?))) {
        Ok((offences, found)) => {
            verdict.offences.extend(offences);
            verdict
                .offences
                .extend(found.into_iter().map(Offence::Marker));
        }
        Err(refusal) => {
            summary.phase_failed("size", &format!("not measured — {refusal}"));
            return;
        }
    }
    let Base::Known { rev, changes, .. } = &base else {
        let Base::Unknown(why) = &base else { return };
        let dispatch_note = in_ci()
            && std::env::var("GITHUB_EVENT_NAME").is_ok_and(|v| v == "workflow_dispatch")
            && std::env::var(BASE_VARIABLE).is_ok_and(|v| v == "origin/deploy-9")
            && git_output(&[
                "rev-parse",
                "--verify",
                "--quiet",
                "origin/deploy-9^{commit}",
            ])
            .is_none();
        if in_ci() && !dispatch_note {
            summary.phase_failed("size", &format!("no usable CI base: {why}"));
            return;
        }
        if dispatch_note {
            // A full dispatch has no PR base; the legacy default may no longer exist.
            summary.note("size: not judged — dispatch default origin/deploy-9 is absent; growth was not measured");
        } else {
            // Not ok, and not a failure of the run: growth was not judged, and the line
            // says so. A workflow that means to judge it sets SIZE_BASE.
            summary.note(&format!(
                "size: unknown — {why}; growth was not judged, so the size is not ok"
            ));
            // A base that was named and cannot be read is a broken workflow, not a dev host.
            if std::env::var(BASE_VARIABLE).is_ok_and(|v| !v.is_empty()) {
                summary.phase_failed("size", "the named base could not be used");
            }
        }
        for offence in &verdict.offences {
            summary.note(&format!("  {}", offence.line()));
        }
        if !verdict.offences.is_empty() {
            summary.phase_failed("size", &format!("{} offence(s)", verdict.offences.len()));
        }
        return;
    };
    let counts = format!(
        "{} red, {} orange and {} yellow files; base: compared with {rev}",
        verdict.red, verdict.orange, verdict.yellow
    );
    if verdict.offences.is_empty() {
        summary.phase_ok("size", &counts);
    } else {
        summary.phase_failed(
            "size",
            &format!("{} offence(s) — {counts}", verdict.offences.len()),
        );
        for offence in &verdict.offences {
            summary.note(&format!("  {}", offence.line()));
        }
    }
    for line in &verdict.warnings {
        summary.note(&format!("  warning: {line}"));
    }
    for line in render_net(rev, &net_by_area(changes)) {
        summary.note(&format!("  {line}"));
    }
}

/// **`bar size`**: the check alone. `--net` prints the table of the change since
/// the base; with `--reason-file FILE` it also refuses a positive total of
/// source plus tests unless the file names a reason.
pub fn run(args: &[String]) -> std::io::Result<std::process::ExitCode> {
    use std::process::ExitCode;
    let usage = || {
        eprintln!("usage: bar size [--weekly | --net [--reason-file FILE]]");
        Ok(ExitCode::from(2))
    };
    let fail = |message: String| {
        eprintln!("bar size: {message}");
        Ok(ExitCode::FAILURE)
    };
    match args {
        [] => {
            let mut summary = Summary::new();
            phase(&mut summary);
            print!("{}", summary.render());
            Ok(if summary.green {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        [weekly] if weekly == "--weekly" => {
            let limits = match load(Path::new(RATCHET_FILE)) {
                Ok(limits) => limits,
                Err(e) => return fail(e),
            };
            let files = match measure() {
                Ok(files) => files,
                Err(e) => return fail(e),
            };
            let mut areas: BTreeMap<String, usize> = BTreeMap::new();
            for file in &files {
                *areas.entry(area_of(&file.path)).or_default() += file.lines;
            }
            let red = files.iter().filter(|f| f.lines > limits.cap).count();
            let orange = files
                .iter()
                .filter(|f| f.lines > ORANGE && f.lines <= limits.cap)
                .count();
            let yellow = files
                .iter()
                .filter(|f| f.lines > YELLOW && f.lines <= ORANGE.min(limits.cap))
                .count();
            println!(
                "total {} lines in {} files; {red} red, {orange} orange, {yellow} yellow",
                files.iter().map(|f| f.lines).sum::<usize>(),
                files.len(),
            );
            for (area, lines) in &areas {
                println!("{lines:>8}  {area}");
            }
            Ok(ExitCode::SUCCESS)
        }
        [net, rest @ ..] if net == "--net" => {
            let reason = match rest {
                [] => None,
                [flag, file] if flag == "--reason-file" => Some(file),
                _ => return usage(),
            };
            let Base::Known { rev, changes, .. } = read_base() else {
                return fail("the base is unknown, so the net cannot be told".into());
            };
            let rows = net_by_area(&changes);
            for line in render_net(&rev, &rows) {
                println!("{line}");
            }
            let code: i64 = rows.iter().map(NetRow::code).sum();
            if code <= 0 {
                return Ok(ExitCode::SUCCESS);
            }
            let named = reason
                .and_then(|file| std::fs::read_to_string(file).ok())
                .is_some_and(|text| !text.trim().is_empty());
            if named {
                println!("source plus tests is {code} lines up, and a reason is named");
                Ok(ExitCode::SUCCESS)
            } else {
                fail(format!(
                    "source plus tests is {code} lines up. Name what grew in order to shrink, \
                     and the step where it shrinks, in a reason file (--reason-file)."
                ))
            }
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, lines: usize) -> Measured {
        Measured {
            path: path.to_string(),
            lines,
        }
    }

    fn known(changes: &[(&str, usize, usize, bool)], was: Limits) -> Base {
        let followed = changes
            .iter()
            .map(|&(path, added, removed, new)| {
                (
                    path.to_string(),
                    Change {
                        added,
                        removed,
                        new,
                    },
                )
            })
            .collect();
        Base::Known {
            rev: "abc".into(),
            limits: was,
            changes: Changes {
                followed,
                flat: HashMap::new(),
            },
        }
    }

    /// **The file the repo ships parses**, so a hand edit that breaks the shape
    /// fails here and not only in the first run that reads it.
    #[test]
    fn the_shipped_file_parses() {
        let text = include_str!("../../../size-ratchet.toml");
        let limits = Limits::parse(text).expect("the shipped size-ratchet.toml parses");
        assert!(limits.test_file_bytes <= 100_000);
    }

    #[test]
    fn the_parser_refuses_each_malformed_shape_by_line() {
        for (text, wants) in [
            (
                "test_file_bytes = x\n",
                "line 1: test_file_bytes must be a whole number",
            ),
            (
                "test_file_bytes = 1\ntest_file_bytes = 2\n",
                "line 2: test_file_bytes is given twice",
            ),
            ("test_file_bytes = 100001\n", "may not exceed 100000"),
            ("cap = x\n", "line 1: cap must be a whole number"),
            ("cap = 1999\n", "cap may not be under 2000"),
            ("cap = 5000\ncap = 4000\n", "line 2: cap is given twice"),
            ("step = 9.3.1\n", "line 1: step must be dotted numbers"),
            (
                "step = \"9.3.1\"\nstep = \"9.3.2\"\n",
                "line 2: step is given twice",
            ),
            ("path = \"a.rs\"\n", "line 1: unknown key path"),
            ("[[file]]\n", "line 1: unknown table [[file]]"),
            ("[[raise]]\n", "line 1: unknown table [[raise]]"),
        ] {
            let error = Limits::parse(text).expect_err(text);
            assert!(
                error.contains(wants),
                "{text:?} gave {error:?}, wanted {wants:?}"
            );
        }
    }

    /// **The file as the earlier lists shaped it still reads at the base**: the cap
    /// and each [[file]] and [[raise]] entry are skipped, and the test limit is read.
    #[test]
    fn the_base_read_skips_the_cap_and_the_old_entries() {
        let old = "cap = 2000\ntest_file_bytes = 90000\n\n[[file]]\npath = \"a.rs\"\nlines = 2100\n\
                   owner = 7\n\n[[file]]\npath = \"b.rs\"\nlines = 3000\n";
        let limits = Limits::parse_base(old).expect("the old shape reads at the base");
        assert_eq!(limits.test_file_bytes, 90_000);
        assert_eq!(limits.cap, CEILING, "the old cap meant the yellow line");
        let raised = "test_file_bytes = 90000\n\n[[raise]]\npath = \"a.rs\"\nrule = 704\n";
        let limits = Limits::parse_base(raised).expect("a raise reads at the base");
        assert_eq!(limits.test_file_bytes, 90_000);
    }

    #[test]
    fn lines_are_counted_as_str_lines_counts_them() {
        for text in ["", "a", "a\n", "a\nb", "a\nb\n", "\n", "\n\n", "a\r\nb\r\n"] {
            assert_eq!(
                count_lines(text.as_bytes()),
                text.lines().count(),
                "{text:?}"
            );
        }
        assert_eq!(count_lines(&[0xff, b'\n', 0xfe]), 2);
    }

    #[test]
    fn a_file_with_a_nul_byte_is_binary_and_one_without_is_text() {
        assert!(is_text(b"hello\nworld\n"));
        assert!(!is_text(&[b'a', 0, b'b']));
    }

    #[test]
    fn kinds_split_source_tests_and_text() {
        for (path, kind) in [
            ("crates/jojobot-bar/src/size.rs", Kind::Source),
            ("crates/jojobot-bar/tests/size_report.rs", Kind::Tests),
            (
                "crates/jojobot-domain/src/memory/testing/fake.rs",
                Kind::Tests,
            ),
            (
                "crates/jojobot-mcp/src/orientation/floor_unmeasured_tests.rs",
                Kind::Tests,
            ),
            (
                "crates/jojobot-domain/src/memory/graph/tests.rs",
                Kind::Tests,
            ),
            ("scripts/wait-run", Kind::Source),
            ("scripts/dev-command-gate", Kind::Source),
            ("flake.nix", Kind::Source),
            ("CLAUDE.md", Kind::Text),
            ("size-ratchet.toml", Kind::Text),
            (".forgejo/workflows/branch.yml", Kind::Text),
            ("crates/jojobot-mcp/served-texts/tools.md", Kind::Text),
            ("crates/jojobot/tests/fixtures/x/doltdump.sql", Kind::Text),
        ] {
            assert_eq!(kind_of(path), kind, "{path}");
        }
    }

    #[test]
    fn areas_name_a_crate_and_its_top_level_module() {
        for (path, area) in [
            (
                "crates/jojobot-adapters/src/dolt/memory.rs",
                "crates/jojobot-adapters/dolt",
            ),
            (
                "crates/jojobot-adapters/src/search.rs",
                "crates/jojobot-adapters/search",
            ),
            (
                "crates/jojobot-adapters/tests/dolt_repair.rs",
                "crates/jojobot-adapters",
            ),
            ("crates/jojobot-bar/Cargo.toml", "crates/jojobot-bar"),
            ("scripts/branch-prune", "scripts"),
            (".forgejo/workflows/branch.yml", ".forgejo"),
            ("CLAUDE.md", "(root)"),
        ] {
            assert_eq!(area_of(path), area, "{path}");
        }
    }

    #[test]
    fn a_red_file_that_shrank_or_stood_still_passes() {
        let files = [file("b.rs", 6000)];
        let base = known(&[("b.rs", 0, 10, false)], Limits::empty());
        assert!(judge(&Limits::empty(), &files, &base).offences.is_empty());
        assert!(
            judge(&Limits::empty(), &files, &known(&[], Limits::empty()))
                .offences
                .is_empty()
        );
    }

    /// **A red file that grew is counted, not refused** (rule 741): the warning
    /// names the file, the before, the after and the growth.
    #[test]
    fn a_red_file_that_grew_is_named_with_its_growth_and_not_refused() {
        let files = [file("b.rs", 6010)];
        let base = known(&[("b.rs", 10, 0, false)], Limits::empty());
        let verdict = judge(&Limits::empty(), &files, &base);
        assert!(verdict.offences.is_empty(), "{:?}", verdict.offences);
        assert_eq!(verdict.red, 1);
        assert_eq!(verdict.warnings.len(), 1, "{:?}", verdict.warnings);
        assert!(
            verdict.warnings[0].contains("b.rs: red file grew from 6000 to 6010 lines, 10 more"),
            "{}",
            verdict.warnings[0]
        );
    }

    #[test]
    fn the_test_byte_limit_and_the_cap_may_only_fall() {
        let was = Limits {
            test_file_bytes: 90_000,
            cap: 4000,
            ..Limits::empty()
        };
        let verdict = judge(&Limits::empty(), &[], &known(&[], was));
        assert_eq!(
            verdict.offences,
            vec![
                Offence::TestLimitRaised {
                    base: 90_000,
                    now: 100_000
                },
                Offence::CapRaised {
                    base: 4000,
                    now: 5000
                },
            ]
        );
    }

    #[test]
    fn a_lower_cap_makes_a_file_red_that_was_yellow() {
        let low = Limits {
            cap: 3000,
            ..Limits::empty()
        };
        let verdict = judge(
            &low,
            &[file("a.rs", 3500)],
            &known(&[("a.rs", 3500, 0, true)], Limits::empty()),
        );
        assert_eq!(verdict.red, 1);
        assert_eq!(
            verdict.offences,
            vec![Offence::New {
                path: "a.rs".into(),
                now: 3500
            }],
            "the cap fell, which is allowed, and the file is new and now red"
        );
    }

    #[test]
    fn the_table_nets_added_against_removed_by_area_and_kind_and_totals() {
        let change = |added, removed| Change {
            added,
            removed,
            new: false,
        };
        let flat: HashMap<String, Change> = [
            ("crates/a/src/x.rs", change(10, 40)),
            ("crates/a/tests/t.rs", change(5, 20)),
            ("crates/a/src/y/z.rs", change(1, 0)),
            ("CLAUDE.md", change(8, 2)),
        ]
        .into_iter()
        .map(|(path, change)| (path.to_string(), change))
        .collect();
        let rows = net_by_area(&Changes {
            followed: HashMap::new(),
            flat,
        });
        let by_area = |area: &str| rows.iter().find(|row| row.area == area).unwrap().clone();
        assert_eq!(by_area("crates/a/x").source, -30);
        assert_eq!(by_area("crates/a").tests, -15);
        assert_eq!(by_area("crates/a/y").source, 1);
        assert_eq!(by_area("(root)").text, 6);
        assert_eq!(
            rows[0].area, "crates/a/x",
            "the most negative code is first"
        );
        let lines = render_net("abc", &rows);
        assert!(
            lines.last().unwrap().contains("source plus tests -44"),
            "{lines:?}"
        );
    }
}
