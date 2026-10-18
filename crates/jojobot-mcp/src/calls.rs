//! **Every call that reaches a verb is counted once, after its answer is
//! known**, under the bot behind its session, the day the run was in and the
//! verb's name. A refusal is counted with the word that says who fixes it.
//!
//! What is kept is a number per key and nothing the caller sent: no argument, no
//! text, no handle. A store that cannot take the count never fails the call; the
//! failure is logged and the call goes on.

use jojobot_domain::usage::{CallKey, CallOutcome};
use rmcp::model::CallToolResponse;

use super::*;

/// The name a call is counted under when the router serves no verb of the name
/// the caller sent. A name a caller chose is text, and a count keyed by it would
/// grow with whatever callers invent.
pub(crate) const UNSERVED_VERB: &str = "unserved";

/// The key under which the word that says who fixes a refusal rides, in a blocked
/// answer and in a protocol error's data.
const FIX_BY: &str = "fix_by";

/// The body of an answer exactly as the caller was handed it, when it has one.
fn body_of(result: &Result<CallToolResponse, McpError>) -> Option<serde_json::Value> {
    let Ok(CallToolResponse::Complete(answer)) = result else {
        return None;
    };
    // The body rides in `structuredContent` once it is shaped for the wire, and
    // as the first text block before that.
    answer.structured_content.clone().or_else(|| {
        answer
            .content
            .iter()
            .filter_map(|block| block.as_text())
            .find_map(|text| serde_json::from_str::<serde_json::Value>(&text.text).ok())
    })
}

/// How a call ended, read from the answer exactly as the caller was handed it.
fn outcome_of(result: &Result<CallToolResponse, McpError>) -> CallOutcome {
    let word = |body: &serde_json::Value| {
        body.get(FIX_BY)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    match result {
        Err(error) => CallOutcome::Refused {
            fix_by: error.data.as_ref().and_then(word),
        },
        Ok(CallToolResponse::Complete(_)) => match body_of(result) {
            Some(body)
                if body.get("status").and_then(serde_json::Value::as_str) == Some("blocked") =>
            {
                CallOutcome::Refused {
                    fix_by: word(&body),
                }
            }
            _ => CallOutcome::Landed,
        },
        // The protocol asking the client for something is no verb's answer.
        Ok(_) => CallOutcome::Landed,
    }
}

/// **The handle of the run an answer opens**, for a call whose request carried
/// none. A boot hands the handle back under `session`, and no other answer does.
fn sid_named_by(result: &Result<CallToolResponse, McpError>) -> Option<String> {
    body_of(result)?
        .get("session")?
        .get("sid")?
        .as_str()
        .map(str::to_string)
}

impl Jojobot {
    /// The verb name a call is counted under: the name when the router serves
    /// it, and [`UNSERVED_VERB`] when it does not.
    pub(crate) fn counted_verb(&self, name: &str) -> String {
        if self.tool_router.has_route(name) {
            name.to_string()
        } else {
            UNSERVED_VERB.to_string()
        }
    }

    /// **Count the call that was just answered.** The day is the run's own: the
    /// day it stated, else the server's clock in the run's zone, and never the
    /// machine's. A call with no session is counted under no bot.
    pub(crate) async fn count_call(
        &self,
        verb: String,
        sid: Option<&str>,
        result: &Result<CallToolResponse, McpError>,
    ) {
        // **A call that carried no handle belongs to the run its answer opens**,
        // when the answer opens one: the boot is the first call of its run.
        let opened = sid.is_none().then(|| sid_named_by(result)).flatten();
        let sid = sid.or(opened.as_deref());
        let bot = self.caller(sid).ok().flatten().map(|caller| caller.bot);
        let day = match self.dated(None, sid).await {
            Ok(day) => day,
            Err(error) => {
                tracing::warn!(%verb, %error, "a call was not counted: its day could not be read");
                return;
            }
        };
        let key = CallKey {
            bot,
            day,
            verb,
            outcome: outcome_of(result),
        };
        if let Err(error) = self.sessions.count_call(&key).await {
            tracing::warn!(verb = %key.verb, %error, "a call was not counted: the store refused the count");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolResult, ContentBlock};

    fn answered(body: serde_json::Value) -> Result<CallToolResponse, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(body.to_string())]).into())
    }

    /// **Only a boot names a run, and only when it opens one.** The shapes are the
    /// boot's own: a named boot hands its handle back under `session`, an
    /// anonymous boot has `session` null, and a read has no `session` at all.
    #[test]
    fn only_an_answer_that_opens_a_run_names_one() {
        let opened = answered(serde_json::json!({
            "session": {"available": true, "sid": "sk-lisa", "run": "ab12cd"},
        }));
        assert_eq!(sid_named_by(&opened).as_deref(), Some("sk-lisa"));
        let anonymous = answered(serde_json::json!({"session": null}));
        assert_eq!(sid_named_by(&anonymous), None);
        let offered = answered(serde_json::json!({
            "session": {"available": true, "choices": ["ab12cd", "new"]},
        }));
        assert_eq!(sid_named_by(&offered), None);
        let read = answered(serde_json::json!({"objects": []}));
        assert_eq!(sid_named_by(&read), None);
    }
}
