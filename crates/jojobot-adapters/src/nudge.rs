//! **Wakes the push after a post lands in a person's box, whichever caller
//! posted.** The writer publishes every post once, so the wake is made where the
//! State changes and not beside each caller that happens to post to the
//! operator. A path added later cannot miss it.
//!
//! A post into a bot's box is published and is not a person's, so it nudges
//! nothing. A post the store blocked and a post that failed publish the position
//! and no post, so they nudge nothing either. When the box's owner cannot be read
//! the push is nudged anyway: a wake with nothing to send costs a look at the
//! outbox, and a missed wake leaves a code to expire unannounced.

use std::sync::Arc;

use async_trait::async_trait;
use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::push::Nudge;

use crate::writer::Writer;
use crate::writer::publish::{Change, Subscriber};

pub struct NudgeOnPost {
    mailboxes: Arc<dyn Mailboxes>,
    push: Arc<dyn Nudge>,
}

impl NudgeOnPost {
    /// Read `writer`'s posts, and tell `push` about each one into a person's box.
    pub fn subscribe(writer: &Writer, mailboxes: Arc<dyn Mailboxes>, push: Arc<dyn Nudge>) {
        writer.subscribe(Arc::new(NudgeOnPost { mailboxes, push }));
    }

    async fn is_a_persons(&self, name: &MailboxName) -> bool {
        match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes
                .iter()
                .any(|held| &held.name == name && held.is_private()),
            Err(_) => true,
        }
    }
}

#[async_trait]
impl Subscriber for NudgeOnPost {
    async fn changed(&self, changes: &[Change]) {
        for change in changes {
            let Change::Posted(to) = change else { continue };
            if self.is_a_persons(to).await {
                self.push.nudge();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use jiff::Timestamp;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{Guarded, NewMessage};
    use jojobot_domain::memory::EntityId;

    use super::*;
    use crate::fold::mail::FoldedMailboxes;

    #[derive(Default)]
    struct Counted(AtomicUsize);

    impl Nudge for Counted {
        fn nudge(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn new(to: &str) -> NewMessage {
        NewMessage {
            mailbox: MailboxName(to.into()),
            body: "a body".into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        }
    }

    /// **A post wakes the push when it lands in a person's box, and only then.**
    /// Each negative sits beside the positive it depends on: the post into the
    /// person's box is counted first, so a subscriber that never nudges and one
    /// that always nudges are each told apart.
    #[tokio::test]
    async fn only_a_post_that_lands_in_a_persons_box_nudges_the_push() {
        jojobot_domain::memory::kinds::load_shipped();
        let store = InMemoryMailboxes::knowing_any_owner().pushing();
        for (name, owner) in [
            ("person-ned-flanders", "person:ned-flanders"),
            ("epsilon", "bot:epsilon"),
        ] {
            store
                .create_mailbox(&MailboxName(name.into()), &EntityId(owner.into()), None)
                .await
                .expect("a box");
        }
        let writer = Writer::default();
        let boxes = Arc::new(FoldedMailboxes::new(Arc::new(store)).with_writer(writer.clone()));
        let nudges = Arc::new(Counted::default());
        NudgeOnPost::subscribe(&writer, boxes.clone(), nudges.clone());

        let landed = boxes
            .post_message(new("person-ned-flanders"))
            .await
            .expect("the post is accepted");
        assert!(matches!(landed, Guarded::Written(_)), "{landed:?}");
        assert_eq!(
            nudges.0.load(Ordering::SeqCst),
            1,
            "a person's box nudges once"
        );

        let to_a_bot = boxes
            .post_message(new("epsilon"))
            .await
            .expect("the post is accepted");
        assert!(matches!(to_a_bot, Guarded::Written(_)), "{to_a_bot:?}");
        assert_eq!(
            nudges.0.load(Ordering::SeqCst),
            1,
            "a bot's box nudges nothing"
        );

        let to_nobody = boxes.post_message(new("nobody-here")).await;
        assert!(
            !matches!(to_nobody, Ok(Guarded::Written(_))),
            "{to_nobody:?}"
        );
        assert_eq!(
            nudges.0.load(Ordering::SeqCst),
            1,
            "a post that did not land nudges nothing"
        );
    }
}
