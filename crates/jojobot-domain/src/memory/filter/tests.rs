use super::*;
use crate::memory::types::Field;

/// A bag of fields from pairs.
fn bag(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// **Three pieces of work, one for each of three bots**, with the statuses now,
/// next and done. Each is a name and the fields it holds.
fn work() -> Vec<(&'static str, BTreeMap<String, String>)> {
    vec![
        ("gamma", bag(&[("status", "now")])),
        ("delta", bag(&[("status", "next")])),
        ("epsilon", bag(&[("status", "done")])),
    ]
}

/// The names of the work a filter keeps, in the order they came.
fn kept(filter: &Filter) -> Vec<&'static str> {
    work()
        .into_iter()
        .filter(|(_, fields)| filter.holds(fields))
        .map(|(name, _)| name)
        .collect()
}

/// A type declaring one key to hold a value type.
fn declaring(key: &str, holds: ValueType) -> DeclaredType {
    DeclaredType::new("tracked", vec![Field::new(key, holds)])
}

/// A type declaring one key to hold a list of text.
fn declaring_a_list(key: &str) -> DeclaredType {
    DeclaredType::new("tracked", vec![Field::listing(key, ValueType::Text)])
}

/// **Any keeps the work that satisfies one of its branches.** Now or next keeps
/// two; the third, done, is the negative, and a single branch keeps only its own.
#[test]
fn any_keeps_what_satisfies_one_branch() {
    let either = Filter::Any(vec![
        Filter::equals("status", "now"),
        Filter::equals("status", "next"),
    ]);
    assert_eq!(kept(&either), ["gamma", "delta"]);
    assert_eq!(kept(&Filter::equals("status", "now")), ["gamma"]);
}

/// **Not keeps what its inner filter refuses.** Not done keeps two, and its
/// positive is the one done thing the inner filter keeps.
#[test]
fn not_keeps_what_the_inner_filter_refuses() {
    let done = Filter::equals("status", "done");
    assert_eq!(kept(&done), ["epsilon"]);
    assert_eq!(kept(&Filter::negated(done)), ["gamma", "delta"]);
}

/// **Not is the complement over the population, and `!=` is not.** A thing with
/// no status is kept by Not(status = done) and refused by status != done, which
/// asks that the key is held. The held things behave alike under both.
#[test]
fn not_includes_what_lacks_the_key_and_not_equals_does_not() {
    let lacking = bag(&[("owner", "gamma")]);
    let held_other = bag(&[("status", "now")]);
    let held_done = bag(&[("status", "done")]);
    let complement = Filter::negated(Filter::equals("status", "done"));
    let differs = Filter::not_equals("status", "done");
    assert!(complement.holds(&lacking));
    assert!(!differs.holds(&lacking));
    assert!(complement.holds(&held_other) && differs.holds(&held_other));
    assert!(!complement.holds(&held_done) && !differs.holds(&held_done));
    // The same for any-of.
    let none_of = Filter::not_in("status", &["done", "next"]);
    assert!(!none_of.holds(&lacking));
    assert!(none_of.holds(&held_other));
    assert!(!none_of.holds(&held_done));
    assert!(Filter::negated(Filter::is_in("status", &["done", "next"])).holds(&lacking));
}

/// **All wants every branch, and an empty All and an empty Any differ.** Asking
/// nothing holds; an Any with nothing to be satisfied by does not.
#[test]
fn all_wants_every_branch_and_the_empty_lists_differ() {
    let both = Filter::All(vec![
        Filter::exists("status"),
        Filter::negated(Filter::equals("status", "done")),
    ]);
    assert_eq!(kept(&both), ["gamma", "delta"]);
    assert_eq!(
        kept(&Filter::All(Vec::new())).len(),
        3,
        "asking nothing holds"
    );
    assert!(kept(&Filter::Any(Vec::new())).is_empty());
}

/// **Contains finds one whole item of a list and never a piece of one.**
/// `skills.rs` is in `essay.rs, skills.rs`; `skills` is not, though it is a
/// substring of the second item; and `essay.rs` is, so the list is read by item.
#[test]
fn contains_matches_a_whole_item_and_never_a_substring() {
    let touches = bag(&[("touches", "essay.rs, skills.rs")]);
    assert!(Filter::contains("touches", "skills.rs").holds(&touches));
    assert!(Filter::contains("touches", "essay.rs").holds(&touches));
    assert!(!Filter::contains("touches", "skills").holds(&touches));
    assert!(!Filter::contains("touches", "rs").holds(&touches));
    assert!(!Filter::contains("touches", "skills.rs").holds(&bag(&[("other", "skills.rs")])));
}

