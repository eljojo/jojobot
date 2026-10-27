use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// Copying a served place does not move the new entry or refuse its words.
#[tokio::test]
async fn read_keys_written_back_placement() {
    let jojobot = handler();
    a_list(&jojobot, "packing").await;
    let first = capture_ok(&jojobot, capture_args("list:packing", "tent")).await;
    let moved = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                after: Some("first".into()),
                ..update_args(&address_of(&first))
            }))
            .await
            .expect("move answers"),
    );
    assert_ne!(moved["status"], "blocked", "{moved}");
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("list:packing")))
            .await
            .expect("read"),
    );
    let fields: std::collections::BTreeMap<String, String> =
        serde_json::from_value(read["objects"][0]["facts"][0]["fields"].clone())
            .expect("served fields");
    for key in ["order", "held_turn", "held_since"] {
        assert!(
            fields.contains_key(key),
            "the moved entry serves {key}: {read}"
        );
    }
    let copied_order = fields["order"].clone();
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(fields),
            ..capture_args("list:packing", "stove")
        },
    )
    .await;
    assert_eq!(
        landed["dropped_keys"],
        serde_json::json!(["held_since", "held_turn", "order"]),
        "{landed}"
    );
    assert!(
        landed["dropped_keys_note"]
            .as_str()
            .unwrap_or("")
            .contains("server"),
        "{landed}"
    );
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("list:packing")))
            .await
            .expect("read"),
    );
    let copied = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|fact| fact["content"] == "stove")
        .expect("copied claim");
    assert_ne!(
        copied["fields"]["order"], copied_order,
        "a fresh place: {read}"
    );
}

/// A copied tick account cannot tick the entry again.
#[tokio::test]
async fn read_keys_written_back_tick_account() {
    let jojobot = handler();
    a_list(&jojobot, "packing").await;
    let entry = capture_ok(&jojobot, capture_args("list:packing", "tent")).await;
    let tick = capture_ok(&jojobot, serde_json::from_value(serde_json::json!({
        "subject":"list:packing", "content":"done it", "tick":{"entry":address_of(&entry), "act":"done"}, "sid":TEST_SID
    })).expect("tick arguments")).await;
    let facts = jojobot
        .memory
        .recall(&EntityId("list:packing".into()))
        .await
        .expect("claims");
    let tick = facts
        .iter()
        .find(|fact| fact.address().to_string() == address_of(&tick))
        .expect("tick account");
    let copied = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(tick.fields.clone()),
            ..capture_args("list:packing", "a copy of the tick")
        },
    )
    .await;
    assert_eq!(
        copied["dropped_keys"],
        serde_json::json!(["tick_act", "tick_entry", "tick_session"]),
        "{copied}"
    );
    let facts = jojobot
        .memory
        .recall(&EntityId("list:packing".into()))
        .await
        .expect("claims");
    let copy = facts
        .iter()
        .find(|fact| fact.address().to_string() == address_of(&copied))
        .expect("copied words landed");
    assert!(
        copy.fields.contains_key("order"),
        "the copy is an entry: {copy:?}"
    );
    for key in ["tick_act", "tick_entry", "tick_session"] {
        assert!(!copy.fields.contains_key(key), "{copy:?}");
    }
}

/// A merge account can be quoted without forging another merge account.
#[tokio::test]
async fn read_keys_written_back_merged_from() {
    copied_merge_fields(true).await;
}

#[tokio::test]
async fn read_keys_written_back_merged_marker_only() {
    copied_merge_fields(false).await;
}

#[tokio::test]
async fn read_keys_written_back_retraction_account() {
    let jojobot = handler();
    let original = capture_ok(
        &jojobot,
        capture_args("thing:kettle", "the label says steel"),
    )
    .await;
    let taken_back = json_of(
        &jojobot
            .retract(Parameters(crate::memory::retract::RetractArgs {
                address: address_of(&original),
                reason: Some("misread the label".into()),
                recorded_at: None,
                sid: Some(TEST_SID.into()),
                request_id: None,
            }))
            .await
            .expect("retract"),
    );
    assert_ne!(taken_back["status"], "blocked", "{taken_back}");
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("thing:kettle")))
            .await
            .expect("read"),
    );
    let account = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|fact| fact["fields"].get("retracts").is_some())
        .expect("retraction account");
    let fields = serde_json::from_value(account["fields"].clone()).expect("served fields");
    let copied = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(fields),
            ..capture_args("thing:kettle", "a copy of the retraction account")
        },
    )
    .await;
    assert_eq!(
        copied["dropped_keys"],
        serde_json::json!(["retracts"]),
        "{copied}"
    );
    let held = jojobot
        .memory
        .recall(&EntityId("thing:kettle".into()))
        .await
        .expect("claims");
    let copy = held
        .iter()
        .find(|fact| fact.address().to_string() == address_of(&copied))
        .expect("copy");
    assert!(
        !copy.fields.contains_key("retracts"),
        "the copy is no retraction: {copy:?}"
    );
    assert!(
        held.iter().any(|fact| fact.fields.contains_key("retracts")),
        "the original account remains"
    );
}

/// The same account rule covers the server's settings, archive and review
/// labels. None may turn a copied claim into an exempt record or a review.
#[tokio::test]
async fn read_keys_written_back_account_bookkeeping() {
    let jojobot = handler();
    let keys = [
        "archive_reason",
        "defragged_at",
        "engine_record",
        "release_notes_told",
        "review_mark",
        "settings_record",
    ];
    let mut fields: std::collections::BTreeMap<String, String> = keys
        .into_iter()
        .map(|key| (key.into(), "copied account value".into()))
        .collect();
    fields.insert("colour".into(), "green".into());
    let copied = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(fields),
            ..capture_args("thing:kettle", "a copy of account labels")
        },
    )
    .await;
    assert_eq!(copied["dropped_keys"], serde_json::json!(keys), "{copied}");
    let held = jojobot
        .memory
        .recall(&EntityId("thing:kettle".into()))
        .await
        .expect("claims");
    let copy = held
        .iter()
        .find(|fact| fact.address().to_string() == address_of(&copied))
        .expect("copy");
    for key in keys {
        assert!(
            !copy.fields.contains_key(key),
            "copied {key} became metadata: {copy:?}"
        );
    }
    assert_eq!(copy.fields.get("colour").map(String::as_str), Some("green"));
}

async fn copied_merge_fields(whole: bool) {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    ensure(&jojobot, "thing:handcart").await;
    let merged = merging(&jojobot, "thing:handcart", "thing:kettle").await;
    assert_ne!(merged["status"], "blocked", "{merged}");
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("thing:kettle")))
            .await
            .expect("read"),
    );
    let account = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|fact| fact["fields"].get("merged_from").is_some())
        .expect("served merge account");
    let mut fields: std::collections::BTreeMap<String, String> =
        serde_json::from_value(account["fields"].clone()).expect("fields");
    if !whole {
        fields.retain(|key, _| key == "merged_from");
    }
    fields.insert("source".into(), "copied account".into());
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(fields),
            ..capture_args("thing:kettle", "a copy of the account")
        },
    )
    .await;
    assert_eq!(
        landed["dropped_keys"],
        if whole {
            serde_json::json!(["engine_record", "merged_from"])
        } else {
            serde_json::json!(["merged_from"])
        },
        "{landed}"
    );
    assert!(
        landed["dropped_keys_note"]
            .as_str()
            .unwrap_or("")
            .contains("server"),
        "{landed}"
    );
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("thing:kettle")))
            .await
            .expect("read"),
    );
    let copied = read["objects"][0]["facts"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|fact| fact["content"] == "a copy of the account")
        .expect("copied claim");
    assert!(copied["fields"].get("merged_from").is_none(), "{copied}");
    assert!(copied["fields"].get("engine_record").is_none(), "{copied}");
    assert_eq!(copied["fields"]["source"], "copied account", "{copied}");
}

/// A known stored link pasted into words follows the same target as its handle.
#[tokio::test]
async fn read_keys_written_back_stored_link() {
    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over_mentions(memory.clone());
    ensure(&jojobot, "thing:handcart").await;
    capture_ok(
        &jojobot,
        capture_args("thing:kettle", "see @thing:handcart"),
    )
    .await;
    let held = memory
        .recall(&EntityId("thing:kettle".into()))
        .await
        .expect("stored claims");
    let words = held
        .iter()
        .find(|fact| fact.content.starts_with("see "))
        .expect("stored link")
        .content
        .clone();
    assert!(words.contains("@#"), "a real stored link: {words}");
    let landed = capture_ok(&jojobot, capture_args("thing:kettle", &words)).await;
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("thing:kettle")))
            .await
            .expect("read"),
    );
    assert!(
        read["objects"][0]["facts"]
            .as_array()
            .expect("claims")
            .iter()
            .any(|fact| fact["address"] == landed["address"]
                && fact["content"] == "see @thing:handcart"),
        "{read}"
    );
}

/// Dropping a copied place leaves an otherwise empty setting bag empty: creating
/// or relabelling the list must not invent an entry to carry the dropped keys.
#[tokio::test]
async fn read_keys_written_back_other_write_doors() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    let copied = || Some([("order".into(), "copied".into())].into());
    let added = json_of(
        &jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                sets: copied(),
                ..add_args("list", "packing", "packing")
            }))
            .await
            .expect("add"),
    );
    assert_ne!(added["status"], "blocked", "{added}");
    assert_eq!(
        added["dropped_keys"],
        serde_json::json!(["order"]),
        "{added}"
    );
    assert!(
        jojobot
            .memory
            .recall(&EntityId("list:packing".into()))
            .await
            .expect("entries")
            .is_empty(),
        "creation with no remaining settings invents no entry: {added}"
    );
    let updated = json_of(
        &jojobot
            .update_entity(Parameters(
                serde_json::from_value(serde_json::json!({
                    "handle": "list:packing", "sets": {"order": "copied"}, "sid": TEST_SID,
                }))
                .expect("update arguments"),
            ))
            .await
            .expect("update"),
    );
    assert_ne!(updated["status"], "blocked", "{updated}");
    assert_eq!(
        updated["dropped_keys"],
        serde_json::json!(["order"]),
        "{updated}"
    );
    assert!(
        jojobot
            .memory
            .recall(&EntityId("list:packing".into()))
            .await
            .expect("entries")
            .is_empty(),
        "an empty setting edit invents no entry: {updated}"
    );
    let entry = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: copied(),
            sets: copied(),
            ..capture_args("list:packing", "tent")
        },
    )
    .await;
    assert_eq!(
        entry["dropped_keys"],
        serde_json::json!(["order"]),
        "{entry}"
    );
    let edited = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                fields: copied(),
                sets: copied(),
                clear_fields: Some(vec!["order".into()]),
                content: Some("tent kept dry".into()),
                ..update_args(&address_of(&entry))
            }))
            .await
            .expect("edit"),
    );
    assert_ne!(edited["status"], "blocked", "{edited}");
    assert_eq!(
        edited["dropped_keys"],
        serde_json::json!(["order"]),
        "{edited}"
    );
    let read = jojobot
        .memory
        .recall(&EntityId("list:packing".into()))
        .await
        .expect("entries");
    assert!(
        read[0].fields.contains_key("order"),
        "a copied clear did not remove the place: {read:?}"
    );
}

/// Copied bookkeeping cannot turn a setting rewrite into another live record.
#[tokio::test]
async fn read_keys_written_back_preserves_setting_reuse() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    for copied in [false, true] {
        let mut sets = serde_json::json!({"colour": "blue"});
        if copied {
            let facts = jojobot
                .memory
                .recall(&EntityId("thing:kettle".into()))
                .await
                .expect("settings read");
            sets["settings_record"] = facts[0].fields["settings_record"].clone().into();
        }
        let answer = json_of(
            &jojobot
                .update_entity(Parameters(
                    serde_json::from_value(serde_json::json!({
                        "handle": "thing:kettle", "sets": sets, "sid": TEST_SID,
                    }))
                    .expect("update arguments"),
                ))
                .await
                .expect("update"),
        );
        assert_ne!(answer["status"], "blocked", "{answer}");
        let facts = jojobot
            .memory
            .recall(&EntityId("thing:kettle".into()))
            .await
            .expect("settings read");
        assert_eq!(facts.len(), 1, "the setting record was reused: {facts:?}");
        if copied {
            assert_eq!(
                answer["dropped_keys"],
                serde_json::json!(["settings_record"]),
                "{answer}"
            );
        }
    }
}

/// A rhythm with a whole schedule on it, ready to take a check-in.
async fn a_weekly_rhythm(jojobot: &Jojobot, handle: &str, counts_from: &str, advances: &str) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", handle, handle)
        }))
        .await
        .expect("add ok");
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), advances.to_string()),
                    ("counts_from".to_string(), counts_from.to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args(&format!("rhythm:{handle}"), "every week")
        },
    )
    .await;
}

/// **A schedule jojobot worked out is not something the user said.**
///
/// A check-in computes the dates the next cycle counts from. Merged into
/// the caller's own record, those keys make a record captured as testimony
/// read a date nobody uttered back as the user's word. **A folded value is
/// read with the certainty of the claim that carried it**, so that is
/// permanent and invisible.
///
/// **Paired with an ordinary capture in the same case**: testimony stays
/// testimony when nothing was computed, so this cannot pass against a build
/// that files everything as a derivation.
#[tokio::test]
async fn a_computed_schedule_does_not_inherit_the_callers_word() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    let checked_in = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            provenance: Some("testimony".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        },
    )
    .await;
    assert_eq!(
        checked_in["provenance"], "inference",
        "a record carrying dates jojobot computed reads as the user's own word: {checked_in}",
    );

    let said = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "their own word: due fortnightly")
        },
    )
    .await;
    assert_eq!(
        said["provenance"], "testimony",
        "a claim with nothing computed in it was demoted too: {said}",
    );
}

/// **A check-in that is sent beside a key it computes is refused, and the
/// refusal does not offer to type the arithmetic by hand.** The way forward it
/// names is to send the check-in without the key, because the check-in works the
/// key out. It used to offer sending the fields without a check-in and keeping
/// the arithmetic, which invites writing the basis of a schedule by hand and
/// leaves it unchecked. Beside it the same check-in without the key, which lands.
#[tokio::test]
async fn a_check_in_beside_a_computed_key_is_refused_and_the_way_forward_is_the_check_in() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                sets: Some(
                    [("counts_from".to_string(), "2026-08-10".to_string())]
                        .into_iter()
                        .collect(),
                ),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "did it this morning")
            }))
            .await
            .expect("a refusal is an answer"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains("check-in") && how.contains("counts_from"),
        "the refusal does not name the check-in and the key: {how}"
    );
    assert!(
        !how.contains("arithmetic") && !how.contains("without check_in"),
        "the refusal offers to type the schedule by hand: {how}"
    );

    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        },
    )
    .await;
    assert!(landed["address"].is_string(), "{landed}");
    assert_ne!(landed["status"], "blocked", "{landed}");
}

