//! **The size check: no new file over the cap, no listed file grows, and a raise
//! that names its card is the only way past.** Rules 273 and 597.
//!
//! `size-ratchet.toml` holds the cap and one entry for each tracked `.rs` file
//! over it, with the exact line count. Two halves judge a change:
//!
//! - **The tree half** compares the entries with the tree. A listed file over
//!   its entry grew. An unlisted file over the cap is new. An entry whose file
//!   is gone, at or under the cap, or smaller than the entry is stale, so the
//!   room a shrunk file freed cannot be spent again. `bar size --write` fixes
//!   the stale entries and nothing else.
//! - **The base half** compares the entries with the file as it stood at the
//!   base of the change. The cap may only fall. An entry may only fall, unless it
//!   names a card that the base entry did not name. A new entry is allowed only
//!   as the move of an entry whose file left the tree. With no base copy the
//!   half says unknown; unknown is never ok.
//!
//! **Incomplete measurement is a failure.** A file the bar cannot read, or a tree
//! git cannot list, fails the phase: it is never read as no growth.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::Summary;

/// The checked-in list, at the repo root.
pub const RATCHET_FILE: &str = "size-ratchet.toml";

/// The cap that `bar size --init` writes. The file carries the cap after that.
pub const INITIAL_CAP: usize = 2000;

/// The base of the change, as a revision. It wins over [`BASE_REF_VARIABLE`]:
/// a workflow sets it to an exact commit, while the other names a branch that a
/// checkout may not hold.
pub const BASE_VARIABLE: &str = "SIZE_BASE";

/// The branch a pull request targets, as a runner sets it.
pub const BASE_REF_VARIABLE: &str = "GITHUB_BASE_REF";

/// One listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub lines: usize,
    /// The card that justifies this entry being higher than at the base.
    pub raised_by: Option<u32>,
}

/// The whole list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ratchet {
    pub cap: usize,
    pub entries: Vec<Entry>,
}

impl Ratchet {
    /// **No list at all**: the initial cap and no entry. A tree with no file over
    /// the cap passes it; every file over the cap is new.
    pub fn empty() -> Ratchet {
        Ratchet {
            cap: INITIAL_CAP,
            entries: Vec::new(),
        }
    }

    fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// **Read the file's text.** Only its own shape is read: `cap = N` before the
    /// first `[[file]]` header, and under each header `path`, `lines` and the
    /// optional `raised_by`. Paths are sorted and unique. Every refusal names the
    /// line it is about.
    pub fn parse(text: &str) -> Result<Ratchet, String> {
        let mut cap: Option<usize> = None;
        let mut pending: Vec<(usize, Pending)> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[file]]" {
                pending.push((number, Pending::default()));
                continue;
            }
            if line.starts_with('[') {
                return Err(format!("line {number}: unknown table {line}"));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {number}: not a key = value line: {line}"));
            };
            let (key, value) = (key.trim(), value.trim());
            match pending.last_mut() {
                None if key == "cap" => {
                    if cap.is_some() {
                        return Err(format!("line {number}: cap is given twice"));
                    }
                    cap = Some(whole(key, value, number)?);
                }
                None => return Err(format!("line {number}: {key} before any [[file]] header")),
                Some(_) if key == "cap" => {
                    return Err(format!(
                        "line {number}: cap must come before the first entry"
                    ));
                }
                Some((_, entry)) => entry.set(key, value, number)?,
            }
        }
        let cap = cap.ok_or("the file has no cap = N line")?;
        let mut entries: Vec<Entry> = Vec::new();
        for (start, entry) in pending {
            let entry = entry.finish(start)?;
            if let Some(last) = entries.last() {
                if last.path == entry.path {
                    return Err(format!("line {start}: {} is listed twice", entry.path));
                }
                if last.path > entry.path {
                    return Err(format!(
                        "line {start}: {} is out of order after {}",
                        entry.path, last.path
                    ));
                }
            }
            entries.push(entry);
        }
        Ok(Ratchet { cap, entries })
    }

    /// The file as `parse` reads it.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# The size ratchet (rules 273 and 597). `bar size` checks the tree against it.\n\
             # A file over `cap` lines is listed with its exact line count. A listed file\n\
             # may not grow, and a file over the cap that is not listed is refused.\n\
             # `bar size --write` lowers an entry or deletes it, and adds nothing.\n\
             # To raise an entry, set `lines` to the new count and add `raised_by` with\n\
             # the card that justifies it. Any other raise is refused against the base.\n",
        );
        out.push_str(&format!("\ncap = {}\n", self.cap));
        for entry in &self.entries {
            out.push_str(&format!(
                "\n[[file]]\npath = \"{}\"\nlines = {}\n",
                entry.path, entry.lines
            ));
            if let Some(card) = entry.raised_by {
                out.push_str(&format!("raised_by = {card}\n"));
            }
        }
        out
    }
}

