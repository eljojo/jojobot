//! The composer's proof: one table per rule, the account's three forms, and the
//! subset property with the account counted.

use super::compose::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// A layer whose served cost is exactly `cost` characters: the text is padded
/// so that `{"name":"xxx"}` measures out.
fn sized(name: &'static str, kind: Kind, tier: Tier, cost: usize) -> Layer {
    let overhead = Layer::new(name, kind, tier, "recall", json!("")).cost;
    assert!(cost >= overhead, "{name} cannot cost less than {overhead}");
    let layer = Layer::new(
        name,
        kind,
        tier,
        "recall",
        json!("x".repeat(cost - overhead)),
    );
    assert_eq!(layer.cost, cost);
    layer
}

fn core() -> Value {
    json!({"id": "thing:kettle"})
}

fn core_len() -> usize {
    crate::answer::served_len(&core())
}

fn names(composed: &Composed) -> Vec<String> {
    let Value::Object(served) = &composed.served else {
        panic!("an object: {}", composed.served)
    };
    served
        .keys()
        .filter(|key| !["id", LEFT_OUT_KEY, OVER_BUDGET_KEY].contains(&key.as_str()))
        .cloned()
        .collect()
}

fn left(composed: &Composed) -> Vec<&'static str> {
    composed
        .left_out
        .iter()
        .map(|omitted| omitted.layer)
        .collect()
}

fn omission(layer: &'static str) -> LeftOut {
    LeftOut {
        layer,
        call: "recall",
    }
}

/// Characters the account of `omitted`, written in `form`, adds to the core.
fn account_cost(form: Form, omitted: &[LeftOut]) -> usize {
    let Value::Object(mut with) = core() else {
        unreachable!()
    };
    with.insert(LEFT_OUT_KEY.to_string(), account(omitted, form));
    crate::answer::served_len(&Value::Object(with)) - core_len()
}

/// The richest account of these layers, as a room to leave beside the layers.
fn full(names: &[&'static str]) -> usize {
    let omitted: Vec<LeftOut> = names.iter().map(|name| omission(name)).collect();
    account_cost(Form::Full, &omitted)
}

/// The layer names an account lists, whatever its form.
fn account_names(account: &Value) -> Vec<String> {
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .expect("an array of names")
            .iter()
            .map(|name| name.as_str().expect("a name").to_string())
            .collect()
    };
    match account {
        Value::Object(floor) => strings(&floor["layers"]),
        Value::Array(rows) => rows
            .iter()
            .flat_map(|row| match row.get("layers") {
                Some(layers) => strings(layers),
                None => vec![row["layer"].as_str().expect("a name").to_string()],
            })
            .collect(),
        other => panic!("not an account: {other}"),
    }
}

fn run(layers: Vec<Layer>, extra_room: usize, taught: &[&str]) -> Composed {
    let taught: BTreeSet<&str> = taught.iter().copied().collect();
    compose_within(
        &Answer::new(core(), layers),
        core_len() + extra_room,
        &taught,
    )
}

/// Rule 1. Row names are the arms of `compose_within`: `core_over_budget`.
#[test]
fn rule_1_core_is_served_whole() {
    // (room beyond the core, layer cost, served?)
    for (room, cost, served) in [(0usize, 30usize, false), (29, 30, false), (30, 30, true)] {
        let composed = run(vec![sized("a", Kind::Detail, Tier::High, cost)], room, &[]);
        assert_eq!(composed.served["id"], "thing:kettle");
        assert_eq!(
            names(&composed).len() == 1,
            served,
            "room {room} cost {cost}"
        );
    }
    // A core over the whole limit is served anyway.
    let composed = compose_within(
        &Answer::new(core(), vec![sized("a", Kind::Detail, Tier::High, 30)]),
        0,
        &BTreeSet::new(),
    );
    assert_eq!(composed.served["id"], "thing:kettle");
    assert_eq!(left(&composed), ["a"]);
}