/// **The case this whole slice exists for: a cadence changes with no
/// check-in in sight, and the stored due moment moves with it.**
///
/// `a_weekly_rhythm` itself is a plain capture — cadence, policy and
/// basis, no `check_in` — so it already proves the mechanism runs on
/// ordinary writes: opening the loop this way already stores `due_on`.
/// This asks the harder question, changing ONE input afterward.
///
/// **Same demotion as a check-in's**, because the same reasoning applies:
/// jojobot's own arithmetic is jojobot's, whatever the caller's own claim
/// says about their cadence.
#[tokio::test]
async fn a_plain_capture_that_moves_the_cadence_keeps_the_due_moment_current() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let opened = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(
        opened["due_on"], "2026-08-08",
        "opening the loop is an ordinary capture too, and it already stores the due moment: \
             {opened}",
    );

    let edited = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some(
                [("cadence_days".to_string(), "14".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:descale", "let's do it every two weeks instead")
        },
    )
    .await;
    assert_eq!(
        edited["provenance"], "inference",
        "a moved due moment overrides the caller's own provenance, same as a check-in's: \
             {edited}",
    );
    // 🚨 **The demotion fires on this path too, and the receipt's own
    // explanation used to be gated on the check-in case alone.** A caller
    // that never asked for a check-in got the substitution with no reason
    // on the wire — the exact silence a difference-with-no-reason is
    // built to avoid.
    let because = edited["delta"]
        .as_array()
        .unwrap_or_else(|| panic!("a demoted provenance is a difference: {edited}"))
        .iter()
        .find(|d| d["field"] == "provenance")
        .unwrap_or_else(|| panic!("the provenance difference is missing: {edited}"))["because"]
        .clone();
    assert_ne!(
        because,
        serde_json::Value::Null,
        "the demotion happened with no reason on the wire, on the one path whose own \
             explanation was gated on a different trigger: {edited}",
    );
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(
        held["due_on"], "2026-08-15",
        "the cadence doubled off the same basis, and the stored due moment moved with it, \
             with no check-in anywhere in this test: {held}",
    );
}

/// **A value the store did not keep as it was sent is named on the
/// receipt.**
///
/// A caller sends `provenance` and gets a record carrying a different one:
/// the check-in path writes its own, because the record it builds mixes a
/// caller's sentence with a schedule jojobot computed. The substitution is
/// correct and the silence is not — a caller that cannot see its own value
/// replaced has to price every other write at its worst case.
///
/// **Paired with a capture where nothing differs**, which carries no delta
/// at all: a line that prints on every write is noise a reader learns to
/// skip, and the pair is what keeps this one meaningful.
#[tokio::test]
async fn a_stored_value_that_differs_from_the_sent_one_is_named() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    let substituted = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            provenance: Some("testimony".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        },
    )
    .await;

    let delta = &substituted["delta"];
    assert!(
        delta.is_array(),
        "the receipt of a write that replaced a caller's value says so: {substituted}",
    );
    let replaced = delta
        .as_array()
        .expect("checked above")
        .iter()
        .find(|d| d["field"] == "provenance")
        .unwrap_or_else(|| panic!("the replaced field is named: {substituted}"));
    assert_eq!(replaced["sent"], "testimony", "{substituted}");
    assert_eq!(replaced["stored"], "inference", "{substituted}");

    let kept = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("rhythm:descale", "their own word: due fortnightly")
        },
    )
    .await;
    assert_eq!(
        kept["delta"],
        serde_json::Value::Null,
        "a write that kept every value it was sent carries no delta: {kept}",
    );
}

/// A check-in that sends a provenance the check-in path overrules — the
/// one call on this surface known to store a value other than the one it
/// was sent.
fn a_late_check_in() -> CaptureArgs {
    CaptureArgs {
        check_in: Some("ran".into()),
        provenance: Some("testimony".into()),
        recorded_at: Some("2026-08-10".into()),
        ..capture_args("rhythm:descale", "did it this morning")
    }
}

/// **A write says what now stands and what it left alone.**
///
/// The measured failure this answers: an agent declined to record a second
/// account of an event because it believed the write would overwrite the
/// first. It would not have. Nothing on the surface said so, and the
/// information was lost permanently and silently.
///
/// ⚠️ **The line is computed, never a constant.** A capture carrying fields
/// moves what those keys answer for the thing, so the line names them; a
/// capture carrying none moves nothing and names none. **That difference is
/// the case**: a line that reads *nothing was changed* unconditionally is a
/// false promise in the one place a caller has been taught to trust, which
/// is worse than no line at all.
#[tokio::test]
async fn a_capture_says_what_now_stands_and_what_it_left_alone() {
    let jojobot = handler();

    let first = capture_ok(
        &jojobot,
        capture_args("person:alpha", "said the kiln was lit"),
    )
    .await;
    let opening = postcondition_of(&first);
    assert!(
        opening.contains('1'),
        "the line has to say how much now stands on this thing: {first}",
    );

    // The second account of the same thing, contradicting the first. This
    // is the write an agent talked itself out of.
    let second = capture_ok(
        &jojobot,
        capture_args("person:alpha", "said the kiln had never been lit"),
    )
    .await;
    let both = postcondition_of(&second);
    assert!(
        both.contains('2'),
        "two accounts now stand and the line has to say so: {second}",
    );
    assert!(
        !both.contains("mood") && !both.contains("kiln"),
        "a write that carried no keys names none: {second}",
    );

    // The same verb, this time displacing what a key answers.
    let with_keys = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("mood".to_string(), "delighted".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:alpha", "was pleased about it")
        },
    )
    .await;
    let moved = postcondition_of(&with_keys);
    assert!(
        moved.contains("mood"),
        "this write moved what 'mood' answers for the thing, and the line that says nothing \
             changed is a false promise unless it names it: {with_keys}",
    );
}

/// The postcondition line of a receipt, which every write carries.
fn postcondition_of(body: &serde_json::Value) -> String {
    body["postcondition"]
        .as_str()
        .unwrap_or_else(|| panic!("a write states what now stands: {body}"))
        .to_string()
}

/// **A delta says why, where the verb that substituted has a reason.**
///
/// ⚠️ **The line must not read as an apology.** This substitution is
/// correct: the record mixes a caller's sentence with a schedule jojobot
/// computed, and only one of those has anybody's word behind it. A bare
/// *stored differs from sent* reads as a fault report, and a caller that
/// reads it as one learns to distrust a verb that did the right thing.
///
/// **The reason is a fact about the record, not about how the server went
/// about the write** — it says what the stored value IS and why that is
/// what the record can carry, which is where rule 158 draws the line.
///
/// ⚠️ **What the pairing below watches, and what it cannot.** It catches a
/// reason smeared onto every difference. It does NOT catch the reason
/// escaping to a provenance substitution some other verb makes, and no
/// case can: `check_in` is the only path on this surface that stores a
/// provenance other than the one it was sent, so a build that attached
/// this reason unconditionally is indistinguishable from this one through
/// the served surface. The condition is held by construction. **The day a
/// second substituting path lands, that is the case to write.**
#[tokio::test]
async fn a_substitution_with_a_reason_carries_it() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let receipt = capture_ok(&jojobot, a_late_check_in()).await;

    let because = receipt["delta"][0]["because"]
        .as_str()
        .unwrap_or_else(|| panic!("the substitution this verb makes has a reason: {receipt}"))
        .to_string();
    assert!(
        because.contains("check_in") || because.contains("schedule"),
        "the reason has to say what about this call replaced the value: {receipt}",
    );

    // The rendered line names the field and points at `delta`, where the
    // reason sits whole beside the values, so the reason is stated once.
    let note = receipt["delta_note"].as_str().expect("a rendered line");
    assert!(!note.contains(&because), "{receipt}");

    // ⭐ **Paired with a difference that has no reason to give.** A caller
    // that names a bare handle gets it qualified, and nothing about that
    // needs explaining — so `because` is absent rather than filled with a
    // sentence restating the comparison. This is the half that fails on a
    // build attaching the reason to every difference.
    let qualified = capture_ok(
        &jojobot,
        CaptureArgs {
            ..capture_args("alpha", "said the kiln was lit")
        },
    )
    .await;
    assert_eq!(qualified["delta"][0]["field"], "subject", "{qualified}");
    assert_eq!(
        qualified["delta"][0]["because"],
        serde_json::Value::Null,
        "a difference with nothing to explain carries no explanation: {qualified}",
    );
}

/// **A check-in records what was found, and jojobot does the arithmetic.**
///
/// The caller says which of the three outcomes it was and on what day; the
/// date the next cycle counts from is worked out here, because a caller
/// doing that by hand is a caller who can get it wrong once and never find
/// out.
///
/// The three outcomes together, because the difference between them is the
/// whole point of having three: a run and a skip move the schedule
/// identically and only the record tells them apart, while a snooze moves
/// nothing at all.
#[tokio::test]
async fn a_check_in_moves_the_schedule_and_a_snooze_does_not() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "descaled it, took ten minutes")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["outcome"], "ran");
    assert_eq!(held["last_check_in"], "2026-08-10");
    assert_eq!(
        held["counts_from"], "2026-08-10",
        "advancing from the check-in date moves the cycle to the day it happened: {held}",
    );

    // A snooze is still a check-in — it says when it happened — and the
    // schedule is exactly where it was.
    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("snoozed".into()),
            recorded_at: Some("2026-08-19".into()),
            sets: Some(
                [("snoozed_until".to_string(), "2026-08-22".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:descale", "not this week")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["outcome"], "snoozed");
    assert_eq!(held["last_check_in"], "2026-08-19");
    assert_eq!(
        held["counts_from"], "2026-08-10",
        "a snooze does not consume the cycle, so the schedule is untouched: {held}",
    );

    // And a skip advances it exactly as the run did, leaving a record that
    // says it did not happen — which is the only place the two differ.
    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("skipped".into()),
            recorded_at: Some("2026-08-20".into()),
            ..capture_args("rhythm:descale", "away, not doing it")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["outcome"], "skipped");
    assert_eq!(
        held["counts_from"], "2026-08-20",
        "a skipped cycle advances as if it had run: {held}",
    );
}

/// A snooze check-in on the weekly rhythm, naming the day it lasts until.
fn a_snooze_until(day: Option<&str>, on: &str) -> CaptureArgs {
    CaptureArgs {
        check_in: Some("snoozed".into()),
        recorded_at: Some(on.into()),
        sets: day.map(|day| {
            [("snoozed_until".to_string(), day.to_string())]
                .into_iter()
                .collect()
        }),
        ..capture_args("rhythm:descale", "away until then")
    }
}

/// **A snoozed loop is next due on the later of its own day and the snooze
/// day, and the stored due moment says so.** The cycle is still not consumed,
/// so `counts_from` stays where it was, and the next ran check-in ends the
/// snooze and puts the loop back on its own cadence.
#[tokio::test]
async fn a_snooze_holds_the_loop_back_until_its_day_and_a_run_ends_it() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    // Due on the 8th; away until the 20th.
    capture_ok(&jojobot, a_snooze_until(Some("2026-08-20"), "2026-08-10")).await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["snoozed_until"], "2026-08-20", "{held}");
    assert_eq!(held["counts_from"], "2026-08-01", "{held}");
    assert_eq!(
        held["due_on"], "2026-08-20",
        "the later of its own day, the 8th, and the snooze day: {held}"
    );

    // Back early, on the 12th, and it runs: the snooze is over and the cadence
    // is the loop's own again. Its own next day, the 19th, is before the day
    // the snooze named, so a loop that went on counting the day would say the
    // 20th.
    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-12".into()),
            ..capture_args("rhythm:descale", "descaled it")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["outcome"], "ran", "{held}");
    assert_eq!(
        held["snoozed_until"], "2026-08-20",
        "no write can take the key off, so the old day stays as history: {held}"
    );
    assert_eq!(held["due_on"], "2026-08-19", "{held}");
}

/// **A check-in can carry the schedule's own keys in `sets`, and that opens a
/// bare rhythm.** The served text says a loop holding neither key is refused
/// until it does, stored or sent in the same call.
#[tokio::test]
async fn a_check_in_carrying_the_cadence_and_its_basis_opens_a_bare_rhythm() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            sets: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("rhythm:descale", "descaled it")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["counts_from"], "2026-08-10", "{held}");
    assert_eq!(held["due_on"], "2026-08-17", "{held}");
}

/// **A snooze day sent in `fields` is read for the check-in and stays on the
/// record: the thing does not hold it, so the loop is not held back.** The
/// refusal that asks for the day names `sets` for that reason.
#[tokio::test]
async fn a_snooze_day_sent_in_fields_does_not_hold_the_loop_back() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("snoozed".into()),
            recorded_at: Some("2026-08-10".into()),
            fields: Some(
                [("snoozed_until".to_string(), "2026-08-20".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:descale", "away until then")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert!(
        held.get("snoozed_until").is_none(),
        "the day stayed on the record: {held}"
    );
    assert_eq!(held["due_on"], "2026-08-08", "its own day, unmoved: {held}");
}

/// **The stored due moment follows the outcome when an edit changes only the
/// outcome.** The snooze day counts only while the last outcome is `snoozed`, so
/// an edit that turns that outcome into `ran` moves the loop back to its own
/// day, and one that turns it back revives the snooze. `outcome` is not a key
/// that makes a thing a loop, so the mover has to be told to watch it, or
/// `due_on` keeps the day the read no longer agrees with.
///
/// **Both directions ride in one case**, so a build that watched the key for
/// one move only would not pass it.
#[tokio::test]
async fn an_edit_that_changes_only_the_outcome_moves_the_stored_due_moment() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let snoozed = capture_ok(&jojobot, a_snooze_until(Some("2026-08-20"), "2026-08-10")).await;
    let address = address_of(&snoozed);
    let due_on = || async { fields_of(&jojobot, "rhythm:descale").await["due_on"].clone() };
    assert_eq!(
        due_on().await,
        "2026-08-20",
        "the snooze holds the loop back"
    );

    let edit = |outcome: &str| UpdateFactArgs {
        sets: Some(
            [("outcome".to_string(), outcome.to_string())]
                .into_iter()
                .collect(),
        ),
        ..update_args(&address)
    };
    jojobot
        .update_fact(Parameters(edit("ran")))
        .await
        .expect("update ok");
    assert_eq!(
        due_on().await,
        "2026-08-08",
        "the outcome is no longer a snooze, so the loop is due on its own day"
    );

    jojobot
        .update_fact(Parameters(edit("snoozed")))
        .await
        .expect("update ok");
    assert_eq!(
        due_on().await,
        "2026-08-20",
        "the outcome is a snooze again, so the held day holds the loop back"
    );
}

/// **A recall of a loop says whether its snooze day is in force.** The old day
/// stays in the record after a run, because no write can take a key off a loop,
/// and a reader of the bare fields could not tell it from a live snooze. So
/// the answer states it beside the fields, on both sides, and says nothing on
/// a loop that was never snoozed.
#[tokio::test]
async fn a_recall_says_whether_the_snooze_day_is_in_force() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let snooze_of = |body: &serde_json::Value| body["objects"][0]["snooze"].clone();
    let recalled = || async {
        json_of(
            &jojobot
                .recall(Parameters(recall_args("rhythm:descale")))
                .await
                .expect("recall ok"),
        )
    };

    assert_eq!(
        snooze_of(&recalled().await),
        serde_json::Value::Null,
        "a loop never snoozed carries no snooze answer"
    );

    capture_ok(&jojobot, a_snooze_until(Some("2026-08-20"), "2026-08-10")).await;
    let snooze = snooze_of(&recalled().await);
    assert_eq!(snooze["until"], "2026-08-20", "{snooze}");
    assert_eq!(snooze["in_force"], true, "{snooze}");

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-12".into()),
            ..capture_args("rhythm:descale", "back early, descaled it")
        },
    )
    .await;
    let snooze = snooze_of(&recalled().await);
    assert_eq!(snooze["until"], "2026-08-20", "{snooze}");
    assert_eq!(
        snooze["in_force"], false,
        "the last check-in was a run, so the old day is history: {snooze}"
    );
}

