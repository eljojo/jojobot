//! "Anything that goes into my mailbox goes into my phone."
//!
//! **A bot leaves the operator a message, and it reaches their phone.** The
//! phone is a push to an ntfy server the instance is set up with, and it is for
//! the operator's own mailbox and nothing else: a note between two bots is
//! nobody's phone.
//!
//! **The fake server is a real HTTP server that can be taken down**, so every
//! case here travels the call the build makes: a JSON publish to the server's
//! root with the credential in the `Authorization` header. A server that is down
//! answers a 503. Each negative sits beside the positive it depends on: the same
//! story that shows a note between bots pushes nothing also shows the operator's
//! message pushed, and the story that gives a push up also shows a later one
//! delivered.

use std::sync::Arc;
use std::time::Duration;

use jojobot::push::{Ntfy, NtfyConfig};
use jojobot_domain::push::Policy;
use serde_json::json;

use super::dsl::{Session, Story};
use crate::support::FakeNtfy;

/// The credential the instance holds. A made-up one: nothing real is in a test.
const TOKEN: &str = "tk_kiln";

/// A policy a story can wait out: a retry in thirty milliseconds, and a window
/// the length of the case rather than of an hour.
const QUICK: Policy = Policy {
    first_delay: Duration::from_millis(30),
    longest_delay: Duration::from_millis(60),
    window: Duration::from_secs(20),
};

/// The sender the instance builds for `ntfy`.
fn notifier_for(ntfy: &FakeNtfy) -> Arc<dyn jojobot_domain::push::Notifier> {
    Arc::new(
        Ntfy::new(NtfyConfig {
            url: ntfy.url.clone(),
            topic: "phone".into(),
            token: Some(TOKEN.into()),
        })
        .expect("a client"),
    )
}

/// A jojobot set up to push to `ntfy`, with `lisa` named as its operator.
async fn pushing_to(ntfy: &FakeNtfy, policy: Policy) -> (Story, Session) {
    let story = Story::begin_pushing("bot:otto", notifier_for(ntfy), policy).await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:sigma", "Sigma").await;
    s.add("topic:instance", "This instance").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    (story, s)
}

/// Wait up to five seconds for `done`, polling every ten milliseconds, and name
/// what was waited for when it never comes.
async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("never happened within five seconds: {what}");
}

/// A buffer a subscriber writes into, read back by the case that installed it.
#[derive(Clone, Default)]
struct Logged(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Logged {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Logged {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// **Capture what is logged for the rest of the case.** The case and the server
/// it spawns run on the test's own thread, so a subscriber set for this thread
/// sees the push task's events too.
fn capture_logs() -> (Logged, tracing::subscriber::DefaultGuard) {
    let buffer = Logged::default();
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    (buffer, tracing::subscriber::set_default(subscriber))
}

#[tokio::test]
async fn a_message_for_the_operator_reaches_their_phone_with_its_subject_sender_and_body() {
    let ntfy = FakeNtfy::start().await;
    let (story, s) = pushing_to(&ntfy, QUICK).await;

    // ── a note between two bots is nobody's phone ───────────────────────────
    s.post("bot:sigma", "an ordinary note", "the kiln is relined")
        .await;

    // ── the operator's message is ──────────────────────────────────────────
    let posted = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "subject": "the quarterly figure",
                   "body": "the oven is preheated"}),
        )
        .await
        .json();
    let id = posted["id"].as_str().expect("a post hands back its id");
    eventually("the operator's message was published", || {
        !ntfy.published().is_empty()
    })
    .await;

    let published = ntfy.published();
    assert_eq!(
        published.len(),
        1,
        "exactly the operator's message was pushed, and not the note between bots: {published:?}"
    );
    assert_eq!(published[0].body["topic"], "phone");
    assert_eq!(published[0].body["title"], "the quarterly figure");
    assert_eq!(
        published[0].body["message"],
        "From bot:otto\n\nthe oven is preheated"
    );
    assert_eq!(
        published[0].authorization.as_deref(),
        Some(format!("Bearer {TOKEN}").as_str()),
        "the credential rides in the header and nowhere else"
    );
    assert!(!published[0].body.to_string().contains(TOKEN));

    // The push took nothing out of the box: the message is still waiting there.
    assert_eq!(story.mail_held_by("person:lisa").await, (1, 0, 0), "{id}");
}

