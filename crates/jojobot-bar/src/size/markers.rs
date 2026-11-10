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

/// A temporary marker the head holds and the release's base does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub place: String,
    pub step: Vec<u32>,
    pub text: String,
}

/// The temporary markers in `head` that `base` does not hold, as many times as
/// the head holds one more than the base. A marker is its own line, so moving it
/// within a file adds nothing.
pub fn added_in(path: &str, head: &str, base: Option<&str>) -> Vec<Added> {
    let needle = temporary();
    let mut held: BTreeMap<&str, usize> = BTreeMap::new();
    for line in base
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains(&needle))
    {
        *held.entry(line.trim()).or_default() += 1;
    }
    let mut added = Vec::new();
    for (at, line) in head.lines().enumerate() {
        let Some((_, rest)) = line.split_once(&needle) else {
            continue;
        };
        if let Some(left) = held.get_mut(line.trim()).filter(|left| **left > 0) {
            *left -= 1;
            continue;
        }
        // A marker that is no step is already an offence of the scan.
        if let Some(step) = steps(rest.split(')').next().unwrap_or("")) {
            added.push(Added {
                place: format!("{path}:{}", at + 1),
                step,
                text: line.trim().into(),
            });
        }
    }
    added
}

/// How many steps `step` lies past `release`: the last part's difference in the
/// same series, one for a sub-step of the release, none at or before it. A step
/// in another series has no distance and is far.
fn steps_out(release: &[u32], step: &[u32]) -> Option<u32> {
    if step <= release {
        return Some(0);
    }
    if step.len() == release.len() + 1 && step.starts_with(release) {
        return Some(1);
    }
    match (release.split_last(), step.split_last()) {
        (Some((r, r_series)), Some((s, s_series))) if r_series == s_series => Some(s - r),
        _ => None,
    }
}

