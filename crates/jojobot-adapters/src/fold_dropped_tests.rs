//! **Every write verb of every fold, dropped after the store committed, leaves
//! the next read correct (decision log 292, rule 498).**
//!
//! The writer owns each accepted write through publication. These cases end
//! the caller's wait after the store commits, then release the store's answer
//! and drain the writer before comparing every held read with the source.
//! Bypassing ownership cuts publication and makes the corresponding row fail.
//!
//! The same tables run over the real store (see the end of this file), where
//! the commit is a real one and a slow write must not be cut off before it lands.
//!
//! Each row starts with a published image and proves that the store changed;
//! a call that wrote nothing cannot pass for a cancelled write.

use std::future::Future;
use std::sync::Arc;

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::mailbox::{Acting, MailboxName, Mailboxes, MessageId, NewMessage, TakenBy};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::testing::contract::{state, state_patch};
use jojobot_domain::memory::types::{DeclaredType, Field, Origin, ValueType};
use jojobot_domain::memory::{
    EntityId, EntityPatch, FactPatch, Memory, NewEntity, NewFact, Override, Provenance,
};
use jojobot_domain::session::history::WriteStamp;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::session::{
    CharterChange, NewEntry, NewSession, SessionId, SessionState, Sessions, Sid, WrapWindow,
};
use jojobot_domain::teaching::Teachings;
use jojobot_domain::teaching::testing::InMemoryTeachings;

use crate::counted::Counted;
use crate::counted_rails::{CountedMailboxes, CountedSessions, CountedTeachings};
use crate::fold::Folded;
use crate::fold::mail::FoldedMailboxes;
use crate::fold::sessions::FoldedSessions;
use crate::fold::teachings::FoldedTeachings;
use crate::fold_battery::Probe;
use crate::fold_rails_battery::{MailRails, SessionRails};

/// **Drops a call once it has committed and hung.** `hung` resolves when the
/// wrapper in front of the store has let the store commit and stopped answering.
/// A timeout in its place would drop a slow store's write before the commit and
/// prove nothing about the moment after it. Whether the call was dropped: false
/// is a call that ended on its own, which no verb under test may do.
async fn dropped_once_hung<T>(
    hung: impl Future<Output = ()>,
    call: impl Future<Output = T>,
) -> bool {
    cancelled_after_commit(hung, call).await.is_err()
}

/// The caller ends at the commit signal; the fixture releases its owned write
/// separately, so cancellation cannot stand in for publication completing.
pub(crate) async fn cancelled_after_commit<T>(
    hung: impl Future<Output = ()>,
    call: impl Future<Output = T>,
) -> Result<T, ()> {
    tokio::select! {
        value = call => Ok(value),
        () = hung => Err(()),
    }
}

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
}

// ── memory ───────────────────────────────────────────────────────────────────

/// Everything the memory store answers that a write can change.
async fn memory_holds(direct: &dyn Memory) -> String {
    let things = direct.list_entities(None).await.expect("the store lists");
    let mut held = format!(
        "{things:?}|{:?}|{:?}|{:?}|{:?}",
        direct.scan().await,
        direct.former_handles().await,
        direct.declared_types().await,
        direct.declared_kinds().await,
    );
    for thing in &things {
        held += &format!(
            "|{:?}|{:?}",
            direct.recall(&thing.id).await,
            direct.fields(&thing.id).await,
        );
    }
    held
}

struct MemoryRow<'a> {
    folded: &'a Folded,
    counted: &'a Counted,
    probe: &'a Probe<'a>,
}

impl MemoryRow<'_> {
    /// **One verb, dropped after the store committed it.** The image is filled
    /// first, the call commits and hangs, and cancellation ends the caller's
    /// wait. After release and drain, the battery compares the held reads.
    async fn dropped<T>(&self, verb: &'static str, call: impl Future<Output = T>) {
        self.folded
            .rebuild()
            .await
            .expect("the image is full before the write");
        let before = memory_holds(self.probe.direct).await;
        self.counted.hang_after_next(verb);
        let cancelled = dropped_once_hung(self.counted.hung(), call).await;
        assert!(cancelled, "{verb}: the call is cancelled");
        self.counted.release_hung();
        self.folded.writer().drain().await;
        assert_ne!(
            before,
            memory_holds(self.probe.direct).await,
            "{verb}: the call did not commit in the store"
        );
        // The battery also holds the change signal to move after every write the
        // image followed, including type and kind declarations.
        *self.probe.last_signal.lock().expect("signal lock") = None;
        self.probe.agree(&format!("after a dropped {verb}")).await;
    }

    /// A setup write that is not under test: it is followed to its end.
    async fn add(&self, handle: &str, name: &str, parent: Option<&str>) {
        self.folded
            .add_entity(NewEntity {
                parent: parent.map(id),
                ..NewEntity::new(id(handle), name, "fold-dropped")
            })
            .await
            .expect("add_entity")
            .written()
            .expect("not blocked");
    }

    async fn say(&self, handle: &str, content: &str) -> jojobot_domain::memory::Fact {
        let mut fact = state(NewFact::about(id(handle), content, date(2026, 9, 1)));
        fact.session = Some("fold-dropped".into());
        self.folded
            .capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
    }
}

