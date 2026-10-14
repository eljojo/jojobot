//! **Two upgrade defects that review alone used to catch**, as tests.
//!
//! **A migration that changes several things while its landed check names one.**
//! This store commits DDL clause by clause and does not roll it back. A file
//! with several clauses that dies partway leaves some of them, and a start asks
//! the landed check whether the file landed: the check names one object, so the
//! answer is wrong for the rest and the re-issued file fails on what is already
//! there, on every start after. [`migration_findings`] reads each file's
//! statement and requires one object, named by its landed check.
//!
//! **A one-time store pass whose code changes while its name stays.** A store
//! where the old pass already finished holds the name in `store_pass`, so the
//! new code never runs there. [`pass_findings`] hashes the code that drives each
//! pass and compares it with a checked-in lock.
//!
//! Both read text and nothing else: no store, no server.

use super::Leaves;

/// **What one clause of a statement does to the schema.**
#[derive(Debug, Clone, PartialEq, Eq)]
enum Touch {
    Table(String),
    NoTable(String),
    Column(String, String),
    ColumnType(String, String),
    Index(String, String),
    NoIndex(String, String),
    Rows(String),
}

impl Touch {
    fn describe(&self) -> String {
        match self {
            Touch::Table(t) => format!("creates table {t}"),
            Touch::NoTable(t) => format!("drops table {t}"),
            Touch::Column(t, c) => format!("adds column {t}.{c}"),
            Touch::ColumnType(t, c) => format!("changes the type of {t}.{c}"),
            Touch::Index(t, i) => format!("adds index {i} on {t}"),
            Touch::NoIndex(t, i) => format!("drops index {i} on {t}"),
            Touch::Rows(t) => format!("changes rows of {t}"),
        }
    }
}

/// **What a landed check asks about**, in the same terms as a clause.
fn asked_by(leaves: Leaves) -> Touch {
    match leaves {
        Leaves::Table(t) => Touch::Table(t.into()),
        Leaves::NoTable(t) => Touch::NoTable(t.into()),
        Leaves::Column(t, c) => Touch::Column(t.into(), c.into()),
        Leaves::ColumnType(t, c, _) => Touch::ColumnType(t.into(), c.into()),
        Leaves::Index(t, i) => Touch::Index(t.into(), i.into()),
        Leaves::NoIndex(t, i) => Touch::NoIndex(t.into(), i.into()),
        Leaves::NoRows(t, _) => Touch::Rows(t.into()),
    }
}

/// The statement without its comment lines and without a trailing comment.
fn without_comments(sql: &str) -> String {
    sql.lines()
        .map(|line| {
            let mut quoted = false;
            let mut cut = line.len();
            let bytes = line.as_bytes();
            for (at, byte) in bytes.iter().enumerate() {
                match byte {
                    b'\'' => quoted = !quoted,
                    b'-' if !quoted && bytes.get(at + 1) == Some(&b'-') => {
                        cut = at;
                        break;
                    }
                    _ => {}
                }
            }
            &line[..cut]
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An identifier as the statement spells it: no quotes, no `(`, no `,`.
fn identifier(word: &str) -> String {
    word.split('(')
        .next()
        .unwrap_or(word)
        .trim_matches(|c| c == '`' || c == ',' || c == ';')
        .to_string()
}

/// Split at the commas that are not inside parentheses.
fn top_level_clauses(text: &str) -> Vec<String> {
    let mut clauses = vec![String::new()];
    let mut depth = 0_i32;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                clauses.push(String::new());
                continue;
            }
            _ => {}
        }
        clauses.last_mut().expect("one clause").push(c);
    }
    clauses
        .into_iter()
        .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|c| !c.is_empty())
        .collect()
}

fn alter_clause(table: &str, clause: &str) -> Result<Touch, String> {
    let words: Vec<&str> = clause.split_whitespace().collect();
    let up: Vec<String> = words.iter().map(|w| w.to_uppercase()).collect();
    let up: Vec<&str> = up.iter().map(String::as_str).collect();
    let t = table.to_string();
    let found = match up.as_slice() {
        ["ADD", "COLUMN", ..] => Touch::Column(t, identifier(words[2])),
        ["ADD", "PRIMARY", "KEY", ..] => Touch::Index(t, "PRIMARY".into()),
        ["ADD", "UNIQUE", "INDEX" | "KEY", ..] => Touch::Index(t, identifier(words[3])),
        ["ADD", "INDEX" | "KEY", ..] => Touch::Index(t, identifier(words[2])),
        ["DROP", "PRIMARY", "KEY"] => Touch::NoIndex(t, "PRIMARY".into()),
        ["DROP", "INDEX" | "KEY", ..] => Touch::NoIndex(t, identifier(words[2])),
        ["MODIFY", "COLUMN", ..] => Touch::ColumnType(t, identifier(words[2])),
        ["MODIFY", ..] => Touch::ColumnType(t, identifier(words[1])),
        ["RENAME", "COLUMN", _, "TO", ..] => Touch::Column(t, identifier(words[4])),
        _ => return Err(format!("the ALTER clause `{clause}`")),
    };
    Ok(found)
}