/// **A snooze that names no usable day is refused with what to send, and
/// nothing is written.** The refusals ride beside one that lands, or a build
/// that refused every snooze would pass.
#[tokio::test]
async fn a_snooze_with_no_usable_day_is_refused_and_writes_nothing() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    for (day, on) in [
        (None, "2026-08-10"),
        (Some("next week"), "2026-08-10"),
        (Some("2026-08-10"), "2026-08-10"),
        (Some("2026-08-02"), "2026-08-10"),
    ] {
        let refused = json_of(
            &jojobot
                .capture(Parameters(a_snooze_until(day, on)))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked", "{day:?}: {refused}");
        assert_eq!(refused["wrote"], false, "{day:?}: {refused}");
        let how = refused["how_to_proceed"].as_str().unwrap_or_default();
        assert!(
            how.contains("snoozed_until"),
            "the way forward names the key to send: {refused}"
        );
        assert!(
            how.contains(on),
            "and the check-in's own day, which the snooze day has to be after: {refused}"
        );
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(held["outcome"], serde_json::Value::Null, "{day:?}: {held}");
        assert_eq!(
            held["snoozed_until"],
            serde_json::Value::Null,
            "{day:?}: {held}"
        );
    }

    capture_ok(&jojobot, a_snooze_until(Some("2026-08-11"), "2026-08-10")).await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["snoozed_until"], "2026-08-11", "the positive: {held}");
}

/// **A ran or skipped check-in that sends a snooze day is refused**: it
/// consumes the cycle, and a snooze day on it would say two things about one
/// loop.
#[tokio::test]
async fn a_run_that_sends_a_snooze_day_is_refused() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                sets: Some(
                    [("snoozed_until".to_string(), "2026-08-20".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("rhythm:descale", "did it")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["outcome"], serde_json::Value::Null, "{held}");
}

/// **A measurement rides on the check-in, never on the schedule.**
///
/// A cadence is always time. What the check found — a reading, a distance,
/// a count — is the caller's own key on the same record, kept as written
/// beside the keys jojobot computed.
#[tokio::test]
async fn a_check_in_carries_the_callers_own_measurement() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "read-meter", "2026-08-01", "due_date").await;

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-12".into()),
            sets: Some(
                [("reading_kwh".to_string(), "4184".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:read-meter", "read it off the dial")
        },
    )
    .await;

    let held = fields_of(&jojobot, "rhythm:read-meter").await;
    assert_eq!(
        held["reading_kwh"], "4184",
        "the caller's key is kept: {held}"
    );
    assert_eq!(
        held["counts_from"], "2026-08-08",
        "and this one advances from the date it fell due, not from the late check-in: {held}",
    );
}

/// **A rhythm that cannot say how often it repeats takes no check-in.**
///
/// A cadence is the one thing no check-in can invent: the refusal names both
/// shapes it can take, a number of days and a calendar rule, and writes
/// nothing. The policy it advances from defaults beside the cadence.
#[tokio::test]
async fn a_rhythm_with_no_cadence_refuses_the_check_in_and_writes_nothing() {
    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over(memory.clone());
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "half-made", "Half Made")
        }))
        .await
        .expect("add ok");
    // A loop made before the schedule was checked at the making: no verb
    // writes this any more, so it is staged the way it arose.
    memory.fields_past_the_guard(
        &EntityId("rhythm:half-made".into()),
        &[("advances_from", "due_date"), ("counts_from", "2026-08-01")],
    );

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-12".into()),
                ..capture_args("rhythm:half-made", "did it")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked");
    assert_eq!(refused["wrote"], false);
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        how.contains("cadence_days") && how.contains("rrule"),
        "the way forward names both shapes a cadence takes: {how}",
    );

    // Nothing was written — not the record, and not the outcome key that a
    // half-applied check-in would have left behind.
    let held = fields_of(&jojobot, "rhythm:half-made").await;
    assert_eq!(held["outcome"], serde_json::Value::Null);
    assert_eq!(held["last_check_in"], serde_json::Value::Null);
}

/// **A key the check-in computes is a contradiction when the caller sends
/// it too**, not an override.
///
/// The record would say two things about one schedule and nothing could say
/// which was meant, so the call is refused and nothing is written. The
/// refusal names the key, because which of the three it was is what the
/// caller has to remove.
#[tokio::test]
async fn a_check_in_that_also_sends_a_computed_key_is_refused() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                sets: Some(
                    [("counts_from".to_string(), "2026-09-01".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(
                    "rhythm:descale",
                    "descaled it, and moved the schedule myself",
                )
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked");
    assert_eq!(refused["wrote"], false);
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed")
            .contains("counts_from"),
        "the way forward names the key that is doubled: {refused}",
    );

    // Nothing moved: not the caller's date, and not the arithmetic either.
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(
        held["counts_from"], "2026-08-01",
        "the schedule is where it was: {held}",
    );
    assert_eq!(held["outcome"], serde_json::Value::Null);

    // The paired positive: the same check-in without that key lands, so the
    // refusal is about the contradiction and not about the check-in.
    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "descaled it")
        },
    )
    .await;
    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(held["counts_from"], "2026-08-10");
}

/// **A check-in is rhythm vocabulary**, and a subject of any other kind is
/// refused rather than quietly written with keys that mean nothing on it.
///
/// ⚠️ **The subject carries a whole schedule on purpose.** Matching is
/// structural everywhere else here, so a `thing` holding the three keys
/// answers every question the schedule reader asks — which leaves the KIND
/// as the only thing that can refuse this call. Without those keys the case
/// passed against a build with no kind check at all, refused by the
/// half-configured gate instead and asserting nothing it claimed to.
#[tokio::test]
async fn a_check_in_on_something_that_is_not_a_rhythm_is_refused() {
    let jojobot = handler();
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                    ("counts_from".to_string(), "2026-08-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("thing:kettle", "descaled weekly, in principle")
        },
    )
    .await;

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("thing:kettle", "boiled it")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked");
    assert_eq!(refused["wrote"], false);
    assert_eq!(refused["attempted"], "thing:kettle");

    // The paired positive: nothing about the schedule was the problem, so
    // the same keys on a real rhythm take the same check-in.
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "boiled it")
        },
    )
    .await;

    // And the refused call left nothing on the thing.
    let held = fields_of(&jojobot, "thing:kettle").await;
    assert_eq!(held["outcome"], serde_json::Value::Null);
    assert_eq!(held["last_check_in"], serde_json::Value::Null);
}

/// **The path the teaching in `add_entity` cannot reach: a session that
/// never saw it, or saw it and hand-typed the schedule anyway.** The
/// receipt says so — the basis this cycle counts from is hand-typed, not
/// derived — and names the call that would derive it instead.
///
/// Three calls, because either negative alone passes on a build that
/// always attaches the note or never does. A check-in derives
/// `counts_from` itself — `WHY_A_CHECK_IN_DERIVES` already covers that
/// path — and a capture naming no schedule key is not this case at all.
#[tokio::test]
async fn a_hand_typed_schedule_basis_says_so_in_the_receipt_and_only_then() {
    let jojobot = handler();
    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

    // The failing path: `counts_from` sent by hand, no `check_in`.
    let hand_typed = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("counts_from".to_string(), "2026-09-01".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:descale", "moved the schedule myself")
        },
    )
    .await;
    let note = hand_typed["postcondition"]
        .as_str()
        .expect("a postcondition string");
    assert!(
        note.contains("hand-typed") && note.contains("check_in"),
        "the receipt says the basis is hand-typed and names the call that would derive it: \
             {note}"
    );

    // The paired negative: an ordinary check-in derives `counts_from`
    // itself, so it is not this case.
    let checked_in = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "descaled it")
        },
    )
    .await;
    assert!(
        !checked_in["postcondition"]
            .as_str()
            .expect("a postcondition string")
            .contains("hand-typed"),
        "a check-in derives counts_from itself, so it is not hand-typed: {checked_in}"
    );

    // The second negative: a capture naming no schedule key at all is
    // not this case either.
    let untouched = capture_ok(
        &jojobot,
        capture_args("rhythm:descale", "just a note about it"),
    )
    .await;
    assert!(
        !untouched["postcondition"]
            .as_str()
            .expect("a postcondition string")
            .contains("hand-typed"),
        "a capture naming no schedule key is not this case: {untouched}"
    );
}

/// A loop that holds a cadence and a policy and has never been checked in
/// — what `add_entity` plus one ordinary capture leaves behind, and the
/// shape every observed opening actually starts from.
async fn a_cadenced_rhythm_with_no_basis(jojobot: &Jojobot, handle: &str, advances: &str) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", handle, handle)
        }))
        .await
        .expect("add ok");
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), advances.to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args(&format!("rhythm:{handle}"), "every week")
        },
    )
    .await;
}

/// Which loops a `recall` says have gone quiet as of a day.
async fn quiet_as_of(jojobot: &Jojobot, as_of: &str) -> Vec<String> {
    let read = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                subject: None,
                overdue: Some(super::recall::OverdueArgs {
                    as_of: Some(as_of.into()),
                }),
                ..recall_args("rhythm:descale")
            }))
            .await
            .expect("recall ok"),
    );
    read["objects"]
        .as_array()
        .map(|objects| {
            objects
                .iter()
                .filter_map(|one| one["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 🚨 **A back-dated check-in ALONE opens a loop, with no basis typed by
/// hand.** This is the route the rhythms procedure teaches, and until the
/// opening derive existed the engine refused it: a check-in was blocked
/// until `counts_from` was already there, and `counts_from` cannot ride a
/// check-in call, so the only way to open a loop was to type the one value
/// a check-in exists to derive.
///
/// **The whole journey, through the served surface**: capture writes it,
/// the store folds it, and the loop's own carrier computes a due date the
/// read selects on. Asserted from BOTH sides of the boundary, because a
/// build that put the basis anywhere else still answers one side
/// correctly.
#[tokio::test]
async fn a_back_dated_check_in_alone_opens_a_loop_that_holds_a_cadence() {
    let jojobot = handler();
    a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

    capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-06-14".into()),
            ..capture_args(
                "rhythm:descale",
                "did it back before this session opened it",
            )
        },
    )
    .await;

    let held = fields_of(&jojobot, "rhythm:descale").await;
    assert_eq!(
        held["counts_from"], "2026-06-14",
        "the basis is the check-in's own date: {held}"
    );
    assert_eq!(
        held["last_check_in"], "2026-06-14",
        "and the turn is recorded on the day it happened: {held}"
    );

    // A cadence of seven from the fourteenth falls due on the twenty-first.
    assert!(
        !quiet_as_of(&jojobot, "2026-06-20")
            .await
            .contains(&"rhythm:descale".to_string()),
        "the day before it falls due, the loop is not owed",
    );
    assert!(
        quiet_as_of(&jojobot, "2026-06-21")
            .await
            .contains(&"rhythm:descale".to_string()),
        "and a cadence after the check-in's date, it is",
    );
}

/// **The receipt says the basis was derived, and says it only when it
/// was.** The mirror of the hand-typed sentence beside it: one caller
/// learns jojobot did the arithmetic, the other learns it did not.
///
/// Three calls, because either negative alone passes on a build that
/// always attaches the note or never does.
#[tokio::test]
async fn the_receipt_says_when_a_check_in_opened_the_loop_and_only_then() {
    let jojobot = handler();
    a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

    let opening = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-06-14".into()),
            ..capture_args("rhythm:descale", "did it back then")
        },
    )
    .await;
    let note = opening["postcondition"]
        .as_str()
        .expect("a postcondition string");
    assert!(
        note.contains("derived") && note.contains("2026-06-14"),
        "the receipt says the basis was derived and from which day: {note}"
    );

    // The paired negative: the NEXT check-in on the same loop advances a
    // basis that was already there, so it opened nothing.
    let later = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-06-28".into()),
            ..capture_args("rhythm:descale", "did it again")
        },
    )
    .await;
    assert!(
        !later["postcondition"]
            .as_str()
            .expect("a postcondition string")
            .contains("derived"),
        "a check-in on a loop that already had a basis opened nothing: {later}"
    );

    // The second negative: an ordinary capture is not this case at all.
    let plain = capture_ok(
        &jojobot,
        capture_args("rhythm:descale", "just a note about it"),
    )
    .await;
    assert!(
        !plain["postcondition"]
            .as_str()
            .expect("a postcondition string")
            .contains("derived"),
        "a capture that asked for no check-in opened nothing: {plain}"
    );
}

/// ⚠️ **A snooze cannot open a loop, and the refusal says what can.**
///
/// A snooze is the outcome that leaves a schedule where it was, and a loop
/// with no basis has no schedule to leave anywhere. The generic
/// missing-key sentence is the WRONG advice here: it tells the caller to
/// capture `counts_from`, which is the one move this whole path exists to
/// remove. So the refusal names the outcomes that do open a loop instead.
#[tokio::test]
async fn a_snooze_cannot_open_a_loop_and_the_refusal_names_what_can() {
    let jojobot = handler();
    a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("snoozed".into()),
                recorded_at: Some("2026-06-14".into()),
                sets: Some(
                    [("snoozed_until".to_string(), "2026-06-20".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("rhythm:descale", "not today")
            }))
            .await
            .expect("capture returns"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains("ran") && how.contains("skipped"),
        "the refusal names the outcomes that DO open a loop: {how}"
    );
    assert!(
        !how.contains("Capture the missing key"),
        "and it does not send the caller to type the basis by hand: {how}"
    );

    // The positive this rests on: the same call with a consuming outcome
    // is not refused, so the refusal is about the outcome and not the loop.
    let opened = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("skipped".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args("rhythm:descale", "did not do it, cycle moves on")
            }))
            .await
            .expect("capture returns"),
    );
    assert_ne!(opened["status"], "blocked", "a skip opens it: {opened}");
}

/// **The refusal that is still right stays exactly as it was.** No
/// check-in can say how long a cycle lasts, so a loop short of its cadence
/// is refused and told which key to capture — which is the case the
/// opening derive must not reach.
#[tokio::test]
async fn a_loop_short_of_its_cadence_still_names_the_key_to_capture() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "half-made", "half-made")
        }))
        .await
        .expect("add ok");

    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args("rhythm:half-made", "did it back then")
            }))
            .await
            .expect("capture returns"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains("cadence_days") && how.contains("Capture the missing key"),
        "a loop with no cadence is told which key to capture: {how}"
    );
    assert!(
        !how.contains("counts_from"),
        "counts_from is not this loop's problem, and a working check-in supplies it — \
             naming it here sends the caller to type the one value this whole path exists to \
             derive: {how}"
    );
}

/// **Fields ride on a fact, and no label is asked for.**
///
/// A record's fields ARE the thing it describes, so the write that puts a
/// key on a record cannot be gated on the writer also naming a class for
/// it. Gated, "the fields of a thing" means "the fields somebody opted
/// in", which is a biased sample — and every read that groups a thing's
/// records computes over that sample.
/// **The receipt reads the day the RUN is asking about, not the day the
/// claim is about.**
///
/// Which day it is belongs to the caller (rule 222). A claim carries the
/// day it is true OF, and that is a different question: a reading taken in
/// January and recorded now is about January, and whether it is still good
/// is asked as of today.
///
/// **Both halves in one run, because the disagreement is the defect.** A
/// receipt that answers as of the claim's own date says a reading is fine,
/// and the very next read of the same record in the same session says it is
/// stale. Nothing else in the answer changes, so a caller has no way to see
/// which of the two it should believe.
#[tokio::test]
async fn a_backdated_claim_is_receipted_as_of_today() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    let receipt = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                recorded_at: Some("2026-01-10".into()),
                stale_after: Some("2026-01-20".into()),
                ..capture_args("person:alpha", "the rate the bank quoted")
            }))
            .await
            .expect("capture ok"),
    );
    assert_eq!(
        receipt["stale_after"], "2026-01-20",
        "the day the writer set is on the receipt: {receipt}",
    );
    assert_eq!(
        receipt["stale"],
        serde_json::json!(true),
        "the receipt read the claim's own day, so a reading past its day came back fine: \
             {receipt}",
    );

    // The same record, read back in the same run: a caller meeting two
    // answers has no way to tell which one this session believes.
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        read["objects"][0]["facts"][0]["stale"], receipt["stale"],
        "the receipt and the read disagree about the same record in one run: {read}",
    );
}

