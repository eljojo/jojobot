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

/// **The question a reference list asks of the things it names is validated
/// like any other.** An ordering no declaration licenses is refused inside it,
/// and declared the same leaf passes. The things named are not of the selected
/// kind, so that kind's declaration does not govern them: a type that licenses
/// the ordering for a thing of any kind does.
#[test]
fn the_question_inside_a_reference_list_is_validated_too() {
    let inside = Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::All,
        then: Box::new(Filter::comparing("cost", Compare::Greater, "10")),
    };
    let refused = inside
        .validate(&[], None)
        .expect_err("no type declares cost");
    assert!(refused.to_string().contains("cost"), "{refused}");
    assert!(
        inside
            .validate(&[declaring("cost", ValueType::Number)], None)
            .is_ok()
    );
    // The selected kind says text for its own cost, and a type of no kind says
    // number: the leaf inside the list is not asked of the selected kind.
    let kinds_own = DeclaredType::new("work", vec![Field::new("cost", ValueType::Text)]);
    let loose = declaring("cost", ValueType::Number);
    assert!(
        inside
            .validate(&[kinds_own.clone(), loose.clone()], Some("work"))
            .is_ok()
    );
    assert!(
        Filter::comparing("cost", Compare::Greater, "10")
            .validate(&[kinds_own, loose], Some("work"))
            .is_err(),
        "the same leaf outside the list is governed by the selected kind"
    );
    // The finished leaf has nothing to validate, in or out of a list.
    assert!(Filter::Finished.validate(&[], None).is_ok());
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

/// **A declaration named like a kind belongs to that kind, and a type under any
/// other name is vocabulary.** Vocabulary licenses any kind that carries its
/// key; another kind's declaration licenses nothing; and where the kind and a
/// vocabulary type disagree about one key, the kind's declaration wins.
#[test]
fn a_kind_named_declaration_belongs_to_its_kind() {
    // Which names are kinds is what a booted process knows.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let greater = Filter::comparing("cost", Compare::Greater, "10");
    let tracked = [declaring("cost", ValueType::Number)];
    assert!(
        greater.validate(&tracked, None).is_ok(),
        "any type licenses"
    );
    assert!(
        greater.validate(&tracked, Some("other")).is_ok(),
        "a type no kind is named for is vocabulary, and licenses any kind"
    );
    let work = |holds| DeclaredType::new("work", vec![Field::new("cost", holds)]);
    assert!(
        greater
            .validate(&[work(ValueType::Number)], Some("work"))
            .is_ok()
    );
    assert!(
        greater
            .validate(&[work(ValueType::Number)], Some("thing"))
            .is_err(),
        "work's declaration of cost is work's"
    );
    assert!(
        greater
            .validate(
                &[work(ValueType::Date), declaring("cost", ValueType::Number)],
                Some("work")
            )
            .is_err(),
        "the kind declared cost a date, and that is its word against a vocabulary number"
    );
    assert!(
        greater
            .validate(
                &[work(ValueType::Number), declaring("cost", ValueType::Date)],
                Some("work")
            )
            .is_ok(),
        "and a vocabulary date does not undo the kind's number"
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
/// is a number under the kind that declares it, and text under another kind: a
/// declaration named like a kind belongs to it. A type under any other name is
/// vocabulary and reads the key as declared for every kind.
#[test]
fn the_kind_a_selection_names_decides_how_a_key_is_ordered() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let items = [("ten", Some("10")), ("two", Some("2"))];
    let under = |declared: &[DeclaredType], kind: Option<&str>| {
        let mut rows: Vec<(&str, BTreeMap<String, String>)> = items
            .iter()
            .map(|(name, value)| (*name, bag(&[("size", value.unwrap())])))
            .collect();
        Order::new("size", Direction::Ascending).sort(
            &mut rows,
            |(_, fields)| Some(fields),
            declared,
            kind,
        );
        rows.into_iter().map(|(name, _)| name).collect::<Vec<_>>()
    };
    let work = [DeclaredType::new(
        "work",
        vec![Field::new("size", ValueType::Number)],
    )];
    assert_eq!(under(&work, Some("work")), ["two", "ten"]);
    assert_eq!(under(&work, Some("thing")), ["ten", "two"]);
    let vocabulary = [declaring("size", ValueType::Number)];
    assert_eq!(under(&vocabulary, Some("thing")), ["two", "ten"]);
}

/// **A set of named things for a reference list to name.** Each is a kind, the
/// fields it holds, and whether it was archived.
struct Things {
    named: BTreeMap<String, (String, BTreeMap<String, String>, bool)>,
    instance: BTreeMap<String, String>,
}

impl Things {
    fn new() -> Things {
        Things {
            named: BTreeMap::new(),
            instance: BTreeMap::new(),
        }
    }

    fn with(mut self, handle: &str, kind: &str, fields: &[(&str, &str)], archived: bool) -> Things {
        self.named.insert(
            handle.to_string(),
            (kind.to_string(), bag(fields), archived),
        );
        self
    }

    fn mapping(mut self, word: &str, to: &str) -> Things {
        self.instance
            .insert(format!("status_word/{word}"), to.to_string());
        self
    }

    /// Is the holder with these fields satisfied by the filter.
    fn holds(&self, filter: &Filter, kind: &str, fields: &[(&str, &str)]) -> bool {
        let fields = bag(fields);
        filter.holds_in(
            &Subject {
                kind,
                fields: &fields,
                archived: false,
            },
            self,
        )
    }
}

impl Lookup for Things {
    fn thing(&self, handle: &str) -> Option<Subject<'_>> {
        self.named
            .get(handle)
            .map(|(kind, fields, archived)| Subject {
                kind,
                fields,
                archived: *archived,
            })
    }

    fn instance(&self) -> &BTreeMap<String, String> {
        &self.instance
    }
}

