//! **A key jojobot computes reaches its thing, when a caller's own fields do
//! not.**
//!
//! A record carries two bags: its own fields, which stay on the record, and
//! what it sets on its thing. The keys jojobot works out itself (a check-in's
//! schedule, a due day, a role's lease) are not a caller's words on a record:
//! they are the thing's own state, and a thing that lost them would stop falling
//! due. These cases run against a store where own fields stay, and hold every
//! computed key to the thing.
//!
//! **The computed keys are not listed here.** Each case does what a caller does,
//! then takes the keys the record holds that the caller did not send: whatever
//! jojobot added. A new computed key lands in that difference without an edit
//! to this file, and the case fails if it stays on the record. The carriers and
//! the role keys come from the code that declares them.
//!
//! Test-only, and declared by `lib.rs`.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use jojobot_domain::attention;
use jojobot_domain::memory::types::ValueType;
use rmcp::handler::server::wrapper::Parameters;
use std::collections::BTreeSet;

/// **What a record holds and what its thing holds**, as a reader sees them: the
/// keys of the claim at `address`, and the keys of the folded thing.
async fn record_and_thing(
    jojobot: &Jojobot,
    subject: &str,
    address: &str,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args(subject)))
            .await
            .expect("recall ok"),
    );
    let object = &body["objects"][0];
    let keys = |node: &serde_json::Value| -> BTreeSet<String> {
        node.as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    };
    let record = object["facts"]
        .as_array()
        .expect("the record is listed")
        .iter()
        .find(|fact| fact["address"] == address)
        .unwrap_or_else(|| panic!("{address} is not among the records: {body}"));
    // The keys the record holds, whichever bag: a key jojobot computed is set on
    // the thing, so it sits under `sets`.
    let mut held = keys(&record["fields"]);
    held.extend(keys(&record["sets"]));
    (held, keys(&object["fields"]))
}

/// A value a field of this type accepts, generated from the declaration.
fn sample(field: &jojobot_domain::memory::types::Field) -> String {
    if let Some(allowed) = &field.one_of {
        return allowed.first().cloned().expect("a set names a value");
    }
    match field.holds {
        ValueType::Date => "2026-10-01".to_string(),
        ValueType::Number => "7".to_string(),
        _ => "a value".to_string(),
    }
}

/// **The control: a key sent as an own field stays on its record here.** It
/// pairs the cases below, which would pass over a store that never switched.
#[tokio::test]
async fn a_key_sent_as_an_own_field_stays_on_the_record_in_a_switched_store() {
    let jojobot = handler();
    ensure(&jojobot, "thing:shelf").await;
    let wrote = capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("colour".to_string(), "green".to_string())].into()),
            ..capture_args("thing:shelf", "painted")
        },
    )
    .await;
    let (record, thing) =
        record_and_thing(&jojobot, "thing:shelf", wrote["address"].as_str().unwrap()).await;
    assert!(record.contains("colour"), "{record:?}");
    assert!(!thing.contains("colour"), "an own field reached: {thing:?}");

    let set = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some([("size".to_string(), "large".to_string())].into()),
            ..capture_args("thing:shelf", "measured")
        },
    )
    .await;
    let (_, thing) =
        record_and_thing(&jojobot, "thing:shelf", set["address"].as_str().unwrap()).await;
    assert!(thing.contains("size"), "a setting did not reach: {thing:?}");
}

/// **Every shipped carrier's computed keys reach the thing.** One thing per
/// carrier, given every key the carrier reads under `sets`; whatever the record
/// then holds beyond what was sent is jojobot's, and has to be on the thing.
#[tokio::test]
async fn every_key_a_carrier_computes_reaches_its_thing() {
    let jojobot = handler();
    // One roster handle per carrier; the case fails if there are more carriers
    // than handles, which is the moment to add a handle.
    let handles = [
        "thing:dinners",
        "thing:jukebox",
        "thing:shelf",
        "thing:the-couch",
        "thing:the-fern",
        "thing:the-gutter",
    ];
    let carriers = attention::shipped();
    assert!(
        carriers.len() <= handles.len(),
        "{} carriers and only {} handles",
        carriers.len(),
        handles.len()
    );
    let mut computed_somewhere = false;
    for (carrier, handle) in carriers.iter().zip(handles) {
        let interface = carrier.interface();
        let sent: std::collections::BTreeMap<String, String> = interface
            .fields
            .iter()
            .map(|field| (field.key.clone(), sample(field)))
            .collect();
        ensure(&jojobot, handle).await;
        let wrote = capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some(sent.clone()),
                ..capture_args(handle, "set the schedule")
            },
        )
        .await;
        let (record, thing) =
            record_and_thing(&jojobot, handle, wrote["address"].as_str().unwrap()).await;
        let computed: BTreeSet<String> = record
            .iter()
            .filter(|key| !sent.contains_key(*key))
            .cloned()
            .collect();
        computed_somewhere |= !computed.is_empty();
        for key in &computed {
            assert!(
                thing.contains(key),
                "{handle} ({}): jojobot computed '{key}' and it stayed on the record; the thing \
                 holds {thing:?}",
                interface.name
            );
        }
    }
    assert!(
        computed_somewhere,
        "no carrier computed a key, so this case checked nothing"
    );
}