/// A claim for a row that drops the write of one.
fn claim(handle: &str, content: &str) -> NewFact {
    let mut fact = state(NewFact::about(id(handle), content, date(2026, 9, 2)));
    fact.session = Some("fold-dropped".into());
    fact
}

/// **Each of the fifteen write verbs of the memory image, dropped after the
/// store committed.**
#[tokio::test]
async fn a_memory_write_dropped_after_the_store_committed_leaves_the_image_correct() {
    jojobot_domain::memory::kinds::load_shipped();
    every_memory_write_dropped(Arc::new(InMemoryMemory::booted())).await;
}

/// The memory table over any store: every verb dropped after it committed.
async fn every_memory_write_dropped(store: Arc<dyn Memory>) {
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the image loads");
    // Nothing is switched: the image is dropped by a dropped write, so a switched
    // read would reach the store, and what is held to the store here is the
    // answer.
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: &[],
        kept_off_misses_are_empty: false,
        last_signal: Default::default(),
    };
    let row = MemoryRow {
        folded: &folded,
        counted: &counted,
        probe: &probe,
    };
    let caller = id("bot:sigma");

    row.add("thing:delta", "Delta Station", None).await;
    row.add("person:homer", "Homer", None).await;
    row.add("person:bart", "Bart", None).await;
    row.add("work:gamma", "Gamma job", Some("thing:delta"))
        .await;

    row.dropped(
        "add_entity",
        folded.add_entity(NewEntity::new(
            id("place:springfield"),
            "Springfield",
            "fold-dropped",
        )),
    )
    .await;

    row.dropped(
        "add_entity_with_first_claim",
        folded.add_entity_with_first_claim(
            NewEntity::new(id("work:zeta"), "Zeta job", "fold-dropped"),
            claim("work:zeta", "zeta, born with a claim"),
        ),
    )
    .await;

    row.dropped(
        "capture",
        folded.capture(claim("person:homer", "homer likes donuts")),
    )
    .await;

    let held = row.say("work:gamma", "gamma is under way").await;
    row.dropped(
        "update_fact",
        folded.update_fact(
            &held.address(),
            state_patch(FactPatch {
                content: Some("gamma is nearly done".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            }),
            &caller,
        ),
    )
    .await;

    let doomed = row.say("person:bart", "bart wrote it down wrong").await;
    row.dropped(
        "retract",
        folded.retract(
            &doomed.address(),
            Some("written wrong"),
            date(2026, 9, 5),
            &caller,
            Some("fold-dropped"),
        ),
    )
    .await;

    row.dropped(
        "update_entity",
        folded.update_entity(
            &id("work:gamma"),
            EntityPatch {
                name: Some("Gamma errand".into()),
                ..Default::default()
            },
        ),
    )
    .await;

    row.dropped(
        "update_entity_with_claim",
        folded.update_entity_with_claim(
            &id("work:zeta"),
            EntityPatch {
                name: Some("Zeta errand".into()),
                ..Default::default()
            },
            claim("work:zeta", "zeta is renamed with a claim"),
        ),
    )
    .await;

    row.dropped(
        "set_prose",
        folded.set_prose(&id("person:homer"), "Homer, and what he is for.", None),
    )
    .await;

    row.dropped(
        "rename_entity",
        folded.rename_entity(
            &id("work:gamma"),
            &id("work:tau"),
            None,
            date(2026, 9, 4),
            Override::none(),
            None,
        ),
    )
    .await;

    row.dropped(
        "archive_entity",
        folded.archive_entity(&id("person:bart"), "not needed", None),
    )
    .await;
    row.dropped(
        "restore_entity",
        folded.restore_entity(&id("person:bart"), None),
    )
    .await;

    row.dropped(
        "merge",
        folded.merge(
            &id("person:bart"),
            &id("person:homer"),
            Some("the same person"),
            date(2026, 9, 8),
            &caller,
            Some("fold-dropped"),
        ),
    )
    .await;

    row.dropped(
        "declare_type",
        folded.declare_type(
            DeclaredType::new(
                "fold-ranked",
                vec![Field::required("rank", ValueType::Number)],
            ),
            None,
        ),
    )
    .await;

    row.dropped(
        "declare_kind",
        folded.declare_kind(
            "fold-kind",
            Origin::Declared,
            vec![Field::required("rank", ValueType::Number)],
        ),
    )
    .await;

    // Only a kind the store holds as shipped is taken back.
    folded
        .declare_kind(
            "fold-shipped-kind",
            Origin::Shipped,
            vec![Field::required("rank", ValueType::Number)],
        )
        .await
        .expect("declare_kind");
    row.dropped("reclaim_kind", folded.reclaim_kind("fold-shipped-kind"))
        .await;
}