/// **In holds for a value in the set and for nothing else.**
#[test]
fn in_holds_for_a_value_in_the_set() {
    let in_flight = Filter::is_in("status", &["now", "next"]);
    assert_eq!(kept(&in_flight), ["gamma", "delta"]);
    assert!(kept(&Filter::is_in("status", &[])).is_empty());
}

/// **An ordering is judged on the typed values, never as equality.** A larger
/// number is greater, an equal one is not, an equal one is at least, and a value
/// that is no number is not greater than anything.
#[test]
fn an_ordering_reads_the_values_it_was_licensed_for() {
    let greater = Filter::comparing("cost", Compare::Greater, "10");
    assert!(greater.holds(&bag(&[("cost", "11")])));
    assert!(
        !greater.holds(&bag(&[("cost", "10")])),
        "equal is not greater"
    );
    assert!(!greater.holds(&bag(&[("cost", "9")])));
    assert!(!greater.holds(&bag(&[("cost", "plenty")])));
    let at_least = Filter::ordering("cost", Op::GreaterOrEqual, ValueType::Number, "10");
    assert!(at_least.holds(&bag(&[("cost", "10")])), "equal is at least");
    assert!(!at_least.holds(&bag(&[("cost", "9")])));
    let at_most = Filter::ordering("cost", Op::LessOrEqual, ValueType::Number, "10");
    assert!(at_most.holds(&bag(&[("cost", "10")])) && at_most.holds(&bag(&[("cost", "9")])));
    assert!(!at_most.holds(&bag(&[("cost", "11")])));
    let after = Filter::comparing("due", Compare::After, "2026-01-01");
    assert!(after.holds(&bag(&[("due", "2026-02-01")])));
    assert!(!after.holds(&bag(&[("due", "2025-12-31")])));
}

/// **A leaf asked of the value under any key holds when some value passes.**
#[test]
fn a_leaf_with_no_key_asks_about_any_value() {
    let anywhere = Filter::anywhere(Test::Equals("now".into()));
    assert!(anywhere.holds(&bag(&[("status", "now"), ("owner", "gamma")])));
    assert!(!anywhere.holds(&bag(&[("status", "next")])));
}

/// **A tree asked of records is answered by one record.** Two records each
/// holding half of an All do not hold it; one record holding both does.
#[test]
fn a_tree_asked_of_records_is_answered_by_one_record() {
    let both = Filter::All(vec![
        Filter::equals("status", "now"),
        Filter::equals("owner", "gamma"),
    ]);
    let halves = [bag(&[("status", "now")]), bag(&[("owner", "gamma")])];
    assert!(!both.held_by_a_record(halves.iter()));
    let whole = [bag(&[("status", "now"), ("owner", "gamma")])];
    assert!(both.held_by_a_record(whole.iter()));
}

/// **An ordering no declaration licenses is an error that names the declaration
/// that would.** Declared, the same leaf passes; declared as the wrong kind, it
/// is refused again; and equality needs none.
#[test]
fn an_ordering_no_declaration_licenses_is_refused_by_name() {
    let greater = Filter::comparing("cost", Compare::Greater, "10");
    let refused = greater
        .validate(&[], None)
        .expect_err("no type declares cost");
    let said = refused.to_string();
    assert!(
        said.contains("cost") && said.contains("number"),
        "the refusal names the key and the type that would license it: {said}"
    );
    assert!(
        greater
            .validate(&[declaring("cost", ValueType::Number)], None)
            .is_ok()
    );
    assert!(
        greater
            .validate(&[declaring("cost", ValueType::Text)], None)
            .is_err(),
        "a key declared text licenses no ordering"
    );
    assert!(Filter::equals("cost", "10").validate(&[], None).is_ok());
    assert!(Filter::is_in("cost", &["10"]).validate(&[], None).is_ok());
    assert!(Filter::exists("cost").validate(&[], None).is_ok());
    // The licensing reaches a leaf inside a combination.
    let buried = Filter::negated(Filter::Any(vec![Filter::exists("status"), greater]));
    assert!(buried.validate(&[], None).is_err());
}

