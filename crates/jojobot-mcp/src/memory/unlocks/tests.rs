use crate::Jojobot;
use crate::harness::*;
use crate::memory::add_entity::AddEntityArgs;
use crate::memory::capture::CaptureArgs;
use crate::memory::declare_type::{DeclareTypeArgs, FieldArgs};
use crate::memory::testing::*;
use rmcp::handler::server::wrapper::Parameters;

fn sets(pairs: &[(&str, &str)]) -> Option<std::collections::BTreeMap<String, String>> {
    Some(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

/// A loop under a thing, holding nothing yet.
async fn a_loop(jojobot: &Jojobot, handle: &str) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", handle, handle)
        }))
        .await
        .expect("add ok");
}

async fn capture_sets(
    jojobot: &Jojobot,
    subject: &str,
    words: &str,
    held: &[(&str, &str)],
) -> serde_json::Value {
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: sets(held),
            ..capture_args(subject, words)
        },
    )
    .await
}

/// The steps of a receipt's `unlocks` block that add `key`.
fn steps_adding<'a>(receipt: &'a serde_json::Value, key: &str) -> Vec<&'a serde_json::Value> {
    receipt["unlocks"]["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter(|step| {
                    step["add"]
                        .as_array()
                        .is_some_and(|keys| keys.iter().any(|k| k == key))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The steps of a receipt's `unlocks` block that name a declared type.
fn steps_for_type<'a>(receipt: &'a serde_json::Value, name: &str) -> Vec<&'a serde_json::Value> {
    receipt["unlocks"]["steps"]
        .as_array()
        .map(|steps| steps.iter().filter(|step| step["type"] == name).collect())
        .unwrap_or_default()
}

fn adds(step: &serde_json::Value) -> Vec<&str> {
    step["add"]
        .as_array()
        .expect("a step names the keys to add")
        .iter()
        .filter_map(|key| key.as_str())
        .collect()
}

async fn declare_journey(jojobot: &Jojobot) {
    let field = |key: &str, holds: &str| FieldArgs {
        key: key.into(),
        holds: Some(holds.into()),
        folds: None,
        required: false,
        one_of: None,
    };
    jojobot
        .declare_type(Parameters(DeclareTypeArgs {
            request_id: None,
            override_token: None,
            override_reason: None,
            name: "journey".into(),
            fields: vec![
                field("departs_from", "text"),
                field("arrives_at", "text"),
                field("leaves_on", "date"),
                field("returns_on", "date"),
            ],
            sid: Some(TEST_SID.into()),
        }))
        .await
        .expect("declare_type ok");
}

/// **A loop with a cadence and no check-in says what a check-in would let it
/// answer**, and the same loop written whole says nothing.
///
/// The negative rides with its positive in one case: the block that goes away
/// on the complete write is the block the incomplete write returned.
#[tokio::test]
async fn a_loop_with_a_cadence_and_no_check_in_names_what_a_check_in_unlocks() {
    let jojobot = handler();
    a_loop(&jojobot, "rhythm:descale").await;

    let partial = capture_sets(
        &jojobot,
        "rhythm:descale",
        "every week",
        &[("cadence_days", "7"), ("advances_from", "check_in_date")],
    )
    .await;
    let steps = steps_adding(&partial, "last_check_in");
    assert_eq!(
        steps.len(),
        1,
        "a cadence with no check-in names the check-in once: {partial}"
    );
    let line = steps[0]["unlocks"]
        .as_str()
        .expect("a step carries a sentence");
    assert!(
        !line.trim().is_empty() && !line.contains("  ") && !line.contains('\n'),
        "the sentence is one line a reader can read: {line:?}"
    );
    assert_eq!(
        partial["unlocks"]["holds"]["keys"], 2,
        "the block says how many keys the thing holds: {partial}"
    );
    assert_eq!(
        partial["unlocks"]["holds"]["records"], 1,
        "the block says how many records stand on the thing: {partial}"
    );

    let whole = capture_sets(
        &jojobot,
        "rhythm:descale",
        "checked in and scheduled",
        &[
            ("name", "descale"),
            ("last_check_in", "2026-08-01"),
            ("counts_from", "2026-08-01"),
        ],
    )
    .await;
    assert!(
        whole.get("unlocks").is_none(),
        "a loop that holds everything it needs has nothing to unlock: {whole}"
    );
}

