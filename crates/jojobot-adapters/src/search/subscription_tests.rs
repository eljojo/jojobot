//! An index is subscribed to its writer without being owned by it: dropping the
//! index releases the writer, and a live index is still told of every write.

use jojobot_domain::memory::NewEntity;
use jojobot_domain::memory::testing::InMemoryMemory;

use super::*;

/// **Dropping every owner of an index releases the index, the fold under it and
/// the writer's queue.** Weak handles to each are kept; once the owners are
/// gone none upgrades. Paired with the live index, which hears a write and
/// finds it: without the pair the drop could pass over an index that never
/// subscribed.
#[tokio::test]
async fn a_dropped_index_releases_its_writer_and_a_live_one_still_hears_writes() {
    let store = IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens");
    store
        .add_entity(NewEntity::new(
            EntityId("person:homer".into()),
            "Zeppelin pilot Homer",
            "fixture",
        ))
        .await
        .expect("adds")
        .written()
        .expect("lands");
    store.writer().drain().await;
    let heard = store
        .index()
        .search(&SearchQuery::text("zeppelin"))
        .expect("search ok");
    assert!(
        !heard.is_empty(),
        "a live index did not hear the write: {heard:?}"
    );

    let index = Arc::downgrade(&store.index());
    let fold = Arc::downgrade(&store.state());
    let queue_alive = store.writer().alive_probe();
    drop(store);
    tokio::task::yield_now().await;
    assert!(index.upgrade().is_none(), "the index outlived its owners");
    assert!(fold.upgrade().is_none(), "the fold outlived its owners");
    assert!(!queue_alive(), "the writer outlived its owners");
}

#[derive(Default)]
struct Woken(std::sync::atomic::AtomicUsize);

impl jojobot_domain::push::Nudge for Woken {
    fn nudge(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// **The binary's stack — memory index, mail index, nudge — releases its writer
/// when every owner is dropped.** Weak handles to the writer's queue and to the
/// mail index are kept; once the owners are gone neither upgrades. Paired with
/// the live stack, which delivers a post to the mail index and wakes the nudge
/// for a post into a person's box: without the pair the drop could pass over
/// subscribers that were never attached.
#[tokio::test]
async fn a_dropped_mail_and_nudge_stack_releases_its_writer_and_a_live_one_still_hears_posts() {
    jojobot_domain::memory::kinds::load_shipped();
    let store = IndexedMemory::new(Arc::new(InMemoryMemory::booted())).expect("index opens");
    let inner = jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner();
    let person = jojobot_domain::mailbox::MailboxName("person-ned-flanders".into());
    inner
        .create_mailbox(
            &person,
            &jojobot_domain::memory::EntityId("person:ned-flanders".into()),
            None,
        )
        .await
        .expect("a box");
    let bots = jojobot_domain::mailbox::MailboxName("alpha".into());
    inner
        .create_mailbox(
            &bots,
            &jojobot_domain::memory::EntityId("bot:alpha".into()),
            None,
        )
        .await
        .expect("a bot's box");
    let (state, mail) = mail_over(Arc::new(inner), store.index());
    let woken = Arc::new(Woken::default());
    let nudge = crate::nudge::NudgeOnPost::subscribe(&store.writer(), state.clone(), woken.clone());

    let new = |to: jojobot_domain::mailbox::MailboxName, body: &str| {
        jojobot_domain::mailbox::NewMessage {
            mailbox: to,
            body: body.into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: jiff::Timestamp::UNIX_EPOCH,
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        }
    };
    state
        .post_message(new(bots, "the damper is still hand-cut"))
        .await
        .expect("the post into a bot's box is accepted");
    state
        .post_message(new(person, "a note for the person"))
        .await
        .expect("the post into a person's box is accepted");
    store.writer().drain().await;
    let heard = store
        .index()
        .search(&mail_fixtures::asking_everywhere("damper"))
        .expect("search ok");
    assert!(!heard.is_empty(), "the mail index did not hear the post");
    assert_eq!(
        woken.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the nudge did not wake for a post into a person's box"
    );

    let mail_index = Arc::downgrade(&mail);
    let nudge_alive = Arc::downgrade(&nudge);
    let queue_alive = store.writer().alive_probe();
    drop((store, state, mail, nudge, woken));
    tokio::task::yield_now().await;
    assert!(
        mail_index.upgrade().is_none(),
        "the mail index outlived its owners"
    );
    assert!(
        nudge_alive.upgrade().is_none(),
        "the nudge outlived its owners"
    );
    assert!(!queue_alive(), "the writer outlived its owners");
}
