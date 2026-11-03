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

/// **Each spec key is judged alone on a creation.** Five keys in one write are
/// refused when any one is, so a key a store forgot to judge hides behind the
/// others; sent alone, each key is licensed for the bot above the target and
/// for nobody else.
#[test]
fn each_spec_key_alone_is_judged_on_a_creation() {
    let lineage = created_for_homer();
    for key in [TARGET_BOT, DESIRED, MACHINE, MODEL, BOUND_APP] {
        assert_eq!(refused("bot:marge", &[key], &lineage), None, "{key}: marge");
        for caller in ["bot:bart", "bot:homer"] {
            assert!(
                refused(caller, &[key], &lineage).is_some(),
                "{key}: {caller}"
            );
        }
    }
}

/// **A creation that carries a status key or the generation is refused for
/// everybody**, the bot above the target included: the status is the bound
/// app's and the generation is jojobot's, and a new request has neither yet.
#[test]
fn a_creation_carrying_a_status_key_or_the_generation_is_refused() {
    let lineage = created_for_homer();
    for key in [
        "observed_state",
        "observed_generation",
        "runtime_ref",
        "last_result",
        "generation",
    ] {
        for caller in ["bot:marge", "bot:bart", "bot:homer"] {
            assert!(
                refused(caller, &[TARGET_BOT, key], &lineage).is_some(),
                "{caller} created a request carrying {key}"
            );
        }
    }
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
        let highest = held
            .get(GENERATION)
            .and_then(|generation| generation.parse().ok())
            .unwrap_or(0);
        super::super::bot_request::next_generation(
            &request,
            held,
            &holding(written),
            &cleared,
            highest,
        )
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
            0,
        ),
        None
    );
}

/// **The number follows the highest ever written, not the fold.** A fold that
/// reads 1 after a retract took the claim carrying 2 out of it still yields 3,
/// and the folded value is not read at all: a fold that holds a number far above
/// the highest ever changes nothing. A resend raises nothing whatever the
/// highest is, and a clear of a held spec key raises to one past it.
#[test]
fn the_generation_is_one_past_the_highest_ever_written() {
    let request = handle(REQUEST);
    let next = |held: &[(&str, &str)], written: &[(&str, &str)], cleared: &[&str], highest: u64| {
        let cleared: Vec<String> = cleared.iter().map(|key| key.to_string()).collect();
        super::super::bot_request::next_generation(
            &request,
            &holding(held),
            &holding(written),
            &cleared,
            highest,
        )
    };
    let after_a_retract = [(DESIRED, "running"), (GENERATION, "1")];
    assert_eq!(
        next(&after_a_retract, &[(DESIRED, "stopped")], &[], 2).as_deref(),
        Some("3"),
        "the highest ever, 2, is above the folded 1"
    );
    let fold_ahead = [(DESIRED, "running"), (GENERATION, "9")];
    assert_eq!(
        next(&fold_ahead, &[(DESIRED, "stopped")], &[], 2).as_deref(),
        Some("3"),
        "a folded value above the highest ever is not read"
    );
    assert_eq!(
        next(&after_a_retract, &[(DESIRED, "running")], &[], 5),
        None,
        "a resend raises nothing"
    );
    assert_eq!(
        next(&after_a_retract, &[], &[DESIRED], 4).as_deref(),
        Some("5"),
        "a clear of a held spec key raises"
    );
}

/// **The highest ever counts every write of the key**, an archived claim's
/// included, and only numbers: a clear, a blank and a value that is no number
/// count as 0.
#[test]
fn the_highest_generation_ever_counts_every_write_that_was_a_number() {
    use crate::memory::{FactAddress, FactId, FactStatus, FieldWrite, Provenance, Standing};
    let write = |value: Option<&str>, status: FactStatus| FieldWrite {
        written_at: None,
        value: value.map(String::from),
        fact: FactAddress::new(handle(REQUEST), FactId("f1".into())),
        recorded_at: Default::default(),
        status,
        provenance: Provenance::Inference,
        standing: Standing::Open,
        note: None,
        recorded_by: None,
        writer: None,
        run: None,
    };
    let highest = super::super::bot_request::highest_generation_ever;
    assert_eq!(highest(&[]), 0);
    assert_eq!(
        highest(&[
            write(Some("1"), FactStatus::Active),
            write(Some("2"), FactStatus::Archived),
            write(Some(" 3 "), FactStatus::Active),
            write(None, FactStatus::Active),
            write(Some(" "), FactStatus::Active),
            write(Some("x"), FactStatus::Active),
        ]),
        3
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

/// **The refusal says which side judged the key**, for the renderer to word the
/// route from: a request's own keys are judged on the request as the duplicate,
/// and on the request as the survivor when an ordinary thing carries them.
#[test]
fn a_refused_fold_says_whether_the_request_was_the_side_that_judged() {
    let chart = Lineage {
        above: vec![handle("bot:marge")],
        holds: [TARGET_BOT.to_string()].into(),
        ..Lineage::default()
    };
    let judged_on_duplicate = |survivor: &str, duplicate: &str| match refuses_merge_carrying(
        &handle("bot:lisa"),
        &handle(survivor),
        &handle(duplicate),
        &writing(&[DESIRED]),
        Some(&chart),
        Some(&chart),
    ) {
        Some(MemoryError::MergeCarriesGuardedKeys {
            judged_on_duplicate,
            ..
        }) => judged_on_duplicate,
        other => panic!("not the merge refusal: {other:?}"),
    };
    assert!(judged_on_duplicate("topic:scrapbook", REQUEST));
    assert!(!judged_on_duplicate(REQUEST, "topic:scrapbook"));
}
