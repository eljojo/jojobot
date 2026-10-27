//! **The advice of a call that stopped after its write, decided once.** A verb
//! whose write landed and whose secondary step then failed (the box that follows a
//! rename, the note that follows a charter, the mark that follows an archive) must
//! tell the caller what to do next. That depends on WHY the step stopped, and the
//! error's own word says it: `retry` is a store failure, which a resend under the
//! same `request_id` outlasts; `change` is a refusal, which the same call meets
//! again at the same step until what it names changes; `person` is damage no
//! resend repairs. A verb that keeps only a boolean, or hard-codes "send the same
//! call again", tells a caller to resend into a refusal for ever (rules 130, 466,
//! 498, 598). Every verb keeps the error and asks [`Stopped`].

use crate::answer::{FixBy, WayForward};
use jojobot_domain::mailbox::MailboxError;
use jojobot_domain::memory::MemoryError;
use jojobot_domain::session::SessionError;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;

/// **Why a step stopped**, which decides who fixes it.
#[derive(Clone, Debug)]
pub(crate) enum Stopped {
    /// The step was refused, with the refusal's own words. Sending the call
    /// again meets the same refusal, so the call, or something it names, has to
    /// change.
    Refused(String),
    /// The store failed under the step. The same step can succeed when sent again.
    Store,
    /// The step ended in an error that is no store failure and no refusal, so a
    /// person reads the server log.
    Unexplained,
}

impl Stopped {
    /// **The cause an error's word names.** `words` are the error's own, kept
    /// for a refusal.
    pub(crate) fn of_word(word: Option<FixBy>, words: impl Into<String>) -> Stopped {
        match word {
            Some(FixBy::Retry) => Stopped::Store,
            Some(FixBy::Change) => Stopped::Refused(words.into()),
            Some(FixBy::Person) | None => Stopped::Unexplained,
        }
    }

    /// The cause of a protocol error, from the word it wears in its `data`.
    pub(crate) fn of_error(error: &McpError) -> Stopped {
        Stopped::of_word(crate::answer::fix_by_of(error), error.message.as_ref())
    }

    /// The cause of a memory error.
    pub(crate) fn of_memory(error: &MemoryError) -> Stopped {
        Stopped::of_word(
            crate::memory::declined::memory_fix_by(error),
            error.to_string(),
        )
    }

    /// The cause of a mailbox error.
    pub(crate) fn of_mailbox(error: &MailboxError) -> Stopped {
        Stopped::of_word(
            crate::mailboxes::declined::mailbox_fix_by(error),
            error.to_string(),
        )
    }

    /// The cause of a session error.
    pub(crate) fn of_session(error: &SessionError) -> Stopped {
        Stopped::of_word(
            crate::session::declined::session_fix_by(error),
            error.to_string(),
        )
    }

    /// **The cause of a refusal that came back as an answer**, from the word and
    /// the way forward its body carries. An answer that is no refusal has no
    /// cause; the caller asks only of an answer that stopped its step.
    pub(crate) fn of_answer(answer: &CallToolResult) -> Stopped {
        let body: serde_json::Value = answer
            .content
            .first()
            .and_then(|block| block.as_text())
            .and_then(|text| serde_json::from_str(&text.text).ok())
            .unwrap_or_default();
        let word = body[FixBy::KEY].as_str().and_then(FixBy::of_token);
        let words = body["how_to_proceed"]
            .as_str()
            .unwrap_or("the step was refused");
        match (word, body["status"] == "blocked") {
            // A blocked answer with no word is a refusal of the call.
            (None, true) => Stopped::Refused(words.to_string()),
            (word, _) => Stopped::of_word(word, words),
        }
    }

    /// **The way forward of a call under a `request_id` whose write landed and
    /// whose next step stopped.** `landed` says what is written and what is not;
    /// `resend` is the sentence that tells a caller a resend finishes the step.
    /// A store failure is told as `retry` with `resend`. A refusal is told as
    /// `change`, with the refusal's words, because the same call under the same id
    /// is refused the same way. Anything else is a person's.
    pub(crate) fn unfinished(&self, landed: &str, resend: &str) -> WayForward {
        match self {
            Stopped::Store => WayForward::retry(format!("{landed} {resend}")),
            Stopped::Refused(why) => format!(
                "{landed} The step that follows was refused: {why}. The same call under the \
                 same request_id is refused the same way until what the refusal names is \
                 changed. Change that, then send the same call again with the same request_id."
            )
            .into(),
            Stopped::Unexplained => WayForward::person(format!(
                "{landed} The step that follows ended in an error that is not a refusal. The \
                 server log holds the error. Do not send the same call again until a person \
                 has read it."
            )),
        }
    }
}
