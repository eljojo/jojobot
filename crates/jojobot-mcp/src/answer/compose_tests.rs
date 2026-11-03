//! The composer's proof: one table per rule, and the subset property.

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
        .filter(|key| *key != "id" && *key != LEFT_OUT_KEY)
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
    // (layers as declared, room, served in rank order)
    type Row = (Vec<(&'static str, Tier, usize)>, usize, Vec<&'static str>);
    let rows: Vec<Row> = vec![
        // tier beats cost
        (
            vec![("cheap_low", Tier::Low, 30), ("dear_high", Tier::High, 60)],
            60,
            vec!["dear_high"],
        ),
        // inside a tier the cheaper goes first
        (
            vec![("dear", Tier::Medium, 60), ("cheap", Tier::Medium, 30)],
            30,
            vec!["cheap"],
        ),
        // equal cost keeps declared order
        (
            vec![("first", Tier::Medium, 30), ("second", Tier::Medium, 30)],
            30,
            vec!["first"],
        ),
        (
            vec![("second", Tier::Medium, 30), ("first", Tier::Medium, 30)],
            30,
            vec!["second"],
        ),
    ];
    for (declared, room, expected) in rows {
        let layers = declared
            .iter()
            .map(|(name, tier, cost)| sized(name, Kind::Detail, *tier, *cost))
            .collect();
        let composed = run(layers, room, &[]);
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
    // (suggestions as declared (name, tier, cost), room, served suggestions)
    type Row = (Vec<(&'static str, Tier, usize)>, usize, Vec<&'static str>);
    let rows: Vec<Row> = vec![
        // two fit: only the higher tier ships
        (
            vec![("low", Tier::Low, 30), ("high", Tier::High, 30)],
            100,
            vec!["high"],
        ),
        // the best does not fit: none, though the other would
        (
            vec![("high", Tier::High, 60), ("low", Tier::Low, 30)],
            40,
            vec![],
        ),
        // one suggestion alone fits
        (vec![("only", Tier::Low, 30)], 30, vec!["only"]),
    ];
    for (declared, room, expected) in rows {
        let layers = declared
            .iter()
            .map(|(name, tier, cost)| sized(name, Kind::Suggestion, *tier, *cost))
            .collect();
        let composed = run(layers, room, &[]);
        assert_eq!(names(&composed), expected, "{declared:?} room {room}");
    }
    // A second suggestion is left out without closing the fill behind it.
    let composed = run(
        vec![
            sized("best", Kind::Suggestion, Tier::High, 30),
            sized("second", Kind::Suggestion, Tier::Medium, 30),
            sized("detail", Kind::Detail, Tier::Low, 30),
        ],
        100,
        &[],
    );
    assert_eq!(names(&composed), ["best", "detail"]);
    assert_eq!(left(&composed), ["second"]);
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
    let mut dear = sized("dear", Kind::Detail, Tier::Low, 60);
    dear.returned_by = "recall with facts: true";
    let composed = run(
        vec![sized("small", Kind::Detail, Tier::High, 30), dear],
        30,
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
    // Room 30 with a dear layer ahead of a cheap one of the same tier: the
    // cheap one ranks first. Put the dear one in a better tier to make the
    // cheap one the layer behind the gap.
    let composed = run(
        vec![
            sized("dear_high", Kind::Detail, Tier::High, 60),
            sized("cheap_low", Kind::Detail, Tier::Low, 30),
        ],
        30,
        &[],
    );
    assert!(names(&composed).is_empty());
    assert_eq!(left(&composed), ["dear_high", "cheap_low"]);
}

#[test]
fn the_two_budgets_are_two_levels_from_the_setting() {
    assert_eq!(Budget::of_setting(false), Budget::Lean);
    assert_eq!(Budget::of_setting(true), Budget::Strict);
    assert!(Budget::Strict.limit() < Budget::Lean.limit());
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
                if kind == Kind::Note && next(3) == 0 {
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
            previous = Some(served);
        }
    }
}
