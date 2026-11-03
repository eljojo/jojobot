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
            &super::super::bot_request::spec_of(held),
            &super::super::bot_request::spec_after(held, &holding(written), &cleared),
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
    // A blank spec value is a key taken off, written or held.
    assert_eq!(
        next(&holding(&[]), &[(DESIRED, "  ")], &[]),
        None,
        "a blank written on an empty request changes nothing"
    );
    assert_eq!(
        next(&at_one, &[(DESIRED, "  ")], &[]).as_deref(),
        Some("2"),
        "a blank written over a held value takes the key off"
    );
    let blank_held = holding(&[(DESIRED, "  "), (GENERATION, "1")]);
    assert_eq!(
        super::super::bot_request::spec_of(&blank_held),
        super::super::bot_request::spec_of(&holding(&[(GENERATION, "1")])),
        "a blank held value equals an absent one"
    );
    assert_eq!(
        super::super::bot_request::next_generation(
            &request,
            &super::super::bot_request::spec_of(&blank_held),
            &holding(&[(DESIRED, "running")]),
            1,
        )
        .as_deref(),
        Some("2"),
        "a value written over a blank held one is a new spec"
    );
    assert_eq!(
        super::super::bot_request::next_generation(
            &handle("topic:theta"),
            &holding(&[]),
            &holding(&[(DESIRED, "running")]),
            0,
        ),
        None
    );
}

