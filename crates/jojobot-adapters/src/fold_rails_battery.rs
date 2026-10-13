//! **The differential battery for the in-memory image of sessions, mail and
//! teachings** (decision log 292, 435).
//!
//! Every read the three ports answer is asked twice: of the image, and of the
//! store the image wraps. The two answers must be the same bytes. Beside each
//! answer from the image, a counting port says how many calls reached the store
//! while it was given, and the reads named in `switched` must have made none. A
//! count with no answer beside it proves nothing — a read that never looked
//! would also make no call — so the two checks live in one function and cannot
//! be run apart.
//!
//! The battery is asked after every kind of write, because the image of the
//! store is correct at startup by construction and wrong later only through a
//! write that did not reach it. **A write that the image follows from the
//! store's own answer makes no read either**, so each write below is also held
//! to that.

use std::fmt::Debug;
use std::future::Future;

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_domain::mailbox::{Mailbox, MailboxName, Mailboxes, MessageId, NewMessage, TakenBy};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    CharterChange, NewEntry, NewSession, SessionId, SessionState, Sessions, Sid, WrapWindow,
};
use jojobot_domain::teaching::Teachings;

use crate::counted_rails::{Calls, CountedMailboxes, CountedSessions, CountedTeachings};

/// One read, asked of both, compared as bytes, with the calls it made counted.
async fn check<T, A, B>(
    when: &str,
    name: &'static str,
    subject: &str,
    calls: &Calls,
    switched: &[&str],
    via_image: A,
    via_store: B,
) where
    T: Debug,
    A: Future<Output = T>,
    B: Future<Output = T>,
{
    calls.reset();
    let from_image = format!("{:?}", via_image.await);
    let made = calls.reads() + calls.writes();
    let seen = calls.seen();
    let from_store = format!("{:?}", via_store.await);
    assert_eq!(
        from_image, from_store,
        "{when}: {name}({subject}) answered differently from the image than from the store"
    );
    if switched.contains(&name) {
        assert_eq!(
            made, 0,
            "{when}: {name}({subject}) reached the store {made} time(s): {seen:?}"
        );
    }
}

/// A write followed by the claim that it read nothing from the store.
fn wrote_without_reading(when: &str, calls: &Calls) {
    assert_eq!(
        calls.reads(),
        0,
        "{when}: the write read the store to bring the image up to date: {:?}",
        calls.seen()
    );
}

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
}

fn bot(slug: &str) -> EntityId {
    EntityId(format!("bot:{slug}"))
}

// ── sessions ─────────────────────────────────────────────────────────────────

/// What one run of the sessions battery is asked of.
pub struct SessionRails<'a> {
    /// The image under test, which wraps `counted`.
    pub folded: &'a dyn Sessions,
    /// The store itself, reached without passing the image or the counter.
    pub direct: &'a dyn Sessions,
    /// The counter between the image and the store.
    pub counted: &'a CountedSessions,
    /// The reads that must make no call to the store.
    pub switched: &'a [&'a str],
}

