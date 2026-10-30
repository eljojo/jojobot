//! **A contract, not a story: what the served verb text says.**
//!
//! These cases read the text a client is served for a verb, its description and
//! the schema of its arguments, and call no verb for a result. They hold the
//! surface's shape, and a feature is not reached by them, so they are not
//! stories. The two real stories of refusals that name real routes are in
//! `refusals_name_real_routes.rs`.

use serde_json::json;

use super::dsl::Story;

/// **A key a served answer carries is named in the text of the verb that
/// carries it.** `list_runs` lists a run whose row cannot be read with
/// `unreadable` and `fix_by`; the four write verbs carry `reads_left` when a read
/// failed under their receipt. A client learns each from the text the verb is
/// served with, its description or its arguments', or not at all.
#[tokio::test]
async fn the_description_of_a_verb_names_the_keys_its_answer_can_carry() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let list_runs = s.served_text("list_runs").await;
    for key in ["unreadable", "fix_by"] {
        assert!(
            list_runs.contains(key),
            "list_runs names {key}: {list_runs}"
        );
    }
    for verb in ["capture", "update_fact", "update_entity", "add_entity"] {
        let description = s.served_text(verb).await;
        assert!(
            description.contains("reads_left"),
            "{verb} names reads_left: {description}"
        );
    }
}

/// **An argument that does nothing is not published.** No refusal of `capture` or
/// `update_fact` carries a token while a type's rule is off (rule 636), so the
/// override arguments are not in their schemas, and a call that still sends one
/// is refused for it. **Paired**: the verbs whose refusals do carry a token still
/// publish it.
#[tokio::test]
async fn capture_and_update_fact_publish_no_override_while_no_refusal_of_theirs_carries_one() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    for argument in ["override_token", "override_reason"] {
        let publishing = s.verbs_publishing(argument).await;
        for verb in ["capture", "update_fact"] {
            assert!(
                !publishing.iter().any(|name| name == verb),
                "{verb} publishes {argument}: {publishing:?}"
            );
        }
        for verb in [
            "add_entity",
            "update_entity",
            "rename_entity",
            "declare_type",
        ] {
            assert!(
                publishing.iter().any(|name| name == verb),
                "{verb} still publishes {argument}: {publishing:?}"
            );
        }
    }
    let call = json!({"subject": "bot:gamma", "content": "a note", "provenance": "inference"});
    let mut with_override = call.clone();
    with_override["override_token"] = json!("t");
    with_override["override_reason"] = json!("r");
    let refused = s.refused("capture", with_override).await;
    // Refused for the argument, and for nothing else: the same call without it
    // lands and reads back.
    assert_eq!(
        refused.json()["refusal_code"],
        "UnknownArgument",
        "{}",
        refused.raw()
    );
    refused.says("override_token");
    s.call("capture", call).await;
    s.recall("bot:gamma").await.says("a note");
}
