//! **Reads a user story alone held, proven at the handler.** A story's failure
//! points at a scenario; these point at the branch.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// The call that reads records by address: no subject, no facts, only the list.
fn records_of(addresses: &[&str], offset: Option<u32>) -> RecallArgs {
    RecallArgs {
        subject: None,
        facts: None,
        records: Some(addresses.iter().map(|a| a.to_string()).collect()),
        offset,
        ..recall_args("alpha")
    }
}

/// **An address on a thing merged away is named in its own place, with its
/// survivor, and the rest of the list is served.** The merge maps the claims the
/// duplicate held then, so those addresses still read. A number the duplicate
/// never held maps to nothing, and the reader is told where to read instead.
/// Paired in one answer: the survivor's own address and the mapped one are
/// served whole.
#[tokio::test]
async fn a_record_address_on_a_merged_away_thing_is_named_with_its_survivor() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    capture_ok(
        &jojobot,
        capture_args("person:milhouse", "plays the cornet"),
    )
    .await;
    capture_ok(&jojobot, capture_args("person:bart", "skates to school")).await;
    let merged = jojobot
        .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
            request_id: None,
            duplicate: "person:bart".into(),
            survivor: "person:milhouse".into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid),
        }))
        .await
        .expect("the merge answers");
    assert_ne!(json_of(&merged)["status"], "blocked", "{merged:?}");

    let answered = json_of(
        &jojobot
            .recall(Parameters(records_of(
                &["person:milhouse#f1", "person:bart#f9", "person:bart#f1"],
                None,
            )))
            .await
            .expect("recall answers"),
    );
    let records = answered["records"].as_array().expect("records");
    assert_eq!(answered["count"], 3, "{answered}");
    assert_eq!(records[0]["content"], "plays the cornet", "{answered}");
    assert_eq!(
        records[2]["content"], "skates to school",
        "an address the merge maps is still served: {answered}"
    );
    let missing = records[1]["missing"]
        .as_str()
        .unwrap_or_else(|| panic!("the unmapped address is named missing: {answered}"));
    assert!(
        missing.contains("person:milhouse"),
        "the reason names the survivor to read: {missing}"
    );
    assert_eq!(records[1]["address"], "person:bart#f9");
}

/// **An offset at the end of a records read says how many there are, and one
/// just inside answers.** The same list reads whole at offset 0 with no note, so
/// the empty part is the offset's doing.
#[tokio::test]
async fn a_records_offset_at_the_end_says_how_many_there_are_and_one_inside_answers() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("person:alpha", "is painted green")).await;
    capture_ok(
        &jojobot,
        capture_args("person:alpha", "has a dent in the flag"),
    )
    .await;
    let both = ["person:alpha#f1", "person:alpha#f2"];
    let jojobot = &jojobot;
    let read = |offset| async move {
        json_of(
            &jojobot
                .recall(Parameters(records_of(&both, offset)))
                .await
                .expect("recall answers"),
        )
    };

    let whole = read(None).await;
    assert_eq!(whole["count"], 2, "{whole}");
    assert!(whole.get("past_the_end").is_none(), "{whole}");

    let inside = read(Some(1)).await;
    assert_eq!(
        inside["records"][0]["address"], "person:alpha#f2",
        "an offset just inside the end answers from there: {inside}"
    );
    assert!(inside.get("past_the_end").is_none(), "{inside}");

    let past = read(Some(2)).await;
    let said = past["past_the_end"]
        .as_str()
        .unwrap_or_else(|| panic!("an offset at the end says so: {past}"));
    assert!(said.contains('2'), "it says how many there are: {said}");
    assert_eq!(past["records"].as_array().map(Vec::len), Some(0), "{past}");
}

/// **A browse that cannot fit a list whole cuts its entries and names how many it
/// left out and the call that reads on; a list that fits names none.** Two lists
/// of one kind: the long one leads and carries `entries_not_shown` with a count
/// and the way to read on, the short one is served whole beside it with none.
#[tokio::test]
async fn a_browse_cuts_a_long_list_and_names_the_rest_while_a_short_one_names_none() {
    let jojobot = handler();
    for handle in ["list:homer-chores", "list:marge-errands"] {
        ensure(&jojobot, handle).await;
    }
    // Four hundred entries of about a hundred characters each cannot fit the
    // answer ceiling, put down fifty to a call.
    let text = |n: usize| {
        format!(
            "item {n:03}: the thing to do, with a note on where it lives in the shed and when \
             it was last done"
        )
    };
    for first in (1..=400).step_by(50) {
        capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                entries: Some(
                    (first..first + 50)
                        .map(|n| crate::memory::capture::EntryItem::Text(text(n)))
                        .collect(),
                ),
                ..capture_args("list:homer-chores", "")
            },
        )
        .await;
    }
    capture_ok(
        &jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            ..capture_args("list:marge-errands", "buy stamps")
        },
    )
    .await;

    let browse = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: None,
                facts: None,
                kind: Some("list".into()),
                ..recall_args("alpha")
            }))
            .await
            .expect("recall answers"),
    );
    let leading = &browse["objects"][0];
    assert_eq!(
        leading["id"], "list:homer-chores",
        "the long list leads: {browse}"
    );
    let shown = leading["entries"].as_array().map_or(0, Vec::len);
    assert!(shown > 0 && shown < 400, "some entries, not all: {shown}");
    let left = &leading["entries_not_shown"];
    assert_eq!(left["count"], (400 - shown) as u64, "{left}");
    assert!(
        left["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("list:homer-chores") && how.contains("offset")),
        "it names the call that reads on: {left}"
    );
    // The pair: a list that fits is served whole and names nothing left out.
    let short = browse["objects"]
        .as_array()
        .and_then(|objects| objects.iter().find(|o| o["id"] == "list:marge-errands"));
    if let Some(short) = short {
        assert!(
            short.get("entries_not_shown").is_none(),
            "a list that fits names none left out: {short}"
        );
    } else {
        let on = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: None,
                    facts: None,
                    kind: Some("list".into()),
                    offset: Some(1),
                    ..recall_args("alpha")
                }))
                .await
                .expect("recall answers"),
        );
        assert_eq!(on["objects"][0]["id"], "list:marge-errands", "{on}");
        assert!(on["objects"][0].get("entries_not_shown").is_none(), "{on}");
    }
}