/// **Every clause of every statement in a file, as what it touches.** A shape
/// this does not read is an error, never an empty list: a lint that skips what
/// it cannot read passes the files that most need it.
fn touches(sql: &str) -> Result<Vec<Touch>, String> {
    let mut found = Vec::new();
    for statement in without_comments(sql).split(';') {
        let statement = statement.split_whitespace().collect::<Vec<_>>().join(" ");
        if statement.is_empty() {
            continue;
        }
        let words: Vec<&str> = statement.split_whitespace().collect();
        let up = statement.to_uppercase();
        if up.starts_with("CREATE TABLE") {
            let at = if up.starts_with("CREATE TABLE IF NOT EXISTS") {
                5
            } else {
                2
            };
            found.push(Touch::Table(identifier(words[at])));
        } else if up.starts_with("DROP TABLE") {
            found.push(Touch::NoTable(identifier(words[2])));
        } else if up.starts_with("CREATE INDEX") || up.starts_with("CREATE UNIQUE INDEX") {
            let at = if up.starts_with("CREATE UNIQUE") {
                3
            } else {
                2
            };
            found.push(Touch::Index(
                identifier(words[at + 2]),
                identifier(words[at]),
            ));
        } else if up.starts_with("DROP INDEX") {
            found.push(Touch::NoIndex(identifier(words[4]), identifier(words[2])));
        } else if up.starts_with("ALTER TABLE") {
            let table = identifier(words[2]);
            let rest = words[3..].join(" ");
            for clause in top_level_clauses(&rest) {
                found.push(alter_clause(&table, &clause)?);
            }
        } else if up.starts_with("UPDATE") {
            found.push(Touch::Rows(identifier(words[1])));
        } else if up.starts_with("DELETE FROM") || up.starts_with("INSERT INTO") {
            found.push(Touch::Rows(identifier(words[2])));
        } else {
            let shown: String = statement.chars().take(48).collect();
            return Err(format!("the statement `{shown}`"));
        }
    }
    Ok(found)
}

/// **The files before 0080 that break the rule, by version name.** The list
/// only shrinks: a file at or after 0080 is never on it, and an entry that no
/// longer breaks the rule, or names no migration, is itself a finding.
const KNOWN_BEFORE_0080: &[&str] = &["0035_fact_write_backfill", "0074_session_key_hash"];

/// The first version that may not be on the known list.
const NO_NEW_KNOWN_FROM: &str = "0080";

/// **What is wrong with a list of migrations**, one sentence each, each saying
/// what to do. Empty when every file touches one object and its landed check
/// names that object. The input is `(version, sql, landed check)`.
fn migration_findings(migrations: &[(&str, &str, Leaves)], known: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    let mut breaking: Vec<&str> = Vec::new();
    for (version, sql, leaves) in migrations {
        let asked = asked_by(*leaves);
        let problem = match touches(sql) {
            Err(shape) => Some(format!(
                "{version}: this lint cannot read {shape}. Teach the lint that shape in the \
                 same commit, because a file it cannot read is a file it cannot check."
            )),
            Ok(touched) if touched.len() != 1 => Some(format!(
                "{version}: the file has {} changes ({}) and its landed check asks only \
                 about: {}. A start after a death partway through cannot tell which of them \
                 landed, and the re-issued file fails on the ones that did. Split it into one \
                 file for each change, each with a landed check that names that change.",
                touched.len(),
                touched
                    .iter()
                    .map(Touch::describe)
                    .collect::<Vec<_>>()
                    .join("; "),
                asked.describe()
            )),
            Ok(touched) if touched[0] != asked => Some(format!(
                "{version}: the file {} but its landed check asks about: {}. Make the check \
                 name the object the file changes.",
                touched[0].describe(),
                asked.describe()
            )),
            Ok(_) => None,
        };
        if let Some(problem) = problem {
            breaking.push(version);
            if !known.contains(version) {
                findings.push(problem);
            }
        }
    }
    for entry in known {
        if *entry >= NO_NEW_KNOWN_FROM {
            findings.push(format!(
                "{entry}: the known list takes no file from {NO_NEW_KNOWN_FROM} on. Fix the \
                 file instead of listing it."
            ));
        } else if !migrations.iter().any(|(version, ..)| version == entry) {
            findings.push(format!(
                "{entry}: the known list names a migration that does not exist. Remove the \
                 entry."
            ));
        } else if !breaking.contains(entry) {
            findings.push(format!(
                "{entry}: the known list holds a file that now follows the rule. Remove the \
                 entry, so the list only shrinks."
            ));
        }
    }
    findings
}

