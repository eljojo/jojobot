use super::*;

/// **The gendered pronouns of English.** A closed set of words rather than a
/// list of sentences somebody wrote: the words are the language's and cannot
/// go stale, where a catalogue of forbidden phrasings re-embeds what it
/// filters and misses the next author's wording (rule 106).
pub(super) const PRONOUNS: &[&str] = &[
    "he", "him", "his", "she", "her", "hers", "himself", "herself",
];

/// These cases attribute the operator's word. A subject named in their setup
/// does not name the speaker of that testimony.
#[test]
fn testimony_fixture_words_do_not_gender_the_speaker() {
    let fixtures = [
        (
            "crates/jojobot-mcp/src/memory/capture/tests.rs",
            "a_computed_schedule_does_not_inherit_the_callers_word",
        ),
        (
            "crates/jojobot-mcp/src/memory/capture/tests.rs",
            "a_stored_value_that_differs_from_the_sent_one_is_named",
        ),
        (
            "crates/jojobot-mcp/src/memory/recall/tests.rs",
            "a_read_can_ask_who_backs_the_value_it_is_about_to_act_on",
        ),
        (
            "crates/jojobot-domain/src/memory/testing/contract/base.rs",
            "a_folded_value_says_who_backs_it",
        ),
    ];
    let mut unattached = Vec::new();
    for (path, case) in fixtures {
        let source = fs::read_to_string(workspace_root().join(path)).expect("the fixture reads");
        let lines: Vec<_> = source.lines().collect();
        let start = lines
            .iter()
            .position(|line| {
                line.contains(&format!("fn {case}(")) || line.contains(&format!("fn {case}<"))
            })
            .unwrap_or_else(|| panic!("the testimony case is present: {path} {case}"));
        let end = start
            + lines[start..]
                .iter()
                .position(|line| *line == "}")
                .expect("the whole fixture function is read");
        for (at, line) in lines.iter().enumerate().take(end).skip(start) {
            if line.contains('"') && PRONOUNS.iter().any(|word| says_word(line, word, false)) {
                unattached.push(format!("{path}:{}: {}", at + 1, line.trim()));
            }
        }
    }
    assert!(
        unattached.is_empty(),
        "testimony in these fixtures must name the speaker neutrally:\n{}",
        unattached.join("\n")
    );
}
