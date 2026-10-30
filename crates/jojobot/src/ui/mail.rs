//! **The operator's own mail, read and finished from the browser.**
//!
//! A bot leaves the operator a message, and no bot reads it back: every verb
//! that could hand the text to a bot refuses a box a person owns. This is the
//! other surface. The operator's page lists their box and moves nothing. Two
//! POSTs act on it: one opens a single message, which takes delivery of that
//! one, and one marks a message processed with a note.
//!
//! **Who may reach this is the login, and only while the login is one person.**
//! A session carries no subject; who may log in was settled at the callback
//! against the allowlist `/mcp` uses, and a bot's bearer token opens no page.
//! With any other number of people admitted, every login would read the
//! operator's mail, so the page withholds the box and both actions refuse. An
//! open allowlist admits every subject, so it counts as not one. Both actions
//! are POSTs behind the same session, and each also checks where the request
//! came from.
//!
//! **Only the operator's box.** The operator is the person the instance's own
//! record names, and the action refuses a message that is not in the box that
//! person owns: a bot's mail is not finished from here.

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use jojobot_domain::mailbox::{Mailbox, MessageId};
use jojobot_domain::memory::EntityId;
use jojobot_query_api::{MailHandle, OperatorMailCommands, Refused};
use serde::Deserialize;

use crate::AppState;
use crate::ui::refuse;

/// What the page says in place of the box, and what both actions answer with,
/// when the allowlist does not admit exactly one person.
pub(crate) const WITHHELD: &str =
    "This instance admits more than one person, so the operator's private mail is not shown here.";

/// **The state the two actions hold**: the session gate's state, and the one
/// handle through which a page changes mail.
#[derive(Clone)]
pub struct MailState {
    pub app: AppState,
    pub commands: OperatorMailCommands,
}

/// **The box the operator owns, as the verbs resolve it**: one, none, several or
/// unreadable are four answers, and a caller never picks one of several
/// (rule 598). The ownership question is the MCP side's `owned_box`; this adds the
/// box itself, for its counts.
pub(crate) enum OperatorsBox {
    /// The one box the operator owns.
    The(Mailbox),
    /// They own none.
    None,
    /// They own more than one, which is damage and not a choice.
    Several(Vec<String>),
    /// The board could not be read.
    Unreadable,
}

/// The box the operator owns.
pub(crate) async fn operators_box(mail: &MailHandle, operator: &EntityId) -> OperatorsBox {
    let Ok(boxes) = mail.boxes().await else {
        return OperatorsBox::Unreadable;
    };
    match jojobot_mcp::owned_box(&boxes, operator) {
        jojobot_mcp::OwnBox::The(name) => boxes
            .into_iter()
            .find(|held| held.name == name)
            .map_or(OperatorsBox::Unreadable, OperatorsBox::The),
        jojobot_mcp::OwnBox::None => OperatorsBox::None,
        jojobot_mcp::OwnBox::Several(names) => {
            OperatorsBox::Several(names.iter().map(|name| name.as_str().to_string()).collect())
        }
        jojobot_mcp::OwnBox::Unreadable => OperatorsBox::Unreadable,
    }
}

/// What the page's forms send.
#[derive(Debug, Deserialize)]
pub struct Processed {
    id: String,
    #[serde(default)]
    note: Option<String>,
}

/// What the Open button sends.
#[derive(Debug, Deserialize)]
pub struct Opened {
    id: String,
}

/// **What the page itself checks before it asks the command**: the allowlist
/// admits one person and the request came from this UI's own origin. Who the
/// operator is and whether the message is theirs is the command's to judge.
fn gate(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let ui = state.ui.as_ref().expect("the action mounted without a UI");
    if !ui.admits_exactly_one_person() {
        return Some(refuse(StatusCode::FORBIDDEN, WITHHELD));
    }
    if !ui.same_origin(headers) {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            "That request did not come from this page, so nothing was changed.",
        ));
    }
    None
}

/// What a refused command answers with, in the sentences the verbs use.
/// `failed` is what a write that failed says, which names the action.
fn answer(refused: Refused, failed: &str) -> Response {
    match refused {
        Refused::NoOperator => refuse(
            StatusCode::NOT_FOUND,
            "This instance has no operator, so there is no mailbox to finish mail in.",
        ),
        // **The MCP side's own sentence**, because the record names something that
        // is not a person and a bot's mail is not finished from here.
        Refused::NotAPerson(held) => refuse(
            StatusCode::NOT_FOUND,
            &jojobot_mcp::orientation::instance_zone::operator_is_not_a_person(&held),
        ),
        Refused::RecordUnreadable(err) => {
            tracing::warn!(error = %err, "the instance's record could not be read for an action");
            refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read the instance's record, so nothing was changed. Try again.",
            )
        }
        Refused::NoBox => refuse(StatusCode::NOT_FOUND, "The operator has no mailbox yet."),
        Refused::SeveralBoxes(names) => refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            &jojobot_mcp::orientation::instance_zone::operator_owns_several_boxes(&names),
        ),
        Refused::BoardUnreadable(err) => {
            tracing::warn!(error = %err, "the mail board could not be read for an action");
            refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read the mail board, so nothing was changed. Try again.",
            )
        }
        Refused::NotTheirs => refuse(
            StatusCode::NOT_FOUND,
            "That message is not in the operator's mailbox, so nothing was changed.",
        ),
        Refused::Failed(err) => {
            tracing::warn!(error = %err, action = failed, "an operator mail action failed");
            refuse(
                StatusCode::BAD_GATEWAY,
                &format!("jojobot could not {failed}. Nothing was changed. Try again."),
            )
        }
    }
}

/// `POST /ui/mail/open` — take delivery of ONE message in the operator's box
/// (`new` to `read`), leave the rest of the box where it is, and land the
/// operator back on their page.
pub async fn open(
    State(MailState { app, commands }): State<MailState>,
    headers: HeaderMap,
    Form(form): Form<Opened>,
) -> Response {
    if let Some(refusal) = gate(&app, &headers) {
        return refusal;
    }
    match commands.open(&MessageId(form.id)).await {
        Ok(operator) => Redirect::to(&format!("/{}/", operator.as_str())).into_response(),
        Err(refused) => answer(refused, "open that message"),
    }
}

/// `POST /ui/mail/processed` — mark one message in the operator's box
/// processed, with an optional note, and land the operator back on their page.
pub async fn processed(
    State(MailState { app, commands }): State<MailState>,
    headers: HeaderMap,
    Form(form): Form<Processed>,
) -> Response {
    if let Some(refusal) = gate(&app, &headers) {
        return refusal;
    }
    let note = form
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    match commands.mark_processed(&MessageId(form.id), note).await {
        Ok(operator) => Redirect::to(&format!("/{}/", operator.as_str())).into_response(),
        Err(refused) => answer(refused, "mark that message processed"),
    }
}
