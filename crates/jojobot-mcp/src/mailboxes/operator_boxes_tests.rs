//! **A post to an operator who owns more than one box says the sentence the
//! operator's page says.** One function renders it, so the verb and the page cannot
//! tell a caller different things (rule 498).

use super::post_message::PostMessageArgs;
use super::testing::*;
use crate::harness::json_of;
use jojobot_domain::mailbox::MailboxName;
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn a_post_to_an_operator_owning_two_boxes_says_the_shared_sentence() {
    let jojobot = mailbox_handler();
    let sender = owning(&jojobot, "epsilon").await;
    name_the_operator(&jojobot, "milhouse").await;
    // The healthy post first: one box, and it lands.
    let landed = send(&jojobot, "person:milhouse", "epsilon", "the first note").await;
    assert_ne!(landed["status"], "blocked", "{landed}");

    jojobot
        .mailboxes
        .create_mailbox(
            &MailboxName("milhouse-spare".to_string()),
            &jojobot_domain::memory::EntityId("person:milhouse".to_string()),
            None,
        )
        .await
        .expect("a second box is written straight to the store");
    let names: Vec<String> = boxes_owned_by(&jojobot, "person:milhouse")
        .await
        .iter()
        .map(|name| name.as_str().to_string())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");

    let refused = json_of(
        &jojobot
            .post_message(Parameters(PostMessageArgs {
                request_id: None,
                to: "person:milhouse".into(),
                body: "the second note".into(),
                in_reply_to: None,
                subject: None,
                sid: sender,
                supersedes: None,
            }))
            .await
            .expect("a refusal is an answer"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["fix_by"], "person", "{refused}");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        how.contains(&crate::orientation::instance_zone::operator_owns_several_boxes(&names)),
        "the verb says the shared sentence: {how}"
    );
}