/// 🚨 **A key sent as one of a claim's own fields, which then stays on the
/// claim, is told so FIRST in the receipt, in words that state what it costs.**
///
/// Paid runs lost 31 writes to the same step: a key under `fields`, a receipt
/// that listed it among the keys that stayed 465 to 790 characters in, and a
/// later read or filter on the thing that found nothing. The sentence is in a
/// key that sorts before every other key, so it leads the serialized body, and
/// it names the key, the thing and the way out (`sets`).
///
/// **The negatives are what keep it worth reading.** A key sent in `sets`
/// reaches the thing and gets none, and neither does a key that only ever
/// labels a claim (`read_from`): "send it in sets" would be wrong for it.
#[tokio::test]
async fn an_own_key_that_stayed_is_told_first_and_a_setting_or_a_label_is_not() {
    let jojobot = handler();
    ensure(&jojobot, "person:alpha").await;
    const LEAD: &str = "a_key_stayed_off_the_thing";

    let stayed = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                fields: Some(
                    [("mood".to_string(), "delighted".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "the kiln was lit")
            }))
            .await
            .expect("capture ok"),
    );
    let said = stayed[LEAD].as_str().unwrap_or_else(|| panic!("{stayed}"));
    for word in ["mood", "person:alpha", "sets"] {
        assert!(
            said.contains(word),
            "the sentence does not name {word}: {said}"
        );
    }
    // **First by sort and first in the text a reader gets.**
    assert_eq!(
        stayed
            .as_object()
            .expect("an object")
            .keys()
            .next()
            .map(String::as_str),
        Some(LEAD),
        "{stayed}"
    );
    assert!(
        stayed.to_string().starts_with(&format!("{{\"{LEAD}\"")),
        "{stayed}"
    );
    // …and it lists the key as data too, as the receipt always did.
    assert_eq!(stayed["stayed_on_the_record"], serde_json::json!(["mood"]));

    // **A setting reaches the thing: nothing stayed, nothing is said.**
    let set = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: Some(
                    [("weather".to_string(), "clear".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "the weather was clear")
            }))
            .await
            .expect("capture ok"),
    );
    assert!(set.get(LEAD).is_none(), "{set}");

    // **A key that only labels a claim can never be set on a thing**, so the
    // sentence that says to send it in `sets` is not told for it.
    let labelled = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                provenance: Some("observation".into()),
                fields: Some(
                    [("read_from".to_string(), "the kiln log".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "the log says it was lit")
            }))
            .await
            .expect("capture ok"),
    );
    assert!(labelled.get(LEAD).is_none(), "{labelled}");
}

/// **A capture is answered with a receipt, not with the record.**
///
/// The content, the details and the fields are what the caller sent in
/// this call. **What the read-back proved is untouched**: the store is
/// still read before the write is called a success, so a claim that did
/// not survive storage is still an error rather than a success with
/// mangled bytes. The proof does not require shipping the proof.
///
/// **What must survive is everything the caller could not know**, and the
/// defaulted values are the sharp part: a caller that omitted `provenance`,
/// `standing` or `date` learns here what was recorded, and a defaulted
/// value that never reaches the receipt vanishes silently. The address is
/// the other half — without it the record cannot be edited, and a receipt
/// that costs a caller the address has broken the verb.
#[tokio::test]
async fn a_capture_is_receipted_without_reading_the_record_back() {
    let jojobot = handler();
    ensure(&jojobot, "alpha").await;
    let content = "said the kiln was finally lit, after three weeks of not being lit";

    let body = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                details: Some("and that the flue was the problem".into()),
                sets: Some(
                    [("mood".to_string(), "delighted".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", content)
            }))
            .await
            .expect("capture ok"),
    );

    assert_eq!(
        body["content"],
        serde_json::Value::Null,
        "the claim comes back to nobody who just wrote it: {body}"
    );
    assert_eq!(body["content_elided"], true, "{body}");
    assert_eq!(body["content_bytes"], content.len(), "{body}");
    assert_eq!(
        body["details"],
        serde_json::Value::Null,
        "…and the nuance beside it: {body}"
    );
    assert_eq!(
        body["sets_count"], 1,
        "how many keys landed, not which: {body}"
    );

    // The half a receipt may never cost: the address, and everything
    // jojobot decided for a caller that named none of it.
    assert_eq!(body["address"], "person:alpha#f1", "{body}");
    assert_eq!(body["subject"], "person:alpha", "{body}");
    assert_eq!(
        body["provenance"], "inference",
        "a caller that named no provenance learns what was recorded: {body}"
    );
    assert!(
        body["standing"].is_string() && body["status"].is_string(),
        "…and the standing this claim was given: {body}"
    );
    assert!(
        body["recorded_at"].is_string(),
        "…and the date it was stamped: {body}"
    );
    assert!(
        body["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("recall")),
        "eliding is never silent — the answer names the call that returns it: {body}"
    );
}

#[tokio::test]
async fn a_capture_carries_fields_with_no_label_and_reads_its_keys_back() {
    let jojobot = handler();
    ensure(&jojobot, "alpha").await;
    ensure(&jojobot, "milhouse").await;

    let body = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: Some(
                    [
                        ("mood".to_string(), "delighted".to_string()),
                        ("weather".to_string(), "clear".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                refs: Some(vec!["person:milhouse".into()]),
                ..capture_args("person:alpha", "the kiln was finally lit")
            }))
            .await
            .expect("capture ok"),
    );
    assert_ne!(body["status"], "blocked", "no label is asked for: {body}");
    // **Both keys were sent as settings, so the receipt counts them under `sets`**
    // and counts none as the record's own.
    assert_eq!(body["sets_count"], 2, "both keys landed: {body}");
    assert_eq!(body["fields_count"], 0, "neither is an own field: {body}");
    assert_eq!(body["refs"], serde_json::json!(["person:milhouse"]));

    // …and they are on the record a later reader takes, not only in the
    // answer to the write.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        recalled["objects"][0]["facts"][0]["sets"]["mood"], "delighted",
        "{recalled}"
    );
    assert!(
        recalled["objects"][0]["facts"][0]["fields"]["mood"].is_null(),
        "a setting is not also one of the record's own fields: {recalled}"
    );
    assert_eq!(
        recalled["objects"][0]["facts"][0]["refs"],
        serde_json::json!(["person:milhouse"]),
        "{recalled}"
    );
}

/// **A record with no fields carries an empty bag, not a missing key.**
///
/// Carrying none is the ordinary case, so a reader must learn it from the
/// answer rather than by branching on whether the key is there at all.
#[tokio::test]
async fn a_capture_with_no_fields_answers_with_an_empty_bag() {
    let jojobot = handler();
    let body = capture_ok(&jojobot, capture_args("person:alpha", "plays go")).await;
    assert_eq!(
        body["fields_count"], 0,
        "a reader learns there were none from the count, not by branching on a missing \
             key: {body}"
    );
    assert!(body.get("refs").is_none(), "{body}");
}

/// **A ref names an entity, so it must already exist.** The rule is not
/// about edges, it is about naming: nothing a write mentions is brought
/// into being as a side effect of mentioning it. A ref that provisioned its
/// own entity would make the open bag the one place on this surface where
/// that stopped being true — and the bag takes any key precisely so that
/// everything else about it stays strict.
#[tokio::test]
async fn a_ref_to_an_entity_nobody_created_is_blocked_and_writes_nothing() {
    let jojobot = handler();
    ensure(&jojobot, "alpha").await;

    let body = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                refs: Some(vec!["person:contract-ghost".into()]),
                ..capture_args("person:alpha", "it happened")
            }))
            .await
            .expect("an answer"),
    );
    assert_eq!(body["status"], "blocked", "{body}");
    assert_eq!(body["attempted"], "person:contract-ghost");
    assert_eq!(body["wrote"], false);

    // …and the fact did not land either: a record is one write, so a ref it
    // could not resolve takes the whole thing with it.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    assert!(
        recalled["objects"][0]["facts"]
            .as_array()
            .expect("a list")
            .is_empty(),
        "a blocked record wrote nothing: {recalled}"
    );
}

#[tokio::test]
async fn a_fact_can_be_about_any_kind() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        capture_args("place:daredevil-gorge", "swimmable in August"),
    )
    .await;
    assert_eq!(captured["subject"], "place:daredevil-gorge");
}

/// Capture's subject must exist, near miss or complete stranger, and the
/// way through is `add_entity` — never an override. The advice must say
/// `add_entity`: `override_token` is not a parameter on this verb, and
/// telling the caller to send one would offer a way out that does not
/// exist.
#[tokio::test]
async fn a_blocked_capture_says_to_add_the_entity_first() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
        .await
        .expect("add ok");

    let near = jojobot
        .capture(Parameters(capture_args("zenit", "should not land")))
        .await
        .expect("call ok");
    let body = blocked(&near);
    assert_eq!(body["candidates"][0]["handle"], "person:zenith");
    // The near-miss branch has its own copy, and it has to earn its keep: the
    // candidate list is the whole reason this case differs from a stranger,
    // so the advice must point at it rather than repeat the stranger's text.
    let advice = body["how_to_proceed"].as_str().expect("advice");
    assert!(
        advice.contains("above"),
        "with candidates in hand, the advice must point at them: {advice}"
    );
    assert!(
        advice.contains("add_entity"),
        "…and still name the way through: {advice}"
    );
    assert!(
        !advice.contains("nothing resembles it"),
        "something does resemble it — that is what the candidates are: {advice}"
    );
    assert!(
        !advice.contains("override_token"),
        "capture has no override_token, near miss or not: {advice}"
    );

    // A handle nothing resembles blocks too, with nothing to suggest.
    let stranger = jojobot
        .capture(Parameters(capture_args("work:first-mix", "32 tracks")))
        .await
        .expect("call ok");
    let body = blocked(&stranger);
    assert_eq!(body["attempted"], "work:first-mix");
    assert!(
        body["candidates"].as_array().unwrap().is_empty(),
        "got {body}"
    );
    let advice = body["how_to_proceed"].as_str().expect("advice");
    assert!(
        advice.contains("add_entity"),
        "must name the way through: {advice}"
    );
    assert!(
        !advice.contains("override_token"),
        "capture has no override_token; advising one offers a way out that does \
             not exist: {advice}"
    );
    assert!(
        !advice.contains("above"),
        "there are no candidates above to point at: {advice}"
    );

    // Two deliberate steps, and it lands.
    jojobot
        .add_entity(Parameters(add_args("work", "first-mix", "First Mix")))
        .await
        .expect("add ok");
    let landed = capture_ok(&jojobot, capture_args("work:first-mix", "32 tracks")).await;
    assert_eq!(landed["subject"], "work:first-mix");
}

/// Every edge spelling a caller receives can be sent back without translation.
#[tokio::test]
async fn every_edge_shape_returned_by_a_read_can_be_copied_to_capture() {
    let mut failures = Vec::new();
    for (shape, object) in [
        ("connection", "person:beta"),
        ("membership", "org:springfield-town-council"),
        ("attendance", "event:krusty-fest"),
        ("location", "place:daredevil-gorge"),
        ("about", "person:beta"),
    ] {
        let jojobot = handler_indexed();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some(shape.into()),
                object: Some(object.into()),
                ..capture_args("alpha", "edgeword")
            },
        )
        .await;
        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    history_record: Some(address_of(&captured)),
                    follow: Some(crate::memory::recall::FollowArgs {
                        shape: Some(shape.into()),
                        direction: None,
                        depth: None,
                        relation: None,
                        fits_type: None,
                        keeping: None,
                    }),
                    ..recall_args("alpha")
                }))
                .await
                .expect("the edge reads"),
        );
        let found = json_of(
            &jojobot
                .search(Parameters(SearchArgs {
                    query: Some("edgeword".into()),
                    include_links: Some(true),
                    ..search_args()
                }))
                .await
                .expect("the edge is searchable"),
        );
        let hit = found["results"]
            .as_array()
            .expect("search results")
            .iter()
            .find(|hit| hit["address"] == captured["address"])
            .unwrap_or_else(|| panic!("the captured record is a search hit: {found}"));
        let walked = &read["objects"][0]["connected"][0];
        let walked_edge = serde_json::json!({
            "type": walked["via"]["type"],
            "object": walked["id"],
        });
        let history_shape = &read["objects"][0]["record_history"]["writes"][0]["edge"]["type"];
        if history_shape != shape {
            failures.push(format!("{shape}/history served {history_shape}"));
        }
        for (source, edge) in [
            ("receipt", &captured["edge"]),
            ("recall", &read["objects"][0]["facts"][0]["edge"]),
            ("search", &hit["edge"]),
            ("walk", &walked_edge),
        ] {
            let shown = edge["type"].as_str().expect("an edge type");
            let target = edge["object"].as_str().expect("an edge object");
            let copied = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: Some(shown.into()),
                    object: Some(target.into()),
                    ..capture_args("alpha", &format!("copied from {source}"))
                }))
                .await;
            match copied {
                Ok(result) => {
                    let body = json_of(&result);
                    if body["status"] == "blocked" {
                        failures.push(format!("{shape}/{source} blocked: {body}"));
                    }
                    if body["edge"] != *edge {
                        failures.push(format!(
                            "{shape}/{source} returned a different edge: {body}"
                        ));
                    }
                }
                Err(error) => failures.push(format!("{shape}/{source}: {error}")),
            }
            if shown != shape {
                failures.push(format!("{shape}/{source} served {shown}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `capture` draws a typed edge, and the edge comes back on every read of the
/// fact — rendered with the writable shape token.
#[tokio::test]
async fn capture_draws_an_edge_and_renders_its_writable_shape() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            shape: Some("membership".into()),
            object: Some("org:springfield-town-council".into()),
            ..capture_args("alpha", "rides with the club")
        },
    )
    .await;
    assert_eq!(captured["edge"]["type"], "membership");
    assert_eq!(captured["edge"]["object"], "org:springfield-town-council");

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        recalled["objects"][0]["facts"][0]["edge"]["type"],
        "membership"
    );
}

/// The shape set is closed, and the response spellings are not input tokens —
/// the input grammar stays lowercase.
#[tokio::test]
async fn an_unknown_shape_is_a_client_error() {
    let jojobot = handler();
    for shape in ["knows", "memberOf", "Location", "attendee"] {
        let err = jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some(shape.into()),
                object: Some("place:daredevil-gorge".into()),
                ..capture_args("alpha", "an unknown shape")
            }))
            .await
            .expect_err("must reject shape {shape}");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {shape}");
        assert!(
            err.message.contains("location"),
            "the error must name the closed set: {}",
            err.message
        );
    }
}

/// A shape's object must be the kind it requires — a `location` pointing at a
/// person is a mis-drawn edge, and the caller hears about it.
#[tokio::test]
async fn a_wrong_kind_edge_object_is_a_client_error() {
    let err = handler()
        .capture(Parameters(CaptureArgs {
            shape: Some("location".into()),
            object: Some("person:beta".into()),
            ..capture_args("alpha", "in the wrong kind of place")
        }))
        .await
        .expect_err("a wrong-kind object must be refused");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.contains("place"),
        "must say what it wanted: {}",
        err.message
    );
}

