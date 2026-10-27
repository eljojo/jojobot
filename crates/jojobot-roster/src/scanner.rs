//! The source and commit classifier shared by the local gate and CI.

use std::fs;
use std::path::PathBuf;

use crate::{is_listed, listed};

fn kinds() -> Vec<&'static str> {
    crate::KINDS.to_vec()
}

/// Every `kind:slug` occurrence in the text, comments included.
pub fn handles_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for kind in kinds() {
        let needle = format!("{kind}:");
        for (idx, _) in text.match_indices(&needle) {
            // A word/URL character right before means this is the tail of
            // something longer (e.g. `.org:` in a URL), not a handle.
            if idx > 0 {
                let prev = text.as_bytes()[idx - 1];
                if prev.is_ascii_alphanumeric() || matches!(prev, b'_' | b'-' | b'.') {
                    continue;
                }
            }
            let slug: String = text[idx + needle.len()..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                .collect();
            if slug.is_empty() || !slug.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                continue;
            }
            found.push(format!("{kind}:{slug}"));
        }
    }
    found
}

/// Every handle in the text built out of two separate literals.
///
/// **Deliberately tolerant of whitespace and strict about everything else**: it
/// reads the next quoted token after the anchor, requires it to be a kind, then
/// reads the next quoted token after the separator. A pair that is not a kind
/// followed by a slug is not a handle and is passed over.
pub fn two_part_handles_in(text: &str) -> Vec<String> {
    let known = kinds();
    let mut found = Vec::new();
    for (anchor, separator) in TWO_PART {
        for (at, _) in text.match_indices(anchor) {
            let after = at + anchor.len();
            let Some((kind, ended)) = quoted_from(&text[after..]) else {
                continue;
            };
            if !known.iter().any(|k| *k == kind) {
                continue;
            }
            let rest = &text[after + ended..];
            let Some(sep) = rest.find(separator) else {
                continue;
            };
            // Only across a short gap: the two halves of one call, never the
            // next one down the file.
            if sep > 40 {
                continue;
            }
            let Some((slug, _)) = quoted_from(&rest[sep + separator.len()..]) else {
                continue;
            };
            if slug.is_empty() || !slug.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                continue;
            }
            if slug
                .chars()
                .any(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-')
            {
                continue;
            }
            found.push(format!("{kind}:{slug}"));
        }
    }
    found
}

/// The next quoted token in `text`, and where its closing quote ends.
pub fn quoted_from(text: &str) -> Option<(String, usize)> {
    let opened = text.find('"')?;
    if text[..opened]
        .chars()
        .any(|c| !c.is_whitespace() && c != ',' && c != ':')
    {
        return None;
    }
    let rest = &text[opened + 1..];
    let closed = rest.find('"')?;
    Some((rest[..closed].to_string(), opened + 1 + closed + 1))
}

/// **The two constructors that take a handle apart**, with the literal each
/// one is handed.
///
/// `EntityId::person` takes the slug first, so its argument is the literal that
/// opens the call. `EntityId::new` takes the kind first and the slug second, so
/// the scan walks past the kind and reads the literal after the comma.
///
/// ⚠️ **A call whose KIND is a variable is returned with `kind_written` false.** A
/// kind can be declared by a caller at runtime, and a handle under one carries a
/// token that is not in the domain's set — so the scan for bare slugs does not
/// demand the whole handle there, which would buy nothing. Its slug is checked
/// against the two lists instead, by [`unlisted_slugs_under_a_declared_kind_in`].
pub fn constructor_arguments(text: &str) -> Vec<Argument> {
    let mut found = Vec::new();
    for (call, takes_a_kind) in [("EntityId::person", false), ("EntityId::new", true)] {
        let needle = format!("{call}(");
        for (idx, _) in text.match_indices(&needle) {
            let mut rest = &text[idx + needle.len()..];
            let mut kind_written = true;
            if takes_a_kind {
                let kind = rest.trim_start();
                // The kind written out, rather than one a caller declared and
                // this call resolved: only the first is a token the scan knows.
                kind_written = kind.starts_with("EntityKind::")
                    || kind.starts_with("jojobot_domain::memory::EntityKind::");
                let Some(comma) = rest.find(',') else {
                    continue;
                };
                rest = &rest[comma + 1..];
            }
            let rest = rest.trim_start();
            let Some(literal) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = literal.find('"') else {
                continue;
            };
            found.push(Argument {
                call,
                literal: literal[..end].to_string(),
                kind_written,
                line: text[..idx].matches('\n').count() + 1,
            });
        }
    }
    found
}

