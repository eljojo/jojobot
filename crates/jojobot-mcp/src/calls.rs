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

/// **The name of the run an answer reports**, when it hands back no handle: a
/// compaction report names the run it is about under `session.run`.
fn run_named_by(result: &Result<CallToolResponse, McpError>) -> Option<String> {
    body_of(result)?
        .get("session")?
        .get("run")?
        .as_str()
        .map(str::to_string)
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

    /// **Count the call that was just answered**, under the caller resolved
    /// before the verb ran: a verb may void the handle (wrapping a run does), and
    /// a lookup after it would count the call that ends a run under nobody. The
    /// day is the run's own: the day it stated, else the server's clock in the
    /// run's zone, and never the machine's. A call with no session is counted
    /// under no bot.
    ///
    /// **A call that carried no handle belongs to the run its answer names**,
    /// when the answer names one: the boot is the first call of its run, and its
    /// caller exists only once the verb ran. The answer names it by the handle it
    /// opens, else by the run a compaction report is about. **An answer that
    /// names no run is counted under the bot the call named**, when the call
    /// landed: the offer to resume names a bot and no run. A refused call counts
    /// under no bot it named, because the name may be one nobody created.
    pub(crate) async fn count_call(
        &self,
        verb: String,
        resolved: Option<Caller>,
        named_bot: Option<EntityId>,
        result: &Result<CallToolResponse, McpError>,
    ) {
        let outcome = outcome_of(result);
        let caller = match resolved {
            Some(caller) => Some(caller),
            None => {
                let opened = sid_named_by(result);
                match self.caller(opened.as_deref()).ok().flatten() {
                    Some(caller) => Some(caller),
                    None => run_named_by(result).and_then(|name| self.caller_of_run(&name)),
                }
            }
        };
        let landed = outcome == CallOutcome::Landed;
        let bot = caller
            .as_ref()
            .map(|caller| caller.bot.clone())
            .or_else(|| named_bot.filter(|_| landed));
        let day = match self.dated_as(caller.as_ref(), None).await {
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
            outcome,
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