impl SessionRails<'_> {
    /// Every read, for every run and bot the store answers to and a few it does
    /// not.
    pub async fn agree(&self, when: &str) {
        let f = self.folded;
        let d = self.direct;
        let calls = &self.counted.calls;
        let s = self.switched;

        let all = d.all_sessions().await.expect("the store lists");
        let mut bots: Vec<EntityId> = all.iter().map(|r| r.bot.clone()).collect();
        bots.push(bot("nobody"));
        bots.sort();
        bots.dedup();
        let mut ids: Vec<SessionId> = all.iter().map(|r| r.id.clone()).collect();
        // Shaped like a run's name and addressing none, and shaped like nothing.
        ids.push(SessionId("zzzzzz".into()));
        ids.push(SessionId("NOT AN ID".into()));
        ids.push(SessionId(String::new()));

        check(
            when,
            "all_sessions",
            "",
            calls,
            s,
            f.all_sessions(),
            d.all_sessions(),
        )
        .await;
        check(
            when,
            "all_summaries",
            "",
            calls,
            s,
            f.all_summaries(),
            d.all_summaries(),
        )
        .await;
        check(
            when,
            "run_counts",
            "",
            calls,
            s,
            f.run_counts(),
            d.run_counts(),
        )
        .await;
        for b in &bots {
            check(
                when,
                "sessions_of",
                b.as_str(),
                calls,
                s,
                f.sessions_of(b),
                d.sessions_of(b),
            )
            .await;
            check(
                when,
                "summaries_of",
                b.as_str(),
                calls,
                s,
                f.summaries_of(b),
                d.summaries_of(b),
            )
            .await;
            check(
                when,
                "charter_change_of",
                b.as_str(),
                calls,
                s,
                f.charter_change_of(b),
                d.charter_change_of(b),
            )
            .await;
        }
        for id in &ids {
            check(
                when,
                "read_session",
                id.as_str(),
                calls,
                s,
                f.read_session(id),
                d.read_session(id),
            )
            .await;
            check(
                when,
                "summary_of",
                id.as_str(),
                calls,
                s,
                f.summary_of(id),
                d.summary_of(id),
            )
            .await;
        }
    }
}