#[derive(Default)]
struct Pending {
    path: Option<String>,
    lines: Option<usize>,
    raised_by: Option<u32>,
}

impl Pending {
    fn set(&mut self, key: &str, value: &str, line: usize) -> Result<(), String> {
        let twice = || format!("line {line}: {key} is given twice in one entry");
        match key {
            "path" => {
                if self.path.is_some() {
                    return Err(twice());
                }
                let path = value
                    .strip_prefix('"')
                    .and_then(|rest| rest.strip_suffix('"'))
                    .filter(|inner| !inner.contains(['"', '\\']) && !inner.is_empty())
                    .ok_or_else(|| {
                        format!("line {line}: path must be a non-empty string in double quotes")
                    })?;
                self.path = Some(path.to_string());
            }
            "lines" => {
                if self.lines.is_some() {
                    return Err(twice());
                }
                self.lines = Some(whole(key, value, line)?);
            }
            "raised_by" => {
                if self.raised_by.is_some() {
                    return Err(twice());
                }
                self.raised_by = Some(value.parse().map_err(|_| {
                    format!("line {line}: raised_by must be a card number, not {value}")
                })?);
            }
            other => return Err(format!("line {line}: unknown key {other}")),
        }
        Ok(())
    }

    fn finish(self, start: usize) -> Result<Entry, String> {
        let missing = |part: &str| format!("line {start}: the entry has no {part}");
        Ok(Entry {
            path: self.path.ok_or_else(|| missing("path"))?,
            lines: self.lines.ok_or_else(|| missing("lines"))?,
            raised_by: self.raised_by,
        })
    }
}

fn whole(key: &str, value: &str, line: usize) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("line {line}: {key} must be a whole number, not {value}"))
}

/// A tracked `.rs` file and how many lines it holds now, counted as
/// `str::lines` counts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub path: String,
    pub lines: usize,
}

/// **Why a check refuses.** Each says what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offence {
    /// A listed file is over its entry.
    Grew {
        path: String,
        entry: usize,
        now: usize,
    },
    /// An unlisted file is over the cap.
    New { path: String, now: usize },
    /// An entry that `--write` would lower or delete.
    Stale {
        path: String,
        entry: usize,
        reason: String,
    },
    /// The cap is higher than at the base.
    CapRaised { base: usize, now: usize },
    /// An entry is higher than at the base and names no new card.
    Raised {
        path: String,
        base: usize,
        now: usize,
    },
    /// An entry the base did not have, and no file left the tree to explain it.
    Added { path: String, now: usize },
}

impl Offence {
    /// One line, naming the file, the before and after, and what to do.
    pub fn line(&self) -> String {
        match self {
            Offence::Grew { path, entry, now } => format!(
                "{path}: grew from {entry} to {now} lines. Shrink it back, or raise its \
                 entry to {now} in {RATCHET_FILE} with raised_by = <the card that needs it>."
            ),
            Offence::New { path, now } => format!(
                "{path}: new file over the cap, {now} lines. Keep it under the cap; put new \
                 code in a new module."
            ),
            Offence::Stale {
                path,
                entry,
                reason,
            } => format!(
                "{path}: entry says {entry} but {reason}. Run `bar size --write` and commit \
                 {RATCHET_FILE}."
            ),
            Offence::CapRaised { base, now } => {
                format!("{RATCHET_FILE}: the cap went from {base} to {now}. The cap only falls.")
            }
            Offence::Raised { path, base, now } => format!(
                "{path}: the entry went from {base} to {now} against the base without a new \
                 card. Add raised_by = <the card that needs it> to the entry."
            ),
            Offence::Added { path, now } => format!(
                "{path}: new entry at {now} lines against the base, and no listed file left \
                 the tree to explain it. A move keeps its entry; a new file stays under the cap."
            ),
        }
    }
}