/// Every bare slug handed to a handle constructor, as `call in path`.
///
/// **A slug carries its kind or it does not.** The argument is read as written:
/// one that already holds a colon is a whole handle and the scan above reads
/// it; one that does not is a name nothing compares against the roster.
///
/// A non-literal argument — a variable, a `format!` — is out of reach here and
/// is out of reach of any text scan. What this catches is the literal, which is
/// where a name a person typed actually enters.
pub fn bare_slugs_in(files: &[PathBuf]) -> Vec<String> {
    let mut bare = Vec::new();
    for file in files {
        if file.file_name().is_some_and(|n| n == "fixture_roster.rs") {
            continue;
        }
        let text = fs::read_to_string(file).expect("readable source file");
        for Argument {
            call,
            literal: argument,
            kind_written,
            ..
        } in constructor_arguments(&text)
        {
            if !kind_written || argument.contains(':') {
                continue;
            }
            bare.push(format!("{call}(\"{argument}\") in {}", file.display()));
        }
    }
    bare.sort();
    bare.dedup();
    bare
}

/// **Every slug handed to `EntityId::new` under a kind the call does not write
/// out**, which is not on [`crate::ROSTER`] or [`crate::ORDINARY`], as `path:line: "slug"`.
pub fn unlisted_slugs_under_a_declared_kind_in(files: &[PathBuf]) -> Vec<String> {
    let mut refused = Vec::new();
    for file in files {
        if file.file_name().is_some_and(|n| n == "fixture_roster.rs") {
            continue;
        }
        let text = fs::read_to_string(file).expect("readable source file");
        for argument in constructor_arguments(&text) {
            if argument.kind_written || argument.call != "EntityId::new" {
                continue;
            }
            if !is_listed_slug(&argument.literal) {
                refused.push(format!(
                    "{}:{}: \"{}\"",
                    file.display(),
                    argument.line,
                    argument.literal
                ));
            }
        }
    }
    refused.sort();
    refused.dedup();
    refused
}

/// Whether the slug of some handle on [`crate::ROSTER`] or [`crate::ORDINARY`] is this. The kind
/// is not read: a kind a caller declared is not one the domain's set holds, so a
/// slug under one can be compared with the lists by its slug alone.
pub fn is_listed_slug(slug: &str) -> bool {
    crate::GREEK.contains(&slug)
        || listed().any(|entry| {
            entry
                .split_once(':')
                .is_some_and(|(_, listed)| listed == slug)
        })
}

/// **Every off-roster handle in these messages**, as `commit: handle`.
///
/// The same rule the source check uses, asked of a commit message: a handle is
/// handle-shaped text wherever it sits, and a commit message is text on its way
/// into the repository exactly as a comment is. Reuses [`violations_in`]'s own
/// extraction — [`handles_in`] and [`two_part_handles_in`] — against [`crate::ROSTER`],
/// never a second parser.
pub fn off_roster_handles_in(commits: &[(String, String)]) -> Vec<String> {
    let mut violations = Vec::new();
    for (commit, message) in commits {
        for handle in handles_in(message)
            .into_iter()
            .chain(two_part_handles_in(message))
        {
            if !is_listed(&handle) {
                violations.push(format!("{commit}: {handle}"));
            }
        }
    }
    violations.sort();
    violations.dedup();
    violations
}

/// **The constructors that build a handle out of two arguments**, and the
/// reason this is a list rather than a rule.
///
/// [`handles_in`] reads `kind:slug` as text, so it cannot see a name whose two
/// halves never meet: `add_args("thing", "red-kite", …)` and a payload carrying
/// `"kind"` and `"handle"` as separate keys both create an entity the scan
/// never reads. **That is the commonest way a fixture is named**, so the gate
/// that enforces the bright line was blind to the idiom most authors reach for.
///
/// ⛔️ **A rule instead of a list would have to guess which bare string is a
/// slug, and that guess does not work.** Measured over this workspace: flagging
/// any literal following any kind literal reports 38 off-roster names, of which
/// 5 are real — `"session", "timezone"` and `"pet", "owner"` are a migration's
/// table and column, not a handle. **A gate that is wrong six times out of
/// seven is one people learn to wave through**, which costs more than the hole.
///
/// So this names the constructors. **It can go stale, exactly as the roster
/// itself can**, and a new way of building a handle is invisible until somebody
/// adds it here. That is the trade: a list that is sometimes short, against a
/// rule that is usually wrong.
const TWO_PART: &[(&str, &str)] = &[
    // The test helper: `add_args("thing", "red-kite", "Red Kite")`.
    ("add_args(", ","),
    // A payload naming the halves as its own keys.
    ("\"kind\":", "\"handle\":"),
];

/// One literal handed to a handle constructor.
pub struct Argument {
    pub call: &'static str,
    pub literal: String,
    /// Whether the call writes its kind out (`EntityKind::`), so the handle is one
    /// the roster scan can read, or hands a kind a caller declared.
    pub kind_written: bool,
    /// The line the call opens on, counted from 1.
    pub line: usize,
}

/// Every off-roster handle in these files, as `handle in path`.
pub fn violations_in(files: &[PathBuf]) -> std::io::Result<Vec<String>> {
    let mut violations = Vec::new();
    for file in files {
        let text = fs::read_to_string(file)?;
        for handle in handles_in(&text)
            .into_iter()
            .chain(two_part_handles_in(&text))
        {
            if !is_listed(&handle) {
                violations.push(format!("{} in {}", handle, file.display()));
            }
        }
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}
