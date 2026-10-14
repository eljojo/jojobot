//! **Pushes to the operator's phone, over ntfy.**
//!
//! The domain owns the policy ([`jojobot_domain::push`]): which messages are
//! pushed, what is said, how long a failed push is chased. This file owns the two
//! things that touch the world: the HTTP call to the ntfy server ([`Ntfy`]) and
//! the task that drains the outbox when a post nudges it ([`Runner`]).
//!
//! **No poller.** The task sleeps until it is nudged by a post or until the
//! earliest pending retry falls due, and goes back to sleep with nothing to
//! wake for when the outbox holds nothing pending.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::push::{Notification, Notifier, Nudge, Outbox, Policy, Pushing, Undelivered};
use tokio::sync::Notify;
use tokio_util::sync::{CancellationToken, DropGuard};

/// **Where pushes go.** All three are the instance's, read from its environment
/// and never from the repository.
#[derive(Clone)]
pub struct NtfyConfig {
    /// The ntfy server's address, scheme included, with no trailing slash.
    pub url: String,
    /// The topic the operator's phone listens on.
    pub topic: String,
    /// The access token, when the topic needs one.
    pub token: Option<String>,
}

// The token is a credential: it never prints.
impl std::fmt::Debug for NtfyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NtfyConfig")
            .field("url", &self.url)
            .field("topic", &self.topic)
            .field("token", &self.token.as_ref().map(|_| "<set>"))
            .finish()
    }
}

/// **The ntfy server as a [`Notifier`].** Publishes as JSON to the server's root,
/// so a subject with any character in it never has to travel in an HTTP header.
pub struct Ntfy {
    http: reqwest::Client,
    config: NtfyConfig,
}

impl Ntfy {
    /// A notifier for this server and topic. A call that has not answered in
    /// ten seconds has failed.
    pub fn new(config: NtfyConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Ntfy { http, config })
    }
}

/// **The most text ntfy carries as a message.** A body over this many bytes is
/// the case ntfy handles as a file, which is ntfy's own rule and not jojobot's.
const MESSAGE_LIMIT: usize = 4096;

/// **How much of a long body the phone always reads**, in bytes, before the line
/// that says where the rest is. Under [`MESSAGE_LIMIT`] with room for that line.
const READABLE_BYTES: usize = 4000;

/// The line that closes the readable part of a body that was too long to show.
const REST_IS_ON_THE_PAGE: &str = "The rest is on the web UI's mail page.";

/// **What a long body shows on the phone**: its first [`READABLE_BYTES`] bytes,
/// cut at a character, then the line that says where the rest is.
fn readable_part(body: &str) -> String {
    let mut end = READABLE_BYTES.min(body.len());
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n{REST_IS_ON_THE_PAGE}", body[..end].trim_end())
}