/// **The tree half.**
pub fn judge_tree(ratchet: &Ratchet, files: &[Measured]) -> Vec<Offence> {
    let mut offences = Vec::new();
    for file in files.iter().filter(|file| file.lines > ratchet.cap) {
        match ratchet.entry(&file.path) {
            None => offences.push(Offence::New {
                path: file.path.clone(),
                now: file.lines,
            }),
            Some(entry) if file.lines > entry.lines => offences.push(Offence::Grew {
                path: file.path.clone(),
                entry: entry.lines,
                now: file.lines,
            }),
            Some(_) => {}
        }
    }
    for entry in &ratchet.entries {
        let reason = match files.iter().find(|file| file.path == entry.path) {
            None => Some("the file is gone".to_string()),
            Some(file) if file.lines <= ratchet.cap => Some(format!(
                "the file holds {} lines, at or under the cap of {}",
                file.lines, ratchet.cap
            )),
            Some(file) if file.lines < entry.lines => {
                Some(format!("the file holds {} lines", file.lines))
            }
            Some(_) => None,
        };
        if let Some(reason) = reason {
            offences.push(Offence::Stale {
                path: entry.path.clone(),
                entry: entry.lines,
                reason,
            });
        }
    }
    offences
}

/// **The base half.** `tracked` is every tracked `.rs` path in the tree now.
pub fn judge_base(now: &Ratchet, base: &Ratchet, tracked: &HashSet<&str>) -> Vec<Offence> {
    let mut offences = Vec::new();
    if now.cap > base.cap {
        offences.push(Offence::CapRaised {
            base: base.cap,
            now: now.cap,
        });
    }
    // The entries whose file left the tree, each able to explain one new entry.
    let mut left: Vec<&Entry> = base
        .entries
        .iter()
        .filter(|entry| !tracked.contains(entry.path.as_str()))
        .collect();
    for entry in &now.entries {
        match base.entry(&entry.path) {
            Some(was) if entry.lines > was.lines => {
                if entry.raised_by.is_none() || entry.raised_by == was.raised_by {
                    offences.push(Offence::Raised {
                        path: entry.path.clone(),
                        base: was.lines,
                        now: entry.lines,
                    });
                }
            }
            Some(_) => {}
            None => {
                let named = entry.raised_by.is_some();
                let explained = left
                    .iter()
                    .position(|gone| named || entry.lines <= gone.lines)
                    .map(|at| left.remove(at))
                    .is_some();
                if !explained {
                    offences.push(Offence::Added {
                        path: entry.path.clone(),
                        now: entry.lines,
                    });
                }
            }
        }
    }
    offences
}

/// **What `--write` makes of the list**: entries lowered to their file, and
/// deleted when the file is gone or at or under the cap. It adds nothing and
/// raises nothing.
pub fn lowered(ratchet: &Ratchet, files: &[Measured]) -> Ratchet {
    let entries = ratchet
        .entries
        .iter()
        .filter_map(|entry| {
            let file = files.iter().find(|file| file.path == entry.path)?;
            (file.lines > ratchet.cap).then(|| Entry {
                lines: entry.lines.min(file.lines),
                ..entry.clone()
            })
        })
        .collect();
    Ratchet {
        cap: ratchet.cap,
        entries,
    }
}