/// Rule 2. `rank_order`: tier first, then cost, then declared order.
#[test]
fn rule_2_layers_fill_by_tier_then_cost_then_declared_order() {
    // (layers as declared, room beside the account of the rest, served in rank order)
    type Row = (Vec<(&'static str, Tier, usize)>, usize, Vec<&'static str>);
    let rows: Vec<Row> = vec![
        // tier beats cost
        (
            vec![
                ("cheap_low", Tier::Low, 100),
                ("dear_high", Tier::High, 200),
            ],
            200,
            vec!["dear_high"],
        ),
        // inside a tier the cheaper goes first
        (
            vec![("dear", Tier::Medium, 200), ("cheap", Tier::Medium, 100)],
            100,
            vec!["cheap"],
        ),
        // equal cost keeps declared order
        (
            vec![("first", Tier::Medium, 100), ("second", Tier::Medium, 100)],
            100,
            vec!["first"],
        ),
        (
            vec![("second", Tier::Medium, 100), ("first", Tier::Medium, 100)],
            100,
            vec!["second"],
        ),
    ];
    for (declared, room, expected) in rows {
        let layers = declared
            .iter()
            .map(|(name, tier, cost)| sized(name, Kind::Detail, *tier, *cost))
            .collect();
        let omitted: Vec<&'static str> = declared
            .iter()
            .map(|(name, ..)| *name)
            .filter(|name| !expected.contains(name))
            .collect();
        let composed = run(layers, room + full(&omitted), &[]);
        let mut served = names(&composed);
        served.sort();
        let mut expected: Vec<String> = expected.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(served, expected, "{declared:?} room {room}");
    }
}

/// Rule 3. `one_suggestion`: the best-ranked suggestion, or none.
#[test]
fn rule_3_at_most_one_suggestion_the_best_ranked_or_none() {
    // (suggestions as declared (name, tier, cost), room beside the account of
    // the rest, served suggestions, named omissions)
    type Row = (
        Vec<(&'static str, Tier, usize)>,
        usize,
        Vec<&'static str>,
        Vec<&'static str>,
    );
    let rows: Vec<Row> = vec![
        // two fit: only the higher tier ships, and the other is named
        (
            vec![("low", Tier::Low, 30), ("high", Tier::High, 30)],
            100,
            vec!["high"],
            vec!["low"],
        ),
        // the best does not fit: none, though the other would; both are named
        (
            vec![("high", Tier::High, 200), ("low", Tier::Low, 30)],
            40,
            vec![],
            vec!["high", "low"],
        ),
        // one suggestion alone fits
        (vec![("only", Tier::Low, 30)], 30, vec!["only"], vec![]),
    ];
    for (declared, room, expected, omitted) in rows {
        let layers = declared
            .iter()
            .map(|(name, tier, cost)| sized(name, Kind::Suggestion, *tier, *cost))
            .collect();
        let composed = run(layers, room + full(&omitted), &[]);
        assert_eq!(left(&composed), omitted, "{declared:?} room {room}");
        assert_eq!(names(&composed), expected, "{declared:?} room {room}");
    }
    // **A suggestion below the best is named, never silently dropped**: it
    // takes no room to serve and the call that returns it rides in the account.
    let composed = run(
        vec![
            sized("best", Kind::Suggestion, Tier::High, 30),
            sized("second", Kind::Suggestion, Tier::Medium, 30),
            sized("detail", Kind::Detail, Tier::Low, 30),
        ],
        60 + full(&["second"]),
        &[],
    );
    assert_eq!(left(&composed), ["second"], "{:?}", composed.left_out);
    assert_eq!(
        composed.served[LEFT_OUT_KEY],
        json!([{"layer": "second", "call": "recall"}])
    );
    assert_eq!(names(&composed), ["best", "detail"]);
}

#[test]
fn a_high_tier_suggestion_is_dropped_before_a_low_tier_note_or_detail() {
    for kind in [Kind::Note, Kind::Detail] {
        let composed = run(
            vec![
                sized("suggestion", Kind::Suggestion, Tier::High, 200),
                sized("needed", kind, Tier::Low, 200),
            ],
            200 + full(&["suggestion"]),
            &[],
        );
        assert_eq!(names(&composed), ["needed"], "{kind:?}");
        assert_eq!(left(&composed), ["suggestion"], "{kind:?}");
        assert_eq!(composed.over_budget, None);
    }
}

#[test]
fn a_taught_suggestion_is_silent_and_does_not_hide_a_fresh_suggestion() {
    let layers = || {
        vec![
            sized("taught", Kind::Suggestion, Tier::High, 200).taught_once("claims"),
            sized("fresh", Kind::Suggestion, Tier::Low, 30),
        ]
    };
    let composed = run(layers(), 30, &["claims"]);
    assert_eq!(names(&composed), ["fresh"]);
    assert!(left(&composed).is_empty());
    assert!(composed.served.get(LEFT_OUT_KEY).is_none());

    let not_taught = run(layers(), 200 + full(&["fresh"]), &["other"]);
    assert_eq!(names(&not_taught), ["taught"]);
    assert_eq!(left(&not_taught), ["fresh"]);
}

/// Rule 4. `taught_domain`: a taught once-per-session note vanishes, unlisted.
#[test]
fn rule_4_a_taught_domain_is_dropped_not_listed() {
    // (taught domains, served, left out)
    type Row = (Vec<&'static str>, Vec<&'static str>, Vec<&'static str>);
    let rows: Vec<Row> = vec![
        (vec![], vec!["claims_note"], vec![]),
        (vec!["claims"], vec![], vec![]),
        (vec!["other"], vec!["claims_note"], vec![]),
    ];
    for (taught, served, omitted) in rows {
        let note = sized("claims_note", Kind::Note, Tier::Low, 30).taught_once("claims");
        let composed = run(vec![note], 100, &taught);
        assert_eq!(names(&composed), served, "{taught:?}");
        assert_eq!(left(&composed), omitted, "{taught:?}");
        assert!(composed.served.get(LEFT_OUT_KEY).is_none() || !omitted.is_empty());
    }
    // A taught note takes no room from the layers behind it.
    let composed = run(
        vec![
            sized("claims_note", Kind::Note, Tier::High, 60).taught_once("claims"),
            sized("detail", Kind::Detail, Tier::Low, 30),
        ],
        30,
        &["claims"],
    );
    assert_eq!(names(&composed), ["detail"]);
}

/// Rule 5. `left_out_named`: every other layer not served, once, with its call.
#[test]
fn rule_5_every_omission_is_named_once_with_its_call() {
    let mut dear = sized("dear", Kind::Detail, Tier::Low, 200);
    dear.returned_by = "recall with facts: true";
    let account = [LeftOut {
        layer: "dear",
        call: "recall with facts: true",
    }];
    let composed = run(
        vec![sized("small", Kind::Detail, Tier::High, 30), dear],
        30 + account_cost(Form::Full, &account),
        &[],
    );
    assert_eq!(names(&composed), ["small"]);
    assert_eq!(
        composed.served[LEFT_OUT_KEY],
        json!([{"layer": "dear", "call": "recall with facts: true"}])
    );
    assert_eq!(left(&composed), ["dear"]);
    // Nothing left out, no key.
    let composed = run(vec![sized("small", Kind::Detail, Tier::High, 30)], 30, &[]);
    assert!(composed.served.get(LEFT_OUT_KEY).is_none());
}

/// Rule 6. `prefix_fill`: a layer that does not fit closes the fill.
#[test]
fn rule_6_a_tighter_budget_only_drops_layers() {
    // Room for the cheap layer and the account of both, with the dear one in a
    // better tier: the cheap one is the layer behind the gap, and does not ship.
    let composed = run(
        vec![
            sized("dear_high", Kind::Detail, Tier::High, 200),
            sized("cheap_low", Kind::Detail, Tier::Low, 30),
        ],
        30 + full(&["dear_high", "cheap_low"]),
        &[],
    );
    assert!(names(&composed).is_empty());
    assert_eq!(left(&composed), ["dear_high", "cheap_low"]);
    assert_eq!(composed.over_budget, None);
}

#[test]
fn the_two_budgets_are_two_levels_from_the_setting() {
    assert_eq!(Budget::of_setting(false), Budget::Lean);
    assert_eq!(Budget::of_setting(true), Budget::Strict);
    assert!(Budget::Strict.limit() < Budget::Lean.limit());
}

#[test]
fn the_entry_uses_the_setting_budget_at_its_boundary() {
    for (saving, charged, kept) in [
        (false, 2400, true),
        (false, 2401, false),
        (true, 160, true),
        (true, 161, false),
    ] {
        let answer = Answer::new(
            core(),
            vec![sized("a", Kind::Detail, Tier::High, charged - core_len())],
        );
        let composed = compose(&answer, Budget::of_setting(saving), &BTreeSet::new());
        assert_eq!(composed.served["id"], "thing:kettle");
        assert_eq!(
            names(&composed),
            if kept { vec!["a"] } else { vec![] },
            "saving {saving}, charged {charged}"
        );
        assert_eq!(
            left(&composed),
            if kept { vec![] } else { vec!["a"] },
            "saving {saving}, charged {charged}"
        );
    }
}

/// The ruler: the core is identical under every budget, and the layers a
/// smaller limit serves are a subset of those a larger one serves, which are a
/// subset of all. Answers come from a fixed-seed generator: the same ones on
/// every run.
#[test]
fn the_core_is_identical_and_served_layers_nest_across_every_limit() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move |bound: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % bound
    };
    const NAMES: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
    for _ in 0..300 {
        let count = next(6) as usize + 1;
        let layers: Vec<Layer> = NAMES[..count]
            .iter()
            .map(|name| {
                let kind = [Kind::Note, Kind::Suggestion, Kind::Detail][next(3) as usize];
                let tier = [Tier::High, Tier::Medium, Tier::Low][next(3) as usize];
                let layer = sized(name, kind, tier, 20 + next(60) as usize);
                if kind != Kind::Detail && next(3) == 0 {
                    layer.taught_once("claims")
                } else {
                    layer
                }
            })
            .collect();
        let answer = Answer::new(core(), layers);
        let taught: BTreeSet<&str> = if next(2) == 0 {
            ["claims"].into()
        } else {
            [].into()
        };
        let all = compose_within(&answer, usize::MAX, &taught);
        let mut previous: Option<BTreeSet<String>> = None;
        for limit in (0..400).step_by(7) {
            let composed = compose_within(&answer, limit, &taught);
            assert_eq!(composed.served["id"], all.served["id"]);
            let served: BTreeSet<String> = names(&composed).into_iter().collect();
            assert!(served.is_subset(&names(&all).into_iter().collect()));
            if let Some(smaller) = &previous {
                assert!(
                    smaller.is_subset(&served),
                    "limit {limit}: {smaller:?} {served:?}"
                );
            }
            // Served and left out together are every layer the session still needs.
            assert_eq!(
                served.len() + composed.left_out.len(),
                names(&all).len() + all.left_out.len()
            );
            // The account is inside the budget: the final text fits, or the
            // answer says it is over and by what floor (the ruler).
            let len = crate::answer::served_len(&composed.served);
            match composed.over_budget {
                None => assert!(len <= limit, "limit {limit}: served {len}"),
                Some(floor) => {
                    assert!(floor > limit, "limit {limit}: floor {floor}");
                    assert_eq!(composed.served[OVER_BUDGET_KEY], json!(floor));
                    assert!(served.is_empty(), "an over-budget answer serves no layer");
                }
            }
            // No omission is ever dropped: the account lists every one.
            let listed = composed.served.get(LEFT_OUT_KEY).map(account_names);
            let mut owed: Vec<String> = left(&composed).iter().map(|n| n.to_string()).collect();
            let mut listed = listed.unwrap_or_default();
            owed.sort();
            listed.sort();
            assert_eq!(listed, owed, "limit {limit}");
            previous = Some(served);
        }
    }
}

/// Six layers whose bodies are far over the room, all returned by one call.
fn six_dear() -> Vec<Layer> {
    ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| sized(name, Kind::Detail, Tier::Medium, 160))
        .collect()
}