/// **A loop that holds a basis and no cadence names both shapes of one**, a
/// number of days or a calendar rule: they are one step, and the policy
/// defaults so it is not asked for.
#[tokio::test]
async fn a_loop_with_a_basis_and_no_cadence_names_both_shapes_of_one() {
    let jojobot = handler();
    a_loop(&jojobot, "rhythm:descale").await;

    let partial = capture_sets(
        &jojobot,
        "rhythm:descale",
        "tracked by the day",
        &[
            ("counts_from", "2026-08-01"),
            ("last_check_in", "2026-08-01"),
        ],
    )
    .await;
    let steps = steps_adding(&partial, "cadence_days");
    assert_eq!(steps.len(), 1, "the missing cadence is named: {partial}");
    assert_eq!(
        adds(steps[0]),
        vec!["cadence_days", "rrule"],
        "the two shapes of a cadence are one step: {partial}"
    );
    assert!(
        steps_adding(&partial, "last_check_in").is_empty(),
        "a key the loop holds is not asked for again: {partial}"
    );
}

/// **A thing holding part of a declared type names the type and the keys it
/// lacks, once per type and set of missing keys per session.**
///
/// The second write that leaves the same keys missing carries no type step; a
/// thing that lacks other keys in the same session gets its own.
#[tokio::test]
async fn a_partly_matched_type_is_named_once_per_set_of_missing_keys_per_session() {
    let jojobot = handler();
    declare_journey(&jojobot).await;

    let first = capture_sets(
        &jojobot,
        "thing:delta",
        "away for a week",
        &[("leaves_on", "2026-09-01"), ("returns_on", "2026-09-08")],
    )
    .await;
    let steps = steps_for_type(&first, "journey");
    assert_eq!(steps.len(), 1, "the type is named: {first}");
    assert_eq!(
        adds(steps[0]),
        vec!["departs_from", "arrives_at"],
        "the keys it lacks come back in the type's own order: {first}"
    );

    let second = capture_sets(
        &jojobot,
        "thing:delta",
        "still a week",
        &[("returns_on", "2026-09-09")],
    )
    .await;
    assert!(
        second.get("unlocks").is_none(),
        "the same missing keys are not named a second time: {second}"
    );

    let other = capture_sets(
        &jojobot,
        "thing:epsilon",
        "away for a day",
        &[("leaves_on", "2026-10-01")],
    )
    .await;
    assert_eq!(
        steps_for_type(&other, "journey").len(),
        1,
        "a thing lacking other keys is told its own: {other}"
    );
}

/// **A type held whole is silent**, and a thing that answers no type is too.
#[tokio::test]
async fn a_thing_that_holds_a_whole_type_or_matches_none_gets_no_block() {
    let jojobot = handler();
    declare_journey(&jojobot).await;

    let partial = capture_sets(
        &jojobot,
        "thing:delta",
        "away for a week",
        &[("leaves_on", "2026-09-01")],
    )
    .await;
    assert_eq!(
        steps_for_type(&partial, "journey").len(),
        1,
        "a partly held type speaks: {partial}"
    );

    let whole = capture_sets(
        &jojobot,
        "thing:epsilon",
        "a whole trip",
        &[
            ("departs_from", "somewhere"),
            ("arrives_at", "elsewhere"),
            ("leaves_on", "2026-09-01"),
            ("returns_on", "2026-09-08"),
        ],
    )
    .await;
    assert!(
        whole.get("unlocks").is_none(),
        "a thing holding every key of the type has nothing to unlock: {whole}"
    );

    let filter = capture_sets(
        &jojobot,
        "thing:gamma",
        "an air filter",
        &[("size", "20x25")],
    )
    .await;
    assert!(
        filter.get("unlocks").is_none(),
        "a thing with no schedule key and no type gets no line: {filter}"
    );
}

