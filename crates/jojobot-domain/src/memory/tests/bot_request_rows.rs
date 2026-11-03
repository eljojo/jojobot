//! **The rows of the one permission check that guard a bot request** (rules
//! 525, 579, 731). Each case asks `refuses_unlicensed_write` as the stores do,
//! with the chart a store would have read.

use std::collections::BTreeMap;

use crate::memory::bot_request::{
    BOUND_APP, DESIRED, GENERATION, MACHINE, MODEL, OBSERVED_STATE, TARGET_BOT,
};
use crate::memory::{
    EntityId, Lineage, MemoryError, refuses_merge_carrying, refuses_unlicensed_write,
};

fn handle(text: &str) -> EntityId {
    EntityId(text.into())
}

fn writing(keys: &[&str]) -> BTreeMap<String, String> {
    keys.iter()
        .map(|key| (key.to_string(), "x".to_string()))
        .collect()
}

/// A request for homer, whose chain is homer then marge.
const REQUEST: &str = "bot-request:theta";

fn created_for_homer() -> Lineage {
    Lineage {
        creating: true,
        named: Some(handle("bot:homer")),
        above_named: vec![handle("bot:marge")],
        ..Lineage::default()
    }
}

fn held_for_homer() -> Lineage {
    Lineage {
        above: vec![handle("bot:marge")],
        holds: [TARGET_BOT.to_string()].into(),
        ..Lineage::default()
    }
}

fn refused(caller: &str, keys: &[&str], lineage: &Lineage) -> Option<(String, Vec<String>)> {
    match refuses_unlicensed_write(
        &handle(REQUEST),
        &handle(caller),
        &writing(keys),
        Some(lineage),
    ) {
        Some(MemoryError::KeyNotYours { key, allowed, .. }) => Some((key, allowed)),
        Some(other) => panic!("a different refusal: {other:?}"),
        None => None,
    }
}

/// **A request is created by a bot above its target and by nobody else**, and
/// the refusal names who may.
#[test]
fn a_request_is_created_by_a_bot_above_its_target() {
    let spec = [TARGET_BOT, DESIRED, MACHINE, MODEL, BOUND_APP];
    let lineage = created_for_homer();
    assert_eq!(
        refused("bot:marge", &spec, &lineage),
        None,
        "marge is above homer"
    );
    let (_, allowed) = refused("bot:bart", &spec, &lineage).expect("bart is not above homer");
    assert_eq!(
        allowed,
        vec!["bot:marge".to_string()],
        "the refusal names who may"
    );
    assert!(
        refused("bot:homer", &spec, &lineage).is_some(),
        "the target does not ask for itself"
    );
}

/// **Every spec key is judged on an edit, and the target is fixed after
/// creation.**
#[test]
fn a_spec_edit_is_a_bot_above_the_target_and_the_target_never_moves() {
    let lineage = held_for_homer();
    for key in [DESIRED, MACHINE, MODEL, BOUND_APP] {
        assert_eq!(refused("bot:marge", &[key], &lineage), None, "{key}: marge");
        assert!(
            refused("bot:bart", &[key], &lineage).is_some(),
            "{key}: bart"
        );
        assert!(
            refused("bot:homer", &[key], &lineage).is_some(),
            "{key}: homer"
        );
    }
    for caller in ["bot:marge", "bot:bart", "bot:homer"] {
        assert!(
            refused(caller, &[TARGET_BOT], &lineage).is_some(),
            "{caller} cannot move the target"
        );
    }
}

/// **A bot writes no status key and no generation, whoever it is.** Marge
/// manages homer and is still refused; only the bound app writes status, and
/// the app path is judged by `permit`.
#[test]
fn no_bot_writes_a_status_key_or_the_generation() {
    let lineage = held_for_homer();
    // The spellings are the app's contract and nothing else declares them, so
    // they are written out and not read from the table under test.
    for key in [
        "observed_state",
        "observed_generation",
        "runtime_ref",
        "last_result",
        "generation",
    ] {
        for caller in ["bot:marge", "bot:bart", "bot:homer"] {
            assert!(
                refused(caller, &[key], &lineage).is_some(),
                "{caller} wrote {key}"
            );
        }
    }
}

/// **The same words are ordinary keys on anything that is not a request.** A
/// machine's `model` is nobody's business here.
#[test]
fn the_request_keys_guard_nothing_off_a_request() {
    for subject in ["machine:theta", "thing:theta", "bot:homer"] {
        assert!(
            refuses_unlicensed_write(
                &handle(subject),
                &handle("bot:bart"),
                &writing(&[MODEL, MACHINE, DESIRED, OBSERVED_STATE, GENERATION]),
                Some(&Lineage::default()),
            )
            .is_none(),
            "{subject}"
        );
    }
}

fn holding(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// **The generation follows the spec and nothing else.** The first spec makes
/// 1; a different value, or a held key taken off, raises it by one; a resend
/// (spaces aside), a key that is no spec key and a clear of a key not held
/// raise nothing; a subject that is no request has none.
#[test]
fn the_generation_rises_when_the_spec_changes_and_only_then() {
    let request = handle(REQUEST);
    let at_one = holding(&[(DESIRED, "running"), (GENERATION, "1")]);
    let next = |held: &BTreeMap<String, String>, written: &[(&str, &str)], cleared: &[&str]| {
        let cleared: Vec<String> = cleared.iter().map(|key| key.to_string()).collect();
        super::super::bot_request::next_generation(&request, held, &holding(written), &cleared)
    };
    assert_eq!(
        next(&holding(&[]), &[(DESIRED, "running")], &[]).as_deref(),
        Some("1")
    );
    assert_eq!(
        next(&at_one, &[(DESIRED, "stopped")], &[]).as_deref(),
        Some("2")
    );
    assert_eq!(next(&at_one, &[(DESIRED, " running ")], &[]), None);
    assert_eq!(next(&at_one, &[(OBSERVED_STATE, "running")], &[]), None);
    assert_eq!(next(&at_one, &[], &[DESIRED]).as_deref(), Some("2"));
    assert_eq!(
        next(&at_one, &[], &[MACHINE]),
        None,
        "nothing held to clear"
    );
    assert_eq!(
        super::super::bot_request::next_generation(
            &handle("topic:theta"),
            &holding(&[]),
            &holding(&[(DESIRED, "running")]),
            &[],
        ),
        None
    );
}

/// **A refused fold of a request names who the request's own chart allows**, not
/// the chart of the thing it would land on. The survivor sits under one manager
/// and the request's target under another; the refusal carries the second.
#[test]
fn a_refused_fold_of_a_request_names_the_chart_that_judged_it() {
    let request_chart = Lineage {
        above: vec![handle("bot:marge")],
        holds: [TARGET_BOT.to_string()].into(),
        ..Lineage::default()
    };
    let survivor_chart = Lineage {
        above: vec![handle("bot:bart")],
        holds: [TARGET_BOT.to_string()].into(),
        ..Lineage::default()
    };
    let refused = refuses_merge_carrying(
        &handle("bot:lisa"),
        &handle("topic:scrapbook"),
        &handle(REQUEST),
        &writing(&[DESIRED]),
        Some(&survivor_chart),
        Some(&request_chart),
    );
    match refused {
        Some(MemoryError::MergeCarriesGuardedKeys { allowed, .. }) => {
            assert_eq!(allowed, vec!["bot:marge".to_string()]);
        }
        other => panic!("not the merge refusal: {other:?}"),
    }
}
