//! **A delivery reads in the order of the moments messages were sent**, not in the
//! order the stored text of those moments sorts (rule 598). The stamp the store
//! keeps varies in length, and `:00.5Z` sorts before `:00Z` as text while it is
//! the later moment.

use super::*;
use crate::dolt::tests::free_port;
use crate::dolt::{Dolt, migrate};
use crate::testing::Scratch;
use jojobot_domain::mailbox::NewMessage;

struct AnyOwner;

#[async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

/// **Two messages whose stamps order differently as text and as moments come back
/// in moment order**, oldest first. The later moment is posted first, so neither
/// the order of posting nor the order of the text gives the answer.
#[tokio::test]
async fn a_delivery_reads_in_moment_order_whatever_the_text_of_the_stamps_sorts_to() {
    let scratch = Scratch::new("mail-moment-order");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    migrate::run(store.pool()).await.expect("the schema");
    migrate::seed_kinds(store.pool())
        .await
        .expect("the kinds are seeded");
    let mail = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
    mail.create_mailbox(
        &MailboxName("inbox".into()),
        &EntityId("bot:gamma".into()),
        None,
    )
    .await
    .expect("create ok")
    .written()
    .expect("not blocked");

    for (body, at) in [
        ("the later moment", "2026-01-02T00:00:00.5Z"),
        ("the earlier moment", "2026-01-02T00:00:00Z"),
    ] {
        mail.post_message(NewMessage {
            mailbox: MailboxName("inbox".into()),
            body: body.into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: at.parse().expect("a moment"),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("post ok")
        .written()
        .expect("not blocked");
    }
    // The setup the case rests on: the two stamps are stored as text that sorts
    // the other way round.
    let stored: Vec<String> = sqlx::query_scalar("SELECT sent_at FROM message ORDER BY sent_at")
        .fetch_all(store.pool())
        .await
        .expect("the stamps read");
    assert!(
        stored[0].contains(".5") && !stored[1].contains(".5"),
        "the text of the later moment sorts first: {stored:?}"
    );

    let delivery = mail
        .read_mailbox(
            &MailboxName("inbox".into()),
            TakenBy::Reading,
            jojobot_domain::mailbox::Acting::anyone_anytime(),
        )
        .await
        .expect("read ok")
        .written()
        .expect("not blocked");
    let order: Vec<&str> = delivery
        .messages
        .iter()
        .map(|d| d.message.body.as_str())
        .collect();
    assert_eq!(order, ["the earlier moment", "the later moment"]);
    store.stop().await;
}