/// A typo'd edge object comes back as the guard's candidates — the same
/// error-flagged response a blocked subject gets, and nothing is written.
#[tokio::test]
async fn a_blocked_edge_object_returns_candidates() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("place", "shelbyville", "Shelbyville")))
        .await
        .expect("add ok");
    // The subject faces the gate too, and the guard reports the first handle
    // it stops — this spec is about the object.
    ensure(&jojobot, "alpha").await;

    let result = jojobot
        .capture(Parameters(CaptureArgs {
            shape: Some("location".into()),
            object: Some("place:shelbyvlle".into()),
            ..capture_args("alpha", "should not land")
        }))
        .await
        .expect("the call succeeds; the guard answers in the body");
    let body = blocked(&result);
    assert_eq!(body["attempted"], "place:shelbyvlle");
    assert_eq!(body["candidates"][0]["handle"], "place:shelbyville");
    assert_eq!(body["candidates"][0]["type"], "Place");

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("alpha")))
            .await
            .expect("recall ok"),
    );
    assert!(
        recalled["objects"][0]["facts"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a blocked edge object must write no fact: {recalled}"
    );
}

/// The end-to-end MCP path: capture through the handler, then recall through
/// the handler, and the fact comes back.
#[tokio::test]
async fn capture_then_recall_through_the_handler() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "drinks oat milk")).await;
    assert_eq!(captured["subject"], "person:alpha");

    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args("alpha")))
            .await
            .expect("recall ok"),
    );
    assert_eq!(body["objects"][0]["id"], "person:alpha");
    let facts = body["objects"][0]["facts"]
        .as_array()
        .expect("recall returns a list");
    assert!(
        facts
            .iter()
            .any(|f| { f["address"] == captured["address"] && f["content"] == "drinks oat milk" }),
        "recall must return the captured fact: {body}"
    );
}

/// Omitting `provenance` defaults to inference (a hypothesis until confirmed).
#[tokio::test]
async fn provenance_defaults_to_inference() {
    let jojobot = handler();
    let captured = capture_ok(&jojobot, capture_args("alpha", "maybe a morning person")).await;
    assert_eq!(captured["provenance"], "inference");
}

/// **A `derived_from` naming no claim is blocked, with the addresses that
/// do exist.**
///
/// The store refuses it; this is the half that says a caller sees a
/// refusal it can act on rather than an error. The pair is here too,
/// because a blocked answer proves nothing on a build where the accepted
/// case never writes the link either.
#[tokio::test]
async fn a_derived_from_must_name_a_claim_that_exists() {
    let jojobot = handler();
    let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
    let address = source["address"].as_str().expect("an address").to_string();

    let linked = capture_ok(
        &jojobot,
        CaptureArgs {
            derived_from: Some(address.clone()),
            ..capture_args("alpha", "so the crossing is longer")
        },
    )
    .await;
    assert_eq!(
        linked["derived_from"], address,
        "a link to a claim that exists is written: {linked}"
    );

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                derived_from: Some("person:alpha#f99".into()),
                ..capture_args("alpha", "and the fare went up")
            }))
            .await
            .expect("a miss is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .is_some_and(|advice| advice.contains(&address)),
        "the addresses that DO exist are what makes it repairable: {refused}"
    );

    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall answers"),
    );
    assert_eq!(
        recalled["objects"][0]["facts"]
            .as_array()
            .expect("facts")
            .len(),
        2,
        "a refused capture wrote nothing: {recalled}"
    );
}

/// **Citing an archived claim as `derived_from` is permitted, and the
/// caller is told rather than left to notice on a later read.**
///
/// Archive is a visibility switch, not a validity gate: refusing the
/// citation would make archiving decide what may be worked from, which
/// is the thing this slice removed. The write must still go through, and
/// the postcondition — the one place a caller reads what a write did —
/// has to name the archived source, or the caller cannot tell without a
/// second call.
#[tokio::test]
async fn citing_an_archived_source_is_allowed_and_named_on_the_receipt() {
    let jojobot = handler();
    let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
    let address = source["address"].as_str().expect("an address").to_string();

    jojobot
        .retract(Parameters(crate::memory::retract::RetractArgs {
            request_id: None,
            address: address.clone(),
            reason: Some("misread the notice".into()),
            recorded_at: None,
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("retract answers");

    let linked = capture_ok(
        &jojobot,
        CaptureArgs {
            derived_from: Some(address.clone()),
            ..capture_args("alpha", "so the crossing is longer")
        },
    )
    .await;
    assert_eq!(
        linked["derived_from"], address,
        "a claim citing an archived source must still be written: {linked}"
    );
    assert!(
        linked["postcondition"]
            .as_str()
            .is_some_and(|note| note.contains(&address) && note.contains("archived")),
        "the receipt did not name the archived source the claim rests on: {linked}"
    );
}

/// **The `standing` argument reaches the store, and comes back.**
///
/// Nothing tested this. The domain contract builds `NewFact { standing }`
/// directly and never touches `parse_standing`; the argument builders only
/// ever sent `None`; and the story that exercises the field asserts with a
/// substring over a response holding two facts, so it cannot see the
/// argument dropped. Setting this verb's `standing` to `None` — the MCP
/// layer silently discarding what the caller asked for — left the entire
/// suite green.
///
/// The hedge is the case that matters: `testimony` with `open` is the one
/// pairing a default cannot produce, so it is the only one that proves the
/// argument travelled rather than being re-derived at the far end.
#[tokio::test]
async fn the_standing_argument_travels_and_reads_back() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            standing: Some("open".into()),
            ..capture_args("alpha", "thinks it shuts early")
        },
    )
    .await;
    // Paired: both halves, because `open` alone is what a default would
    // give an inference and `testimony` alone is what the provenance
    // argument already proves.
    assert_eq!(captured["provenance"], "testimony");
    assert_eq!(captured["standing"], "open");

    // …and it is on the page, not just in the answer.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall answers"),
    );
    assert_eq!(
        recalled["objects"][0]["facts"][0]["standing"], "open",
        "{recalled}"
    );
}

/// An unknown `standing` is a client error, not a silent default — a
/// caller who wrote something else meant something, and guessing which of
/// two values they meant is how a hedge becomes a settled fact.
#[tokio::test]
async fn an_unknown_standing_is_a_client_error() {
    let jojobot = handler();
    let refused = jojobot
        .capture(Parameters(CaptureArgs {
            standing: Some("maybe".into()),
            ..capture_args("alpha", "something")
        }))
        .await;
    assert!(refused.is_err(), "an unknown standing must be refused");
}

/// **Two runs in two zones disagree about what today is, and both are
/// right.**
///
/// The frame belongs to the caller, so a claim captured with no date is
/// stamped with the day it is in the run's own zone. The two zones here are
/// the extremes on purpose: twenty-six hours apart, so their local dates
/// differ at every instant and this case does not pass or fail by the hour
/// it is run at.
///
/// **Each date is pinned to its own zone, not merely to being different.**
/// A case asserting only that two answers differ passes on a build that
/// stamps them wrong in two directions.
#[tokio::test]
async fn two_runs_in_two_zones_stamp_a_claim_with_their_own_day() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    ensure(&jojobot, "person:milhouse").await;

    // Twenty-six hours apart: the widest the map goes, so the two local
    // dates can never coincide.
    // Both answer `new`: the fixture handle already has a run in flight,
    // and a bot may have several at once — which is what lets one case hold
    // two of them in two zones.
    let behind = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
    let ahead = booted_in(&jojobot, "otto", "Pacific/Kiritimati", Some("new")).await;

    let stamped = async |sid: &str| {
        let mut args = capture_args("milhouse", "no date on this one");
        args.sid = Some(sid.to_string());
        args.recorded_at = None;
        capture_ok(&jojobot, args).await["recorded_at"]
            .as_str()
            .expect("a capture is stamped with a day")
            .to_string()
    };
    let (behind, ahead) = (stamped(&behind).await, stamped(&ahead).await);

    let day_in = |zone: &str| {
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone"))
            .date()
            .to_string()
    };
    assert_eq!(behind, day_in("Etc/GMT+12"), "the run west of everything");
    assert_eq!(
        ahead,
        day_in("Pacific/Kiritimati"),
        "and the run east of it"
    );
    assert_ne!(
        behind, ahead,
        "…which are never the same day, whatever hour this runs at",
    );
}

/// 🚨 **A run that stated its day writes under that day, not under the
/// server's.**
///
/// The door takes the day a run is in, and the sweep and the beats already
/// read it. A CLAIM did not: a session acting out March made every write
/// under the day the run actually happened, and the prose it wrote was
/// perfectly in period, so nothing in the store said the date was wrong.
///
/// ⚠️ **Three halves, and each alone passes on a build nobody wants.** A
/// run that stated no day must still get today, or the frame becomes a
/// requirement rather than an option. And a write naming its own date must
/// still win, or a run acting out a period can no longer record a claim
/// about any other day — which is most of what such a run is for.
#[tokio::test]
async fn a_run_that_stated_its_day_writes_under_it() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    ensure(&jojobot, "person:milhouse").await;
    // **Answering `new`**, because the fixture handle already has a run of
    // this bot in flight and a boot meeting one hands back a choice rather
    // than a handle.
    let booted = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-03-15".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("the boot call is ok"),
    );
    let acting = sid_of(&booted).unwrap_or_else(|| panic!("a boot that states a day: {booted}"));

    let mut args = capture_args("milhouse", "went to the fair");
    args.sid = Some(acting.clone());
    args.recorded_at = None;
    let stated = capture_ok(&jojobot, args).await;
    assert_eq!(
        stated["recorded_at"], "2026-03-15",
        "the claim was stamped with the server's day, not the run's: {stated}"
    );

    // **A write naming its own date still wins.** A run acting out a period
    // records claims about other days, and this is how.
    let mut args = capture_args("milhouse", "had been at the fair the day before");
    args.sid = Some(acting.clone());
    args.recorded_at = Some("2026-03-14".into());
    let named = capture_ok(&jojobot, args).await;
    assert_eq!(named["recorded_at"], "2026-03-14");

    // **A second run states another day, and the first one's frame does not
    // reach it.** Two runs of one bot are legitimately in two periods, and
    // the handle is what tells them apart.
    let elsewhere = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-07-04".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("the boot call is ok"),
    );
    let mut args = capture_args("milhouse", "was at the parade");
    args.sid = sid_of(&elsewhere);
    args.recorded_at = None;
    let second = capture_ok(&jojobot, args).await;
    assert_eq!(
        second["recorded_at"], "2026-07-04",
        "the second run wrote under the first one's day: {second}"
    );

    // ⚠️ **A run that stated no day still gets today**, on the clock in its
    // own zone. Without this the frame stops being optional.
    let now = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
    let mut args = capture_args("milhouse", "happening now");
    args.sid = Some(now);
    args.recorded_at = None;
    let clocked = capture_ok(&jojobot, args).await;
    assert_eq!(
        clocked["recorded_at"],
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::get("Etc/GMT+12").expect("a zone"))
            .date()
            .to_string(),
        "a run that stated no day was answered with somebody else's frame: {clocked}"
    );
}

/// **The receipt says when an explicit `recorded_at` differs from the
/// run's own stated day, and stays silent on every other shape** —
/// omitted, matching, or a run with no stated day to differ from. Four
/// cases in one, so the one that fires is read against the three that do
/// not rather than trusted alone.
#[tokio::test]
async fn the_receipt_names_a_recorded_at_that_differs_from_the_runs_day() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    ensure(&jojobot, "person:milhouse").await;

    let booted = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-03-15".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("the boot call is ok"),
    );
    let acting = sid_of(&booted).expect("a handle");

    // A different day: the sentence names both.
    let mut args = capture_args("milhouse", "stated on a different day");
    args.sid = Some(acting.clone());
    args.recorded_at = Some("2026-03-01".into());
    let differing = capture_ok(&jojobot, args).await;
    let note = differing["recorded_at_note"]
        .as_str()
        .unwrap_or_else(|| panic!("a sentence naming both days: {differing}"));
    assert!(
        note.contains("2026-03-01") && note.contains("2026-03-15"),
        "{note}"
    );

    // The same day as the run: silent.
    let mut args = capture_args("milhouse", "stated on the run's own day");
    args.sid = Some(acting.clone());
    args.recorded_at = Some("2026-03-15".into());
    let same = capture_ok(&jojobot, args).await;
    assert!(same["recorded_at_note"].is_null(), "{same}");

    // No explicit day at all: silent.
    let mut args = capture_args("milhouse", "no day named at all");
    args.sid = Some(acting.clone());
    args.recorded_at = None;
    let omitted = capture_ok(&jojobot, args).await;
    assert!(omitted["recorded_at_note"].is_null(), "{omitted}");

    // A run with no stated day: silent, even naming a day that would
    // otherwise disagree with the server's own clock.
    let undated = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
    let mut args = capture_args("milhouse", "a run with no stated day");
    args.sid = Some(undated);
    args.recorded_at = Some("2026-01-01".into());
    let no_stated_day = capture_ok(&jojobot, args).await;
    assert!(
        no_stated_day["recorded_at_note"].is_null(),
        "{no_stated_day}"
    );
}

/// **A run that named no zone is answered in UTC**, which is the stated
/// fallback rather than a server setting.
///
/// The negative the case above rests on: without it, a build that always
/// used the fallback and a build that reads the run's zone are told apart
/// by nothing here.
#[tokio::test]
async fn a_run_with_no_zone_is_dated_in_the_fallback() {
    let jojobot = handler();
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date();
    let captured = capture_ok(&jojobot, capture_args("alpha", "dated today")).await;
    assert_eq!(captured["recorded_at"], today.to_string());
}

/// An explicit testimony provenance is honoured.
#[tokio::test]
async fn explicit_testimony_is_honoured() {
    let jojobot = handler();
    let captured = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            recorded_at: Some("2026-01-01".into()),
            ..capture_args("alpha", "speaks two languages")
        },
    )
    .await;
    assert_eq!(captured["provenance"], "testimony");
    assert_eq!(captured["recorded_at"], "2026-01-01");
}

#[tokio::test]
async fn unknown_provenance_is_a_client_error() {
    let err = handler()
        .capture(Parameters(CaptureArgs {
            provenance: Some("maybe".into()),
            ..capture_args("alpha", "x")
        }))
        .await
        .expect_err("must reject unknown provenance");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
}

#[tokio::test]
async fn malformed_date_is_a_client_error() {
    let err = handler()
        .capture(Parameters(CaptureArgs {
            recorded_at: Some("not-a-date".into()),
            ..capture_args("alpha", "x")
        }))
        .await
        .expect_err("must reject a malformed date");
    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
}

/// Empty content is a caller mistake, so it comes back as a blocked
/// answer with a way forward rather than as a protocol error (rule 68).
#[tokio::test]
async fn empty_content_is_a_blocked_answer() {
    let body = blocked(
        &handler()
            .capture(Parameters(capture_args("alpha", "   ")))
            .await
            .expect("a caller mistake is an answer, not a protocol failure"),
    );
    assert_eq!(body["wrote"], false, "{body}");
    let said = jojobot_domain::memory::validate_content("   ")
        .expect_err("empty content is refused")
        .to_string();
    assert!(
        body["how_to_proceed"]
            .as_str()
            .is_some_and(|advice| advice.contains(&said)),
        "the refusal names the fault: {body}"
    );
}

/// **A line break in `content` is refused, and the refusal and the argument's
/// own schema both say where the rest goes.** `details` holds paragraph
/// breaks; the claim stays one line.
#[tokio::test]
async fn a_multi_line_claim_is_told_where_the_rest_goes() {
    let body = blocked(
        &handler()
            .capture(Parameters(capture_args("alpha", "the claim\nand the rest")))
            .await
            .expect("a caller mistake is an answer, not a protocol failure"),
    );
    assert!(
        body["how_to_proceed"]
            .as_str()
            .is_some_and(|advice| advice.contains("details")),
        "the refusal names details: {body}"
    );
    let properties = crate::teaching::published_arguments("capture").expect("capture is a tool");
    let content = properties["content"]["description"]
        .as_str()
        .expect("content is described");
    assert!(content.contains("details"), "{content}");
}