// ── runs ─────────────────────────────────────────────────────────────────────

/// Everything the sessions store answers that a write can change.
async fn runs_hold(direct: &dyn Sessions) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        direct.all_sessions().await,
        direct.charter_changes().await,
        direct.call_counts().await,
        direct.served_days().await
    )
}

struct RunRow<'a> {
    folded: &'a FoldedSessions,
    counted: &'a CountedSessions,
    rails: &'a SessionRails<'a>,
}

impl RunRow<'_> {
    async fn dropped<T>(&self, verb: &'static str, call: impl Future<Output = T>) {
        self.folded
            .rebuild()
            .await
            .expect("the image is full before the write");
        let before = runs_hold(self.rails.direct).await;
        self.counted.calls.hang_after_next(verb);
        let cancelled = dropped_once_hung(self.counted.calls.hung(), call).await;
        assert!(cancelled, "{verb}: the call is cancelled");
        self.counted.calls.release_hung();
        self.folded.wait_for_writes().await;
        assert_ne!(
            before,
            runs_hold(self.rails.direct).await,
            "{verb}: the call did not commit in the store"
        );
        self.rails.agree(&format!("after a dropped {verb}")).await;
    }
}

fn new_run(slug: &str, sid: &str, run: &str, second: i64) -> NewSession {
    NewSession {
        bot: id(&format!("bot:{slug}")),
        sid: Sid(sid.into()),
        id: Some(SessionId(run.into())),
        key_hash: Some(format!("hash-of-{sid}")),
        focus: format!("{slug} starts"),
        started_at: at(second),
        timezone: Some("America/New_York".into()),
        started_on: Some(date(2026, 9, 1)),
    }
}

/// **Each of the fifteen write verbs of the runs image, dropped after the store
/// committed.**
#[tokio::test]
async fn a_run_write_dropped_after_the_store_committed_leaves_the_image_correct() {
    let store = Arc::new(InMemorySessions::new());
    every_run_write_dropped(
        store.clone(),
        store.write_stamp(),
        crate::writer::Writer::default(),
    )
    .await;
}