/// The read of g1 found: a 21-character core and six omissions served 214
/// characters under Strict 160, because the account was added after the fill.
#[test]
fn six_omissions_under_strict_fit_by_the_grouped_form() {
    assert_eq!(core_len(), 21);
    let answer = Answer::new(core(), six_dear());
    let composed = compose_within(&answer, Budget::Strict.limit(), &BTreeSet::new());
    assert_eq!(composed.over_budget, None);
    assert!(crate::answer::served_len(&composed.served) <= Budget::Strict.limit());
    assert_eq!(composed.form, Form::Grouped);
    assert_eq!(left(&composed), ["a", "b", "c", "d", "e", "f"]);
    assert_eq!(
        account_names(&composed.served[LEFT_OUT_KEY]),
        ["a", "b", "c", "d", "e", "f"]
    );
}

/// The three forms, richest first: the richest that fits is the one served,
/// and the floor's size is the signal when none fits.
#[test]
fn the_account_takes_the_richest_form_that_fits() {
    let omitted = [
        LeftOut {
            layer: "a",
            call: "recall with facts: true",
        },
        LeftOut {
            layer: "b",
            call: "recall with facts: true",
        },
        LeftOut {
            layer: "c",
            call: "search with include_mail: true",
        },
    ];
    let layers = || -> Vec<Layer> {
        omitted
            .iter()
            .map(|omitted| {
                let mut layer = sized(omitted.layer, Kind::Detail, Tier::Medium, 200);
                layer.returned_by = omitted.call;
                layer
            })
            .collect()
    };
    let cost = |form| account_cost(form, &omitted);
    assert!(cost(Form::Full) > cost(Form::Grouped) && cost(Form::Grouped) > cost(Form::Floor));
    // (room beside the core, form served, over budget by this floor)
    let rows = [
        (cost(Form::Full), Form::Full, None),
        (cost(Form::Full) + 1, Form::Full, None),
        (cost(Form::Full) - 1, Form::Grouped, None),
        (cost(Form::Grouped), Form::Grouped, None),
        (cost(Form::Grouped) - 1, Form::Floor, None),
        (cost(Form::Floor), Form::Floor, None),
        (
            cost(Form::Floor) - 1,
            Form::Floor,
            Some(core_len() + cost(Form::Floor)),
        ),
    ];
    for (room, form, over) in rows {
        let composed = run(layers(), room, &[]);
        assert_eq!(composed.form, form, "room {room}");
        assert_eq!(composed.over_budget, over, "room {room}");
        assert_eq!(
            composed.served[LEFT_OUT_KEY],
            account(&composed.left_out, form),
            "room {room}"
        );
        assert_eq!(left(&composed), ["a", "b", "c"], "room {room}");
        assert!(names(&composed).is_empty(), "room {room}");
    }
}