/// **The write says it landed, never that it failed, when only the fold
/// behind it could not confirm it** (rule 130) — `capture`'s own catch of
/// `MemoryError::FoldBehind`, proven through the served verb rather than
/// by calling `Folded` directly.
#[tokio::test]
async fn a_capture_whose_fold_could_not_refresh_answers_landed_not_failed() {
    let jojobot = Jojobot::new(
        Arc::new(FoldBehindMemory(Arc::new(InMemoryMemory::booted()))),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );

    let receipt = capture_ok(
        &jojobot,
        capture_args("person:alpha", "said the kiln was lit"),
    )
    .await;

    assert_eq!(receipt["fold"]["behind"], "stale", "{receipt}");
    assert!(
        receipt["fold"]["note"]
            .as_str()
            .is_some_and(|note| note.contains("landed")),
        "the note has to say the write landed, not that it failed: {receipt}"
    );

    // **The positive half.** The receipt still carries the record itself,
    // exactly as an ordinary capture does — a `FoldBehind` error is never
    // allowed to swallow what the write actually produced.
    assert_eq!(receipt["subject"], "person:alpha", "{receipt}");
    assert_ne!(receipt["status"], "blocked", "{receipt}");
    // **What it changed could not be read**, said as such and not as `nothing`.
    assert_eq!(receipt["changed"], "not_read", "{receipt}");
}

/// **Ageing frees a room's slot through `capture`'s own served path** —
/// the one seam where `Memory` and `Sessions` meet, so this is the one
/// case that has to go through both real ports rather than calling the
/// domain directly. A bot that has run `AGES_AFTER_RUNS` times ages an
/// old thought out of its own room: a write that a plain capacity count
/// would refuse instead lands with nothing dropped, and a write that
/// would still be refused after that says how many aged thoughts it is
/// not counting.
#[tokio::test]
async fn an_aged_thought_frees_the_room_and_the_refusal_still_counts_it() {
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        sessions.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let bot = "bot:mcp-thought-aging";
    ensure(&jojobot, bot).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;

    let old = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:the-couch".into()),
            ..capture_args(bot, "the couch needs a leg fixed")
        },
    )
    .await;
    let old_address = address_of(&old);

    // **Twenty runs, all begun after the old thought's own write** —
    // enough for the bot to have run `AGES_AFTER_RUNS` times, which is
    // what makes the question askable at all. Seeded straight onto the
    // real `Sessions` double rather than through a boot, because the
    // question here is what `capture` does with the answer, not how a
    // run begins.
    for n in 0..20 {
        sessions
            .begin(jojobot_domain::session::NewSession {
                bot: EntityId(bot.to_string()),
                sid: jojobot_domain::session::Sid(format!("ar{n:02}")),
                focus: "working".into(),
                started_at: jiff::Timestamp::now(),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("seeding a run");
    }

    // The room nominally holds one, capacity is one — a plain count
    // would refuse this. The old thought has aged out, so it lands.
    let fresh = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:the-fern".into()),
            ..capture_args(bot, "the fern needs water")
        },
    )
    .await;
    assert_ne!(
        fresh["status"], "blocked",
        "the old thought aged out of a room at capacity one, so a fresh one must land \
             with nothing dropped: {fresh}"
    );

    // The room now nominally holds two — the aged one and the fresh
    // one — and capacity is still one, so a third is refused. The
    // refusal must name how many aged thoughts it is not counting,
    // rather than folding them silently into `live`.
    ensure(&jojobot, "thing:the-air-filter").await;
    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
                shape: Some("connection".into()),
                object: Some("thing:the-air-filter".into()),
                ..capture_args(bot, "the air filter is due")
            }))
            .await
            .expect("capture answers rather than failing the protocol"),
    );
    assert_eq!(
        refused["aged_out"], 1,
        "the refusal must say one thought aged out of the count: {refused}"
    );
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    let attempted = refused["attempted"]
        .as_str()
        .expect("a blocked answer names what it attempted");
    assert!(
        how.contains(&format!("'{attempted}'s room")),
        "the sentence must name the room with the SAME value 'attempted' carries, not a \
             prefixed variant of it: {how}"
    );

    // **Not silently dropped.** The aged thought is still there, still
    // active, exactly as it stood — findable by its own address.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args(bot)))
            .await
            .expect("recall ok"),
    );
    let old_fact = recalled["objects"][0]["facts"]
        .as_array()
        .expect("facts asked for")
        .iter()
        .find(|f| f["address"] == old_address)
        .unwrap_or_else(|| panic!("the aged thought must still be readable: {recalled}"));
    assert_eq!(
        old_fact["status"], "active",
        "an aged thought is excluded from the count, never archived: {old_fact}"
    );
}

/// **A bot's room, one slot big, holding one old thought**, and the sessions
/// double its runs are seeded onto. `threshold` is written onto the bot as
/// its ageing setting by a different identity, or left off.
async fn a_full_room_of_one_old_thought(
    threshold: Option<&str>,
) -> (
    Jojobot,
    Arc<jojobot_domain::session::testing::InMemorySessions>,
    &'static str,
    String,
) {
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        sessions.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let bot = "bot:mcp-thought-aging";
    ensure(&jojobot, bot).await;
    let mut fields: std::collections::BTreeMap<String, String> = [(
        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
        "1".to_string(),
    )]
    .into_iter()
    .collect();
    if let Some(threshold) = threshold {
        // The spelling is pinned here as a literal: it is stored on the bot,
        // and nothing outside this process declares it.
        fields.insert("thought_ages_after_runs".to_string(), threshold.to_string());
    }
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(fields),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;
    let old = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:the-couch".into()),
            ..capture_args(bot, "the couch needs a leg fixed")
        },
    )
    .await;
    (jojobot, sessions, bot, address_of(&old))
}

/// Seed `count` runs of `bot`, begun after everything captured so far.
async fn seed_runs(
    sessions: &jojobot_domain::session::testing::InMemorySessions,
    bot: &str,
    first: usize,
    count: usize,
) {
    for n in first..first + count {
        sessions
            .begin(jojobot_domain::session::NewSession {
                bot: EntityId(bot.to_string()),
                sid: jojobot_domain::session::Sid(format!("as{n:02}")),
                focus: "working".into(),
                started_at: jiff::Timestamp::now(),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("seeding a run");
    }
}

/// A second thought into the room `a_full_room_of_one_old_thought` made.
async fn a_second_thought(jojobot: &Jojobot, bot: &str) -> serde_json::Value {
    ensure(jojobot, "thing:the-fern").await;
    json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(crate::harness::writing_as_bot(jojobot, bot)),
                shape: Some("connection".into()),
                object: Some("thing:the-fern".into()),
                ..capture_args(bot, "the fern needs water")
            }))
            .await
            .expect("capture answers rather than failing the protocol"),
    )
}

/// **A bot that carries an ageing setting ages its thoughts on that value.**
/// Two runs are far below the default of twenty, so a build that ignores the
/// field refuses the second thought. Paired with
/// [`a_bot_with_no_ageing_setting_ages_on_twenty_runs_and_not_before`].
#[tokio::test]
async fn a_bot_with_an_ageing_setting_ages_a_thought_on_that_value() {
    let (jojobot, sessions, bot, old_address) = a_full_room_of_one_old_thought(Some("2")).await;
    seed_runs(&sessions, bot, 0, 2).await;

    let landed = a_second_thought(&jojobot, bot).await;
    assert_ne!(
        landed["status"], "blocked",
        "two runs age the old thought when the bot's own setting is two: {landed}"
    );
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args(bot)))
            .await
            .expect("recall ok"),
    );
    let old_fact = recalled["objects"][0]["facts"]
        .as_array()
        .expect("facts asked for")
        .iter()
        .find(|f| f["address"] == old_address)
        .unwrap_or_else(|| panic!("an aged thought is still readable: {recalled}"));
    assert_eq!(
        old_fact["status"], "active",
        "ageing leaves the thought active, never archived: {old_fact}"
    );
}

/// **A bot with no setting ages on twenty runs, and not on nineteen.** The
/// default is exactly the old constant: one run short of it refuses, the run
/// that reaches it lets the write land.
#[tokio::test]
async fn a_bot_with_no_ageing_setting_ages_on_twenty_runs_and_not_before() {
    let (jojobot, sessions, bot, old_address) = a_full_room_of_one_old_thought(None).await;
    // The bot's own run, which wrote the old thought, is the first of the twenty.
    seed_runs(&sessions, bot, 0, 18).await;

    let refused = a_second_thought(&jojobot, bot).await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(
        refused["aged_out"], 0,
        "nineteen runs age nothing without a setting: {refused}"
    );
    // The refusal says where the threshold behind `aged_out` comes from.
    assert_eq!(
        refused["ageing"]["setting"], "thought_ages_after_runs",
        "{refused}"
    );
    assert_eq!(refused["ageing"]["default"], 20, "{refused}");

    seed_runs(&sessions, bot, 18, 1).await;
    let landed = a_second_thought(&jojobot, bot).await;
    assert_ne!(
        landed["status"], "blocked",
        "the twentieth run ages the old thought: {landed}"
    );
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args(bot)))
            .await
            .expect("recall ok"),
    );
    assert!(
        recalled["objects"][0]["facts"]
            .as_array()
            .expect("facts asked for")
            .iter()
            .any(|f| f["address"] == old_address && f["status"] == "active"),
        "nothing that ages is archived or deleted: {recalled}"
    );
}

/// **The thing a setting binds cannot write that setting.** A bot capturing
/// its ageing setting about its own handle is refused, and nothing is left
/// behind. Paired with
/// [`a_different_bot_can_set_this_bots_ageing_setting`].
#[tokio::test]
async fn a_bot_cannot_set_its_own_ageing_setting() {
    let jojobot = handler();
    ensure(&jojobot, "bot:otto").await;

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: Some(
                    [("thought_ages_after_runs".to_string(), "1".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:otto", "shortening my own ageing")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");

    let after = fields_of(&jojobot, "bot:otto").await;
    assert!(
        after.get("thought_ages_after_runs").is_none(),
        "the refused write must not be readable back: {after}"
    );
}

/// **The positive half.** A different identity setting the same key on a bot
/// lands, so the guard is about who asks and not about the key.
#[tokio::test]
async fn a_different_bot_can_set_this_bots_ageing_setting() {
    let jojobot = handler();
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("thought_ages_after_runs".to_string(), "7".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("bot:milhouse", "ageing is seven runs")
        },
    )
    .await;
    assert_ne!(landed["status"], "blocked", "{landed}");

    let after = fields_of(&jojobot, "bot:milhouse").await;
    assert_eq!(
        after["thought_ages_after_runs"], "7",
        "a different identity's write must land: {after}"
    );
}

/// **The thing a ceiling binds cannot write that ceiling.** A bot
/// capturing `thought_capacity` about its own handle is refused,
/// naming why and who has to do it instead — never a state the store
/// is left holding.
#[tokio::test]
async fn a_bot_cannot_raise_its_own_thought_capacity() {
    let jojobot = handler();
    // `capture_args`'s own sid resolves to bot:otto — so this is
    // bot:otto capturing about bot:otto, deliberately.
    ensure(&jojobot, "bot:otto").await;

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "5".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("bot:otto", "raising my own ceiling")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        how.contains("different identity"),
        "the refusal must say who has to do it instead: {how}"
    );

    let after = fields_of(&jojobot, "bot:otto").await;
    assert!(
        after
            .get(jojobot_domain::memory::THOUGHT_CAPACITY)
            .is_none(),
        "the refused write must not be readable back: {after}"
    );
}

/// **A renamed bot cannot raise its own ceiling through the handle it used to
/// wear.** The session is rebound to the bot's new handle on a rename, and the
/// capture's subject is whatever was typed, so comparing the two as typed let
/// the old handle through. Paired with the same capture on a different bot
/// through a handle it used to wear, which lands.
#[tokio::test]
async fn a_renamed_bot_cannot_raise_its_own_ceiling_through_its_former_handle() {
    let jojobot = handler();
    // `capture_args`'s own sid resolves to bot:otto.
    ensure(&jojobot, "bot:otto").await;
    ensure(&jojobot, "bot:milhouse").await;
    for (from, to) in [("bot:otto", "bot:delta"), ("bot:milhouse", "bot:sigma")] {
        let renamed = json_of(
            &jojobot
                .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                    request_id: None,
                    handle: from.into(),
                    to: to.into(),
                    parent: None,
                    recorded_at: None,
                    override_token: None,
                    override_reason: None,
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("rename ok"),
        );
        assert_ne!(renamed["status"], "blocked", "{renamed}");
    }
    let ceiling = || {
        Some(
            [(
                jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                "5".to_string(),
            )]
            .into_iter()
            .collect(),
        )
    };

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: ceiling(),
                ..capture_args("bot:otto", "raising my own ceiling by the old name")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    let after = fields_of(&jojobot, "bot:delta").await;
    assert!(
        after
            .get(jojobot_domain::memory::THOUGHT_CAPACITY)
            .is_none(),
        "the refused write must not be readable back: {after}"
    );

    // The positive half: another bot's ceiling, named by a handle that bot
    // used to wear, is somebody else's to set and lands.
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: ceiling(),
            ..capture_args("bot:milhouse", "capacity is five")
        },
    )
    .await;
    assert_ne!(landed["status"], "blocked", "{landed}");
    let other = fields_of(&jojobot, "bot:sigma").await;
    assert_eq!(
        other[jojobot_domain::memory::THOUGHT_CAPACITY],
        "5",
        "a different identity's write must land: {other}"
    );
}

/// **The positive half of [`a_bot_cannot_raise_its_own_thought_capacity`].**
/// A DIFFERENT identity setting the same key, on the same kind of
/// subject, lands — proving the guard is about who is asking, never
/// about the key or the kind.
#[tokio::test]
async fn a_different_bot_can_raise_this_bots_thought_capacity() {
    let jojobot = handler();
    // bot:otto (this harness's default caller) sets a capacity on a
    // DIFFERENT bot's own handle.
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "5".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args("bot:milhouse", "capacity is five")
        },
    )
    .await;
    assert_ne!(landed["status"], "blocked", "{landed}");

    let after = fields_of(&jojobot, "bot:milhouse").await;
    assert_eq!(
        after[jojobot_domain::memory::THOUGHT_CAPACITY],
        "5",
        "a different identity's write must land: {after}"
    );
}

