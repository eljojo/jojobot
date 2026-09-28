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
//!
//! **A story rarely spells a verb's own name.** It calls a DSL wrapper —
//! `s.shape(..., json!({"answers_type": "trip"}))` calls `recall` without
//! `"recall"` ever appearing in the story — so a check keyed on the verb's
//! literal name alone would read the wrapper's 77 call sites as zero. See
//! [`dsl_wrappers`] for how a wrapper is found: derived from dsl.rs's own
//! source, never a hand-kept alias list.
//!
//! **A known limit, stated rather than implied**: an argument built on a
//! separate line from the call that sends it is not seen, whether the call
//! names its verb directly or through a wrapper — a zero here may be that.
//! Matching is textual, not a parse of the call expression. **Nor is a
//! wrapper taking its arguments as named Rust parameters rather than a
//! `json!` object** — `Session::merge_entities`'s own `recorded_at`
//! parameter is invisible the same way, because there is no `{...}` for
//! [`sites_for_anchor`] to read. And a method whose body names either no
//! real verb or more than one is not derived as anybody's wrapper at all —
//! see [`dsl_wrappers`]'s own doc for what that excludes today.

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

/// **Every occurrence of `anchor` in `text`**, as the whole brace-balanced
/// object that follows it — the arguments a call sends, whatever the anchor
/// actually is: a quoted verb name (`"recall"`) or a wrapper method's own
/// call syntax (`.shape(`). Reused for both, so the two cannot come to read
/// a call two different ways.
fn sites_for_anchor<'a>(text: &'a str, anchor: &str) -> Vec<&'a str> {
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(anchor) {
        let after = from + found + anchor.len();
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

/// **Every DSL wrapper method, and the one served verb its own body
/// dispatches to** — derived by reading dsl.rs itself, never a hand-kept
/// alias list. A `pub async fn` whose body names EXACTLY ONE of `tool_names`
/// as a quoted string literal is that verb's wrapper — `Session::shape`
/// names only `"recall"`, so a story calling `.shape(...)` is calling
/// `recall` as surely as one that spells it.
///
/// **A method naming none of them, or more than one, is not derived as a
/// wrapper at all.** The low-level dispatch methods (`Session::call`,
/// `Session::write`, `Session::read`) take the verb as a parameter rather
/// than a literal, so none of `tool_names` appears in their own bodies —
/// correctly excluded, since treating them as `SOMEONE`'s wrapper would be a
/// guess about which verb a caller happened to pass. A method naming two is
/// excluded for the opposite reason: which one a given call site meant is
/// not this rule's to decide.
fn dsl_wrappers(tool_names: &[String]) -> Vec<(String, String)> {
    let dsl = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/user_stories/dsl.rs"),
    )
    .expect("dsl.rs reads");
    derive_wrappers_from(&dsl, tool_names)
}

/// **The parsing rule itself**, apart from reading the real file — so a test
/// can hand it a few lines of fabricated source and check what it derives,
/// without that case depending on dsl.rs staying a particular shape forever.
/// [`dsl_wrappers`] is this, read over the real file.
fn derive_wrappers_from(dsl: &str, tool_names: &[String]) -> Vec<(String, String)> {
    let mut wrappers = Vec::new();
    let mut from = 0;
    while let Some(found) = dsl[from..].find("pub async fn ") {
        let name_start = from + found + "pub async fn ".len();
        let Some(paren) = dsl[name_start..].find('(') else {
            break;
        };
        let name = dsl[name_start..name_start + paren].trim().to_string();
        let Some(sig_open) = dsl[name_start..].find('{') else {
            from = name_start;
            continue;
        };
        let body_start = name_start + sig_open;
        let Some(body) = balanced_span(&dsl[body_start..]) else {
            from = body_start;
            continue;
        };
        let named: Vec<&String> = tool_names
            .iter()
            .filter(|tool| body.contains(&format!("\"{tool}\"")))
            .collect();
        if let [one] = named[..] {
            wrappers.push((name, (*one).clone()));
        }
        from = body_start + body.len();
    }
    wrappers
}