/// **The generation follows the FOLDED spec before and after an act, whatever
/// the act.** A withdrawal that uncovers an older value changes the spec though it
/// sends no key; a withdrawal of a claim the fold did not read changes nothing.
/// The sequence of the finding: A sets desired and model, B sets desired, C sets
/// model; withdrawing B leaves desired from A and model from C.
#[test]
fn the_generation_follows_the_folded_spec_before_and_after_an_act() {
    let request = handle(REQUEST);
    let next = |before: &[(&str, &str)], after: &[(&str, &str)], highest: u64| {
        super::super::bot_request::next_generation(
            &request,
            &holding(before),
            &holding(after),
            highest,
        )
    };
    let with_b_and_c = [(DESIRED, "stopped"), (MODEL, "beta")];
    let with_b_withdrawn = [(DESIRED, "running"), (MODEL, "beta")];
    assert_eq!(
        next(&with_b_and_c, &with_b_withdrawn, 3).as_deref(),
        Some("4"),
        "withdrawing B changes desired under the number C holds"
    );
    // C already overrides every key B held: B withdrawn leaves the spec as it was.
    let covered = [(DESIRED, "stopped"), (MODEL, "beta")];
    assert_eq!(next(&covered, &covered, 3), None, "nothing uncovered");
    assert_eq!(
        next(&with_b_withdrawn, &with_b_and_c, 4).as_deref(),
        Some("5"),
        "a number is never given twice, the way back included"
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
            &super::super::bot_request::spec_of(&holding(held)),
            &super::super::bot_request::spec_after(&holding(held), &holding(written), &cleared),
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

/// **The spec the history holds, with one claim taken out**, is the fold without
/// that claim: each spec key reads its newest active write, a clear leaves the
/// key off, and a claim taken out uncovers the write under it. The sequence is
/// the one a withdrawal changes: A sets desired and model, B sets desired, C sets
/// model.
#[test]
fn the_spec_a_withdrawal_leaves_is_read_from_the_history() {
    use crate::memory::{FactAddress, FactId, FactStatus, FieldWrite, Provenance, Standing};
    let write = |fact: &str, value: Option<&str>, status: FactStatus| FieldWrite {
        written_at: None,
        value: value.map(String::from),
        fact: FactAddress::new(handle(REQUEST), FactId(fact.into())),
        recorded_at: Default::default(),
        status,
        provenance: Provenance::Inference,
        standing: Standing::Open,
        note: None,
        recorded_by: None,
        writer: None,
        run: None,
    };
    let history: BTreeMap<String, Vec<FieldWrite>> = [
        (
            DESIRED.to_string(),
            vec![
                write("f1", Some("running"), FactStatus::Active),
                write("f2", Some("stopped"), FactStatus::Active),
            ],
        ),
        (
            MODEL.to_string(),
            vec![
                write("f1", Some("alpha"), FactStatus::Active),
                write("f3", Some(" beta "), FactStatus::Active),
            ],
        ),
    ]
    .into();
    let spec = super::super::bot_request::spec_withdrawing;
    let held = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    assert_eq!(
        spec(&history, None),
        held(&[(DESIRED, "stopped"), (MODEL, "beta")]),
        "nothing taken out reads the newest active write of each key",
    );
    assert_eq!(
        spec(&history, Some(&FactId("f2".into()))),
        held(&[(DESIRED, "running"), (MODEL, "beta")]),
        "the older claim's value comes back under the one taken out",
    );
    assert_eq!(
        spec(&history, Some(&FactId("f3".into()))),
        held(&[(DESIRED, "stopped"), (MODEL, "alpha")]),
    );
    assert_eq!(
        spec(&history, Some(&FactId("f9".into()))),
        spec(&history, None),
        "a claim that wrote no spec key leaves the spec as it was",
    );
    let mut cleared = history.clone();
    cleared
        .get_mut(MODEL)
        .expect("the model key")
        .push(write("f4", None, FactStatus::Active));
    assert_eq!(
        spec(&cleared, None),
        held(&[(DESIRED, "stopped")]),
        "a clear leaves the key off the spec",
    );
    let mut archived = history;
    archived
        .get_mut(DESIRED)
        .expect("the desired key")
        .push(write("f5", Some("paused"), FactStatus::Archived));
    assert_eq!(
        spec(&archived, None),
        held(&[(DESIRED, "stopped"), (MODEL, "beta")]),
        "a write of an archived claim does not count",
    );
}

/// **The spec a merge leaves is read from the two histories placed by the moment
/// each write was made**, the rule the merge itself places writes by: the newer
/// write of a key wins, the survivor's wins a tie, a write with no moment is older
/// than any that has one, and a write of an archived claim does not count.
#[test]
fn the_spec_a_merge_leaves_is_read_from_both_histories() {
    use crate::memory::{FactAddress, FactId, FactStatus, FieldWrite, Provenance, Standing};
    let at = |second: i64| jiff::Timestamp::from_second(1_800_000_000 + second).expect("a moment");
    let write =
        |home: &str, fact: &str, value: Option<&str>, moment: Option<i64>, status: FactStatus| {
            FieldWrite {
                written_at: moment.map(at),
                value: value.map(String::from),
                fact: FactAddress::new(handle(home), FactId(fact.into())),
                recorded_at: Default::default(),
                status,
                provenance: Provenance::Inference,
                standing: Standing::Open,
                note: None,
                recorded_by: None,
                writer: None,
                run: None,
            }
        };
    let history = |pairs: Vec<(&str, Vec<FieldWrite>)>| -> BTreeMap<String, Vec<FieldWrite>> {
        pairs
            .into_iter()
            .map(|(key, writes)| (key.to_string(), writes))
            .collect()
    };
    let held = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let active = FactStatus::Active;
    let spec = super::super::bot_request::spec_merging;
    let survivor = history(vec![
        (
            DESIRED,
            vec![write(REQUEST, "f1", Some("running"), Some(10), active)],
        ),
        (
            MODEL,
            vec![write(REQUEST, "f1", Some("alpha"), Some(10), active)],
        ),
    ]);

    let newer = history(vec![
        (
            DESIRED,
            vec![write(
                "topic:scrapbook",
                "f1",
                Some("stopped"),
                Some(20),
                active,
            )],
        ),
        (
            MACHINE,
            vec![write(
                "topic:scrapbook",
                "f1",
                Some("machine:theta"),
                Some(5),
                active,
            )],
        ),
    ]);
    assert_eq!(
        spec(&survivor, &newer),
        held(&[
            (DESIRED, "stopped"),
            (MODEL, "alpha"),
            (MACHINE, "machine:theta"),
        ]),
        "a newer write of the folded thing wins, and a key only it wrote arrives",
    );

    let older = history(vec![
        (
            DESIRED,
            vec![write(
                "topic:scrapbook",
                "f1",
                Some("stopped"),
                Some(5),
                active,
            )],
        ),
        (
            MODEL,
            vec![write(
                "topic:scrapbook",
                "f2",
                Some("beta"),
                Some(10),
                active,
            )],
        ),
    ]);
    assert_eq!(
        spec(&survivor, &older),
        held(&[(DESIRED, "running"), (MODEL, "alpha")]),
        "an older write loses, and a write at the same moment loses to the survivor's",
    );

    let unstamped = history(vec![(
        DESIRED,
        vec![write(
            "topic:scrapbook",
            "f1",
            Some("stopped"),
            None,
            active,
        )],
    )]);
    assert_eq!(
        spec(&survivor, &unstamped),
        held(&[(DESIRED, "running"), (MODEL, "alpha")]),
        "a write with no moment is older than a stamped one",
    );

    let archived = history(vec![(
        DESIRED,
        vec![write(
            "topic:scrapbook",
            "f1",
            Some("stopped"),
            Some(20),
            FactStatus::Archived,
        )],
    )]);
    assert_eq!(
        spec(&survivor, &archived),
        held(&[(DESIRED, "running"), (MODEL, "alpha")]),
        "a write of an archived claim does not count",
    );

    let cleared = history(vec![(
        MODEL,
        vec![write("topic:scrapbook", "f1", None, Some(20), active)],
    )]);
    assert_eq!(
        spec(&survivor, &cleared),
        held(&[(DESIRED, "running")]),
        "a newer clear leaves the key off the spec",
    );

    assert_eq!(
        spec(&survivor, &BTreeMap::new()),
        held(&[(DESIRED, "running"), (MODEL, "alpha")]),
        "a duplicate that wrote no spec key leaves the spec as it was",
    );
}

/// **A withdrawal makes a number when the folded generation would fall, even
/// under an unchanged spec.** The history is A's 1, D's 2 and a resend E that
/// carries none: taking D out leaves the spec as it was and the fold at 1.
#[test]
fn a_withdrawal_that_lowers_the_folded_generation_makes_the_next_number() {
    use crate::memory::bot_request::{
        generation_withdrawing, highest_generation_ever, next_generation_withdrawing,
    };
    use crate::memory::{FactAddress, FactId, FactStatus, FieldWrite, Provenance, Standing};
    let write = |fact: &str, value: Option<&str>, status: FactStatus| FieldWrite {
        written_at: None,
        value: value.map(String::from),
        fact: FactAddress::new(handle(REQUEST), FactId(fact.into())),
        recorded_at: Default::default(),
        status,
        provenance: Provenance::Inference,
        standing: Standing::Open,
        note: None,
        recorded_by: None,
        writer: None,
        run: None,
    };
    let numbers = vec![
        write("f1", Some("1"), FactStatus::Active),
        write("f2", Some(" 2 "), FactStatus::Active),
    ];
    let same: BTreeMap<String, String> = [(DESIRED.to_string(), "paused".to_string())].into();
    let other: BTreeMap<String, String> = [(DESIRED.to_string(), "running".to_string())].into();
    let d = FactId("f2".into());
    assert_eq!(generation_withdrawing(&numbers, None), 2);
    assert_eq!(generation_withdrawing(&numbers, Some(&d)), 1);
    assert_eq!(
        generation_withdrawing(&numbers, Some(&FactId("f9".into()))),
        2,
        "a claim that carries no number leaves the fold as it was",
    );
    assert_eq!(generation_withdrawing(&[], None), 0);
    let mut archived = numbers.clone();
    archived.push(write("f3", Some("7"), FactStatus::Archived));
    assert_eq!(
        generation_withdrawing(&archived, None),
        2,
        "an archived number does not count"
    );
    assert_eq!(highest_generation_ever(&archived), 7);

    let request = handle(REQUEST);
    let makes = |before: &BTreeMap<String, String>, after: &BTreeMap<String, String>, lowered| {
        next_generation_withdrawing(&request, before, after, 2, if lowered { 1 } else { 2 }, 2)
    };
    assert_eq!(
        makes(&same, &same, true).as_deref(),
        Some("3"),
        "an unchanged spec under a falling number makes the next one",
    );
    assert_eq!(
        makes(&same, &same, false),
        None,
        "an unchanged spec under an unchanged number makes none",
    );
    assert_eq!(
        makes(&same, &other, false).as_deref(),
        Some("3"),
        "a changed spec makes the next number as before",
    );
    assert_eq!(
        next_generation_withdrawing(&handle("topic:scrapbook"), &same, &same, 2, 1, 2),
        None,
        "a thing that is no request holds no generation",
    );
}