/// **A role object's own two fields are the boot door's, never an ordinary
/// capture's — whoever the caller is.** Unlike `thought_capacity`'s guard,
/// this one does not turn on who is asking: nobody may open this side door,
/// because the claim path and the renewal path are the only legitimate
/// writers and neither goes through this verb. The guard is scoped to the
/// role object: the `role/<role>/holder` key an older build kept on a bot is an
/// ordinary field now, and nothing reads it as a lease.
#[tokio::test]
async fn a_role_holder_field_cannot_be_set_through_an_ordinary_capture() {
    let jojobot = handler();
    ensure(&jojobot, "bot:otto").await;
    ensure(&jojobot, "role:dev-dispatch").await;

    for key in ["holder", "claimed_at"] {
        let refused = blocked(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sets: Some(
                        [(key.to_string(), "bot:otto".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("role:dev-dispatch", "taking the role directly")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked", "{key}: {refused}");
        assert_eq!(refused["wrote"], false, "{key}: {refused}");
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            how.contains("start_here"),
            "the refusal must name the boot door as the way forward: {how}"
        );
    }
    let after = fields_of(&jojobot, "role:dev-dispatch").await;
    assert!(
        after.get("holder").is_none() && after.get("claimed_at").is_none(),
        "the refused writes must not be readable back: {after}"
    );

    let old_key = jojobot_domain::session::role_keys::role_holder_key("dev-dispatch");
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(old_key.clone(), "bot:otto".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("bot:otto", "an old key on a bot is an ordinary field")
        },
    )
    .await;
    assert_ne!(landed["status"], "blocked", "{landed}");
}

/// **The emergency reserve, spent through the served verb — and the
/// receipt says so.** A refusal at exactly capacity offers `borrow`; a
/// refusal once the debt is outstanding does not offer it again — two
/// different sentences for two different states, not one refusal reused.
#[tokio::test]
async fn a_borrow_lands_visibly_and_a_second_one_is_refused() {
    let jojobot = handler();
    let bot = "bot:mcp-thought-borrow";
    ensure(&jojobot, bot).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:jukebox".into()),
            ..capture_args(bot, "the jukebox needs a needle")
        },
    )
    .await;

    // At exactly capacity, with no borrow: refused, and the refusal
    // offers borrowing as a way forward.
    ensure(&jojobot, "thing:battery").await;
    let refused_at_capacity = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
                shape: Some("connection".into()),
                object: Some("thing:battery".into()),
                ..capture_args(bot, "the battery needs replacing")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(
        refused_at_capacity["status"], "blocked",
        "{refused_at_capacity}"
    );
    let how_at_capacity = refused_at_capacity["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        how_at_capacity.contains("borrow"),
        "at exactly capacity, borrowing must be offered as a way forward: \
             {how_at_capacity}"
    );

    // Borrowing lands — over the ceiling, visibly, in this receipt.
    let borrowed = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:battery".into()),
            borrow: Some(true),
            ..capture_args(bot, "the battery needs replacing")
        },
    )
    .await;
    let receipt = borrowed["postcondition"]
        .as_str()
        .expect("a capture answers with a postcondition");
    assert!(
        receipt.contains("2 of 1"),
        "the receipt must say the room is now over its own capacity, in numbers: {receipt}"
    );

    let after = fields_of(&jojobot, bot).await;
    // The write landed as an ordinary claim — nothing about the bound
    // thing's own capacity field moved.
    assert_eq!(
        after[jojobot_domain::memory::THOUGHT_CAPACITY],
        "1",
        "borrowing does not raise the ceiling itself: {after}"
    );

    // The debt is outstanding — a second borrow is refused, and this
    // refusal does NOT invite borrowing again.
    ensure(&jojobot, "thing:the-fern").await;
    let refused_over_capacity = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
                shape: Some("connection".into()),
                object: Some("thing:the-fern".into()),
                borrow: Some(true),
                ..capture_args(bot, "the fern needs water")
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(
        refused_over_capacity["status"], "blocked",
        "{refused_over_capacity}"
    );
    let how_over_capacity = refused_over_capacity["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        !how_over_capacity.contains("borrow: true"),
        "once the debt is outstanding, the refusal must not invite spending the reserve \
             again: {how_over_capacity}"
    );
}

/// 🚨 **A borrow whose receipt cannot read the bot's runs still says the room is
/// over its ceiling.** The write has landed by then and cannot be taken back, so
/// the receipt does not decline. It counts the room without ageing and says the
/// runs could not be read, so a reader knows a quiet thought may be counted live.
///
/// Paired with the same borrow when the runs read cleanly: the receipt carries the
/// numbers and no such sentence. The store answers the one read the write itself
/// makes and refuses the next.
#[tokio::test]
async fn a_borrow_receipt_keeps_the_reserve_notice_when_the_runs_cannot_be_read() {
    let counting = Arc::new(crate::session::testing::CountingSessions::new(
        jojobot_domain::session::testing::InMemorySessions::new(),
    ));
    let (jojobot, _memory) =
        crate::session::testing::with_sessions_port_and_memory(counting.clone());
    let bot = "bot:mcp-thought-borrow";
    ensure(&jojobot, bot).await;
    let bot_sid = crate::harness::booted(&jojobot, bot).await;
    ensure(&jojobot, "thing:jukebox").await;
    ensure(&jojobot, "thing:battery").await;
    ensure(&jojobot, "thing:the-fern").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(bot_sid.clone()),
            shape: Some("connection".into()),
            object: Some("thing:jukebox".into()),
            ..capture_args(bot, "the jukebox needs a needle")
        },
    )
    .await;
    let borrow = |object: &str, content: &str| CaptureArgs {
        sid: Some(bot_sid.clone()),
        shape: Some("connection".into()),
        object: Some(object.into()),
        borrow: Some(true),
        ..capture_args(bot, content)
    };

    let clean = capture_ok(
        &jojobot,
        borrow("thing:battery", "the battery needs replacing"),
    )
    .await;
    let clean_receipt = clean["postcondition"].as_str().expect("a postcondition");
    assert!(clean_receipt.contains("2 of 1"), "{clean_receipt}");
    assert!(
        !clean_receipt.contains("could not be read"),
        "a readable store was reported unreadable: {clean_receipt}"
    );

    // A fresh bot in the same state, so the second borrow is the first on its room.
    let other = "bot:mcp-seats-update";
    ensure(&jojobot, other).await;
    let other_sid = crate::harness::booted(&jojobot, other).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(other, "capacity is one")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(other_sid.clone()),
            shape: Some("connection".into()),
            object: Some("thing:jukebox".into()),
            ..capture_args(other, "the jukebox needs a needle")
        },
    )
    .await;
    counting.refuse_runs_after(1);
    let unread = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(other_sid.clone()),
            shape: Some("connection".into()),
            object: Some("thing:the-fern".into()),
            borrow: Some(true),
            ..capture_args(other, "the fern needs water")
        },
    )
    .await;
    let unread_receipt = unread["postcondition"].as_str().expect("a postcondition");
    assert!(
        unread_receipt.contains("2 of 1"),
        "the reserve notice vanished with the runs: {unread_receipt}"
    );
    assert!(
        unread_receipt.contains("could not be read"),
        "the notice did not say it was counted without the runs: {unread_receipt}"
    );
}

/// **A thought over its container's body cap is refused through the
/// served surface**, and the way forward names the repair the brief
/// itself describes — move the substance onto what the thought points
/// at, and keep the thought itself short.
///
/// The default cap binds a container that carries a thought capacity, so
/// this one is given one. Paired with a bot that carries none, which lands
/// the same over-cap claim: without that half this passes on a build that
/// caps every thought.
#[tokio::test]
async fn a_thought_over_the_body_cap_is_refused_through_the_served_surface() {
    let jojobot = handler();
    let bot = "bot:mcp-thought-body-cap";
    ensure(&jojobot, bot).await;
    ensure(&jojobot, "thing:jukebox").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "10".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is ten")
        },
    )
    .await;

    let over_cap = "x".repeat(jojobot_domain::memory::DEFAULT_THOUGHT_BODY_CAP + 1);
    let refused = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
                shape: Some("connection".into()),
                object: Some("thing:jukebox".into()),
                ..capture_args(bot, &over_cap)
            }))
            .await
            .expect("a refusal is an answer, not a failure"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    assert!(
        how.contains("Re-call capture") && how.contains("substance"),
        "the way forward must tell the caller to re-call capture with the substance moved \
             off the thought: {how}"
    );
    assert!(
        how.contains("deliberate"),
        "the refusal says the cap is on purpose: {how}"
    );

    // At exactly the cap, it lands.
    let at_cap = "x".repeat(jojobot_domain::memory::DEFAULT_THOUGHT_BODY_CAP);
    ensure(&jojobot, "thing:battery").await;
    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, bot)),
            shape: Some("connection".into()),
            object: Some("thing:battery".into()),
            ..capture_args(bot, &at_cap)
        },
    )
    .await;
    assert_ne!(landed["status"], "blocked", "{landed}");

    // A bot that carries no capacity is not bound by the default cap.
    ensure(&jojobot, "bot:milhouse").await;
    let unbound = capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(crate::harness::writing_as_bot(&jojobot, "bot:milhouse")),
            shape: Some("connection".into()),
            object: Some("thing:jukebox".into()),
            ..capture_args("bot:milhouse", &over_cap)
        },
    )
    .await;
    assert_ne!(unbound["status"], "blocked", "{unbound}");
}

/// **Starring a rule into full seats is refused, naming the seated rules
/// (rule 591).** Five captures fill the seats exactly and name nothing; a
/// sixth, starred too, is refused with nothing written, and the refusal names
/// the five, the oldest among them.
#[tokio::test]
async fn a_capture_starring_a_rule_into_full_seats_is_refused_naming_the_seated() {
    let jojobot = handler();
    let bot = "bot:mcp-seats-capture";
    ensure(&jojobot, bot).await;

    let mut oldest = String::new();
    for n in 0..jojobot_domain::text::CARRIED_RULES {
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(bot, &format!("starred rule {n}"))
            },
        )
        .await;
        assert_eq!(
            receipt["seats"],
            serde_json::Value::Null,
            "filling a seat, not overflowing one, must name nothing: {receipt}"
        );
        if n == 0 {
            oldest = receipt["address"]
                .as_str()
                .expect("a capture receipt carries its own address")
                .to_string();
        }
    }

    let sixth = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sets: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(bot, "the sixth starred rule")
            }))
            .await
            .expect("capture answers"),
    );
    assert_eq!(sixth["status"], "blocked", "{sixth}");
    assert_eq!(sixth["wrote"], false, "{sixth}");
    let seated: Vec<&str> = sixth["seated"]
        .as_array()
        .unwrap_or_else(|| panic!("the refusal names the seated rules: {sixth}"))
        .iter()
        .filter_map(|rule| rule["address"].as_str())
        .collect();
    assert_eq!(seated.len(), jojobot_domain::text::CARRIED_RULES, "{sixth}");
    assert!(seated.contains(&oldest.as_str()), "{sixth}");
}

/// **The echo note reaches the receipt — the wiring, not only the
/// mechanism [`jojobot_adapters::provisioned::Provisioned`]'s own tests
/// already prove.** A case that only called `Memory::echoed_defaults`
/// directly would prove the comparison and nothing about whether this
/// verb ever asks it.
///
/// **Paired in one case**: a field equal to today's shipped default
/// carries the note naming it, and an ordinary field with no shipped
/// default carries none — without the second half this passes on a verb
/// that names every field it is handed.
#[tokio::test]
async fn a_captured_field_equal_to_todays_default_is_named_in_the_receipt() {
    let subject = EntityId::person("person:milhouse");
    let jojobot = handler_field_provisioned(subject, "one_liner", "The disposable implementer.");
    ensure(&jojobot, "person:milhouse").await;

    let echoing = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [(
                    "one_liner".to_string(),
                    "The disposable implementer.".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(
                "person:milhouse",
                "milhouse's own one-liner, matching today's default",
            )
        },
    )
    .await;
    let notes = echoing["echoes_defaults"]
        .as_array()
        .expect("an array of notes");
    assert_eq!(notes.len(), 1, "{echoing}");
    assert!(
        notes[0]
            .as_str()
            .expect("a note string")
            .contains("one_liner"),
        "the note names the echoing key: {echoing}"
    );

    let ordinary = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("greeting".to_string(), "Hey, it's Milhouse.".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:milhouse", "milhouse's own greeting field")
        },
    )
    .await;
    assert!(
        ordinary.get("echoes_defaults").is_none(),
        "a field with no shipped default must carry no note: {ordinary}"
    );
}

/// A promise under somebody, ready to take a day.
async fn a_promise(jojobot: &Jojobot, handle: &str) {
    ensure(jojobot, "person:ned-flanders").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("person:ned-flanders".into()),
            ..add_args("promise", handle, handle)
        }))
        .await
        .expect("add ok");
}

/// **What the cross-kind owed question and the per-kind one each say is owed
/// on a day** — the two reads whose agreement is the point of the stored due
/// moment.
async fn owed_both_ways(jojobot: &Jojobot, day: &str) -> (Vec<String>, Vec<String>) {
    let ask = |args: serde_json::Value| async move {
        let args: RecallArgs = serde_json::from_value(args).expect("recall args");
        let body = json_of(&jojobot.recall(Parameters(args)).await.expect("recall ok"));
        let mut handles: Vec<String> = body["objects"]
            .as_array()
            .expect("objects is an array")
            .iter()
            .map(|o| o["id"].as_str().expect("an id").to_string())
            .collect();
        handles.sort();
        handles
    };
    let by_key = ask(serde_json::json!({
        "fields": [{"key": "due_on"}], "overdue": {"as_of": day},
    }))
    .await;
    let by_kind = ask(serde_json::json!({
        "kind": "promise", "overdue": {"as_of": day},
    }))
    .await;
    (by_key, by_kind)
}

/// **A plain capture of a day carrier copies the caller's own date and keeps
/// the caller's word.**
///
/// A promise's day, a thing's run-out day and a decision's day are dates the
/// caller said. Setting the stored due moment from them adds no date nobody
/// said, so the record stays testimony. **Paired with the rhythm case in the
/// same body**: where jojobot does the arithmetic, the record is still a
/// derivation, so this cannot pass on a build that never demotes.
#[tokio::test]
async fn a_plain_capture_of_a_day_carrier_keeps_the_callers_word() {
    let jojobot = handler();
    a_promise(&jojobot, "return-the-wrench").await;
    for (subject, key, day) in [
        ("promise:return-the-wrench", "promised_by", "2026-07-01"),
        ("thing:torque-wrench", "runs_out", "2026-08-02"),
        ("thing:standing-desk", "decide_by", "2026-09-03"),
    ] {
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                sets: Some([(key.to_string(), day.to_string())].into_iter().collect()),
                ..capture_args(subject, "the operator said the day")
            },
        )
        .await;
        assert_eq!(
            receipt["provenance"], "testimony",
            "{subject}: a date the caller said was filed as a derivation: {receipt}",
        );
        assert_eq!(
            fields_of(&jojobot, subject).await["due_on"],
            day,
            "{subject}: the stored due moment is the caller's own date",
        );
    }

    a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
    let moved = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some(
                [("cadence_days".to_string(), "14".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:descale", "fortnightly now")
        },
    )
    .await;
    assert_eq!(
        moved["provenance"], "inference",
        "a due moment jojobot worked out from a cadence is still its own arithmetic: {moved}",
    );
}

/// **A caller cannot write the stored due moment.** It is jojobot's, set from
/// the carrier keys, and a hand-written copy is how the two owed reads came to
/// disagree. The refusal names the keys that do set a due day, and nothing is
/// written. **Paired with the same capture without `due_on`**, which lands,
/// so the refusal is about the key and not about the record.
#[tokio::test]
async fn a_capture_that_writes_the_due_moment_by_hand_is_refused() {
    let jojobot = handler_indexed();
    a_promise(&jojobot, "return-the-wrench").await;

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                provenance: Some("testimony".into()),
                sets: Some(
                    [
                        ("promised_by".to_string(), "2026-01-16".to_string()),
                        ("due_on".to_string(), "2026-01-16".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("promise:return-the-wrench", "back by the sixteenth")
            }))
            .await
            .expect("capture answers"),
    );
    let said = refused.to_string();
    for carrier_key in ["runs_out", "decide_by", "promised_by", "cadence_days"] {
        assert!(
            said.contains(carrier_key),
            "the refusal names '{carrier_key}', a key that sets a due day: {said}",
        );
    }
    let held = fields_of(&jojobot, "promise:return-the-wrench").await;
    assert!(
        held.get("promised_by").is_none() && held.get("due_on").is_none(),
        "a refused capture leaves the record as it was: {held}",
    );

    let landed = capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some(
                [("promised_by".to_string(), "2026-01-16".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("promise:return-the-wrench", "back by the sixteenth")
        },
    )
    .await;
    assert_eq!(landed["provenance"], "testimony", "{landed}");
    assert_eq!(
        fields_of(&jojobot, "promise:return-the-wrench").await["due_on"],
        "2026-01-16",
    );
    let (by_key, by_kind) = owed_both_ways(&jojobot, "2026-02-01").await;
    assert_eq!(by_key, ["promise:return-the-wrench"], "{by_key:?}");
    assert_eq!(by_kind, by_key, "the two owed reads agree on a promise");
}

