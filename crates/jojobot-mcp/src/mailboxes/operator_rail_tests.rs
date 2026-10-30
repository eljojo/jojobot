//! **The operator's two actions are commands that judge the operator and the box
//! themselves, and the rail's read takes no delivery.** A page that holds only
//! these handles cannot reach a message that is not the operator's, and cannot
//! move one by looking.

use super::operator_rail::{commands, rail};
use super::testing::*;
use jojobot_domain::mailbox::{MessageId, MessageState};
use jojobot_domain::memory::EntityId;
use jojobot_query_api::{OperatorIs, Refused};

/// A handler whose operator is milhouse and who has been left a message by
/// epsilon, plus a message epsilon left gamma. Returns both ids.
async fn a_world(jojobot: &crate::Jojobot) -> (MessageId, MessageId) {
    owning(jojobot, "epsilon").await;
    owning(jojobot, "gamma").await;
    name_the_operator(jojobot, "milhouse").await;
    let theirs = send(jojobot, "person:milhouse", "epsilon", "for the operator").await;
    let others = send(jojobot, "gamma", "epsilon", "for gamma").await;
    (
        MessageId(theirs["id"].as_str().expect("an id").to_string()),
        MessageId(others["id"].as_str().expect("an id").to_string()),
    )
}

async fn state_of(jojobot: &crate::Jojobot, id: &MessageId) -> MessageState {
    jojobot
        .mailboxes
        .message_by_id(id)
        .await
        .expect("the board reads")
        .expect("the message exists")
        .state
}

#[tokio::test]
async fn opening_takes_delivery_of_the_one_message_and_names_the_operator() {
    let jojobot = mailbox_handler();
    let (theirs, others) = a_world(&jojobot).await;
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    let operator = acting.open(&theirs).await.expect("the operator's own mail");
    assert_eq!(operator, EntityId("person:milhouse".into()));
    assert_eq!(state_of(&jojobot, &theirs).await, MessageState::Read);
    assert_eq!(state_of(&jojobot, &others).await, MessageState::New);
}

#[tokio::test]
async fn marking_processed_keeps_the_note_and_leaves_the_rest_of_the_box() {
    let jojobot = mailbox_handler();
    let (theirs, _) = a_world(&jojobot).await;
    let second = send(&jojobot, "person:milhouse", "epsilon", "a second").await;
    let second = MessageId(second["id"].as_str().expect("an id").to_string());
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    acting
        .mark_processed(&theirs, Some("seen to"))
        .await
        .expect("the operator's own mail");
    let kept = jojobot
        .mailboxes
        .message_by_id(&theirs)
        .await
        .expect("reads")
        .expect("exists");
    assert_eq!(kept.state, MessageState::Processed);
    assert_eq!(kept.notes.as_deref(), Some("seen to"));
    assert_eq!(state_of(&jojobot, &second).await, MessageState::New);
}

#[tokio::test]
async fn a_message_in_a_bots_box_is_refused_and_changes_nothing() {
    let jojobot = mailbox_handler();
    let (_, others) = a_world(&jojobot).await;
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    assert_eq!(acting.open(&others).await, Err(Refused::NotTheirs));
    assert_eq!(
        acting.mark_processed(&others, None).await,
        Err(Refused::NotTheirs)
    );
    assert_eq!(state_of(&jojobot, &others).await, MessageState::New);
}

#[tokio::test]
async fn an_instance_with_no_operator_refuses_both_actions() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "gamma").await;
    let sent = send(&jojobot, "gamma", "gamma", "to myself").await;
    let id = MessageId(sent["id"].as_str().expect("an id").to_string());
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    assert_eq!(acting.open(&id).await, Err(Refused::NoOperator));
    assert_eq!(
        acting.mark_processed(&id, None).await,
        Err(Refused::NoOperator)
    );
}

#[tokio::test]
async fn an_operator_with_no_box_yet_refuses_both_actions() {
    let jojobot = mailbox_handler();
    owning(&jojobot, "epsilon").await;
    owning(&jojobot, "gamma").await;
    name_the_operator(&jojobot, "milhouse").await;
    // Nobody has written to the operator, so their box is not open.
    let sent = send(&jojobot, "gamma", "epsilon", "for gamma").await;
    let id = MessageId(sent["id"].as_str().expect("an id").to_string());
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    assert_eq!(acting.open(&id).await, Err(Refused::NoBox));
    assert_eq!(acting.mark_processed(&id, None).await, Err(Refused::NoBox));
    assert_eq!(state_of(&jojobot, &id).await, MessageState::New);
}

#[tokio::test]
async fn an_operator_owning_two_boxes_is_damage_and_not_a_choice() {
    let jojobot = mailbox_handler();
    let (theirs, _) = a_world(&jojobot).await;
    a_second_box_for_a_person(&jojobot, "milhouse", "milhouse-spare").await;
    let acting = commands(
        jojobot.memory.clone(),
        jojobot.mailboxes.clone(),
        jojobot_domain::clock::Clock::default(),
    );
    let refused = acting.open(&theirs).await.expect_err("damage");
    assert!(matches!(refused, Refused::SeveralBoxes(names) if names.len() == 2));
    assert_eq!(state_of(&jojobot, &theirs).await, MessageState::New);
}

#[tokio::test]
async fn reading_the_rail_moves_no_message() {
    let jojobot = mailbox_handler();
    let (theirs, others) = a_world(&jojobot).await;
    let read = rail(jojobot.memory.clone(), jojobot.mailboxes.clone());
    assert_eq!(
        read.operator().await.expect("the record reads"),
        OperatorIs::Person(EntityId("person:milhouse".into()))
    );
    let owners: Vec<String> = read
        .boxes()
        .await
        .expect("boxes")
        .into_iter()
        .map(|held| held.owner.as_str().to_string())
        .collect();
    for owner in ["bot:epsilon", "bot:gamma", "person:milhouse"] {
        assert!(
            owners.iter().any(|held| held == owner),
            "{owner}: {owners:?}"
        );
    }
    let ids: Vec<String> = read
        .messages()
        .await
        .expect("messages")
        .into_iter()
        .map(|message| message.id.as_str().to_string())
        .collect();
    for sent in [&theirs, &others] {
        assert!(
            ids.iter().any(|id| id == sent.as_str()),
            "{sent:?}: {ids:?}"
        );
    }
    assert_eq!(state_of(&jojobot, &theirs).await, MessageState::New);
    assert_eq!(state_of(&jojobot, &others).await, MessageState::New);
}

/// A second box written straight to the store for a person, as stored damage.
async fn a_second_box_for_a_person(jojobot: &crate::Jojobot, person: &str, name: &str) {
    jojobot
        .mailboxes
        .create_mailbox(
            &jojobot_domain::mailbox::MailboxName(name.to_string()),
            &EntityId(format!("person:{person}")),
            None,
        )
        .await
        .expect("a second box is written straight to the store");
}