/// **The floor names the call that returns what it left out.** Every form says
/// which call returns an omitted layer (the briefs: whatever is left out is named,
/// with the call that returns it), and the floor does it once per distinct call,
/// in the order the layers were left out, so it stays the smallest form. Paired
/// with the full form, which names the call beside each layer.
#[test]
fn the_floor_names_each_call_that_returns_what_it_left_out() {
    let omitted = [
        LeftOut {
            layer: "a",
            call: "recall with facts: true",
        },
        LeftOut {
            layer: "b",
            call: "search with include_mail: true",
        },
        LeftOut {
            layer: "c",
            call: "recall with facts: true",
        },
    ];
    let layers = || -> Vec<Layer> {
        omitted
            .iter()
            .map(|omitted| {
                let mut layer = sized(omitted.layer, Kind::Detail, Tier::Medium, 200);
                layer.returned_by = omitted.call;
                layer
            })
            .collect()
    };
    let room = account_cost(Form::Floor, &omitted);
    let floor = run(layers(), room, &[]);
    assert_eq!(floor.form, Form::Floor);
    assert_eq!(floor.over_budget, None);
    let calls = floor.served[LEFT_OUT_KEY]["calls"]
        .as_str()
        .expect("the floor says its calls");
    for call in ["recall with facts: true", "search with include_mail: true"] {
        assert_eq!(calls.matches(call).count(), 1, "{call}: {calls}");
    }
    assert_eq!(account_names(&floor.served[LEFT_OUT_KEY]), ["a", "b", "c"]);
    let full = run(layers(), account_cost(Form::Full, &omitted), &[]);
    assert_eq!(full.form, Form::Full);
    assert_eq!(full.served[LEFT_OUT_KEY][1]["call"], omitted[1].call);
}