/// **A store the deployed code filled reads correctly under the new code, and
/// the next write to a carrier key puts the stored due moment right.**
///
/// The deployed code let a caller write the stored due moment and clear it. A
/// promise can therefore hold a hand-written `due_on` that is not its day, and
/// another can hold its day and no `due_on`. Neither is refused or rewritten
/// on read: the per-kind read finds both, and the cross-kind read, which finds a
/// thing by the stored key, misses the one that lost it. A capture of the
/// carrier key heals both.
#[tokio::test]
async fn a_promise_the_deployed_code_left_without_its_due_moment_is_healed_by_its_next_carrier_write()
 {
    let memory = std::sync::Arc::new(InMemoryMemory::booted());
    let jojobot = handler_indexed_over(memory.clone());
    a_promise(&jojobot, "return-the-wrench").await;
    a_promise(&jojobot, "return-the-desk").await;
    memory.fields_past_the_guard(
        &EntityId("promise:return-the-wrench".into()),
        &[("promised_by", "2026-07-01"), ("due_on", "2026-06-01")],
    );
    memory.fields_past_the_guard(
        &EntityId("promise:return-the-desk".into()),
        &[("promised_by", "2026-07-02")],
    );

    // **The disagreement the deployed code left, and the reads still serve it.**
    // The cross-kind read finds a thing by the stored key, so the promise that
    // lost its key is missing from it; the per-kind read finds both.
    let (by_key, by_kind) = owed_both_ways(&jojobot, "2026-07-10").await;
    assert_eq!(
        by_key,
        ["promise:return-the-wrench"],
        "the cross-kind read finds only the promise that still holds the key: {by_key:?}",
    );
    assert_eq!(
        by_kind,
        ["promise:return-the-desk", "promise:return-the-wrench"],
        "the per-kind read finds both: {by_kind:?}",
    );

    for (subject, day) in [
        ("promise:return-the-wrench", "2026-07-01"),
        ("promise:return-the-desk", "2026-07-02"),
    ] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some(
                    [("promised_by".to_string(), day.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(subject, "still the day")
            },
        )
        .await;
        assert_eq!(
            fields_of(&jojobot, subject).await["due_on"],
            day,
            "{subject}: the carrier write set the stored due moment from the carrier",
        );
    }
    let (by_key, by_kind) = owed_both_ways(&jojobot, "2026-07-10").await;
    assert_eq!(
        by_key,
        ["promise:return-the-desk", "promise:return-the-wrench"],
        "{by_key:?}",
    );
    assert_eq!(by_kind, by_key, "the two reads agree once both are healed");
}

/// **A capture names the claim it created, and the keys it moved.** A key
/// written for the first time has no `from`, a key written again with a new
/// value has both sides, and a key written again with the same value is not
/// listed. A capture is never `nothing`: the claim is always new.
#[tokio::test]
async fn a_capture_lists_its_claim_and_only_the_keys_that_moved() {
    let jojobot = handler();
    let setting = |value: &str| CaptureArgs {
        sets: Some(
            [("mood".to_string(), value.to_string())]
                .into_iter()
                .collect(),
        ),
        ..capture_args("person:bart", "was in a mood")
    };

    let first = capture_ok(&jojobot, setting("cheerful")).await;
    let address = first["address"].as_str().expect("an address").to_string();
    let changed = first["changed"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of changes: {first}"));
    assert!(
        changed
            .iter()
            .any(|c| c["on"] == address.as_str() && c["what"] == "created"),
        "the claim is named by its address: {first}"
    );
    let key = changed
        .iter()
        .find(|c| c["what"] == "key")
        .unwrap_or_else(|| panic!("the key is named: {first}"));
    assert_eq!(key["on"], "person:bart", "{first}");
    assert_eq!(key["key"], "mood", "{first}");
    assert_eq!(key["to"], "cheerful", "{first}");
    assert!(key.get("from").is_none(), "nothing was there: {first}");

    let moved = capture_ok(&jojobot, setting("grumpy")).await;
    let key = moved["changed"]
        .as_array()
        .and_then(|list| list.iter().find(|c| c["what"] == "key"))
        .unwrap_or_else(|| panic!("the key is named: {moved}"));
    assert_eq!(key["from"], "cheerful", "{moved}");
    assert_eq!(key["to"], "grumpy", "{moved}");

    let same = capture_ok(&jojobot, setting("grumpy")).await;
    let changed = same["changed"]
        .as_array()
        .unwrap_or_else(|| panic!("a capture is never nothing: {same}"));
    assert!(
        changed.iter().all(|c| c["what"] != "key"),
        "a key that did not move is not listed: {same}"
    );
    assert!(
        changed.iter().any(|c| c["what"] == "created"),
        "the claim is still new: {same}"
    );
}

/// A list under a thing, ready to take entries.
async fn a_list(jojobot: &Jojobot, handle: &str) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("list", handle, handle)
        }))
        .await
        .expect("add ok");
}

/// **An entry is placed under the list's own lock.** The keys are minted from a
/// read of the neighbours, so a second capture that read before the first one
/// wrote would mint the same key. The case holds the lock of one list, starts a
/// capture on it, and shows the capture waits; it lands once the lock is let go.
/// **Paired in the same case with a capture on another list, which does not
/// wait**, so the wait is the lock's and not the harness's.
#[tokio::test]
async fn a_capture_on_a_list_waits_for_that_lists_placement_lock_and_no_other() {
    let jojobot = handler();
    a_list(&jojobot, "packing").await;
    a_list(&jojobot, "errands").await;

    let held = jojobot
        .hold_placing(&EntityId("list:packing".into()))
        .await
        .expect("the lock of a list that exists");
    let wait = std::time::Duration::from_millis(400);

    let elsewhere = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        capture_ok(&jojobot, capture_args("list:errands", "post the letter")),
    )
    .await;
    assert!(
        elsewhere.is_ok(),
        "a capture on a list nobody holds the lock of must not wait"
    );

    let mut blocked = Box::pin(capture_ok(&jojobot, capture_args("list:packing", "stove")));
    assert!(
        tokio::time::timeout(wait, &mut blocked).await.is_err(),
        "a capture on a list whose placement lock is held must wait for it"
    );
    drop(held);
    let landed = tokio::time::timeout(std::time::Duration::from_secs(5), &mut blocked)
        .await
        .expect("the capture lands once the lock is let go");
    assert!(landed["address"].as_str().is_some(), "{landed}");
}

/// **Two connections over one store that gives up the thread where a real one
/// does.** The transport builds a handler per connection and the process shares
/// one registry, so two handlers over one registry are the shape production has.
/// An in-memory double answers at once, so two callers on one thread never
/// interleave and a lost update between a read and the write built on it cannot
/// happen; this one lets the second caller read before the first has written.
fn two_connections() -> (Jojobot, Jojobot, Arc<InMemoryMemory>) {
    two_connections_holding_a_write(None)
}

/// [`two_connections`], over a store whose next claim write can be held before
/// it reaches the store.
fn two_connections_holding_a_write(
    write_pause: Option<Arc<ListingPause>>,
) -> (Jojobot, Jojobot, Arc<InMemoryMemory>) {
    let memory = Arc::new(InMemoryMemory::seeded());
    let store: Arc<dyn Memory> = Arc::new(YieldsLikeAStore {
        inner: memory.clone(),
        pause: None,
        write_pause,
    });
    let registry = seeded_registry();
    let on = || {
        Jojobot::new(
            store.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            registry.clone(),
        )
    };
    (on(), on(), memory)
}

/// One item added to the list `work:tau` waits on, by this connection.
async fn adding_to_depends_on(who: &Jojobot, item: &str) -> serde_json::Value {
    capture_ok(
        who,
        CaptureArgs {
            sets_add: Some(
                [("depends_on".to_string(), item.to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("work:tau", "adds one")
        },
    )
    .await
}

/// **Two items added to one list at the same moment both stay.** Each call reads
/// the thing's fold, adds its item and stores the whole value; with nothing
/// holding the thing between the read and the write, both read the same two
/// items and the second write drops the first one's addition.
#[tokio::test]
async fn two_items_added_to_a_list_at_the_same_moment_both_survive() {
    let (a, b, memory) = two_connections();
    for handle in [
        "work:delta",
        "work:epsilon",
        "work:gamma",
        "work:phi",
        "work:tau",
    ] {
        ensure(&a, handle).await;
    }
    capture_ok(
        &a,
        CaptureArgs {
            sets: Some(
                [(
                    "depends_on".to_string(),
                    "work:delta, work:epsilon".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args("work:tau", "waits on two")
        },
    )
    .await;

    tokio::join!(
        adding_to_depends_on(&a, "work:gamma"),
        adding_to_depends_on(&b, "work:phi")
    );

    let held = memory
        .fields(&EntityId::person("work:tau"))
        .await
        .expect("the thing reads");
    let items: std::collections::BTreeSet<&str> =
        jojobot_domain::memory::types::list_items(&held["depends_on"])
            .into_iter()
            .collect();
    assert_eq!(
        items,
        ["work:delta", "work:epsilon", "work:gamma", "work:phi"]
            .into_iter()
            .collect(),
        "every item stayed and both additions landed: {}",
        held["depends_on"]
    );
}

/// **Two items added through `update_fact` at the same moment both stay**, for the
/// reason the same case holds for `capture`: the verb reads the thing's fold and
/// writes the whole value, and only the lock on the thing between the two keeps
/// the second from writing over the first.
#[tokio::test]
async fn two_items_added_through_update_fact_at_the_same_moment_both_survive() {
    let (a, b, memory) = two_connections();
    for handle in [
        "work:delta",
        "work:epsilon",
        "work:gamma",
        "work:phi",
        "work:tau",
    ] {
        ensure(&a, handle).await;
    }
    let first = capture_ok(
        &a,
        CaptureArgs {
            sets: Some(
                [(
                    "depends_on".to_string(),
                    "work:delta, work:epsilon".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args("work:tau", "waits on two")
        },
    )
    .await;
    let address = first["address"].as_str().expect("an address").to_string();

    let adding = |who: &Jojobot, item: &str| {
        let args = crate::memory::update_fact::UpdateFactArgs {
            sets_add: Some(
                [("depends_on".to_string(), item.to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..update_args(&address)
        };
        let who = who.clone();
        async move {
            who.update_fact(Parameters(args))
                .await
                .expect("update_fact answers")
        }
    };
    tokio::join!(adding(&a, "work:gamma"), adding(&b, "work:phi"));

    let held = memory
        .fields(&EntityId::person("work:tau"))
        .await
        .expect("the thing reads");
    let items: std::collections::BTreeSet<&str> =
        jojobot_domain::memory::types::list_items(&held["depends_on"])
            .into_iter()
            .collect();
    assert_eq!(
        items,
        ["work:delta", "work:epsilon", "work:gamma", "work:phi"]
            .into_iter()
            .collect(),
        "every item stayed and both additions landed: {}",
        held["depends_on"]
    );
}

/// The order key of every entry the list holds, as the store keeps them.
async fn order_keys_of(memory: &InMemoryMemory, list: &str) -> Vec<String> {
    memory
        .recall(&EntityId::person(list))
        .await
        .expect("the list reads")
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter_map(|fact| fact.fields.get("order").cloned())
        .collect()
}

/// **A capture on a list and a merge of another list into it, at the same
/// moment, leave every entry with a key of its own.** The capture reads the
/// neighbours and mints a key after the last. The case holds it there, before it
/// writes, and lets the merge run: the merge mints its keys after the same last,
/// so with nothing holding the survivor both mint one key.
#[tokio::test]
async fn a_capture_and_a_merge_into_one_list_at_the_same_moment_give_every_entry_its_own_key() {
    let pause = Arc::new(ListingPause::default());
    let (a, b, memory) = two_connections_holding_a_write(Some(pause.clone()));
    a_list(&a, "packing").await;
    a_list(&a, "errands").await;
    capture_ok(&a, capture_args("list:packing", "tent")).await;
    capture_ok(&a, capture_args("list:errands", "letter")).await;
    capture_ok(&a, capture_args("list:errands", "bread")).await;
    let sid = writing_as(&b);

    pause.arm();
    let capturing = {
        let a = a.clone();
        tokio::spawn(async move { capture_ok(&a, capture_args("list:packing", "stove")).await })
    };
    pause.reached().await;
    let merging = tokio::spawn(async move {
        b.merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
            request_id: None,
            duplicate: "list:errands".into(),
            survivor: "list:packing".into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid),
        }))
        .await
    });
    // The merge either lands now, or waits for the list's lock until the capture
    // is let go.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    pause.release();
    capturing.await.expect("the capture ran");
    merging
        .await
        .expect("the merge ran")
        .expect("the merge lands");

    let keys = order_keys_of(&memory, "list:packing").await;
    let distinct: std::collections::BTreeSet<&String> = keys.iter().collect();
    assert_eq!(keys.len(), 4, "every entry is still there: {keys:?}");
    assert_eq!(
        distinct.len(),
        keys.len(),
        "no two entries share a key: {keys:?}"
    );
}

/// One merge of `duplicate` into `survivor`, through the handler.
async fn merging(who: &Jojobot, duplicate: &str, survivor: &str) -> serde_json::Value {
    let sid = writing_as(who);
    let result = who
        .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
            request_id: None,
            duplicate: duplicate.into(),
            survivor: survivor.into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid),
        }))
        .await
        .expect("a merge answers");
    json_of(&result)
}

/// **Two callers that name the same two lists the other way round take the
/// locks in one order.** The case holds the lock that sorts first. Both callers
/// then wait for it, and neither has taken the other lock: a caller that took the
/// second lock first would hold it while it waited, and the caller that took the
/// first would then wait for it, which is a wait with no end.
#[tokio::test]
async fn callers_naming_two_lists_the_other_way_round_take_their_locks_in_one_order() {
    let (a, _b, _memory) = two_connections();
    a_list(&a, "packing").await;
    a_list(&a, "errands").await;
    let alpha = EntityId::person("list:packing");
    let beta = EntityId::person("list:errands");
    let mut keys = [
        a.placing_key(&alpha).await.expect("a key"),
        a.placing_key(&beta).await.expect("a key"),
    ];
    keys.sort();
    let first = a.registry.hold_placing(&keys[0]).await;

    let waiting = |one: &EntityId, other: &EntityId| {
        let (who, one, other) = (a.clone(), one.clone(), other.clone());
        tokio::spawn(async move {
            who.hold_placing_both(&one, &other)
                .await
                .map(|held| held.len())
        })
    };
    let (forward, backward) = (waiting(&alpha, &beta), waiting(&beta, &alpha));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let second = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        a.registry.hold_placing(&keys[1]),
    )
    .await;
    assert!(
        second.is_ok(),
        "a caller waiting for the first lock must not hold the second"
    );
    drop(second);
    drop(first);
    for waiter in [forward, backward] {
        let held = tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
            .await
            .expect("the caller gets both locks once the first is let go")
            .expect("the caller ran")
            .expect("the locks are taken");
        assert_eq!(held, 2);
    }
}

/// **A list merged into itself is refused, not left waiting on its own lock.**
/// Both names answer to one thing, so the lock is taken once.
#[tokio::test]
async fn a_list_merged_into_itself_is_refused_and_does_not_wait_on_its_own_lock() {
    let (a, _b, _memory) = two_connections();
    a_list(&a, "packing").await;

    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        merging(&a, "list:packing", "list:packing"),
    )
    .await
    .expect("the refusal comes back");

    assert_eq!(answer["status"], "blocked", "{answer}");
}
