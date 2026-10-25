use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// The key the receipt carries the sentence under. A spelling nothing outside
/// this process declares, and a reader finds the sentence by it.
const SENTENCE_KEY: &str = "a_named_thing_did_not_receive_them";

async fn known(jojobot: &Jojobot) {
    ensure(jojobot, "thing:the-furnace").await;
    ensure(jojobot, "org:mr-plow").await;
}

fn sets_runs_out() -> Option<std::collections::BTreeMap<String, String>> {
    Some(
        [("runs_out".to_string(), "2027-03-01".to_string())]
            .into_iter()
            .collect(),
    )
}

async fn capture_on(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
    json_of(&jojobot.capture(Parameters(args)).await.expect("capture ok"))
}

/// **A write that sets keys on its subject while its refs name another thing
/// that gets none says where the keys went.**
#[tokio::test]
async fn keys_landing_on_the_subject_while_a_ref_receives_none_are_said_to_have_missed_it() {
    let jojobot = handler();
    known(&jojobot).await;
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["thing:the-furnace".into()]),
            sets: sets_runs_out(),
            ..capture_args("org:mr-plow", "the plan lapses in March")
        },
    )
    .await;
    assert_eq!(
        receipt[SENTENCE_KEY],
        "runs_out landed on org:mr-plow; thing:the-furnace, named in your refs, did not \
         receive it",
        "{receipt}"
    );
}

/// **Nothing extra when no key was set.**
#[tokio::test]
async fn the_sentence_is_silent_when_the_write_sets_no_key() {
    let jojobot = handler();
    known(&jojobot).await;
    let no_sets = capture_on(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["thing:the-furnace".into()]),
            ..capture_args("org:mr-plow", "they service it each spring")
        },
    )
    .await;
    assert!(no_sets.get(SENTENCE_KEY).is_none(), "{no_sets}");
}

/// **Nothing extra when the named thing holds the key too.**
#[tokio::test]
async fn the_sentence_is_silent_when_the_named_thing_holds_the_key() {
    let jojobot = handler();
    known(&jojobot).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: sets_runs_out(),
            ..capture_args("thing:the-furnace", "the warranty lapses in March")
        },
    )
    .await;
    let also_holds = capture_on(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["thing:the-furnace".into()]),
            sets: sets_runs_out(),
            ..capture_args("org:mr-plow", "their plan lapses in March too")
        },
    )
    .await;
    assert!(also_holds.get(SENTENCE_KEY).is_none(), "{also_holds}");
}

const FURNACE_WORDS: &str = "The furnace's service is due in March";

/// **A check-in names no key in `sets`, so it has no key to report** even when its
/// words name another known thing: the outcome and the dates it records are
/// jojobot's arithmetic, not the caller's keys.
#[tokio::test]
async fn a_check_in_whose_words_name_another_thing_is_silent() {
    let jojobot = handler();
    known(&jojobot).await;
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
            sets: Some(
                [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "check_in_date".to_string()),
                    ("counts_from".to_string(), "2026-08-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("rhythm:descale", "every week")
        },
    )
    .await;
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            check_in: Some("ran".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", FURNACE_WORDS)
        },
    )
    .await;
    assert!(
        receipt["reached_the_thing"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|k| k == "outcome")),
        "the check-in reached the thing with its outcome, so the case is not vacuous: {receipt}"
    );
    assert!(receipt.get(SENTENCE_KEY).is_none(), "{receipt}");
}

/// **A write by defrag carries the review stamp, which the caller did not send.**
#[tokio::test]
async fn a_write_by_defrag_is_silent_about_its_review_stamp() {
    let jojobot = handler();
    known(&jojobot).await;
    make_bot(&jojobot, "defrag").await;
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            sid: Some(as_bot(&jojobot, "defrag")),
            ..capture_args("org:mr-plow", FURNACE_WORDS)
        },
    )
    .await;
    assert!(
        receipt["reached_the_thing"]
            .as_array()
            .is_some_and(|keys| !keys.is_empty()),
        "the stamp reached the thing, so the case is not vacuous: {receipt}"
    );
    assert!(receipt.get(SENTENCE_KEY).is_none(), "{receipt}");
}