/// **Every kind of write on a run, each followed by the whole battery.**
/// `p.folded` takes the writes; `p.direct` is the oracle.
pub async fn walk_every_kind_of_session_write(p: &SessionRails<'_>) {
    let w = p.folded;
    let calls = &p.counted.calls;
    p.agree("an empty store").await;

    let new = |slug: &str, sid: &str, id: &str, second: i64| NewSession {
        bot: bot(slug),
        sid: Sid(sid.into()),
        id: Some(SessionId(id.into())),
        key_hash: Some(format!("hash-of-{sid}")),
        focus: format!("{slug} starts"),
        started_at: at(second),
        timezone: Some("America/Toronto".into()),
        started_on: Some(date(2026, 9, 1)),
    };
    calls.reset();
    let homer = w
        .begin(new("homer", "h0m3", "hhhhh1", 10))
        .await
        .expect("begin");
    wrote_without_reading("begin", calls);
    p.agree("after a run begins").await;
    calls.reset();
    w.begin(new("homer", "h0m3", "hhhhh2", 20))
        .await
        .expect("a retried begin returns the run that is open");
    wrote_without_reading("begin again", calls);
    p.agree("after a begin offered the handle an open run holds")
        .await;
    w.begin(new("bart", "b4rt", "bbbbb1", 15))
        .await
        .expect("begin");
    w.begin(new("lisa", "k15a", "lllll1", 15))
        .await
        .expect("begin");
    p.agree("after three runs, two starting at one instant")
        .await;

    calls.reset();
    let first = w
        .append(
            &homer.id,
            NewEntry::manual("homer eats a donut", at(30), Some(date(2026, 9, 1))),
        )
        .await
        .expect("append");
    wrote_without_reading("append", calls);
    p.agree("after a manual entry").await;
    let beat = w
        .append(
            &homer.id,
            NewEntry::beat("memory", "homer wrote 1 time", at(40), None),
        )
        .await
        .expect("append a beat");
    p.agree("after a beat").await;
    calls.reset();
    w.amend_last(&homer.id, "homer wrote 2 times")
        .await
        .expect("amend_last");
    wrote_without_reading("amend_last", calls);
    p.agree("after the newest entry is amended").await;
    calls.reset();
    w.amend_beat(&homer.id, &beat.id, "homer wrote 3 times", at(50))
        .await
        .expect("amend_beat");
    wrote_without_reading("amend_beat", calls);
    p.agree("after a beat is touched").await;
    assert!(
        w.amend_beat(&homer.id, &first.id, "not a beat", at(60))
            .await
            .is_err(),
        "an entry the run wrote is not a beat"
    );
    p.agree("after a refused amend").await;

    calls.reset();
    w.set_focus(&homer.id, "homer naps")
        .await
        .expect("set_focus");
    wrote_without_reading("set_focus", calls);
    p.agree("after the focus moves").await;
    calls.reset();
    w.set_timezone(&homer.id, Some("Europe/Berlin"))
        .await
        .expect("set_timezone");
    wrote_without_reading("set_timezone", calls);
    p.agree("after the zone is set").await;
    w.set_timezone(&homer.id, None)
        .await
        .expect("clear the zone");
    p.agree("after the zone is cleared").await;
    calls.reset();
    w.set_stated_day(&homer.id, Some(date(2026, 9, 2)))
        .await
        .expect("set_stated_day");
    wrote_without_reading("set_stated_day", calls);
    p.agree("after the day is stated").await;
    calls.reset();
    w.add_served(&homer.id, 120).await.expect("add_served");
    w.add_served(&homer.id, 30).await.expect("add_served");
    wrote_without_reading("add_served", calls);
    p.agree("after the served total grows").await;
    calls.reset();
    w.set_key_hash(&homer.id, "another-hash")
        .await
        .expect("set_key_hash");
    wrote_without_reading("set_key_hash", calls);
    p.agree("after the key hash changes").await;
    calls.reset();
    w.set_charter_told(&homer.id, at(70))
        .await
        .expect("set_charter_told");
    wrote_without_reading("set_charter_told", calls);
    p.agree("after the charter is told").await;
    calls.reset();
    w.note_charter_change(
        &bot("homer"),
        CharterChange {
            by: bot("lisa"),
            at: at(80),
        },
    )
    .await
    .expect("note_charter_change");
    wrote_without_reading("note_charter_change", calls);
    p.agree("after a charter change is noted").await;
    w.note_charter_change(
        &bot("homer"),
        CharterChange {
            by: bot("bart"),
            at: at(90),
        },
    )
    .await
    .expect("note_charter_change again");
    w.note_charter_change(
        &bot("marge"),
        CharterChange {
            by: bot("bart"),
            at: at(91),
        },
    )
    .await
    .expect("a change to a bot with no run");
    p.agree("after a second change to one bot and one to a bot with no run")
        .await;

    // A run abandoned, taken back, then wrapped, with the window reopened.
    let bart = SessionId("bbbbb1".into());
    calls.reset();
    w.close(&bart, SessionState::Abandoned)
        .await
        .expect("abandon");
    wrote_without_reading("close", calls);
    p.agree("after a run is abandoned").await;
    calls.reset();
    w.reopen(&bart).await.expect("reopen");
    wrote_without_reading("reopen", calls);
    p.agree("after an abandoned run is reopened").await;
    w.append(
        &bart,
        NewEntry::manual("bart wraps", at(100), None).closing(Some("bart starts".into())),
    )
    .await
    .expect("closing entry");
    w.close(&bart, SessionState::Wrapped).await.expect("wrap");
    p.agree("after a run is wrapped").await;
    assert!(
        w.append(&bart, NewEntry::manual("too late", at(110), None))
            .await
            .is_err(),
        "a wrapped run takes nothing"
    );
    p.agree("after a refused append to a wrapped run").await;
    calls.reset();
    w.set_wrap_window(&bart, Some(WrapWindow::Open("wc-code".into())))
        .await
        .expect("open the window");
    wrote_without_reading("set_wrap_window", calls);
    p.agree("after the wrap window opens").await;
    w.append(&bart, NewEntry::manual("one last change", at(120), None))
        .await
        .expect("a wrapped run with an open window takes a write");
    w.close(&bart, SessionState::Wrapped)
        .await
        .expect("wrap again");
    p.agree("after the window closes").await;
    assert!(
        w.reopen(&bart).await.is_err(),
        "a wrapped run is the last word"
    );
    p.agree("after a refused reopen").await;
    assert!(
        w.add_served(&SessionId("zzzzzz".into()), 5).await.is_err(),
        "a served total for no run is refused"
    );
    p.agree("after a refused add_served").await;
}

