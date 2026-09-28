//! **Every argument the served surface publishes, checked against the story
//! suite that is supposed to reach it.**
//!
//! Rules 151 and 177 ask for a story on every capability and a re-open on
//! every landing; neither performs its own first mechanical step. A check
//! keyed on a verb NAME sees a new tool ship with no story — the tool's own
//! name has never appeared in the suite before. It cannot see a new ARGUMENT
//! ship on a tool the suite already calls constantly, because the verb's own
//! name is covered a hundred times over and the argument rides in unseen.
//! This is that missing half.
//!
//! **It reports. It never refuses or gates.** A zero here is a lead, never a
//! finding: some arguments are legitimately exercised under another name — a
//! view fills them in, a default reaches the same code a spelled-out value
//! would — and only a person reading the call sites can tell that apart from
//! a real gap.

use std::path::PathBuf;

use jojobot_mcp::Jojobot;

/// Every `.rs` file under this crate's own `tests/user_stories`, whole.
fn story_sources() -> Vec<(PathBuf, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/user_stories");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the story directory reads") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("a readable story file");
            files.push((path, text));
        }
    }
    files
}

/// Every argument name a schema node publishes, at every depth — the keys of
/// every `properties` map the document holds, nested definitions included.
/// Reads the same document a client is served; nothing here enumerates how
/// many levels a schema may have.
fn argument_names(node: &serde_json::Value, found: &mut Vec<String>) {
    if let Some(properties) = node.get("properties").and_then(|p| p.as_object()) {
        found.extend(properties.keys().cloned());
    }
    match node {
        serde_json::Value::Object(map) => {
            map.values().for_each(|child| argument_names(child, found));
        }
        serde_json::Value::Array(items) => {
            items.iter().for_each(|child| argument_names(child, found));
        }
        _ => {}
    }
}

/// The brace-balanced span starting at `text`'s own first `{`, closing brace
/// included — `None` when the braces never balance before `text` ends.
fn balanced_span(text: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// **Every call site of `verb` in `text`**, as the whole brace-balanced
/// arguments object that follows its name — `.call("verb", json!({ ... }))`
/// and `.refused("verb", json!({ ... }))` alike, since a refusal is still a
/// story reaching the argument. Matched on the quoted verb name rather than
/// on `.call`/`.refused` so a story that names the verb any other way this
/// suite ever grows is still found.
fn call_sites<'a>(text: &'a str, verb: &str) -> Vec<&'a str> {
    let needle = format!("\"{verb}\"");
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(&needle) {
        let after = from + found + needle.len();
        from = after;
        let Some(open) = text[after..].find('{') else {
            continue;
        };
        if let Some(span) = balanced_span(&text[after + open..]) {
            sites.push(span);
        }
    }
    sites
}

/// **Whether any story calls `verb` with `argument` among its keys**, at any
/// depth inside that call's own arguments object.
fn covered(sources: &[(PathBuf, String)], verb: &str, argument: &str) -> bool {
    let needle = format!("\"{argument}\"");
    sources.iter().any(|(_, text)| {
        call_sites(text, verb)
            .iter()
            .any(|site| site.contains(&needle))
    })
}

/// **Every (verb, argument) pair the surface publishes, and whether the
/// story suite ever calls that verb with that argument.**
fn argument_coverage() -> Vec<(String, String, bool)> {
    let sources = story_sources();
    let tools = Jojobot::tool_router().list_all();
    let mut rows = Vec::new();
    for tool in &tools {
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let mut names = Vec::new();
        argument_names(&schema, &mut names);
        names.sort();
        names.dedup();
        for name in names {
            let is_covered = covered(&sources, &tool.name, &name);
            rows.push((tool.name.to_string(), name, is_covered));
        }
    }
    rows
}