#[tokio::test]
async fn the_server_down_and_then_up_delivers_the_message_once() {
    let ntfy = FakeNtfy::start().await;
    ntfy.go_down();
    let (story, s) = pushing_to(&ntfy, QUICK).await;

    let posted = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
        )
        .await
        .json();
    assert!(
        posted["id"].is_string(),
        "the post succeeds while the server is down"
    );
    assert_eq!(
        story.mail_held_by("person:lisa").await,
        (1, 0, 0),
        "the message landed whatever the push did"
    );

    eventually("the push was tried and refused", || ntfy.attempts() >= 2).await;
    assert!(ntfy.published().is_empty());

    ntfy.come_up();
    eventually("the push went out once the server was back", || {
        !ntfy.published().is_empty()
    })
    .await;
    // A late retry would be a second publish: give it time to show up.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(ntfy.published().len(), 1, "delivered once, never twice");
}

#[tokio::test]
async fn a_push_that_never_goes_out_is_logged_naming_the_message_and_the_message_stays() {
    let (logged, _guard) = capture_logs();
    let ntfy = FakeNtfy::start().await;
    ntfy.go_down();
    let short = Policy {
        window: Duration::from_millis(300),
        ..QUICK
    };
    let (story, s) = pushing_to(&ntfy, short).await;

    let posted = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
        )
        .await
        .json();
    let id = posted["id"]
        .as_str()
        .expect("a post hands back its id")
        .to_string();

    eventually("the push was given up on", || {
        logged.text().contains("A PUSH NEVER WENT OUT")
    })
    .await;
    let warning = logged
        .text()
        .lines()
        .find(|line| line.contains("A PUSH NEVER WENT OUT"))
        .expect("the warning line")
        .to_string();
    assert!(warning.contains("WARN"), "{warning}");
    assert!(
        warning.contains(&id),
        "the warning names the message: {warning}"
    );
    assert!(
        !warning.contains(TOKEN),
        "the credential is never logged: {warning}"
    );
    assert!(ntfy.published().is_empty());
    assert_eq!(
        story.mail_held_by("person:lisa").await,
        (1, 0, 0),
        "the message is still in the operator's box"
    );

    // The positive the give-up rests on: with the server back, the next
    // message goes out, so the first was given up on because it could not go,
    // and not because nothing goes.
    ntfy.come_up();
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the insurance", "body": "it is done"}),
    )
    .await;
    eventually("a later message was published", || {
        !ntfy.published().is_empty()
    })
    .await;
    assert_eq!(ntfy.published()[0].body["title"], "the insurance");
}

#[tokio::test]
async fn an_instance_with_no_push_service_pushes_nothing_and_logs_nothing_about_it() {
    let (logged, _guard) = capture_logs();
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("topic:instance", "This instance").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;

    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
    )
    .await;
    assert_eq!(story.mail_held_by("person:lisa").await, (1, 0, 0));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let text = logged.text().to_lowercase();
    assert!(
        !text.contains("push"),
        "a word about pushing was logged: {text}"
    );
    assert!(
        !text.contains("ntfy"),
        "a word about ntfy was logged: {text}"
    );
}

/// A body of `bytes` bytes of plain text. One letter and a space, so a cut at
/// any place shows in the length.
fn body_of(bytes: usize) -> String {
    // Ends in a letter: a stored body is trimmed, and a trimmed space would
    // make the length the case sends differ from the length that is stored.
    let mut body = "kiln ".repeat(bytes / 5 + 1)[..bytes].to_string();
    body.replace_range(bytes - 1.., "x");
    body
}

/// What `post_message` adds in front of a body: the sender's line and a blank
/// line. ntfy's limit counts the text with it.
const FRAMING: usize = "From bot:otto\n\n".len();

/// The line a long body's readable part ends with.
const CLOSING: &str = "The rest is on the web UI's mail page.";

