//! **A type stored with a describing key still answers a read** (rules 342 and
//! 466): `recall` by `answers_type`, bare and as the `of-type` view asks it, say what a thing
//! lacks, and are never refused for the stored declaration.

use crate::harness::*;
use crate::memory::testing::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::{DESCRIBING_KEYS, DeclaredType, Field, Fold, ValueType};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;
use std::sync::Arc;

use super::RecallArgs;
use crate::Jojobot;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Hit;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;

/// A pantry as an older build kept it: the describing `key` with a fold of its
/// own, and `donuts`.
fn stored_pantry(key: &str) -> DeclaredType {
    DeclaredType::new(
        "legacy-pantry",
        vec![
            Field {
                folds: Fold::Newest,
                ..Field::new(key, ValueType::Text)
            },
            Field::new("donuts", ValueType::Number),
        ],
    )
}

fn recall_args(value: serde_json::Value) -> RecallArgs {
    serde_json::from_value(value).expect("recall arguments")
}

/// **For each of the six keys: the stored pantry names the key as lacking, the
/// same calls over a clean stored type fit the thing, and the view agrees.**
#[tokio::test]
async fn a_stored_type_with_a_describing_key_answers_recall_by_type() {
    for key in DESCRIBING_KEYS {
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = handler_over(memory.clone());
        ensure(&jojobot, "thing:tau").await;
        jojobot
            .capture(Parameters(crate::memory::CaptureArgs {
                sets: Some(
                    [("donuts".to_string(), "2".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("thing:tau", "ate two")
            }))
            .await
            .expect("capture ok");
        let tau = memory
            .list_entities(None)
            .await
            .expect("list ok")
            .into_iter()
            .find(|e| e.id.as_str() == "thing:tau")
            .expect("tau was captured");
        // A type-only question is routed to the search port, so the port answers
        // with the thing it would find.
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::answering(vec![Hit::Entity {
                entity: tau,
                doc_id: "doc-tau".into(),
                edges: vec![],
                answers: None,
            }])),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        memory.stored_type_past_the_guard(stored_pantry(key));
        memory.stored_type_past_the_guard(DeclaredType::new(
            "legacy-clean",
            vec![Field::new("donuts", ValueType::Number)],
        ));

        for (what, call) in [
            ("answers_type", json!({"answers_type": "legacy-pantry"})),
            // What the of-type view expands to: it selects things and binds its
            // input into `answers_type`.
            (
                "the of-type view's own selection",
                json!({"kind": "thing", "answers_type": "legacy-pantry"}),
            ),
        ] {
            let found = json_of(
                &jojobot
                    .recall(Parameters(recall_args(call)))
                    .await
                    .unwrap_or_else(|e| panic!("{key}, {what}: {e:?}")),
            );
            assert_ne!(found["status"], "blocked", "{key}, {what}: {found}");
            let object = &found["objects"][0];
            assert_eq!(object["id"], "thing:tau", "{key}, {what}: {found}");
            assert!(
                object.to_string().contains(&format!("\"{key}\"")),
                "{key}, {what} names the key as lacking: {object}"
            );
        }
        let clean = json_of(
            &jojobot
                .recall(Parameters(recall_args(
                    json!({"kind": "thing", "answers_type": "legacy-clean"}),
                )))
                .await
                .expect("recall ok"),
        );
        assert_eq!(clean["objects"][0]["id"], "thing:tau", "{key}: {clean}");
        assert!(
            !clean["objects"][0]
                .to_string()
                .contains(&format!("\"{key}\"")),
            "{key}: a clean stored type lacks nothing: {clean}"
        );
    }
}
