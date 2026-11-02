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

use jojobot_domain::actor::{Actor, Attribution};
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
        Actor::App(self.app.id.clone())
    }

    fn attribution<'a>(&'a self, actor: &'a Actor) -> Attribution<'a> {
        Attribution {
            actor,
            app_stamp: Some(&self.stamp),
        }
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
    Answer::Refused(for_an_app(
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    ))
}

/// **A refusal that names other things is rewritten for an app.** A near-name, a
/// taken-handle or an unknown-handle refusal lists candidate handles of any kind
/// and offers an `override_token` that `permit` refuses an app, so it names things
/// the grant never judged and a route the app may not take (rule 731). It is read
/// by its refusal code, never by its prose; every other refusal passes whole.
fn for_an_app(mut body: Value) -> Value {
    if body["status"] != "blocked" {
        return body;
    }
    let advice = match body["refusal_code"].as_str() {
        Some("NearName") => {
            "Nothing was written. The name is too close to something already recorded. Send a \
             distinct name or handle, or ask the operator."
        }
        Some("HandleTaken") => {
            "Nothing was written. The handle is already in use. Send a different handle."
        }
        Some("UnknownEntity") => {
            "Nothing was written. The handle names nothing this app may reach. Check the \
             handle, or ask the operator."
        }
        _ => return body,
    };
    if let Some(refusal) = body.as_object_mut() {
        refusal.remove("candidates");
        refusal.insert("how_to_proceed".into(), advice.into());
    }
    body
}

fn unavailable(error: impl std::fmt::Display) -> Answer {
    Answer::Unavailable(error.to_string())
}

/// **What a verb's protocol error means to an app.** A verb that parses its input
/// answers a caller's mistake as invalid params, and that is the caller's to fix:
/// a 422 that says what was wrong. Any other error is the store's, and the same
/// call can be sent again.
fn mistaken(error: rmcp::model::ErrorData) -> Answer {
    if error.code == rmcp::model::ErrorCode::INVALID_PARAMS {
        Answer::Invalid(error.message.to_string())
    } else {
        unavailable(error)
    }
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
    /// The operations extracted so far: the four writes and the one read.
    pub fn shipped() -> Self {
        Registry::default()
            .with(Entry::new("recall", recall))
            .with(Entry::new("capture", capture))
            .with(Entry::new("add_entity", add_entity))
            .with(Entry::new("post_message", post_message))
            .with(Entry::new("mailbox_counts", mailbox_counts))
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

/// **What `recall` takes from an app**: a subject, a kind, a `where`, key filters,
/// the keys to keep, a page size, `backing` (which claim holds each value, and
/// when it was written) and the view `ready` on work. Another view, a walk, a
/// window of days, a charter or a run is not one of them, and a call that sends
/// one is refused naming it.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RecallInput {
    #[serde(default, rename = "subject")]
    _subject: Option<String>,
    #[serde(default, rename = "kind")]
    _kind: Option<String>,
    #[serde(default, rename = "where")]
    _expression: Option<String>,
    #[serde(default, rename = "fields")]
    _fields: Option<Value>,
    #[serde(default, rename = "keys")]
    _keys: Option<Vec<String>>,
    #[serde(default, rename = "limit")]
    _limit: Option<u32>,
    #[serde(default, rename = "view")]
    _view: Option<String>,
    #[serde(default, rename = "backing")]
    _backing: Option<bool>,
}

/// The most objects one app read returns when the call names no limit, and the most it may name.
const RECALL_DEFAULT_LIMIT: usize = 50;
const RECALL_MOST: usize = 200;

/// **`recall` for an app**: the verb's own question, prepared with no asker, so the
/// app reads what nobody owns and never a run, a usage record or a bot's own
/// thing. The grant is yes or no.
async fn recall(call: Call) -> Answer {
    let operation = Operation {
        name: "recall".into(),
        args_sent: call.args_sent(),
        ..Operation::default()
    };
    if let Err(answer) = call.permit(&operation) {
        return answer;
    }
    if let Err(answer) = call.input::<RecallInput>() {
        return answer;
    }
    let args: crate::memory::recall::RecallArgs = match call.input() {
        Ok(args) => args,
        Err(answer) => return answer,
    };
    // **The one view an app may ask is `ready`**, on work: the shelf a program
    // that nudges a stopped line reads. Any other is refused naming it.
    if let Some(view) = &args.view
        && view.trim().trim_start_matches("view:") != "ready"
    {
        return Answer::Invalid(format!(
            "an app may ask the view `ready` only, not `{}`",
            view.trim()
        ));
    }
    let limit = args
        .limit
        .map_or(RECALL_DEFAULT_LIMIT, |limit| limit as usize)
        .clamp(1, RECALL_MOST);
    let handler = &call.handler;
    let (question, only_keys) = match handler.app_recall_question(args).await {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(refused)) => return refusal(&refused),
        Err(e) => return mistaken(e),
    };
    let queries = crate::memory::view_run::handle(
        handler.memory.clone(),
        handler.sessions.clone(),
        handler.search.clone(),
    );
    let answered = match queries.ask(&call.actor(), question).await {
        Ok(answered) => answered,
        Err(
            jojobot_query_api::Failed::Selection(why) | jojobot_query_api::Failed::Readiness(why),
        ) => {
            return match why {
                jojobot_domain::memory::MemoryError::InvalidQuery(words) => Answer::Invalid(words),
                // **A handle the app cannot read is as good as a handle nothing wears**,
                // and is refused so, naming no near handle: a nearest list would name
                // things the app may not read.
                jojobot_domain::memory::MemoryError::UnknownEntity { attempted, .. } => {
                    Answer::Refused(serde_json::json!({ "unknown_entity": attempted }))
                }
                other => unavailable(other),
            };
        }
        // **A part of the answer that could not be prepared is the store's**, and
        // the same call can be sent again.
        Err(jojobot_query_api::Failed::Preparation(part, why)) => {
            return unavailable(format!("{part}: {why}"));
        }
    };
    match jojobot_query_api::RecalledView::of(answered, only_keys.as_deref(), limit) {
        Ok(view) => Answer::Done(served_with_verdicts(&view)),
        Err(e) => unavailable(e),
    }
}