/// **A check-in's schedule reaches the rhythm.** The rhythm is made whole with
/// its settings, then checked in; every key the check-in's record holds that the
/// caller did not send is the schedule jojobot worked out.
#[tokio::test]
async fn every_key_a_check_in_computes_reaches_the_rhythm() {
    let jojobot = handler();
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let bag: std::collections::BTreeMap<String, String> = [
        ("cadence_days".to_string(), "7".to_string()),
        ("advances_from".to_string(), "check_in_date".to_string()),
        ("counts_from".to_string(), "2026-08-01".to_string()),
    ]
    .into_iter()
    .collect();
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(bag),
            ..capture_args("rhythm:descale", "every week")
        },
    )
    .await;
    let sent: BTreeSet<String> = BTreeSet::new();
    let checked = capture_ok(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        },
    )
    .await;
    let (record, thing) = record_and_thing(
        &jojobot,
        "rhythm:descale",
        checked["address"].as_str().unwrap(),
    )
    .await;
    let computed: BTreeSet<&String> = record.iter().filter(|key| !sent.contains(*key)).collect();
    assert!(!computed.is_empty(), "the check-in computed nothing");
    for key in computed {
        assert!(
            thing.contains(key),
            "the check-in computed '{key}' and it stayed on the record; the thing holds {thing:?}"
        );
    }
}

/// **A role's lease keys reach the role object.** The keys are the constants
/// the code that writes them declares.
#[tokio::test]
async fn the_role_keys_a_claim_writes_reach_the_role_object() {
    use jojobot_domain::session::{ROLE_CLAIMED_AT, ROLE_HOLDER};
    let jojobot = handler();
    make_bot(&jojobot, "alpha").await;
    let booted = json_of(
        &jojobot
            .start_here(Parameters(crate::orientation::start_here::OrientArgs {
                compacted: None,
                claim: Some("gamma".into()),
                timezone: None,
                bot: Some("alpha".into()),
                brief: Some(true),
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    );
    assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args("role:gamma")))
            .await
            .expect("recall ok"),
    );
    let thing = body["objects"][0]["fields"]
        .as_object()
        .unwrap_or_else(|| panic!("the role object holds no fields: {body}"));
    for key in [ROLE_HOLDER, ROLE_CLAIMED_AT] {
        assert!(
            thing.contains_key(key),
            "'{key}' did not reach the role object: {thing:?}"
        );
    }

    // **A release is a patch to the record the claim made**, and it writes the
    // lease moment again: that key has to go on reaching, and the holder, which
    // the release takes off, has to leave the object.
    let sid = booted["session"]["sid"]
        .as_str()
        .expect("a sid")
        .to_string();
    let wrapped = jojobot
        .wrap_session(Parameters(crate::session::wrap_session::WrapSessionArgs {
            story: "done".into(),
            sid,
        }))
        .await
        .expect("wrap ok");
    assert_ne!(json_of(&wrapped)["status"], "blocked", "{wrapped:?}");
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args("role:gamma")))
            .await
            .expect("recall ok"),
    );
    let thing = body["objects"][0]["fields"]
        .as_object()
        .unwrap_or_else(|| panic!("the role object holds no fields: {body}"));
    assert!(
        thing.contains_key(ROLE_CLAIMED_AT),
        "the release's lease moment did not reach the role object: {thing:?}"
    );
    assert!(
        !thing.contains_key(ROLE_HOLDER),
        "the release left the holder on the role object: {thing:?}"
    );
}

/// **What a thing holds**, as a reader sees it: the keys of the folded object.
async fn thing_keys(jojobot: &Jojobot, subject: &str) -> BTreeSet<String> {
    let body = json_of(
        &jojobot
            .recall(Parameters(recall_args(subject)))
            .await
            .expect("recall ok"),
    );
    body["objects"][0]["fields"]
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// **Two records on one thing, the second the one taken back.** The first sets
/// `size` and `depth`; the second sets `size` again and `tint`, and keeps `colour`
/// as an own field, which stays on it. Returns the second record's address.
async fn a_shelf_with_a_record_to_take_back(jojobot: &Jojobot) -> String {
    ensure(jojobot, "thing:shelf").await;
    capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("size".to_string(), "small".to_string()),
                    ("depth".to_string(), "deep".to_string()),
                ]
                .into(),
            ),
            ..capture_args("thing:shelf", "measured")
        },
    )
    .await;
    let second = capture_ok(
        jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("size".to_string(), "large".to_string()),
                    ("tint".to_string(), "red".to_string()),
                ]
                .into(),
            ),
            fields: Some([("colour".to_string(), "green".to_string())].into()),
            ..capture_args("thing:shelf", "measured again and painted")
        },
    )
    .await;
    let held = thing_keys(jojobot, "thing:shelf").await;
    assert!(held.contains("tint") && held.contains("size"), "{held:?}");
    assert!(!held.contains("colour"), "an own field reached: {held:?}");
    second["address"].as_str().expect("an address").to_string()
}