/// **One store pass as the source declares it**: the constant, the name stores
/// record, and a hash of the code that drives it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pass {
    constant: String,
    name: String,
    code_hash: u64,
}

impl Pass {
    /// The line the lock holds for this pass.
    fn lock_line(&self) -> String {
        format!(
            "{} | {} | {:016x}",
            self.constant, self.name, self.code_hash
        )
    }
}

/// FNV-1a over the bytes: stable across builds and machines, which the
/// standard library's hasher does not promise.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

fn starts_a_function(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("pub(crate) ")
        .or_else(|| trimmed.strip_prefix("pub "))
        .unwrap_or(trimmed);
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    rest.starts_with("fn ").then(|| line.len() - trimmed.len())
}

/// **The function that first uses a pass's constant, as code.** The nearest
/// `fn` above the first use, to the line that closes it at the same
/// indentation, with comment lines left out and each line trimmed: a change to
/// a comment or to spacing does not change what the pass does.
fn driver_code(text: &str, constant: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let defined = lines
        .iter()
        .position(|l| !is_comment(l) && l.contains("const ") && l.contains(constant))
        .ok_or_else(|| format!("{constant} is not defined in the source"))?;
    let used = lines
        .iter()
        .enumerate()
        .skip(defined + 1)
        .find(|(_, l)| !is_comment(l) && l.contains(constant))
        .map(|(at, _)| at)
        .ok_or_else(|| format!("{constant} is defined and used nowhere"))?;
    let (start, indent) = (0..=used)
        .rev()
        .find_map(|at| starts_a_function(lines[at]).map(|indent| (at, indent)))
        .ok_or_else(|| format!("{constant} is used outside any function"))?;
    let closing = format!("{}}}", " ".repeat(indent));
    let end = (start..lines.len())
        .find(|&at| lines[at] == closing)
        .ok_or_else(|| format!("the function using {constant} never closes"))?;
    Ok(lines[start..=end]
        .iter()
        .filter(|l| !is_comment(l))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// **Every pass a source declares**: a `const NAME_PASS: &str = "...";`.
fn passes_in(text: &str) -> Result<Vec<Pass>, String> {
    let mut passes = Vec::new();
    for line in text.lines().filter(|l| !is_comment(l)) {
        let Some((head, value)) = line.split_once(": &str = ") else {
            continue;
        };
        let Some(constant) = head.trim().rsplit(' ').next() else {
            continue;
        };
        if !head.contains("const ") || !constant.ends_with("_PASS") {
            continue;
        }
        let name = value.trim().trim_end_matches(';').trim_matches('"');
        passes.push(Pass {
            constant: constant.to_string(),
            name: name.to_string(),
            code_hash: fnv1a(&driver_code(text, constant)?),
        });
    }
    Ok(passes)
}

/// The lock as `(constant, name, hash)` lines; `#` lines are comments.
fn lock_lines(lock: &str) -> Vec<(String, String, String)> {
    lock.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.split('|').map(str::trim);
            Some((
                parts.next()?.to_string(),
                parts.next()?.to_string(),
                parts.next()?.to_string(),
            ))
        })
        .collect()
}

