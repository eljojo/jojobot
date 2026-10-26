//! A rule index names whole labels and leaves engine records out.

use super::identity::unseated_rules;
use crate::harness::{boot, handler_over};
use jojobot_domain::memory::testing::{InMemoryMemory, contract::rules_index_records};
use jojobot_domain::memory::{EntityId, Fact};
use std::sync::Arc;

async fn unseated_rule() -> Fact {
    rules_index_records(&InMemoryMemory::booted())
        .await
        .into_iter()
        .find(|fact| fact.content == "An unseated rule")
        .unwrap()
}

fn line(rule: &Fact) -> String {
    let index = unseated_rules(&rule.subject, std::slice::from_ref(rule), 1).unwrap();
    assert_eq!(index["listed"][0]["address"], rule.address().to_string());
    index["listed"][0]["line"].as_str().unwrap().into()
}

#[tokio::test]
async fn rules_index_boot_counts_and_lists_only_rules() {
    let memory = Arc::new(InMemoryMemory::booted());
    let records = rules_index_records(memory.as_ref()).await;
    let answer = boot(&handler_over(memory), records[0].subject.slug()).await;
    let identity = &answer["identity"];
    let carried = identity["rules"]
        .as_array()
        .expect("the boot carries rules");
    assert_eq!(carried.len(), 1, "{identity}");
    assert_eq!(carried[0]["content"], "A carried rule");
    assert!(
        identity["rules_note"]
            .as_str()
            .unwrap()
            .contains("1 of 2 rules in force"),
        "{identity}"
    );
    let listed = identity["unseated_rules"]["listed"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{identity}");
    let unseated = records
        .iter()
        .find(|f| f.content == "An unseated rule")
        .unwrap();
    assert_eq!(listed[0]["address"], unseated.address().to_string());
    assert_eq!(listed[0]["line"], "An unseated rule");
}

#[tokio::test]
async fn rules_index_never_cuts_an_unlabelled_rules_applicability() {
    let mut rule = unseated_rule().await;
    assert_eq!(line(&rule), "An unseated rule");
    rule.content = format!(
        "{} only when editing release workflows.",
        "Keep every assertion intact ".repeat(5)
    );
    let note = "Set a short subject label; recall this address for the whole rule.";
    assert_eq!(line(&rule), note);
    rule.fields
        .insert("subject".into(), "Before editing release workflows".into());
    assert_eq!(line(&rule), "Before editing release workflows");
}

#[tokio::test]
async fn rules_index_never_cuts_a_long_subject_label() {
    let mut rule = unseated_rule().await;
    rule.fields.insert("subject".into(), "界".repeat(80));
    assert_eq!(line(&rule), "界".repeat(80));
    rule.fields.insert("subject".into(), "界".repeat(81));
    assert_eq!(
        line(&rule),
        "Set a short subject label; recall this address for the whole rule."
    );
}

#[test]
fn rules_index_settings_label_is_engine_owned() {
    let jojobot = crate::harness::handler();
    let bot = EntityId("bot:gamma".into());
    let label = [("settings_record".into(), "true".into())].into();
    assert!(
        jojobot
            .refuses_a_hand_written_review_stamp(&bot, &label, &[])
            .is_some()
    );
    assert!(
        jojobot
            .refuses_a_hand_written_review_stamp(
                &bot,
                &Default::default(),
                &["settings_record".into()]
            )
            .is_some()
    );
    let setting = [("example_mode".into(), "on".into())].into();
    assert!(
        jojobot
            .refuses_a_hand_written_review_stamp(&bot, &setting, &[])
            .is_none()
    );
}

#[test]
fn rules_index_settings_label_cannot_be_smuggled_by_spaces() {
    let jojobot = crate::harness::handler();
    let bot = EntityId("bot:gamma".into());
    let label = [(" settings_record ".into(), "true".into())].into();
    assert!(
        jojobot
            .refuses_a_hand_written_review_stamp(&bot, &label, &[])
            .is_some()
    );
    assert!(
        jojobot
            .refuses_a_hand_written_review_stamp(
                &bot,
                &Default::default(),
                &[" settings_record ".into()]
            )
            .is_some()
    );
}

#[tokio::test]
async fn rules_index_engine_settings_are_marked_but_authored_sets_remain_rules() {
    use crate::harness::{json_of, writing_as};
    use crate::memory::testing::{add_args, capture_args, capture_ok};
    use crate::memory::{AddEntityArgs, CaptureArgs, UpdateEntityArgs};
    use jojobot_domain::memory::Memory;
    use rmcp::handler::server::wrapper::Parameters;

    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = handler_over(memory.clone());
    let created = json_of(
        &jojobot
            .add_entity(Parameters(AddEntityArgs {
                sets: Some([("example_mode".into(), "on".into())].into()),
                ..add_args("bot", "gamma", "Gamma rule index fixture")
            }))
            .await
            .unwrap(),
    );
    assert_ne!(created["status"], "blocked", "{created}");
    let gamma = EntityId("bot:gamma".into());
    let records = memory.recall(&gamma).await.unwrap();
    let first = records
        .iter()
        .find(|f| f.content == "Gamma rule index fixture")
        .unwrap();
    assert_eq!(
        first.fields.get("settings_record").map(String::as_str),
        Some("true")
    );
    for value in ["off", "on"] {
        let args: UpdateEntityArgs = serde_json::from_value(serde_json::json!({
            "handle": "bot:gamma", "sets": {"example_mode": value}, "sid": writing_as(&jojobot)
        }))
        .unwrap();
        let answer = json_of(&jojobot.update_entity(Parameters(args)).await.unwrap());
        assert_ne!(answer["status"], "blocked", "{answer}");
    }
    let records = memory.recall(&gamma).await.unwrap();
    let updates: Vec<_> = records
        .iter()
        .filter(|f| f.content == "Set example_mode on @bot:gamma.")
        .collect();
    assert_eq!(
        updates.len(),
        1,
        "repeated own settings still rewrite their claim"
    );
    assert_eq!(
        updates[0].fields.get("settings_record").map(String::as_str),
        Some("true")
    );
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some([("example_mode".into(), "on".into())].into()),
            ..capture_args(
                "bot:gamma",
                "Before editing workflows, keep every assertion.",
            )
        },
    )
    .await;
    let records = memory.recall(&gamma).await.unwrap();
    let rules = jojobot_domain::memory::rules_in_force(&records);
    assert_eq!(rules.len(), 1, "{rules:?}");
    assert_eq!(
        rules[0].content,
        "Before editing workflows, keep every assertion."
    );
    assert!(
        !memory
            .fields(&gamma)
            .await
            .unwrap()
            .contains_key("settings_record")
    );
}
