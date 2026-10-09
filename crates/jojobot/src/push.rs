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
/// sent as a file instead, which is ntfy's own rule and not jojobot's.
const MESSAGE_LIMIT: usize = 4096;

#[async_trait]
impl Notifier for Ntfy {
    async fn deliver(&self, notification: &Notification) -> Result<(), Undelivered> {
        let request = match notification.body.len() > MESSAGE_LIMIT {
            false => self.as_a_message(notification),
            true => self.as_an_attachment(notification),
        };
        // **Nothing here quotes the request**: it carries the credential.
        let answer = request.send().await.map_err(|e| {
            Undelivered(format!(
                "the ntfy server could not be reached: {}",
                e.without_url()
            ))
        })?;
        match answer.status().is_success() {
            true => Ok(()),
            false => Err(Undelivered(format!(
                "the ntfy server answered {}",
                answer.status()
            ))),
        }
    }
}

impl Ntfy {
    /// **Text that fits**, published as JSON to the server's root so a subject
    /// with any character in it never travels in an HTTP header.
    fn as_a_message(&self, notification: &Notification) -> reqwest::RequestBuilder {
        self.authorised(self.http.post(&self.config.url).json(&serde_json::json!({
            "topic": self.config.topic,
            "title": notification.title,
            "message": notification.body,
        })))
    }

    /// **Text that does not fit, sent whole as a file.** The body is the file and
    /// the notification beside it says so. The title rides in a header, which
    /// ntfy reads as UTF-8; a title that cannot be a header value is replaced by
    /// a plain one rather than losing the message.
    fn as_an_attachment(&self, notification: &Notification) -> reqwest::RequestBuilder {
        let title = reqwest::header::HeaderValue::from_bytes(notification.title.as_bytes())
            .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("a message"));
        let sender = notification
            .body
            .lines()
            .next()
            .unwrap_or("A message")
            .to_string();
        self.authorised(
            self.http
                .put(format!("{}/{}", self.config.url, self.config.topic))
                .header("Title", title)
                .header("Filename", "message.txt")
                .header(
                    "Message",
                    format!("{sender}. The whole message is attached."),
                )
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
pub struct Runner {
    wake: Arc<Notify>,
    _stop: DropGuard,
}

impl Runner {
    /// Start the task. It does nothing until a post nudges it.
    pub fn start(
        outbox: Arc<dyn Outbox>,
        mailboxes: Arc<dyn Mailboxes>,
        notifier: Arc<dyn Notifier>,
        policy: Policy,
    ) -> Self {
        let wake = Arc::new(Notify::new());
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
            _stop: stop.drop_guard(),
        }
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
        loop {
            let pushing = Pushing {
                outbox: outbox.as_ref(),
                mailboxes: mailboxes.as_ref(),
                notifier: notifier.as_ref(),
                policy,
            };
            let next = match pushing.drain(Timestamp::now()).await {
                Ok(drained) => {
                    for message in &drained.sent {
                        tracing::debug!(message = %message, "pushed to the operator's phone");
                    }
                    for (message, why) in &drained.abandoned {
                        tracing::warn!(
                            message = %message,
                            reason = %why,
                            "A PUSH NEVER WENT OUT — the operator's phone was not told about \
                             this message. It is in their box, unread."
                        );
                    }
                    drained.next_due
                }
                Err(e) => {
                    tracing::warn!(error = %e, "the push outbox could not be drained");
                    None
                }
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