// ── mail ─────────────────────────────────────────────────────────────────────

/// What one run of the mail battery is asked of.
pub struct MailRails<'a> {
    pub folded: &'a dyn Mailboxes,
    pub direct: &'a dyn Mailboxes,
    pub counted: &'a CountedMailboxes,
    pub switched: &'a [&'a str],
}

impl MailRails<'_> {
    /// Every read, for every message and sender the store answers to and a few it
    /// does not.
    pub async fn agree(&self, when: &str) {
        let f = self.folded;
        let d = self.direct;
        let calls = &self.counted.calls;
        let s = self.switched;

        let board = d.scan_messages().await.expect("the store scans");
        let mut ids: Vec<MessageId> = board.iter().map(|m| m.id.clone()).collect();
        ids.push(MessageId("zzzzzz".into()));
        ids.push(MessageId("NOT AN ID".into()));
        let mut senders: Vec<String> = board.iter().map(|m| m.sender.clone()).collect();
        senders.sort();
        senders.dedup();

        check(
            when,
            "list_mailboxes",
            "",
            calls,
            s,
            f.list_mailboxes(),
            d.list_mailboxes(),
        )
        .await;
        check(
            when,
            "scan_messages",
            "",
            calls,
            s,
            f.scan_messages(),
            d.scan_messages(),
        )
        .await;
        for id in &ids {
            check(
                when,
                "message_by_id",
                id.as_str(),
                calls,
                s,
                f.message_by_id(id),
                d.message_by_id(id),
            )
            .await;
        }
        let mut sets: Vec<Vec<&str>> = vec![vec![], vec!["nobody-of-that-name"]];
        sets.extend(senders.iter().map(|s| vec![s.as_str()]));
        if senders.len() > 1 {
            sets.push(senders.iter().map(String::as_str).collect());
            sets.push(vec![senders[0].as_str(), "nobody-of-that-name"]);
        }
        for set in &sets {
            check(
                when,
                "sent_by",
                &format!("{set:?}"),
                calls,
                s,
                f.sent_by(set),
                d.sent_by(set),
            )
            .await;
        }
    }
}

fn name(n: &str) -> MailboxName {
    MailboxName(n.into())
}

fn post(to: &str, sender: &str, body: &str, second: i64) -> NewMessage {
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

impl MailRails<'_> {
    /// **A delivery in a box with nothing new in it moves nothing**, so it is a
    /// read, and both ledgers must give it the same answer: the messages already
    /// taken, handed over again, and the replaced ones withheld by name. A box
    /// that does not exist is a refusal the store alone can word, so it reaches
    /// the store.
    async fn nothing_new_in(&self, when: &str, boxes: &[&str]) {
        let board = self.direct.scan_messages().await.expect("the store scans");
        for n in boxes {
            let which = name(n);
            assert!(
                !board.iter().any(|m| m.mailbox == which
                    && m.state == jojobot_domain::mailbox::MessageState::New
                    && m.superseded_by.is_none()),
                "{when}: box {n} holds mail nobody has taken, so a delivery would move it"
            );
            check(
                when,
                "read_mailbox",
                n,
                &self.counted.calls,
                self.switched,
                self.folded.read_mailbox(&which, TakenBy::Posting),
                self.direct.read_mailbox(&which, TakenBy::Posting),
            )
            .await;
        }
        let unknown = name("nonesuch");
        check(
            when,
            "read_mailbox of no box",
            "nonesuch",
            &self.counted.calls,
            &[],
            self.folded.read_mailbox(&unknown, TakenBy::Posting),
            self.direct.read_mailbox(&unknown, TakenBy::Posting),
        )
        .await;
    }
}