/// **A thing named only in the words is named as such.**
#[tokio::test]
async fn a_thing_named_in_the_words_is_said_to_be_named_in_the_words() {
    let jojobot = handler();
    known(&jojobot).await;
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            sets: sets_runs_out(),
            ..capture_args("org:mr-plow", FURNACE_WORDS)
        },
    )
    .await;
    assert_eq!(
        receipt[SENTENCE_KEY],
        "runs_out landed on org:mr-plow; thing:the-furnace, named in your words, did not \
         receive it",
        "{receipt}"
    );
}

/// **A thing the claim already points at is not reported**: its handle written
/// into the words is a link, as the possible-subject note counts it.
#[tokio::test]
async fn a_handle_written_into_the_words_is_not_reported() {
    let jojobot = handler();
    known(&jojobot).await;
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            sets: sets_runs_out(),
            ..capture_args("org:mr-plow", "Plan for thing:the-furnace, due in March")
        },
    )
    .await;
    assert!(receipt.get(SENTENCE_KEY).is_none(), "{receipt}");
}

/// **At most three things are named, and several keys read "them".**
#[tokio::test]
async fn at_most_three_things_are_named_and_several_keys_say_them() {
    let jojobot = handler();
    known(&jojobot).await;
    for handle in ["thing:delta", "thing:gamma", "thing:sigma", "thing:tau"] {
        ensure(&jojobot, handle).await;
    }
    let receipt = capture_on(
        &jojobot,
        CaptureArgs {
            refs: Some(
                ["thing:delta", "thing:gamma", "thing:sigma", "thing:tau"]
                    .map(String::from)
                    .to_vec(),
            ),
            sets: Some(
                [
                    ("runs_out".to_string(), "2027-03-01".to_string()),
                    ("decide_by".to_string(), "2027-02-01".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args("org:mr-plow", "the plan")
        },
    )
    .await;
    let said = receipt[SENTENCE_KEY].as_str().expect("the sentence");
    assert_eq!(said.matches("named in your refs").count(), 3, "{said}");
    assert!(
        said.starts_with("decide_by, runs_out landed on org:mr-plow;"),
        "{said}"
    );
    assert!(said.ends_with("did not receive them"), "{said}");
}

/// **An edit that sets a key says it too**, through the same composer.
#[tokio::test]
async fn an_edit_that_sets_a_key_beside_a_ref_says_it_too() {
    let jojobot = handler();
    known(&jojobot).await;
    let filed = capture_ok(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["thing:the-furnace".into()]),
            ..capture_args("org:mr-plow", "they service it each spring")
        },
    )
    .await;
    let edited = json_of(
        &jojobot
            .update_fact(Parameters(crate::memory::update_fact::UpdateFactArgs {
                sets: sets_runs_out(),
                ..update_args(&address_of(&filed))
            }))
            .await
            .expect("update ok"),
    );
    assert_eq!(
        edited[SENTENCE_KEY],
        "runs_out landed on org:mr-plow; thing:the-furnace, named in your refs, did not \
         receive it",
        "{edited}"
    );
}

/// **A thing the claim already points at is not reported on the words path
/// either**: the edge's object and the subject's parent are named in the words
/// here and receive no key, and the sentence stays silent for both.
#[tokio::test]
async fn the_edge_object_and_the_parent_named_in_the_words_are_not_reported() {
    let jojobot = handler();
    known(&jojobot).await;
    let edged = capture_on(
        &jojobot,
        CaptureArgs {
            shape: Some("about".into()),
            object: Some("thing:the-furnace".into()),
            sets: sets_runs_out(),
            ..capture_args("org:mr-plow", FURNACE_WORDS)
        },
    )
    .await;
    assert!(
        edged["reached_the_thing"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|k| k == "runs_out")),
        "the key reached the subject, so the case is not vacuous: {edged}"
    );
    assert!(edged.get(SENTENCE_KEY).is_none(), "{edged}");

    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "descale", "descale")
        }))
        .await
        .expect("add ok");
    let child = capture_on(
        &jojobot,
        CaptureArgs {
            sets: sets_runs_out(),
            ..capture_args("rhythm:descale", "The kettle is descaled in March")
        },
    )
    .await;
    assert!(child.get(SENTENCE_KEY).is_none(), "{child}");
}