/// Every work item `depends_on` names is finished.
fn all_finished() -> Filter {
    Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::All,
        then: Box::new(Filter::Finished),
    }
}

/// **All holds when every named thing is finished, and the unfinished one is its
/// negative.** The same holder, the same list, one name changed.
#[test]
fn all_over_a_list_needs_every_named_thing() {
    let things = Things::new()
        .with("work:phi", "work", &[("status", "done")], false)
        .with("work:sigma", "work", &[("status", "done")], false)
        .with("work:tau", "work", &[("status", "now")], false);
    let both_done = [("depends_on", "work:phi, work:sigma")];
    let one_open = [("depends_on", "work:phi, work:tau")];
    assert!(things.holds(&all_finished(), "work", &both_done));
    assert!(!things.holds(&all_finished(), "work", &one_open));
}

/// **Any holds when one named thing is finished.** Its negatives: none finished,
/// and an empty list, which has nothing to satisfy it.
#[test]
fn any_over_a_list_needs_one_named_thing() {
    let things = Things::new()
        .with("work:phi", "work", &[("status", "done")], false)
        .with("work:tau", "work", &[("status", "now")], false);
    let any_finished = Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::Any,
        then: Box::new(Filter::Finished),
    };
    assert!(things.holds(
        &any_finished,
        "work",
        &[("depends_on", "work:tau, work:phi")]
    ));
    assert!(!things.holds(&any_finished, "work", &[("depends_on", "work:tau")]));
    assert!(!things.holds(&any_finished, "work", &[("depends_on", "")]));
    assert!(!things.holds(&any_finished, "work", &[]));
}

/// **All over an empty list holds**, whether the key is absent or holds nothing.
/// Its pair: the same filter over a one-item list of an unfinished thing fails.
#[test]
fn all_over_an_empty_list_holds() {
    let things = Things::new().with("work:tau", "work", &[("status", "now")], false);
    assert!(things.holds(&all_finished(), "work", &[]));
    assert!(things.holds(&all_finished(), "work", &[("depends_on", "")]));
    assert!(!things.holds(&all_finished(), "work", &[("depends_on", "work:tau")]));
}