/// **The one exemption, derived from a real mechanism rather than
/// hand-picked** — `sid` cannot appear as a literal key in a story's own
/// source, because `Session::riding`
/// (`crates/jojobot/tests/user_stories/dsl.rs`) injects it into every
/// call's arguments before the request goes out, precisely so a story never
/// has to spell out the identity it already holds. Every other argument
/// this report checks is exercised, when it is exercised, by a story
/// writing it down; this one is exercised by never having to.
///
/// **It cannot outlive its reason**: [`the_sid_exemption_still_matches_what_riding_actually_does`]
/// reads `riding`'s own source and goes red the day it stops injecting —
/// this list is not a standing allowance, it is a claim that test keeps
/// honest. No other name belongs here without the same kind of proof.
const INJECTED_ARGUMENTS: &[&str] = &["sid"];

/// **Whether any story calls `verb` with `argument` among its keys**, at any
/// depth inside that call's own arguments object — reached directly, by
/// `verb`'s own quoted name, or through any `wrappers` entry naming `verb` —
/// or the argument is one the story DSL injects on every call regardless of
/// what any story wrote, see [`INJECTED_ARGUMENTS`].
fn covered(
    sources: &[(PathBuf, String)],
    verb: &str,
    argument: &str,
    wrappers: &[(String, String)],
) -> bool {
    if INJECTED_ARGUMENTS.contains(&argument) {
        return true;
    }
    let mut anchors = vec![format!("\"{verb}\"")];
    anchors.extend(
        wrappers
            .iter()
            .filter(|(_, wrapped)| wrapped == verb)
            .map(|(name, _)| format!(".{name}(")),
    );
    let needle = format!("\"{argument}\"");
    sources.iter().any(|(_, text)| {
        anchors.iter().any(|anchor| {
            sites_for_anchor(text, anchor)
                .iter()
                .any(|site| site.contains(&needle))
        })
    })
}