/// **What the phone reads of a long body**, from the one place that says it: the
/// notification text of a publish, with the header's written line breaks turned
/// back into line breaks. Returns the part before the closing line, and checks
/// the closing line is there.
fn readable_of(published_message: &str) -> String {
    let shown = published_message.replace("\\n", "\n");
    let (head, closing) = shown
        .rsplit_once("\n\n")
        .unwrap_or_else(|| panic!("no closing line in {shown:?}"));
    assert_eq!(
        closing, CLOSING,
        "the readable part closes by saying where the rest is"
    );
    assert!(
        shown.len() <= 4096,
        "the readable part fits ntfy's message limit: {} bytes",
        shown.len()
    );
    head.to_string()
}

#[tokio::test]
async fn a_long_body_shows_its_start_and_goes_whole_as_a_file_when_the_server_takes_files() {
    let ntfy = FakeNtfy::start().await;
    let (_story, s) = pushing_to(&ntfy, QUICK).await;

    let long = body_of(6000);
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the long report", "body": long}),
    )
    .await;
    eventually("the long message was published", || {
        !ntfy.published().is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let published = ntfy.published();
    assert_eq!(
        published.len(),
        1,
        "one notification, text and file together"
    );
    let sent = &published[0].body;
    let whole = format!("From bot:otto\n\n{long}");
    assert_eq!(sent["attachment"], true, "{sent}");
    assert_eq!(sent["topic"], "phone");
    assert_eq!(sent["title"], "the long report");
    assert_eq!(sent["filename"], "message.txt");
    assert_eq!(
        sent["text"], whole,
        "the file holds the whole message, never a cut of it"
    );

    let readable = readable_of(sent["message"].as_str().expect("a notification text"));
    assert!(
        whole.starts_with(&readable) && (3900..=4000).contains(&readable.len()),
        "the readable part is the start of the message, about 4,000 bytes: {} bytes",
        readable.len()
    );
}

#[tokio::test]
async fn a_server_that_refuses_the_file_is_sent_the_readable_part_alone_once_and_without_a_warning()
{
    let (logged, _guard) = capture_logs();
    // The three answers a server gives to a file it will not take: attachments
    // off, over the size limit, and storage used up.
    for refusal in [400, 413, 507] {
        let ntfy = FakeNtfy::start().await;
        ntfy.refuse_files_with(refusal);
        let (story, s) = pushing_to(&ntfy, QUICK).await;
        let long = body_of(6000);
        s.call(
            "post_message",
            json!({"to": "person:lisa", "subject": "the long report", "body": long}),
        )
        .await;
        eventually("the readable part was published", || {
            !ntfy.published().is_empty()
        })
        .await;
        tokio::time::sleep(Duration::from_millis(400)).await;

        let published = ntfy.published();
        assert_eq!(published.len(), 1, "once, after a {refusal}: {published:?}");
        let sent = &published[0].body;
        assert!(
            sent["attachment"].is_null(),
            "text alone after a {refusal}: {sent}"
        );
        assert_eq!(sent["title"], "the long report");
        let whole = format!("From bot:otto\n\n{long}");
        let text = sent["message"].as_str().expect("a text message");
        let (head, closing) = text.rsplit_once("\n\n").expect("a closing line");
        assert_eq!(closing, CLOSING);
        assert!(
            whole.starts_with(head) && (3900..=4000).contains(&head.len()),
            "{} bytes",
            head.len()
        );
        assert!(text.len() <= 4096);
        assert_eq!(
            story.mail_held_by("person:lisa").await,
            (1, 0, 0),
            "the whole message stays in the box"
        );
    }
    assert!(
        !logged.text().contains("A PUSH NEVER WENT OUT"),
        "a refused file is not a failed push: {}",
        logged.text()
    );
}

#[tokio::test]
async fn the_last_body_that_fits_goes_as_text_and_one_byte_more_goes_as_a_file() {
    let ntfy = FakeNtfy::start().await;
    let (_story, s) = pushing_to(&ntfy, QUICK).await;
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the last that fits",
               "body": body_of(4096 - FRAMING)}),
    )
    .await;
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the first that does not",
               "body": body_of(4096 - FRAMING + 1)}),
    )
    .await;
    eventually("both messages were published", || {
        ntfy.published().len() == 2
    })
    .await;
    let published = ntfy.published();
    let fits = &published[0].body;
    assert_eq!(fits["title"], "the last that fits");
    assert!(
        fits["message"].is_string() && fits["attachment"].is_null(),
        "{fits}"
    );
    let over = &published[1].body;
    assert_eq!(over["title"], "the first that does not");
    assert_eq!(over["attachment"], true, "{over}");
}