/// **Archiving a record names the keys that left the thing.** `tint` was set by
/// the archived record alone and is gone. `size` goes back to the earlier
/// record's value and has not left. `colour` stayed on the record and never
/// reached, so it is not named: a list that named it would say the thing lost
/// something it never held.
#[tokio::test]
async fn archiving_a_record_names_the_keys_that_left_the_thing() {
    let jojobot = handler();
    let address = a_shelf_with_a_record_to_take_back(&jojobot).await;
    let receipt = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        receipt["left_the_thing"],
        serde_json::json!(["tint"]),
        "{receipt}"
    );
    let held = thing_keys(&jojobot, "thing:shelf").await;
    assert!(!held.contains("tint"), "the key stayed: {held:?}");
    assert!(
        held.contains("size"),
        "the earlier value went too: {held:?}"
    );
}

/// **A retraction takes back what the record set, and says which keys left.**
/// Retracting archives the record, so the thing loses what only that record set
/// and the receipt names it the same way an archive does.
#[tokio::test]
async fn retracting_a_record_takes_back_what_it_set_and_names_the_keys_that_left() {
    let jojobot = handler();
    let address = a_shelf_with_a_record_to_take_back(&jojobot).await;
    let sid = writing_as(&jojobot);
    let receipt = json_of(
        &jojobot
            .retract(Parameters(RetractArgs {
                address,
                reason: Some("measured the wrong shelf".into()),
                recorded_at: None,
                sid: Some(sid),
            }))
            .await
            .expect("retract ok"),
    );
    assert_eq!(
        receipt["left_the_thing"],
        serde_json::json!(["tint"]),
        "{receipt}"
    );
    let held = thing_keys(&jojobot, "thing:shelf").await;
    assert!(!held.contains("tint"), "the key stayed: {held:?}");
}

/// **Nothing left is said as an empty list**, so a caller reads "no key left" and
/// does not infer it from a missing field. The record archived here set a key an
/// earlier record set too, so the thing still holds it.
#[tokio::test]
async fn archiving_a_record_that_took_nothing_with_it_says_so() {
    let jojobot = handler();
    ensure(&jojobot, "thing:shelf").await;
    for said in ["measured", "measured again"] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("size".to_string(), "small".to_string())].into()),
                ..capture_args("thing:shelf", said)
            },
        )
        .await;
    }
    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                ..recall_args("thing:shelf")
            }))
            .await
            .expect("recall ok"),
    );
    let address = body["objects"][0]["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .find(|fact| fact["content"] == "measured again")
        .and_then(|fact| fact["address"].as_str())
        .expect("the second record")
        .to_string();
    let receipt = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("archived".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        receipt["left_the_thing"],
        serde_json::json!([]),
        "{receipt}"
    );
}

/// **A receipt places each key by the class the store kept.** In a switched store
/// the key sent as an own field stays on the record and the key sent in `sets`
/// reaches. The same call on a store that has not switched places both on the
/// thing, so the list follows the store and not a list of key names.
#[tokio::test]
async fn a_receipt_places_each_key_by_the_class_the_store_kept() {
    for (jojobot, own_field_reaches) in [(handler(), false), (handler_reaching(), true)] {
        ensure(&jojobot, "thing:shelf").await;
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                sets: Some([("size".to_string(), "large".to_string())].into()),
                fields: Some([("colour".to_string(), "green".to_string())].into()),
                ..capture_args("thing:shelf", "measured and painted")
            },
        )
        .await;
        let (reached, stayed) = if own_field_reaches {
            (serde_json::json!(["colour", "size"]), serde_json::json!([]))
        } else {
            (serde_json::json!(["size"]), serde_json::json!(["colour"]))
        };
        assert_eq!(receipt["reached_the_thing"], reached, "{receipt}");
        assert_eq!(receipt["stayed_on_the_record"], stayed, "{receipt}");

        // An edit that changes the own field keeps the key where it was written.
        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some([("colour".to_string(), "blue".to_string())].into()),
                    ..update_args(receipt["address"].as_str().expect("an address"))
                }))
                .await
                .expect("update ok"),
        );
        let (reached, stayed) = if own_field_reaches {
            (serde_json::json!(["colour", "size"]), serde_json::json!([]))
        } else {
            (serde_json::json!(["size"]), serde_json::json!(["colour"]))
        };
        assert_eq!(edited["reached_the_thing"], reached, "{edited}");
        assert_eq!(edited["stayed_on_the_record"], stayed, "{edited}");
    }
}