/// The runs table over any store, given the stamp the store reads its moment from.
async fn every_run_write_dropped(
    store: Arc<dyn Sessions>,
    stamp: WriteStamp,
    writer: crate::writer::Writer,
) {
    let counted = Arc::new(CountedSessions::new(store.clone()));
    let folded = FoldedSessions::new(counted.clone(), stamp, Clock::Real).with_writer(writer);
    folded.rebuild().await.expect("the image loads");
    let rails = SessionRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: &[],
    };
    let row = RunRow {
        folded: &folded,
        counted: &counted,
        rails: &rails,
    };

    // The runs the rows work on, begun and followed in full.
    let gamma = folded
        .begin(new_run("gamma", "g4mm", "ggggg1", 10))
        .await
        .expect("begin");
    folded
        .begin(new_run("marge", "m4rg", "mmmmm1", 11))
        .await
        .expect("begin");
    folded
        .begin(new_run("bart", "b4rt", "bbbbb1", 12))
        .await
        .expect("begin");

    row.dropped("begin", folded.begin(new_run("lisa", "k15a", "lllll1", 15)))
        .await;

    row.dropped(
        "append",
        folded.append(
            &gamma.id,
            NewEntry::manual("homer eats a donut", at(30), Some(date(2026, 9, 1))),
        ),
    )
    .await;

    row.dropped(
        "amend_last",
        folded.amend_last(&gamma.id, "homer eats two donuts"),
    )
    .await;

    let beat = folded
        .append(
            &gamma.id,
            NewEntry::beat("memory", "homer wrote 1 time", at(40), None),
        )
        .await
        .expect("append a beat");
    row.dropped(
        "amend_beat",
        folded.amend_beat(&gamma.id, &beat.id, "homer wrote 3 times", at(50)),
    )
    .await;

    row.dropped("set_focus", folded.set_focus(&gamma.id, "homer naps"))
        .await;
    row.dropped(
        "set_timezone",
        folded.set_timezone(&gamma.id, Some("Europe/Berlin")),
    )
    .await;
    row.dropped(
        "set_stated_day",
        folded.set_stated_day(&gamma.id, Some(date(2026, 9, 2))),
    )
    .await;
    row.dropped(
        "add_served",
        folded.add_served(&gamma.id, 120, jiff::civil::date(2026, 9, 2)),
    )
    .await;
    row.dropped("reset_served", folded.reset_served(&gamma.id))
        .await;
    row.dropped(
        "count_call",
        folded.count_call(&jojobot_domain::usage::CallKey {
            bot: Some(id("bot:gamma")),
            day: date(2026, 9, 2),
            verb: "recall".to_string(),
            outcome: jojobot_domain::usage::CallOutcome::Landed,
            run: None,
        }),
    )
    .await;
    row.dropped(
        "set_key_hash",
        folded.set_key_hash(
            &gamma.id,
            "another-hash",
            &jojobot_domain::session::KeyTrail::default(),
        ),
    )
    .await;
    row.dropped(
        "set_charter_told",
        folded.set_charter_told(&gamma.id, at(70)),
    )
    .await;
    row.dropped(
        "note_charter_change",
        folded.note_charter_change(
            &id("bot:gamma"),
            CharterChange {
                by: id("bot:lisa"),
                at: at(80),
            },
            None,
        ),
    )
    .await;

    // A run abandoned and taken back, then one wrapped with its window opened.
    let marge = SessionId("mmmmm1".into());
    row.dropped("close", folded.close(&marge, SessionState::Abandoned))
        .await;
    row.dropped("reopen", folded.reopen(&marge)).await;

    let bart = SessionId("bbbbb1".into());
    folded
        .append(
            &bart,
            NewEntry::manual("bart wraps", at(100), None).closing(Some("bart starts".into())),
        )
        .await
        .expect("closing entry");
    folded
        .close(&bart, SessionState::Wrapped)
        .await
        .expect("wrap");
    row.dropped(
        "set_wrap_window",
        folded.set_wrap_window(&bart, Some(WrapWindow::Open("wc-code".into()))),
    )
    .await;
}

// ── mail ─────────────────────────────────────────────────────────────────────

/// Everything the mail store answers that a write can change.
async fn board_holds(direct: &dyn Mailboxes) -> String {
    format!(
        "{:?}|{:?}|{:?}",
        direct.list_mailboxes().await,
        direct.scan_messages().await,
        direct.scan_message_events().await,
    )
}

struct BoardRow<'a> {
    folded: &'a FoldedMailboxes,
    counted: &'a CountedMailboxes,
    rails: &'a MailRails<'a>,
}

impl BoardRow<'_> {
    async fn dropped<T>(&self, verb: &'static str, call: impl Future<Output = T>) {
        self.folded
            .rebuild()
            .await
            .expect("the image is full before the write");
        let before = board_holds(self.rails.direct).await;
        self.counted.calls.hang_after_next(verb);
        let cancelled = dropped_once_hung(self.counted.calls.hung(), call).await;
        assert!(cancelled, "{verb}: the call is cancelled");
        self.counted.calls.release_hung();
        self.folded.wait_for_writes().await;
        assert_ne!(
            before,
            board_holds(self.rails.direct).await,
            "{verb}: the call did not commit in the store"
        );
        self.rails.agree(&format!("after a dropped {verb}")).await;
    }

    /// A setup post that is not under test: it is followed to its end.
    async fn post(&self, to: &str, sender: &str, body: &str, second: i64) -> MessageId {
        self.folded
            .post_message(message(to, sender, body, second))
            .await
            .expect("post")
            .written()
            .expect("not blocked")
            .id
    }
}