/// **A list made from the tree**, for `--init`: every tracked file over the cap.
pub fn from_tree(files: &[Measured]) -> Ratchet {
    let mut entries: Vec<Entry> = files
        .iter()
        .filter(|file| file.lines > INITIAL_CAP)
        .map(|file| Entry {
            path: file.path.clone(),
            lines: file.lines,
            raised_by: None,
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ratchet {
        cap: INITIAL_CAP,
        entries,
    }
}

/// How many lines `str::lines` counts, read from bytes so a file that is not
/// UTF-8 is counted rather than skipped.
fn count_lines(bytes: &[u8]) -> usize {
    let breaks = bytes.iter().filter(|&&b| b == b'\n').count();
    breaks + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"))
}

/// **Every tracked `.rs` file and its line count.** A tracked path that is gone
/// from the work tree is left out, as a deleted file is. A path that cannot be read,
/// or a tree git cannot list, is an error.
pub fn measure() -> Result<Vec<Measured>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "*.rs"])
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
            Ok(bytes) => files.push(Measured {
                path,
                lines: count_lines(&bytes),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{path}: could not be read: {e}")),
        }
    }
    Ok(files)
}

/// The list in the work tree. No file is [`Ratchet::empty`]; a file that exists
/// and is malformed is an error.
pub fn load(path: &Path) -> Result<Ratchet, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ratchet::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ratchet::empty()),
        Err(e) => Err(format!("{}: could not be read: {e}", path.display())),
    }
}

/// **What the base half compared with**, or why it could not.
pub enum Base {
    Unknown(String),
    Copy { rev: String, ratchet: Ratchet },
}

/// **The list as it stood where this change branched from the base.** The base
/// is `SIZE_BASE`, else `GITHUB_BASE_REF`. No remote is guessed: a name git does
/// not resolve here is unknown, and says so.
pub fn read_base() -> Base {
    let wanted = [BASE_VARIABLE, BASE_REF_VARIABLE]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    let Some(wanted) = wanted else {
        return Base::Unknown(format!(
            "neither {BASE_VARIABLE} nor {BASE_REF_VARIABLE} is set"
        ));
    };
    if !crate::is_a_revision(&wanted) {
        return Base::Unknown(format!("{wanted} is not a revision git knows"));
    }
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let Some(joint) = git(&["merge-base", "HEAD", &wanted]).filter(|joint| !joint.is_empty())
    else {
        return Base::Unknown(format!("{wanted} has no common commit with HEAD here"));
    };
    let short: String = joint.chars().take(10).collect();
    let Some(text) = git(&["show", &format!("{joint}:{RATCHET_FILE}")]) else {
        return Base::Unknown(format!("no {RATCHET_FILE} at {short}"));
    };
    match Ratchet::parse(&text) {
        Ok(ratchet) => Base::Copy {
            rev: short,
            ratchet,
        },
        Err(e) => Base::Unknown(format!("{RATCHET_FILE} at {short} cannot be read: {e}")),
    }
}

/// **The verdict of both halves.**
pub struct Verdict {
    /// Tracked files over the cap now.
    pub over: usize,
    /// Entries in the list.
    pub listed: usize,
    pub cap: usize,
    pub offences: Vec<Offence>,
    /// What the base half did: the revision it compared with, or why it could not.
    pub base: String,
}

/// Judge the tree and, when there is a base copy, the change.
pub fn judge(ratchet: &Ratchet, files: &[Measured], base: &Base) -> Verdict {
    let mut offences = judge_tree(ratchet, files);
    let base_note = match base {
        Base::Unknown(why) => format!("unknown — {why}"),
        Base::Copy { rev, ratchet: was } => {
            let tracked: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
            offences.extend(judge_base(ratchet, was, &tracked));
            format!("compared with {rev}")
        }
    };
    Verdict {
        over: files.iter().filter(|file| file.lines > ratchet.cap).count(),
        listed: ratchet.entries.len(),
        cap: ratchet.cap,
        offences,
        base: base_note,
    }
}

