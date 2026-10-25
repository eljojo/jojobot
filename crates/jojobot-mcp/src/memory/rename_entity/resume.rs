use super::*;
use jojobot_domain::request::Recorded;

/// A number can name the new handle after the rename commits. The mailbox
/// still belongs to the source the committing call recorded.
pub(super) fn source(handle: &str, recorded: &Recorded) -> Result<EntityId, CallToolResult> {
    let from = EntityId::person(handle);
    match &recorded.note {
        Some(source) => Ok(EntityId(source.clone())),
        None if jojobot_domain::memory::numbered::numbered_handle(&from).is_some() => {
            Err(crate::caller::handle_declined(
                "rename_entity",
                crate::answer::WayForward::person(format!(
                    "The rename was written under this request_id. For this older numbered \
                     rename receipt, {}.",
                    crate::request::LEGACY_RECEIPT_REPAIR
                )),
            ))
        }
        None => Ok(from),
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use jojobot_domain::request::{self, Recorded, Request, RequestId, Scope};
    use serde_json::json;

    #[tokio::test]
    async fn a_legacy_numbered_rename_receipt_requires_operator_identity_repair() {
        for (source, note, finishes) in [
            ("bot:delta", None, true),
            ("topic:alpha/2", None, false),
            ("topic:alpha/2", Some("bot:delta"), true),
        ] {
            let jojobot = handler();
            let sid = writing_as(&jojobot);
            let mut collection = add_args("topic", "alpha", "Alpha");
            collection.sets = Some([("numbers_by".into(), "number".into())].into());
            jojobot.add_entity(Parameters(collection)).await.unwrap();
            let mut member = add_args("bot", "delta", "Delta");
            member.parent = Some("topic:alpha".into());
            member.sets = Some([("number".into(), "2".into())].into());
            jojobot.add_entity(Parameters(member)).await.unwrap();
            // Stage the committed rename without its mailbox follow-up.
            jojobot
                .memory
                .rename_entity(
                    &EntityId("bot:delta".into()),
                    &EntityId("bot:epsilon".into()),
                    None,
                    jiff::civil::date(2026, 6, 6),
                    jojobot_domain::memory::Override::none(),
                    None,
                )
                .await
                .unwrap()
                .written()
                .unwrap();
            let scope = Scope::resuming(
                Request {
                    bot: EntityId("bot:otto".into()),
                    id: RequestId::parse("rename-legacy").unwrap(),
                    verb: "rename_entity".into(),
                    digest: request::digest_of("{}"),
                },
                vec![(
                    "rename".into(),
                    Recorded {
                        landed: json!({"handle": "bot:epsilon"}).to_string(),
                        note: note.map(str::to_string),
                    },
                )],
            );
            let args = serde_json::from_value(json!({
                "handle": source, "to": "bot:epsilon", "sid": sid
            }))
            .unwrap();
            let body = json_of(
                &request::within(scope.clone(), jojobot.rename_entity(Parameters(args)))
                    .await
                    .unwrap(),
            );
            let owner = if finishes {
                assert_ne!(body["status"], "blocked", "{source}, {note:?}: {body}");
                assert_eq!(body["mailbox"], "epsilon", "{body}");
                "bot:epsilon"
            } else {
                assert_eq!(body["status"], "blocked", "{body}");
                assert_eq!(body["fix_by"], "person", "{body}");
                assert_eq!(body["wrote"], false, "{body}");
                let way = body["how_to_proceed"].as_str().unwrap();
                for word in ["operator", "identity", "request", "numbered"] {
                    assert!(way.contains(word), "missing {word}: {way}");
                }
                "bot:delta"
            };
            assert_eq!(scope.was_finished(), finishes, "{body}");
            let boxes = jojobot.mailboxes.list_mailboxes().await.unwrap();
            assert_eq!(boxes.len(), 1, "{boxes:?}");
            assert_eq!(boxes[0].owner.as_str(), owner, "{boxes:?}");
        }
    }
}