/// **A handle that names nothing fails**, under All and under Any, and it fails
/// beside a finished neighbour: one good name does not cover for a bad one.
#[test]
fn a_dangling_handle_fails() {
    let things = Things::new().with("work:phi", "work", &[("status", "done")], false);
    assert!(things.holds(&all_finished(), "work", &[("depends_on", "work:phi")]));
    assert!(!things.holds(
        &all_finished(),
        "work",
        &[("depends_on", "work:phi, work:atlas-root")]
    ));
    let any_finished = Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::Any,
        then: Box::new(Filter::Finished),
    };
    assert!(!things.holds(&any_finished, "work", &[("depends_on", "work:atlas-root")]));
}

/// **An archived thing is not finished**, although its status says done. Its pair
/// is the same thing, not archived.
#[test]
fn an_archived_thing_is_not_finished() {
    let live = Things::new().with("work:phi", "work", &[("status", "done")], false);
    let archived = Things::new().with("work:phi", "work", &[("status", "done")], true);
    let wants = [("depends_on", "work:phi")];
    assert!(live.holds(&all_finished(), "work", &wants));
    assert!(!archived.holds(&all_finished(), "work", &wants));
}

/// **Finished reads the instance's status words.** A word mapped to done
/// finishes; an unmapped word does not; and a kind that finishes nothing never
/// does, whatever its status says.
#[test]
fn finished_follows_the_instance_status_words() {
    let wants = [("depends_on", "work:phi")];
    let mapped = Things::new()
        .with("work:phi", "work", &[("status", "shipped")], false)
        .mapping("shipped", "done");
    assert!(mapped.holds(&all_finished(), "work", &wants));
    let unmapped = Things::new().with("work:phi", "work", &[("status", "shipped")], false);
    assert!(!unmapped.holds(&all_finished(), "work", &wants));
    let a_person = Things::new().with("person:bart", "person", &[("status", "done")], false);
    assert!(!a_person.holds(&all_finished(), "work", &[("depends_on", "person:bart")]));
}

/// **The question runs on each named thing and not on the holder.** The holder
/// is done and its dependency is not, so an equality on the status keeps the
/// holder only when it is asked of the dependency's fields correctly.
#[test]
fn the_question_is_asked_of_the_named_thing() {
    let things = Things::new().with("work:tau", "work", &[("status", "now")], false);
    let wants_done = Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::All,
        then: Box::new(Filter::equals("status", "done")),
    };
    let holder = [("status", "done"), ("depends_on", "work:tau")];
    assert!(!things.holds(&wants_done, "work", &holder));
    let wants_now = Filter::Refs {
        key: "depends_on".into(),
        quantifier: Quantifier::All,
        then: Box::new(Filter::equals("status", "now")),
    };
    assert!(things.holds(&wants_now, "work", &holder));
}

/// **Finished at the top asks about the holder itself.** Done holds, now does
/// not.
#[test]
fn finished_at_the_top_asks_about_the_holder() {
    let things = Things::new();
    assert!(things.holds(&Filter::Finished, "work", &[("status", "done")]));
    assert!(!things.holds(&Filter::Finished, "work", &[("status", "now")]));
}

/// **Without a lookup nothing resolves.** `holds` has none, so a list with an
/// item fails and an empty list under All holds; a tree needing other things is
/// not answered true by being asked without them.
#[test]
fn without_a_lookup_nothing_resolves() {
    assert!(!all_finished().holds(&bag(&[("depends_on", "work:phi")])));
    assert!(all_finished().holds(&bag(&[])));
}

// ── What the index can answer for a tree ────────────────────────────────────

/// A term as the index holds it: a key, and the value when equality is asked.
fn term(key: &str, value: Option<&str>) -> Term {
    Term {
        key: key.to_string(),
        value: value.map(str::to_string),
        ask: None,
    }
}