#[tokio::test]
async fn a_push_that_failed_goes_out_by_itself_when_the_server_is_back_with_no_second_post() {
    let ntfy = FakeNtfy::start().await;
    ntfy.go_down();
    let (_story, s) = pushing_to(&ntfy, QUICK).await;
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
    )
    .await;
    eventually("the push was tried and refused", || ntfy.attempts() >= 2).await;
    assert!(ntfy.published().is_empty());

    // Nothing else is posted. The task retries on its own timer.
    ntfy.come_up();
    eventually("the push went out with no further post", || {
        !ntfy.published().is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(ntfy.published().len(), 1, "once");
}

#[tokio::test]
async fn a_restart_with_a_push_pending_inside_its_window_delivers_it_once() {
    let ntfy = FakeNtfy::start().await;
    ntfy.go_down();
    let (story, s) = pushing_to(&ntfy, QUICK).await;
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
    )
    .await;
    eventually("the push was tried and refused", || ntfy.attempts() >= 1).await;

    // The process stops with the push still pending, and the first task tries
    // nothing more: a second task trying as well would be a second delivery.
    story.stop_pushing();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stopped_at = ntfy.attempts();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(ntfy.attempts(), stopped_at, "the stopped task tried again");
    assert!(ntfy.published().is_empty());

    // The server comes back and the instance starts again over the same stores.
    ntfy.come_up();
    let restarted = story.restarted_pushing(notifier_for(&ntfy), QUICK).await;
    eventually("the restart delivered the pending push", || {
        !ntfy.published().is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(ntfy.published().len(), 1, "once");
    assert_eq!(ntfy.published()[0].body["title"], "the lease");

    // A third start finds nothing pending: a sent push is not sent again.
    restarted.stop_pushing();
    let again = restarted
        .restarted_pushing(notifier_for(&ntfy), QUICK)
        .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(ntfy.published().len(), 1, "never twice across restarts");
    again.stop_pushing();
}

#[tokio::test]
async fn a_restart_past_a_pushs_window_gives_it_up_aloud_and_the_message_stays() {
    let (logged, _guard) = capture_logs();
    let ntfy = FakeNtfy::start().await;
    ntfy.go_down();
    let short = Policy {
        window: Duration::from_millis(300),
        ..QUICK
    };
    let (story, s) = pushing_to(&ntfy, short).await;
    let posted = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "subject": "the lease", "body": "it is signed"}),
        )
        .await
        .json();
    let id = posted["id"]
        .as_str()
        .expect("a post hands back its id")
        .to_string();
    eventually("the push was tried and refused", || ntfy.attempts() >= 1).await;
    story.stop_pushing();
    // Long enough for the window to end while nothing is running.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !logged.text().contains("A PUSH NEVER WENT OUT"),
        "nothing was running to say it"
    );

    // The server is back, and the restart finds the push past its hour: it is
    // given up on aloud and not delivered late.
    ntfy.come_up();
    let restarted = story.restarted_pushing(notifier_for(&ntfy), short).await;
    eventually("the restart gave the push up", || {
        logged.text().contains("A PUSH NEVER WENT OUT")
    })
    .await;
    assert!(logged.text().contains(&id), "the warning names the message");
    assert!(
        ntfy.published().is_empty(),
        "a push past its window is not sent late"
    );
    assert_eq!(story.mail_held_by("person:lisa").await, (1, 0, 0));
    restarted.stop_pushing();
}