/// **Every (verb, argument) pair the surface publishes, and whether the
/// story suite ever calls that verb with that argument** — directly, or
/// through a DSL wrapper method (see [`dsl_wrappers`]).
fn argument_coverage() -> Vec<(String, String, bool)> {
    let sources = story_sources();
    let tools = Jojobot::tool_router().list_all();
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let wrappers = dsl_wrappers(&tool_names);
    let mut rows = Vec::new();
    for tool in &tools {
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let mut names = Vec::new();
        argument_names(&schema, &mut names);
        names.sort();
        names.dedup();
        for name in names {
            let is_covered = covered(&sources, &tool.name, &name, &wrappers);
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
        assert!(covered(&sources, "declare_type", "fields", &[]));
    }

    /// 🚨 **What the report exists to catch**: a call to the verb that never
    /// names the argument leaves it uncovered.
    #[test]
    fn a_call_that_never_names_the_argument_is_not_covered() {
        let sources = source("s.call(\"declare_type\", json!({ \"name\": \"x\" })).await;");
        assert!(!covered(&sources, "declare_type", "fields", &[]));
    }

    /// **`sid` counts as covered even where no story ever calls the verb at
    /// all** — the exemption is unconditional, the way `Session::riding`'s
    /// own injection is. Empty sources are the sharpest form of the claim:
    /// nothing else here could make this pass by accident.
    #[test]
    fn the_sid_exemption_applies_with_no_call_site_at_all() {
        assert!(covered(&[], "declare_type", "sid", &[]));
    }

    /// **An argument nested under a sub-object is still found** — `follow`
    /// on `recall` carries its own keys, and a check that only read the top
    /// level would report every one of them uncovered regardless of what a
    /// story actually sent.
    #[test]
    fn a_nested_argument_under_a_sub_object_is_still_found() {
        let sources =
            source("s.call(\"recall\", json!({ \"follow\": { \"fits_type\": \"pet\" } })).await;");
        assert!(covered(&sources, "recall", "fits_type", &[]));
    }

    /// **Coverage is scoped to the verb actually called.** `fits_type` is
    /// named for `search` here and never for `recall` — a check that matched
    /// the argument name alone, anywhere in the corpus, would call `recall`
    /// covered on the strength of a call it was never part of.
    #[test]
    fn coverage_is_scoped_to_the_verb_actually_called() {
        let sources = source("s.call(\"search\", json!({ \"fits_type\": \"pet\" })).await;");
        assert!(covered(&sources, "search", "fits_type", &[]));
        assert!(
            !covered(&sources, "recall", "fits_type", &[]),
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
        assert!(covered(&sources, "declare_type", "fields", &[]));
    }

    /// **A DSL wrapper counts as its own verb's call site.** `shape` maps to
    /// `recall` — derived here rather than declared, so the story below
    /// never spells `"recall"` at all, exactly as `typing.rs` and
    /// `vocabulary.rs` do not.
    #[test]
    fn a_call_through_a_derived_wrapper_counts_as_coverage() {
        let sources =
            source("s.shape(\"what fits\", json!({ \"answers_type\": \"trip\" })).await;");
        let wrappers = vec![("shape".to_string(), "recall".to_string())];
        assert!(covered(&sources, "recall", "answers_type", &wrappers));
    }

    /// **The wrapper only covers the verb it was derived for.** A `shape`
    /// call carrying `answers_type` says nothing about `search`, which has
    /// its own argument of the same name and was never called here.
    #[test]
    fn a_wrapper_derived_for_one_verb_does_not_cover_another() {
        let sources =
            source("s.shape(\"what fits\", json!({ \"answers_type\": \"trip\" })).await;");
        let wrappers = vec![("shape".to_string(), "recall".to_string())];
        assert!(!covered(&sources, "search", "answers_type", &wrappers));
    }

    /// **`dsl_wrappers` reads dsl.rs itself**: `Session::shape` names only
    /// `"recall"` in its own body, so it is derived as `recall`'s wrapper —
    /// the same claim [`the_shape_wrapper_still_maps_to_recall`] pins against
    /// dsl.rs as it stands today.
    #[test]
    fn a_method_naming_one_real_verb_is_derived_as_its_wrapper() {
        let dsl = "pub async fn shape(&self, what: &str, args: Value) -> Answer { \
                    self.read(what.to_string(), \"recall\", args).await }";
        let tools = vec!["recall".to_string(), "search".to_string()];
        assert_eq!(
            derive_wrappers_from(dsl, &tools),
            vec![("shape".to_string(), "recall".to_string())],
        );
    }

    /// **A method naming no real verb, or more than one, is not a wrapper**
    /// — the two ways a call site cannot say what it means. `write` takes
    /// the verb as a parameter rather than a literal, and a fabricated
    /// method naming both `recall` and `search` names neither on its own.
    #[test]
    fn a_method_naming_none_or_two_real_verbs_is_not_derived() {
        let dsl = "pub async fn write(&self, tool: &str, args: Value) { call(tool, args).await; } \
                   pub async fn ambiguous(&self) { let _ = \"recall\"; let _ = \"search\"; }";
        let tools = vec!["recall".to_string(), "search".to_string()];
        assert_eq!(
            derive_wrappers_from(dsl, &tools),
            Vec::<(String, String)>::new()
        );
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

    /// 🚨 **The `sid` exemption cannot outlive its reason.** If
    /// `Session::riding` ever stops injecting `sid` into every call's
    /// arguments, this goes red the same day — not some later day somebody
    /// notices the report has gone quiet on an argument nothing exercises
    /// any more.
    #[test]
    fn the_sid_exemption_still_matches_what_riding_actually_does() {
        let dsl = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/user_stories/dsl.rs"),
        )
        .expect("dsl.rs reads");
        assert!(
            dsl.contains("args[\"sid\"] = self.sid"),
            "Session::riding no longer injects sid the way INJECTED_ARGUMENTS assumes — read \
             riding() in dsl.rs and update the exemption to match what it actually does"
        );
    }

    /// 🚨 **The wrapper derivation cannot outlive its reason either.** If
    /// `Session::shape` ever stops being `recall`'s wrapper — a rewrite that
    /// makes it call some other verb, or names two — this goes red the same
    /// day, over the real dsl.rs rather than a fabricated snippet.
    #[test]
    fn the_shape_wrapper_still_maps_to_recall() {
        let tools = Jojobot::tool_router().list_all();
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        let wrappers = dsl_wrappers(&tool_names);
        assert!(
            wrappers.contains(&("shape".to_string(), "recall".to_string())),
            "Session::shape no longer derives as recall's wrapper — read shape() in dsl.rs: \
             {wrappers:?}"
        );
    }

    /// **A low-level dispatch method is not treated as anybody's wrapper** —
    /// proven over the real file, not a fabricated one. `Session::call`
    /// takes the verb as a parameter rather than a literal, so it must never
    /// be derived as one verb's alone.
    #[test]
    fn a_low_level_dispatch_method_is_not_derived_as_a_wrapper() {
        let tools = Jojobot::tool_router().list_all();
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        let wrappers = dsl_wrappers(&tool_names);
        assert!(
            !wrappers.iter().any(|(name, _)| name == "call"),
            "Session::call takes the verb as a parameter and names none as a literal — it must \
             never be derived as a wrapper: {wrappers:?}"
        );
    }
}