/// A term the store answers: the test a leaf makes, asked of the key.
fn asked(key: &str, test: Test) -> Term {
    Term {
        key: key.to_string(),
        value: None,
        ask: Some(test),
    }
}

fn today() -> jiff::civil::Date {
    jiff::civil::date(2026, 2, 1)
}

/// **A tree, read the way a caller writes it, and routed.**
fn routed(text: &str) -> Option<Routed> {
    expression::parse(text, today())
        .expect("the expression reads")
        .routed()
}

#[test]
fn an_equality_a_set_and_a_bare_key_route_exactly() {
    let equal = routed("status = now").expect("an equality routes");
    assert_eq!(equal.any_of, vec![vec![term("status", Some("now"))]]);
    assert!(equal.exact, "an equality is what the index holds");

    let any_of_two = routed("status in (now, next)").expect("a set routes");
    assert_eq!(
        any_of_two.any_of,
        vec![
            vec![term("status", Some("now"))],
            vec![term("status", Some("next"))]
        ]
    );
    assert!(any_of_two.exact);

    let carried = routed("assignee").expect("a bare key routes");
    assert_eq!(carried.any_of, vec![vec![term("assignee", None)]]);
    assert!(carried.exact);
}

#[test]
fn an_and_of_servable_conjuncts_routes_as_one_conjunction_and_stays_exact() {
    let both = routed("status = now && assignee = bea").expect("an and routes");
    assert_eq!(
        both.any_of,
        vec![vec![
            term("status", Some("now")),
            term("assignee", Some("bea"))
        ]]
    );
    assert!(both.exact);
}

#[test]
fn an_and_with_a_conjunct_nothing_can_serve_routes_the_rest_and_is_not_exact() {
    let mixed = routed("status = now && !(assignee = bea)").expect("the servable half routes");
    assert_eq!(mixed.any_of, vec![vec![term("status", Some("now"))]]);
    assert!(!mixed.exact, "the evaluator has to decide the rest");

    let unservable = routed("!(status = now) && !(status = next)");
    assert!(unservable.is_none(), "a negation routes to nothing");
}

/// **A leaf the index of the keys cannot hold is asked of the store**, and the
/// store answers it exactly: `!=`, `not in`, `contains` and the four orderings.
#[test]
fn a_leaf_only_the_store_can_answer_is_asked_of_the_store_and_stays_exact() {
    let not_equal = routed("status != done").expect("a != routes");
    assert_eq!(
        not_equal.any_of,
        vec![vec![asked("status", Test::NotEquals("done".into()))]]
    );
    assert!(not_equal.exact);

    let none_of = Filter::not_in("status", &["done", "waiting"])
        .routed()
        .expect("a not in routes");
    assert_eq!(
        none_of.any_of,
        vec![vec![asked(
            "status",
            Test::NotIn(vec!["done".into(), "waiting".into()])
        )]]
    );

    let listed = routed("tags contains kitchen").expect("a contains routes");
    assert_eq!(
        listed.any_of,
        vec![vec![asked("tags", Test::Contains("kitchen".into()))]]
    );
    assert!(listed.exact);

    let ranged = routed("due <= 2026-02-01").expect("an ordering routes");
    assert_eq!(
        ranged.any_of,
        vec![vec![asked(
            "due",
            Test::Order(Op::LessOrEqual, ValueType::Date, "2026-02-01".into())
        )]]
    );
    assert!(ranged.exact);

    let mixed = routed("status = now && assignee != bea").expect("both halves route");
    assert_eq!(
        mixed.any_of,
        vec![vec![
            term("status", Some("now")),
            asked("assignee", Test::NotEquals("bea".into()))
        ]]
    );
    assert!(mixed.exact, "the index and the store decide every conjunct");
}

#[test]
fn an_or_routes_only_when_every_branch_does() {
    let either = routed("status = now || status = next").expect("an or of equalities routes");
    assert_eq!(
        either.any_of,
        vec![
            vec![term("status", Some("now"))],
            vec![term("status", Some("next"))]
        ]
    );
    assert!(either.exact);

    assert!(
        routed("status = now || !(status = next)").is_none(),
        "one branch the index cannot serve loses every other branch's hits"
    );
}