fn name(n: &str) -> MailboxName {
    MailboxName(n.into())
}

fn message(to: &str, sender: &str, body: &str, second: i64) -> NewMessage {
    NewMessage {
        mailbox: name(to),
        body: body.into(),
        subject: Some(format!("about {body}")),
        sender: sender.into(),
        sent_at: at(second),
        in_reply_to: None,
        supersedes: None,
        sender_mail_waiting_at_send: Some(0),
        posted_by_session: Some("h0m3".into()),
    }
}

/// **Each of the seven write verbs of the mail image, dropped after the store
/// committed.**
#[tokio::test]
async fn a_mail_write_dropped_after_the_store_committed_leaves_the_image_correct() {
    jojobot_domain::memory::kinds::load_shipped();
    every_mail_write_dropped(
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        crate::writer::Writer::default(),
    )
    .await;
}

/// The mail table over any store whose owners all resolve.
async fn every_mail_write_dropped(store: Arc<dyn Mailboxes>, writer: crate::writer::Writer) {
    let counted = Arc::new(CountedMailboxes::new(store.clone()));
    let folded = FoldedMailboxes::new(counted.clone()).with_writer(writer);
    folded.rebuild().await.expect("the image loads");
    let rails = MailRails {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: &[],
    };
    let row = BoardRow {
        folded: &folded,
        counted: &counted,
        rails: &rails,
    };

    for (n, owner) in [("gamma", "bot:gamma"), ("delta", "bot:delta")] {
        folded
            .create_mailbox(&name(n), &id(owner), None)
            .await
            .expect("create")
            .written()
            .expect("not blocked");
    }
    // The mail the rows act on, posted and followed in full.
    row.post("gamma", "delta", "for the delivery", 10).await;
    let taken = row.post("delta", "gamma", "to take by id", 20).await;
    let processed = row.post("delta", "gamma", "to process", 30).await;
    let doubtful = row.post("delta", "gamma", "to set aside", 40).await;

    row.dropped(
        "create_mailbox",
        folded.create_mailbox(&name("epsilon"), &id("bot:epsilon"), None),
    )
    .await;

    row.dropped(
        "post_message",
        folded.post_message(message(
            "gamma",
            "delta",
            "written, then the caller left",
            50,
        )),
    )
    .await;

    row.dropped(
        "read_mailbox",
        folded.read_mailbox(&name("gamma"), TakenBy::Reading, Acting::anyone_anytime()),
    )
    .await;

    row.dropped(
        "read_message",
        folded.read_message(&taken, Acting::anyone_anytime()),
    )
    .await;

    row.dropped(
        "mark_processed",
        folded.mark_processed(
            &processed,
            &name("delta"),
            Some("done"),
            Acting::anyone_anytime(),
        ),
    )
    .await;

    row.dropped(
        "quarantine",
        folded.quarantine(
            &doubtful,
            &name("delta"),
            "not trusted",
            Acting::unnamed(at(80)),
        ),
    )
    .await;

    row.dropped(
        "repoint_owner",
        folded.repoint_owner(&id("bot:gamma"), &id("bot:psi")),
    )
    .await;
}

// ── teachings ────────────────────────────────────────────────────────────────

/// **Both write verbs of the teaching ledger, dropped after the store
/// committed.** A contact that committed and was dropped must not be offered as
/// a first one again, and a forget that committed must not leave the pair
/// held.
#[tokio::test]
async fn a_teaching_write_dropped_after_the_store_committed_leaves_the_ledger_correct() {
    every_teaching_write_dropped(Arc::new(InMemoryTeachings::new())).await;
}

