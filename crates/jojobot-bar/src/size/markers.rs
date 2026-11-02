//! **Owned markers.** A temporary path names the step that retires it. A GAP
//! marker or a tripwire in the user stories names the work item that owns the
//! missing capability, by handle, or says it is an invariant. The bar lists
//! both and refuses what names no owner, or a temporary path that retires at or
//! before the release being cut.
//! A legacy reader and its planted upgrade row name the same case. Removing
//! either side leaves an orphan that the bar lists and refuses.

use super::Measured;
use std::collections::BTreeMap;

const STORIES: &str = "crates/jojobot/tests/user_stories/";

/// The needles are built here so that this file never matches its own scan.
fn temporary() -> String {
    ["TEMPO", "RARY("].concat()
}
fn gap() -> String {
    ["// GA", "P"].concat()
}
fn tripwire() -> String {
    [".has_no_", "verb("].concat()
}

pub(super) fn steps(text: &str) -> Option<Vec<u32>> {
    text.split('.').map(|part| part.parse().ok()).collect()
}

/// A work-queue handle with a name, or the word INVARIANT.
fn names_an_owner(text: &str) -> bool {
    let prefix = ["work", ":q-"].concat();
    let invariant = ["INVARI", "ANT"].concat();
    text.contains(&invariant)
        || text.match_indices(&prefix).any(|(at, _)| {
            text[at + prefix.len()..].starts_with(|c: char| c.is_ascii_alphanumeric())
        })
}

/// Offences in one file. `release` is the release being cut, none outside a
/// release gate: a marker is then only checked for being a step.
pub fn offences_in(path: &str, text: &str, release: Option<&str>) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let stories = path.starts_with(STORIES);
    let cutting = release.and_then(steps);
    for (at, line) in lines.iter().enumerate() {
        let place = format!("{path}:{}", at + 1);
        if let Some((_, rest)) = line.split_once(&temporary()) {
            let named = rest.split(')').next().unwrap_or("");
            match (steps(named), &cutting) {
                (None, _) => found.push(format!("{place}: the step '{named}' is not a step")),
                (Some(own), Some(cutting)) if own <= *cutting => found.push(format!(
                    "{place}: retiring step {named} is at or before release {}",
                    release.unwrap_or_default()
                )),
                _ => {}
            }
        }
        if !stories {
            continue;
        }
        let trimmed = line.trim_start();
        let call = line.contains(&tripwire());
        if !trimmed.starts_with(&gap()) && !call {
            continue;
        }
        // The comment block this line opens, or the one directly above a call.
        let (from, to) = if call {
            let top = (0..at)
                .rev()
                .take_while(|&i| lines[i].trim_start().starts_with("//"))
                .last()
                .unwrap_or(at);
            (top, at + 1)
        } else {
            let end = (at + 1..lines.len())
                .take_while(|&i| lines[i].trim_start().starts_with("//"))
                .last()
                .unwrap_or(at);
            (at, end + 1)
        };
        if !names_an_owner(&lines[from..to].join("\n")) {
            let what = if call { "tripwire" } else { "GAP marker" };
            found.push(format!("{place}: the {what} names no owner"));
        }
    }
    found
}

#[derive(Default)]
struct LegacyPair {
    readers: Vec<String>,
    rows: Vec<String>,
}

fn legacy_markers(
    path: &str,
    text: &str,
    pairs: &mut BTreeMap<String, LegacyPair>,
    found: &mut Vec<String>,
) {
    let reader = ["// LEGACY_", "READER("].concat();
    let row = ["// LEGACY_", "ROW("].concat();
    for (at, line) in text.lines().enumerate() {
        let line = line.trim_start();
        let (is_reader, rest) = if let Some(rest) = line.strip_prefix(&reader) {
            (true, rest)
        } else if let Some(rest) = line.strip_prefix(&row) {
            (false, rest)
        } else {
            continue;
        };
        let place = format!("{path}:{}", at + 1);
        let Some((case, owner)) = rest.split_once(')') else {
            found.push(format!("{place}: legacy marker has no closing parenthesis"));
            continue;
        };
        let case = case.trim();
        if case.is_empty() {
            found.push(format!("{place}: legacy marker names no case"));
            continue;
        }
        if !names_an_owner(owner) {
            found.push(format!("{place}: legacy marker names no owner"));
        }
        let pair = pairs.entry(case.into()).or_default();
        if is_reader {
            pair.readers.push(place);
        } else {
            pair.rows.push(place);
        }
    }
}