/// **A text as one header value.** A header carries no line break, and ntfy reads
/// a backslash and an n in the message header as one, so a break is written that
/// way. Any other control character becomes a space.
fn as_a_header_line(text: &str) -> String {
    text.replace("\r\n", "\\n")
        .replace('\n', "\\n")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Why a publish did not go through.
enum Failed {
    /// The server answered with this status.
    Answered(reqwest::StatusCode),
    /// The server was not reached.
    Unreached(String),
}

impl Failed {
    /// **The answers a server gives to a file it will not take**: attachments
    /// are off (400), the file is over the size limit (413), or the visitor's
    /// storage is used up (507). Anything else is a failure of the push.
    fn is_a_refused_file(&self) -> bool {
        matches!(self, Failed::Answered(status) if matches!(status.as_u16(), 400 | 413 | 507))
    }

    fn undelivered(self) -> Undelivered {
        match self {
            Failed::Answered(status) => Undelivered(format!("the ntfy server answered {status}")),
            Failed::Unreached(why) => {
                Undelivered(format!("the ntfy server could not be reached: {why}"))
            }
        }
    }
}

#[async_trait]
impl Notifier for Ntfy {
    /// **Text that fits goes as a message. Text that does not shows its first
    /// part and goes whole as a file beside it**, and a server that will not take
    /// the file is sent the readable part alone. The phone always gets text to
    /// read, and a refused file is never a failed push.
    async fn deliver(&self, notification: &Notification) -> Result<(), Undelivered> {
        if notification.body.len() <= MESSAGE_LIMIT {
            return self
                .publish(self.as_a_message(notification))
                .await
                .map_err(Failed::undelivered);
        }
        let readable = readable_part(&notification.body);
        match self.publish(self.as_a_file(notification, &readable)).await {
            Err(refused) if refused.is_a_refused_file() => self
                .publish(self.as_a_message(&Notification {
                    title: notification.title.clone(),
                    body: readable,
                }))
                .await
                .map_err(Failed::undelivered),
            other => other.map_err(Failed::undelivered),
        }
    }
}

impl Ntfy {
    /// Send one request and say whether it went through.
    async fn publish(&self, request: reqwest::RequestBuilder) -> Result<(), Failed> {
        // **Nothing here quotes the request**: it carries the credential.
        let answer = request
            .send()
            .await
            .map_err(|e| Failed::Unreached(e.without_url().to_string()))?;
        match answer.status().is_success() {
            true => Ok(()),
            false => Err(Failed::Answered(answer.status())),
        }
    }

    /// **Text that fits**, published as JSON to the server's root so a subject
    /// with any character in it never travels in an HTTP header.
    fn as_a_message(&self, notification: &Notification) -> reqwest::RequestBuilder {
        self.authorised(self.http.post(&self.config.url).json(&serde_json::json!({
            "topic": self.config.topic,
            "title": notification.title,
            "message": notification.body,
        })))
    }

    /// **The whole text as a file, with the readable part beside it.** The body
    /// is the file; the notification text is `readable`. The title rides in a
    /// header, which ntfy reads as UTF-8; one that cannot be a header value is
    /// replaced by a plain one rather than losing the message.
    fn as_a_file(&self, notification: &Notification, readable: &str) -> reqwest::RequestBuilder {
        let title = reqwest::header::HeaderValue::from_bytes(
            as_a_header_line(&notification.title).as_bytes(),
        )
        .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("a message"));
        let shown = reqwest::header::HeaderValue::from_bytes(as_a_header_line(readable).as_bytes())
            .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static(REST_IS_ON_THE_PAGE));
        self.authorised(
            self.http
                .put(format!("{}/{}", self.config.url, self.config.topic))
                .header("Title", title)
                .header("Filename", "message.txt")
                .header("Message", shown)
                .body(notification.body.clone()),
        )
    }

    fn authorised(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}

/// **The task that delivers pushes.** Started once, when the instance is set up
/// to push, and stopped when this is dropped.
///
/// **It drains once as it starts**, so a push left pending by an earlier run is
/// tried again as soon as the instance is up, and one past its window is given
/// up on aloud. A push that fails is retried by this task on its own timer, to
/// the end of its window, whether or not another message ever arrives.
pub struct Runner {
    wake: Arc<Notify>,
    stop: CancellationToken,
    _stop_on_drop: DropGuard,
}

impl Runner {
    /// Start the task. Its first pass is the drain of whatever an earlier run
    /// left pending.
    pub fn start(
        outbox: Arc<dyn Outbox>,
        mailboxes: Arc<dyn Mailboxes>,
        notifier: Arc<dyn Notifier>,
        policy: Policy,
    ) -> Self {
        let wake = Arc::new(Notify::new());
        // A stored permit: the task's first wait ends at once.
        wake.notify_one();
        let stop = CancellationToken::new();
        tokio::spawn(run(
            outbox,
            mailboxes,
            notifier,
            policy,
            wake.clone(),
            stop.clone(),
        ));
        Runner {
            wake,
            stop: stop.clone(),
            _stop_on_drop: stop.drop_guard(),
        }
    }

    /// Stop the task now. A push still pending stays pending in the outbox for
    /// whichever run starts next.
    pub fn stop(&self) {
        self.stop.cancel();
    }
}

impl Nudge for Runner {
    /// `notify_one` stores a permit when the task is busy, so a post that lands
    /// while a drain is running is looked at by the next pass and never lost.
    fn nudge(&self) {
        self.wake.notify_one();
    }
}

