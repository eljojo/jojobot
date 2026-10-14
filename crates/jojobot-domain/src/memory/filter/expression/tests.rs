use super::*;
use crate::memory::filter::{Direction, Op, Test};
use crate::memory::types::ValueType;

fn day(text: &str) -> Date {
    text.parse().expect("a civil date")
}

/// The tree an expression reads into, on the asking day 2026-10-09.
fn tree(text: &str) -> Filter {
    parse(text, day("2026-10-09")).unwrap_or_else(|e| panic!("'{text}' should parse: {e:?}"))
}

/// The refusal an expression earns, as the words the caller reads.
fn refusal(text: &str) -> String {
    match parse(text, day("2026-10-09")) {
        Err(MemoryError::InvalidQuery(words)) => words,
        other => panic!("'{text}' should be refused, got {other:?}"),
    }
}

/// **Every comparison word reads into the leaf the tree already has.** Each
/// case is the same leaf its builder makes, so the expression adds a spelling
/// and no new meaning.
#[test]
fn each_word_reads_into_its_leaf() {
    assert_eq!(tree("status = done"), Filter::equals("status", "done"));
    assert_eq!(tree("status != done"), Filter::not_equals("status", "done"));
    assert_eq!(
        tree("size < 5"),
        Filter::ordering("size", Op::Less, ValueType::Number, "5")
    );
    assert_eq!(
        tree("size <= 5"),
        Filter::ordering("size", Op::LessOrEqual, ValueType::Number, "5")
    );
    assert_eq!(
        tree("size > 5"),
        Filter::ordering("size", Op::Greater, ValueType::Number, "5")
    );
    assert_eq!(
        tree("size >= 2.5"),
        Filter::ordering("size", Op::GreaterOrEqual, ValueType::Number, "2.5")
    );
    assert_eq!(
        tree("due < 2026-11-01"),
        Filter::ordering("due", Op::Less, ValueType::Date, "2026-11-01")
    );
    assert_eq!(
        tree("status in (now, next)"),
        Filter::is_in("status", &["now", "next"])
    );
    assert_eq!(
        tree("touches contains skills.rs"),
        Filter::contains("touches", "skills.rs")
    );
    assert_eq!(tree("due"), Filter::exists("due"));
}

/// **A handle is a value like any other word**, and a key may carry the slashes
/// and colons keys already carry.
#[test]
fn handles_and_slashed_keys_are_words() {
    assert_eq!(
        tree("owner = bot:gamma"),
        Filter::equals("owner", "bot:gamma")
    );
    assert_eq!(
        tree("role/dev3/agent = abc"),
        Filter::equals("role/dev3/agent", "abc")
    );
}

