//! **A jojobot served with apps of the unstable tier**, and the call an app makes
//! to it. An app holds a token and a grant of operations; it speaks to
//! `/app/unstable/NAME` and never to the tool surface.

use jojobot_domain::app::operations::OperationSpec;
use serde_json::Value;

use super::{Arc, InMemoryMailboxes, InMemoryMemory, InMemorySessions, Story, Timing};

impl Story {
    /// **Serve a jojobot with one unstable-tier app registered**, wired as the
    /// binary wires its apps: the app's name, its operation grants and the hash of
    /// its token, which is all the server keeps. Hands back the token beside the
    /// story, because sending it is the app's move.
    pub async fn begin_serving_an_app(
        bot: &str,
        app: &str,
        operations: Vec<OperationSpec>,
    ) -> (Self, String) {
        let bare = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        // **The principal is made after the memory has booted, as the binary makes
        // it**: a grant names kinds, and a kind is known once the store has loaded
        // them. The fake loads the shipped ones when it boots. The kind set is one
        // table for the whole test process, so a story cannot hold it empty to pin
        // that order; `tests/app_grant_kinds_start.rs` starts the real binary and
        // does.
        let memory = InMemoryMemory::booted();
        let token = jojobot_domain::app::mint_token();
        let spec = jojobot_domain::app::AppSpec {
            name: app.to_string(),
            operation: String::new(),
            thread: String::new(),
            kind: String::new(),
            keys: vec![],
            operations,
            revoked: false,
        };
        let principal = jojobot_domain::app::AppPrincipal::from_spec(
            &spec,
            &jojobot_domain::app::hash_token(&token),
        )
        .expect("the app's grant is one the server can hold");
        let story = Self::spawn_serving(
            bare,
            Self::wired(memory),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Timing {
                clock: jojobot_domain::clock::Clock::default(),
                quiet_after: Some(jiff::SignedDuration::ZERO),
            },
            None,
            jojobot_mcp::orientation::skills::SKILLS,
            jojobot::app_api::Apps::new(vec![principal]),
        )
        .await;
        story.stand_up_the_bot().await;
        (story, token)
    }

    /// **An app's call to an operation**, the way an outside program sends it:
    /// over HTTP, to the unstable route, with its token. Answers the status and
    /// the body.
    pub async fn app_calls(&self, token: &str, operation: &str, body: Value) -> (u16, Value) {
        let answer = reqwest::Client::new()
            .post(format!("http://{}/app/unstable/{operation}", self.addr))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .expect("the unstable route answers");
        let status = answer.status().as_u16();
        let text = answer.text().await.expect("the answer is text");
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }
}
