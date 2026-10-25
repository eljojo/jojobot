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
