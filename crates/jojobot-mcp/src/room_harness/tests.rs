use super::*;
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;
use serde_json::json;

async fn room() -> Jojobot {
    let supplied = crate::provisions();
    let memory = Arc::new(
        IndexedMemory::new(Arc::new(jojobot_adapters::provisioned::Provisioned::new(
            InMemoryMemory::seeded().knowing(supplied.clone()),
            supplied,
        )))
        .expect("indexed memory"),
    );
    let index = memory.index();
    let mail = Arc::new(IndexedMailboxes::new(
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        index.clone(),
    ));
    let handler = Jojobot::new(
        memory.clone(),
        Arc::new(Retrieval::new(index, vec![memory, mail.clone()])),
        mail,
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        Arc::new(sid::SessionRegistry::new()),
    );
    crate::seed::ensure_default_identity(&handler.memory, &handler.mailboxes).await;
    assert!(
        handler
            .memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .expect("bots")
            .iter()
            .any(|bot| bot.id.as_str() == "bot:assistant")
    );
    handler
}

#[tokio::test]
async fn room_harness_reads_mail_without_an_account_or_a_state_change() {
    let handler = room().await;
    let bot = EntityId("bot:assistant".into());
    handler
        .mailboxes
        .create_mailbox(&MailboxName("assistant".into()), &bot, None)
        .await
        .expect("box");
    handler
        .mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName("assistant".into()),
            body: "a room brief".into(),
            subject: Some("the room's goal".into()),
            sender: "bot:assistant".into(),
            sent_at: handler.clock.now(),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("message");
    let before = handler.mailboxes.scan_messages().await.expect("mail");
    let grader = Harness::new(handler.clone());
    let read = grader
        .read(
            "search",
            json!({"query":"*", "include_mail":true,"limit":200}),
        )
        .await
        .expect("read");
    assert!(
        read["results"]
            .as_array()
            .expect("results")
            .iter()
            .any(|hit| hit["hit"] == "message"),
        "{read}"
    );
    assert_eq!(
        handler.mailboxes.scan_messages().await.expect("mail"),
        before
    );
    assert!(
        handler
            .sessions
            .all_summaries()
            .await
            .expect("runs")
            .is_empty()
    );
    let anonymous = body(
        handler
            .search(Parameters(
                decode(json!({"query":"*", "include_mail":true,"limit":200})).unwrap(),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        !anonymous["results"]
            .as_array()
            .expect("results")
            .iter()
            .any(|hit| hit["hit"] == "message"),
        "{anonymous}"
    );
}

#[tokio::test]
async fn room_harness_reads_preserve_the_occupants_boot_both_ways() {
    let handler = room().await;
    let grader = Harness::new(handler.clone());
    let bot = EntityId("bot:assistant".into());
    let held_names = || {
        handler
            .registry
            .held_runs_of(&bot)
            .into_iter()
            .map(|run| run.name)
            .collect::<Vec<_>>()
    };
    let before = handler.sessions.all_summaries().await.expect("runs");
    assert!(held_names().is_empty());
    grader
        .read("start_here", json!({"bot":"assistant", "brief":true}))
        .await
        .expect("snapshot");
    assert!(
        held_names().is_empty(),
        "the grader minted a run before the occupant booted"
    );
    assert_eq!(
        handler.sessions.all_summaries().await.expect("runs"),
        before
    );
    let boot = body(
        handler
            .start_here(Parameters(
                decode(json!({"bot":"assistant", "brief":true})).unwrap(),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    let key = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the occupant could not boot: {boot}"));
    let args = json!({"bot":"assistant", "sid":key, "brief":true});
    let mut expected = body(
        handler
            .start_here(Parameters(decode(args.clone()).unwrap()))
            .await
            .unwrap(),
    )
    .unwrap();
    let held_before = held_names();
    grader
        .read("start_here", json!({"bot":"assistant", "brief":true}))
        .await
        .expect("snapshot");
    assert_eq!(
        held_names(),
        held_before,
        "the grader changed a booted occupant's runs"
    );
    let mut after = body(
        handler
            .start_here(Parameters(decode(args).unwrap()))
            .await
            .unwrap(),
    )
    .unwrap();
    // A boot deliberately draws a new name and key. Nothing else is omitted.
    for answer in [&mut expected, &mut after] {
        for key in ["sid", "run"] {
            assert!(answer["session"][key].is_string(), "{answer}");
            answer["session"][key] = json!("drawn at this boot");
        }
    }
    assert_eq!(after, expected);
}
