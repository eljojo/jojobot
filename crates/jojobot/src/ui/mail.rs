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
use serde::Deserialize;

use crate::AppState;
use crate::ui::refuse;
use jojobot_mcp::orientation::instance_zone::OperatorIs;

/// What the page says in place of the box, and what both actions answer with,
/// when the allowlist does not admit exactly one person.
pub(crate) const WITHHELD: &str =
    "This instance admits more than one person, so the operator's private mail is not shown here.";

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
pub(crate) async fn operators_box(state: &AppState, operator: &EntityId) -> OperatorsBox {
    let Ok(boxes) = state.mailboxes.list_mailboxes().await else {
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

/// **What the page and both actions say when the operator owns more than one
/// box**: damage, in the MCP side's words, never a box picked from the several.
pub(crate) fn several_boxes(names: &[String]) -> String {
    format!(
        "The operator owns more than one mailbox ({}), and one person has exactly one. This is \
         damage rather than anything you did: report it, it needs a person.",
        names.join(", ")
    )
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

/// **Everything both actions need to be true before they touch a message**:
/// the allowlist admits one person, the request came from this UI's own origin,
/// the instance names an operator who has a box, and the message is in it.
/// Returns the operator, whose page the action lands back on, or the refusal.
async fn guard(
    state: &AppState,
    headers: &HeaderMap,
    id: &MessageId,
) -> Result<(EntityId, Mailbox), Response> {
    let ui = state.ui.as_ref().expect("the action mounted without a UI");
    if !ui.admits_exactly_one_person() {
        return Err(refuse(StatusCode::FORBIDDEN, WITHHELD));
    }
    if !ui.same_origin(headers) {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            "That request did not come from this page, so nothing was changed.",
        ));
    }
    let operator = match jojobot_mcp::orientation::instance_zone::operator_of(&*state.memory).await
    {
        Ok(OperatorIs::Person(operator)) => operator,
        Ok(OperatorIs::Nobody) => {
            return Err(refuse(
                StatusCode::NOT_FOUND,
                "This instance has no operator, so there is no mailbox to finish mail in.",
            ));
        }
        // **The MCP side's own sentence**, because the record names something that
        // is not a person and a bot's mail is not finished from here.
        Ok(OperatorIs::NotAPerson(held)) => {
            return Err(refuse(
                StatusCode::NOT_FOUND,
                &jojobot_mcp::orientation::instance_zone::operator_is_not_a_person(&held),
            ));
        }
        Err(err) => {
            tracing::warn!(error = %err, "the instance's record could not be read for an action");
            return Err(refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read the instance's record, so nothing was changed. Try again.",
            ));
        }
    };
    let own_box = match operators_box(state, &operator).await {
        OperatorsBox::The(held) => held,
        OperatorsBox::None => {
            return Err(refuse(
                StatusCode::NOT_FOUND,
                "The operator has no mailbox yet.",
            ));
        }
        OperatorsBox::Several(names) => {
            return Err(refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                &several_boxes(&names),
            ));
        }
        OperatorsBox::Unreadable => {
            return Err(refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read the mail board, so nothing was changed. Try again.",
            ));
        }
    };
    let in_their_box = match state.mailboxes.message_by_id(id).await {
        Ok(Some(message)) => message.mailbox == own_box.name,
        Ok(None) => false,
        Err(err) => {
            tracing::warn!(error = %err, "the mail board could not be read for an action");
            return Err(refuse(
                StatusCode::BAD_GATEWAY,
                "jojobot could not read the mail board, so nothing was changed. Try again.",
            ));
        }
    };
    if !in_their_box {
        return Err(refuse(
            StatusCode::NOT_FOUND,
            "That message is not in the operator's mailbox, so nothing was changed.",
        ));
    }
    Ok((operator, own_box))
}

/// `POST /ui/mail/open` — take delivery of ONE message in the operator's box
/// (`new` to `read`), leave the rest of the box where it is, and land the
/// operator back on their page.
pub async fn open(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Opened>,
) -> Response {
    let id = MessageId(form.id);
    let (operator, _) = match guard(&state, &headers, &id).await {
        Ok(held) => held,
        Err(refusal) => return refusal,
    };
    if let Err(err) = state
        .mailboxes
        .read_message(
            &id,
            // The operator is a person and has no run to name.
            jojobot_domain::mailbox::Acting::unnamed(state.clock.now()),
        )
        .await
    {
        tracing::warn!(error = %err, "opening the operator's message failed");
        return refuse(
            StatusCode::BAD_GATEWAY,
            "jojobot could not open that message. Nothing was changed. Try again.",
        );
    }
    Redirect::to(&format!("/{}/", operator.as_str())).into_response()
}

/// `POST /ui/mail/processed` — mark one message in the operator's box
/// processed, with an optional note, and land the operator back on their page.
pub async fn processed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Processed>,
) -> Response {
    let id = MessageId(form.id);
    let (operator, own_box) = match guard(&state, &headers, &id).await {
        Ok(held) => held,
        Err(refusal) => return refusal,
    };
    let note = form
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if let Err(err) = state
        .mailboxes
        .mark_processed(
            &id,
            &own_box.name,
            note,
            jojobot_domain::mailbox::Acting::unnamed(state.clock.now()),
        )
        .await
    {
        tracing::warn!(error = %err, "marking the operator's message processed failed");
        return refuse(
            StatusCode::BAD_GATEWAY,
            "jojobot could not mark that message processed. Nothing was changed. Try again.",
        );
    }
    Redirect::to(&format!("/{}/", operator.as_str())).into_response()
}
