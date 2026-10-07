//! **The tests that are red today and already have a card.**
//!
//! `known-reds.toml` lists them, one entry each. The bar reads the list when it
//! renders a verdict and says, beside a failing test that is on it, which card
//! owns the red. That is information and nothing else: the verdict stays red,
//! the exit code does not change, and no test is retried, skipped or hidden.

use std::path::Path;

/// One test that is known to fail, and who is answerable for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRed {
    pub test: String,
    pub card: u32,
    pub owner: String,
    pub since: String,
}

/// The whole list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownReds {
    entries: Vec<KnownRed>,
}

impl KnownReds {
    /// The entry for a failing test's name, as `cargo test` printed it.
    pub fn find(&self, test: &str) -> Option<&KnownRed> {
        self.entries.iter().find(|entry| entry.test == test)
    }

    /// Read the file's text. Every refusal names the line it is about.
    ///
    /// **Only the shape the file uses is read**: `[[known_red]]` headers, and
    /// under each the keys `test`, `card`, `owner` and `since`, with strings in
    /// double quotes and no escapes. Anything else is refused rather than
    /// guessed at, which is why this holds no dependency on a TOML crate.
    pub fn parse(text: &str) -> Result<KnownReds, String> {
        let mut entries: Vec<(usize, Pending)> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[known_red]]" {
                entries.push((number, Pending::default()));
                continue;
            }
            if line.starts_with('[') {
                return Err(format!("line {number}: unknown table {line}"));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {number}: not a key = value line: {line}"));
            };
            let Some((_, pending)) = entries.last_mut() else {
                return Err(format!(
                    "line {number}: a key before any [[known_red]] header"
                ));
            };
            pending.set(key.trim(), value.trim(), number)?;
        }
        let mut known = KnownReds::default();
        let mut starts: Vec<usize> = Vec::new();
        for (start, pending) in entries {
            let red = pending.finish(start)?;
            if let Some(at) = known.entries.iter().position(|e| e.test == red.test) {
                return Err(format!(
                    "line {start}: {} is already listed at line {}",
                    red.test, starts[at]
                ));
            }
            starts.push(start);
            known.entries.push(red);
        }
        Ok(known)
    }
}

/// An entry while its keys are being read.
#[derive(Default)]
struct Pending {
    test: Option<String>,
    card: Option<u32>,
    owner: Option<String>,
    since: Option<String>,
}

impl Pending {
    fn set(&mut self, key: &str, value: &str, line: usize) -> Result<(), String> {
        let taken =
            |held: bool| held.then(|| format!("line {line}: {key} is given twice in one entry"));
        match key {
            "test" => {
                if let Some(e) = taken(self.test.is_some()) {
                    return Err(e);
                }
                self.test = Some(quoted(key, value, line)?);
            }
            "card" => {
                if let Some(e) = taken(self.card.is_some()) {
                    return Err(e);
                }
                self.card = Some(value.parse().map_err(|_| {
                    format!("line {line}: card must be a whole number, not {value}")
                })?);
            }
            "owner" => {
                if let Some(e) = taken(self.owner.is_some()) {
                    return Err(e);
                }
                self.owner = Some(quoted(key, value, line)?);
            }
            "since" => {
                if let Some(e) = taken(self.since.is_some()) {
                    return Err(e);
                }
                let day = quoted(key, value, line)?;
                if !is_day(&day) {
                    return Err(format!(
                        "line {line}: since must be a day as YYYY-MM-DD, not {day}"
                    ));
                }
                self.since = Some(day);
            }
            other => return Err(format!("line {line}: unknown key {other}")),
        }
        Ok(())
    }

    fn finish(self, start: usize) -> Result<KnownRed, String> {
        let missing = |part: &str| format!("line {start}: the entry has no {part}");
        Ok(KnownRed {
            test: self.test.ok_or_else(|| missing("test"))?,
            card: self.card.ok_or_else(|| missing("card"))?,
            owner: self.owner.ok_or_else(|| missing("owner"))?,
            since: self.since.ok_or_else(|| missing("since"))?,
        })
    }
}

