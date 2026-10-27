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

#[tokio::test]
async fn room_harness_seed_keeps_the_scenario_author_and_opens_no_harness_run() {
    let handler = room().await;
    let grader = Harness::new(handler.clone());
    grader.seed("bot:assistant", vec![
        SeedWrite::AsTheOccupant("add_entity".into(), json!({
            "kind":"person", "handle":"milhouse", "name":"Milhouse", "source":"the room"
        })),
        SeedWrite::AsTheOccupant("capture".into(), json!({
            "subject":"person:milhouse", "content":"Milhouse prefers blue", "provenance":"testimony", "sets":{"colour":"blue"}
        })),
        SeedWrite::AsTheOccupant("post_message".into(), json!({
            "to":"assistant", "subject":"the room's goal", "body":"Find the colour Milhouse prefers"
        })),
    ]).await.expect("furnished");
    let facts = handler
        .memory
        .recall(&EntityId("person:milhouse".into()))
        .await
        .expect("facts");
    assert_eq!(facts.len(), 1);
    assert_eq!(
        handler
            .memory
            .fields(&EntityId("person:milhouse".into()))
            .await
            .expect("fields")["colour"],
        "blue"
    );
    let history = handler
        .memory
        .claim_history(&facts[0].address())
        .await
        .expect("history");
    assert_eq!(
        history
            .last()
            .expect("the seed write")
            .bot
            .as_ref()
            .map(EntityId::as_str),
        Some("bot:assistant")
    );
    assert!(history.iter().all(|write| write.session.is_none()));
    let mail = handler.mailboxes.scan_messages().await.expect("mail");
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].sender, "bot:assistant");
    let events = handler
        .mailboxes
        .message_history(&mail[0].id)
        .await
        .expect("mail history");
    assert!(events.iter().all(|event| event.by_session.is_none()));
    assert!(
        handler
            .registry
            .held_runs_of(&EntityId("bot:assistant".into()))
            .is_empty()
    );
    assert!(
        handler
            .sessions
            .all_summaries()
            .await
            .expect("runs")
            .is_empty()
    );
}

#[tokio::test]
async fn room_harness_notices_resume_from_a_cursor_without_a_participant() {
    let handler = room().await;
    let grader = Harness::new(handler.clone());
    grader
        .seed(
            "bot:assistant",
            vec![SeedWrite::AsTheOccupant(
                "add_entity".into(),
                json!({
                    "kind":"person", "handle":"milhouse", "name":"Milhouse", "source":"the room"
                }),
            )],
        )
        .await
        .expect("furnished");
    let first = grader.changes(Value::Null).await.expect("initial position");
    assert_eq!(first["notices"], json!([]));
    grader.seed("bot:assistant", vec![SeedWrite::AsTheOccupant("capture".into(), json!({
        "subject":"person:milhouse", "content":"Milhouse prefers blue", "sets":{"colour":"blue"}
    }))]).await.expect("a scenario write");
    let changed = grader
        .changes(first["position"].clone())
        .await
        .expect("notices");
    assert!(
        changed["notices"]
            .as_array()
            .expect("notices")
            .iter()
            .any(|notice| notice["handle"] == "person:milhouse"
                && notice["claims_new"] == json!(["person:milhouse#f1"])),
        "{changed}"
    );
    let unchanged = grader
        .changes(changed["position"].clone())
        .await
        .expect("next position");
    assert_eq!(unchanged["notices"], json!([]));
    assert!(
        handler
            .registry
            .held_runs_of(&EntityId("bot:assistant".into()))
            .is_empty()
    );
    assert!(
        handler
            .sessions
            .all_summaries()
            .await
            .expect("runs")
            .is_empty()
    );
}

