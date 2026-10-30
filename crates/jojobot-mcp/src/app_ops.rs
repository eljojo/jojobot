//! **The unstable tier's registry: the operations an app can call, as itself.**
//!
//! An app does not speak the tool protocol bots use (rule 732). It calls an
//! operation by name with JSON, and the route serves whatever the registry
//! holds: one [`Entry`] for each operation, with no handler written per
//! operation on the route. An entry decodes its typed input, asks
//! [`jojobot_domain::app::operations::permit`] whether the app's grant covers
//! the call, runs the write function the tool handler also runs, with
//! [`Actor::App`] as the actor, and answers a typed view of what the function
//! returned. A refusal comes back as the verb gave it, with its status.
//!
//! An operation not extracted yet is absent from the registry and is never
//! written by hand (rule 676).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use jojobot_domain::actor::Actor;
use jojobot_domain::app::operations::{Operation, permit};
use jojobot_domain::app::{AppError, AppPrincipal, AppStamp};
use jojobot_domain::memory::EntityId;
use rmcp::model::CallToolResult;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::Jojobot;

/// **What one call to an operation carries.**
pub struct Call {
    /// The handler the write functions run on.
    pub handler: Jojobot,
    /// The app and its grants.
    pub app: AppPrincipal,
    /// The mark the app's claim carries.
    pub stamp: AppStamp,
    /// The request body.
    pub body: Value,
}

impl Call {
    fn actor(&self) -> Actor {
        Actor::App(self.stamp.clone())
    }