/// **Every kind of write on the board, each followed by the whole battery.**
/// The owners `bot:gamma`, `bot:delta` and `person:milhouse` must resolve in the
/// store under test.
pub async fn walk_every_kind_of_mail_write(p: &MailRails<'_>) {
    jojobot_domain::memory::kinds::load_shipped();
    let w = p.folded;
    let calls = &p.counted.calls;
    p.agree("an empty board").await;

    for (n, owner) in [
        ("gamma", "bot:gamma"),
        ("delta", "bot:delta"),
        ("milhouse", "person:milhouse"),
    ] {
        w.create_mailbox(&name(n), &EntityId(owner.into()), None)
            .await
            .expect("create")
            .written()
            .expect("not blocked");
    }
    p.agree("after three boxes are created").await;

    calls.reset();
    let one = w
        .post_message(post("gamma", "delta", "one", 10))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    wrote_without_reading("post_message", calls);
    p.agree("after a message is posted").await;
    let two = w
        .post_message(NewMessage {
            in_reply_to: Some(one.id.clone()),
            ..post("gamma", "delta", "two", 20)
        })
        .await
        .expect("post a reply")
        .written()
        .expect("not blocked");
    p.agree("after a reply is posted").await;
    calls.reset();
    let replacement = w
        .post_message(NewMessage {
            supersedes: Some(two.id.clone()),
            ..post("gamma", "delta", "two, corrected", 30)
        })
        .await
        .expect("post a replacement")
        .written()
        .expect("not blocked");
    wrote_without_reading("post_message that replaces", calls);
    p.agree("after a message is replaced").await;
    let three = w
        .post_message(post("delta", "gamma", "three", 40))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    w.post_message(post("milhouse", "gamma", "four", 50))
        .await
        .expect("post to a person")
        .written()
        .expect("not blocked");
    p.agree("after mail for other boxes").await;
    // **Sent earlier than mail already on the board**: the board lists by the
    // instant the sender declared, so this one lands before the rest and not
    // after them.
    w.post_message(post("delta", "gamma", "three, written earlier", 5))
        .await
        .expect("post an earlier-dated message")
        .written()
        .expect("not blocked");
    p.agree("after a message dated before the others").await;
    assert!(
        matches!(
            w.post_message(post("gamm", "delta", "five", 60)).await,
            Ok(jojobot_domain::mailbox::Guarded::Blocked { .. })
        ),
        "a box that does not exist is blocked"
    );
    p.agree("after a post to no box").await;

    calls.reset();
    let delivery = w
        .read_mailbox(&name("gamma"), TakenBy::Reading)
        .await
        .expect("read_mailbox")
        .written()
        .expect("not blocked");
    wrote_without_reading("read_mailbox", calls);
    assert_eq!(
        delivery.superseded.len(),
        1,
        "the replaced message is withheld: {delivery:?}"
    );
    p.agree("after a delivery that withholds a replaced message")
        .await;
    w.post_message(post("gamma", "delta", "six", 70))
        .await
        .expect("post")
        .written()
        .expect("not blocked");
    w.read_mailbox(&name("gamma"), TakenBy::Posting)
        .await
        .expect("read_mailbox")
        .written()
        .expect("not blocked");
    p.agree("after a second delivery that also hands over a leftover")
        .await;
    for n in ["delta", "milhouse"] {
        w.read_mailbox(&name(n), TakenBy::Reading)
            .await
            .expect("read_mailbox")
            .written()
            .expect("not blocked");
    }
    p.nothing_new_in("after the deliveries", &["gamma", "delta", "milhouse"])
        .await;

    calls.reset();
    w.read_message(&three.id).await.expect("read_message");
    wrote_without_reading("read_message", calls);
    p.agree("after one message is taken by id").await;
    w.read_message(&three.id).await.expect("read_message again");
    p.agree("after the same message is taken again").await;

    calls.reset();
    w.mark_processed(&one.id, Some("done with one"))
        .await
        .expect("mark_processed");
    wrote_without_reading("mark_processed", calls);
    p.agree("after a message is processed").await;
    w.mark_processed(&one.id, None)
        .await
        .expect("mark_processed again keeps the notes");
    w.mark_processed(&replacement.id, None)
        .await
        .expect("mark a message processed from new");
    p.agree("after a second processing and one straight from new")
        .await;
    p.nothing_new_in("after a message is processed", &["gamma"])
        .await;
    assert!(
        w.mark_processed(&MessageId("zzzzzz".into()), None)
            .await
            .is_err(),
        "a message that does not exist is refused"
    );
    p.agree("after a refused mark").await;

    w.quarantine(&three.id, &name("delta"), "not trusted", at(80))
        .await
        .expect("quarantine");
    p.agree("after a quarantine").await;

    w.repoint_owner(&bot("gamma"), &bot("epsilon"))
        .await
        .expect("repoint_owner")
        .expect("the box was found");
    p.agree("after a repoint").await;
}