/// **What is wrong between the passes in the sources and the lock.** Each
/// finding says what to do.
fn pass_findings(sources: &[&str], lock: &str) -> Vec<String> {
    let locked = lock_lines(lock);
    let mut findings = Vec::new();
    let mut seen = Vec::new();
    for source in sources {
        let passes = match passes_in(source) {
            Ok(passes) => passes,
            Err(reason) => {
                findings.push(format!(
                    "the pass lint cannot read a source: {reason}. Teach the lint the shape in \
                     the same commit."
                ));
                continue;
            }
        };
        for pass in passes {
            seen.push(pass.constant.clone());
            let now = pass.lock_line();
            match locked
                .iter()
                .find(|(constant, ..)| *constant == pass.constant)
            {
                None => findings.push(format!(
                    "{} has no line in the lock. Add this line: {now}",
                    pass.constant
                )),
                Some((_, name, _)) if *name != pass.name => findings.push(format!(
                    "{} is now named {:?} and the lock says {name:?}. A new name makes stores \
                     that ran the old pass run this one, which is the point of renaming it. \
                     Update the lock line to: {now}",
                    pass.constant, pass.name
                )),
                Some((_, _, hash)) if *hash != format!("{:016x}", pass.code_hash) => {
                    findings.push(format!(
                        "{} ({:?}) changed: the code that drives it is not the code the lock \
                         holds. A store where the old pass already finished never runs the new \
                         one. Give it a new name so those stores run it once; or, if running \
                         it again is unnecessary, update the lock line to `{now}` and say why \
                         in the commit.",
                        pass.constant, pass.name
                    ));
                }
                Some(_) => {}
            }
        }
    }
    for (constant, ..) in &locked {
        if !seen.contains(constant) {
            findings.push(format!(
                "the lock names {constant} and no source declares it. Remove the line."
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::migrate::MIGRATIONS;

    const MEMORY_SOURCE: &str = include_str!("../memory.rs");
    const STORE_SOURCE: &str = include_str!("../../dolt.rs");
    const PASS_LOCK: &str = include_str!("pass-lock.txt");

    /// **The migration that carried thirteen columns under a check that named
    /// the last of them**, as the repository held it before it was split: the
    /// file is the real one and the check is the one its entry gave.
    const OLD_0079: &str = include_str!("fixtures/0079_entity_write_snapshot.sql");

    fn real_migrations() -> Vec<(&'static str, &'static str, Leaves)> {
        MIGRATIONS
            .iter()
            .map(|m| (m.version, m.sql, m.leaves))
            .collect()
    }

    fn real_findings() -> Vec<String> {
        migration_findings(&real_migrations(), KNOWN_BEFORE_0080)
    }

    /// **The files in the repository follow the rule**, but for the ones the
    /// known list names.
    #[test]
    fn every_migration_changes_one_object_and_its_landed_check_names_it() {
        let findings = real_findings();
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// **A file adding two columns under a check for one is refused**, and the
    /// finding names the file, both columns and the check.
    #[test]
    fn a_file_adding_two_columns_under_a_check_for_one_is_refused() {
        let two =
            "ALTER TABLE ledger\n    ADD COLUMN alpha INT NULL,\n    ADD COLUMN beta INT NULL;";
        let findings = migration_findings(
            &[("0090_two_columns", two, Leaves::Column("ledger", "beta"))],
            &[],
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        let finding = &findings[0];
        assert!(finding.starts_with("0090_two_columns:"), "{finding}");
        assert!(finding.contains("ledger.alpha"), "{finding}");
        assert!(finding.contains("ledger.beta"), "{finding}");
        assert!(finding.contains("Split it into one file"), "{finding}");
    }

    /// **The 0079 shape, taken from the repository's own history, is refused**:
    /// thirteen columns in one file and a check that names the last of them.
    #[test]
    fn the_shape_0079_had_before_it_was_split_is_refused() {
        let findings = migration_findings(
            &[(
                "0079_entity_write_snapshot",
                OLD_0079,
                Leaves::Column("entity_write", "session"),
            )],
            &[],
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("13 changes"), "{}", findings[0]);
        assert!(findings[0].contains("entity_write.kind"), "{}", findings[0]);
        assert!(findings[0].contains("Split it"), "{}", findings[0]);
    }

    /// **A file whose one change is not the one its check asks about is
    /// refused**, and the finding says which is which.
    #[test]
    fn a_file_whose_check_names_another_object_is_refused() {
        let one = "ALTER TABLE ledger ADD COLUMN alpha INT NULL;";
        let findings = migration_findings(
            &[("0091_other", one, Leaves::Column("ledger", "beta"))],
            &[],
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("ledger.alpha"), "{}", findings[0]);
        assert!(findings[0].contains("ledger.beta"), "{}", findings[0]);
        assert!(findings[0].contains("Make the check"), "{}", findings[0]);
    }

    /// **A statement the lint cannot read is a finding**, not a pass.
    #[test]
    fn a_statement_the_lint_cannot_read_is_a_finding_not_a_pass() {
        let findings = migration_findings(
            &[(
                "0092_odd",
                "TRUNCATE TABLE ledger;",
                Leaves::Table("ledger"),
            )],
            &[],
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("cannot read"), "{}", findings[0]);
    }

    /// **A file on the known list is not a finding while it still breaks the
    /// rule**, and the list takes no file from 0080 on, no entry that follows
    /// the rule and no entry naming nothing.
    #[test]
    fn the_known_list_only_shrinks() {
        let two = "ALTER TABLE ledger ADD COLUMN alpha INT NULL, ADD COLUMN beta INT NULL;";
        let one = "ALTER TABLE ledger ADD COLUMN alpha INT NULL;";
        let migrations = [
            ("0010_old_two", two, Leaves::Column("ledger", "beta")),
            ("0011_old_one", one, Leaves::Column("ledger", "alpha")),
            ("0085_new_two", two, Leaves::Column("ledger", "beta")),
        ];
        let findings = migration_findings(
            &migrations,
            &["0010_old_two", "0011_old_one", "0085_new_two", "0012_gone"],
        );
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("0085_new_two:") && f.contains("takes no file")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("0011_old_one:") && f.contains("now follows")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("0012_gone:") && f.contains("does not exist")),
            "{findings:?}"
        );
    }

    /// **Every shape the repository's migrations use is read**: no file is
    /// unreadable to the lint.
    #[test]
    fn the_lint_reads_every_migration_in_the_repository() {
        for (version, sql, _) in real_migrations() {
            assert!(touches(sql).is_ok(), "{version}: {:?}", touches(sql));
        }
    }

    /// **The passes in the sources match the lock.**
    #[test]
    fn every_store_pass_matches_the_lock() {
        let findings = pass_findings(&[MEMORY_SOURCE, STORE_SOURCE], PASS_LOCK);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// **Editing the body of a pass without renaming it is refused**, and the
    /// finding says to rename, or to update the lock and say why.
    #[test]
    fn editing_a_pass_without_renaming_it_is_refused() {
        let site = "report.settled = true;";
        // The edit starts at the forwarded-links pass and changes the first
        // match after it, which is inside that pass's own function.
        let at = MEMORY_SOURCE
            .find("pub async fn migrate_forwarded_links")
            .expect("the pass's function");
        let edited = format!(
            "{}{}",
            &MEMORY_SOURCE[..at],
            MEMORY_SOURCE[at..].replacen(site, "report.settled = false;", 1)
        );
        assert_ne!(edited, MEMORY_SOURCE, "the edit reached the source");
        let findings = pass_findings(&[&edited, STORE_SOURCE], PASS_LOCK);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("FORWARDED_LINKS_PASS"),
            "{}",
            findings[0]
        );
        assert!(
            findings[0].contains("Give it a new name"),
            "{}",
            findings[0]
        );
        assert!(findings[0].contains("say why"), "{}", findings[0]);
    }

    /// **A change to a comment or to spacing in a pass is not a change to the
    /// pass.**
    #[test]
    fn a_comment_or_spacing_in_a_pass_is_not_a_change() {
        let at = MEMORY_SOURCE
            .find("pub async fn migrate_forwarded_links")
            .expect("the pass's function");
        let edited = format!(
            "{}{}",
            &MEMORY_SOURCE[..at],
            MEMORY_SOURCE[at..].replacen(
                "let mut report = ForwardedLinkReport::default();",
                "// A comment that was not there.\n        let mut report =   ForwardedLinkReport::default();",
                1
            )
        );
        assert_ne!(edited, MEMORY_SOURCE, "the edit reached the source");
        let findings = pass_findings(&[&edited, STORE_SOURCE], PASS_LOCK);
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// **A new pass with no lock line, and a lock line for no pass, are both
    /// findings.**
    #[test]
    fn a_pass_the_lock_lacks_and_a_lock_line_for_no_pass_are_findings() {
        let source = "const NEWEST_PASS: &str = \"the newest\";\n\
                      fn run() {\n    if done(NEWEST_PASS) {}\n}\n";
        let findings = pass_findings(&[source], "GONE_PASS | gone | 0000000000000000\n");
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("NEWEST_PASS has no line")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.contains("GONE_PASS") && f.contains("Remove the line")),
            "{findings:?}"
        );
    }

    /// **A renamed pass whose code also changed says to update the lock to the
    /// new name and hash.**
    #[test]
    fn a_renamed_pass_says_to_update_the_lock_line() {
        let before =
            "const SOME_PASS: &str = \"old name\";\nfn run() {\n    if done(SOME_PASS) {}\n}\n";
        let after = "const SOME_PASS: &str = \"new name\";\nfn run() {\n    if done(SOME_PASS) { more(); }\n}\n";
        let lock = format!("{}\n", passes_in(before).expect("readable")[0].lock_line());
        let findings = pass_findings(&[after], &lock);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("\"new name\""), "{}", findings[0]);
        assert!(
            findings[0].contains("Update the lock line"),
            "{}",
            findings[0]
        );
    }
}