    /// The names of the arguments the caller sent with a value.
    fn args_sent(&self) -> Vec<String> {
        match &self.body {
            Value::Object(map) => map
                .iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, _)| k.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The body as the operation's typed input.
    fn input<I: DeserializeOwned>(&self) -> Result<I, Answer> {
        serde_json::from_value(self.body.clone()).map_err(|e| Answer::Invalid(e.to_string()))
    }

    fn permit(&self, call: &Operation) -> Result<(), Answer> {
        permit(&self.app, call).map_err(Answer::from)
    }
}

/// **How an operation answered**, which the route maps to a status.
#[derive(Debug)]
pub enum Answer {
    /// The call ran and this is the typed view of its result.
    Done(Value),
    /// The grant does not cover the call. Nothing was written.
    Outside(String),
    /// The input is not the shape the operation takes.
    Invalid(String),
    /// The verb refused the call, in its own words. Nothing was written.
    Refused(Value),
    /// The store could not take the call. The same call can be sent again.
    Unavailable(String),
}

impl From<AppError> for Answer {
    fn from(error: AppError) -> Self {
        match error {
            AppError::OutsideGrant(why) => Answer::Outside(why),
            AppError::Invalid(why) | AppError::Config(why) => Answer::Invalid(why),
        }
    }
}

/// **A verb's refusal, as data**: the body its text carries, as JSON when it is
/// JSON and as a string when it is not.
fn refusal(result: &CallToolResult) -> Answer {
    let text = serde_json::to_value(result)
        .ok()
        .and_then(|v| v["content"][0]["text"].as_str().map(str::to_string))
        .unwrap_or_default();
    Answer::Refused(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

fn unavailable(error: impl std::fmt::Display) -> Answer {
    Answer::Unavailable(error.to_string())
}

fn done(view: &impl Serialize) -> Answer {
    match serde_json::to_value(view) {
        Ok(view) => Answer::Done(view),
        Err(e) => unavailable(e),
    }
}

/// What an operation's function answers.
pub type Running = Pin<Box<dyn Future<Output = Answer> + Send>>;

/// **One operation: its name and the function that runs it.**
#[derive(Clone)]
pub struct Entry {
    name: String,
    run: Arc<dyn Fn(Call) -> Running + Send + Sync>,
}

impl Entry {
    /// An operation of this name, run by `run`.
    pub fn new<F, Fut>(name: &str, run: F) -> Self
    where
        F: Fn(Call) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Answer> + Send + 'static,
    {
        Entry {
            name: name.to_string(),
            run: Arc::new(move |call| Box::pin(run(call))),
        }
    }

    /// The operation's name, which is its path under `/app/unstable/`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Run the operation.
    pub fn run(&self, call: Call) -> Running {
        (self.run)(call)
    }
}

/// **Every operation the route serves.**
#[derive(Clone, Default)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// The operations extracted so far: the four writes.
    pub fn shipped() -> Self {
        Registry::default()
            .with(Entry::new("capture", capture))
            .with(Entry::new("add_entity", add_entity))
            .with(Entry::new("post_message", post_message))
            .with(Entry::new("set_charter", set_charter))
    }

    /// This registry and one more entry. An entry of the same name replaces the
    /// one held.
    pub fn with(mut self, entry: Entry) -> Self {
        self.entries.retain(|held| held.name != entry.name);
        self.entries.push(entry);
        self
    }

    /// The entry of this name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The names the registry serves.
    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }
}

/// **What `capture` served.**
#[derive(Debug, Serialize)]
pub struct CapturedView {
    /// The claim's address.
    pub address: String,
    /// The handle the claim is on.
    pub subject: String,
    /// The day the claim is filed under.
    pub recorded_at: String,
    /// Whether the claim landed and the search fold could not be refreshed.
    pub fold_behind: bool,
}

/// **What `add_entity` served.**
#[derive(Debug, Serialize)]
pub struct AddedView {
    /// The handle of the new thing.
    pub handle: String,
    /// The address of the claim made with it, when one was.
    pub first_claim: Option<String>,
    /// Whether the thing landed and the search fold could not be refreshed.
    pub fold_behind: bool,
}

/// **What `post_message` served.**
#[derive(Debug, Serialize)]
pub struct PostedView {
    /// The message's id.
    pub id: String,
    /// The box it is in.
    pub mailbox: String,
    /// When it was sent.
    pub sent_at: String,
}

async fn capture(call: Call) -> Answer {
    let args_sent = call.args_sent();
    let args: crate::memory::capture::CaptureArgs = match call.input() {
        Ok(args) => args,
        Err(answer) => return answer,
    };
    let handler = &call.handler;
    // **The kind the store holds now**, never the spelling the caller used.
    let subject = EntityId::person(&args.subject);
    let kind = match handler.kind_now(&subject).await {
        Ok(kind) => kind,
        Err(e) => return unavailable(e),
    };
    let operation = Operation {
        name: "capture".into(),
        args_sent,
        kind,
        keys: args.sets.iter().flatten().map(|(k, _)| k.clone()).collect(),
        to_operator: false,
    };
    if let Err(answer) = call.permit(&operation) {
        return answer;
    }
    let day = handler.instance_today().await;
    match handler
        .capture_write(
            &call.actor(),
            args,
            day,
            None,
            &crate::memory::capture::Mode::default(),
            None,
        )
        .await
    {
        Ok(Ok(captured)) => done(&captured.view()),
        Ok(Err(refused)) => refusal(&refused),
        Err(e) => unavailable(e),
    }
}

async fn add_entity(call: Call) -> Answer {
    let args_sent = call.args_sent();
    let args: crate::memory::add_entity::AddEntityArgs = match call.input() {
        Ok(args) => args,
        Err(answer) => return answer,
    };
    let kind = jojobot_domain::memory::EntityKind::from_token(args.kind.trim());
    let operation = Operation {
        name: "add_entity".into(),
        args_sent,
        kind,
        // **The keys a creation sets are judged as a capture's are**: the write
        // function runs no licence check for an app.
        keys: args.sets.iter().flatten().map(|(k, _)| k.clone()).collect(),
        to_operator: false,
    };
    if let Err(answer) = call.permit(&operation) {
        return answer;
    }
    let day = call.handler.instance_today().await;
    match call
        .handler
        .add_entity_write(&call.actor(), args, day, None, None)
        .await
    {
        Ok(Ok(added)) => done(&added.view()),
        Ok(Err(refused)) => refusal(&refused),
        Err(e) => unavailable(e),
    }
}

async fn post_message(call: Call) -> Answer {
    let args_sent = call.args_sent();
    let args: crate::mailboxes::post_message::write::MessageWrite = match call.input() {
        Ok(args) => args,
        Err(answer) => return answer,
    };
    let handler = &call.handler;
    let recipient = match handler.message_recipient(&args).await {
        Ok(Ok(recipient)) => recipient,
        Ok(Err(refused)) => return refusal(&refused),
        Err(e) => return unavailable(e),
    };
    let operation = Operation {
        name: "post_message".into(),
        args_sent,
        kind: None,
        keys: Vec::new(),
        to_operator: recipient.to_the_operator,
    };
    if let Err(answer) = call.permit(&operation) {
        return answer;
    }
    let destination = match handler.message_destination(&recipient).await {
        Ok(Ok(destination)) => destination,
        Ok(Err(refused)) => return refusal(&refused),
        Err(e) => return unavailable(e),
    };
    match handler
        .post_message_write(
            &call.actor(),
            args,
            &recipient,
            destination,
            Default::default(),
        )
        .await
    {
        Ok(Ok(message)) => done(&PostedView {
            id: message.id.0.clone(),
            mailbox: message.mailbox.as_str().to_string(),
            sent_at: message.sent_at.to_string(),
        }),
        Ok(Err(refused)) => refusal(&refused),
        Err(e) => unavailable(e),
    }
}

/// **`set_charter` is in the registry and no app can be granted it**: a charter
/// belongs to a bot. The check refuses every call, and the write function would
/// refuse an app after it.
async fn set_charter(call: Call) -> Answer {
    let operation = Operation {
        name: "set_charter".into(),
        args_sent: call.args_sent(),
        ..Operation::default()
    };
    match call.permit(&operation) {
        Err(answer) => answer,
        Ok(()) => Answer::Outside("an app may not write a charter".into()),
    }
}

impl Jojobot {
    /// **The day of the instance's own zone**, which the stored day of a claim is
    /// filed under, so a write taken late in the evening is not dated the next
    /// UTC day.
    async fn instance_today(&self) -> jiff::civil::Date {
        let frame = crate::orientation::instance_zone::unzoned_frame(&*self.memory).await;
        self.clock().today_in(&frame)
    }
}
