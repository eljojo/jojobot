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