/// The teaching table over any store.
async fn every_teaching_write_dropped(store: Arc<dyn Teachings>) {
    let counted = Arc::new(CountedTeachings::new(store.clone()));
    let folded = FoldedTeachings::new(counted.clone());
    folded.rebuild().await.expect("the ledger loads");
    let sid = Sid("s001".into());
    let other = Sid("s002".into());

    counted.calls.hang_after_next("first_contact");
    let cancelled = dropped_once_hung(
        counted.calls.hung(),
        folded.first_contact(&sid, "claims", at(1)),
    )
    .await;
    assert!(cancelled, "first_contact: the call is cancelled");
    counted.calls.release_hung();
    folded.wait_for_writes().await;
    assert_eq!(
        store.taught().await.expect("the store answers"),
        Some(vec![(sid.clone(), "claims".to_string())]),
        "first_contact: the call did not commit in the store"
    );
    // The owner has published before this repeat. A lost caller must not make a
    // known contact read the ledger again.
    counted.calls.reset();
    assert!(
        !folded
            .first_contact(&sid, "claims", at(2))
            .await
            .expect("answers"),
        "first_contact: the store holds the pair, so the next contact is not a first one"
    );
    assert_eq!(
        counted.calls.reads(),
        0,
        "first_contact: the owned writer kept the ledger, so the repeat needs no read: {:?}",
        counted.calls.seen()
    );

    folded.rebuild().await.expect("the ledger is full");
    assert!(
        folded
            .first_contact(&other, "claims", at(3))
            .await
            .expect("answers"),
        "a second session is a first contact"
    );
    counted.calls.hang_after_next("forget");
    let cancelled = dropped_once_hung(counted.calls.hung(), folded.forget(&sid)).await;
    assert!(cancelled, "forget: the call is cancelled");
    counted.calls.release_hung();
    folded.wait_for_writes().await;
    assert_eq!(
        store.taught().await.expect("the store answers"),
        Some(vec![(other.clone(), "claims".to_string())]),
        "forget: the call did not commit in the store"
    );
    assert!(
        folded
            .first_contact(&sid, "claims", at(4))
            .await
            .expect("answers"),
        "forget: the store forgot the session, so its next contact is a first one"
    );
}

// ── over the real store ──────────────────────────────────────────────────────

/// **The same tables, over the store production runs.** The wrapper commits in
/// the real store and then hangs, so the drop falls after a real commit. The
/// guard sits above the store, and these cases hold that to the store's own
/// answers rather than to a fake's.
mod over_the_real_store {
    use async_trait::async_trait;
    use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};

    use super::*;
    use crate::dolt::mailboxes::DoltMailboxes;
    use crate::dolt::memory::DoltMemory;
    use crate::dolt::sessions::DoltSessions;
    use crate::dolt::teaching::DoltTeachings;
    use crate::dolt::tests::free_port;
    use crate::dolt::{Dolt, migrate};
    use crate::testing::Scratch;
    use crate::testing::migrated_database;

    /// Every well-formed owner resolves: the table is about the board, not about
    /// who may own a box.
    struct AnyOwner;

    #[async_trait]
    impl OwnerIndex for AnyOwner {
        async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
            Ok(OwnerLookup::Known)
        }
    }

    /// A store of its own, migrated and seeded with the shipped kinds.
    async fn database(server: &Dolt, what: &str) -> sqlx::MySqlPool {
        let pool = migrated_database(server, what)
            .await
            .expect("a database of its own");
        migrate::run(&pool).await.expect("the schema");
        jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
            .await
            .expect("the kinds are seeded");
        pool
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_memory_write_dropped_after_the_real_store_committed_leaves_the_image_correct() {
        let scratch = Scratch::new("dropped-memory");
        let mut server = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = database(&server, "dropped_memory").await;
        jojobot_domain::memory::kinds::load_shipped();
        every_memory_write_dropped(Arc::new(DoltMemory::open(pool))).await;
        server.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_run_write_dropped_after_the_real_store_committed_leaves_the_image_correct() {
        let scratch = Scratch::new("dropped-runs");
        let mut server = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = database(&server, "dropped_runs").await;
        let stamp = WriteStamp::new();
        let writer = crate::writer::Writer::default();
        every_run_write_dropped(
            Arc::new(
                DoltSessions::open(pool)
                    .stamped_by(stamp.clone())
                    .with_writer(writer.clone()),
            ),
            stamp,
            writer,
        )
        .await;
        server.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_mail_write_dropped_after_the_real_store_committed_leaves_the_image_correct() {
        let scratch = Scratch::new("dropped-mail");
        let mut server = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = database(&server, "dropped_mail").await;
        jojobot_domain::memory::kinds::load_shipped();
        let writer = crate::writer::Writer::default();
        every_mail_write_dropped(
            Arc::new(DoltMailboxes::open(pool, Arc::new(AnyOwner)).with_writer(writer.clone())),
            writer,
        )
        .await;
        server.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_teaching_write_dropped_after_the_real_store_committed_leaves_the_ledger_correct() {
        let scratch = Scratch::new("dropped-teachings");
        let mut server = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = database(&server, "dropped_teachings").await;
        every_teaching_write_dropped(Arc::new(DoltTeachings::open(pool))).await;
        server.stop().await;
    }
}