/// **The phase of `bar check`, `bar report` and `bar dev`**, run first. It
/// fails the summary on any offence and on any measurement it could not make,
/// and it skips none of the phases after it.
pub fn phase(summary: &mut Summary) {
    let measured = measure().and_then(|files| Ok((files, load(Path::new(RATCHET_FILE))?)));
    let (files, ratchet) = match measured {
        Ok(both) => both,
        Err(refusal) => {
            summary.phase_failed("size", &format!("not measured — {refusal}"));
            return;
        }
    };
    let verdict = judge(&ratchet, &files, &read_base());
    let counts = format!(
        "{} files over {} lines, {} listed in {RATCHET_FILE}; base: {}",
        verdict.over, verdict.cap, verdict.listed, verdict.base
    );
    if verdict.offences.is_empty() {
        summary.phase_ok("size", &counts);
        return;
    }
    summary.phase_failed(
        "size",
        &format!("{} offence(s) — {counts}", verdict.offences.len()),
    );
    for offence in &verdict.offences {
        summary.note(&format!("  {}", offence.line()));
    }
}

/// **`bar size`**: the check alone. `--write` lowers or deletes stale entries.
/// `--init` writes the list from the tree, once, and refuses when one exists.
pub fn run(args: &[String]) -> std::io::Result<std::process::ExitCode> {
    use std::process::ExitCode;
    let usage = || {
        eprintln!("usage: bar size [--write | --init]");
        Ok(ExitCode::from(2))
    };
    let mode = match args {
        [] => None,
        [flag] if flag == "--write" || flag == "--init" => Some(flag.as_str()),
        _ => return usage(),
    };
    let path = Path::new(RATCHET_FILE);
    let fail = |message: String| {
        eprintln!("bar size: {message}");
        Ok(ExitCode::FAILURE)
    };
    let files = match measure() {
        Ok(files) => files,
        Err(refusal) => return fail(format!("not measured — {refusal}")),
    };
    match mode {
        Some("--init") => {
            if path.exists() {
                return fail(format!(
                    "{RATCHET_FILE} exists; --init writes the first list only"
                ));
            }
            let ratchet = from_tree(&files);
            std::fs::write(path, ratchet.render())?;
            println!(
                "wrote {RATCHET_FILE}: cap {}, {} entries",
                ratchet.cap,
                ratchet.entries.len()
            );
            return Ok(ExitCode::SUCCESS);
        }
        Some(_) => {
            let ratchet = match load(path) {
                Ok(ratchet) => ratchet,
                Err(refusal) => return fail(refusal),
            };
            let after = lowered(&ratchet, &files);
            if after != ratchet {
                std::fs::write(path, after.render())?;
                println!("rewrote {RATCHET_FILE}: lowered or deleted the stale entries");
            }
        }
        None => {}
    }
    let mut summary = Summary::new();
    phase(&mut summary);
    print!("{}", summary.render());
    Ok(if summary.green {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, lines: usize, raised_by: Option<u32>) -> Entry {
        Entry {
            path: path.to_string(),
            lines,
            raised_by,
        }
    }

    fn list(entries: Vec<Entry>) -> Ratchet {
        Ratchet { cap: 2000, entries }
    }

    fn file(path: &str, lines: usize) -> Measured {
        Measured {
            path: path.to_string(),
            lines,
        }
    }

    fn tracked(files: &[Measured]) -> HashSet<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    /// **The list the repo ships parses**, and is sorted, so a hand edit that
    /// breaks the shape fails here and not only in the first run that reads it.
    #[test]
    fn the_shipped_list_parses_and_renders_as_it_is_written() {
        let text = include_str!("../../../size-ratchet.toml");
        let ratchet = Ratchet::parse(text).expect("the shipped size-ratchet.toml parses");
        assert_eq!(ratchet.cap, INITIAL_CAP);
        assert!(!ratchet.entries.is_empty());
        assert!(
            ratchet.entries.iter().all(|e| e.lines > ratchet.cap),
            "an entry at or under the cap is stale"
        );
        let again = Ratchet::parse(&ratchet.render()).expect("a rendered list parses");
        assert_eq!(again, ratchet);
    }

    #[test]
    fn the_parser_refuses_each_malformed_shape_by_line() {
        for (text, wants) in [
            ("[[file]]\npath = \"a.rs\"\nlines = 1\n", "no cap"),
            ("cap = x\n", "line 1: cap must be a whole number"),
            ("cap = 1\ncap = 2\n", "line 2: cap is given twice"),
            (
                "cap = 1\n[[file]]\ncap = 2\n",
                "line 3: cap must come before",
            ),
            ("cap = 1\n[[file]]\npath = a.rs\n", "line 3: path must be"),
            (
                "cap = 1\n[[file]]\npath = \"a.rs\"\n",
                "line 2: the entry has no lines",
            ),
            (
                "cap = 1\n[[file]]\nlines = 3\n",
                "line 2: the entry has no path",
            ),
            ("cap = 1\n[[bot]]\n", "line 2: unknown table"),
            ("cap = 1\n[[file]]\nsize = 3\n", "line 3: unknown key size"),
            (
                "cap = 1\n[[file]]\npath = \"b.rs\"\nlines = 3\n[[file]]\npath = \"a.rs\"\nlines = 3\n",
                "line 5: a.rs is out of order after b.rs",
            ),
            (
                "cap = 1\n[[file]]\npath = \"a.rs\"\nlines = 3\n[[file]]\npath = \"a.rs\"\nlines = 3\n",
                "line 5: a.rs is listed twice",
            ),
            (
                "cap = 1\n[[file]]\npath = \"a.rs\"\nlines = 3\nraised_by = abc\n",
                "line 5: raised_by",
            ),
        ] {
            let refusal = Ratchet::parse(text).expect_err(text);
            assert!(refusal.contains(wants), "{text:?}: {refusal}");
        }
    }

    #[test]
    fn lines_are_counted_as_str_lines_counts_them() {
        for text in [
            "",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "\n",
            "a\r\nb\r\n",
            "a\n\n\n",
        ] {
            assert_eq!(
                count_lines(text.as_bytes()),
                text.lines().count(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_listed_file_over_its_entry_grew_and_an_unlisted_one_over_the_cap_is_new() {
        let ratchet = list(vec![entry("homer.rs", 2100, None)]);
        let files = [
            file("homer.rs", 2101),
            file("marge.rs", 2001),
            file("lisa.rs", 2000),
        ];
        let offences = judge_tree(&ratchet, &files);
        assert_eq!(
            offences,
            vec![
                Offence::Grew {
                    path: "homer.rs".into(),
                    entry: 2100,
                    now: 2101
                },
                Offence::New {
                    path: "marge.rs".into(),
                    now: 2001
                },
            ]
        );
    }

    #[test]
    fn a_file_at_its_entry_passes_and_one_at_the_cap_is_not_over() {
        let ratchet = list(vec![entry("homer.rs", 2100, None)]);
        let files = [file("homer.rs", 2100), file("lisa.rs", 2000)];
        assert!(judge_tree(&ratchet, &files).is_empty());
    }

    #[test]
    fn an_entry_that_is_gone_at_the_cap_or_above_its_file_is_stale() {
        let ratchet = list(vec![
            entry("bart.rs", 2100, None),
            entry("homer.rs", 2100, None),
            entry("lisa.rs", 2100, None),
            entry("marge.rs", 2100, None),
        ]);
        let files = [
            file("homer.rs", 2000),
            file("lisa.rs", 2050),
            file("marge.rs", 2100),
        ];
        let reasons: Vec<(String, String)> = judge_tree(&ratchet, &files)
            .into_iter()
            .map(|o| match o {
                Offence::Stale { path, reason, .. } => (path, reason),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(reasons.len(), 3);
        assert_eq!(
            reasons[0],
            ("bart.rs".to_string(), "the file is gone".to_string())
        );
        assert!(reasons[1].1.contains("at or under the cap"), "{reasons:?}");
        assert!(reasons[2].1.contains("holds 2050 lines"), "{reasons:?}");
    }

    #[test]
    fn the_cap_may_only_fall() {
        let now = Ratchet {
            cap: 2500,
            entries: vec![],
        };
        let base = list(vec![]);
        let files: [Measured; 0] = [];
        assert_eq!(
            judge_base(&now, &base, &tracked(&files)),
            vec![Offence::CapRaised {
                base: 2000,
                now: 2500
            }]
        );
        let lower = Ratchet {
            cap: 1500,
            entries: vec![],
        };
        assert!(judge_base(&lower, &base, &tracked(&files)).is_empty());
    }

    /// **An entry rises only under a card the base entry did not name.** The
    /// same raised number passes with a new card and fails with none, and with
    /// the card the base already carried.
    #[test]
    fn an_entry_rises_only_under_a_new_card() {
        let base = list(vec![entry("homer.rs", 2100, Some(7))]);
        let files = [file("homer.rs", 2150)];
        for (raised_by, passes) in [(None, false), (Some(7), false), (Some(8), true)] {
            let now = list(vec![entry("homer.rs", 2150, raised_by)]);
            assert_eq!(
                judge_base(&now, &base, &tracked(&files)).is_empty(),
                passes,
                "{raised_by:?}"
            );
        }
        let lower = list(vec![entry("homer.rs", 2050, None)]);
        assert!(judge_base(&lower, &base, &tracked(&files)).is_empty());
    }

    /// **A new entry is a move, or it is refused.** The old file left the tree
    /// and the new entry is no larger; a larger one needs a card of its own.
    #[test]
    fn a_new_entry_is_allowed_only_as_the_move_of_a_file_that_left() {
        let base = list(vec![entry("homer.rs", 2100, None)]);
        let moved = [file("marge.rs", 2100)];
        let kept = [file("homer.rs", 2100), file("marge.rs", 2100)];
        let at = |lines, raised_by| list(vec![entry("marge.rs", lines, raised_by)]);

        assert!(judge_base(&at(2100, None), &base, &tracked(&moved)).is_empty());
        assert!(judge_base(&at(2090, None), &base, &tracked(&moved)).is_empty());
        assert_eq!(
            judge_base(&at(2101, None), &base, &tracked(&moved)),
            vec![Offence::Added {
                path: "marge.rs".into(),
                now: 2101
            }]
        );
        assert!(judge_base(&at(2101, Some(9)), &base, &tracked(&moved)).is_empty());
        assert_eq!(
            judge_base(&at(2100, None), &base, &tracked(&kept)).len(),
            1,
            "the old file is still tracked, so this is a copy"
        );
    }

    #[test]
    fn one_departed_entry_explains_one_new_entry() {
        let base = list(vec![entry("homer.rs", 2100, None)]);
        let now = list(vec![
            entry("lisa.rs", 2050, None),
            entry("marge.rs", 2050, None),
        ]);
        let files = [file("lisa.rs", 2050), file("marge.rs", 2050)];
        assert_eq!(judge_base(&now, &base, &tracked(&files)).len(), 1);
    }

    #[test]
    fn write_lowers_or_deletes_and_never_adds_or_raises() {
        let ratchet = list(vec![
            entry("bart.rs", 2100, None),
            entry("homer.rs", 2100, Some(3)),
            entry("lisa.rs", 2100, None),
            entry("marge.rs", 2100, None),
        ]);
        let files = [
            file("homer.rs", 2040),
            file("lisa.rs", 1999),
            file("marge.rs", 2200),
            file("maggie.rs", 3000),
        ];
        let after = lowered(&ratchet, &files);
        assert_eq!(
            after.entries,
            vec![
                entry("homer.rs", 2040, Some(3)),
                entry("marge.rs", 2100, None)
            ]
        );
    }

    #[test]
    fn init_lists_every_file_over_the_cap_sorted() {
        let files = [
            file("marge.rs", 2001),
            file("lisa.rs", 2000),
            file("homer.rs", 5000),
        ];
        let ratchet = from_tree(&files);
        assert_eq!(
            ratchet.entries,
            vec![entry("homer.rs", 5000, None), entry("marge.rs", 2001, None)]
        );
    }

    #[test]
    fn an_unknown_base_is_said_and_never_called_ok() {
        let ratchet = list(vec![]);
        let verdict = judge(&ratchet, &[], &Base::Unknown("no base".into()));
        assert!(verdict.base.starts_with("unknown"), "{}", verdict.base);
        assert!(!verdict.base.contains("ok"), "{}", verdict.base);
    }
}