/// **A quoted value keeps its spaces and its operator characters**, and a
/// quoted word is never a relative date.
#[test]
fn a_quoted_value_is_taken_whole() {
    assert_eq!(
        tree(r#"title = "fix && ship (soon)""#),
        Filter::equals("title", "fix && ship (soon)")
    );
    assert_eq!(
        tree(r#"status = "today""#),
        Filter::equals("status", "today")
    );
}

/// **`&&` binds tighter than `||`, `!` binds tightest, and parentheses override.**
#[test]
fn precedence_is_not_then_and_then_or() {
    let a = || Filter::equals("a", "1");
    let b = || Filter::equals("b", "2");
    let c = || Filter::equals("c", "3");
    assert_eq!(
        tree("a = 1 && b = 2 || c = 3"),
        Filter::Any(vec![Filter::All(vec![a(), b()]), c()])
    );
    assert_eq!(
        tree("a = 1 && (b = 2 || c = 3)"),
        Filter::All(vec![a(), Filter::Any(vec![b(), c()])])
    );
    assert_eq!(
        tree("!a = 1 && b = 2"),
        Filter::All(vec![Filter::negated(a()), b()])
    );
    assert_eq!(
        tree("!(a = 1 || b = 2)"),
        Filter::negated(Filter::Any(vec![a(), b()]))
    );
    assert_eq!(tree("!!a = 1"), Filter::negated(Filter::negated(a())));
}

/// **A relative date is the asking day moved by whole days or weeks**, and it
/// reads as a date wherever an ordering asks for one.
#[test]
fn a_relative_date_is_resolved_on_the_asking_day() {
    let due = |op, date: &str| Filter::ordering("due", op, ValueType::Date, date);
    assert_eq!(tree("due <= today"), due(Op::LessOrEqual, "2026-10-09"));
    assert_eq!(tree("due <= today+7d"), due(Op::LessOrEqual, "2026-10-16"));
    assert_eq!(tree("due > today-2w"), due(Op::Greater, "2026-09-25"));
    assert_eq!(tree("due = today+1d"), Filter::equals("due", "2026-10-10"));
    // The same text on another day is another date: nothing was stored.
    assert_eq!(
        parse("due <= today+7d", day("2026-12-30")).unwrap(),
        due(Op::LessOrEqual, "2027-01-06")
    );
}

/// **A mistake names the token, where it is and the words accepted there.**
#[test]
fn a_mistake_names_the_token_its_position_and_what_is_accepted() {
    let words = refusal("status == done");
    assert!(words.contains("position 9"), "{words}");
    assert!(words.contains("'='"), "{words}");
    for accepted in ["!=", "<=", ">=", "in", "contains", "&&", "||"] {
        let words = refusal("status done");
        assert!(words.contains(accepted), "{accepted} missing from: {words}");
    }
    let words = refusal("status = ");
    assert!(words.contains("end"), "{words}");
    assert!(words.contains("value"), "{words}");
    let words = refusal("(status = done");
    assert!(words.contains("')'"), "{words}");
    let words = refusal("status = done )");
    assert!(words.contains("position 15"), "{words}");
    let words = refusal("status = done & owner = x");
    assert!(words.contains("position 15"), "{words}");
    assert!(words.contains("&&"), "{words}");
    let words = refusal("   ");
    assert!(words.contains("empty"), "{words}");
    let words = refusal(r#"title = "open"#);
    assert!(words.contains("position 9"), "{words}");
}

/// **An ordering reads dates and numbers and nothing else**, and a unit that is
/// not days or weeks is refused with the units that are.
#[test]
fn an_ordering_needs_a_date_or_a_number() {
    let words = refusal("due < banana");
    assert!(words.contains("banana"), "{words}");
    assert!(words.contains("position 7"), "{words}");
    assert!(words.contains("date"), "{words}");
    let words = refusal("due < today+3m");
    assert!(words.contains("today+3m"), "{words}");
    assert!(words.contains('d') && words.contains('w'), "{words}");
    let _ = (Direction::Ascending, Test::Exists);
}

/// **A relative date that ends in a character wider than a byte is refused with
/// the word and where it is, never a panic.** The unit is the last character of
/// the word, so a split by byte lands inside it. Both the ordering and the
/// equality path read the word, and the position counts characters: the key in
/// the last case has a character that takes two bytes before the word starts.
/// The ordinary forms still parse.
#[test]
fn a_relative_date_with_a_wide_character_is_refused_with_its_word_and_position() {
    for (text, word, position) in [
        ("due < today+\u{e9}", "today+\u{e9}", "position 7"),
        ("due < today+3\u{e9}", "today+3\u{e9}", "position 7"),
        ("title = today-\u{e9}", "today-\u{e9}", "position 9"),
        ("t\u{ed}tulo = today-\u{e9}", "today-\u{e9}", "position 10"),
    ] {
        let words = refusal(text);
        assert!(words.contains(word), "{text}: {words}");
        assert!(words.contains(position), "{text}: {words}");
        assert!(words.contains("relative"), "{text}: {words}");
    }
    assert_eq!(
        tree("due <= today+3d"),
        Filter::ordering("due", Op::LessOrEqual, ValueType::Date, "2026-10-12")
    );
}

/// **Nothing a caller sends may take the parser down.** Text past the length
/// limit is refused before it is read, naming the limit and the first character
/// over it; nesting past the depth limit is refused at the token that passes it.
/// Each hostile shape is its own line, because a stack that overflows takes the
/// whole test thread with it. The edges parse: the limit itself is allowed.
#[test]
fn an_expression_past_the_length_or_depth_limit_is_refused_with_the_limit_named() {
    let limit = MAX_LENGTH.to_string();
    for hostile in [
        "(".repeat(100_000),
        format!("{}a = 1", "!".repeat(100_000)),
        format!("status = {}", "x".repeat(MAX_LENGTH)),
    ] {
        let words = refusal(&hostile);
        assert!(words.contains(&limit), "{}: {words}", &hostile[..16]);
        assert!(
            words.contains(&format!("position {}", MAX_LENGTH + 1)),
            "{}: {words}",
            &hostile[..16]
        );
    }
    let depth = MAX_DEPTH.to_string();
    let nested = |n: usize| format!("{}a = 1{}", "(".repeat(n), ")".repeat(n));
    let negated = |n: usize| format!("{}a = 1", "!".repeat(n));
    for (hostile, position) in [
        (nested(MAX_DEPTH + 1), MAX_DEPTH + 1),
        (negated(MAX_DEPTH + 1), MAX_DEPTH + 1),
        (
            format!("{}{}a = 1", "(".repeat(MAX_DEPTH), "!"),
            MAX_DEPTH + 1,
        ),
    ] {
        let words = refusal(&hostile);
        assert!(words.contains(&depth), "{words}");
        assert!(words.contains(&format!("position {position}")), "{words}");
    }
    // The edge parses, and so does a long ordinary line under the length limit.
    parse(&nested(MAX_DEPTH), day("2026-10-09")).expect("the limit itself is allowed");
    parse(&negated(MAX_DEPTH), day("2026-10-09")).expect("the limit itself is allowed");
    // Each term is 13 characters and each joint 4, so n terms take 17n - 4.
    let long = vec!["status = done"; (MAX_LENGTH + 4) / 17].join(" && ");
    assert!(long.chars().count() <= MAX_LENGTH);
    parse(&long, day("2026-10-09")).expect("a long flat line is not nesting");
}

/// **A day count past what a date can hold is refused, never a panic.** The span
/// type holds a bounded number of days; a count past it, or a date past the
/// calendar, is the same refusal as any other word that is no relative date.
/// The ordinary counts still name their days.
#[test]
fn a_relative_date_past_the_calendar_is_refused_not_a_panic() {
    for text in [
        "due < today+8000000d",
        "due < today+1043498w",
        "due < today-1043498w",
        "due < today+4294967295d",
        "due < today+4294967295w",
        "due < today+7000000d",
    ] {
        let words = refusal(text);
        assert!(words.contains("relative"), "{text}: {words}");
        assert!(words.contains("position 7"), "{text}: {words}");
    }
    assert_eq!(tree("due = today+3d"), Filter::equals("due", "2026-10-12"));
    assert_eq!(tree("due = today-2w"), Filter::equals("due", "2026-09-25"));
}

/// **The values are the words an equality, an any-of or a `contains` compares
/// against, with the characters they sit at**, a quoted one without its quotes.
/// A key and an ordering's operand are not values.
#[test]
fn the_values_come_back_where_they_sit() {
    let text = r#"owner = bot:gamma && status in (now, "bot:delta") && due < 2026-11-01"#;
    let found = value_words(text).expect("it parses");
    let spelled: Vec<(&str, String)> = found
        .iter()
        .map(|v| {
            (
                v.text.as_str(),
                text.chars().skip(v.start).take(v.end - v.start).collect(),
            )
        })
        .collect();
    assert_eq!(
        spelled,
        [
            ("bot:gamma", "bot:gamma".to_string()),
            ("now", "now".to_string()),
            ("bot:delta", "bot:delta".to_string()),
        ],
        "each value is read back from the place it was said to sit"
    );
    assert!(
        value_words("status = ").is_err(),
        "an unreadable line has no values"
    );
}

/// **A doubled equals sign says equality is one.** The token after the first is
/// not a value, and the refusal names what to write.
#[test]
fn a_doubled_equals_sign_says_equality_is_written_with_one() {
    let words = refusal("status == done");
    assert!(words.contains("=="), "{words}");
    assert_eq!(tree("status = done"), Filter::equals("status", "done"));
}

/// **A quoted date or a quoted `today` is literal text, and the refusal of an
/// ordering says so.** The same words without quotes are what the ordering reads.
#[test]
fn a_quoted_operand_of_an_ordering_says_a_quote_makes_it_literal_text() {
    for text in [
        "due < \"2026-01-01\"",
        "due < \"today\"",
        "due >= \"today+3d\"",
    ] {
        let words = refusal(text);
        assert!(words.contains("literal"), "{text}: {words}");
    }
    tree("due < 2026-01-01");
    tree("due < today");
}

/// The inputs a call bound: a value for each name given, and no value for the
/// names given as `None`, which a call left out.
fn bound(pairs: &[(&str, Option<&str>)]) -> crate::memory::view_inputs::Bound {
    use crate::memory::view_inputs::{Bound, Value};
    Bound::new(
        pairs
            .iter()
            .map(|(name, text)| {
                (
                    name.to_string(),
                    text.map(|text| Value {
                        text: text.to_string(),
                        handle: None,
                    }),
                )
            })
            .collect(),
    )
}

fn filled(text: &str, pairs: &[(&str, Option<&str>)]) -> Option<String> {
    with_inputs(text, &bound(pairs)).unwrap_or_else(|e| panic!("'{text}' should fill: {e:?}"))
}

/// **A `$name` becomes the value bound to it**, in an equality, an ordering and a
/// `contains`, and the filled line reads as the line written with the value in
/// it. **Paired** with the dropping case below: the same line with the input left
/// out loses exactly the term that used it.
#[test]
fn an_input_is_replaced_by_the_value_bound_to_it() {
    let line = filled(
        "status != done && project = $project",
        &[("project", Some("project:atlas"))],
    );
    assert_eq!(
        line.as_deref(),
        Some("status != done && project = project:atlas")
    );
    assert_eq!(
        tree(line.as_deref().unwrap()),
        tree("status != done && project = project:atlas")
    );
    assert_eq!(
        filled("due <= $when", &[("when", Some("2026-10-17"))]).as_deref(),
        Some("due <= 2026-10-17")
    );
    // Words with a space in them stay one value.
    assert_eq!(
        filled("title contains $q", &[("q", Some("two words"))]).as_deref(),
        Some("title contains \"two words\"")
    );
    assert_eq!(
        tree("title contains \"two words\""),
        Filter::contains("title", "two words")
    );
}

/// **A term that uses an input the call left out is dropped**, and so is a group
/// or a negation left holding nothing. What is left still reads, with the same
/// grouping it had. **Paired**: a term that uses no input, or one the call did
/// bind, is kept.
#[test]
fn a_term_using_a_left_out_input_is_dropped_with_what_it_leaves_empty() {
    let left_out = &[("project", None)];
    assert_eq!(
        filled("status != done && project = $project", left_out).as_deref(),
        Some("status != done")
    );
    assert_eq!(
        filled("project = $project && status != done", left_out).as_deref(),
        Some("status != done")
    );
    assert_eq!(
        filled(
            "(project = $project || owner = $who) && status != done",
            &[("project", None), ("who", Some("bot:gamma"))]
        )
        .as_deref(),
        Some("(owner = bot:gamma) && status != done")
    );
    assert_eq!(
        filled("(project = $project) && status != done", left_out).as_deref(),
        Some("status != done")
    );
    assert_eq!(
        filled("!(project = $project) && status != done", left_out).as_deref(),
        Some("status != done")
    );
    assert_eq!(
        filled("tags in ($a, b) && status != done", &[("a", None)]).as_deref(),
        Some("status != done")
    );
    // Nothing left to ask.
    assert_eq!(filled("project = $project", left_out), None);
    assert_eq!(
        filled(
            "project = $project || owner = $who",
            &[("project", None), ("who", None)]
        ),
        None
    );
    // The line read with the left-out term dropped still parses to the rest.
    assert_eq!(
        tree(
            filled("a = 1 && b = $b || c = 3", &[("b", None)])
                .unwrap()
                .as_str()
        ),
        tree("a = 1 || c = 3")
    );
}

/// **A `$word` the view did not declare is a word.** Nothing is replaced or
/// dropped for it, and a line with no inputs bound reads exactly as written.
#[test]
fn a_word_that_is_no_declared_input_is_left_as_written() {
    assert_eq!(
        filled("price = $5 && name = $other", &[("project", Some("x"))]).as_deref(),
        Some("price = $5 && name = $other")
    );
    assert_eq!(
        with_inputs("status != done", &bound(&[]))
            .unwrap()
            .as_deref(),
        Some("status != done")
    );
}

/// **A saved expression is checked with each `$name` standing for a value**, in
/// an equality, a list, a `contains` and an ordering, and `$name`s are listed in
/// the order written. **Paired**: a line that does not read with its names
/// replaced is refused as it always was.
#[test]
fn a_saved_expression_reads_with_its_inputs_as_values() {
    for line in [
        "project = $project",
        "due <= $when && status != done",
        "tags in ($a, b)",
        "title contains $q",
        "!(owner = $who)",
    ] {
        assert!(reads_saved(line).is_ok(), "{line} should read");
    }
    for line in [
        "due <= soon",
        "project =",
        "project = $project &&",
        "due <= \"$when\"",
    ] {
        assert!(reads_saved(line).is_err(), "{line} should not read");
    }
    assert_eq!(
        placeholders("project = $project && (due <= $when || owner = $who) && price = $5"),
        vec!["project", "when", "who"]
    );
    assert!(placeholders("status != done").is_empty());
}