/// **`add_entity` with `sets` and `update_fact` with `sets` say the same.**
#[tokio::test]
async fn every_write_that_sets_keys_carries_the_block() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;

    let created = json_of(
        &jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                sets: sets(&[("cadence_days", "7"), ("advances_from", "check_in_date")]),
                ..add_args("rhythm", "descale", "descale")
            }))
            .await
            .expect("add ok"),
    );
    assert_eq!(
        steps_adding(&created, "last_check_in").len(),
        1,
        "a thing created with a cadence names the check-in: {created}"
    );

    let captured = capture_ok(&jojobot, capture_args("rhythm:descale", "every week")).await;
    let address = captured["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let edited = json_of(
        &jojobot
            .update_fact(Parameters(crate::memory::update_fact::UpdateFactArgs {
                sets: sets(&[("counts_from", "2026-08-01")]),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        steps_adding(&edited, "last_check_in").len(),
        1,
        "an edit that sets keys names what the thing still lacks: {edited}"
    );
    assert!(
        steps_adding(&edited, "counts_from").is_empty(),
        "the key the edit just set is not asked for: {edited}"
    );
}

/// **A shipped type whose keys jojobot works out itself is not offered**, and a
/// type the caller can complete still is, on the same thing in the same read.
#[tokio::test]
async fn a_type_of_computed_keys_is_not_offered_but_a_completable_one_is() {
    let jojobot = handler();
    crate::seed::ensure_shipped_types(&jojobot.memory)
        .await
        .expect("the shipped types declare");
    declare_journey(&jojobot).await;
    a_loop(&jojobot, "rhythm:descale").await;

    let receipt = capture_sets(
        &jojobot,
        "rhythm:descale",
        "tracked from a basis",
        &[("counts_from", "2026-08-01"), ("leaves_on", "2026-09-01")],
    )
    .await;
    assert_eq!(
        steps_for_type(&receipt, "journey").len(),
        1,
        "a type the caller can complete is named: {receipt}"
    );
    assert!(
        steps_for_type(&receipt, "computed-dates").is_empty(),
        "a type of keys jojobot computes is not offered: {receipt}"
    );
}

/// **A kind's optional keys never earn a line, and a kind's required keys do.**
///
/// A work item holds a status and none of the optional keys its kind lists, and
/// its receipt is silent. A loop that holds its schedule and its check-in but no
/// `name`, a key its kind requires, is told once. Both ride in one case so the
/// silence cannot pass on a build that never speaks.
#[tokio::test]
async fn a_kinds_required_keys_are_named_and_its_optional_keys_are_not() {
    let jojobot = handler();
    a_loop(&jojobot, "rhythm:descale").await;
    ensure(&jojobot, "project:atlas").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("project:atlas".into()),
            ..add_args("work", "work:delta", "Delta")
        }))
        .await
        .expect("add ok");

    let work = capture_sets(
        &jojobot,
        "work:delta",
        "started on it",
        &[("status", "now")],
    )
    .await;
    assert!(
        work.get("unlocks").is_none(),
        "a work item missing only optional keys has nothing to unlock: {work}"
    );

    let stray = capture_sets(
        &jojobot,
        "thing:kettle",
        "carries a work key",
        &[("status", "now")],
    )
    .await;
    assert!(
        stray.get("unlocks").is_none(),
        "a key of another kind's shape does not offer that kind's keys: {stray}"
    );

    let unnamed = capture_sets(
        &jojobot,
        "rhythm:descale",
        "scheduled and checked in",
        &[
            ("cadence_days", "7"),
            ("advances_from", "check_in_date"),
            ("counts_from", "2026-08-01"),
            ("last_check_in", "2026-08-01"),
        ],
    )
    .await;
    let steps = steps_for_type(&unnamed, "rhythm");
    assert_eq!(steps.len(), 1, "the kind's own shape is named: {unnamed}");
    assert_eq!(
        adds(steps[0]),
        vec!["name"],
        "only the key the kind requires, and not the optional ones: {unnamed}"
    );
}

/// **A required key the loop step already names is named once.**
#[tokio::test]
async fn a_key_the_loop_step_names_is_not_named_again_by_the_kind() {
    let jojobot = handler();
    a_loop(&jojobot, "rhythm:descale").await;

    let receipt = capture_sets(
        &jojobot,
        "rhythm:descale",
        "every week",
        &[
            ("cadence_days", "7"),
            ("advances_from", "check_in_date"),
            ("name", "descale"),
        ],
    )
    .await;
    assert_eq!(
        steps_adding(&receipt, "last_check_in").len(),
        1,
        "the check-in is named by one step only: {receipt}"
    );
    assert!(
        steps_for_type(&receipt, "rhythm").is_empty(),
        "a kind step with nothing left to add is not sent: {receipt}"
    );
}