/// The ruling a marker's own line names: a rule number or a work handle.
fn ruling_of(text: &str) -> Option<String> {
    let rule = text
        .match_indices("rule ")
        .map(|(at, _)| &text[at..])
        .find(|rest| rest["rule ".len()..].starts_with(|c: char| c.is_ascii_digit()))
        .map(|rest| {
            let digits = rest["rule ".len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            format!("rule {digits}")
        });
    rule.or_else(|| {
        let prefix = ["work", ":q-"].concat();
        text.match_indices(&prefix).find_map(|(at, _)| {
            let handle: String = text[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-'))
                .collect();
            (handle.len() > prefix.len()).then_some(handle)
        })
    })
}

/// **The block the ready line prints.** The added markers by step, nearest first;
/// each marker more than one step past `release` is listed under its step with
/// its ruling, or "far, no ruling" when its line names none. It refuses nothing.
pub fn distance_report(release: &str, added: &[Added]) -> Vec<String> {
    let Some(release_step) = steps(release).filter(|_| !added.is_empty()) else {
        return Vec::new();
    };
    let mut by_step: BTreeMap<&[u32], Vec<&Added>> = BTreeMap::new();
    for marker in added {
        by_step.entry(&marker.step).or_default().push(marker);
    }
    let mut lines = vec![format!(
        "temporary markers added since the base, by step ({} for release {release}):",
        added.len()
    )];
    for (step, markers) in by_step {
        let name = step
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let far = steps_out(&release_step, step).is_none_or(|out| out > 1);
        let mark = if far { ", far" } else { "" };
        lines.push(format!("  {name}: {} added{mark}", markers.len()));
        if far {
            for marker in markers {
                let ruling = ruling_of(&marker.text)
                    .map_or_else(|| "far, no ruling".to_string(), |r| format!("far, {r}"));
                lines.push(format!("    {}: {ruling}", marker.place));
            }
        }
    }
    lines
}

/// The added temporary markers of every file the bar measured. `at_base` reads a
/// path as the base held it: none for a path the base lacks, an error for a base
/// that cannot be read, which must never read as every marker added.
pub fn added_markers(
    files: &[Measured],
    at_base: impl Fn(&str) -> Result<Option<String>, String>,
) -> Result<Vec<Added>, String> {
    let mut added = Vec::new();
    for file in files {
        let own = file.path.starts_with("crates/jojobot-bar/src/size");
        if own || !(file.path.ends_with(".rs") || file.path.ends_with(".nix")) {
            continue;
        }
        let text = std::fs::read_to_string(&file.path)
            .map_err(|e| format!("{}: could not be read: {e}", file.path))?;
        let base = at_base(&file.path)?;
        added.extend(added_in(&file.path, &text, base.as_deref()));
    }
    Ok(added)
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

    fn added(place: &str, step: &str, text: &str) -> Added {
        Added {
            place: place.into(),
            step: steps(step).expect("a step"),
            text: text.into(),
        }
    }

    #[test]
    fn the_report_groups_by_step_and_flags_a_far_marker_by_its_ruling() {
        let t = temporary();
        let rows = [
            added("a.rs:1", "9.3.4", &format!("// {t}9.3.4): near")),
            added("a.rs:2", "9.3.6", &format!("// {t}9.3.6): rule 741")),
            added("a.rs:3", "9.3.6", &format!("// {t}9.3.6): work:q-1001")),
            added("b.rs:4", "9.3.10", &format!("// {t}9.3.10): nothing")),
        ];
        let lines = distance_report("9.3.3", &rows);
        let text = lines.join("\n");
        let at = |needle: &str| lines.iter().position(|l| l.contains(needle));
        assert!(
            at("9.3.4") < at("9.3.6") && at("9.3.6") < at("9.3.10"),
            "{text}"
        );
        assert!(at("9.3.4").is_some(), "{text}");
        let line_of = |place: &str| lines.iter().find(|l| l.contains(place)).cloned();
        assert!(
            line_of("a.rs:1").is_none(),
            "a near marker is not listed: {text}"
        );
        let ruled = line_of("a.rs:2").expect(&text);
        assert!(ruled.contains("far") && ruled.contains("741") && !ruled.contains("no ruling"));
        let handled = line_of("a.rs:3").expect(&text);
        assert!(
            handled.contains("far") && !handled.contains("no ruling"),
            "{handled}"
        );
        let bare = line_of("b.rs:4").expect(&text);
        assert!(bare.contains("far, no ruling"), "{bare}");
        assert!(distance_report("9.3.3", &[]).is_empty());
    }

    #[test]
    fn the_distance_is_numeric_on_the_last_part_and_a_longer_step_is_one_out() {
        let t = temporary();
        let far = |release: &str, step: &str| {
            let row = added("a.rs:1", step, &format!("// {t}{step}): x"));
            distance_report(release, &[row])
                .iter()
                .any(|l| l.contains("far, no ruling"))
        };
        assert!(!far("9.3.3", "9.3.4"));
        assert!(far("9.3.3", "9.3.5"));
        assert!(far("9.3.3", "9.3.10"));
        assert!(!far("9.3.3", "9.3.3.1"), "a sub-step is one out");
        assert!(far("9.3.3", "10"), "another series is far");
        assert!(!far("9.3.9", "9.3.10"));
    }

    #[test]
    fn a_marker_is_added_when_the_base_holds_fewer_of_its_line() {
        let t = temporary();
        let one = format!("fn a() {{}}\n// {t}9.3.5): x\n");
        let two = format!("{one}// {t}9.3.5): x\n// {t}9.3.6): y\n");
        assert!(added_in("a.rs", &one, Some(&one)).is_empty());
        let rows = added_in("a.rs", &two, Some(&one));
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.step == vec![9, 3, 6] && r.place == "a.rs:4")
        );
        assert_eq!(
            added_in("a.rs", &one, None).len(),
            1,
            "no base file: all added"
        );
        assert!(added_in("a.rs", &format!("x\n{one}"), Some(&one)).is_empty());
    }

    #[test]
    fn added_markers_read_each_file_against_the_base_copy() {
        let root = std::env::temp_dir().join(crate::scratch::name("added-markers"));
        std::fs::create_dir_all(&root).expect("source directory");
        let t = temporary();
        let head = format!("// {t}9.3.7): x\n");
        let path = root.join("a.rs");
        std::fs::write(&path, &head).expect("source text");
        let files = [Measured {
            path: path.to_str().expect("source path").into(),
            lines: 1,
        }];
        let same = added_markers(&files, |_| Ok(Some(head.clone()))).expect("scan");
        let new = added_markers(&files, |_| Ok(None)).expect("scan");
        let unread = added_markers(&files, |_| Err("base gone".into()));
        std::fs::remove_dir_all(&root).expect("remove source directory");
        assert!(same.is_empty(), "{same:?}");
        assert_eq!(new.len(), 1, "{new:?}");
        assert_eq!(
            unread,
            Err("base gone".into()),
            "an unreadable base is no list"
        );
    }

    #[test]
    fn a_release_that_is_no_step_fails_the_scan_and_names_the_value() {
        let error = scan(&[], Some("soon")).expect_err("not a step");
        assert!(error.contains("soon"), "{error}");
        assert!(scan(&[], Some("9.3.3")).is_ok());
        assert!(scan(&[], None).is_ok());
    }
}