/// A string in double quotes, with nothing inside that needs an escape.
fn quoted(key: &str, value: &str, line: usize) -> Result<String, String> {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .filter(|inner| !inner.contains(['"', '\\']) && !inner.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("line {line}: {key} must be a non-empty string in double quotes"))
}

/// `YYYY-MM-DD`, with a month 1 to 12 and a day 1 to 31.
fn is_day(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    let number = |part: &str, width: usize| {
        (part.len() == width && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    number(year, 4).is_some()
        && number(month, 2).is_some_and(|m| (1..=12).contains(&m))
        && number(day, 2).is_some_and(|d| (1..=31).contains(&d))
}

/// Read the list from `path`. A file that does not exist is an empty list; a
/// file that exists and is malformed is refused, naming the path.
pub fn load(path: &Path) -> Result<KnownReds, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => KnownReds::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(KnownReds::default()),
        Err(e) => Err(format!("{}: cannot be read: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[[known_red]]\n\
        test = \"testing::tests::two_stores_started_together_both_take_a_port\"\n\
        card = 2036\n\
        owner = \"dev3\"\n\
        since = \"2026-10-07\"\n";

    /// **An entry reads back with all four of its parts.**
    #[test]
    fn an_entry_reads_back_with_its_card_owner_and_date() {
        let known = KnownReds::parse(ENTRY).expect("a well-formed file parses");
        let red = known
            .find("testing::tests::two_stores_started_together_both_take_a_port")
            .expect("the listed test is found");
        assert_eq!(red.card, 2036);
        assert_eq!(red.owner, "dev3");
        assert_eq!(red.since, "2026-10-07");
        assert!(known.find("testing::tests::some_other_test").is_none());
    }

    /// **Comments, blank lines and several entries are read.** The paired
    /// positive for every refusal below: a parser that refused everything
    /// would pass them all.
    #[test]
    fn comments_blank_lines_and_several_entries_are_read() {
        let text = format!(
            "# why this file exists\n\n{ENTRY}\n# the next one\n[[known_red]]\n\
             test = \"b::tests::second\"\ncard = 9\nowner = \"dev2\"\nsince = \"2026-01-31\"\n"
        );
        let known = KnownReds::parse(&text).expect("a well-formed file parses");
        assert_eq!(known.find("b::tests::second").map(|r| r.card), Some(9));
        assert!(
            known
                .find("testing::tests::two_stores_started_together_both_take_a_port")
                .is_some()
        );
    }

    /// **An empty file is an empty list**, not a refusal.
    #[test]
    fn an_empty_file_is_an_empty_list() {
        assert_eq!(
            KnownReds::parse("# nothing yet\n"),
            Ok(KnownReds::default())
        );
    }

    fn refused(text: &str) -> String {
        KnownReds::parse(text).expect_err(text)
    }

    /// **Every malformed shape is refused, and the refusal names the line.**
    #[test]
    fn a_malformed_file_is_refused_with_the_line_it_is_about() {
        // An unknown key, on line 3.
        let err = refused("[[known_red]]\ntest = \"a::b\"\nflavour = \"x\"\n");
        assert!(err.contains("line 3") && err.contains("flavour"), "{err}");
        // A key before any entry, on line 1.
        let err = refused("test = \"a::b\"\n");
        assert!(
            err.contains("line 1") && err.contains("before any"),
            "{err}"
        );
        // An unknown table header, on line 2.
        let err = refused("# c\n[[known_reds]]\n");
        assert!(
            err.contains("line 2") && err.contains("unknown table"),
            "{err}"
        );
        // A card that is not a whole number, on line 3.
        let err = refused("[[known_red]]\ntest = \"a::b\"\ncard = \"2036\"\n");
        assert!(err.contains("line 3") && err.contains("card"), "{err}");
        // A string that is never closed, on line 2.
        let err = refused("[[known_red]]\ntest = \"a::b\n");
        assert!(
            err.contains("line 2") && err.contains("double quotes"),
            "{err}"
        );
        // An empty string, and a string holding a backslash, on line 2.
        let err = refused("[[known_red]]\ntest = \"\"\n");
        assert!(err.contains("line 2") && err.contains("non-empty"), "{err}");
        let err = refused("[[known_red]]\nowner = \"a\\b\"\n");
        assert!(
            err.contains("line 2") && err.contains("double quotes"),
            "{err}"
        );
        // A date that is not a date, on line 5.
        let err = refused(
            "[[known_red]]\ntest = \"a::b\"\ncard = 1\nowner = \"dev\"\nsince = \"yesterday\"\n",
        );
        assert!(err.contains("line 5") && err.contains("since"), "{err}");
        // A month that does not exist, on line 5.
        let err = refused(
            "[[known_red]]\ntest = \"a::b\"\ncard = 1\nowner = \"dev\"\nsince = \"2026-13-01\"\n",
        );
        assert!(err.contains("line 5") && err.contains("since"), "{err}");
        // A key given twice, on line 3.
        let err = refused("[[known_red]]\ntest = \"a::b\"\ntest = \"c::d\"\n");
        assert!(err.contains("line 3") && err.contains("twice"), "{err}");
        // A line that is no key and no header, on line 2.
        let err = refused("[[known_red]]\nthis is not toml\n");
        assert!(
            err.contains("line 2") && err.contains("key = value"),
            "{err}"
        );
    }

    /// **An entry missing a part is refused at the line its entry starts on**,
    /// and says which part.
    #[test]
    fn an_entry_missing_a_part_is_refused_at_its_header() {
        let err =
            refused("# c\n[[known_red]]\ntest = \"a::b\"\ncard = 1\nsince = \"2026-10-07\"\n");
        assert!(err.contains("line 2") && err.contains("owner"), "{err}");
        // The same gap in an entry that is not the last.
        let err = refused(&format!(
            "[[known_red]]\ntest = \"a::b\"\ncard = 1\nsince = \"2026-10-07\"\n\n{ENTRY}"
        ));
        assert!(err.contains("line 1") && err.contains("owner"), "{err}");
    }

    /// **A test listed twice is refused**, naming both entries' lines: two
    /// cards cannot own one red.
    #[test]
    fn a_test_listed_twice_is_refused_naming_both_lines() {
        let err = refused(&format!("{ENTRY}\n{ENTRY}"));
        assert!(err.contains("line 1") && err.contains("line 7"), "{err}");
    }

    /// **A file that is not there is an empty list; one that is there and
    /// broken is refused with its path and line.** Read from disk, so the path
    /// a caller gives is the one that is read.
    #[test]
    fn a_missing_file_is_empty_and_a_broken_one_is_refused_with_its_path() {
        let dir = std::env::temp_dir().join(format!("known-reds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("known-reds.toml");
        assert_eq!(load(&path), Ok(KnownReds::default()));
        std::fs::write(&path, "[[known_red]]\nbogus = 1\n").expect("write");
        let err = load(&path).expect_err("a broken file is refused");
        assert!(
            err.contains("known-reds.toml") && err.contains("line 2"),
            "{err}"
        );
        std::fs::write(&path, ENTRY).expect("write");
        assert_eq!(load(&path).expect("loads").entries.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The list the repository ships is well-formed.** The file is read from
    /// where it lives, so a malformed entry fails here and not at the next
    /// bar run.
    #[test]
    fn the_shipped_list_parses() {
        let text = include_str!("../../../known-reds.toml");
        KnownReds::parse(text).expect("known-reds.toml at the repository root is well-formed");
    }
}
