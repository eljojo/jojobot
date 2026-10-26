//! **Owned markers.** A temporary path names the step that retires it. A GAP
//! marker or a tripwire in the user stories names the work item that owns the
//! missing capability, by handle, or says it is an invariant. The bar lists
//! both and refuses what names no owner or names a step already passed.

use super::Measured;

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

/// A `work:q-` handle with a name, or the word INVARIANT.
fn names_an_owner(text: &str) -> bool {
    let invariant = ["INVARI", "ANT"].concat();
    text.contains(&invariant)
        || text
            .match_indices("work:q-")
            .any(|(at, _)| text[at + 7..].starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// Offences in one file. `step` is the step the series is at.
pub fn offences_in(path: &str, text: &str, step: Option<&str>) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let stories = path.starts_with(STORIES);
    let now = step.and_then(steps);
    for (at, line) in lines.iter().enumerate() {
        let place = format!("{path}:{}", at + 1);
        if let Some((_, rest)) = line.split_once(&temporary()) {
            let named = rest.split(')').next().unwrap_or("");
            match (steps(named), &now) {
                (None, _) => found.push(format!("{place}: the step '{named}' is not a step")),
                (Some(own), Some(now)) if own < *now => {
                    found.push(format!("{place}: retiring step {named} is past"))
                }
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
            found.push(format!("{place}: the {what} names no work:q- owner"));
        }
    }
    found
}

/// Every tracked file's offences, for the files the bar already measured.
pub fn scan(files: &[Measured], step: Option<&str>) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for file in files {
        let own = file.path.starts_with("crates/jojobot-bar/src/size");
        if own || !(file.path.ends_with(".rs") || file.path.ends_with(".nix")) {
            continue;
        }
        let text = std::fs::read_to_string(&file.path)
            .map_err(|e| format!("{}: could not be read: {e}", file.path))?;
        found.extend(offences_in(&file.path, &text, step));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(body: &str) -> Vec<String> {
        offences_in(&format!("{STORIES}x.rs"), body, None)
    }

    #[test]
    fn a_gap_marker_needs_a_handle_and_a_tripwire_takes_its_blocks() {
        let (g, t) = (gap(), tripwire());
        assert_eq!(story(&format!("    {g} a\n    // b\n")).len(), 1);
        let owned = format!("    {g} a\n    // work:q-x\n    s{t}\"a\", &[]).await;\n");
        assert!(story(&owned).is_empty(), "{:?}", story(&owned));
        let bare = format!("    // plain\n    s{t}\"a\", &[]).await;\n");
        assert_eq!(story(&bare).len(), 1);
        let invariant = format!("    // INVARIANT\n    s{t}\"a\", &[]).await;\n");
        assert!(story(&invariant).is_empty());
        assert_eq!(story(&format!("    {g} work:q- x\n")).len(), 1, "no name");
    }

    #[test]
    fn a_temporary_marker_fails_only_past_its_step() {
        let m = |s: &str| format!("// {}{s}): x\n", temporary());
        let run = |s: &str| offences_in("a.rs", &m(s), Some("9.3.1"));
        assert!(run("9.3.1").is_empty() && run("9.3.10").is_empty());
        assert_eq!(run("9.3.0").len(), 1);
        assert_eq!(run("soon").len(), 1);
    }
}