/// **The report.** Runs the coverage over the real served surface and the
/// real story suite, and prints every (verb, argument) pair with no story
/// site. Always passes: naming a gap is not the same claim as there being
/// none, and only a person reading the names can tell a real gap from an
/// argument covered under another one's own default.
#[test]
fn served_arguments_with_no_story_site_are_reported() {
    let zero: Vec<String> = argument_coverage()
        .into_iter()
        .filter(|(_, _, is_covered)| !is_covered)
        .map(|(verb, argument, _)| format!("{verb}.{argument}"))
        .collect();
    println!(
        "ARGUMENTS WITH NO STORY SITE ({}), a lead and never a failure:",
        zero.len()
    );
    for name in &zero {
        println!("  {name}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> Vec<(PathBuf, String)> {
        vec![(PathBuf::from("scratch.rs"), text.to_string())]
    }

    /// **The positive `served_arguments_with_no_story_site_are_reported`
    /// rests on**: a call naming the argument marks it covered.
    #[test]
    fn a_call_naming_the_argument_marks_it_covered() {
        let sources =
            source("s.call(\"declare_type\", json!({ \"name\": \"x\", \"fields\": [] })).await;");
        assert!(covered(&sources, "declare_type", "fields"));
    }

    /// 🚨 **What the report exists to catch**: a call to the verb that never
    /// names the argument leaves it uncovered.
    #[test]
    fn a_call_that_never_names_the_argument_is_not_covered() {
        let sources = source("s.call(\"declare_type\", json!({ \"name\": \"x\" })).await;");
        assert!(!covered(&sources, "declare_type", "fields"));
    }

    /// **An argument nested under a sub-object is still found** — `follow`
    /// on `recall` carries its own keys, and a check that only read the top
    /// level would report every one of them uncovered regardless of what a
    /// story actually sent.
    #[test]
    fn a_nested_argument_under_a_sub_object_is_still_found() {
        let sources =
            source("s.call(\"recall\", json!({ \"follow\": { \"fits_type\": \"pet\" } })).await;");
        assert!(covered(&sources, "recall", "fits_type"));
    }

    /// **Coverage is scoped to the verb actually called.** `fits_type` is
    /// named for `search` here and never for `recall` — a check that matched
    /// the argument name alone, anywhere in the corpus, would call `recall`
    /// covered on the strength of a call it was never part of.
    #[test]
    fn coverage_is_scoped_to_the_verb_actually_called() {
        let sources = source("s.call(\"search\", json!({ \"fits_type\": \"pet\" })).await;");
        assert!(covered(&sources, "search", "fits_type"));
        assert!(
            !covered(&sources, "recall", "fits_type"),
            "recall was never called here at all"
        );
    }

    /// **A refusal is still a story reaching the argument.** `refused` is
    /// the one verb a story deliberately expects `blocked` from — an
    /// argument only ever sent to provoke that refusal is still exercised,
    /// not merely reachable in theory.
    #[test]
    fn a_refused_call_counts_as_coverage_too() {
        let sources = source(
            "s.refused(\"declare_type\", json!({ \"name\": \"x\", \"fields\": [] })).await;",
        );
        assert!(covered(&sources, "declare_type", "fields"));
    }

    /// **What a check that reads nothing would print**: every argument on
    /// every verb, indistinguishable from a real, sparse zero list only by
    /// its size. Real coverage over this workspace's own story suite must
    /// come back far short of the surface's whole argument count, or the
    /// report is not reading the corpus at all.
    #[test]
    fn the_real_report_is_not_silently_empty_of_evidence() {
        let rows = argument_coverage();
        let total = rows.len();
        let covered_count = rows.iter().filter(|(_, _, c)| *c).count();
        assert!(
            total > 20,
            "the served surface must publish more than a couple of arguments, or this proves \
             nothing: {total}"
        );
        assert!(
            covered_count > 0,
            "a check that read no story text would report every argument uncovered — at least \
             one real argument must come back covered: {covered_count} of {total}"
        );
    }
}