/// **Contains on a key not declared a list is refused, naming the list
/// declaration that would license it.** Declared a list, it passes; declared a
/// single value of the same type, it does not.
#[test]
fn contains_on_a_key_not_declared_a_list_is_refused_by_name() {
    let contains = Filter::contains("touches", "skills.rs");
    let said = contains
        .validate(&[], None)
        .expect_err("no type declares touches")
        .to_string();
    assert!(
        said.contains("touches") && said.contains("list"),
        "the refusal names the key and the list declaration: {said}"
    );
    assert!(
        contains
            .validate(&[declaring("touches", ValueType::Text)], None)
            .is_err(),
        "a key declared a single text is not a list"
    );
    assert!(
        contains
            .validate(&[declaring_a_list("touches")], None)
            .is_ok()
    );
}

/// **When a selection names a kind, that kind's declaration alone governs.**
/// A declaration under another name licenses nothing then, and licenses as
/// before when no kind is named.
#[test]
fn a_named_kind_is_governed_by_its_own_declaration_alone() {
    let greater = Filter::comparing("cost", Compare::Greater, "10");
    let tracked = [declaring("cost", ValueType::Number)];
    assert!(
        greater.validate(&tracked, None).is_ok(),
        "any type licenses"
    );
    assert!(greater.validate(&tracked, Some("tracked")).is_ok());
    assert!(
        greater.validate(&tracked, Some("other")).is_err(),
        "the named kind has no declaration of cost"
    );
}

/// **A comparison against a value that is no value of its type is malformed.**
#[test]
fn an_ordering_against_the_wrong_kind_of_value_is_refused() {
    let declared = [declaring("due", ValueType::Date)];
    assert!(
        Filter::comparing("due", Compare::After, "soon")
            .validate(&declared, None)
            .is_err()
    );
    assert!(
        Filter::comparing("due", Compare::After, "2026-01-01")
            .validate(&declared, None)
            .is_ok()
    );
    assert!(
        Filter::ordering("due", Op::Less, ValueType::Text, "a")
            .validate(&[declaring("due", ValueType::Text)], None)
            .is_err(),
        "only a date and a number are orderable"
    );
}

/// **A leaf that asks nothing is refused.** An empty key, an existence test
/// with no key, and a negation or a list question that names no key; a value
/// asked under any key is fine.
#[test]
fn a_leaf_that_asks_nothing_is_refused() {
    assert!(Filter::exists("  ").validate(&[], None).is_err());
    assert!(Filter::anywhere(Test::Exists).validate(&[], None).is_err());
    assert!(
        Filter::anywhere(Test::NotEquals("now".into()))
            .validate(&[], None)
            .is_err()
    );
    assert!(
        Filter::anywhere(Test::Contains("a".into()))
            .validate(&[], None)
            .is_err()
    );
    assert!(
        Filter::anywhere(Test::Equals("now".into()))
            .validate(&[], None)
            .is_ok()
    );
}