/// Layers come before the richness of the account: a layer is served in the
/// grouped form rather than left out for the sake of the full one.
#[test]
fn a_layer_is_served_before_the_account_is_enriched() {
    let layers = || {
        vec![
            sized("kept", Kind::Detail, Tier::High, 60),
            sized("left_a", Kind::Detail, Tier::Low, 400),
            sized("left_b", Kind::Detail, Tier::Low, 400),
        ]
    };
    let omitted = [omission("left_a"), omission("left_b")];
    let room = 60 + account_cost(Form::Full, &omitted) - 1;
    let composed = run(layers(), room, &[]);
    assert_eq!(names(&composed), ["kept"]);
    assert_eq!(composed.form, Form::Grouped);
    assert_eq!(composed.over_budget, None);
}

/// Even the floor does not fit: it is served whole and its size is said.
#[test]
fn a_floor_over_the_limit_is_served_and_its_size_said() {
    let answer = Answer::new(core(), six_dear());
    let all = compose_within(&answer, 0, &BTreeSet::new()).left_out;
    let floor = core_len() + account_cost(Form::Floor, &all);
    let composed = compose_within(&answer, core_len() + 10, &BTreeSet::new());
    assert_eq!(composed.over_budget, Some(floor));
    assert_eq!(composed.served[OVER_BUDGET_KEY], json!(floor));
    assert_eq!(composed.form, Form::Floor);
    assert!(names(&composed).is_empty());
    assert_eq!(
        account_names(&composed.served[LEFT_OUT_KEY]),
        ["a", "b", "c", "d", "e", "f"]
    );
    // A core with no layer at all, over the limit, says so as well.
    let bare = compose_within(&Answer::new(core(), vec![]), 0, &BTreeSet::new());
    assert_eq!(bare.over_budget, Some(core_len()));
    assert_eq!(bare.served["id"], "thing:kettle");
}