/// Every tracked file's offences, for the files the bar already measured.
/// A release that is no step is refused here, naming it, so it cannot let a
/// marker through.
pub fn scan(files: &[Measured], release: Option<&str>) -> Result<Vec<String>, String> {
    if let Some(release) = release.filter(|release| steps(release).is_none()) {
        return Err(format!(
            "BAR_RELEASE '{release}' is not a step, as 9.3.3 is"
        ));
    }
    let mut found = Vec::new();
    let mut pairs = BTreeMap::new();
    for file in files {
        let own = file.path.starts_with("crates/jojobot-bar/src/size");
        if own || !(file.path.ends_with(".rs") || file.path.ends_with(".nix")) {
            continue;
        }
        let text = std::fs::read_to_string(&file.path)
            .map_err(|e| format!("{}: could not be read: {e}", file.path))?;
        found.extend(offences_in(&file.path, &text, release));
        legacy_markers(&file.path, &text, &mut pairs, &mut found);
    }
    for (case, pair) in pairs {
        if pair.rows.is_empty() {
            found.extend(
                pair.readers
                    .iter()
                    .map(|place| format!("{place}: legacy reader '{case}' has no planted row")),
            );
        }
        if pair.readers.is_empty() {
            found.extend(
                pair.rows
                    .iter()
                    .map(|place| format!("{place}: planted row '{case}' has no legacy reader")),
            );
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_sources(reader: &str, rows: &str) -> Vec<String> {
        let root = std::env::temp_dir().join(crate::scratch::name("legacy-pairing"));
        std::fs::create_dir_all(&root).expect("source directory");
        let files = [("reader.rs", reader), ("rows.rs", rows)]
            .into_iter()
            .map(|(name, text)| {
                let path = root.join(name);
                std::fs::write(&path, text).expect("source text");
                Measured {
                    path: path.to_str().expect("source path").into(),
                    lines: text.lines().count(),
                }
            })
            .collect::<Vec<_>>();
        let result = scan(&files, None);
        std::fs::remove_dir_all(&root).expect("remove source directory");
        result.expect("source scan")
    }

    #[test]
    fn legacy_pairing_names_a_reader_without_a_planted_row() {
        let reader = "// LEGACY_READER(alpha): work:q-1001\n";
        let row = "// LEGACY_ROW(alpha): work:q-1001\n";
        let missing = legacy_sources(reader, "");
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert!(missing[0].contains("alpha") && missing[0].contains("reader.rs:1"));
        assert!(legacy_sources(reader, row).is_empty());
    }

    #[test]
    fn legacy_pairing_names_a_planted_row_without_a_reader() {
        let reader = "// LEGACY_READER(beta): work:q-1001\n";
        let row = "// LEGACY_ROW(beta): work:q-1001\n";
        let missing = legacy_sources("", row);
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert!(missing[0].contains("beta") && missing[0].contains("rows.rs:1"));
        assert!(legacy_sources(reader, row).is_empty());
    }

    #[test]
    fn legacy_pairing_keeps_the_owner_requirement_on_both_sides() {
        for (reader, row) in [
            (
                "// LEGACY_READER(gamma)\n",
                "// LEGACY_ROW(gamma): work:q-1001\n",
            ),
            (
                "// LEGACY_READER(gamma): work:q-1001\n",
                "// LEGACY_ROW(gamma)\n",
            ),
        ] {
            let missing = legacy_sources(reader, row);
            assert_eq!(missing.len(), 1, "{missing:?}");
            assert!(missing[0].contains("owner"), "{missing:?}");
        }
    }

    fn story(body: &str) -> Vec<String> {
        offences_in(&format!("{STORIES}x.rs"), body, None)
    }

    #[test]
    fn a_gap_marker_needs_a_handle_and_a_tripwire_takes_its_blocks() {
        let (g, t) = (gap(), tripwire());
        assert_eq!(story(&format!("    {g} a\n    // b\n")).len(), 1);
        let owned = format!("    {g} a\n    // work:q-1001\n    s{t}\"a\", &[]).await;\n");
        assert!(story(&owned).is_empty(), "{:?}", story(&owned));
        let bare = format!("    // plain\n    s{t}\"a\", &[]).await;\n");
        assert_eq!(story(&bare).len(), 1);
        let invariant = format!("    // INVARIANT\n    s{t}\"a\", &[]).await;\n");
        assert!(story(&invariant).is_empty());
        assert_eq!(
            story(&format!("    {g} {} x\n", ["work", ":q-"].concat())).len(),
            1,
            "no name"
        );
    }

    /// **A marker retiring at the release being cut, or before it, is refused.**
    /// Step order is numeric per part and a prefix sorts first.
    #[test]
    fn a_temporary_marker_is_refused_at_or_before_the_release_being_cut() {
        let m = |s: &str| format!("// {}{s}): x\n", temporary());
        let refused = |release: Option<&str>, s: &str| offences_in("a.rs", &m(s), release).len();
        for (release, marker, expected) in [
            ("9.3.3", "9.3.2", 1),
            ("9.3.3", "9.3.3", 1),
            ("9.3.3", "9.3.4", 0),
            ("9.3.3", "9.3.10", 0),
            ("9.3.2.1", "9.3.2", 1),
            ("9.3.2.1", "9.3.3", 0),
        ] {
            assert_eq!(
                refused(Some(release), marker),
                expected,
                "release {release}, marker {marker}"
            );
        }
        assert_eq!(refused(None, "9.3.0"), 0, "no release");
        assert_eq!(refused(None, "soon"), 1, "no release, not a step");
        assert_eq!(refused(Some("9.3.3"), "soon"), 1, "a release, not a step");
    }

    /// **A release of one part is a step, and orders after every longer step
    /// that starts with a smaller part.** The order is numeric per part, never
    /// the order of the text.
    #[test]
    fn a_one_part_release_is_a_step_and_orders_after_9_3_10() {
        let m = |s: &str| format!("// {}{s}): x\n", temporary());
        assert!(scan(&[], Some("10")).is_ok());
        assert_eq!(offences_in("a.rs", &m("9.3.10"), Some("10")).len(), 1);
        assert!(offences_in("a.rs", &m("10"), Some("9.3.10")).is_empty());
        assert_eq!(offences_in("a.rs", &m("10"), Some("10")).len(), 1);
    }

    #[test]
    fn a_release_that_is_no_step_fails_the_scan_and_names_the_value() {
        let error = scan(&[], Some("soon")).expect_err("not a step");
        assert!(error.contains("soon"), "{error}");
        assert!(scan(&[], Some("9.3.3")).is_ok());
        assert!(scan(&[], None).is_ok());
    }
}