/// The items an ordering by `key` leaves, by their `name`, and what it said.
fn ordered(
    order: &Order,
    items: &[(&'static str, Option<&str>)],
    declared: &[DeclaredType],
) -> (Vec<&'static str>, Ordered) {
    let mut items: Vec<(&'static str, BTreeMap<String, String>)> = items
        .iter()
        .map(|(name, value)| {
            let fields = match value {
                Some(value) => bag(&[(order.key.as_str(), value)]),
                None => BTreeMap::new(),
            };
            (*name, fields)
        })
        .collect();
    let said = order.sort(&mut items, |(_, fields)| Some(fields), declared, None);
    (items.into_iter().map(|(name, _)| name).collect(), said)
}

/// **A key declared to hold a number orders as one, and the same key undeclared
/// orders as text.** Two comes before ten with the declaration; ten comes before
/// two without it, and the undeclared case says so, once.
#[test]
fn a_declared_number_orders_as_a_number_and_an_undeclared_one_as_text() {
    let items = [("ten", Some("10")), ("two", Some("2")), ("five", Some("5"))];
    let up = Order::new("size", Direction::Ascending);
    let (declared_order, note) = ordered(&up, &items, &[declaring("size", ValueType::Number)]);
    assert_eq!(declared_order, ["two", "five", "ten"]);
    assert!(!note.read_as_text_though_it_looks_otherwise);
    let (text_order, note) = ordered(&up, &items, &[]);
    assert_eq!(text_order, ["ten", "two", "five"]);
    assert!(
        note.read_as_text_though_it_looks_otherwise,
        "numbers read as text are the case the answer owes one line about"
    );
    let down = Order::new("size", Direction::Descending);
    assert_eq!(
        ordered(&down, &items, &[declaring("size", ValueType::Number)]).0,
        ["ten", "five", "two"]
    );
}

/// **An undeclared key holding only words orders as text with no note**, and so
/// does a key the author declared text although its values look like numbers.
#[test]
fn text_that_looks_like_nothing_else_carries_no_note() {
    let words = [("b", Some("pear")), ("a", Some("apple"))];
    let up = Order::new("fruit", Direction::Ascending);
    let (order, note) = ordered(&up, &words, &[]);
    assert_eq!(order, ["a", "b"]);
    assert!(!note.read_as_text_though_it_looks_otherwise);
    let numbers = [("ten", Some("10")), ("two", Some("2"))];
    let (order, note) = ordered(
        &Order::new("size", Direction::Ascending),
        &numbers,
        &[declaring("size", ValueType::Text)],
    );
    assert_eq!(order, ["ten", "two"]);
    assert!(
        !note.read_as_text_though_it_looks_otherwise,
        "a declaration that says text is the author's word"
    );
    // A date-looking value is the other case that earns the note.
    let dates = [
        ("later", Some("2026-10-02")),
        ("sooner", Some("2026-09-30")),
    ];
    let (_, note) = ordered(&Order::new("due", Direction::Ascending), &dates, &[]);
    assert!(note.read_as_text_though_it_looks_otherwise);
}

/// **A declared date orders as a date.**
#[test]
fn a_declared_date_orders_as_a_date() {
    let items = [
        ("later", Some("2026-10-02")),
        ("earlier", Some("2026-09-30")),
    ];
    let up = Order::new("due", Direction::Ascending);
    assert_eq!(
        ordered(&up, &items, &[declaring("due", ValueType::Date)]).0,
        ["earlier", "later"]
    );
}

/// **Things that do not hold the key come last in either direction.** So do
/// things whose value is not what the key was declared to hold. The ones that
/// hold it are the positive each direction rests on.
#[test]
fn things_without_the_key_come_last_in_either_direction() {
    let items = [
        ("none", None),
        ("two", Some("2")),
        ("mess", Some("plenty")),
        ("ten", Some("10")),
    ];
    let declared = [declaring("size", ValueType::Number)];
    assert_eq!(
        ordered(&Order::new("size", Direction::Ascending), &items, &declared).0,
        ["two", "ten", "none", "mess"]
    );
    assert_eq!(
        ordered(
            &Order::new("size", Direction::Descending),
            &items,
            &declared
        )
        .0,
        ["ten", "two", "none", "mess"]
    );
}

/// **Ties keep the order the items came in, in both directions.**
#[test]
fn ties_keep_the_order_they_came_in() {
    let items = [("a", Some("1")), ("b", Some("1")), ("c", Some("0"))];
    let declared = [declaring("size", ValueType::Number)];
    assert_eq!(
        ordered(&Order::new("size", Direction::Ascending), &items, &declared).0,
        ["c", "a", "b"]
    );
    assert_eq!(
        ordered(
            &Order::new("size", Direction::Descending),
            &items,
            &declared
        )
        .0,
        ["a", "b", "c"]
    );
}

/// **A key two types declare differently orders as text.** No one reading is the
/// declared one, and the numbers read as text earn the note.
#[test]
fn a_key_declared_two_ways_orders_as_text() {
    let items = [("ten", Some("10")), ("two", Some("2"))];
    let declared = [
        declaring("size", ValueType::Number),
        declaring("size", ValueType::Date),
    ];
    let (order, note) = ordered(&Order::new("size", Direction::Ascending), &items, &declared);
    assert_eq!(order, ["ten", "two"]);
    assert!(note.read_as_text_though_it_looks_otherwise);
}

/// **A kind named by the selection governs how its key is read.** The same key
/// is a number under the kind that declares it and text under one that does not.
#[test]
fn the_kind_a_selection_names_decides_how_a_key_is_ordered() {
    let items = [("ten", Some("10")), ("two", Some("2"))];
    let declared = [declaring("size", ValueType::Number)];
    let under = |kind: Option<&str>| {
        let mut rows: Vec<(&str, BTreeMap<String, String>)> = items
            .iter()
            .map(|(name, value)| (*name, bag(&[("size", value.unwrap())])))
            .collect();
        Order::new("size", Direction::Ascending).sort(
            &mut rows,
            |(_, fields)| Some(fields),
            &declared,
            kind,
        );
        rows.into_iter().map(|(name, _)| name).collect::<Vec<_>>()
    };
    assert_eq!(under(Some("tracked")), ["two", "ten"]);
    assert_eq!(under(Some("other")), ["ten", "two"]);
}