/// **The view as served, with each card's verdict when the call asked what is
/// ready.** The same functions the verb uses write the verdict and the left-out
/// count (rule 498), so a named card that is blocked says `ready: false` and
/// what stops it, and the list says how many cards it left out.
fn served_with_verdicts(view: &jojobot_query_api::RecalledView) -> serde_json::Value {
    let mut body = view.to_json();
    for (served, object) in body["objects"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .zip(&view.objects)
    {
        if let Some(verdict) = &object.verdict {
            crate::memory::ready::render(served, verdict);
        }
    }
    if let Some(marker) = crate::memory::left_out::ready_left_out(view.ready_left_out, None) {
        body["ready_left_out"] = marker;
    }
    // **A thing the read could not read is named**, by the function the bots' recall uses.
    if !view.unreadable.is_empty() {
        body["unreadable"] = crate::memory::recall::unreadable_things_json(&view.unreadable);
    }
    crate::memory::ready::fit_under_ceiling(&mut body);
    body
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
    let actor = call.actor();
    match handler
        .capture_write(
            call.attribution(&actor),
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
        Err(e) => mistaken(e),
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
    let actor = call.actor();
    match call
        .handler
        .add_entity_write(call.attribution(&actor), args, day, None, None)
        .await
    {
        Ok(Ok(added)) => done(&added.view()),
        Ok(Err(refused)) => refusal(&refused),
        Err(e) => mistaken(e),
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
        Err(e) => return mistaken(e),
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
        Err(e) => return mistaken(e),
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
        Err(e) => mistaken(e),
    }
}

/// **One bot's box as `mailbox_counts` serves it**: whose box, how many messages
/// wait in `new`, and when the oldest of them was sent. No id, subject, sender or
/// body, so counts and an age are the whole exposure (rules 91, 139).
#[derive(Debug, Serialize)]
pub struct BoxWaiting {
    /// The bot the box belongs to.
    pub bot: String,
    /// How many messages wait in `new`.
    pub new: usize,
    /// How many cards of the box could not be read as messages, the number
    /// `read_mailbox` with `counts_only` reports beside the counts.
    pub quarantined: usize,
    /// When the oldest of them was sent, or null for an empty `new`.
    pub oldest_new_sent_at: Option<String>,
}

/// **What `mailbox_counts` served**: the rows under a named key, so the answer can
/// gain a key without breaking a caller.
#[derive(Debug, Serialize)]
pub struct BoxesWaiting {
    /// One row for each bot's box, by bot.
    pub boxes: Vec<BoxWaiting>,
}

/// **`mailbox_counts`: what is waiting in each bot's box, read and nothing moved.**
/// The numbers are read out of `wire::mailbox_json`, the rendering `read_mailbox`
/// with `counts_only` answers with, so the two cannot disagree about a box. The
/// age is read off the messages without a delivery, which is a scan of every
/// message that `counts_only` does not make. A person's box is not listed. It
/// reads current state and is not a feed (rule 724).
async fn mailbox_counts(call: Call) -> Answer {
    let operation = Operation {
        name: "mailbox_counts".into(),
        args_sent: call.args_sent(),
        ..Operation::default()
    };
    if let Err(answer) = call.permit(&operation) {
        return answer;
    }
    let boxes = match call.handler.mailboxes.list_mailboxes().await {
        Ok(boxes) => boxes,
        Err(e) => return unavailable(e),
    };
    let messages = match call.handler.mailboxes.scan_messages().await {
        Ok(messages) => messages,
        Err(e) => return unavailable(e),
    };
    let mut rows: Vec<BoxWaiting> = boxes
        .iter()
        .filter(|held| !held.is_private())
        .map(|held| {
            let counted = crate::mailboxes::wire::mailbox_json(held);
            let number = |at: &serde_json::Value| at.as_u64().unwrap_or_default() as usize;
            BoxWaiting {
                bot: held.owner.as_str().to_string(),
                new: number(&counted["counts"]["new"]),
                quarantined: number(&counted["quarantined"]["count"]),
                oldest_new_sent_at: messages
                    .iter()
                    .filter(|m| {
                        m.mailbox == held.name
                            && m.state == jojobot_domain::mailbox::MessageState::New
                            && m.superseded_by.is_none()
                    })
                    .map(|m| m.sent_at)
                    .min()
                    .map(|at| at.to_string()),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.bot.cmp(&b.bot));
    done(&BoxesWaiting { boxes: rows })
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

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorData;

    /// **A verb's protocol error is the caller's only when it says invalid params.**
    /// Both halves in one read: invalid params is a 422 with the message, and any
    /// other error is the store's and can be sent again.
    #[test]
    fn a_protocol_error_is_the_callers_only_when_it_is_invalid_params() {
        match mistaken(ErrorData::invalid_params("date must be YYYY-MM-DD", None)) {
            Answer::Invalid(why) => assert!(why.contains("YYYY-MM-DD"), "{why}"),
            other => panic!("a caller's mistake was answered {other:?}"),
        }
        match mistaken(ErrorData::internal_error("the store is down", None)) {
            Answer::Unavailable(_) => {}
            other => panic!("a store failure was answered {other:?}"),
        }
    }
}