#[tokio::test]
async fn room_harness_regression_seed_keeps_due_arithmetic_and_derived_provenance() {
    for on_creation in [false, true] {
        let handler = room().await;
        let grader = Harness::new(handler.clone());
        let settings = json!({"cadence_days":"7", "counts_from":"2026-01-01"});
        let mut rhythm = json!({"kind":"rhythm", "handle":"beta", "name":"Beta",
            "source":"the room", "parent":"thing:alpha"});
        if on_creation {
            rhythm["sets"] = settings.clone();
        }
        let mut writes = vec![
            SeedWrite::AsTheOccupant(
                "add_entity".into(),
                json!({"kind":"thing",
                "handle":"alpha", "name":"Alpha", "source":"the room"}),
            ),
            SeedWrite::AsTheOccupant("add_entity".into(), rhythm),
        ];
        if !on_creation {
            writes.push(SeedWrite::AsTheOccupant(
                "capture".into(),
                json!({
                    "subject":"rhythm:beta", "content":"the stated rhythm",
                    "provenance":"testimony", "sets":settings
                }),
            ));
        }
        grader
            .seed("bot:assistant", writes)
            .await
            .expect("furniture");
        let subject = EntityId("rhythm:beta".into());
        let fields = handler.memory.fields(&subject).await.expect("settings");
        assert_eq!(fields.get("due_on").map(String::as_str), Some("2026-01-08"));
        let facts = handler.memory.recall(&subject).await.expect("facts");
        assert_eq!(facts[0].provenance, Provenance::Inference);
        assert!(
            handler
                .sessions
                .all_summaries()
                .await
                .expect("runs")
                .is_empty()
        );
        let boot = body(
            handler
                .start_here(Parameters(
                    decode(json!({
                        "bot":"assistant", "brief":true
                    }))
                    .unwrap(),
                ))
                .await
                .unwrap(),
        )
        .unwrap();
        let cleared = body(
            handler
                .update_fact(Parameters(
                    decode(json!({
                        "sid":boot["session"]["sid"], "address":facts[0].address().to_string(),
                        "clear_fields":["cadence_days", "advances_from", "counts_from"]
                    }))
                    .unwrap(),
                ))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_ne!(
            cleared["status"], "blocked",
            "the piano's seeded cadence can be cleared: {cleared}"
        );
        assert!(
            !handler
                .memory
                .fields(&subject)
                .await
                .expect("settings")
                .contains_key("due_on")
        );
    }
}

#[tokio::test]
async fn room_harness_regression_active_run_is_a_read_only_handoff() {
    let handler = room().await;
    let boot = body(
        handler
            .start_here(Parameters(
                decode(json!({
                    "bot":"assistant", "brief":true
                }))
                .unwrap(),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    let key = boot["session"]["sid"].as_str().expect("occupant key");
    handler
        .journal(Parameters(
            decode(json!({"sid":key,
                "entry":"The question is answered", "focus":"The completed work"
            }))
            .unwrap(),
        ))
        .await
        .expect("occupant journal");
    let before = handler.sessions.all_summaries().await.expect("runs");
    let grader = Harness::new(handler.clone());
    let seen = grader
        .read("start_here", json!({"bot":"assistant","brief":true}))
        .await
        .expect("snapshot");
    assert!(
        seen["session"]["choices"]
            .as_array()
            .expect("runs")
            .iter()
            .any(|run| run["working_on"] == "The completed work"),
        "{seen}"
    );
    assert_eq!(
        handler.sessions.all_summaries().await.expect("runs"),
        before
    );
}

#[tokio::test]
async fn room_harness_regression_document_fallback_reads_the_kept_answer() {
    let handler = room().await;
    let boot = body(
        handler
            .start_here(Parameters(
                decode(json!({
                    "bot":"assistant", "brief":true
                }))
                .unwrap(),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    let key = boot["session"]["sid"].as_str().expect("occupant key");
    let made = body(
        handler
            .add_entity(Parameters(
                decode(json!({"sid":key,
                    "kind":"document", "handle":"alpha", "name":"Alpha", "source":"the room"
                }))
                .unwrap(),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_ne!(made["status"], "blocked", "{made}");
    let written = body(handler.put_document(Parameters(decode(json!({"sid":key,
        "document":"document:alpha", "markdown":"# The answer\n\nMilhouse prefers blue.\n",
        "provenance":"testimony"
    })).unwrap())).await.unwrap()).unwrap();
    assert_ne!(written["status"], "blocked", "{written}");
    let grader = Harness::new(handler.clone());
    let seen = grader
        .read("read_document", json!({"document":"document:alpha"}))
        .await
        .expect("document fallback");
    assert!(
        seen.to_string().contains("Milhouse prefers blue."),
        "{seen}"
    );
}