/// Nothing outside this process declares these spellings, and a stored or
/// served record cannot fail a test: one case pins them as literals.
#[test]
fn the_served_spellings_of_the_account_are_pinned() {
    let composed = run(six_dear(), 0, &[]);
    let served = composed.served.as_object().expect("an object");
    assert!(served.contains_key("left_out"));
    assert!(served.contains_key("over_budget"));
}

/// A note whose text is `text`, served inside the `teaching` array.
fn teaching_note(name: &'static str, tier: Tier, text: &str) -> Layer {
    Layer::new(name, Kind::Note, tier, "recall", json!(text)).in_slot("teaching")
}

/// Layers that share a slot ride one array under its key, in ranked order, and
/// the account names each by its own name (rule 4 of the receipt owner's
/// architect ruling: one layer per text).
#[test]
fn notes_sharing_a_slot_ride_one_array_in_ranked_order() {
    let layers = || {
        vec![
            teaching_note("note_low", Tier::Low, &"low note ".repeat(20)),
            teaching_note("note_high", Tier::High, "high note"),
        ]
    };
    let both = run(layers(), 1000, &[]);
    assert_eq!(names(&both), ["teaching"], "{}", both.served);
    assert_eq!(
        both.served["teaching"],
        json!(["high note", "low note ".repeat(20)]),
        "{}",
        both.served
    );
    assert!(left(&both).is_empty(), "{}", both.served);

    // Room for one: the higher tier ships, the other is named under its own name.
    let one_cost = layers()[1].cost;
    let one = run(layers(), one_cost + full(&["note_low"]), &[]);
    assert_eq!(
        one.served["teaching"],
        json!(["high note"]),
        "{}",
        one.served
    );
    assert_eq!(left(&one), ["note_low"], "{}", one.served);
}

/// The subset property holds across a shared slot: a smaller limit serves a
/// subset of what a larger one serves.
#[test]
fn a_smaller_limit_serves_a_subset_of_a_shared_slot() {
    let layers = || {
        vec![
            teaching_note("a", Tier::High, "first note"),
            teaching_note("b", Tier::Medium, "second note, a little longer"),
            teaching_note("c", Tier::Low, "third note"),
        ]
    };
    let served_at = |room: usize| -> Vec<String> {
        let composed = run(layers(), room, &[]);
        composed
            .served
            .get("teaching")
            .and_then(Value::as_array)
            .map(|notes| {
                notes
                    .iter()
                    .map(|note| note.as_str().expect("a note").to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    // Walking the room down from a roomy limit, what is served only shrinks.
    let mut larger = served_at(1000);
    assert_eq!(larger.len(), 3, "{larger:?}");
    for room in (0..400).rev() {
        let smaller = served_at(room);
        assert!(
            smaller.iter().all(|note| larger.contains(note)),
            "room {room} served {smaller:?}, not a subset of {larger:?}"
        );
        larger = smaller;
    }
}