/// The boxes a board holds, for a case that wants to name them.
pub fn names_of(boxes: &[Mailbox]) -> Vec<String> {
    boxes.iter().map(|b| b.name.as_str().to_string()).collect()
}

// ── teachings ────────────────────────────────────────────────────────────────

/// What one run of the teaching battery is asked of.
pub struct TeachRails<'a> {
    /// The image under test, which wraps `counted`, which wraps the store.
    pub folded: &'a dyn Teachings,
    pub counted: &'a CountedTeachings,
    /// A second ledger that receives every call the first does, with nothing
    /// above it: what the store answers.
    pub twin: &'a dyn Teachings,
    pub switched: &'a [&'a str],
}

/// **Every kind of write on the ledger, each answered as the store answers.**
/// A first contact asks the ledger and writes it in one call, so the oracle is a
/// twin ledger given the same calls: the answers must agree, and the ledgers
/// must hold the same pairs after each.
pub async fn walk_every_kind_of_teaching(p: &TeachRails<'_>) {
    let calls = &p.counted.calls;
    let sid = |s: &str| Sid(s.into());
    let steps: &[(&str, &str)] = &[
        ("h0m3", "claims"),
        ("h0m3", "claims"),
        ("h0m3", "search"),
        ("b4rt", "claims"),
        ("h0m3", "claims"),
        ("b4rt", "claims"),
    ];
    for (n, (who, domain)) in steps.iter().enumerate() {
        calls.reset();
        let from_image = p
            .folded
            .first_contact(&sid(who), domain, at(n as i64))
            .await
            .expect("first_contact");
        let made = calls.reads() + calls.writes();
        let from_store = p
            .twin
            .first_contact(&sid(who), domain, at(n as i64))
            .await
            .expect("first_contact on the twin");
        assert_eq!(
            from_image, from_store,
            "step {n}: first_contact({who}, {domain}) answered differently from the image"
        );
        if p.switched.contains(&"first_contact") && !from_image {
            assert_eq!(
                made,
                0,
                "step {n}: a contact the ledger already holds reached the store: {:?}",
                calls.seen()
            );
        }
        if from_image {
            assert_eq!(
                calls.writes(),
                1,
                "step {n}: a first contact is written once"
            );
        }
        assert_eq!(
            p.folded.taught().await.expect("taught"),
            p.twin.taught().await.expect("taught on the twin"),
            "step {n}: the ledgers differ"
        );
    }
    p.folded.forget(&sid("h0m3")).await.expect("forget");
    p.twin
        .forget(&sid("h0m3"))
        .await
        .expect("forget on the twin");
    assert_eq!(
        p.folded.taught().await.expect("taught"),
        p.twin.taught().await.expect("taught on the twin"),
        "after a forget the ledgers differ"
    );
    for (n, (who, domain)) in steps.iter().take(2).enumerate() {
        calls.reset();
        let from_image = p
            .folded
            .first_contact(&sid(who), domain, at(100 + n as i64))
            .await
            .expect("first_contact");
        let from_store = p
            .twin
            .first_contact(&sid(who), domain, at(100 + n as i64))
            .await
            .expect("first_contact on the twin");
        assert_eq!(
            from_image, from_store,
            "after a forget: first_contact({who}, {domain}) answered differently"
        );
    }
}