async fn run(
    outbox: Arc<dyn Outbox>,
    mailboxes: Arc<dyn Mailboxes>,
    notifier: Arc<dyn Notifier>,
    policy: Policy,
    wake: Arc<Notify>,
    stop: CancellationToken,
) {
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            () = wake.notified() => {}
        }
        // Drained until nothing is pending, sleeping to the earliest retry in
        // between. A nudge during a sleep cuts it short and drains again.
        // **A pass the store spoiled is retried on this task's own timer**, for
        // as long as it stays spoiled, because no other message may ever arrive
        // to nudge it.
        let mut spoiled_passes = 0u32;
        loop {
            let pushing = Pushing {
                outbox: outbox.as_ref(),
                mailboxes: mailboxes.as_ref(),
                notifier: notifier.as_ref(),
                policy,
            };
            let (next, spoiled) = match pushing.drain(Timestamp::now()).await {
                Ok(drained) => {
                    for message in &drained.sent {
                        tracing::debug!(message = %message, "pushed to the operator's phone");
                    }
                    for (message, why) in &drained.withdrawn {
                        tracing::debug!(message = %message, reason = %why, "a push was not needed");
                    }
                    for (message, why) in &drained.abandoned {
                        tracing::warn!(
                            message = %message,
                            reason = %why,
                            "A PUSH NEVER WENT OUT — the operator's phone was not told about \
                             this message. It is in their box, unread."
                        );
                    }
                    for (message, why) in &drained.unrecorded {
                        tracing::warn!(
                            message = %message,
                            reason = %why,
                            "the push outbox could not record how a push ended; it is tried \
                             again"
                        );
                    }
                    (drained.next_due, !drained.unrecorded.is_empty())
                }
                Err(e) => {
                    tracing::warn!(error = %e, "the push outbox could not be drained");
                    (None, true)
                }
            };
            let next = if spoiled {
                spoiled_passes = spoiled_passes.saturating_add(1);
                let retry = Timestamp::now()
                    .checked_add(policy.delay_after(spoiled_passes))
                    .unwrap_or(Timestamp::MAX);
                Some(next.map_or(retry, |due| due.max(retry)))
            } else {
                spoiled_passes = 0;
                next
            };
            let Some(next) = next else { break };
            let wait = next
                .duration_since(Timestamp::now())
                .try_into()
                .unwrap_or(Duration::ZERO);
            tokio::select! {
                () = stop.cancelled() => return,
                () = wake.notified() => {}
                () = tokio::time::sleep(wait) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::mailbox::{Guarded, MailboxName, NewMessage};
    use jojobot_domain::memory::EntityId;
    use jojobot_domain::push::testing::{
        Call, FakeNotifier, FlakyOutbox, InMemoryOutbox, Standing,
    };

    const QUICK: Policy = Policy {
        first_delay: Duration::from_millis(30),
        longest_delay: Duration::from_millis(120),
        window: Duration::from_secs(60),
    };

    /// A store that pushes, with one message already in the person's box and the
    /// runner started over an outbox that fails as told. The runner is returned
    /// so it lives for the case.
    async fn running(
        flaky: impl FnOnce(&FlakyOutbox),
    ) -> (Runner, Arc<InMemoryOutbox>, Arc<FakeNotifier>) {
        jojobot_domain::memory::kinds::load_shipped();
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner().pushing());
        boxes
            .create_mailbox(
                &MailboxName("person-ned-flanders".into()),
                &EntityId("person:ned-flanders".into()),
                None,
            )
            .await
            .expect("a box");
        let posted = boxes
            .post_message(NewMessage {
                mailbox: MailboxName("person-ned-flanders".into()),
                body: "a body".into(),
                subject: Some("a subject".into()),
                sender: "gamma".into(),
                sent_at: Timestamp::now(),
                in_reply_to: None,
                supersedes: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
            })
            .await
            .expect("the post lands");
        assert!(matches!(posted, Guarded::Written(_)), "{posted:?}");
        let rows = boxes.outbox();
        let outbox = Arc::new(FlakyOutbox::over(rows.clone()));
        flaky(&outbox);
        let notifier = Arc::new(FakeNotifier::new());
        let runner = Runner::start(outbox, boxes, notifier.clone(), QUICK);
        (runner, rows, notifier)
    }

    /// Wait up to three seconds for `done`, polling the way a person would watch.
    async fn within_three_seconds(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..300 {
            if done() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        done()
    }

    /// **A store error does not stop the retry timer.** The outbox fails to list
    /// what is due on the runner's first pass; no other message ever arrives and
    /// nothing nudges the task again, and the push still goes out on the task's
    /// own timer once the store answers.
    #[tokio::test]
    async fn a_store_error_on_a_pass_is_retried_on_the_runners_own_timer() {
        let (_runner, rows, notifier) = running(|o| o.fail_next(Call::Due, 1)).await;
        assert!(
            within_three_seconds(|| rows.rows()[0].standing == Standing::Sent).await,
            "the push was never sent: {:?}",
            rows.rows()
        );
        assert_eq!(notifier.delivered().len(), 1);
    }

    /// **A push the store could not mark sent is tried again on the timer**, not
    /// on the next unrelated nudge, and ends marked sent.
    #[tokio::test]
    async fn a_push_the_store_could_not_mark_sent_is_marked_on_the_next_timed_pass() {
        let (_runner, rows, notifier) = running(|o| o.fail_next(Call::Sent, 1)).await;
        assert!(
            within_three_seconds(|| rows.rows()[0].standing == Standing::Sent).await,
            "the push was never marked sent: {:?}",
            rows.rows()
        );
        assert_eq!(
            notifier.delivered().len(),
            2,
            "a delivery whose record was lost is made again"
        );
    }

    /// **A store that keeps failing to record is retried at the policy's pace,
    /// not in a tight loop.** The row stays due, so a runner that slept to its
    /// due moment would not sleep at all. Paired with the positive: the push is
    /// tried again more than once.
    #[tokio::test]
    async fn a_store_that_keeps_failing_is_retried_at_the_policys_pace() {
        let (_runner, _rows, notifier) = running(|o| o.fail_next(Call::Sent, u32::MAX)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let tries = notifier.delivered().len();
        assert!(tries >= 2, "the push was not tried again: {tries}");
        assert!(tries <= 12, "the retries were not paced: {tries}");
    }

    /// **The readable part of a long body is cut at a character, never inside
    /// one.** A body of three-byte characters puts the 4,000th byte inside one;
    /// a cut at the byte would panic or write half a character.
    #[test]
    fn the_readable_part_is_cut_at_a_character_and_stays_under_the_limit() {
        // 4,000 is not a multiple of three, so the byte at 4,000 is mid-character.
        let body = "あ".repeat(2000);
        assert!(!body.is_char_boundary(READABLE_BYTES));
        let readable = readable_part(&body);
        let (shown, closing) = readable.rsplit_once("\n\n").expect("a closing line");
        assert_eq!(closing, REST_IS_ON_THE_PAGE);
        assert!(body.starts_with(shown));
        assert!(shown.len() <= READABLE_BYTES && shown.len() > READABLE_BYTES - 3);
        assert!(readable.len() <= MESSAGE_LIMIT);
    }

    /// **The notification text travels as one header line.** A line break is
    /// written the way ntfy reads one in a header, and no other control
    /// character survives into the value.
    #[test]
    fn a_text_becomes_one_header_line_with_its_breaks_written_out() {
        assert_eq!(
            as_a_header_line("one\n\ntwo\r\nthree"),
            "one\\n\\ntwo\\nthree"
        );
        assert_eq!(as_a_header_line("tab\there\u{7}bell"), "tab here bell");
        assert!(
            reqwest::header::HeaderValue::from_bytes(as_a_header_line("あ\n\u{0}い").as_bytes())
                .is_ok()
        );
    }

    /// **Only a file the server will not take is retried as text.** The three
    /// answers a server gives to such a file; anything else is a failed push.
    #[test]
    fn only_the_answers_to_a_refused_file_fall_back_to_text() {
        let answered = |code: u16| Failed::Answered(reqwest::StatusCode::from_u16(code).unwrap());
        for refusal in [400, 413, 507] {
            assert!(answered(refusal).is_a_refused_file(), "{refusal}");
        }
        for failure in [401, 403, 404, 429, 500, 503] {
            assert!(!answered(failure).is_a_refused_file(), "{failure}");
        }
        assert!(!Failed::Unreached("down".into()).is_a_refused_file());
    }
}