#[test]
fn an_and_over_an_or_distributes_and_a_wide_product_is_refused() {
    let wide = routed("(status = now || status = next) && assignee = bea").expect("routes");
    assert_eq!(
        wide.any_of,
        vec![
            vec![term("status", Some("now")), term("assignee", Some("bea"))],
            vec![term("status", Some("next")), term("assignee", Some("bea"))],
        ]
    );
    let big = (0..6)
        .map(|n| format!("(k{n} in (a, b, c, d))"))
        .collect::<Vec<_>>()
        .join(" && ");
    assert!(
        routed(&big).is_none(),
        "4 to the 6th conjunctions is not worth the searches"
    );
}

#[test]
fn a_value_asked_under_any_key_and_a_negation_do_not_route() {
    assert!(
        Filter::anywhere(Test::Equals("bea".into()))
            .routed()
            .is_none(),
        "a value under any key names no key to look up"
    );
    assert!(Filter::Any(Vec::new()).routed().is_none());
    assert!(Filter::All(Vec::new()).routed().is_none());
    assert!(Filter::Finished.routed().is_none());
}

/// **A value is read once, for every question that orders.** A comparison reads
/// the infinities and an ordering of things does not place them; negative zero is
/// zero; a value that is no value of the type reads as nothing.
#[test]
fn a_value_reads_as_one_place_in_the_order() {
    let read = |holds, text| Orderable::read(holds, text);
    assert_eq!(read(ValueType::Number, " 3 "), Some(Orderable::Number(3.0)));
    assert_eq!(read(ValueType::Number, "-0"), read(ValueType::Number, "0"));
    assert_eq!(read(ValueType::Number, "NaN"), None);
    assert_eq!(read(ValueType::Number, "soon"), None);
    assert_eq!(
        read(ValueType::Date, "2026-02-01"),
        Some(Orderable::Date(today()))
    );
    assert_eq!(read(ValueType::Date, "2026-2-1"), None);
    assert_eq!(read(ValueType::Text, "2026-02-01"), None);

    assert!(
        read(ValueType::Number, "inf").is_some(),
        "a bound can be read"
    );
    assert_eq!(
        Orderable::read_to_order(ValueType::Number, "inf"),
        None,
        "a number with no end has no place in an ordering of things"
    );
    assert!(Orderable::read_to_order(ValueType::Number, "1e3").is_some());
}

/// **A term a declaration licenses is exact only where every kind it is asked of
/// is licensed.** A `contains` needs a list and an ordering a date or a number,
/// by the declaration of each thing's own kind; an equality and a `!=` need
/// nothing.
#[test]
fn a_term_is_exact_for_the_kinds_a_declaration_licenses() {
    let declared = vec![
        DeclaredType::new(
            "ticket",
            vec![
                Field::new("due", ValueType::Date),
                Field::listing("tags", ValueType::Text),
            ],
        ),
        // A kind that declares the key as something else.
        DeclaredType::new("person", vec![Field::new("due", ValueType::Number)]),
    ];
    let licensed = |text: &str, kinds: &[&str]| {
        routed(text)
            .expect("the tree routes")
            .licensed_for_every(&declared, kinds)
    };
    assert!(licensed("tags contains kitchen", &["thing"]));
    assert!(licensed("due <= 2026-02-01", &["thing", "place"]));
    assert!(
        !licensed("due <= 2026-02-01", &["thing", "person"]),
        "a person's own declaration says its due is a number"
    );
    assert!(
        licensed("tags contains kitchen", &["thing", "person"]),
        "a type under no kind's name licenses a key the kind does not declare"
    );
    assert!(
        licensed("status != done", &["person"]),
        "nothing licenses a !="
    );
    assert!(licensed("status = now", &["person"]));
    assert!(licensed("status != done && due > 2026-02-01", &["thing"]));
}
