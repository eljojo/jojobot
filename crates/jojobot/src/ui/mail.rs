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

/// The record the instance holds about itself, and the key on it that names
/// the operator. The same record and key the boot reads.
const INSTANCE_RECORD: &str = "topic:instance";
const OPERATOR_KEY: &str = "operator";

/// **The person the instance's record names as its operator**, or `None` when
/// the record or the key is absent or the store cannot be read. A store that
/// cannot be read names nobody.
pub(crate) async fn operator_of(state: &AppState) -> Option<EntityId> {
    state
        .memory
        .fields(&EntityId(INSTANCE_RECORD.to_string()))
        .await
        .ok()
        .and_then(|fields| fields.get(OPERATOR_KEY).cloned())
        .map(|raw| raw.trim().to_string())
        .filter(|handle| !handle.is_empty())
        .map(EntityId)
}

/// What the page says in place of the box, and what both actions answer with,
/// when the allowlist does not admit exactly one person.
pub(crate) const WITHHELD: &str =
    "This instance admits more than one person, so the operator's private mail is not shown here.";

/// The box the operator owns, if the board can say and they have one.
pub(crate) async fn operators_box(state: &AppState, operator: &EntityId) -> Option<Mailbox> {
    state
        .mailboxes
        .list_mailboxes()
        .await
        .ok()?
        .into_iter()
        .find(|held| &held.owner == operator)
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
) -> Result<EntityId, Response> {
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
    let Some(operator) = operator_of(state).await else {
        return Err(refuse(
            StatusCode::NOT_FOUND,
            "This instance has no operator, so there is no mailbox to finish mail in.",
        ));
    };
    let Some(own_box) = operators_box(state, &operator).await else {
        return Err(refuse(
            StatusCode::NOT_FOUND,
            "The operator has no mailbox yet.",
        ));
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
    Ok(operator)
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
    let operator = match guard(&state, &headers, &id).await {
        Ok(operator) => operator,
        Err(refusal) => return refusal,
    };
    if let Err(err) = state.mailboxes.read_message(&id).await {
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
    let operator = match guard(&state, &headers, &id).await {
        Ok(operator) => operator,
        Err(refusal) => return refusal,
    };
    let note = form
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if let Err(err) = state.mailboxes.mark_processed(&id, note).await {
        tracing::warn!(error = %err, "marking the operator's message processed failed");
        return refuse(
            StatusCode::BAD_GATEWAY,
            "jojobot could not mark that message processed. Nothing was changed. Try again.",
        );
    }
    Redirect::to(&format!("/{}/", operator.as_str())).into_response()
}
